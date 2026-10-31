//! The checkout state machine (group 1.4, `ref/domain-api.md` §6).
//!
//! Blueprint §8's diagram as a Rust enum. A sale is in exactly one state and
//! each state is its own type, so a transition from the wrong state does not
//! compile. Every transition is a free function that takes a state by value
//! and returns the next one or a [`CartError`]; nothing mutates in place.
//!
//! As delivered at 1.4.1, the machine holds the states whose data exists: a
//! cart being built and a parked one, which is its own type since 1.4.4 so
//! that only a parked cart can be resumed. Each later state arrives with the
//! transition that reaches it (tendering, finalizing, complete and voided at
//! 1.4.8), and each later field of a cart or a line with the step that gives
//! it meaning: a line's manual discount at 1.4.5, a basket's at 1.4.6, a price
//! override and the ministry ceiling at 1.4.7, and a promotion group when
//! promotions are first captured. A field added before its rule would be a
//! value nothing checks.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use serde_json::json;

use crate::audit::AuditIntent;
use crate::catalog::{
    DerivedWeight, LabelReprint, PriceOrigin, PriceSource, Product, ProductHit, ReprintCause,
    ScanLookup, UnitOfMeasure,
};
use crate::ids::{
    CustomerId, ProductId, RegisterId, SaleId, SaleLineId, ShiftId, TaxCategoryId, UserId,
};
use crate::money::{Currency, Money, MoneyError, Qty, RoundingRule};
use crate::permissions::{
    ApprovalBinding, ApprovalHandle, Authorized, Capability, PermissionError, cap,
};
use crate::pricing::{
    BasketDiscount, DiscountAttribution, DiscountCap, DiscountKind, DiscountRequest,
    DiscountSource, LineDiscount, OverrideReason, PriceOverride, PricingError, ProrationLine,
    exceeds_basket, exceeds_value, prorate,
};
use crate::tax::{
    LineTax, PriceMode, StoreTaxProfile, SupplyDestination, SupplyTaxContext, TaxComponent,
    TaxError, TaxSummaryRow, compute_line_tax, summarize_tax,
};
use crate::time::Timestamp;

/// A sale, in the one state it is in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Sale {
    /// Lines are being added, changed or removed.
    Building(Cart),
    /// Set aside by the cashier to serve someone else, and resumed later
    /// (1.4.4).
    Parked(ParkedCart),
}

/// What a cart is opened with: the register, the shift, the cashier and the
/// moment, which the shell supplies because this crate reads no clock (I-8).
///
/// `shift_id` is not optional. A cart's business date is its shift's, and a
/// sale outside a shift has no business date at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartContext {
    pub register_id: RegisterId,
    pub shift_id: ShiftId,
    pub cashier_id: UserId,
    pub currency: Currency,
    /// Training carts are carried from the first moment, because every report
    /// and the fiscal queue must be able to leave them out.
    pub is_training: bool,
    pub opened_at: Timestamp,
}

/// A sale being built: its lines and who and what it is for.
///
/// `Debug` is written by hand so that `buyer_name` never reaches a log line,
/// a crash report or an error's detail (`.claude/rules/security.md`). The TIN
/// stays visible, as on the receipt's buyer block, because a fiscal invoice
/// that names the wrong buyer is diagnosed by it.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Cart {
    pub id: SaleId,
    pub register_id: RegisterId,
    pub shift_id: ShiftId,
    pub cashier_id: UserId,
    pub currency: Currency,
    pub lines: Vec<CartLine>,
    /// The basket's manual discount, as it was asked for (1.4.6). At most one:
    /// another replaces it, a zero clears it, and an amount off is cleared by
    /// any change that lowers the basket's value.
    pub basket_discount: Option<BasketDiscount>,
    pub customer_id: Option<CustomerId>,
    /// A business buyer's tax number, for a B2B fiscal invoice (master plan
    /// B.2).
    pub buyer_tin: Option<String>,
    pub buyer_name: Option<String>,
    /// Why this supply is taxed the way it is (§5). Every cart opens
    /// `Domestic`, and 1.3.3 refuses any other destination until the
    /// accountant names the zero-rated lines (`ref/merchant-decisions.md`
    /// 6.14).
    pub supply: SupplyTaxContext,
    /// Checked by every report and, later, by the fiscal queue.
    pub is_training: bool,
    pub opened_at: Timestamp,
}

impl fmt::Debug for Cart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cart")
            .field("id", &self.id)
            .field("register_id", &self.register_id)
            .field("shift_id", &self.shift_id)
            .field("cashier_id", &self.cashier_id)
            .field("currency", &self.currency)
            .field("lines", &self.lines)
            .field("basket_discount", &self.basket_discount)
            .field("customer_id", &self.customer_id)
            .field("buyer_tin", &self.buyer_tin)
            .field(
                "buyer_name",
                &self.buyer_name.as_ref().map(|_| "[redacted]"),
            )
            .field("supply", &self.supply)
            .field("is_training", &self.is_training)
            .field("opened_at", &self.opened_at)
            .finish()
    }
}

/// One line of a cart, with everything the sale must remember about it.
///
/// The name and the price are copied when the line is captured and never read
/// from the catalogue again (I-5), and `price_origin` says where that price
/// came from. A department sale has no product (§6.5, 1.4.12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartLine {
    pub id: SaleLineId,
    pub product_id: Option<ProductId>,
    pub name_snapshot: String,
    pub unit_price: Money,
    pub price_origin: PriceOrigin,
    /// The ministry ceiling on the line's unit price, captured with it (J.3,
    /// E.71, 1.4.7). `None` where no ceiling applies, and on a weighed
    /// product's price label: that package is of a weight the till does not
    /// know, so its price per unit is the scale's, which the catalogue the
    /// scale was loaded from holds under the ceiling.
    pub max_price: Option<Money>,
    pub qty: Qty,
    pub unit: UnitOfMeasure,
    pub is_weighed: bool,
    pub tax_category_id: TaxCategoryId,
    /// The line's manual discount, as it was asked for (1.4.5). At most one:
    /// another replaces it, and a discount of zero clears it.
    pub discount: Option<LineDiscount>,
    /// The line's price, overridden (1.4.7). `unit_price` keeps the price the
    /// line was captured at; [`CartLine::price`] is what it charges.
    pub price_override: Option<PriceOverride>,
    /// Captured with the line, so a catalogue edit while the cart is open
    /// cannot clear the age gate (1.4.3, 1.4.3b).
    pub min_age: Option<u8>,
    pub age_confirmed: bool,
    /// The weight a scale's label claimed: the stock basis, never money
    /// (§4.3).
    pub label: Option<DerivedWeight>,
}

impl CartLine {
    /// What the line charges a unit: its override's price, or the price it
    /// was captured at. Every value a discount is measured against is this
    /// price times the quantity.
    #[must_use]
    pub fn price(&self) -> Money {
        self.price_override
            .as_ref()
            .map_or(self.unit_price, |o| o.to)
    }
}

/// A new, empty cart for sale `id`, being built.
///
/// It sells to no named customer or business buyer, and its supply is an
/// ordinary domestic one.
#[must_use]
pub fn open(id: SaleId, ctx: &CartContext) -> Cart {
    Cart {
        id,
        register_id: ctx.register_id,
        shift_id: ctx.shift_id,
        cashier_id: ctx.cashier_id,
        currency: ctx.currency,
        lines: Vec::new(),
        basket_discount: None,
        customer_id: None,
        buyer_tin: None,
        buyer_name: None,
        supply: SupplyTaxContext {
            destination: SupplyDestination::Domestic,
            reason: None,
            evidence_ref: None,
        },
        is_training: ctx.is_training,
        opened_at: ctx.opened_at,
    }
}

/// The only way to describe a new line, and it carries no number the caller
/// chose (`ref/domain-api.md` §6.1).
///
/// `price` is a [`PriceSource`]: a catalogue price from a [`Product`], a
/// label's price from a resolved scan, or a department amount, which
/// [`add_line`] refuses. There is no `Deserialize`, because `PriceSource` has
/// none: a command arriving from the screen cannot carry a price into a line.
///
/// It has no `age_confirmed`: an age is confirmed only by an operator,
/// through 1.4.3b, never by the request that adds the line. Its `max_price`
/// arrived at 1.4.7, from the product its constructors read, so the line is
/// held to the ministry ceiling when it enters and when its price changes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AddLine {
    pub line_id: SaleLineId,
    pub product_id: Option<ProductId>,
    pub name_snapshot: String,
    pub price: PriceSource,
    /// The ceiling on the line's unit price, [`CartLine::max_price`].
    pub max_price: Option<Money>,
    pub qty: Qty,
    pub unit: UnitOfMeasure,
    pub is_weighed: bool,
    pub tax_category_id: TaxCategoryId,
    pub min_age: Option<u8>,
    pub label: Option<DerivedWeight>,
}

impl AddLine {
    /// `qty` of `product` at its catalogue price, named as the catalogue names
    /// it in Arabic.
    ///
    /// # Errors
    ///
    /// [`CartError::ProductInactive`] for a product that is not for sale
    /// (E.38). Only adding is refused: a line already captured never reads the
    /// catalogue again, so a sale of the product, and a refund of that sale,
    /// are untouched by its deactivation.
    pub fn from_product(
        line_id: SaleLineId,
        product: &Product,
        qty: Qty,
    ) -> Result<AddLine, CartError> {
        if !product.is_active {
            return Err(CartError::ProductInactive(product.id));
        }
        Ok(AddLine {
            line_id,
            product_id: Some(product.id),
            name_snapshot: product.name_ar.clone(),
            price: PriceSource::from_catalog(product),
            max_price: product.max_price,
            qty,
            unit: product.unit,
            is_weighed: product.is_weighed,
            tax_category_id: product.tax_category_id,
            min_age: product.min_age,
            label: None,
        })
    }

    /// The line a scan resolved to, and the one place a barcode's pack
    /// quantity and a label's price or weight are applied.
    ///
    /// * A barcode or code: the code's pack quantity of the product, at its
    ///   catalogue price, so a six-pack's outer code adds six (E.78).
    /// * A weight label: the label's weight, at the catalogue price.
    /// * A price label: **one package at the label's price**. The label is
    ///   what the customer is charged (E.80), and the weight it implies is
    ///   kept on the line as the stock basis, never as its quantity, so
    ///   `unit_price × qty` is the line's amount (`ref/domain-api.md` §4.3).
    ///   A counted product's label is one unit, held to the product's
    ///   ceiling; a weighed product's is a package of a weight the till does
    ///   not know, which no ceiling per kilogram can be compared with.
    ///
    /// # Errors
    ///
    /// [`CartError::ProductInactive`] for a product not for sale, and
    /// [`CartError::ScanNotOneProduct`] for a scan that found several products,
    /// none, or a damaged label: the till chooses, or sells it through a
    /// department (1.4.12).
    pub fn from_scan(line_id: SaleLineId, hit: ScanLookup) -> Result<AddLine, CartError> {
        match hit {
            ScanLookup::Product { hit, pack_qty } => {
                let ProductHit { product, .. } = hit;
                AddLine::from_product(line_id, &product, pack_qty)
            }
            ScanLookup::WeightEmbedded { hit, qty } => {
                let ProductHit { product, .. } = hit;
                AddLine::from_product(line_id, &product, qty)
            }
            ScanLookup::PriceEmbedded {
                hit,
                price,
                derived,
                ..
            } => {
                let ProductHit { product, .. } = hit;
                let mut line = AddLine::from_product(line_id, &product, Qty::ONE)?;
                line.price = price;
                if product.is_weighed {
                    line.max_price = None;
                }
                line.unit = UnitOfMeasure::Package;
                line.is_weighed = false;
                line.label = derived;
                Ok(line)
            }
            ScanLookup::Ambiguous { .. }
            | ScanLookup::ChecksumFailed { .. }
            | ScanLookup::Unknown { .. } => Err(CartError::ScanNotOneProduct),
        }
    }
}

/// The configured reason a line was voided. A code from the store's list, never
/// free text, because free text is unreportable (`ref/ui-spec.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoidReason(pub String);

/// `cart` with one more line, as `req` describes it.
///
/// The name, the price and where it came from are copied onto the line (I-5).
/// An age-restricted line goes in **blocked**: it stays in the cart, so the
/// cashier sees the exact pack and quantity while checking the customer, and
/// [`check_age_confirmed`] refuses to tender until an operator confirms it
/// (1.4.3b).
///
/// # Errors
///
/// - [`CartError::DepartmentPriceOutsideDepartmentSale`]: a department price,
///   the one price a caller types. Only the capped, audited department sale
///   takes one (1.4.12).
/// - [`CartError::NonPositiveQty`]: a quantity of zero or less.
/// - [`CartError::FractionalQtyForDiscrete`]: a fraction of a unit that is
///   counted, not measured.
/// - [`CartError::Money`]: a price in another currency than the cart's, or a
///   ceiling that cannot be compared with it.
/// - [`CartError::AboveMaxPrice`]: a price above the ministry ceiling (E.71),
///   which nothing admits. A catalogue save refuses the same price, so this
///   is the sale's half of the block; a label is where it can arise.
pub fn add_line(cart: Cart, req: AddLine) -> Result<Cart, CartError> {
    let product = match (req.price.origin(), req.product_id) {
        (PriceOrigin::Department { .. }, _) | (_, None) => {
            return Err(CartError::DepartmentPriceOutsideDepartmentSale);
        }
        (_, Some(product)) => product,
    };
    check_qty(product, req.unit, req.qty)?;
    let unit_price = req.price.amount();
    if unit_price.currency() != cart.currency {
        return Err(MoneyError::CurrencyMismatch(
            cart.currency.code(),
            unit_price.currency().code(),
        )
        .into());
    }
    if let Some(ceiling) = req.max_price
        && unit_price.checked_cmp(ceiling)? == Ordering::Greater
    {
        return Err(CartError::AboveMaxPrice(
            unit_price.format_exact(),
            ceiling.format_exact(),
        ));
    }
    let mut cart = cart;
    cart.lines.push(CartLine {
        id: req.line_id,
        product_id: Some(product),
        name_snapshot: req.name_snapshot,
        unit_price,
        price_origin: req.price.origin().clone(),
        max_price: req.max_price,
        qty: req.qty,
        unit: req.unit,
        is_weighed: req.is_weighed,
        tax_category_id: req.tax_category_id,
        discount: None,
        price_override: None,
        min_age: req.min_age,
        age_confirmed: false,
        label: req.label,
    });
    Ok(cart)
}

/// `cart` with line `line`'s quantity set to `qty`.
///
/// A quantity of zero is not a way to remove a line: removal is
/// [`void_line`], which is audited.
///
/// # Errors
///
/// - [`CartError::LineNotFound`].
/// - [`CartError::NonPositiveQty`] and [`CartError::FractionalQtyForDiscrete`],
///   as for [`add_line`].
/// - [`CartError::LabelLineIsOnePackage`]: a price label's line is the one
///   package that label was printed for; another package is another scan.
/// - [`CartError::DepartmentPriceOutsideDepartmentSale`]: a department line,
///   whose quantity times its typed amount is still a typed amount (1.4.12).
/// - [`CartError::AmountDiscountFixesQuantity`]: a line with an amount off.
///   Fewer units would make the same amount a larger share than the cap that
///   admitted it, or more than the line is worth (E.19), so the discount is
///   cleared first. A percentage off keeps its share and needs nothing.
///
/// A lower quantity lowers the basket's value, so it clears an amount off the
/// basket (1.4.6).
pub fn set_qty(cart: Cart, line: SaleLineId, qty: Qty) -> Result<Cart, CartError> {
    let mut cart = cart;
    let found = cart
        .lines
        .iter_mut()
        .find(|l| l.id == line)
        .ok_or(CartError::LineNotFound(line))?;
    let product = match (&found.price_origin, found.product_id) {
        (PriceOrigin::Label { .. }, _) => return Err(CartError::LabelLineIsOnePackage(line)),
        (PriceOrigin::Department { .. }, _) | (_, None) => {
            return Err(CartError::DepartmentPriceOutsideDepartmentSale);
        }
        (PriceOrigin::Catalog { .. }, Some(product)) => product,
    };
    check_qty(product, found.unit, qty)?;
    if let Some(LineDiscount {
        kind: DiscountKind::Amount(_),
        ..
    }) = found.discount
    {
        return Err(CartError::AmountDiscountFixesQuantity(line));
    }
    let lowered = qty < found.qty;
    found.qty = qty;
    if lowered {
        clear_amount_off_basket(&mut cart);
    }
    Ok(cart)
}

/// `cart` without line `line`, and the audit intent that records who removed
/// it, why, and what it was.
///
/// `at` is when, supplied by the shell (I-8). The intent's action is the
/// `line.void` capability's spelling, and its payload carries the line's
/// facts — never its name, which is not needed to identify it. A void lowers
/// the basket's value, so it clears an amount off the basket (1.4.6).
///
/// # Errors
///
/// [`CartError::LineNotFound`].
pub fn void_line(
    cart: Cart,
    line: SaleLineId,
    reason: VoidReason,
    by: UserId,
    at: Timestamp,
) -> Result<(Cart, AuditIntent), CartError> {
    let mut cart = cart;
    let position = cart
        .lines
        .iter()
        .position(|l| l.id == line)
        .ok_or(CartError::LineNotFound(line))?;
    let removed = cart.lines.remove(position);
    clear_amount_off_basket(&mut cart);
    let intent = AuditIntent {
        actor: by,
        approver: None,
        approval: None,
        action: cap::LineVoid::NAME,
        entity: "sale_line",
        entity_id: line.as_uuid(),
        reason: Some(reason.0),
        payload: json!({
            "sale_id": cart.id.as_uuid().to_string(),
            "product_id": removed.product_id.map(|p| p.as_uuid().to_string()),
            "qty_milli": removed.qty.milli(),
            "unit_price_minor": removed.unit_price.minor(),
            "currency": removed.unit_price.currency().code(),
        }),
        at,
    };
    Ok((cart, intent))
}

/// `cart` with line `line` discounted as `request` asks, and the audit intent
/// that records it under `discount.manual` (1.4.5).
///
/// `limit` is the actor's own cap, which the shell resolves from their grant
/// and never from the request (merchant decisions 3.1–3.3). `rule` is the
/// store's rounding rule: it states in money what a share takes off the line
/// now, for the intent and for an approval's binding, and decides nothing else.
/// The discount itself is kept as asked, so a percentage keeps its share if
/// the quantity or the price changes, and is turned into money when the cart
/// is priced. The line is worth its [`CartLine::price`] times its quantity, so
/// an overridden line is measured at the price it charges (1.4.7).
///
/// A discount replaces any the line had, so two discounts within the cap never
/// add up past it, and a discount of zero clears it. A discount on a line
/// lowers the basket's value, so it clears an amount off the basket (1.4.6);
/// the line's discount and the basket's are each held to the cap on their own. Within the cap no approval
/// is needed and none is consumed. Above it, `approval` must be a handle a
/// manager issued for this actor, this line and this amount, for the same
/// reason, valid at `auth.at()`; the intent then names its approver and its id,
/// which the shell consumes with this effect.
///
/// # Errors
///
/// - [`CartError::LineNotFound`].
/// - [`CartError::NegativeDiscount`]: a surcharge is a price change, which only
///   an audited override makes (1.4.7).
/// - [`CartError::NegativeLine`]: a line worth less than nothing (E.19).
/// - [`CartError::Money`]: an amount in another currency than the cart's.
/// - [`CartError::DiscountExceedsLine`]: more than the line is worth (E.19).
/// - [`CartError::DiscountAboveCap`]: above the actor's cap, with no approval.
/// - [`CartError::ApprovalReasonMismatch`] and [`CartError::Permission`]: an
///   approval given for another reason, actor, line or amount, or no longer
///   valid.
pub fn apply_line_discount(
    cart: Cart,
    line: SaleLineId,
    request: DiscountRequest,
    limit: DiscountCap,
    rule: RoundingRule,
    auth: &Authorized<cap::DiscountManual>,
    approval: Option<&ApprovalHandle>,
) -> Result<(Cart, AuditIntent), CartError> {
    let mut cart = cart;
    let (sale, currency) = (cart.id, cart.currency);
    let found = cart
        .lines
        .iter_mut()
        .find(|l| l.id == line)
        .ok_or(CartError::LineNotFound(line))?;
    let kind = request.kind;
    if kind.is_negative() {
        return Err(CartError::NegativeDiscount(kind.format()));
    }
    let price = found.price();
    if price.is_negative() {
        return Err(CartError::NegativeLine);
    }
    if let DiscountKind::Amount(amount) = kind
        && amount.currency() != currency
    {
        return Err(MoneyError::CurrencyMismatch(currency.code(), amount.currency().code()).into());
    }
    if exceeds_value(kind, price, found.qty) {
        return Err(CartError::DiscountExceedsLine(kind.format()));
    }
    let discount = LineDiscount {
        kind,
        reason: request.reason,
        by: auth.actor(),
        approver: None,
    };
    let gross = price.mul_qty(found.qty, rule)?;
    let amount = discount.amount_off(gross, rule)?;
    let (approver, approval_id) = if kind.is_zero() || limit.admits(kind, price, found.qty) {
        (auth.approver(), auth.approval())
    } else {
        let Some(handle) = approval else {
            return Err(CartError::DiscountAboveCap(kind.format(), limit.format()));
        };
        if handle.reason() != discount.reason.0 {
            return Err(CartError::ApprovalReasonMismatch(
                handle.reason().to_owned(),
                discount.reason.0,
            ));
        }
        let binding = ApprovalBinding {
            entity_id: line.as_uuid(),
            amount_minor: amount.minor(),
            content_hash: None,
        };
        handle.matches::<cap::DiscountManual>(auth.actor(), &binding, auth.at())?;
        (Some(handle.approver()), Some(handle.id()))
    };
    let product = found.product_id;
    let reason = discount.reason.0.clone();
    found.discount = (!kind.is_zero()).then_some(LineDiscount {
        approver,
        ..discount
    });
    if !kind.is_zero() {
        clear_amount_off_basket(&mut cart);
    }
    let intent = AuditIntent {
        actor: auth.actor(),
        approver,
        approval: approval_id,
        action: cap::DiscountManual::NAME,
        entity: "sale_line",
        entity_id: line.as_uuid(),
        reason: Some(reason),
        payload: json!({
            "sale_id": sale.as_uuid().to_string(),
            "product_id": product.map(|p| p.as_uuid().to_string()),
            "kind": match kind {
                DiscountKind::Percent(_) => "percent",
                DiscountKind::Amount(_) => "amount",
            },
            "percent_ppm": match kind {
                DiscountKind::Percent(p) => Some(p.ppm()),
                DiscountKind::Amount(_) => None,
            },
            "amount_minor": amount.minor(),
            "line_value_minor": gross.minor(),
            "currency": currency.code(),
            "cap_ppm": limit.ppm(),
        }),
        at: auth.at(),
    };
    Ok((cart, intent))
}

