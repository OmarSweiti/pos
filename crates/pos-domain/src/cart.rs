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
//! override at 1.4.7, and a promotion group when promotions are first
//! captured. A field added before its rule would be a value nothing checks.

use std::fmt;

use serde::{Deserialize, Serialize};

use serde_json::json;

use crate::audit::AuditIntent;
use crate::catalog::{
    DerivedWeight, PriceOrigin, PriceSource, Product, ProductHit, ScanLookup, UnitOfMeasure,
};
use crate::ids::{
    CustomerId, ProductId, RegisterId, SaleId, SaleLineId, ShiftId, TaxCategoryId, UserId,
};
use crate::money::{Currency, Money, MoneyError, Qty, RoundingRule};
use crate::permissions::{
    ApprovalBinding, ApprovalHandle, Authorized, Capability, PermissionError, cap,
};
use crate::pricing::{DiscountCap, DiscountKind, DiscountRequest, LineDiscount, exceeds_value};
use crate::tax::{SupplyDestination, SupplyTaxContext, TaxError};
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
    pub qty: Qty,
    pub unit: UnitOfMeasure,
    pub is_weighed: bool,
    pub tax_category_id: TaxCategoryId,
    /// The line's manual discount, as it was asked for (1.4.5). At most one:
    /// another replaces it, and a discount of zero clears it.
    pub discount: Option<LineDiscount>,
    /// Captured with the line, so a catalogue edit while the cart is open
    /// cannot clear the age gate (1.4.3, 1.4.3b).
    pub min_age: Option<u8>,
    pub age_confirmed: bool,
    /// The weight a scale's label claimed: the stock basis, never money
    /// (§4.3).
    pub label: Option<DerivedWeight>,
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
/// As delivered at 1.4.3, it has no `max_price` and no `age_confirmed`: the
/// regulated ceiling is checked when a price changes (1.4.7), and an age is
/// confirmed only by an operator, through 1.4.3b, never by the request that
/// adds the line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AddLine {
    pub line_id: SaleLineId,
    pub product_id: Option<ProductId>,
    pub name_snapshot: String,
    pub price: PriceSource,
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
/// - [`CartError::Money`]: a price in another currency than the cart's.
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
    let mut cart = cart;
    cart.lines.push(CartLine {
        id: req.line_id,
        product_id: Some(product),
        name_snapshot: req.name_snapshot,
        unit_price,
        price_origin: req.price.origin().clone(),
        qty: req.qty,
        unit: req.unit,
        is_weighed: req.is_weighed,
        tax_category_id: req.tax_category_id,
        discount: None,
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
    found.qty = qty;
    Ok(cart)
}

/// `cart` without line `line`, and the audit intent that records who removed
/// it, why, and what it was.
///
/// `at` is when, supplied by the shell (I-8). The intent's action is the
/// `line.void` capability's spelling, and its payload carries the line's
/// facts — never its name, which is not needed to identify it.
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
/// the quantity changes, and is turned into money when the cart is priced.
///
/// A discount replaces any the line had, so two discounts within the cap never
/// add up past it, and a discount of zero clears it. Within the cap no approval
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
    if found.unit_price.is_negative() {
        return Err(CartError::NegativeLine);
    }
    if let DiscountKind::Amount(amount) = kind
        && amount.currency() != currency
    {
        return Err(MoneyError::CurrencyMismatch(currency.code(), amount.currency().code()).into());
    }
    if exceeds_value(kind, found.unit_price, found.qty) {
        return Err(CartError::DiscountExceedsLine(kind.format()));
    }
    let discount = LineDiscount {
        kind,
        reason: request.reason,
        by: auth.actor(),
        approver: None,
    };
    let gross = found.unit_price.mul_qty(found.qty, rule)?;
    let amount = discount.amount_off(gross, rule)?;
    let (approver, approval_id) =
        if kind.is_zero() || limit.admits(kind, found.unit_price, found.qty) {
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
/// Confirmed, the line stays and is no longer blocked; declined, it is removed.
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
    /// Added at 1.4.5: a discount of less than nothing is a surcharge.
    #[error("discount {0} is negative")]
    NegativeDiscount(String),
    /// Added at 1.4.5: an amount off fixes the line's quantity until cleared.
    #[error("line {0} has an amount off; clear the discount to change its quantity")]
    AmountDiscountFixesQuantity(SaleLineId),
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
    const GOLDEN_CART_JSON: &str = r#"{"id":"00000000-0000-0000-0000-000000000001","register_id":"00000000-0000-0000-0000-000000000002","shift_id":"00000000-0000-0000-0000-000000000003","cashier_id":"00000000-0000-0000-0000-000000000004","currency":"JOD","lines":[{"id":"00000000-0000-0000-0000-000000000010","product_id":"00000000-0000-0000-0000-000000000011","name_snapshot":"SYNTH item","unit_price":{"minor":1250,"currency":"JOD"},"price_origin":{"catalog":{"product_id":"00000000-0000-0000-0000-000000000011"}},"qty":1500,"unit":"kilogram","is_weighed":true,"tax_category_id":"00000000-0000-0000-0000-000000000012","discount":null,"min_age":18,"age_confirmed":false,"label":{"qty":1500,"catalog_unit_price":{"minor":1250,"currency":"JOD"}}}],"customer_id":null,"buyer_tin":"999888777","buyer_name":"SYNTH buyer","supply":{"destination":"Domestic","reason":null,"evidence_ref":null},"is_training":true,"opened_at":978307200000}"#;

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
            qty: Qty::from_milli(1_500),
            unit: UnitOfMeasure::Kilogram,
            is_weighed: true,
            tax_category_id: TaxCategoryId::from_uuid(id(0x12)),
            discount: None,
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
        (
            (uuids(), proptest::option::of(uuids()), any::<String>()),
            (any::<i64>(), currencies(), origin),
            (any::<i64>(), units(), any::<bool>(), uuids()),
            (
                proptest::option::of(any::<u8>()),
                any::<bool>(),
                label,
                discount,
            ),
        )
            .prop_map(
                |(
                    (line, product, name),
                    (price, currency, price_origin),
                    (qty, unit, is_weighed, tax),
                    (min_age, age_confirmed, label, discount),
                )| CartLine {
                    id: SaleLineId::from_uuid(line),
                    product_id: product.map(ProductId::from_uuid),
                    name_snapshot: name,
                    unit_price: Money::from_minor(price, currency),
                    price_origin,
                    qty: Qty::from_milli(qty),
                    unit,
                    is_weighed,
                    tax_category_id: TaxCategoryId::from_uuid(tax),
                    discount,
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
        (
            (uuids(), uuids(), uuids(), uuids()),
            (currencies(), proptest::collection::vec(lines(), 0..=8)),
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
                    (currency, lines),
                    (customer, buyer_tin, buyer_name),
                    (supply, is_training, opened_at),
                )| Cart {
                    id: SaleId::from_uuid(sale),
                    register_id: RegisterId::from_uuid(register),
                    shift_id: ShiftId::from_uuid(shift),
                    cashier_id: UserId::from_uuid(cashier),
                    currency,
                    lines,
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
        let handle = ApprovalId::from_uuid(id(0x700));
        let binding = ApprovalBinding {
            entity_id: line.as_uuid(),
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
}
