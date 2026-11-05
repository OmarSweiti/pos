//! Building a basket: `cart_open`, `cart_current`, `cart_add_line`,
//! `cart_add_scan` and `cart_confirm_age` (`ref/ipc-contract.md` §3, "Cart";
//! microstep 1.4.11, its first part).
//!
//! **No command here takes a price.** A line's price comes from the catalogue,
//! through `AddLine::from_product`, or from a label the scan handler resolved,
//! through `AddLine::from_scan`. `PriceSource` cannot be built from an
//! integer, so nothing the webview sends can become a line's price
//! (`ref/ipc-contract.md` §1, rule 6). The commands that do take a price are
//! later parts of this step, each controlled.
//!
//! **Every answer is the priced cart.** [`CartSnapshot`] is `price_cart`'s
//! result under the store's own settings, in minor units, so the screen
//! computes no money (§5, rule 1). A cart a store cannot price is refused
//! when it would be changed or opened, not when it is paid for.
//!
//! **Every change is on disk before it is answered.** The till holds its
//! working cart as an `active` `parked_cart` row from the moment it opens, and
//! each change replaces that row's snapshot in the transaction that answers it
//! (the owner's decision of 5 November 2026). A change that is refused leaves
//! both the row and the till's copy as they were.
//!
//! **Stock is screened as a line is added** (1.10.4's note). Under the store's
//! hard block, a line that would take a product below zero is refused and the
//! cart is unchanged. Otherwise the products it would leave short are flagged
//! in the answer, which the screen shows loudly and does not block on.

use std::collections::BTreeSet;

use pos_db::DbError;
use pos_db::repo::parked_cart::ParkedCartRepository;
use pos_db::repo::product::ProductRepository;
use pos_db::repo::shift::ShiftRepository;
use pos_db::repo::stock::StockRepository;
use pos_db::tax_pack::TaxPackRepository;
use pos_db::tax_policy::TaxPolicyRepository;
use pos_domain::cart::{AddLine, add_line, confirm_age, open, price_cart};
use pos_domain::{
    ApprovalBinding, AuditIntent, Authorized, Cart, CartContext, CartError, ClockConfidence,
    Currency, EscalationPolicy, PricedCart, ProductId, Qty, SaleId, SaleLineId,
    ScalePricingRefusal, ScanLookup, Shortfall, StockError, StoreId, StoreTaxProfile, TaxBands,
    Timestamp, UnitOfMeasure, cap,
};
use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use super::{audit, sqlite, stored};
use crate::ipc::context::{authorize, not_signed_in, permission_refused};
use crate::ipc::{IpcError, IpcResult};
use crate::scan::{ScanContext, ScanHandlerError, ScanOutcome, handle_scan};
use crate::state::{Register, Session, Till};

/// The basket as the screen shows it: every number `price_cart` decided, and
/// nothing the screen must compute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CartSnapshot {
    pub sale_id: SaleId,
    /// The currency every `*_minor` is in, by its ISO code.
    pub currency: &'static str,
    pub lines: Vec<LineSnapshot>,
    /// Every line's gross before its discounts.
    pub subtotal_minor: i64,
    pub discount_total_minor: i64,
    pub tax_total_minor: i64,
    /// What the customer owes, before any cash rounding.
    pub total_minor: i64,
    /// The receipt's tax rows.
    pub tax_rows: Vec<TaxRowSnapshot>,
    /// Lines whose buyer's age is still to be confirmed. Payment is refused
    /// until there are none (E.69).
    pub awaiting_age: Vec<SaleLineId>,
    /// Products the basket would leave below zero, which the screen shows
    /// loudly and does not block on.
    pub stock_flags: Vec<StockFlag>,
}