/// An operator's answer to an age-restricted line's gate (E.69), and the audit
/// intent that records it.
///
/// Confirmed, the line stays and is no longer blocked; declined, it is removed,
/// which clears an amount off the basket as any lower basket value does (1.4.6).
/// Either way the intent names the operator, the line and its product's
/// minimum age, at `at`, which the shell supplies (I-8). The blocked line was
/// kept in the cart so that this call has a stable target across every
/// rerender of the screen (1.4.3).
///
/// # Errors
///
/// [`CartError::LineNotFound`], and [`CartError::AgeConfirmationNotPending`]
/// for a line with no age gate or one already confirmed: an answer for the
/// wrong line never confirms the right one, and a second answer would be a
/// second audit row for one decision.
pub fn confirm_age(
    cart: Cart,
    line_id: SaleLineId,
    confirmed: bool,
    actor: UserId,
    at: Timestamp,
) -> Result<(Cart, AuditIntent), CartError> {
    let mut cart = cart;
    let position = cart
        .lines
        .iter()
        .position(|l| l.id == line_id)
        .ok_or(CartError::LineNotFound(line_id))?;
    let (min_age, line) = match cart.lines.get(position) {
        Some(line) => match (line.min_age, line.age_confirmed) {
            (Some(min_age), false) => (min_age, line.clone()),
            _ => return Err(CartError::AgeConfirmationNotPending(line_id)),
        },
        None => return Err(CartError::LineNotFound(line_id)),
    };
    let action = if confirmed {
        if let Some(gated) = cart.lines.get_mut(position) {
            gated.age_confirmed = true;
        }
        "age.confirm"
    } else {
        cart.lines.remove(position);
        clear_amount_off_basket(&mut cart);
        "age.decline"
    };
    let intent = AuditIntent {
        actor,
        approver: None,
        approval: None,
        action,
        entity: "sale_line",
        entity_id: line_id.as_uuid(),
        reason: None,
        payload: json!({
            "sale_id": cart.id.as_uuid().to_string(),
            "product_id": line.product_id.map(|p| p.as_uuid().to_string()),
            "min_age": min_age,
            "confirmed": confirmed,
            "qty_milli": line.qty.milli(),
            "unit_price_minor": line.unit_price.minor(),
            "currency": line.unit_price.currency().code(),
        }),
        at,
    };
    Ok((cart, intent))
}

/// `cart` with its basket discounted as `request` asks, and the audit intent
/// that records it under `discount.manual` (1.4.6).
///
/// The basket is worth what its lines are worth once each line's own discount
/// is taken off, priced under `rule`, and the discount is measured against
/// that. It is held to the actor's cap `limit` on its own: a line's discount
/// and the basket's may both apply, each within the cap (the owner's decision,
/// 29 October 2026). As for a line, a discount replaces any the basket had and
/// a zero clears it; above the cap, `approval` must be a handle a manager
/// issued for this actor, this sale (`entity_id` is the sale's id), this
/// amount and this reason, valid at `auth.at()`.
///
/// A share keeps its share as the basket changes. An amount off is cleared by
/// any change that lowers the basket's value — a void, an age decline, a
/// lower quantity, a line discount — so it never takes more than the basket is
/// worth nor a larger share than the cap admitted; the priced cart shows it
/// gone, and the cashier gives it again if it still applies.
///
/// # Errors
///
/// - [`CartError::NegativeDiscount`]: a surcharge (1.4.7's to make).
/// - [`CartError::NegativeLine`]: a line worth less than nothing (E.19).
/// - [`CartError::Money`]: an amount in another currency than the cart's, or
///   an overflow pricing the lines.
/// - [`CartError::DiscountExceedsBasket`]: more than the basket is worth.
/// - [`CartError::DiscountAboveCap`]: above the actor's cap, with no approval.
/// - [`CartError::ApprovalReasonMismatch`] and [`CartError::Permission`]: an
///   approval given for another reason, actor, sale or amount, or no longer
///   valid.
pub fn apply_basket_discount(
    cart: Cart,
    request: DiscountRequest,
    limit: DiscountCap,
    rule: RoundingRule,
    auth: &Authorized<cap::DiscountManual>,
    approval: Option<&ApprovalHandle>,
) -> Result<(Cart, AuditIntent), CartError> {
    let mut cart = cart;
    let kind = request.kind;
    if kind.is_negative() {
        return Err(CartError::NegativeDiscount(kind.format()));
    }
    if cart.lines.iter().any(|l| l.price().is_negative()) {
        return Err(CartError::NegativeLine);
    }
    if let DiscountKind::Amount(amount) = kind
        && amount.currency() != cart.currency
    {
        return Err(
            MoneyError::CurrencyMismatch(cart.currency.code(), amount.currency().code()).into(),
        );
    }
    let value = basket_value(&cart, rule)?;
    if exceeds_basket(kind, value) {
        return Err(CartError::DiscountExceedsBasket(kind.format()));
    }
    let discount = BasketDiscount {
        kind,
        reason: request.reason,
        by: auth.actor(),
        approver: None,
    };
    let amount = discount.amount_off(value, rule)?;
    let (approver, approval_id) = if kind.is_zero() || limit.admits_basket(kind, value) {
        (auth.approver(), auth.approval())
    } else {
        let Some(handle) = approval else {
            return Err(CartError::DiscountAboveCap(kind.format(), limit.format()));
        };
        if handle.reason() != discount.reason.0 {
            return Err(CartError::ApprovalReasonMismatch(
                handle.reason().to_owned(),
                discount.reason.0,
            ));
        }
        let binding = ApprovalBinding {
            entity_id: cart.id.as_uuid(),
            amount_minor: amount.minor(),
            content_hash: None,
        };
        handle.matches::<cap::DiscountManual>(auth.actor(), &binding, auth.at())?;
        (Some(handle.approver()), Some(handle.id()))
    };
    let reason = discount.reason.0.clone();
    cart.basket_discount = (!kind.is_zero()).then_some(BasketDiscount {
        approver,
        ..discount
    });
    let intent = AuditIntent {
        actor: auth.actor(),
        approver,
        approval: approval_id,
        action: cap::DiscountManual::NAME,
        entity: "sale",
        entity_id: cart.id.as_uuid(),
        reason: Some(reason),
        payload: json!({
            "sale_id": cart.id.as_uuid().to_string(),
            "kind": match kind {
                DiscountKind::Percent(_) => "percent",
                DiscountKind::Amount(_) => "amount",
            },
            "percent_ppm": match kind {
                DiscountKind::Percent(p) => Some(p.ppm()),
                DiscountKind::Amount(_) => None,
            },
            "amount_minor": amount.minor(),
            "basket_value_minor": value.minor(),
            "lines": cart.lines.len(),
            "currency": cart.currency.code(),
            "cap_ppm": limit.ppm(),
        }),
        at: auth.at(),
    };
    Ok((cart, intent))
}

/// `cart` with line `line` charging `to` a unit, for `reason`, and the audit
/// intent that records it under `price.override` (1.4.7). A displayed price
/// also returns the label to print again (E.70).
///
/// `unit_cost` is the product's cost per unit, [`StockPosition::cost_basis`],
/// which the shell reads from the stock ledger: `None` when no cost was ever
/// recorded. The line keeps the price it was captured at (I-5) and charges
/// `to` from now on. Another override replaces this one, and an override to
/// the captured price clears it.
///
/// Three limits hold, and the owner set the last two on 29 October 2026:
/// - **The ministry ceiling (E.71) is a hard block.** No price goes above it,
///   whoever approved it, a displayed price included.
/// - **The floor is cost** (merchant decision 3.4). A price below the one the
///   line was captured at may not go below the product's cost, and a product
///   with no recorded cost may not be lowered at all. The floor never refuses
///   the captured price or more, even where the catalogue sells below cost.
/// - **A displayed price wins over the floor** (J.3). The shelf showed less
///   than the till charges, and the customer pays the shelf's price whatever
///   the cost. It only ever lowers the price, and asks for the product's label
///   to be printed again.
///
/// Who may override is the grid's: `price.override` is the shift lead's and
/// the manager's, never the cashier's. Where the merchant's policy escalates
/// it, `auth` carries the approver, whose handle is bound to this line
/// (`entity_id`) and to `to` (`amount_minor`), the amount the intent records.
/// A lower price lowers the basket's value, so it clears an amount off the
/// basket (1.4.6); a share off the line or the basket keeps its share.
///
/// [`StockPosition::cost_basis`]: crate::stock::StockPosition::cost_basis
///
/// # Errors
///
/// - [`CartError::LineNotFound`].
/// - [`CartError::LabelPriceNotOverridable`]: a price label is the price
///   displayed on the package itself; a wrong one is weighed and labelled
///   again.
/// - [`CartError::DepartmentPriceOutsideDepartmentSale`]: a department line,
///   whose typed amount is its own capped command's (1.4.12).
/// - [`CartError::Money`]: a price in another currency than the cart's, or a
///   ceiling or cost that cannot be compared with it.
/// - [`CartError::NegativeLine`]: a price below nothing (E.19).
/// - [`CartError::AmountDiscountFixesPrice`]: a line with an amount off. A
///   lower price would make the same amount a larger share than the cap that
///   admitted it, or more than the line is worth, so the discount is cleared
///   first. A share off keeps its share and needs nothing.
/// - [`CartError::AboveMaxPrice`]: above the ministry ceiling.
/// - [`CartError::DisplayedPriceNotLower`]: a displayed price at or above the
///   price the line charges, which is no shelf price to honour.
/// - [`CartError::NoRecordedCost`] and [`CartError::BelowPriceFloor`]: a
///   lower price with no cost to hold it to, or below the cost.
pub fn override_price(
    cart: Cart,
    line: SaleLineId,
    to: Money,
    reason: OverrideReason,
    unit_cost: Option<Money>,
    auth: &Authorized<cap::PriceOverride>,
) -> Result<(Cart, AuditIntent, Option<LabelReprint>), CartError> {
    let mut cart = cart;
    let (sale, currency) = (cart.id, cart.currency);
    let found = cart
        .lines
        .iter_mut()
        .find(|l| l.id == line)
        .ok_or(CartError::LineNotFound(line))?;
    let product = match (&found.price_origin, found.product_id) {
        (PriceOrigin::Label { .. }, _) => return Err(CartError::LabelPriceNotOverridable(line)),
        (PriceOrigin::Department { .. }, _) | (_, None) => {
            return Err(CartError::DepartmentPriceOutsideDepartmentSale);
        }
        (PriceOrigin::Catalog { .. }, Some(product)) => product,
    };
    if to.currency() != currency {
        return Err(MoneyError::CurrencyMismatch(currency.code(), to.currency().code()).into());
    }
    if to.is_negative() {
        return Err(CartError::NegativeLine);
    }
    if let Some(LineDiscount {
        kind: DiscountKind::Amount(_),
        ..
    }) = found.discount
    {
        return Err(CartError::AmountDiscountFixesPrice(line));
    }
    let (captured, current, ceiling) = (found.unit_price, found.price(), found.max_price);
    if let Some(ceiling) = ceiling
        && to.checked_cmp(ceiling)? == Ordering::Greater
    {
        return Err(CartError::AboveMaxPrice(
            to.format_exact(),
            ceiling.format_exact(),
        ));
    }
    let lowered = to.checked_cmp(current)? == Ordering::Less;
    let displayed = reason.is_displayed_price();
    let floor = if displayed {
        if !lowered {
            return Err(CartError::DisplayedPriceNotLower(
                to.format_exact(),
                current.format_exact(),
            ));
        }
        None
    } else if to.checked_cmp(captured)? == Ordering::Less {
        let Some(cost) = unit_cost else {
            return Err(CartError::NoRecordedCost(product, to.format_exact()));
        };
        if to.checked_cmp(cost)? == Ordering::Less {
            return Err(CartError::BelowPriceFloor(
                to.format_exact(),
                cost.format_exact(),
            ));
        }
        Some(cost)
    } else {
        None
    };
    let code = reason.code().to_owned();
    found.price_override = (to != captured).then(|| PriceOverride {
        to,
        reason,
        by: auth.actor(),
        approver: auth.approver(),
    });
    let qty = found.qty;
    if lowered {
        clear_amount_off_basket(&mut cart);
    }
    let intent = AuditIntent {
        actor: auth.actor(),
        approver: auth.approver(),
        approval: auth.approval(),
        action: cap::PriceOverride::NAME,
        entity: "sale_line",
        entity_id: line.as_uuid(),
        reason: Some(code),
        payload: json!({
            "sale_id": sale.as_uuid().to_string(),
            "product_id": product.as_uuid().to_string(),
            "amount_minor": to.minor(),
            "captured_minor": captured.minor(),
            "previous_minor": current.minor(),
            "qty_milli": qty.milli(),
            "currency": currency.code(),
            "floor_minor": floor.map(Money::minor),
            "ceiling_minor": ceiling.map(Money::minor),
            "displayed_price": displayed,
        }),
        at: auth.at(),
    };
    let reprint = displayed.then_some(LabelReprint {
        product_id: product,
        cause: ReprintCause::DisplayedPriceOverride,
    });
    Ok((cart, intent, reprint))
}

/// Every line's share of every manual discount on `cart`, in money, priced
/// under `rule` (1.4.6, §6.2).
///
/// A line's own discount is attributed to it whole. The basket's is turned
/// into money against the basket's value and divided among the lines in
/// proportion to what each is worth once its own discount is off, by
/// [`prorate`]: to the fil, largest remainder, ties by what the lines are and
/// never by where they sit. A share of nothing is no attribution. The basket's
/// attributions add up to its discount exactly, which is the recap a fiscal
/// document repeats (2.7.3).
///
/// # Errors
///
/// [`CartError::Money`] when pricing a line overflows, and
/// [`CartError::DiscountExceedsBasket`] for a basket discount the lines can no
/// longer carry, which the clearing rule above keeps from arising.
pub fn discount_attributions(
    cart: &Cart,
    rule: RoundingRule,
) -> Result<Vec<DiscountAttribution>, CartError> {
    let mut attributions = Vec::new();
    let mut lines = Vec::with_capacity(cart.lines.len());
    for line in &cart.lines {
        let gross = line.price().mul_qty(line.qty, rule)?;
        let off = match &line.discount {
            Some(d) => d.amount_off(gross, rule)?,
            None => Money::zero(gross.currency()),
        };
        if let Some(d) = line.discount.as_ref().filter(|_| !off.is_zero()) {
            attributions.push(DiscountAttribution {
                line_id: line.id,
                source: DiscountSource::ManualLine,
                amount: off,
                percent: percent_of(d.kind),
                reason: d.reason.clone(),
                by: d.by,
                approver: d.approver,
            });
        }
        lines.push(ProrationLine {
            line_id: line.id,
            value: gross.checked_sub(off)?,
            tax_category_id: line.tax_category_id,
            product_id: line.product_id,
            // What the line is: the price it was captured at. What it is
            // worth, an override included, is its value above.
            unit_price: line.unit_price,
            qty: line.qty,
        });
    }
    let Some(basket) = &cart.basket_discount else {
        return Ok(attributions);
    };
    let value = Money::sum(lines.iter().map(|l| l.value), cart.currency)?;
    let amount = basket.amount_off(value, rule)?;
    let shares = prorate(amount, &lines).map_err(|e| match e {
        PricingError::ExceedsTotal(asked, _) => CartError::DiscountExceedsBasket(asked),
        PricingError::Money(m) => CartError::Money(m),
    })?;
    for (line, share) in lines.iter().zip(shares) {
        if share.is_zero() {
            continue;
        }
        attributions.push(DiscountAttribution {
            line_id: line.line_id,
            source: DiscountSource::ManualBasket,
            amount: share,
            percent: percent_of(basket.kind),
            reason: basket.reason.clone(),
            by: basket.by,
            approver: basket.approver,
        });
    }
    Ok(attributions)
}

/// What a store's lines are priced under (1.4.9): its price mode (merchant
/// decision 2.4, `store.price_mode`), its tax profile (1.3.8: `Unregistered`
/// where `store.gst_profile` says so, otherwise `store.tax_profile`), and the
/// rounding rule its approved tax computation policy pins. None of the three
/// has a default, so a setting the shell failed to read cannot become a
/// plausible one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PricingSettings {
    pub price_mode: PriceMode,
    pub profile: StoreTaxProfile,
    pub rule: RoundingRule,
}

/// The tax components a store's pack charges on each tax category on the
/// sale's business date: `pos_db::tax_pack::TaxPackRepository::components_for`
/// for every category the cart holds (1.3.7). The pack is already the store's
/// profile's and already dated, so pricing chooses nothing among them.
pub type TaxBands = BTreeMap<TaxCategoryId, Vec<TaxComponent>>;

/// A cart in money (§6.2): every line priced, and the receipt's totals.
///
/// `total` is the sum of the lines' totals, and equals `subtotal` less
/// `discount_total`, plus `tax_total` where prices exclude tax. Where they
/// include it, the tax is already inside every line's total. It is what the
/// customer owes before any cash rounding, which a final cash tender adds
/// (1.5.3, 1.4.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricedCart {
    pub lines: Vec<PricedLine>,
    /// Every line's gross before its discounts.
    pub subtotal: Money,
    /// Every discount attributed to a line, line and basket alike.
    pub discount_total: Money,
    /// The receipt's tax rows, from the lines' own results (1.3.6).
    pub tax_summary: Vec<TaxSummaryRow>,
    pub tax_total: Money,
    pub total: Money,
    pub currency: Currency,
}

/// One line in money (§6.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricedLine {
    pub line_id: SaleLineId,
    /// What the line charges a unit, [`CartLine::price`], times its quantity,
    /// rounded once by the store's rule: an overridden line at the price it
    /// charges.
    pub gross_before_discount: Money,
    /// The line's own discount, then its share of the basket's, exactly as
    /// [`discount_attributions`] divides them.
    pub discount_attributions: Vec<DiscountAttribution>,
    /// What the tax is computed on: the gross less its discounts. Where prices
    /// include tax this amount contains it, and where they exclude it this is
    /// the net.
    pub taxable: Money,
    pub tax: LineTax,
    /// What the line adds to the total, `tax.gross`: the taxable amount where
    /// prices include tax, and the taxable amount plus its tax where they
    /// exclude it.
    pub line_total: Money,
}