/// One line, priced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineSnapshot {
    pub line_id: SaleLineId,
    /// `None` only for a department sale, which has no product.
    pub product_id: Option<ProductId>,
    /// The name copied onto the line when it was captured (I-5).
    pub name: String,
    pub qty_milli: i64,
    pub unit: UnitOfMeasure,
    /// What the line charges a unit.
    pub unit_price_minor: i64,
    pub gross_minor: i64,
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub line_total_minor: i64,
    pub min_age: Option<u8>,
    pub age_confirmed: bool,
}

/// One row of the receipt's tax summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaxRowSnapshot {
    pub code: String,
    /// The rate in parts per million: 16% is 160 000.
    pub rate_ppm: i64,
    pub net_minor: i64,
    pub tax_minor: i64,
    pub gross_minor: i64,
}

/// A product the basket would leave below zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StockFlag {
    pub product_id: ProductId,
    pub on_hand_milli: i64,
    pub taken_milli: i64,
}

impl CartSnapshot {
    fn of(cart: &Cart, priced: &PricedCart, flags: &[Shortfall]) -> CartSnapshot {
        let lines = cart
            .lines
            .iter()
            .map(|line| {
                let money = priced.lines.iter().find(|p| p.line_id == line.id);
                let minor = |pick: fn(&pos_domain::PricedLine) -> i64| money.map_or(0, pick);
                LineSnapshot {
                    line_id: line.id,
                    product_id: line.product_id,
                    name: line.name_snapshot.clone(),
                    qty_milli: line.qty.milli(),
                    unit: line.unit,
                    unit_price_minor: line.price().minor(),
                    gross_minor: minor(|p| p.gross_before_discount.minor()),
                    discount_minor: minor(|p| {
                        p.discount_attributions
                            .iter()
                            .map(|a| a.amount.minor())
                            .sum::<i64>()
                    }),
                    tax_minor: minor(|p| p.tax.tax_total.minor()),
                    line_total_minor: minor(|p| p.line_total.minor()),
                    min_age: line.min_age,
                    age_confirmed: line.age_confirmed,
                }
            })
            .collect();
        CartSnapshot {
            sale_id: cart.id,
            currency: cart.currency.code(),
            lines,
            subtotal_minor: priced.subtotal.minor(),
            discount_total_minor: priced.discount_total.minor(),
            tax_total_minor: priced.tax_total.minor(),
            total_minor: priced.total.minor(),
            tax_rows: priced
                .tax_summary
                .iter()
                .map(|row| TaxRowSnapshot {
                    code: row.code.clone(),
                    rate_ppm: row.rate.ppm(),
                    net_minor: row.net.minor(),
                    tax_minor: row.tax.minor(),
                    gross_minor: row.gross.minor(),
                })
                .collect(),
            awaiting_age: cart
                .lines
                .iter()
                .filter(|line| line.min_age.is_some() && !line.age_confirmed)
                .map(|line| line.id)
                .collect(),
            stock_flags: flags
                .iter()
                .map(|flag| StockFlag {
                    product_id: flag.product_id,
                    on_hand_milli: flag.on_hand.milli(),
                    taken_milli: flag.taken.milli(),
                })
                .collect(),
        }
    }
}

#[tauri::command]
pub fn cart_open(till: State<'_, Till>) -> IpcResult<CartSnapshot> {
    open_cart(till.register()?)
}

#[tauri::command]
pub fn cart_current(till: State<'_, Till>) -> IpcResult<Option<CartSnapshot>> {
    current(till.register()?)
}

#[tauri::command]
pub fn cart_add_line(
    till: State<'_, Till>,
    product_id: ProductId,
    qty_milli: Option<i64>,
) -> IpcResult<CartSnapshot> {
    add_product(till.register()?, product_id, qty_milli)
}

#[tauri::command]
pub fn cart_add_scan(till: State<'_, Till>, raw_code: String) -> IpcResult<CartSnapshot> {
    let register = till.register()?;
    add_scan(register, &raw_code, register.clock_confidence())
}

#[tauri::command]
pub fn cart_confirm_age(
    till: State<'_, Till>,
    line_id: SaleLineId,
    confirmed: bool,
) -> IpcResult<CartSnapshot> {
    answer_age(till.register()?, line_id, confirmed)
}