/// The one function that turns a cart into money (1.4.9, §6.2): the sole
/// source of every number on the receipt and in the fiscal document, never
/// recomputed elsewhere.
///
/// Each line is priced on its own. Its gross is [`CartLine::price`] times its
/// quantity, rounded once by the store's rule, so an overridden line is priced
/// at the price it charges. Its discounts are [`discount_attributions`],
/// read rather than divided again, so the receipt and the fiscal document
/// share one division of the basket. Its tax is [`compute_line_tax`] on what
/// is left, under the store's price mode, with the components the store's pack
/// charges on its category. The summary is [`summarize_tax`] over the lines'
/// own results. Nothing depends on where a line sits: a line's price and tax are
/// its own, and the basket's division is decided by what the lines are
/// (1.4.6), so the same items in any order cost the same (E.19b).
///
/// A store not registered for GST charges no tax and needs no band. A discount
/// of everything is a sale of nothing, priced at zero (E.18).
///
/// # Errors
///
/// - [`CartError::DuplicateLineId`]: two lines with one id, which no division
///   or sale row could tell apart.
/// - [`CartError::TaxBandMissing`]: a registered store with no components for
///   a line's tax category. It is refused, never sold at a familiar rate.
/// - [`CartError::NegativeLine`]: a line worth less than nothing after its
///   discounts (E.19), which the discount transitions keep from arising.
/// - [`CartError::DiscountExceedsBasket`]: a basket discount the lines can no
///   longer carry, as for [`discount_attributions`].
/// - [`CartError::Tax`]: what [`compute_line_tax`] and [`summarize_tax`]
///   refuse, such as a supply that is not ordinary domestic retail.
/// - [`CartError::Money`]: an amount in another currency, or an overflow.
pub fn price_cart(
    cart: &Cart,
    bands: &TaxBands,
    settings: &PricingSettings,
) -> Result<PricedCart, CartError> {
    let (currency, rule) = (cart.currency, settings.rule);
    let mut seen = BTreeSet::new();
    if let Some(twice) = cart.lines.iter().find(|l| !seen.insert(l.id)) {
        return Err(CartError::DuplicateLineId(twice.id));
    }
    let mut attributions: BTreeMap<SaleLineId, Vec<DiscountAttribution>> = BTreeMap::new();
    for a in discount_attributions(cart, rule)? {
        attributions.entry(a.line_id).or_default().push(a);
    }
    let zero = Money::zero(currency);
    let (mut subtotal, mut discount_total, mut tax_total, mut total) = (zero, zero, zero, zero);
    let mut lines = Vec::with_capacity(cart.lines.len());
    for line in &cart.lines {
        let gross = line.price().mul_qty(line.qty, rule)?;
        let own = attributions.remove(&line.id).unwrap_or_default();
        let off = Money::sum(own.iter().map(|a| a.amount), currency)?;
        let taxable = gross.checked_sub(off)?;
        if taxable.is_negative() {
            return Err(CartError::NegativeLine);
        }
        let components = if settings.profile == StoreTaxProfile::Unregistered {
            &[][..]
        } else {
            bands
                .get(&line.tax_category_id)
                .ok_or(CartError::TaxBandMissing(line.tax_category_id))?
                .as_slice()
        };
        let tax = compute_line_tax(
            taxable,
            line.qty,
            settings.price_mode,
            components,
            settings.profile,
            &cart.supply,
            rule,
        )?;
        subtotal = subtotal.checked_add(gross)?;
        discount_total = discount_total.checked_add(off)?;
        tax_total = tax_total.checked_add(tax.tax_total)?;
        total = total.checked_add(tax.gross)?;
        lines.push(PricedLine {
            line_id: line.id,
            gross_before_discount: gross,
            discount_attributions: own,
            taxable,
            line_total: tax.gross,
            tax,
        });
    }
    let taxes: Vec<LineTax> = lines.iter().map(|l| l.tax.clone()).collect();
    Ok(PricedCart {
        tax_summary: summarize_tax(&taxes)?,
        lines,
        subtotal,
        discount_total,
        tax_total,
        total,
        currency,
    })
}

/// What the basket is worth: each line at its price, overridden or not, priced
/// under `rule`, less its own discount.
fn basket_value(cart: &Cart, rule: RoundingRule) -> Result<Money, CartError> {
    let mut value = Money::zero(cart.currency);
    for line in &cart.lines {
        let gross = line.price().mul_qty(line.qty, rule)?;
        let off = match &line.discount {
            Some(d) => d.amount_off(gross, rule)?,
            None => Money::zero(gross.currency()),
        };
        value = value.checked_add(gross.checked_sub(off)?)?;
    }
    Ok(value)
}

/// The percentage a discount was asked as, for its attributions' provenance.
const fn percent_of(kind: DiscountKind) -> Option<crate::money::Percent> {
    match kind {
        DiscountKind::Percent(p) => Some(p),
        DiscountKind::Amount(_) => None,
    }
}

/// Clears an amount off the basket after a change that lowered its value; a
/// share off stays (1.4.6).
fn clear_amount_off_basket(cart: &mut Cart) {
    if let Some(BasketDiscount {
        kind: DiscountKind::Amount(_),
        ..
    }) = cart.basket_discount
    {
        cart.basket_discount = None;
    }
}

/// Whether every age-restricted line has been confirmed by an operator.
///
/// 1.4.8's `begin_tender` refuses a cart this refuses.
///
/// # Errors
///
/// [`CartError::AgeConfirmationRequired`] naming the first line's product
/// still waiting for confirmation.
pub fn check_age_confirmed(cart: &Cart) -> Result<(), CartError> {
    match cart
        .lines
        .iter()
        .find(|l| l.min_age.is_some() && !l.age_confirmed)
    {
        Some(CartLine {
            product_id: Some(product),
            ..
        }) => Err(CartError::AgeConfirmationRequired(*product)),
        // A line with no product is a department sale, which carries no age.
        Some(_) | None => Ok(()),
    }
}

/// A cart set aside so the cashier can serve the next customer (1.4.4).
///
/// Its own type, so the state machine holds here too: a cart being built
/// cannot be passed to [`resume`], and no line of a parked cart can be added,
/// changed or voided until it is resumed. [`park`] makes one, and so does its
/// stored form, which the register reads back after a restart (E.3, 1.8.2b);
/// that form is exactly its cart's, because parking adds nothing to a cart.
/// [`ParkedCart::cart`] reads it, for the list of parked carts the till shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParkedCart(Cart);

impl ParkedCart {
    /// The cart as it was parked.
    #[must_use]
    pub fn cart(&self) -> &Cart {
        &self.0
    }
}

/// `cart` set aside exactly as it is, so the cashier can serve the next
/// customer.
///
/// Nothing on it changes: its lines keep the names and prices they were
/// captured at (I-5), and a line waiting for an age confirmation is still
/// waiting when the cart is resumed. The label the cashier gives it and its
/// end-of-day expiry are the register's to keep beside it (1.8.2b).
///
/// # Errors
///
/// [`CartError::NothingToPark`] for a cart with no line, which is no basket to
/// come back to.
pub fn park(cart: Cart) -> Result<ParkedCart, CartError> {
    if cart.lines.is_empty() {
        return Err(CartError::NothingToPark);
    }
    Ok(ParkedCart(cart))
}

/// The parked cart, being built again at `till`: the register, shift and
/// cashier resuming it.
///
/// Its lines, prices, age gates, buyer, supply and the moment it was opened are
/// exactly as they were parked. Its shift and cashier become the till's: a cart
/// parked before a shift handover is paid for in the shift that takes the
/// money, and a completed sale must name its register's open shift
/// (`ref/schema.md` §0005). `till.opened_at` is not read, because a resumed sale
/// began when it was first opened.
///
/// # Errors
///
/// - [`CartError::ParkedOnAnotherRegister`]: a parked cart never leaves the
///   till it was parked at, where its customer is standing (master plan C.14).
/// - [`CartError::TrainingModeMismatch`]: a training cart never resumes as a
///   real sale, nor a real one as training. Every report and the fiscal queue
///   tell the two apart by this flag, so either would count the sale wrongly.
/// - [`CartError::Money`]: a till in another currency than the cart's prices.
pub fn resume(parked: ParkedCart, till: &CartContext) -> Result<Cart, CartError> {
    let ParkedCart(mut cart) = parked;
    if cart.register_id != till.register_id {
        return Err(CartError::ParkedOnAnotherRegister(cart.id));
    }
    if cart.is_training != till.is_training {
        return Err(CartError::TrainingModeMismatch(cart.id, cart.is_training));
    }
    if cart.currency != till.currency {
        return Err(
            MoneyError::CurrencyMismatch(cart.currency.code(), till.currency.code()).into(),
        );
    }
    cart.shift_id = till.shift_id;
    cart.cashier_id = till.cashier_id;
    Ok(cart)
}

/// How a cart's training mode reads in [`CartError::TrainingModeMismatch`].
fn training_mode(is_training: bool) -> &'static str {
    if is_training {
        "in training mode"
    } else {
        "live"
    }
}

/// A quantity a line of `unit` may carry: more than nothing, and a whole number
/// of units where the unit is counted rather than measured.
fn check_qty(product: ProductId, unit: UnitOfMeasure, qty: Qty) -> Result<(), CartError> {
    if qty.milli() <= 0 {
        return Err(CartError::NonPositiveQty);
    }
    if !unit.is_divisible() && !qty.is_whole_units() {
        return Err(CartError::FractionalQtyForDiscrete(product, qty.milli()));
    }
    Ok(())
}