/// `cart_open`: the till's working cart, opened on the register's open shift
/// if it holds none. Asking again answers with the same cart, so a second
/// press opens nothing.
///
/// # Errors
///
/// What the context refuses; `shift_not_open` when the register has no open
/// shift, because a sale outside a shift has no business date; the pricing
/// refusals of [`CartSnapshot`], so no sale opens at a store that cannot
/// price one; and `storage_failed`.
pub fn open_cart(register: &Register) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = sale_create(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), &auth)?;
    if let Some(cart) = till.cart.as_ref() {
        return snapshot(&till.db, register.store(), cart);
    }
    let shift = ShiftRepository::new(&till.db)
        .current(register.register())
        .map_err(stored)?
        .ok_or_else(|| IpcError::new("shift_not_open", "error.shift.not_open"))?;
    // The register sells in its store's currency, which `0003` holds to an
    // exponent of three and no step provisions as anything but JOD.
    let cart = open(
        SaleId::from_uuid(register.next_id()),
        &CartContext {
            register_id: register.register(),
            shift_id: shift.id,
            cashier_id: auth.actor(),
            currency: Currency::JOD,
            is_training: false,
            opened_at: at,
        },
    );
    let answer = snapshot(&till.db, register.store(), &cart)?;
    let tx = till.db.transaction().map_err(sqlite)?;
    ParkedCartRepository::new(&tx)
        .hold(&tx, &cart, register.process_nonce())
        .map_err(stored)?;
    tx.commit().map_err(sqlite)?;
    *till.cart = Some(cart);
    Ok(answer)
}

/// `cart_current`: the till's working cart, priced, or `None` when it holds
/// none.
///
/// # Errors
///
/// What the context refuses, the pricing refusals of [`CartSnapshot`], and
/// `storage_failed`.
pub fn current(register: &Register) -> IpcResult<Option<CartSnapshot>> {
    let auth = sale_create(register, register.now())?;
    let till = register.lock_till();
    signed_in_as(till.session.as_ref(), &auth)?;
    till.cart
        .as_ref()
        .map(|cart| snapshot(&till.db, register.store(), cart))
        .transpose()
}

/// `cart_add_line`: `qty_milli` of `product` at its catalogue price, one unit
/// when no quantity is given.
///
/// # Errors
///
/// What the context refuses; `cart_not_open`; `product_unknown`; the cart's
/// own refusals, such as `product_inactive`, `qty_not_whole` or
/// `price_above_ceiling`; `stock_would_go_negative` under the store's hard
/// block; the pricing refusals; and `storage_failed`.
pub fn add_product(
    register: &Register,
    product: ProductId,
    qty_milli: Option<i64>,
) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = sale_create(register, at)?;
    let line_id = SaleLineId::from_uuid(register.next_id());
    change(register, &auth, Screen::Refuse, |conn, cart| {
        let product = ProductRepository::new(conn)
            .by_id(product)
            .map_err(stored)?
            .ok_or_else(|| IpcError::new("product_unknown", "error.cart.product_unknown"))?;
        let qty = qty_milli.map_or(Qty::ONE, Qty::from_milli);
        let line = AddLine::from_product(line_id, &product, qty).map_err(|e| cart_refused(&e))?;
        Ok((add_line(cart, line).map_err(|e| cart_refused(&e))?, None))
    })
}