/// Every refusal a cart transition can return (`ref/domain-api.md` §6.3).
///
/// Each carries the data its message needs, so the shell maps a variant to
/// the code the screen renders from and never parses a message. Amounts in
/// a message are carried as their exact rendering.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CartError {
    #[error("no line {0}")]
    LineNotFound(SaleLineId),
    /// I-4: nothing changes a completed sale.
    #[error("cannot modify a completed sale")]
    SaleIsComplete,
    #[error("cannot go back to building: {0} tender(s) collected")]
    TenderCollected(usize),
    #[error("quantity must be positive")]
    NonPositiveQty,
    #[error("{0} is sold in whole units; {1} milli is not one")]
    FractionalQtyForDiscrete(ProductId, i64),
    #[error("a caller-supplied price needs the department-sale capability")]
    DepartmentPriceOutsideDepartmentSale,
    #[error("department sale of {0} exceeds the cap {1}")]
    DepartmentAmountAboveCap(String, String),
    /// E.19.
    #[error("negative line amounts are not allowed")]
    NegativeLine,
    #[error("discount {0} exceeds line value")]
    DiscountExceedsLine(String),
    /// 1.4.7: the price asked for and the floor, the product's cost
    /// (merchant decision 3.4).
    #[error("price {0} is below the floor {1}")]
    BelowPriceFloor(String, String),
    /// E.71: a regulated ceiling is a hard block.
    #[error("price {0} exceeds the regulated ceiling {1}")]
    AboveMaxPrice(String, String),
    /// E.69.
    #[error("age confirmation required for {0}")]
    AgeConfirmationRequired(ProductId),
    /// E.38.
    #[error("product {0} is not sellable")]
    ProductInactive(ProductId),
    #[error("collected {0} is less than due {1}")]
    Underpaid(String, String),
    #[error("overtender is only allowed for cash")]
    OvertenderNotCash,
    /// Added at 1.4.3: a scan that found several products, none, or a damaged
    /// label is a choice for the till, not a line.
    #[error("the scan did not resolve to one product")]
    ScanNotOneProduct,
    /// Added at 1.4.3: a price label's line is one package.
    #[error("line {0} is one labelled package; scan the next label for another")]
    LabelLineIsOnePackage(SaleLineId),
    /// Added at 1.4.3b: no age is waiting to be confirmed on that line.
    #[error("line {0} has no age waiting to be confirmed")]
    AgeConfirmationNotPending(SaleLineId),
    /// Added at 1.4.4: a cart with no line is no basket to come back to.
    #[error("an empty cart cannot be parked")]
    NothingToPark,
    /// Added at 1.4.4: a parked cart resumes only at the till it was parked at.
    #[error("sale {0} was parked on another register")]
    ParkedOnAnotherRegister(SaleId),
    /// Added at 1.4.4: the sale and whether it was parked in training mode,
    /// which the resuming till's mode must match.
    #[error(
        "sale {0} was parked {parked} and this till is {till}",
        parked = training_mode(*.1),
        till = training_mode(!*.1)
    )]
    TrainingModeMismatch(SaleId, bool),
    /// Added at 1.4.5: the discount asked for and the actor's cap, rendered.
    #[error("discount {0} is above the cap {1} and needs a manager's approval")]
    DiscountAboveCap(String, String),
    /// Added at 1.4.6: the basket discount asked for, rendered.
    #[error("discount {0} exceeds the basket's value")]
    DiscountExceedsBasket(String),
    /// Added at 1.4.5: a discount of less than nothing is a surcharge.
    #[error("discount {0} is negative")]
    NegativeDiscount(String),
    /// Added at 1.4.5: an amount off fixes the line's quantity until cleared.
    #[error("line {0} has an amount off; clear the discount to change its quantity")]
    AmountDiscountFixesQuantity(SaleLineId),
    /// Added at 1.4.7: an amount off fixes the line's price too.
    #[error("line {0} has an amount off; clear the discount to change its price")]
    AmountDiscountFixesPrice(SaleLineId),
    /// Added at 1.4.7: a price label is the price displayed on the package.
    #[error("line {0} is priced by its label; weigh and label it again to change its price")]
    LabelPriceNotOverridable(SaleLineId),
    /// Added at 1.4.7: the displayed price asked for, then the price the line
    /// charges. A shelf price only ever lowers what the till charges.
    #[error("a displayed price of {0} is not below the {1} the line charges")]
    DisplayedPriceNotLower(String, String),
    /// Added at 1.4.7: the product and the lower price asked for. With no
    /// recorded cost there is no floor to hold the price to (the owner's
    /// decision, 29 October 2026).
    #[error("{0} has no recorded cost, so its price cannot be lowered to {1}")]
    NoRecordedCost(ProductId, String),
    /// Added at 1.4.9: the store's pack charges nothing the register holds on
    /// this tax category. An unconfigured band fails closed.
    #[error("no tax band is configured for category {0}")]
    TaxBandMissing(TaxCategoryId),
    /// Added at 1.4.9: two lines carry this id.
    #[error("line {0} appears twice in the cart")]
    DuplicateLineId(SaleLineId),
    /// Added at 1.4.5: the approval's reason, then the request's. The audit
    /// row repeats the handle's reason, so the two must be one.
    #[error("the approval was given for {0}, not {1}")]
    ApprovalReasonMismatch(String, String),
    /// Added at 1.4.5: an approval that does not match the operation.
    #[error(transparent)]
    Permission(#[from] PermissionError),
    #[error(transparent)]
    Tax(#[from] TaxError),
    #[error(transparent)]
    Money(#[from] MoneyError),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::catalog::DerivedWeight;
    use uuid::Uuid;

    // Wire-format tripwires, not business data: every id, amount and name is
    // visibly synthetic.
    const GOLDEN_CART_JSON: &str = r#"{"id":"00000000-0000-0000-0000-000000000001","register_id":"00000000-0000-0000-0000-000000000002","shift_id":"00000000-0000-0000-0000-000000000003","cashier_id":"00000000-0000-0000-0000-000000000004","currency":"JOD","lines":[{"id":"00000000-0000-0000-0000-000000000010","product_id":"00000000-0000-0000-0000-000000000011","name_snapshot":"SYNTH item","unit_price":{"minor":1250,"currency":"JOD"},"price_origin":{"catalog":{"product_id":"00000000-0000-0000-0000-000000000011"}},"max_price":null,"qty":1500,"unit":"kilogram","is_weighed":true,"tax_category_id":"00000000-0000-0000-0000-000000000012","discount":null,"price_override":null,"min_age":18,"age_confirmed":false,"label":{"qty":1500,"catalog_unit_price":{"minor":1250,"currency":"JOD"}}}],"basket_discount":null,"customer_id":null,"buyer_tin":"999888777","buyer_name":"SYNTH buyer","supply":{"destination":"Domestic","reason":null,"evidence_ref":null},"is_training":true,"opened_at":978307200000}"#;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn context(is_training: bool) -> CartContext {
        CartContext {
            register_id: RegisterId::from_uuid(id(2)),
            shift_id: ShiftId::from_uuid(id(3)),
            cashier_id: UserId::from_uuid(id(4)),
            currency: Currency::JOD,
            is_training,
            opened_at: Timestamp::from_epoch_milliseconds(978_307_200_000).unwrap(),
        }
    }

    fn line() -> CartLine {
        CartLine {
            id: SaleLineId::from_uuid(id(0x10)),
            product_id: Some(ProductId::from_uuid(id(0x11))),
            name_snapshot: "SYNTH item".to_owned(),
            unit_price: Money::from_minor(1_250, Currency::JOD),
            price_origin: PriceOrigin::Catalog {
                product_id: ProductId::from_uuid(id(0x11)),
            },
            max_price: None,
            qty: Qty::from_milli(1_500),
            unit: UnitOfMeasure::Kilogram,
            is_weighed: true,
            tax_category_id: TaxCategoryId::from_uuid(id(0x12)),
            discount: None,
            price_override: None,
            min_age: Some(18),
            age_confirmed: false,
            label: Some(DerivedWeight {
                qty: Qty::from_milli(1_500),
                catalog_unit_price: Money::from_minor(1_250, Currency::JOD),
            }),
        }
    }

    fn cart() -> Cart {
        let mut cart = open(SaleId::from_uuid(id(1)), &context(true));
        cart.lines.push(line());
        // A TIN no id in the fixture contains, so seeing it proves the TIN.
        cart.buyer_tin = Some("999888777".to_owned());
        cart.buyer_name = Some("SYNTH buyer".to_owned());
        cart
    }

    #[test]
    fn a_new_cart_is_empty_domestic_and_its_context_is_copied() {
        let ctx = context(false);
        let cart = open(SaleId::from_uuid(id(1)), &ctx);
        assert_eq!(cart.id, SaleId::from_uuid(id(1)));
        assert_eq!(
            (cart.register_id, cart.shift_id, cart.cashier_id),
            (ctx.register_id, ctx.shift_id, ctx.cashier_id)
        );
        assert_eq!(cart.currency, Currency::JOD);
        assert_eq!(cart.opened_at, ctx.opened_at);
        assert!(cart.lines.is_empty());
        assert_eq!(
            (cart.customer_id, cart.buyer_tin, cart.buyer_name),
            (None, None, None)
        );
        assert_eq!(
            cart.supply,
            SupplyTaxContext {
                destination: SupplyDestination::Domestic,
                reason: None,
                evidence_ref: None,
            }
        );
        assert!(!cart.is_training);
    }

    #[test]
    fn a_training_cart_says_so_from_its_first_moment() {
        assert!(open(SaleId::from_uuid(id(1)), &context(true)).is_training);
        assert!(!open(SaleId::from_uuid(id(1)), &context(false)).is_training);
    }

    #[test]
    fn golden_cart_json_is_stable() {
        assert_eq!(serde_json::to_string(&cart()).unwrap(), GOLDEN_CART_JSON);
        let back: Cart = serde_json::from_str(GOLDEN_CART_JSON).unwrap();
        assert_eq!(back, cart());
    }

    #[test]
    fn a_sale_round_trips_in_either_state() {
        for sale in [Sale::Building(cart()), Sale::Parked(park(cart()).unwrap())] {
            let json = serde_json::to_string(&sale).unwrap();
            assert_eq!(serde_json::from_str::<Sale>(&json).unwrap(), sale);
        }
    }

    #[test]
    fn buyer_name_never_reaches_debug_output() {
        let cart = cart();
        for rendered in [
            format!("{cart:?}"),
            format!("{cart:#?}"),
            format!("{:?}", Sale::Parked(park(cart.clone()).unwrap())),
        ] {
            assert!(!rendered.contains("SYNTH buyer"), "{rendered}");
            assert!(rendered.contains("[redacted]"));
            // The TIN stays visible, as on the receipt's buyer block.
            assert!(rendered.contains("999888777"));
        }
        // A cart with no buyer shows no redaction marker.
        let anonymous = open(SaleId::from_uuid(id(1)), &context(false));
        assert!(!format!("{anonymous:?}").contains("[redacted]"));
    }

    #[test]
    fn cart_errors_carry_their_data() {
        let product = ProductId::from_uuid(id(0x11));
        let line = SaleLineId::from_uuid(id(0x10));
        let cases = [
            (CartError::LineNotFound(line), format!("no line {line}")),
            (
                CartError::TenderCollected(2),
                "cannot go back to building: 2 tender(s) collected".to_owned(),
            ),
            (
                CartError::FractionalQtyForDiscrete(product, 1_500),
                format!("{product} is sold in whole units; 1500 milli is not one"),
            ),
            (
                CartError::DepartmentAmountAboveCap("12.000".to_owned(), "10.000".to_owned()),
                "department sale of 12.000 exceeds the cap 10.000".to_owned(),
            ),
            (
                CartError::BelowPriceFloor("0.900".to_owned(), "1.000".to_owned()),
                "price 0.900 is below the floor 1.000".to_owned(),
            ),
            (
                CartError::AboveMaxPrice("1.200".to_owned(), "1.100".to_owned()),
                "price 1.200 exceeds the regulated ceiling 1.100".to_owned(),
            ),
            (
                CartError::AgeConfirmationRequired(product),
                format!("age confirmation required for {product}"),
            ),
            (
                CartError::ProductInactive(product),
                format!("product {product} is not sellable"),
            ),
            (
                CartError::Underpaid("4.000".to_owned(), "5.000".to_owned()),
                "collected 4.000 is less than due 5.000".to_owned(),
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn tax_and_money_errors_pass_through_unchanged() {
        assert_eq!(
            CartError::from(TaxError::NoComponents).to_string(),
            TaxError::NoComponents.to_string()
        );
        let money = MoneyError::Overflow;
        assert_eq!(CartError::from(money).to_string(), "arithmetic overflow");
    }

    // ── add_line, set_qty, void_line (1.4.3) ────────────────────────────

    use crate::catalog::{
        EmbeddedBarcodeRule, EmbeddedValue, MatchedOn, RegulatedSaleForm, SaleForm, parse_scan,
        resolve_scan,
    };
    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;

    const JOD: Currency = Currency::JOD;

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, JOD)
    }

    /// A synthetic product sold in `unit`, at 1.250 JOD a unit.
    fn product(n: u128, unit: UnitOfMeasure) -> Product {
        Product {
            id: ProductId::from_uuid(id(n)),
            sku: format!("SYNTH-{n}"),
            name_ar: format!("صنف اصطناعي {n}"),
            name_en: Some(format!("SYNTH item {n}")),
            category_id: None,
            tax_category_id: TaxCategoryId::from_uuid(id(0x12)),
            unit,
            unit_price: jod(1_250),
            is_weighed: unit.is_divisible(),
            is_service: false,
            is_active: true,
            min_age: None,
            max_price: None,
            reorder_point: None,
            regulated_sale_form: RegulatedSaleForm::Unregulated(if unit.is_divisible() {
                SaleForm::Bulk
            } else {
                SaleForm::SealedPack
            }),
        }
    }

    fn building() -> Cart {
        open(SaleId::from_uuid(id(1)), &context(false))
    }

    fn line_id(n: u128) -> SaleLineId {
        SaleLineId::from_uuid(id(0x1000 + n))
    }

    fn add(cart: Cart, n: u128, p: &Product, milli: i64) -> Result<Cart, CartError> {
        add_line(
            cart,
            AddLine::from_product(line_id(n), p, Qty::from_milli(milli))?,
        )
    }

    #[test]
    fn add_line_snapshots_name_and_price() {
        let mut item = product(0x20, UnitOfMeasure::Each);
        let cart = add(building(), 1, &item, 2_000).unwrap();
        let line = cart.lines.first().unwrap();
        assert_eq!(line.name_snapshot, item.name_ar);
        assert_eq!(line.unit_price, jod(1_250));
        assert_eq!(
            line.price_origin,
            PriceOrigin::Catalog {
                product_id: item.id
            }
        );
        assert_eq!(line.qty, Qty::from_milli(2_000));
        assert_eq!(
            (line.unit, line.is_weighed, line.tax_category_id),
            (item.unit, false, item.tax_category_id)
        );
        // The catalogue changes after the line was captured; the line does not,
        // because it is a copy and nothing reads the product again (I-5).
        let captured = line.clone();
        item.unit_price = jod(1_500);
        item.name_ar = "اسم جديد".to_owned();
        let cart = set_qty(cart, line_id(1), Qty::from_milli(3_000)).unwrap();
        let line = cart.lines.first().unwrap();
        assert_eq!(line.unit_price, captured.unit_price);
        assert_eq!(line.name_snapshot, captured.name_snapshot);
    }

    #[test]
    fn client_supplied_price_cannot_bypass_override() {
        let item = product(0x20, UnitOfMeasure::Each);
        let honest = AddLine::from_product(line_id(1), &item, Qty::ONE).unwrap();
        // The one constructor that takes an amount is the department's. Swapped
        // into an ordinary line to undercut the catalogue, it is refused: a
        // deliberate change of price is an override (1.4.7), audited.
        let mut undercut = honest.clone();
        undercut.price = PriceSource::from_department(jod(100), item.tax_category_id);
        assert_eq!(
            add_line(building(), undercut),
            Err(CartError::DepartmentPriceOutsideDepartmentSale)
        );
        // A line with no product is a department sale, and that path is
        // capped and audited (1.4.12), not this one.
        let mut nameless = honest.clone();
        nameless.product_id = None;
        assert_eq!(
            add_line(building(), nameless),
            Err(CartError::DepartmentPriceOutsideDepartmentSale)
        );
        // A price in another currency than the cart's is not this cart's price.
        let mut dollars = item.clone();
        dollars.unit_price = Money::from_minor(1_250, Currency::USD);
        assert_eq!(
            add_line(
                building(),
                AddLine::from_product(line_id(2), &dollars, Qty::ONE).unwrap()
            ),
            Err(CartError::Money(MoneyError::CurrencyMismatch("JOD", "USD")))
        );
        // And the honest line is added at the catalogue's price.
        let cart = add_line(building(), honest).unwrap();
        assert_eq!(cart.lines.first().unwrap().unit_price, jod(1_250));
    }

    #[test]
    fn inactive_product_cannot_be_added_but_can_be_refunded() {
        let mut item = product(0x20, UnitOfMeasure::Each);
        let sold = add(building(), 1, &item, 1_000).unwrap();
        // Deactivated with stock remaining (E.38): it can no longer be added,
        // by its product or by a scan of its barcode.
        item.is_active = false;
        assert_eq!(
            AddLine::from_product(line_id(2), &item, Qty::ONE),
            Err(CartError::ProductInactive(item.id))
        );
        let scanned = ScanLookup::Product {
            hit: ProductHit {
                product: item.clone(),
                matched_on: MatchedOn::Plu("12345".to_owned()),
            },
            pack_qty: Qty::ONE,
        };
        assert_eq!(
            AddLine::from_scan(line_id(2), scanned),
            Err(CartError::ProductInactive(item.id))
        );
        // But only adding is refused. The line already captured is what a
        // refund reads (I-5, phase 2's refund), and nothing re-reads the
        // product: it keeps its price and can still be changed in its cart.
        let still = set_qty(sold, line_id(1), Qty::from_milli(2_000)).unwrap();
        assert_eq!(still.lines.first().unwrap().unit_price, jod(1_250));
    }

    #[test]
    fn age_restricted_line_requires_confirmation() {
        let mut tobacco = product(0x30, UnitOfMeasure::Package);
        tobacco.min_age = Some(18);
        let plain = product(0x20, UnitOfMeasure::Each);
        let cart = add(building(), 1, &plain, 1_000).unwrap();
        assert_eq!(check_age_confirmed(&cart), Ok(()));
        // The line goes in, visibly blocked, with its exact pack and quantity,
        // and nothing may be tendered until an operator confirms the age
        // (1.4.3b). It is not dropped, which would invite a second scan.
        let cart = add(cart, 2, &tobacco, 1_000).unwrap();
        let line = cart.lines.iter().find(|l| l.id == line_id(2)).unwrap();
        assert_eq!((line.min_age, line.age_confirmed), (Some(18), false));
        assert_eq!(
            check_age_confirmed(&cart),
            Err(CartError::AgeConfirmationRequired(tobacco.id))
        );
        // Changing the quantity does not confirm it.
        let cart = set_qty(cart, line_id(2), Qty::from_milli(2_000)).unwrap();
        assert_eq!(
            check_age_confirmed(&cart),
            Err(CartError::AgeConfirmationRequired(tobacco.id))
        );
    }

    #[test]
    fn set_qty_zero_is_rejected() {
        let item = product(0x20, UnitOfMeasure::Each);
        let cart = add(building(), 1, &item, 1_000).unwrap();
        for milli in [0, -1_000] {
            assert_eq!(
                set_qty(cart.clone(), line_id(1), Qty::from_milli(milli)),
                Err(CartError::NonPositiveQty),
                "removing a line is a void, which is audited"
            );
        }
        assert_eq!(
            set_qty(cart.clone(), line_id(9), Qty::ONE),
            Err(CartError::LineNotFound(line_id(9)))
        );
        assert_eq!(add(building(), 1, &item, 0), Err(CartError::NonPositiveQty));
    }

    #[test]
    fn void_line_removes_it_and_says_who_and_why() {
        let item = product(0x20, UnitOfMeasure::Each);
        let cart = add(add(building(), 1, &item, 2_000).unwrap(), 2, &item, 1_000).unwrap();
        let cashier = UserId::from_uuid(id(4));
        let at = Timestamp::from_epoch_milliseconds(978_307_260_000).unwrap();
        let (cart, intent) = void_line(
            cart,
            line_id(1),
            VoidReason("mis_scan".to_owned()),
            cashier,
            at,
        )
        .unwrap();
        assert_eq!(
            cart.lines.iter().map(|l| l.id).collect::<Vec<_>>(),
            vec![line_id(2)]
        );
        assert_eq!(
            (intent.actor, intent.approver, intent.action, intent.entity),
            (cashier, None, "line.void", "sale_line")
        );
        assert_eq!(intent.entity_id, line_id(1).as_uuid());
        assert_eq!(intent.reason.as_deref(), Some("mis_scan"));
        assert_eq!(intent.at, at);
        assert_eq!(
            intent.payload,
            serde_json::json!({
                "sale_id": "00000000-0000-0000-0000-000000000001",
                "product_id": item.id.as_uuid().to_string(),
                "qty_milli": 2_000,
                "unit_price_minor": 1_250,
                "currency": "JOD",
            })
        );
        // The payload names no line text: not needed to identify it.
        assert!(!intent.payload.to_string().contains(&item.name_ar));
        assert_eq!(
            void_line(
                cart,
                line_id(1),
                VoidReason("mis_scan".to_owned()),
                cashier,
                at
            )
            .map(|_| ()),
            Err(CartError::LineNotFound(line_id(1)))
        );
    }

    /// The price layout of `catalog::scan`'s tests: `2`, five digits of item
    /// code, a digit this rule does not read, five digits of price in fils,
    /// and the EAN-13 check digit.
    fn price_rule() -> EmbeddedBarcodeRule {
        EmbeddedBarcodeRule {
            prefix: "2".to_owned(),
            item_code_span: (1, 5),
            value_span: (7, 5),
            value_kind: EmbeddedValue::PriceMinor,
            value_scale: 1,
            verify_checksum: true,
        }
    }

    fn price_label(item: &str, price_minor: i64) -> String {
        let first_twelve = format!("2{item}7{price_minor:05}");
        let sum: u32 = first_twelve
            .chars()
            .rev()
            .enumerate()
            .map(|(i, c)| c.to_digit(10).unwrap() * if i % 2 == 0 { 3 } else { 1 })
            .sum();
        format!("{first_twelve}{}", (10 - sum % 10) % 10)
    }

    /// 1.2.4's last test, which waited for this constructor: a price label's
    /// line is one package at the label's price, and the weight it implies is
    /// the stock basis, never the quantity.
    #[test]
    fn price_embedded_line_is_one_unit_not_a_weight() {
        let mut cheese = product(0xC4, UnitOfMeasure::Kilogram);
        cheese.unit_price = jod(8_500);
        let code = price_label("12345", 416);
        let parsed = parse_scan(&code, &[price_rule()]).unwrap();
        let hit = ProductHit {
            product: cheese.clone(),
            matched_on: MatchedOn::Plu("12345".to_owned()),
        };
        let lookup = resolve_scan(parsed, vec![hit], JOD).unwrap();
        let cart = add_line(building(), AddLine::from_scan(line_id(1), lookup).unwrap()).unwrap();
        let line = cart.lines.first().unwrap();
        assert_eq!(line.qty, Qty::ONE);
        assert_eq!(
            (line.unit, line.is_weighed),
            (UnitOfMeasure::Package, false)
        );
        assert_eq!(line.unit_price, jod(416));
        assert_eq!(
            line.price_origin,
            PriceOrigin::Label {
                rule_prefix: "2".to_owned(),
                raw_code: code,
            }
        );
        // 416 fils at 8.500 JOD a kilogram is 48.94 g, kept as the stock basis.
        assert_eq!(
            line.label,
            Some(DerivedWeight {
                qty: Qty::from_milli(49),
                catalog_unit_price: jod(8_500),
            })
        );
        // One package is what the label was printed for; another is a scan.
        assert_eq!(
            set_qty(cart, line_id(1), Qty::from_milli(2_000)),
            Err(CartError::LabelLineIsOnePackage(line_id(1)))
        );
    }

    #[test]
    fn a_scan_adds_its_code_s_pack_quantity_or_its_label_s_weight() {
        let cola = product(0x40, UnitOfMeasure::Each);
        let six_pack = ScanLookup::Product {
            hit: ProductHit {
                product: cola.clone(),
                matched_on: MatchedOn::Plu("12345".to_owned()),
            },
            pack_qty: Qty::from_milli(6_000),
        };
        let line = AddLine::from_scan(line_id(1), six_pack).unwrap();
        assert_eq!(
            (line.qty, line.price.amount()),
            (Qty::from_milli(6_000), jod(1_250))
        );
        let mut cheese = product(0xC4, UnitOfMeasure::Kilogram);
        cheese.unit_price = jod(8_500);
        let weighed = ScanLookup::WeightEmbedded {
            hit: ProductHit {
                product: cheese,
                matched_on: MatchedOn::Plu("12345".to_owned()),
            },
            qty: Qty::from_milli(1_250),
        };
        let line = AddLine::from_scan(line_id(2), weighed).unwrap();
        assert_eq!(
            (line.qty, line.unit, line.is_weighed),
            (Qty::from_milli(1_250), UnitOfMeasure::Kilogram, true)
        );
        assert_eq!(line.price.amount(), jod(8_500));
        for no_line in [
            ScanLookup::Unknown {
                code: "1".to_owned(),
            },
            ScanLookup::ChecksumFailed {
                code: "1".to_owned(),
            },
            ScanLookup::Ambiguous { hits: Vec::new() },
        ] {
            assert_eq!(
                AddLine::from_scan(line_id(3), no_line),
                Err(CartError::ScanNotOneProduct)
            );
        }
    }

    /// Every unit, and a quantity anywhere from a negative thousand units to a
    /// thousand, fractions included. It excludes nothing: the refusals are
    /// what is checked.
    fn units() -> impl Strategy<Value = UnitOfMeasure> {
        prop_oneof![
            Just(UnitOfMeasure::Each),
            Just(UnitOfMeasure::Package),
            Just(UnitOfMeasure::Kilogram),
            Just(UnitOfMeasure::Litre),
            Just(UnitOfMeasure::Metre),
        ]
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// A line of something counted rather than measured carries a whole
        /// number of units, whichever way it was entered: by product, by a
        /// barcode's pack quantity, by a price label, or by changing its
        /// quantity afterwards. A fraction of one is refused by name, and a
        /// quantity of zero or less is refused too.
        #[test]
        fn prop_discrete_products_never_carry_a_fractional_quantity(
            unit in units(),
            first in -1_000_000i64..=1_000_000,
            later in -1_000_000i64..=1_000_000,
            pack in -1_000_000i64..=1_000_000,
        ) {
            let item = product(0x50, unit);
            let expect = |milli: i64| {
                if milli <= 0 {
                    Err(CartError::NonPositiveQty)
                } else if !unit.is_divisible() && milli % 1_000 != 0 {
                    Err(CartError::FractionalQtyForDiscrete(item.id, milli))
                } else {
                    Ok(())
                }
            };
            let added = add(building(), 1, &item, first);
            let want = expect(first);
            prop_assert_eq!(added.as_ref().err(), want.as_ref().err());
            if let Ok(cart) = added {
                let changed = set_qty(cart, line_id(1), Qty::from_milli(later));
                let want = expect(later);
                prop_assert_eq!(changed.as_ref().err(), want.as_ref().err());
            }
            let scanned = ScanLookup::Product {
                hit: ProductHit { product: item.clone(), matched_on: MatchedOn::Plu("1".to_owned()) },
                pack_qty: Qty::from_milli(pack),
            };
            let by_scan = add_line(building(), AddLine::from_scan(line_id(2), scanned).unwrap());
            let want = expect(pack);
            prop_assert_eq!(by_scan.as_ref().err(), want.as_ref().err());
            // And every line that did get in is whole, where it must be.
            for cart in [by_scan.ok()].into_iter().flatten() {
                for line in &cart.lines {
                    prop_assert!(line.unit.is_divisible() || line.qty.is_whole_units());
                }
            }
        }
    }

    // ── age confirmation (1.4.3b) ───────────────────────────────────────

    /// A sealed pack of tobacco, which the law sells only to the over-18s.
    fn tobacco() -> Product {
        let mut pack = product(0x30, UnitOfMeasure::Package);
        pack.min_age = Some(18);
        pack.regulated_sale_form = RegulatedSaleForm::Tobacco;
        pack
    }

    /// A cart with an ordinary line (1) and a tobacco pack (2), still blocked.
    fn with_tobacco() -> Cart {
        let plain = product(0x20, UnitOfMeasure::Each);
        add(
            add(building(), 1, &plain, 1_000).unwrap(),
            2,
            &tobacco(),
            1_000,
        )
        .unwrap()
    }

    /// A shift lead, not the cart's cashier: the intent must name whoever
    /// actually answered the gate.
    fn operator() -> UserId {
        UserId::from_uuid(id(5))
    }

    fn moment() -> Timestamp {
        Timestamp::from_epoch_milliseconds(978_307_320_000).unwrap()
    }

    #[test]
    fn age_restricted_line_blocks_tender_until_confirmation() {
        let cart = with_tobacco();
        // What 1.4.8's `begin_tender` asks before anything is collected.
        assert_eq!(
            check_age_confirmed(&cart),
            Err(CartError::AgeConfirmationRequired(tobacco().id))
        );
        let (cart, intent) = confirm_age(cart, line_id(2), true, operator(), moment()).unwrap();
        assert_eq!(check_age_confirmed(&cart), Ok(()));
        // It is the confirmation, by a named operator, that opened the gate.
        assert_eq!((intent.actor, intent.action), (operator(), "age.confirm"));
        let line = cart.lines.iter().find(|l| l.id == line_id(2)).unwrap();
        assert!(line.age_confirmed);
        assert_eq!(cart.lines.len(), 2);
    }

    #[test]
    fn age_confirmation_is_audited() {
        let (_, intent) =
            confirm_age(with_tobacco(), line_id(2), true, operator(), moment()).unwrap();
        assert_eq!(intent.actor, operator());
        assert_eq!((intent.approver, intent.approval), (None, None));
        assert_eq!((intent.action, intent.entity), ("age.confirm", "sale_line"));
        assert_eq!(intent.entity_id, line_id(2).as_uuid());
        assert_eq!(intent.at, moment());
        assert_eq!(
            intent.payload,
            serde_json::json!({
                "sale_id": "00000000-0000-0000-0000-000000000001",
                "product_id": tobacco().id.as_uuid().to_string(),
                "min_age": 18,
                "confirmed": true,
                "qty_milli": 1_000,
                "unit_price_minor": 1_250,
                "currency": "JOD",
            })
        );
        // Nothing about the customer: the gate records that it was checked,
        // not who was checked.
        assert_eq!(intent.reason, None);
    }

    #[test]
    fn age_confirmation_for_a_different_line_is_refused() {
        let cart = with_tobacco();
        // The ordinary line has no age gate: confirming it confirms nothing,
        // and certainly not the tobacco beside it.
        assert_eq!(
            confirm_age(cart.clone(), line_id(1), true, operator(), moment()).map(|_| ()),
            Err(CartError::AgeConfirmationNotPending(line_id(1)))
        );
        assert_eq!(
            confirm_age(cart.clone(), line_id(9), true, operator(), moment()).map(|_| ()),
            Err(CartError::LineNotFound(line_id(9)))
        );
        assert_eq!(
            check_age_confirmed(&cart),
            Err(CartError::AgeConfirmationRequired(tobacco().id))
        );
        // A second answer for a line already confirmed would be a second audit
        // row for one decision.
        let (confirmed, _) = confirm_age(cart, line_id(2), true, operator(), moment()).unwrap();
        assert_eq!(
            confirm_age(confirmed, line_id(2), false, operator(), moment()).map(|_| ()),
            Err(CartError::AgeConfirmationNotPending(line_id(2)))
        );
    }

    #[test]
    fn age_decline_removes_line_and_audits() {
        let (cart, intent) =
            confirm_age(with_tobacco(), line_id(2), false, operator(), moment()).unwrap();
        assert_eq!(
            cart.lines.iter().map(|l| l.id).collect::<Vec<_>>(),
            vec![line_id(1)]
        );
        assert_eq!(check_age_confirmed(&cart), Ok(()));
        assert_eq!((intent.actor, intent.action), (operator(), "age.decline"));
        assert_eq!(intent.entity_id, line_id(2).as_uuid());
        assert_eq!(
            intent.payload.get("confirmed"),
            Some(&serde_json::json!(false))
        );
        assert_eq!(intent.payload.get("min_age"), Some(&serde_json::json!(18)));
    }

    #[test]
    fn unconfirmed_age_line_survives_cart_rerender() {
        // The screen renders the cart from its serialized form and sends back
        // only the line's id; the blocked line must be there, under the same
        // id, every time it is redrawn.
        let mut cart = with_tobacco();
        for _ in 0..3 {
            let rendered = serde_json::to_string(&Sale::Building(cart)).unwrap();
            let back = match serde_json::from_str::<Sale>(&rendered).unwrap() {
                Sale::Building(back) => Some(back),
                Sale::Parked(_) => None,
            };
            cart = back.expect("a building cart renders as a building cart");
            let line = cart.lines.iter().find(|l| l.id == line_id(2)).unwrap();
            assert_eq!((line.min_age, line.age_confirmed), (Some(18), false));
        }
        let (cart, _) = confirm_age(cart, line_id(2), true, operator(), moment()).unwrap();
        assert_eq!(check_age_confirmed(&cart), Ok(()));
    }

    // ── park and resume (1.4.4) ─────────────────────────────────────────

    use crate::money::Percent;
    use crate::pricing::DiscountReason;
    use crate::tax::ZeroRatingReason;

    /// The till a cart was opened at, which is where it resumes as it was.
    fn till_of(cart: &Cart) -> CartContext {
        CartContext {
            register_id: cart.register_id,
            shift_id: cart.shift_id,
            cashier_id: cart.cashier_id,
            currency: cart.currency,
            is_training: cart.is_training,
            opened_at: cart.opened_at,
        }
    }

    /// The same register after a shift handover: another shift, another
    /// cashier, a later moment.
    fn next_shift() -> CartContext {
        CartContext {
            shift_id: ShiftId::from_uuid(id(6)),
            cashier_id: UserId::from_uuid(id(7)),
            opened_at: Timestamp::from_epoch_milliseconds(978_336_000_000).unwrap(),
            ..context(false)
        }
    }

    fn uuids() -> impl Strategy<Value = Uuid> {
        any::<u128>().prop_map(Uuid::from_u128)
    }

    fn currencies() -> impl Strategy<Value = Currency> {
        prop_oneof![
            Just(Currency::JOD),
            Just(Currency::USD),
            Just(Currency::EUR)
        ]
    }

    /// Any line, including the ones no transition makes — a department line, a
    /// price in another currency, a quantity of zero — because parking keeps
    /// whatever it is given and checks nothing it does not have to.
    fn lines() -> impl Strategy<Value = CartLine> {
        let origin = prop_oneof![
            uuids().prop_map(|u| PriceOrigin::Catalog {
                product_id: ProductId::from_uuid(u)
            }),
            (any::<String>(), any::<String>()).prop_map(|(rule_prefix, raw_code)| {
                PriceOrigin::Label {
                    rule_prefix,
                    raw_code,
                }
            }),
            uuids().prop_map(|u| PriceOrigin::Department {
                tax_category_id: TaxCategoryId::from_uuid(u)
            }),
        ];
        let discount = proptest::option::of(
            (
                prop_oneof![
                    any::<i64>().prop_map(|ppm| DiscountKind::Percent(Percent::from_ppm(ppm))),
                    (any::<i64>(), currencies())
                        .prop_map(|(minor, c)| DiscountKind::Amount(Money::from_minor(minor, c))),
                ],
                any::<String>(),
                uuids(),
                proptest::option::of(uuids()),
            )
                .prop_map(|(kind, reason, by, approver)| LineDiscount {
                    kind,
                    reason: DiscountReason(reason),
                    by: UserId::from_uuid(by),
                    approver: approver.map(UserId::from_uuid),
                }),
        );
        let label = proptest::option::of((any::<i64>(), any::<i64>(), currencies()).prop_map(
            |(qty, price, currency)| DerivedWeight {
                qty: Qty::from_milli(qty),
                catalog_unit_price: Money::from_minor(price, currency),
            },
        ));
        let ceiling = proptest::option::of(
            (any::<i64>(), currencies()).prop_map(|(minor, c)| Money::from_minor(minor, c)),
        );
        // Any code, `displayed_price` spelled by hand among them, which reads
        // back as the dedicated variant and is equal to it.
        let price_override = proptest::option::of(
            (
                (any::<i64>(), currencies()),
                prop_oneof![
                    Just(OverrideReason::DisplayedPrice),
                    Just(OverrideReason::Code("displayed_price".to_owned())),
                    any::<String>().prop_map(OverrideReason::Code),
                ],
                uuids(),
                proptest::option::of(uuids()),
            )
                .prop_map(|((minor, c), reason, by, approver)| PriceOverride {
                    to: Money::from_minor(minor, c),
                    reason,
                    by: UserId::from_uuid(by),
                    approver: approver.map(UserId::from_uuid),
                }),
        );
        (
            (uuids(), proptest::option::of(uuids()), any::<String>()),
            (any::<i64>(), currencies(), origin, ceiling),
            (any::<i64>(), units(), any::<bool>(), uuids()),
            (
                proptest::option::of(any::<u8>()),
                any::<bool>(),
                label,
                discount,
                price_override,
            ),
        )
            .prop_map(
                |(
                    (line, product, name),
                    (price, currency, price_origin, max_price),
                    (qty, unit, is_weighed, tax),
                    (min_age, age_confirmed, label, discount, price_override),
                )| CartLine {
                    id: SaleLineId::from_uuid(line),
                    product_id: product.map(ProductId::from_uuid),
                    name_snapshot: name,
                    unit_price: Money::from_minor(price, currency),
                    price_origin,
                    max_price,
                    qty: Qty::from_milli(qty),
                    unit,
                    is_weighed,
                    tax_category_id: TaxCategoryId::from_uuid(tax),
                    discount,
                    price_override,
                    min_age,
                    age_confirmed,
                    label,
                },
            )
    }

    /// Any cart of up to eight lines, the empty one included, with or without
    /// a customer and a buyer, in any supply and either mode, opened at any
    /// moment a `Timestamp` can hold.
    fn carts() -> impl Strategy<Value = Cart> {
        let supply = (
            prop_oneof![
                Just(SupplyDestination::Domestic),
                Just(SupplyDestination::Export),
                Just(SupplyDestination::FreeZone),
                Just(SupplyDestination::DevelopmentArea),
                Just(SupplyDestination::EligibleBody),
            ],
            proptest::option::of(prop_oneof![
                Just(ZeroRatingReason::Export),
                Just(ZeroRatingReason::FreeZoneSupply),
                Just(ZeroRatingReason::EligibleEntity),
                Just(ZeroRatingReason::ProductCategory),
            ]),
            proptest::option::of(any::<String>()),
        )
            .prop_map(|(destination, reason, evidence_ref)| SupplyTaxContext {
                destination,
                reason,
                evidence_ref,
            });
        let basket = proptest::option::of(
            (
                prop_oneof![
                    any::<i64>().prop_map(|ppm| DiscountKind::Percent(Percent::from_ppm(ppm))),
                    (any::<i64>(), currencies())
                        .prop_map(|(minor, c)| DiscountKind::Amount(Money::from_minor(minor, c))),
                ],
                any::<String>(),
                uuids(),
                proptest::option::of(uuids()),
            )
                .prop_map(|(kind, reason, by, approver)| BasketDiscount {
                    kind,
                    reason: DiscountReason(reason),
                    by: UserId::from_uuid(by),
                    approver: approver.map(UserId::from_uuid),
                }),
        );
        (
            (uuids(), uuids(), uuids(), uuids()),
            (
                currencies(),
                proptest::collection::vec(lines(), 0..=8),
                basket,
            ),
            (
                proptest::option::of(uuids()),
                proptest::option::of(any::<String>()),
                proptest::option::of(any::<String>()),
            ),
            (
                supply,
                any::<bool>(),
                -377_705_023_201_000i64..=253_402_207_200_000,
            ),
        )
            .prop_map(
                |(
                    (sale, register, shift, cashier),
                    (currency, lines, basket_discount),
                    (customer, buyer_tin, buyer_name),
                    (supply, is_training, opened_at),
                )| Cart {
                    id: SaleId::from_uuid(sale),
                    register_id: RegisterId::from_uuid(register),
                    shift_id: ShiftId::from_uuid(shift),
                    cashier_id: UserId::from_uuid(cashier),
                    currency,
                    lines,
                    basket_discount,
                    customer_id: customer.map(CustomerId::from_uuid),
                    buyer_tin,
                    buyer_name,
                    supply,
                    is_training,
                    opened_at: Timestamp::from_epoch_milliseconds(opened_at).unwrap(),
                },
            )
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// A cart parked and resumed at the till it was parked at comes back
        /// exactly as it was — every line with its name, price, quantity and
        /// age gate, the buyer, the supply and the moment it was opened —
        /// whether it is resumed at once or read back from the form the
        /// register keeps it in across a restart (E.3). Parking adds nothing to
        /// that form, and the one cart it refuses is a cart with no line.
        #[test]
        fn prop_park_resume_roundtrip_is_identity(cart in carts()) {
            let till = till_of(&cart);
            let parked = match park(cart.clone()) {
                Ok(parked) => parked,
                Err(refused) => {
                    prop_assert!(cart.lines.is_empty());
                    prop_assert_eq!(refused, CartError::NothingToPark);
                    return Ok(());
                }
            };
            prop_assert!(!cart.lines.is_empty());
            prop_assert_eq!(parked.cart(), &cart);
            prop_assert_eq!(
                serde_json::to_string(&parked).unwrap(),
                serde_json::to_string(&cart).unwrap()
            );
            let stored = serde_json::to_string(&Sale::Parked(parked.clone())).unwrap();
            let restored = match serde_json::from_str::<Sale>(&stored).unwrap() {
                Sale::Parked(restored) => Some(restored),
                Sale::Building(_) => None,
            };
            prop_assert_eq!(restored.as_ref(), Some(&parked));
            let resumed = resume(parked, &till);
            prop_assert_eq!(resumed.as_ref(), Ok(&cart));
            if let Some(restored) = restored {
                let resumed = resume(restored, &till);
                prop_assert_eq!(resumed.as_ref(), Ok(&cart));
            }
        }
    }

    #[test]
    fn an_empty_cart_cannot_be_parked() {
        assert_eq!(park(building()), Err(CartError::NothingToPark));
        // Nor one whose last line was voided: there is no basket to come back
        // to.
        let item = product(0x20, UnitOfMeasure::Each);
        let (emptied, _) = void_line(
            add(building(), 1, &item, 1_000).unwrap(),
            line_id(1),
            VoidReason("mis_scan".to_owned()),
            UserId::from_uuid(id(4)),
            moment(),
        )
        .unwrap();
        assert_eq!(park(emptied), Err(CartError::NothingToPark));
        assert_eq!(
            CartError::NothingToPark.to_string(),
            "an empty cart cannot be parked"
        );
    }

    #[test]
    fn a_cart_parked_before_a_shift_change_is_paid_for_in_the_next_one() {
        let cart = with_tobacco();
        let resumed = resume(park(cart.clone()).unwrap(), &next_shift()).unwrap();
        // Rung up in the shift that takes the money, by the cashier taking it,
        // and otherwise the cart that was parked: its lines at the prices they
        // were captured at, and the moment it was first opened, not the
        // moment it was resumed.
        assert_eq!(
            resumed,
            Cart {
                shift_id: next_shift().shift_id,
                cashier_id: next_shift().cashier_id,
                ..cart
            }
        );
        // Parking confirms no age: the tobacco is still waiting.
        assert_eq!(
            check_age_confirmed(&resumed),
            Err(CartError::AgeConfirmationRequired(tobacco().id))
        );
    }

    #[test]
    fn a_parked_cart_resumes_only_at_its_own_till_in_its_own_mode() {
        let live = with_tobacco();
        let sale = live.id;
        let parked = park(live).unwrap();
        // The customer is standing at the till the cart was parked at.
        let elsewhere = CartContext {
            register_id: RegisterId::from_uuid(id(8)),
            ..next_shift()
        };
        let refused = resume(parked.clone(), &elsewhere);
        assert_eq!(refused, Err(CartError::ParkedOnAnotherRegister(sale)));
        assert_eq!(
            refused.unwrap_err().to_string(),
            format!("sale {sale} was parked on another register")
        );
        // A real sale never resumes as training, which every report and the
        // fiscal queue would leave out ...
        let training = CartContext {
            is_training: true,
            ..next_shift()
        };
        let refused = resume(parked.clone(), &training);
        assert_eq!(refused, Err(CartError::TrainingModeMismatch(sale, false)));
        assert_eq!(
            refused.unwrap_err().to_string(),
            format!("sale {sale} was parked live and this till is in training mode")
        );
        // ... nor a training cart as a real sale.
        let practice = SaleId::from_uuid(id(9));
        let item = product(0x20, UnitOfMeasure::Each);
        let rehearsed = add(open(practice, &context(true)), 1, &item, 1_000).unwrap();
        let refused = resume(park(rehearsed).unwrap(), &next_shift());
        assert_eq!(
            refused,
            Err(CartError::TrainingModeMismatch(practice, true))
        );
        assert_eq!(
            refused.unwrap_err().to_string(),
            format!("sale {practice} was parked in training mode and this till is live")
        );
        // Its prices are in dinars; a till in dollars is not its till.
        let dollars = CartContext {
            currency: Currency::USD,
            ..next_shift()
        };
        assert_eq!(
            resume(parked.clone(), &dollars),
            Err(CartError::Money(MoneyError::CurrencyMismatch("JOD", "USD")))
        );
        // At its own till, in its own mode, it resumes.
        assert!(resume(parked, &next_shift()).is_ok());
    }

    // ── manual line discounts (1.4.5) ───────────────────────────────────

    use crate::ids::ApprovalId;
    use crate::permissions::{EscalationPolicy, GrantSet, Role, authorize};

    const RULE: RoundingRule = RoundingRule::HalfAwayFromZero;

    /// The cashier's cap, merchant decision 3.1.
    fn five_percent() -> DiscountCap {
        DiscountCap::AtMost(Percent::from_ppm(50_000))
    }

    fn cashier() -> UserId {
        UserId::from_uuid(id(4))
    }

    fn manager() -> UserId {
        UserId::from_uuid(id(0x55))
    }

    /// `who`'s proof of `discount.manual`, under the shipped grid for `role`,
    /// checked at `at`.
    fn proof(who: UserId, role: Role, at: Timestamp) -> Authorized<cap::DiscountManual> {
        proof_of(who, role, at)
    }

    /// `who`'s proof of `C`, under the shipped grid for `role`, checked at `at`,
    /// where the merchant's policy escalates nothing.
    fn proof_of<C: Capability>(who: UserId, role: Role, at: Timestamp) -> Authorized<C> {
        let unbound = ApprovalBinding {
            entity_id: Uuid::nil(),
            amount_minor: 0,
            content_hash: None,
        };
        authorize(
            who,
            &GrantSet::of_role(role),
            None,
            &unbound,
            &EscalationPolicy::empty(),
            at,
        )
        .unwrap()
    }

    /// A manager's approval for the cashier: `amount_minor` off line `line`,
    /// for `reason`, valid for two minutes from `moment()`.
    fn approval(line: SaleLineId, amount_minor: i64, reason: &str) -> ApprovalHandle {
        approval_for(line.as_uuid(), amount_minor, reason)
    }

    /// As [`approval`], for any entity: a line, or the sale itself (1.4.6).
    fn approval_for(entity: Uuid, amount_minor: i64, reason: &str) -> ApprovalHandle {
        let handle = ApprovalId::from_uuid(id(0x700));
        let binding = ApprovalBinding {
            entity_id: entity,
            amount_minor,
            content_hash: None,
        };
        ApprovalHandle::issue(
            handle,
            cashier(),
            &proof(manager(), Role::Manager, moment()),
            &binding,
            reason.to_owned(),
            moment(),
            120_000,
            // Derived from the handle's id, not written as a literal: CodeQL
            // reads a fixed array passed as a nonce as a hard-coded one.
            *handle.as_uuid().as_bytes(),
        )
        .unwrap()
    }

    fn ask(kind: DiscountKind, reason: &str) -> DiscountRequest {
        DiscountRequest {
            kind,
            reason: DiscountReason(reason.to_owned()),
        }
    }

    fn pct(ppm: i64) -> DiscountKind {
        DiscountKind::Percent(Percent::from_ppm(ppm))
    }

    fn off(minor: i64) -> DiscountKind {
        DiscountKind::Amount(jod(minor))
    }

    /// Two units at 1.250: a line worth 2.500.
    fn two_teas() -> Cart {
        add(building(), 1, &product(0x20, UnitOfMeasure::Each), 2_000).unwrap()
    }

    /// The cashier, within their own cap and with no approval.
    fn discount(cart: Cart, kind: DiscountKind) -> Result<(Cart, AuditIntent), CartError> {
        apply_line_discount(
            cart,
            line_id(1),
            ask(kind, "loyal_customer"),
            five_percent(),
            RULE,
            &proof(cashier(), Role::Cashier, moment()),
            None,
        )
    }

    #[test]
    fn discount_above_role_cap_is_denied() {
        // 5% of 2.500 is 0.125: up to it, the cashier needs nobody.
        for within in [pct(50_000), off(125)] {
            let (cart, intent) = discount(two_teas(), within).unwrap();
            assert_eq!(
                cart.lines.first().unwrap().discount.as_ref().unwrap().kind,
                within
            );
            assert_eq!((intent.approver, intent.approval), (None, None));
        }
        // A fil past it, as a share or as an amount, needs a manager.
        assert_eq!(
            discount(two_teas(), pct(50_001)).map(|_| ()),
            Err(CartError::DiscountAboveCap(
                "5.0001%".to_owned(),
                "5%".to_owned()
            ))
        );
        let refused = discount(two_teas(), off(126)).map(|_| ());
        assert_eq!(
            refused,
            Err(CartError::DiscountAboveCap(
                "0.126 JOD".to_owned(),
                "5%".to_owned()
            ))
        );
        assert_eq!(
            refused.unwrap_err().to_string(),
            "discount 0.126 JOD is above the cap 5% and needs a manager's approval"
        );
        // An approval offered for a discount within the cap is not spent on it.
        let (_, intent) = apply_line_discount(
            two_teas(),
            line_id(1),
            ask(pct(50_000), "loyal_customer"),
            five_percent(),
            RULE,
            &proof(cashier(), Role::Cashier, moment()),
            Some(&approval(line_id(1), 125, "loyal_customer")),
        )
        .unwrap();
        assert_eq!((intent.approver, intent.approval), (None, None));
        // The manager's grant holds no cap, and gives half off alone.
        let (cart, intent) = apply_line_discount(
            two_teas(),
            line_id(1),
            ask(pct(500_000), "damaged_pack"),
            DiscountCap::Unlimited,
            RULE,
            &proof(manager(), Role::Manager, moment()),
            None,
        )
        .unwrap();
        assert!(cart.lines.first().unwrap().discount.is_some());
        assert_eq!((intent.actor, intent.approver), (manager(), None));
    }

    #[test]
    fn a_discount_above_the_cap_is_given_with_a_managers_approval() {
        // 20% of 2.500 is 0.500, and the manager approved exactly that.
        let given = |handle: &ApprovalHandle, at: Timestamp| {
            apply_line_discount(
                two_teas(),
                line_id(1),
                ask(pct(200_000), "loyal_customer"),
                five_percent(),
                RULE,
                &proof(cashier(), Role::Cashier, at),
                Some(handle),
            )
        };
        let handle = approval(line_id(1), 500, "loyal_customer");
        let (cart, intent) = given(&handle, moment()).unwrap();
        let line = cart.lines.first().unwrap();
        assert_eq!(line.discount.as_ref().unwrap().approver, Some(manager()));
        assert_eq!(
            (intent.actor, intent.approver, intent.approval),
            (cashier(), Some(manager()), Some(handle.id()))
        );
        // The handle the database consumes names this line and this amount.
        assert_eq!(intent.entity_id, line_id(1).as_uuid());
        assert_eq!(
            intent.payload.get("amount_minor"),
            Some(&serde_json::json!(500))
        );
        assert_eq!(intent.reason.as_deref(), Some(handle.reason()));

        // An approval for anything else approves nothing.
        assert_eq!(
            given(&approval(line_id(1), 400, "loyal_customer"), moment()).map(|_| ()),
            Err(CartError::Permission(
                PermissionError::ApprovalAmountMismatch("400".to_owned(), "500".to_owned())
            ))
        );
        assert_eq!(
            given(&approval(line_id(2), 500, "loyal_customer"), moment()).map(|_| ()),
            Err(CartError::Permission(
                PermissionError::ApprovalEntityMismatch(line_id(2).as_uuid(), line_id(1).as_uuid())
            ))
        );
        assert_eq!(
            given(&approval(line_id(1), 500, "damaged_pack"), moment()).map(|_| ()),
            Err(CartError::ApprovalReasonMismatch(
                "damaged_pack".to_owned(),
                "loyal_customer".to_owned()
            ))
        );
        let expired = Timestamp::from_epoch_milliseconds(978_307_320_000 + 120_000).unwrap();
        assert!(matches!(
            given(&handle, expired),
            Err(CartError::Permission(PermissionError::ApprovalExpired(_)))
        ));
        // Nor does one issued to somebody else, used by the cashier.
        let lead = UserId::from_uuid(id(5));
        let theirs = ApprovalHandle::issue(
            ApprovalId::from_uuid(id(0x701)),
            lead,
            &proof(manager(), Role::Manager, moment()),
            &ApprovalBinding {
                entity_id: line_id(1).as_uuid(),
                amount_minor: 500,
                content_hash: None,
            },
            "loyal_customer".to_owned(),
            moment(),
            120_000,
            *ApprovalId::from_uuid(id(0x701)).as_uuid().as_bytes(),
        )
        .unwrap();
        assert_eq!(
            given(&theirs, moment()).map(|_| ()),
            Err(CartError::Permission(
                PermissionError::ApprovalActorMismatch(lead, cashier())
            ))
        );
    }

    #[test]
    fn a_line_discount_is_audited_with_its_amount() {
        let (_, intent) = discount(two_teas(), pct(40_000)).unwrap();
        assert_eq!(
            (intent.actor, intent.action, intent.entity),
            (cashier(), "discount.manual", "sale_line")
        );
        assert_eq!(intent.entity_id, line_id(1).as_uuid());
        assert_eq!(intent.reason.as_deref(), Some("loyal_customer"));
        assert_eq!(intent.at, moment());
        assert_eq!(
            intent.payload,
            serde_json::json!({
                "sale_id": "00000000-0000-0000-0000-000000000001",
                "product_id": "00000000-0000-0000-0000-000000000020",
                "kind": "percent",
                "percent_ppm": 40_000,
                "amount_minor": 100,
                "line_value_minor": 2_500,
                "currency": "JOD",
                "cap_ppm": 50_000,
            })
        );
        // The line's name is not needed to identify it.
        assert!(!intent.payload.to_string().contains("صنف"));
    }

    #[test]
    fn a_second_discount_replaces_the_first_and_zero_clears_it() {
        let (cart, _) = discount(two_teas(), pct(50_000)).unwrap();
        // Two discounts each within the cap do not add up past it: the second
        // replaces the first.
        let (cart, _) = discount(cart, off(125)).unwrap();
        let line = cart.lines.first().unwrap();
        assert_eq!(line.discount.as_ref().unwrap().kind, off(125));
        // A discount of zero clears it, and is audited as one.
        let (cart, intent) = discount(cart, pct(0)).unwrap();
        assert_eq!(cart.lines.first().unwrap().discount, None);
        assert_eq!(
            intent.payload.get("amount_minor"),
            Some(&serde_json::json!(0))
        );
    }

    #[test]
    fn a_discount_that_is_negative_or_exceeds_the_line_is_refused() {
        assert_eq!(
            discount(two_teas(), pct(-10_000)).map(|_| ()),
            Err(CartError::NegativeDiscount("-1%".to_owned()))
        );
        assert_eq!(
            discount(two_teas(), off(-1)).map(|_| ()),
            Err(CartError::NegativeDiscount("-0.001 JOD".to_owned()))
        );
        // Above the line's value is refused before the cap is asked (E.19).
        let everything = |kind| {
            apply_line_discount(
                two_teas(),
                line_id(1),
                ask(kind, "damaged_pack"),
                DiscountCap::Unlimited,
                RULE,
                &proof(manager(), Role::Manager, moment()),
                None,
            )
        };
        assert!(everything(pct(1_000_000)).is_ok());
        assert!(everything(off(2_500)).is_ok());
        assert_eq!(
            everything(pct(1_000_001)).map(|_| ()),
            Err(CartError::DiscountExceedsLine("100.0001%".to_owned()))
        );
        assert_eq!(
            everything(off(2_501)).map(|_| ()),
            Err(CartError::DiscountExceedsLine("2.501 JOD".to_owned()))
        );
        // Fifty dollars is 5,000 minor units, more than the line's 2,500 fils:
        // it is refused as another currency, never weighed against the line as
        // if a cent were a fil.
        assert_eq!(
            everything(DiscountKind::Amount(Money::from_minor(
                5_000,
                Currency::USD
            )))
            .map(|_| ()),
            Err(CartError::Money(MoneyError::CurrencyMismatch("JOD", "USD")))
        );
        assert_eq!(
            discount(building(), pct(10_000)).map(|_| ()),
            Err(CartError::LineNotFound(line_id(1)))
        );
        // A line already worth less than nothing takes no discount at all: a
        // share of it would be a surcharge.
        let mut odd = product(0x21, UnitOfMeasure::Each);
        odd.unit_price = jod(-100);
        let negative = add(building(), 1, &odd, 1_000).unwrap();
        assert_eq!(
            discount(negative, pct(10_000)).map(|_| ()),
            Err(CartError::NegativeLine)
        );
    }

    #[test]
    fn an_amount_off_fixes_the_line_s_quantity_until_cleared() {
        let (cart, _) = discount(two_teas(), off(125)).unwrap();
        // One tea instead of two would make 0.125 a 10% discount the cashier's
        // cap never admitted.
        assert_eq!(
            set_qty(cart.clone(), line_id(1), Qty::ONE),
            Err(CartError::AmountDiscountFixesQuantity(line_id(1)))
        );
        let (cleared, _) = discount(cart, pct(0)).unwrap();
        assert!(set_qty(cleared, line_id(1), Qty::ONE).is_ok());
        // A share off keeps its share, whatever the quantity.
        let (shared, _) = discount(two_teas(), pct(50_000)).unwrap();
        let shared = set_qty(shared, line_id(1), Qty::from_milli(5_000)).unwrap();
        let line = shared.lines.first().unwrap();
        assert_eq!(line.discount.as_ref().unwrap().kind, pct(50_000));
    }

    fn rules() -> impl Strategy<Value = RoundingRule> {
        prop_oneof![
            Just(RoundingRule::HalfAwayFromZero),
            Just(RoundingRule::HalfEven),
            Just(RoundingRule::Floor),
            Just(RoundingRule::Ceil),
        ]
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// No discount the cart accepts takes a line below nothing (E.19):
        /// whatever the price, the quantity, the share or amount asked, the
        /// cap and the rounding rule, and whatever quantity the line is then
        /// given, the money the discount takes off is between zero and the
        /// line's value. What would take more is refused by name, before any
        /// cap is asked, and so is a discount of less than nothing.
        #[test]
        fn prop_discount_never_makes_a_line_negative(
            price in 0i64..=10_000_000_000,
            milli in 1i64..=1_000_000,
            unit in units(),
            share in -100_000i64..=1_200_000,
            jitter in -3i64..=3,
            as_amount in any::<bool>(),
            cap in prop_oneof![
                Just(DiscountCap::Unlimited),
                (0i64..=1_000_000).prop_map(|ppm| DiscountCap::AtMost(Percent::from_ppm(ppm))),
            ],
            rule in rules(),
            later in 1i64..=1_000_000,
        ) {
            let mut item = product(0x60, unit);
            item.unit_price = jod(price);
            let qty = if unit.is_divisible() { milli } else { (milli + 999) / 1_000 * 1_000 };
            let cart = add(building(), 1, &item, qty).unwrap();
            // An amount near the asked share of the line's exact value, so the
            // boundary is crossed in both directions.
            let exact = i128::from(price) * i128::from(qty) / 1_000;
            let kind = if as_amount {
                let near = exact * i128::from(share) / 1_000_000 + i128::from(jitter);
                off(i64::try_from(near).unwrap())
            } else {
                pct(share)
            };
            let applied = apply_line_discount(
                cart,
                line_id(1),
                ask(kind, "loyal_customer"),
                cap,
                rule,
                &proof(manager(), Role::Manager, moment()),
                None,
            );
            if kind.is_negative() {
                prop_assert_eq!(applied.err(), Some(CartError::NegativeDiscount(kind.format())));
                return Ok(());
            }
            if exceeds_value(kind, item.unit_price, Qty::from_milli(qty)) {
                prop_assert_eq!(applied.err(), Some(CartError::DiscountExceedsLine(kind.format())));
                return Ok(());
            }
            let mut cart = match applied {
                Ok((cart, _)) => cart,
                Err(refused) => {
                    // Only the cap may refuse what fits the line.
                    prop_assert!(matches!(refused, CartError::DiscountAboveCap(..)));
                    prop_assert!(!cap.admits(kind, item.unit_price, Qty::from_milli(qty)));
                    return Ok(());
                }
            };
            let later = if unit.is_divisible() { later } else { (later + 999) / 1_000 * 1_000 };
            if let Ok(changed) = set_qty(cart.clone(), line_id(1), Qty::from_milli(later)) {
                cart = changed;
            }
            let line = cart.lines.first().unwrap();
            for priced_under in [
                RoundingRule::HalfAwayFromZero,
                RoundingRule::HalfEven,
                RoundingRule::Floor,
                RoundingRule::Ceil,
            ] {
                let gross = line.unit_price.mul_qty(line.qty, priced_under).unwrap();
                let taken = match &line.discount {
                    Some(d) => d.amount_off(gross, priced_under).unwrap(),
                    None => jod(0),
                };
                prop_assert!(taken.minor() >= 0, "{taken:?} off {gross:?}");
                prop_assert!(taken.minor() <= gross.minor(), "{taken:?} off {gross:?}");
            }
        }
    }

    // ── basket discounts (1.4.6) ────────────────────────────────────────

    use crate::pricing::{DiscountAttribution, DiscountSource};

    /// Two teas at 1.250 (line 1, 2.500) and three packs at 1.250 (line 2,
    /// 3.750): a basket worth 6.250.
    fn basket() -> Cart {
        add(two_teas(), 2, &product(0x21, UnitOfMeasure::Package), 3_000).unwrap()
    }

    /// `who`, holding `role`'s grid and cap `limit`, discounting the basket.
    fn basket_off(
        cart: Cart,
        kind: DiscountKind,
        limit: DiscountCap,
        who: UserId,
        role: Role,
        approval: Option<&ApprovalHandle>,
    ) -> Result<(Cart, AuditIntent), CartError> {
        apply_basket_discount(
            cart,
            ask(kind, "loyal_customer"),
            limit,
            RULE,
            &proof(who, role, moment()),
            approval,
        )
    }

    fn by_cashier(cart: Cart, kind: DiscountKind) -> Result<(Cart, AuditIntent), CartError> {
        basket_off(cart, kind, five_percent(), cashier(), Role::Cashier, None)
    }

    fn by_manager(cart: Cart, kind: DiscountKind) -> Result<(Cart, AuditIntent), CartError> {
        basket_off(
            cart,
            kind,
            DiscountCap::Unlimited,
            manager(),
            Role::Manager,
            None,
        )
    }

    fn sale() -> Uuid {
        SaleId::from_uuid(id(1)).as_uuid()
    }

    #[test]
    fn a_basket_discount_within_the_cap_is_given_and_audited() {
        let (cart, intent) = by_cashier(basket(), pct(50_000)).unwrap();
        assert_eq!(cart.basket_discount.as_ref().unwrap().kind, pct(50_000));
        assert_eq!(
            (intent.actor, intent.approver, intent.approval),
            (cashier(), None, None)
        );
        assert_eq!((intent.action, intent.entity), ("discount.manual", "sale"));
        assert_eq!(intent.entity_id, sale());
        assert_eq!(intent.reason.as_deref(), Some("loyal_customer"));
        // 5% of 6.250 is 312.5 fils, which the store's rule rounds to 313.
        assert_eq!(
            intent.payload,
            serde_json::json!({
                "sale_id": "00000000-0000-0000-0000-000000000001",
                "kind": "percent",
                "percent_ppm": 50_000,
                "amount_minor": 313,
                "basket_value_minor": 6_250,
                "lines": 2,
                "currency": "JOD",
                "cap_ppm": 50_000,
            })
        );
        // As an amount, the cap is the exact share: 0.312 is within 5% of
        // 6.250 and 0.313 is not, although 5% asked as a share comes to 0.313.
        assert!(by_cashier(basket(), off(312)).is_ok());
        assert_eq!(
            by_cashier(basket(), off(313)).map(|_| ()),
            Err(CartError::DiscountAboveCap(
                "0.313 JOD".to_owned(),
                "5%".to_owned()
            ))
        );
    }

    #[test]
    fn a_basket_discount_above_the_cap_needs_an_approval_bound_to_the_sale() {
        // 20% of 6.250 is 1.250.
        assert_eq!(
            by_cashier(basket(), pct(200_000)).map(|_| ()),
            Err(CartError::DiscountAboveCap(
                "20%".to_owned(),
                "5%".to_owned()
            ))
        );
        let handle = approval_for(sale(), 1_250, "loyal_customer");
        let approved = |handle: &ApprovalHandle| {
            basket_off(
                basket(),
                pct(200_000),
                five_percent(),
                cashier(),
                Role::Cashier,
                Some(handle),
            )
        };
        let (cart, intent) = approved(&handle).unwrap();
        assert_eq!(
            cart.basket_discount.as_ref().unwrap().approver,
            Some(manager())
        );
        assert_eq!(
            (intent.approver, intent.approval, intent.entity_id),
            (Some(manager()), Some(handle.id()), sale())
        );
        assert_eq!(
            intent.payload.get("amount_minor"),
            Some(&serde_json::json!(1_250))
        );
        // An approval for one line is not an approval for the basket.
        assert_eq!(
            approved(&approval(line_id(1), 1_250, "loyal_customer")).map(|_| ()),
            Err(CartError::Permission(
                PermissionError::ApprovalEntityMismatch(line_id(1).as_uuid(), sale())
            ))
        );
        assert_eq!(
            approved(&approval_for(sale(), 1_000, "loyal_customer")).map(|_| ()),
            Err(CartError::Permission(
                PermissionError::ApprovalAmountMismatch("1000".to_owned(), "1250".to_owned())
            ))
        );
        assert_eq!(
            approved(&approval_for(sale(), 1_250, "damaged_pack")).map(|_| ()),
            Err(CartError::ApprovalReasonMismatch(
                "damaged_pack".to_owned(),
                "loyal_customer".to_owned()
            ))
        );
    }

    #[test]
    fn a_basket_is_worth_its_lines_after_their_own_discounts() {
        // 0.125 off the teas leaves a basket worth 6.125.
        let (cart, _) = discount(basket(), off(125)).unwrap();
        assert!(by_manager(cart.clone(), off(6_125)).is_ok());
        let refused = by_manager(cart.clone(), off(6_126)).map(|_| ());
        assert_eq!(
            refused,
            Err(CartError::DiscountExceedsBasket("6.126 JOD".to_owned()))
        );
        assert_eq!(
            refused.unwrap_err().to_string(),
            "discount 6.126 JOD exceeds the basket's value"
        );
        let (_, all) = by_manager(cart.clone(), pct(1_000_000)).unwrap();
        assert_eq!(
            all.payload.get("amount_minor"),
            Some(&serde_json::json!(6_125))
        );
        assert_eq!(
            by_manager(cart, pct(1_000_001)).map(|_| ()),
            Err(CartError::DiscountExceedsBasket("100.0001%".to_owned()))
        );
    }

    #[test]
    fn a_basket_discount_is_refused_by_name() {
        assert_eq!(
            by_manager(basket(), pct(-10_000)).map(|_| ()),
            Err(CartError::NegativeDiscount("-1%".to_owned()))
        );
        // Seventy dollars is 7,000 minor units, more than the basket's 6,250
        // fils: refused as another currency, never weighed as fils.
        assert_eq!(
            by_manager(
                basket(),
                DiscountKind::Amount(Money::from_minor(7_000, Currency::USD))
            )
            .map(|_| ()),
            Err(CartError::Money(MoneyError::CurrencyMismatch("JOD", "USD")))
        );
        let mut odd = product(0x22, UnitOfMeasure::Each);
        odd.unit_price = jod(-100);
        let negative = add(basket(), 3, &odd, 1_000).unwrap();
        assert_eq!(
            by_manager(negative, pct(10_000)).map(|_| ()),
            Err(CartError::NegativeLine)
        );
        // An empty basket is worth nothing: a share of it is nothing, and any
        // amount is too much.
        assert!(by_manager(building(), pct(100_000)).is_ok());
        assert_eq!(
            by_manager(building(), off(1)).map(|_| ()),
            Err(CartError::DiscountExceedsBasket("0.001 JOD".to_owned()))
        );
    }

    #[test]
    fn a_line_discount_and_a_basket_discount_each_within_the_cap_both_apply() {
        // The owner's decision: each discount is held to the cap on its own.
        let (cart, _) = discount(basket(), off(125)).unwrap();
        let (cart, _) = by_cashier(cart, pct(50_000)).unwrap();
        // 5% of 6.125 is 306.25 fils, 306 once rounded. Divided by what the
        // lines are worth after their own discounts, 2.375 and 3.750, the
        // exact shares are 118.65 and 187.35: 118 and 187, and the fil left
        // over goes to the larger remainder, the teas.
        let attributions = discount_attributions(&cart, RULE).unwrap();
        let summary: Vec<_> = attributions
            .iter()
            .map(|a| (a.line_id, a.source, a.amount.minor()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (line_id(1), DiscountSource::ManualLine, 125),
                (line_id(1), DiscountSource::ManualBasket, 119),
                (line_id(2), DiscountSource::ManualBasket, 187),
            ]
        );
        let basket_share: i64 = attributions
            .iter()
            .filter(|a| a.source == DiscountSource::ManualBasket)
            .map(|a| a.amount.minor())
            .sum();
        assert_eq!(basket_share, 306);
        // Who gave each, and as what, travels with it.
        let first = attributions.first().unwrap();
        assert_eq!(
            (first.by, first.approver, first.percent),
            (cashier(), None, None)
        );
        let shared = attributions.get(1).unwrap();
        assert_eq!(shared.percent, Some(Percent::from_ppm(50_000)));
    }

    #[test]
    fn a_line_worth_nothing_takes_no_share_of_the_basket() {
        let mut free = product(0x23, UnitOfMeasure::Each);
        free.unit_price = jod(0);
        let cart = add(basket(), 3, &free, 1_000).unwrap();
        let (cart, _) = by_manager(cart, off(1_000)).unwrap();
        let attributions = discount_attributions(&cart, RULE).unwrap();
        assert!(attributions.iter().all(|a| a.line_id != line_id(3)));
        let total: i64 = attributions.iter().map(|a| a.amount.minor()).sum();
        assert_eq!(total, 1_000);
        // A cart with no discount has nothing to attribute.
        assert_eq!(
            discount_attributions(&basket(), RULE).unwrap(),
            Vec::<DiscountAttribution>::new()
        );
    }

    #[test]
    fn an_amount_off_the_basket_is_cleared_when_the_basket_shrinks() {
        let with_amount = |cart: Cart| by_manager(cart, off(500)).unwrap().0;
        let kept = |cart: &Cart| cart.basket_discount.is_some();
        // Each change that lowers the basket's value clears it.
        let voided = void_line(
            with_amount(basket()),
            line_id(2),
            VoidReason("mis_scan".to_owned()),
            cashier(),
            moment(),
        )
        .unwrap()
        .0;
        assert!(!kept(&voided));
        let lowered = set_qty(with_amount(basket()), line_id(2), Qty::from_milli(2_000)).unwrap();
        assert!(!kept(&lowered));
        let discounted = discount(with_amount(basket()), pct(50_000)).unwrap().0;
        assert!(!kept(&discounted));
        let declined = confirm_age(
            with_amount(with_tobacco()),
            line_id(2),
            false,
            operator(),
            moment(),
        )
        .unwrap()
        .0;
        assert!(!kept(&declined));
        // A change that raises it keeps it: more of a line, another line, or a
        // line's discount cleared.
        let raised = set_qty(with_amount(basket()), line_id(2), Qty::from_milli(4_000)).unwrap();
        assert!(kept(&raised));
        let grown = add(
            with_amount(basket()),
            3,
            &product(0x24, UnitOfMeasure::Each),
            1_000,
        )
        .unwrap();
        assert!(kept(&grown));
        let (discounted_first, _) = discount(basket(), pct(50_000)).unwrap();
        let cleared_line = discount(with_amount(discounted_first), pct(0)).unwrap().0;
        assert!(kept(&cleared_line));
        // And a share off the basket keeps its share through all of them.
        let (shared, _) = by_manager(basket(), pct(100_000)).unwrap();
        let shared = void_line(
            shared,
            line_id(2),
            VoidReason("mis_scan".to_owned()),
            cashier(),
            moment(),
        )
        .unwrap()
        .0;
        assert_eq!(shared.basket_discount.unwrap().kind, pct(100_000));
    }

    // ── price overrides (1.4.7) ─────────────────────────────────────────

    use crate::audit::check_payload;
    use crate::catalog::{LabelReprint, ReprintCause};
    use crate::pricing::{OverrideReason, PriceOverride};

    fn shift_lead() -> UserId {
        UserId::from_uuid(id(0x56))
    }

    fn code(reason: &str) -> OverrideReason {
        OverrideReason::Code(reason.to_owned())
    }

    fn tea() -> ProductId {
        ProductId::from_uuid(id(0x20))
    }

    /// The shift lead, under the shipped grid, setting line 1 to `to`, where
    /// the product cost `cost` a unit or has no recorded cost.
    fn reprice(
        cart: Cart,
        to: i64,
        reason: OverrideReason,
        cost: Option<i64>,
    ) -> Result<(Cart, AuditIntent, Option<LabelReprint>), CartError> {
        override_price(
            cart,
            line_id(1),
            jod(to),
            reason,
            cost.map(jod),
            &proof_of(shift_lead(), Role::ShiftLead, moment()),
        )
    }

    /// The shift lead's proof where the merchant escalates `price.override`:
    /// a manager's handle for line 1 at `to`, for `reason`.
    fn escalated(to: i64, reason: &str) -> Authorized<cap::PriceOverride> {
        let handle = ApprovalId::from_uuid(id(0x701));
        let binding = ApprovalBinding {
            entity_id: line_id(1).as_uuid(),
            amount_minor: to,
            content_hash: None,
        };
        let approval = ApprovalHandle::issue(
            handle,
            shift_lead(),
            &proof_of::<cap::PriceOverride>(manager(), Role::Manager, moment()),
            &binding,
            reason.to_owned(),
            moment(),
            120_000,
            *handle.as_uuid().as_bytes(),
        )
        .unwrap();
        authorize(
            shift_lead(),
            &GrantSet::of_role(Role::ShiftLead),
            Some(&approval),
            &binding,
            &EscalationPolicy::empty().with_escalation::<cap::PriceOverride>(),
            moment(),
        )
        .unwrap()
    }

    fn price_of(cart: &Cart) -> Money {
        cart.lines.first().unwrap().price()
    }

    /// One field of an intent's payload, which must be there.
    fn field<'a>(intent: &'a AuditIntent, key: &str) -> &'a serde_json::Value {
        intent.payload.get(key).unwrap()
    }

    /// E.33: the friend price stops at what the goods cost the shop.
    #[test]
    fn override_below_floor_is_denied() {
        // Two teas sell at 1.250 and cost 1.000 a unit.
        assert_eq!(
            reprice(two_teas(), 999, code("damaged_pack"), Some(1_000)).map(|r| r.0),
            Err(CartError::BelowPriceFloor(
                "0.999".to_owned(),
                "1.000".to_owned()
            ))
        );
        let (cart, intent, reprint) =
            reprice(two_teas(), 1_000, code("damaged_pack"), Some(1_000)).unwrap();
        assert_eq!(price_of(&cart), jod(1_000));
        assert_eq!(field(&intent, "floor_minor"), 1_000);
        assert_eq!(reprint, None);
        // The floor is not a cap an approver lifts: a manager is held to it,
        // and so is a shift lead whose override a manager approved.
        for auth in [
            proof_of(manager(), Role::Manager, moment()),
            escalated(999, "damaged_pack"),
        ] {
            assert_eq!(
                override_price(
                    two_teas(),
                    line_id(1),
                    jod(999),
                    code("damaged_pack"),
                    Some(jod(1_000)),
                    &auth
                )
                .map(|r| r.0),
                Err(CartError::BelowPriceFloor(
                    "0.999".to_owned(),
                    "1.000".to_owned()
                ))
            );
        }
        // A cashier holds no `price.override` at all, so no proof can be made
        // for one, and with none the transition cannot be called.
        assert_eq!(
            authorize::<cap::PriceOverride>(
                cashier(),
                &GrantSet::of_role(Role::Cashier),
                None,
                &ApprovalBinding {
                    entity_id: line_id(1).as_uuid(),
                    amount_minor: 990,
                    content_hash: None,
                },
                &EscalationPolicy::empty(),
                moment(),
            )
            .err(),
            Some(PermissionError::Denied(cashier(), "price.override"))
        );
    }

    #[test]
    fn a_product_with_no_recorded_cost_is_never_lowered() {
        assert_eq!(
            reprice(two_teas(), 1_249, code("damaged_pack"), None).map(|r| r.0),
            Err(CartError::NoRecordedCost(tea(), "1.249".to_owned()))
        );
        // Raising it, and putting it back, need no cost.
        let (raised, ..) = reprice(two_teas(), 1_300, code("catalogue_error"), None).unwrap();
        assert_eq!(price_of(&raised), jod(1_300));
        let (back, ..) = reprice(raised, 1_250, code("catalogue_error"), None).unwrap();
        assert_eq!(back.lines.first().unwrap().price_override, None);
        // The shelf's price is honoured all the same (the owner's decision).
        let (shelf, ..) = reprice(two_teas(), 990, OverrideReason::DisplayedPrice, None).unwrap();
        assert_eq!(price_of(&shelf), jod(990));
    }

    #[test]
    fn the_floor_never_refuses_the_catalogue_s_own_price() {
        // The catalogue sells the tea at 1.250 though it cost 1.400.
        let (up, intent, _) =
            reprice(two_teas(), 1_300, code("catalogue_error"), Some(1_400)).unwrap();
        assert_eq!(price_of(&up), jod(1_300));
        assert_eq!(field(&intent, "floor_minor"), &serde_json::Value::Null);
        // Lower than the price charged, still not below the catalogue's.
        let (down, ..) = reprice(up, 1_250, code("catalogue_error"), Some(1_400)).unwrap();
        assert_eq!(down.lines.first().unwrap().price_override, None);
        assert_eq!(
            reprice(down, 1_249, code("damaged_pack"), Some(1_400)).map(|r| r.0),
            Err(CartError::BelowPriceFloor(
                "1.249".to_owned(),
                "1.400".to_owned()
            ))
        );
    }

    /// E.71: the ministry ceiling holds when a line enters and whenever its
    /// price changes, and nobody's approval lifts it.
    #[test]
    fn sale_above_ceiling_is_hard_blocked() {
        let mut staple = product(0x20, UnitOfMeasure::Each);
        // Sold at its ceiling, which is lawful: equal is not above.
        staple.max_price = Some(jod(1_250));
        let cart = add(building(), 1, &staple, 1_000).unwrap();
        assert_eq!(cart.lines.first().unwrap().max_price, Some(jod(1_250)));
        for auth in [
            proof_of(shift_lead(), Role::ShiftLead, moment()),
            proof_of(manager(), Role::Manager, moment()),
            escalated(1_251, "catalogue_error"),
        ] {
            assert_eq!(
                override_price(
                    cart.clone(),
                    line_id(1),
                    jod(1_251),
                    code("catalogue_error"),
                    Some(jod(900)),
                    &auth
                )
                .map(|r| r.0),
                Err(CartError::AboveMaxPrice(
                    "1.251".to_owned(),
                    "1.250".to_owned()
                ))
            );
        }
        // A product held above its ceiling, which no catalogue save keeps,
        // never enters a cart.
        let mut over = staple.clone();
        over.unit_price = jod(1_300);
        assert_eq!(
            add(building(), 1, &over, 1_000),
            Err(CartError::AboveMaxPrice(
                "1.300".to_owned(),
                "1.250".to_owned()
            ))
        );
        // Nor does a counted product's price label above its ceiling.
        let scan = |item: &Product, price: i64| {
            let code = price_label("12345", price);
            let hit = ProductHit {
                product: item.clone(),
                matched_on: MatchedOn::Plu("12345".to_owned()),
            };
            let lookup =
                resolve_scan(parse_scan(&code, &[price_rule()]).unwrap(), vec![hit], JOD).unwrap();
            add_line(building(), AddLine::from_scan(line_id(1), lookup).unwrap())
        };
        assert_eq!(
            scan(&staple, 1_300),
            Err(CartError::AboveMaxPrice(
                "1.300".to_owned(),
                "1.250".to_owned()
            ))
        );
        let marked_down = scan(&staple, 1_200).unwrap();
        assert_eq!(
            marked_down.lines.first().unwrap().max_price,
            Some(jod(1_250))
        );
        // A weighed product's label is a package of a weight the till does
        // not know, which no ceiling per kilogram compares with.
        let mut cheese = product(0xC4, UnitOfMeasure::Kilogram);
        cheese.unit_price = jod(8_500);
        cheese.max_price = Some(jod(9_000));
        let weighed = scan(&cheese, 9_500).unwrap();
        assert_eq!(weighed.lines.first().unwrap().max_price, None);
        // A ceiling that cannot be compared with the price is refused, not
        // passed.
        let mut dollars = staple.clone();
        dollars.max_price = Some(Money::from_minor(1_250, Currency::USD));
        assert_eq!(
            add(building(), 1, &dollars, 1_000),
            Err(CartError::Money(MoneyError::CurrencyMismatch("JOD", "USD")))
        );
    }

    /// E.70: the shelf says 0.990, the till says 1.250. The customer pays the
    /// shelf's price, and the label goes on the worklist to be printed again.
    #[test]
    fn displayed_price_override_queues_a_label_reprint() {
        // The tea cost 1.000, so the floor would refuse 0.990; the shelf wins.
        let (cart, intent, reprint) =
            reprice(two_teas(), 990, OverrideReason::DisplayedPrice, Some(1_000)).unwrap();
        assert_eq!(
            reprint,
            Some(LabelReprint {
                product_id: tea(),
                cause: ReprintCause::DisplayedPriceOverride,
            })
        );
        let line = cart.lines.first().unwrap();
        assert_eq!((line.unit_price, line.price()), (jod(1_250), jod(990)));
        assert_eq!(intent.reason.as_deref(), Some("displayed_price"));
        assert_eq!(field(&intent, "displayed_price"), true);
        assert_eq!(field(&intent, "floor_minor"), &serde_json::Value::Null);
        // The code typed from the store's list is the same reason, and does
        // the same.
        let typed = reprice(two_teas(), 990, code("displayed_price"), Some(1_000)).unwrap();
        assert_eq!((typed.1, typed.2), (intent, reprint));
        // Any other reason asks for no label.
        let (.., none) = reprice(two_teas(), 1_100, code("damaged_pack"), Some(1_000)).unwrap();
        assert_eq!(none, None);
        // A shelf price only ever lowers what the till charges.
        for not_lower in [1_250, 1_300] {
            assert_eq!(
                reprice(
                    two_teas(),
                    not_lower,
                    OverrideReason::DisplayedPrice,
                    Some(1_000)
                )
                .map(|r| r.0),
                Err(CartError::DisplayedPriceNotLower(
                    jod(not_lower).format_exact(),
                    "1.250".to_owned()
                ))
            );
        }
        // Measured against the price charged now, not the one captured.
        let (twice, ..) = reprice(cart, 950, OverrideReason::DisplayedPrice, None).unwrap();
        assert_eq!(price_of(&twice), jod(950));
        assert_eq!(
            reprice(twice, 960, OverrideReason::DisplayedPrice, None).map(|r| r.0),
            Err(CartError::DisplayedPriceNotLower(
                "0.960".to_owned(),
                "0.950".to_owned()
            ))
        );
    }

    #[test]
    fn an_override_is_audited_with_its_prices() {
        let (cart, intent, _) =
            reprice(two_teas(), 1_100, code("damaged_pack"), Some(1_000)).unwrap();
        assert_eq!(
            intent,
            AuditIntent {
                actor: shift_lead(),
                approver: None,
                approval: None,
                action: "price.override",
                entity: "sale_line",
                entity_id: line_id(1).as_uuid(),
                reason: Some("damaged_pack".to_owned()),
                payload: serde_json::json!({
                    "sale_id": "00000000-0000-0000-0000-000000000001",
                    "product_id": "00000000-0000-0000-0000-000000000020",
                    "amount_minor": 1_100,
                    "captured_minor": 1_250,
                    "previous_minor": 1_250,
                    "qty_milli": 2_000,
                    "currency": "JOD",
                    "floor_minor": 1_000,
                    "ceiling_minor": null,
                    "displayed_price": false,
                }),
                at: moment(),
            }
        );
        check_payload(&intent.payload).unwrap();
        assert_eq!(
            cart.lines.first().unwrap().price_override,
            Some(PriceOverride {
                to: jod(1_100),
                reason: code("damaged_pack"),
                by: shift_lead(),
                approver: None,
            })
        );
        // Where policy escalates `price.override`, the approver and the handle
        // ride on the proof, and the intent names both.
        let (cart, intent, _) = override_price(
            two_teas(),
            line_id(1),
            jod(1_100),
            code("damaged_pack"),
            Some(jod(1_000)),
            &escalated(1_100, "damaged_pack"),
        )
        .unwrap();
        assert_eq!(
            (intent.approver, intent.approval),
            (Some(manager()), Some(ApprovalId::from_uuid(id(0x701))))
        );
        assert_eq!(field(&intent, "amount_minor"), 1_100);
        assert_eq!(
            cart.lines
                .first()
                .unwrap()
                .price_override
                .as_ref()
                .unwrap()
                .approver,
            Some(manager())
        );
        // The line's name is not needed to identify it.
        assert!(!intent.payload.to_string().contains("صنف"));
    }

    #[test]
    fn a_second_override_replaces_the_first_and_the_captured_price_clears_it() {
        let (cart, ..) = reprice(two_teas(), 1_100, code("damaged_pack"), Some(1_000)).unwrap();
        let (cart, intent, _) = reprice(cart, 1_050, code("damaged_pack"), Some(1_000)).unwrap();
        assert_eq!(price_of(&cart), jod(1_050));
        assert_eq!(
            (
                field(&intent, "captured_minor"),
                field(&intent, "previous_minor")
            ),
            (&serde_json::json!(1_250), &serde_json::json!(1_100))
        );
        let (cart, intent, _) = reprice(cart, 1_250, code("catalogue_error"), Some(1_000)).unwrap();
        let line = cart.lines.first().unwrap();
        assert_eq!(
            (line.price_override.as_ref(), line.price()),
            (None, jod(1_250))
        );
        assert_eq!(field(&intent, "amount_minor"), 1_250);
    }

    #[test]
    fn an_override_is_refused_by_name() {
        let refused = |cart: Cart, line: SaleLineId, to: Money| {
            override_price(
                cart,
                line,
                to,
                code("damaged_pack"),
                Some(jod(1_000)),
                &proof_of(shift_lead(), Role::ShiftLead, moment()),
            )
            .map(|r| r.0)
        };
        assert_eq!(
            refused(two_teas(), line_id(9), jod(1_100)),
            Err(CartError::LineNotFound(line_id(9)))
        );
        assert_eq!(
            refused(
                two_teas(),
                line_id(1),
                Money::from_minor(1_100, Currency::USD)
            ),
            Err(CartError::Money(MoneyError::CurrencyMismatch("JOD", "USD")))
        );
        assert_eq!(
            refused(two_teas(), line_id(1), jod(-1)),
            Err(CartError::NegativeLine)
        );
        // A price label is the price displayed on the package itself.
        let mut cheese = product(0xC4, UnitOfMeasure::Kilogram);
        cheese.unit_price = jod(8_500);
        let code_read = price_label("12345", 416);
        let hit = ProductHit {
            product: cheese,
            matched_on: MatchedOn::Plu("12345".to_owned()),
        };
        let lookup = resolve_scan(
            parse_scan(&code_read, &[price_rule()]).unwrap(),
            vec![hit],
            JOD,
        )
        .unwrap();
        let labelled =
            add_line(building(), AddLine::from_scan(line_id(1), lookup).unwrap()).unwrap();
        assert_eq!(
            refused(labelled, line_id(1), jod(400)),
            Err(CartError::LabelPriceNotOverridable(line_id(1)))
        );
        // A department line's amount is its own capped command's (1.4.12).
        let mut department = two_teas();
        if let Some(line) = department.lines.first_mut() {
            line.product_id = None;
            line.price_origin = PriceOrigin::Department {
                tax_category_id: line.tax_category_id,
            };
        }
        assert_eq!(
            refused(department, line_id(1), jod(1_100)),
            Err(CartError::DepartmentPriceOutsideDepartmentSale)
        );
        // An amount off fixes the price until it is cleared; a share does not.
        let (amount_off, _) = discount(two_teas(), off(100)).unwrap();
        assert_eq!(
            refused(amount_off.clone(), line_id(1), jod(1_100)),
            Err(CartError::AmountDiscountFixesPrice(line_id(1)))
        );
        let (cleared, _) = discount(amount_off, off(0)).unwrap();
        assert!(refused(cleared, line_id(1), jod(1_100)).is_ok());
        let (share_off, _) = discount(two_teas(), pct(50_000)).unwrap();
        assert!(refused(share_off, line_id(1), jod(1_100)).is_ok());
        // A cost that cannot be compared with the price is refused, not passed.
        assert_eq!(
            override_price(
                two_teas(),
                line_id(1),
                jod(1_100),
                code("damaged_pack"),
                Some(Money::from_minor(1_000, Currency::USD)),
                &proof_of(shift_lead(), Role::ShiftLead, moment()),
            )
            .map(|r| r.0),
            Err(CartError::Money(MoneyError::CurrencyMismatch("JOD", "USD")))
        );
    }

    #[test]
    fn an_overridden_line_is_discounted_and_divided_at_its_new_price() {
        // Two teas at 1.000 instead of 1.250: a line worth 2.000.
        let (cart, ..) = reprice(two_teas(), 1_000, code("damaged_pack"), Some(900)).unwrap();
        // 5% of 2.000 is 0.100, which is the cashier's to give, and no more.
        assert!(discount(cart.clone(), off(100)).is_ok());
        assert_eq!(
            discount(cart.clone(), off(101)).map(|r| r.0),
            Err(CartError::DiscountAboveCap(
                "0.101 JOD".to_owned(),
                "5%".to_owned()
            ))
        );
        let managed = |cart: Cart, kind: DiscountKind| {
            apply_line_discount(
                cart,
                line_id(1),
                ask(kind, "loyal_customer"),
                DiscountCap::Unlimited,
                RULE,
                &proof(manager(), Role::Manager, moment()),
                None,
            )
            .map(|r| r.0)
        };
        assert!(managed(cart.clone(), off(2_000)).is_ok());
        assert_eq!(
            managed(cart, off(2_001)),
            Err(CartError::DiscountExceedsLine("2.001 JOD".to_owned()))
        );
        // A share off the line, given before the override, is a share of the
        // new price; the basket is worth its lines at their prices.
        let (shared, _) = discount(basket(), pct(50_000)).unwrap();
        let (shared, ..) = reprice(shared, 1_000, code("damaged_pack"), Some(900)).unwrap();
        let (shared, intent) = by_manager(shared, off(5_650)).unwrap();
        assert_eq!(field(&intent, "basket_value_minor"), 5_650);
        assert_eq!(
            discount_attributions(&shared, RULE)
                .unwrap()
                .iter()
                .map(|a| (a.line_id, a.source, a.amount.minor()))
                .collect::<Vec<_>>(),
            vec![
                (line_id(1), DiscountSource::ManualLine, 100),
                (line_id(1), DiscountSource::ManualBasket, 1_900),
                (line_id(2), DiscountSource::ManualBasket, 3_750),
            ]
        );
    }

    #[test]
    fn a_lower_price_clears_an_amount_off_the_basket() {
        let (cart, _) = by_manager(basket(), off(500)).unwrap();
        // A higher price raises the basket's value, and the amount stays.
        let (cart, ..) = reprice(cart, 1_300, code("catalogue_error"), Some(1_000)).unwrap();
        assert_eq!(cart.basket_discount.as_ref().unwrap().kind, off(500));
        // A lower one lowers it, and the amount is cleared, as for a void.
        let (cart, ..) = reprice(cart, 1_100, code("damaged_pack"), Some(1_000)).unwrap();
        assert_eq!(cart.basket_discount, None);
        // A share off the basket keeps its share.
        let (shared, _) = by_manager(basket(), pct(100_000)).unwrap();
        let (shared, ..) =
            reprice(shared, 990, OverrideReason::DisplayedPrice, Some(1_000)).unwrap();
        assert_eq!(shared.basket_discount.unwrap().kind, pct(100_000));
    }

    fn anchors() -> impl Strategy<Value = (usize, i64, i64)> {
        (0usize..=4, -2i64..=2, -50i64..=3_600)
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// No line charges more than its ministry ceiling, from the moment it
        /// enters the cart through every override. No override takes a price
        /// below what the catalogue charged unless the product's recorded cost
        /// admits it, or the shelf showed that price, which only ever lowers
        /// it. And every override is the one the line then charges, recorded
        /// at that amount, with a label to reprint exactly when the shelf was
        /// wrong (E.33, E.70, E.71).
        #[test]
        fn prop_no_price_passes_its_floor_or_its_ceiling(
            catalogue in 0i64..=3_000,
            ceiling in proptest::option::of((any::<bool>(), -2i64..=2, 0i64..=3_500)),
            cost in proptest::option::of((any::<bool>(), -2i64..=2, 0i64..=3_500)),
            steps in proptest::collection::vec((anchors(), any::<bool>()), 1..=4),
        ) {
            // Each limit sits at the catalogue's price, give or take a fil,
            // or anywhere: the boundaries are where a wrong comparison shows.
            let near = |(anchored, delta, anywhere): (bool, i64, i64)| {
                if anchored { (catalogue + delta).max(0) } else { anywhere }
            };
            let (ceiling, cost) = (ceiling.map(near), cost.map(near));
            let mut item = product(0x20, UnitOfMeasure::Each);
            item.unit_price = jod(catalogue);
            item.max_price = ceiling.map(jod);
            let added = add(building(), 1, &item, 2_000);
            if let Some(c) = ceiling.filter(|c| catalogue > *c) {
                prop_assert_eq!(
                    added,
                    Err(CartError::AboveMaxPrice(
                        jod(catalogue).format_exact(),
                        jod(c).format_exact()
                    ))
                );
                return Ok(());
            }
            let mut cart = added.unwrap();
            let auth = proof_of(shift_lead(), Role::ShiftLead, moment());
            for ((anchor, delta, anywhere), displayed) in steps {
                let current = price_of(&cart).minor();
                let to = match anchor {
                    0 => catalogue + delta,
                    1 => ceiling.map_or(anywhere, |c| c + delta),
                    2 => cost.map_or(anywhere, |c| c + delta),
                    3 => current + delta,
                    _ => anywhere,
                };
                let reason = if displayed {
                    OverrideReason::DisplayedPrice
                } else {
                    code("damaged_pack")
                };
                let fmt = |minor: i64| jod(minor).format_exact();
                let expected = if to < 0 {
                    Some(CartError::NegativeLine)
                } else if let Some(c) = ceiling.filter(|c| to > *c) {
                    Some(CartError::AboveMaxPrice(fmt(to), fmt(c)))
                } else if displayed && to >= current {
                    Some(CartError::DisplayedPriceNotLower(fmt(to), fmt(current)))
                } else if !displayed && to < catalogue {
                    match cost {
                        None => Some(CartError::NoRecordedCost(item.id, fmt(to))),
                        Some(c) if to < c => Some(CartError::BelowPriceFloor(fmt(to), fmt(c))),
                        Some(_) => None,
                    }
                } else {
                    None
                };
                match override_price(cart.clone(), line_id(1), jod(to), reason, cost.map(jod), &auth) {
                    Ok((next, intent, reprint)) => {
                        prop_assert_eq!(expected, None);
                        let line = next.lines.first().unwrap();
                        prop_assert_eq!(line.price(), jod(to));
                        prop_assert_eq!(line.unit_price, jod(catalogue));
                        prop_assert_eq!(line.price_override.is_none(), to == catalogue);
                        prop_assert_eq!(field(&intent, "amount_minor"), &serde_json::json!(to));
                        prop_assert_eq!(reprint.is_some(), displayed);
                        cart = next;
                    }
                    Err(refused) => prop_assert_eq!(Some(refused), expected),
                }
                let line = cart.lines.first().unwrap();
                if let Some(c) = line.max_price {
                    prop_assert!(line.price().minor() <= c.minor());
                }
            }
        }
    }

    // ── pricing the cart (1.4.9) ────────────────────────────────────────

    use crate::tax::{TaxBase, TaxBasis, TaxTreatment};

    /// The standard band `product()` sells in, and two untaxed ones.
    fn band(n: u8) -> TaxCategoryId {
        TaxCategoryId::from_uuid(id(0x12 + u128::from(n)))
    }

    fn gst(treatment: TaxTreatment, ppm: i64) -> Vec<TaxComponent> {
        vec![TaxComponent {
            code: "GST".to_owned(),
            treatment,
            basis: TaxBasis::Percentage {
                rate: Percent::from_ppm(ppm),
            },
            sequence: 1,
            base: TaxBase::LineNet,
            is_inclusive_capable: true,
        }]
    }

    /// A synthetic pack: 16% standard, zero-rated, exempt.
    fn bands() -> TaxBands {
        TaxBands::from([
            (band(0), gst(TaxTreatment::Standard, 160_000)),
            (band(1), gst(TaxTreatment::Zero, 0)),
            (band(2), gst(TaxTreatment::Exempt, 0)),
        ])
    }

    fn store(price_mode: PriceMode) -> PricingSettings {
        PricingSettings {
            price_mode,
            profile: StoreTaxProfile::Standard,
            rule: RULE,
        }
    }

    fn priced(cart: &Cart, price_mode: PriceMode) -> PricedCart {
        price_cart(cart, &bands(), &store(price_mode)).unwrap()
    }

    fn minors(priced: &PricedCart) -> [i64; 4] {
        [
            priced.subtotal.minor(),
            priced.discount_total.minor(),
            priced.tax_total.minor(),
            priced.total.minor(),
        ]
    }

    #[test]
    fn a_line_is_priced_in_the_store_s_price_mode() {
        // Two teas at 1.250 under 16%. Inclusive: 2.500 holds its tax, the net
        // 2.500 / 1.16 = 2.15517… rounded once to 2.155, and the tax 0.345 is
        // what is left, so the two add back to the shelf's price.
        let inclusive = priced(&two_teas(), PriceMode::Inclusive);
        let line = inclusive.lines.first().unwrap();
        assert_eq!(
            (line.gross_before_discount, line.taxable, line.line_total),
            (jod(2_500), jod(2_500), jod(2_500))
        );
        assert_eq!((line.tax.net, line.tax.tax_total), (jod(2_155), jod(345)));
        assert_eq!(minors(&inclusive), [2_500, 0, 345, 2_500]);
        // Exclusive: 2.500 is the net and the tax goes on top.
        let exclusive = priced(&two_teas(), PriceMode::Exclusive);
        assert_eq!(
            (exclusive.lines.first().unwrap().tax.net, exclusive.total),
            (jod(2_500), jod(2_900))
        );
        assert_eq!(minors(&exclusive), [2_500, 0, 400, 2_900]);
        // One row on the receipt, from the line's own result.
        let row = inclusive.tax_summary.first().unwrap();
        assert_eq!(inclusive.tax_summary.len(), 1);
        assert_eq!(
            (row.net, row.tax, row.gross),
            (jod(2_155), jod(345), jod(2_500))
        );
        assert_eq!(inclusive.currency, Currency::JOD);
    }

    #[test]
    fn discounts_come_off_before_tax() {
        // Teas 2.500 and packs 3.750; the cashier's 5% off the teas (0.125),
        // then the manager's 10% off a basket worth 6.125: 0.613, divided
        // 0.238 and 0.375, the leftover fil to the teas' larger remainder.
        let (cart, _) = discount(basket(), pct(50_000)).unwrap();
        let (cart, _) = by_manager(cart, pct(100_000)).unwrap();
        let exclusive = priced(&cart, PriceMode::Exclusive);
        let shares: Vec<Vec<i64>> = exclusive
            .lines
            .iter()
            .map(|l| {
                l.discount_attributions
                    .iter()
                    .map(|a| a.amount.minor())
                    .collect()
            })
            .collect();
        assert_eq!(shares, vec![vec![125, 238], vec![375]]);
        // They are the attributions 1.4.6 makes, line by line.
        let flat: Vec<DiscountAttribution> = exclusive
            .lines
            .iter()
            .flat_map(|l| l.discount_attributions.clone())
            .collect();
        let mut expected = discount_attributions(&cart, RULE).unwrap();
        expected.sort_by_key(|a| (a.line_id, a.source == DiscountSource::ManualBasket));
        assert_eq!(flat, expected);
        // Tax is charged on what is left: 2.137 and 3.375 at 16%.
        let taxed: Vec<[i64; 3]> = exclusive
            .lines
            .iter()
            .map(|l| {
                [
                    l.taxable.minor(),
                    l.tax.tax_total.minor(),
                    l.line_total.minor(),
                ]
            })
            .collect();
        assert_eq!(taxed, vec![[2_137, 342, 2_479], [3_375, 540, 3_915]]);
        // 6.250 − 0.738 + 0.882 = 6.394.
        assert_eq!(minors(&exclusive), [6_250, 738, 882, 6_394]);
        // Inclusive: the customer pays what is left, its tax inside it.
        let inclusive = priced(&cart, PriceMode::Inclusive);
        assert_eq!(inclusive.total, jod(2_137 + 3_375));
    }

    #[test]
    fn an_overridden_line_is_priced_at_the_price_it_charges() {
        let (cart, ..) = reprice(two_teas(), 1_000, code("damaged_pack"), Some(900)).unwrap();
        let inclusive = priced(&cart, PriceMode::Inclusive);
        let line = inclusive.lines.first().unwrap();
        assert_eq!(cart.lines.first().unwrap().unit_price, jod(1_250));
        assert_eq!(
            (line.gross_before_discount, line.discount_attributions.len()),
            (jod(2_000), 0)
        );
        assert_eq!(minors(&inclusive), [2_000, 0, 276, 2_000]);
    }

    #[test]
    fn a_receipt_of_three_rates_has_a_row_for_each() {
        let mut cart = two_teas();
        for (n, untaxed) in [(2, 1), (3, 2)] {
            let mut item = product(0x20 + n, UnitOfMeasure::Each);
            item.tax_category_id = band(untaxed);
            cart = add(cart, n, &item, 1_000).unwrap();
        }
        let exclusive = priced(&cart, PriceMode::Exclusive);
        let rows: Vec<(TaxTreatment, i64, i64)> = exclusive
            .tax_summary
            .iter()
            .map(|r| (r.treatment, r.net.minor(), r.tax.minor()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (TaxTreatment::Standard, 2_500, 400),
                (TaxTreatment::Zero, 1_250, 0),
                (TaxTreatment::Exempt, 1_250, 0),
            ]
        );
        assert_eq!(minors(&exclusive), [5_000, 0, 400, 5_400]);
    }

    #[test]
    fn a_line_without_its_tax_band_is_refused_by_name() {
        let mut partial = bands();
        partial.remove(&band(0));
        assert_eq!(
            price_cart(&two_teas(), &partial, &store(PriceMode::Inclusive)),
            Err(CartError::TaxBandMissing(band(0)))
        );
        // A store not registered for GST charges none and needs no band.
        for price_mode in [PriceMode::Inclusive, PriceMode::Exclusive] {
            let unregistered = PricingSettings {
                profile: StoreTaxProfile::Unregistered,
                ..store(price_mode)
            };
            let untaxed = price_cart(&two_teas(), &TaxBands::new(), &unregistered).unwrap();
            assert_eq!(minors(&untaxed), [2_500, 0, 0, 2_500]);
            assert!(untaxed.tax_summary.is_empty());
        }
        // A supply that is not ordinary domestic retail is the tax engine's
        // to refuse, and it is refused here as there.
        let mut export = two_teas();
        export.supply.destination = SupplyDestination::Export;
        assert_eq!(
            price_cart(&export, &bands(), &store(PriceMode::Inclusive)),
            Err(CartError::Tax(TaxError::SupplyDestinationRefused(
                SupplyDestination::Export
            )))
        );
    }

    #[test]
    fn an_empty_cart_prices_to_nothing() {
        let nothing = priced(&building(), PriceMode::Inclusive);
        assert!(nothing.lines.is_empty() && nothing.tax_summary.is_empty());
        assert_eq!(minors(&nothing), [0, 0, 0, 0]);
    }

    #[test]
    fn a_cart_holding_one_line_id_twice_is_refused() {
        let mut cart = two_teas();
        let again = cart.lines.first().unwrap().clone();
        cart.lines.push(again);
        assert_eq!(
            price_cart(&cart, &bands(), &store(PriceMode::Inclusive)),
            Err(CartError::DuplicateLineId(line_id(1)))
        );
    }

    /// One line a cashier might ring up, and what is done to it.
    #[derive(Debug, Clone)]
    struct Scan {
        product: u8,
        price: i64,
        weighed: bool,
        milli: i64,
        band: u8,
        override_to: Option<i64>,
        /// A share in ppm, or an amount as that share of the line's value.
        discount: Option<(bool, i64)>,
    }

    fn scans() -> impl Strategy<Value = Vec<Scan>> {
        let scan = (
            (0u8..3, prop_oneof![3 => 0i64..=20, 1 => 0i64..=20_000]),
            (any::<bool>(), 1i64..=3_000, 1i64..=5, 0u8..3),
            proptest::option::of(prop_oneof![3 => 0i64..=20, 1 => 0i64..=20_000]),
            proptest::option::of((any::<bool>(), 0i64..=1_000_000)),
        )
            .prop_map(
                |((product, price), (weighed, grams, units, band), override_to, discount)| Scan {
                    product,
                    price,
                    weighed,
                    milli: if weighed { grams } else { units * 1_000 },
                    band,
                    override_to,
                    discount,
                },
            );
        proptest::collection::vec(scan, 0..=8)
    }

    fn modes() -> impl Strategy<Value = PriceMode> {
        prop_oneof![Just(PriceMode::Inclusive), Just(PriceMode::Exclusive)]
    }

    fn profiles() -> impl Strategy<Value = StoreTaxProfile> {
        prop_oneof![
            3 => Just(StoreTaxProfile::Standard),
            1 => Just(StoreTaxProfile::Unregistered),
        ]
    }

    /// A discount asked as `share`: a percentage, or that share of `value` as
    /// an amount, which is then within the value by construction.
    fn asked((percent, share): (bool, i64), value: i64) -> DiscountKind {
        if percent {
            pct(share)
        } else {
            off(i64::try_from(i128::from(value) * i128::from(share) / 1_000_000).unwrap())
        }
    }

    /// A manager's discount of `kind` off the whole basket, under the store's
    /// `rule`: the rule the basket is measured by when it is priced.
    fn basket_off_under(
        cart: Cart,
        kind: DiscountKind,
        rule: RoundingRule,
    ) -> Result<Cart, CartError> {
        apply_basket_discount(
            cart,
            ask(kind, "loyal_customer"),
            DiscountCap::Unlimited,
            rule,
            &proof(manager(), Role::Manager, moment()),
            None,
        )
        .map(|r| r.0)
    }

    /// The cart those scans ring up through the transitions themselves, under
    /// the store's `rule`, with any basket discount last. A step a transition
    /// refuses is left out, as the till would leave it out.
    fn ring(scans: &[Scan], basket: Option<(bool, i64)>, rule: RoundingRule) -> Cart {
        let mut cart = building();
        for (n, scan) in (1u128..).zip(scans) {
            let mut item = product(
                0x30 + u128::from(scan.product),
                if scan.weighed {
                    UnitOfMeasure::Kilogram
                } else {
                    UnitOfMeasure::Each
                },
            );
            item.unit_price = jod(scan.price);
            item.tax_category_id = band(scan.band);
            let Ok(next) = add(cart.clone(), n, &item, scan.milli) else {
                continue;
            };
            cart = next;
            if let Some(to) = scan.override_to
                && let Ok((next, ..)) = override_price(
                    cart.clone(),
                    line_id(n),
                    jod(to),
                    code("damaged_pack"),
                    Some(jod(0)),
                    &proof_of(shift_lead(), Role::ShiftLead, moment()),
                )
            {
                cart = next;
            }
            if let Some(share) = scan.discount {
                let value = cart
                    .lines
                    .last()
                    .unwrap()
                    .price()
                    .mul_qty(Qty::from_milli(scan.milli), RoundingRule::Floor)
                    .unwrap()
                    .minor();
                if let Ok((next, _)) = apply_line_discount(
                    cart.clone(),
                    line_id(n),
                    ask(asked(share, value), "loyal_customer"),
                    DiscountCap::Unlimited,
                    rule,
                    &proof(manager(), Role::Manager, moment()),
                    None,
                ) {
                    cart = next;
                }
            }
        }
        if let Some(share) = basket {
            let value = basket_value(&cart, rule).unwrap().minor();
            if let Ok(next) = basket_off_under(cart.clone(), asked(share, value), rule) {
                cart = next;
            }
        }
        cart
    }

    /// What a priced line is, apart from where it sits and the id it was
    /// given, for comparing two orders of one basket.
    fn described(cart: &Cart, priced: &PricedCart) -> Vec<String> {
        let mut all: Vec<String> = cart
            .lines
            .iter()
            .zip(&priced.lines)
            .map(|(line, p)| {
                let mut shares: Vec<(String, i64)> = p
                    .discount_attributions
                    .iter()
                    .map(|a| (format!("{:?}", a.source), a.amount.minor()))
                    .collect();
                shares.sort();
                format!(
                    "{:?} {:?} {} {} {} {:?} {:?} | {} {:?} {} {:?} {}",
                    line.tax_category_id,
                    line.product_id,
                    line.unit_price.minor(),
                    line.price().minor(),
                    line.qty.milli(),
                    line.discount.as_ref().map(|d| d.kind),
                    line.price_override.as_ref().map(|o| o.to.minor()),
                    p.gross_before_discount.minor(),
                    shares,
                    p.taxable.minor(),
                    p.tax,
                    p.line_total.minor(),
                )
            })
            .collect();
        all.sort();
        all
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// The master identity, for every cart the transitions ring up: the
        /// total is the sum of the lines' totals, and equals the subtotal less
        /// the discounts, plus the tax where prices exclude it. Each line is its
        /// gross less exactly its own attributions, taxed on the rest; the
        /// receipt's tax rows add up to the tax; and every such cart can be
        /// priced, whatever the price mode, rule or registration.
        #[test]
        fn prop_total_equals_lines_minus_discounts_plus_tax(
            scans in scans(),
            basket in proptest::option::of((any::<bool>(), 0i64..=1_000_000)),
            price_mode in modes(),
            rule in rules(),
            profile in profiles(),
        ) {
            let cart = ring(&scans, basket, rule);
            let settings = PricingSettings { price_mode, profile, rule };
            let priced = price_cart(&cart, &bands(), &settings).unwrap();
            let sum = |f: &dyn Fn(&PricedLine) -> Money| {
                Money::sum(priced.lines.iter().map(f), JOD).unwrap()
            };
            prop_assert_eq!(priced.total, sum(&|l| l.line_total));
            prop_assert_eq!(priced.subtotal, sum(&|l| l.gross_before_discount));
            prop_assert_eq!(priced.tax_total, sum(&|l| l.tax.tax_total));
            prop_assert_eq!(
                priced.discount_total,
                Money::sum(
                    priced.lines.iter().flat_map(|l| l.discount_attributions.iter().map(|a| a.amount)),
                    JOD
                )
                .unwrap()
            );
            let on_top = match price_mode {
                PriceMode::Exclusive => priced.tax_total,
                PriceMode::Inclusive => jod(0),
            };
            prop_assert_eq!(
                priced.total,
                priced
                    .subtotal
                    .checked_sub(priced.discount_total)
                    .unwrap()
                    .checked_add(on_top)
                    .unwrap()
            );
            prop_assert_eq!(
                priced.tax_total,
                Money::sum(priced.tax_summary.iter().map(|r| r.tax), JOD).unwrap()
            );
            prop_assert_eq!(priced.lines.len(), cart.lines.len());
            for (line, p) in cart.lines.iter().zip(&priced.lines) {
                prop_assert_eq!(p.line_id, line.id);
                prop_assert_eq!(p.gross_before_discount, line.price().mul_qty(line.qty, rule).unwrap());
                let off = Money::sum(p.discount_attributions.iter().map(|a| a.amount), JOD).unwrap();
                prop_assert_eq!(p.taxable, p.gross_before_discount.checked_sub(off).unwrap());
                prop_assert!(!p.taxable.is_negative());
                prop_assert_eq!(p.line_total, p.tax.gross);
                let expected_total = match price_mode {
                    PriceMode::Exclusive => p.taxable.checked_add(p.tax.tax_total).unwrap(),
                    PriceMode::Inclusive => p.taxable,
                };
                prop_assert_eq!(p.line_total, expected_total);
                prop_assert!(p.discount_attributions.iter().all(|a| a.line_id == line.id));
            }
        }

        /// The same cart priced twice, or read back from the form a parked
        /// cart is stored in, prices to byte-identical output.
        #[test]
        fn prop_price_cart_is_deterministic(
            scans in scans(),
            basket in proptest::option::of((any::<bool>(), 0i64..=1_000_000)),
            price_mode in modes(),
            rule in rules(),
        ) {
            let cart = ring(&scans, basket, rule);
            let settings = PricingSettings { price_mode, profile: StoreTaxProfile::Standard, rule };
            let once = serde_json::to_string(&price_cart(&cart, &bands(), &settings).unwrap()).unwrap();
            let twice = serde_json::to_string(&price_cart(&cart, &bands(), &settings).unwrap()).unwrap();
            let stored: Cart = serde_json::from_str(&serde_json::to_string(&cart).unwrap()).unwrap();
            let restored = serde_json::to_string(&price_cart(&stored, &bands(), &settings).unwrap()).unwrap();
            prop_assert_eq!(&once, &twice);
            prop_assert_eq!(&once, &restored);
        }

        /// E.19b: the same basket scanned in any order, its ids minted afresh
        /// in that order, costs the same: the same total, subtotal, discounts
        /// and tax, the same receipt rows, and the same lines priced the same.
        #[test]
        fn prop_price_cart_is_invariant_under_line_reordering(
            scans in scans(),
            basket in proptest::option::of((any::<bool>(), 0i64..=1_000_000)),
            price_mode in modes(),
            rule in rules(),
            order in proptest::collection::vec(any::<proptest::sample::Index>(), 8),
        ) {
            let cart = ring(&scans, basket, rule);
            let mut shuffled = cart.clone();
            for (i, pick) in (0..shuffled.lines.len()).rev().zip(&order) {
                let j = pick.index(i + 1);
                shuffled.lines.swap(i, j);
            }
            for (n, line) in (0x500u128..).zip(shuffled.lines.iter_mut()) {
                line.id = line_id(n);
            }
            let settings = PricingSettings { price_mode, profile: StoreTaxProfile::Standard, rule };
            let (a, b) = (
                price_cart(&cart, &bands(), &settings).unwrap(),
                price_cart(&shuffled, &bands(), &settings).unwrap(),
            );
            prop_assert_eq!(minors(&a), minors(&b));
            prop_assert_eq!(&a.tax_summary, &b.tax_summary);
            prop_assert_eq!(described(&cart, &a), described(&shuffled, &b));
        }

        /// E.18: a basket discounted by everything it is worth is a lawful
        /// sale of nothing. It prices without error to a total, a tax and
        /// every line of zero, whether the discount was asked as all of the
        /// basket or as its whole value.
        #[test]
        fn prop_zero_total_cart_is_valid(
            scans in scans(),
            as_share in any::<bool>(),
            price_mode in modes(),
            rule in rules(),
        ) {
            let mut cart = ring(&scans, None, rule);
            if !cart.lines.is_empty() {
                let value = basket_value(&cart, rule).unwrap().minor();
                let all = if as_share { pct(1_000_000) } else { off(value) };
                cart = basket_off_under(cart, all, rule).unwrap();
            }
            let settings = PricingSettings { price_mode, profile: StoreTaxProfile::Standard, rule };
            let priced = price_cart(&cart, &bands(), &settings).unwrap();
            prop_assert_eq!(priced.total, jod(0));
            prop_assert_eq!(priced.tax_total, jod(0));
            prop_assert_eq!(priced.discount_total, priced.subtotal);
            prop_assert!(priced.lines.iter().all(|l| l.line_total.is_zero() && l.taxable.is_zero()));
        }
    }
}