/// `cart_add_scan`: the line `raw_code` resolves to: a product by its barcode
/// or PLU at the code's pack quantity, a weight label's weight, or a price
/// label's package at the label's price. `clock` is how far the register
/// trusts its clock, which a scale label answers to.
///
/// # Errors
///
/// What the context refuses; `cart_not_open`; one refusal per scan that adds
/// no line, each its own code so the screen can say what to do:
/// `scan_unknown` (offer the department sale), `scan_ambiguous` (the cashier
/// chooses), `scan_checksum_failed` and `scan_unreadable` (scan again),
/// `scan_scale_refused` and `scan_layout_switched_off` (a commissioning
/// matter, never a quick-add); then what `cart_add_line` refuses after the
/// product is found; and `scan_failed` when the lookup itself fails.
pub fn add_scan(
    register: &Register,
    raw_code: &str,
    clock: ClockConfidence,
) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = sale_create(register, at)?;
    let line_id = SaleLineId::from_uuid(register.next_id());
    let context = ScanContext {
        store: *register.store().as_uuid().as_bytes(),
        currency: Currency::JOD,
        now: at,
        clock,
    };
    change(register, &auth, Screen::Refuse, |conn, cart| {
        let found = scanned(conn, &context, raw_code)?;
        let line = AddLine::from_scan(line_id, found).map_err(|e| cart_refused(&e))?;
        Ok((add_line(cart, line).map_err(|e| cart_refused(&e))?, None))
    })
}

/// `cart_confirm_age`: an operator's answer for the age-restricted line
/// `line_id`. Yes unblocks it; no removes it. Either is audited, `age.confirm`
/// or `age.decline`, in the transaction that saves the cart.
///
/// # Errors
///
/// What the context refuses; `cart_not_open`; `line_unknown`; `age_not_pending`
/// for a line with no age to confirm, or one already answered; the pricing
/// refusals; and `storage_failed`.
pub fn answer_age(
    register: &Register,
    line_id: SaleLineId,
    confirmed: bool,
) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = sale_create(register, at)?;
    let actor = auth.actor();
    change(register, &auth, Screen::Flag, |_, cart| {
        let (cart, intent) =
            confirm_age(cart, line_id, confirmed, actor, at).map_err(|e| cart_refused(&e))?;
        Ok((cart, Some(intent)))
    })
}

/// Whether a stock shortfall refuses the change, or is only flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    /// A line is being added: the store's hard block refuses it.
    Refuse,
    /// Nothing is being added: a shortfall is shown, never refused.
    Flag,
}

/// Change the till's working cart with `work`, which reads the database and
/// returns the cart it becomes and any audit intent. The change is screened
/// and priced before anything is written; then the saved snapshot and the
/// audit row commit together, and only then does the till hold the new cart.
fn change(
    register: &Register,
    auth: &Authorized<cap::SaleCreate>,
    screen: Screen,
    work: impl FnOnce(&Connection, Cart) -> IpcResult<(Cart, Option<AuditIntent>)>,
) -> IpcResult<CartSnapshot> {
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth)?;
    let working = till
        .cart
        .clone()
        .ok_or_else(|| IpcError::new("cart_not_open", "error.cart.not_open"))?;
    let (next, intent) = work(&till.db, working)?;
    let flags = stock_flags(&till.db, register.store(), &next, screen)?;
    let priced = priced(&till.db, register.store(), &next)?;
    let tx = till.db.transaction().map_err(sqlite)?;
    ParkedCartRepository::new(&tx)
        .save_active(&tx, register.process_nonce(), &next)
        .map_err(stored)?;
    if let Some(intent) = intent {
        audit(register, &tx, &intent)?;
    }
    tx.commit().map_err(sqlite)?;
    let answer = CartSnapshot::of(&next, &priced, &flags);
    *till.cart = Some(next);
    Ok(answer)
}

/// `cart`, screened and priced.
fn snapshot(conn: &Connection, store: StoreId, cart: &Cart) -> IpcResult<CartSnapshot> {
    let flags = stock_flags(conn, store, cart, Screen::Flag)?;
    let priced = priced(conn, store, cart)?;
    Ok(CartSnapshot::of(cart, &priced, &flags))
}

/// The proof every command here takes. `sale.create` is held by every role
/// that sells and escalates under no policy, so no approval is ever asked for.
fn sale_create(register: &Register, at: Timestamp) -> IpcResult<Authorized<cap::SaleCreate>> {
    let unbound = ApprovalBinding {
        entity_id: Uuid::nil(),
        amount_minor: 0,
        content_hash: None,
    };
    authorize::<cap::SaleCreate>(register, None, &unbound, &EscalationPolicy::empty(), at)
}

/// Refuse the change when the proof is not the signed-in user's: the session
/// ended, or changed hands, between the proof and the lock. The till's cart
/// is always the signed-in cashier's, because every sign-out sets it aside
/// under the same lock.
fn signed_in_as(session: Option<&Session>, auth: &Authorized<cap::SaleCreate>) -> IpcResult<()> {
    match session {
        Some(session) if session.user == auth.actor() => Ok(()),
        _ => Err(not_signed_in()),
    }
}

/// What `cart` would leave below zero at `store`, per line as each line moves
/// stock: its quantity, or a scale label's weight. Under the store's hard
/// block a shortfall refuses a line being added; otherwise it is a flag.
fn stock_flags(
    conn: &Connection,
    store: StoreId,
    cart: &Cart,
    screen: Screen,
) -> IpcResult<Vec<Shortfall>> {
    let taken: Vec<(ProductId, Qty)> = cart
        .lines
        .iter()
        .filter_map(|line| {
            let moved = line.label.as_ref().map_or(line.qty, |weight| weight.qty);
            line.product_id.map(|product| (product, moved))
        })
        .collect();
    match StockRepository::new(conn).screen_basket(store, &taken) {
        Ok(flags) => Ok(flags),
        Err(DbError::StockRefused(StockError::WouldGoNegative(flags))) => match screen {
            Screen::Flag => Ok(flags),
            Screen::Refuse => Err(IpcError::new(
                "stock_would_go_negative",
                "error.stock.would_go_negative",
            )),
        },
        Err(other) => Err(stored(other)),
    }
}

/// `cart` priced under `store`'s settings: its price mode, its tax profile and
/// its approved policy's rule, with the components its pack charges on each
/// category the cart holds on the shift's business date. A store not
/// registered for GST charges no tax and needs no band.
fn priced(conn: &Connection, store: StoreId, cart: &Cart) -> IpcResult<PricedCart> {
    let settings = TaxPolicyRepository::new(conn)
        .pricing_settings(store)
        .map_err(|e| pricing_refused(&e))?;
    let mut bands = TaxBands::new();
    if settings.profile != StoreTaxProfile::Unregistered {
        let day = ShiftRepository::new(conn)
            .by_id(cart.shift_id)
            .map_err(stored)?
            .ok_or_else(|| IpcError::new("shift_not_open", "error.shift.not_open"))?
            .business_date;
        let pack = TaxPackRepository::new(conn);
        let categories: BTreeSet<_> = cart.lines.iter().map(|line| line.tax_category_id).collect();
        for category in categories {
            let components = pack
                .components_for(store, category, day)
                .map_err(|e| pricing_refused(&e))?;
            bands.insert(category, components);
        }
    }
    price_cart(cart, &bands, &settings).map_err(|e| cart_refused(&e))
}

/// What a scan resolved to, when it is one product; every other answer is
/// its own refusal.
fn scanned(conn: &Connection, context: &ScanContext, raw_code: &str) -> IpcResult<ScanLookup> {
    let outcome = handle_scan(conn, context, raw_code.as_bytes()).map_err(|e| scan_failed(&e))?;
    match outcome {
        ScanOutcome::Lookup(lookup) => match *lookup {
            ScanLookup::Unknown { .. } => Err(IpcError::new("scan_unknown", "error.scan.unknown")),
            ScanLookup::Ambiguous { .. } => {
                Err(IpcError::new("scan_ambiguous", "error.scan.ambiguous"))
            }
            ScanLookup::ChecksumFailed { .. } => Err(IpcError::new(
                "scan_checksum_failed",
                "error.scan.checksum_failed",
            )),
            found @ (ScanLookup::Product { .. }
            | ScanLookup::WeightEmbedded { .. }
            | ScanLookup::PriceEmbedded { .. }) => Ok(found),
        },
        ScanOutcome::ScaleRefused { refusal, .. } => Err(IpcError::new(
            "scan_scale_refused",
            "error.scan.scale_refused",
        )
        .with_detail(scale_refusal(refusal))),
        ScanOutcome::LayoutSwitchedOff { .. } => Err(IpcError::new(
            "scan_layout_switched_off",
            "error.scan.layout_switched_off",
        )),
        ScanOutcome::Unreadable(_) => {
            Err(IpcError::new("scan_unreadable", "error.scan.unreadable"))
        }
    }
}

/// Why a scale's labels may not price a line, as the operator is told it.
const fn scale_refusal(refusal: ScalePricingRefusal) -> &'static str {
    match refusal {
        ScalePricingRefusal::NoEvidence => "no verification evidence is recorded for this scale",
        ScalePricingRefusal::Revoked => "this scale's verification was revoked",
        ScalePricingRefusal::MaintenancePending => {
            "this scale is awaiting maintenance or reverification"
        }
        ScalePricingRefusal::NotYetEffective(_) => "this scale's verification is not yet effective",
        ScalePricingRefusal::Expired(_) => "this scale's verification has expired",
        ScalePricingRefusal::ClockNotTrusted => {
            "the register's clock is not trusted, so the scale's verification cannot be judged"
        }
    }
}

fn scan_failed(error: &ScanHandlerError) -> IpcError {
    IpcError::from_source("scan_failed", "error.scan.failed", error)
}

/// A store that cannot price a cart, as the screen is told it. The source,
/// which names the store, goes to the scrubbed log.
fn pricing_refused(error: &DbError) -> IpcError {
    let unavailable = |detail: &'static str| {
        IpcError::from_source("pricing_unavailable", "error.pricing.unavailable", error)
            .with_detail(detail)
    };
    match error {
        DbError::TaxPolicyStoreUnknown(_) | DbError::TaxPackStoreUnknown(_) => {
            unavailable("this register holds no such store")
        }
        DbError::TaxPolicyMissing(_) | DbError::TaxPolicyUnapproved { .. } => {
            unavailable("the store's tax computation policy is not approved")
        }
        DbError::TaxPackMissing(_) | DbError::TaxPackUnapproved(_) => {
            unavailable("the store's tax pack is not approved")
        }
        DbError::TaxBandUnconfigured { .. } => {
            IpcError::from_source("tax_band_missing", "error.pricing.tax_band_missing", error)
        }
        _ => IpcError::from_source("storage_failed", "error.storage.failed", error),
    }
}

/// A refusal of the cart's own, as the screen is told it: a code to branch
/// on and a key to translate, never the error's text, which names products
/// and amounts. A refusal these commands cannot meet, such as a tender's, is
/// `cart_refused`; its source is in the scrubbed log under the trace id.
fn cart_refused(error: &CartError) -> IpcError {
    let (code, key) = match error {
        CartError::Permission(refusal) => return permission_refused(refusal),
        CartError::ProductInactive(_) => ("product_inactive", "error.cart.product_inactive"),
        CartError::NonPositiveQty => ("qty_not_positive", "error.cart.qty_not_positive"),
        CartError::FractionalQtyForDiscrete(..) => ("qty_not_whole", "error.cart.qty_not_whole"),
        CartError::AboveMaxPrice(..) => ("price_above_ceiling", "error.cart.price_above_ceiling"),
        CartError::ScanNotOneProduct => ("scan_not_one_product", "error.scan.not_one_product"),
        CartError::LineNotFound(_) => ("line_unknown", "error.cart.line_unknown"),
        CartError::AgeConfirmationNotPending(_) => {
            ("age_not_pending", "error.cart.age_not_pending")
        }
        CartError::TaxBandMissing(_) => ("tax_band_missing", "error.pricing.tax_band_missing"),
        _ => ("cart_refused", "error.cart.refused"),
    };
    IpcError::from_source(code, key, error)
}
