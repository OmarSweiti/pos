//! Building a basket, and setting it aside: `cart_open`, `cart_current`,
//! `cart_add_line`, `cart_add_scan` and `cart_confirm_age`, then `cart_park`,
//! `cart_list_parked` and `cart_resume`, then `cart_override_price`
//! (`ref/ipc-contract.md` §3, "Cart"; microstep 1.4.11, its first three
//! parts).
//!
//! **No base command here takes a price.** A line's price comes from the
//! catalogue, through `AddLine::from_product`, or from a label the scan
//! handler resolved, through `AddLine::from_scan`. `PriceSource` cannot be
//! built from an integer, so nothing the webview sends can become a line's
//! captured price (`ref/ipc-contract.md` §1, rule 6). The one command that
//! takes a price is `cart_override_price`, under `price.override`, which the
//! grid gives a shift lead and a manager and never a cashier. Every override
//! is audited with its reason, a code from the store's list, and the line
//! keeps the price it was captured at beside the price it now charges (I-5).
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
//! **A basket being paid for belongs to the tender commands** (1.8.3). While
//! it is, every command here that would change the basket, open another or
//! set it aside is refused `sale_being_paid`, and `cart_current` answers it at
//! the prices it went to payment with.
//!
//! **Stock is screened as a line is added** (1.10.4's note). Under the store's
//! hard block, a line that would take a product below zero is refused and the
//! cart is unchanged. Otherwise the products it would leave short are flagged
//! in the answer, which the screen shows loudly and does not block on.
//!
//! **A basket set aside comes back as it was.** `cart_park` moves the till's
//! basket to the parked list, under an optional label, and leaves the till
//! free for the next customer. `cart_list_parked` lists the baskets that can
//! still be resumed on the shift's business day, and `cart_resume` makes one
//! the till's working cart again, at the prices its lines were captured at
//! (I-5). A resume onto a till that already holds a basket sets that basket
//! aside first, in the same transaction (the owner's decision of 6 November
//! 2026), so no resume loses one. Taking over another cashier's basket is
//! audited, with its delivery envelope, in the transaction that claims it
//! (#363's decision).

use std::collections::BTreeSet;

use pos_db::DbError;
use pos_db::repo::parked_cart::ParkedCartRepository;
use pos_db::repo::product::ProductRepository;
use pos_db::repo::setting::SettingRepository;
use pos_db::repo::shift::ShiftRepository;
use pos_db::repo::stock::StockRepository;
use pos_db::repo::user::UserRepository;
use pos_db::tax_pack::TaxPackRepository;
use pos_db::tax_policy::TaxPolicyRepository;
use pos_domain::cart::{AddLine, add_line, confirm_age, open, override_price, park, price_cart};
use pos_domain::{
    ApprovalBinding, ApprovalId, AuditIntent, Authorized, Capability, Cart, CartContext, CartError,
    ClockConfidence, Currency, EscalationPolicy, Money, OverrideReason, PricedCart,
    PricingSettings, ProductId, Qty, SaleId, SaleLineId, ScalePricingRefusal, ScanLookup, Shift,
    Shortfall, StockError, StoreId, StoreTaxProfile, TaxBands, Tendering, Timestamp, UnitOfMeasure,
    UserId, cap,
};
use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use super::{audit, sqlite, stored};
use crate::ipc::context::{authorize, merchant_policy, not_signed_in, permission_refused};
use crate::ipc::{IpcError, IpcResult};
use crate::scan::{ScanContext, ScanHandlerError, ScanOutcome, handle_scan};
use crate::state::{Register, Session, Till, Working};

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
    /// What the line charges a unit: the price it was captured at, or the
    /// price an override set.
    pub unit_price_minor: i64,
    /// The price the line was captured at (I-5), which an override leaves
    /// beside the price it sets.
    pub captured_price_minor: i64,
    /// Why the line's price was overridden, by its reason code, when it was.
    pub override_reason: Option<String>,
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

/// A basket in the parked list, as the resume list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParkedRef {
    /// The id `cart_resume` takes, which is its sale's id.
    pub parked_id: SaleId,
    /// The label the cashier gave it, if any.
    pub label: Option<String>,
    /// The cashier who set it aside, and their name as the lock screen shows
    /// it.
    pub parked_by: UserId,
    pub parked_by_name: String,
    /// When it was set aside, in milliseconds since the Unix epoch.
    pub parked_at: Timestamp,
    pub line_count: usize,
    /// The currency `total_minor` is in, by its ISO code.
    pub currency: &'static str,
    /// What it comes to, priced as `cart_resume` will show it.
    pub total_minor: i64,
}

/// The longest label a parked basket takes, in characters: enough to name a
/// customer's basket, and one line of a slip.
pub const PARK_LABEL_MAX_CHARS: usize = 40;

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
                    captured_price_minor: line.unit_price.minor(),
                    override_reason: line
                        .price_override
                        .as_ref()
                        .map(|over| over.reason.code().to_owned()),
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
            stock_flags: flags.iter().map(StockFlag::from).collect(),
        }
    }
}

impl From<&Shortfall> for StockFlag {
    fn from(flag: &Shortfall) -> StockFlag {
        StockFlag {
            product_id: flag.product_id,
            on_hand_milli: flag.on_hand.milli(),
            taken_milli: flag.taken.milli(),
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

#[tauri::command]
pub fn cart_override_price(
    till: State<'_, Till>,
    line_id: SaleLineId,
    unit_price_minor: i64,
    reason: String,
    approval_id: Option<ApprovalId>,
) -> IpcResult<CartSnapshot> {
    override_line_price(
        till.register()?,
        &PriceChange {
            line_id,
            unit_price_minor,
            reason: &reason,
            approval: approval_id,
        },
        &merchant_policy(),
    )
}

#[tauri::command]
pub fn cart_park(till: State<'_, Till>, label: Option<String>) -> IpcResult<ParkedRef> {
    park_cart(till.register()?, label.as_deref())
}

#[tauri::command]
pub fn cart_list_parked(till: State<'_, Till>) -> IpcResult<Vec<ParkedRef>> {
    list_parked(till.register()?)
}

#[tauri::command]
pub fn cart_resume(till: State<'_, Till>, parked_id: SaleId) -> IpcResult<CartSnapshot> {
    resume_cart(till.register()?, parked_id)
}

/// `cart_open`: the till's working cart, opened on the register's open shift
/// if it holds none. Asking again answers with the same cart, so a second
/// press opens nothing.
///
/// # Errors
///
/// What the context refuses; `sale_being_paid` while the till's basket is
/// being paid for; `shift_not_open` when the register has no open shift,
/// because a sale outside a shift has no business date; the pricing
/// refusals of [`CartSnapshot`], so no sale opens at a store that cannot
/// price one; and `storage_failed`.
pub fn open_cart(register: &Register) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = proof::<cap::SaleCreate>(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    if till.working.is_some() {
        let cart = building(till.working.as_ref())?;
        return snapshot(&till.db, register.store(), cart);
    }
    let shift = open_shift(&till.db, register)?;
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
    *till.working = Some(Working::Building(Box::new(cart)));
    Ok(answer)
}

/// `cart_current`: the till's working cart, priced, or `None` when it holds
/// none. A basket being paid for is answered at the prices it went to payment
/// with, which are the ones its tenders pay.
///
/// # Errors
///
/// What the context refuses, the pricing refusals of [`CartSnapshot`], and
/// `storage_failed`.
pub fn current(register: &Register) -> IpcResult<Option<CartSnapshot>> {
    let auth = proof::<cap::SaleCreate>(register, register.now())?;
    let till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    match till.working.as_ref() {
        None => Ok(None),
        Some(Working::Building(cart)) => snapshot(&till.db, register.store(), cart).map(Some),
        Some(Working::Paying(paying)) => {
            paid_for(&till.db, register.store(), &paying.tendering).map(Some)
        }
    }
}

/// `cart_add_line`: `qty_milli` of `product` at its catalogue price, one unit
/// when no quantity is given.
///
/// # Errors
///
/// What the context refuses; `cart_not_open`; `sale_being_paid`;
/// `product_unknown`; the cart's own refusals, such as `product_inactive`,
/// `qty_not_whole` or `price_above_ceiling`; `stock_would_go_negative` under
/// the store's hard block; the pricing refusals; and `storage_failed`.
pub fn add_product(
    register: &Register,
    product: ProductId,
    qty_milli: Option<i64>,
) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = proof::<cap::SaleCreate>(register, at)?;
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
/// What the context refuses; `cart_not_open`; `sale_being_paid`; one refusal
/// per scan that adds no line, each its own code so the screen can say what
/// to do: `scan_unknown` (offer the department sale), `scan_ambiguous` (the
/// cashier chooses), `scan_checksum_failed` and `scan_unreadable` (scan
/// again), `scan_scale_refused` and `scan_layout_switched_off` (a
/// commissioning matter, never a quick-add); then what `cart_add_line`
/// refuses after the product is found; and `scan_failed` when the lookup
/// itself fails.
pub fn add_scan(
    register: &Register,
    raw_code: &str,
    clock: ClockConfidence,
) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = proof::<cap::SaleCreate>(register, at)?;
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
/// What the context refuses; `cart_not_open`; `sale_being_paid`;
/// `line_unknown`; `age_not_pending` for a line with no age to confirm, or one
/// already answered; the pricing refusals; and `storage_failed`.
pub fn answer_age(
    register: &Register,
    line_id: SaleLineId,
    confirmed: bool,
) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = proof::<cap::SaleCreate>(register, at)?;
    let actor = auth.actor();
    change(register, &auth, Screen::Flag, |_, cart| {
        let (cart, intent) =
            confirm_age(cart, line_id, confirmed, actor, at).map_err(|e| cart_refused(&e))?;
        Ok((cart, Some(intent)))
    })
}

/// What `cart_override_price` asks for: the line, the price it is to charge
/// a unit, the reason by its code, and the approval when one is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceChange<'r> {
    pub line_id: SaleLineId,
    pub unit_price_minor: i64,
    pub reason: &'r str,
    pub approval: Option<ApprovalId>,
}

/// `cart_override_price`: the price `line_id` charges a unit, set to
/// `unit_price_minor` for `reason`, under the signed-in user's
/// `price.override`. Audited, in the transaction that saves the cart.
///
/// - **The reason is a code from the store's list** (merchant decision 3.7),
///   never free text: free text is unreportable, and a customer's name typed
///   there would stay in the audit chain for good.
/// - **The floor is the product's cost** (merchant decision 3.4), read as
///   `StockPosition::cost_basis`, so a product never costed has no cost and
///   may not be lowered. A displayed price wins over it, and nothing passes
///   the ministry ceiling. The domain's `override_price` holds all three.
/// - **Where `policy` escalates `price.override`** the approval must be bound
///   to this line and this price, and it is spent with the audit row. The
///   register's policy escalates nothing yet ([`merchant_policy`]).
/// - **A displayed price asks for the product's label to be printed again**,
///   and `label_reprint_queue` arrives with migration `0014` (4.6.2). Until
///   then the audit row, with the reason `displayed_price`, is the record
///   that worklist will read.
///
/// # Errors
///
/// - What the context refuses: `permission_denied` for a role without
///   `price.override`, and under an escalating policy `approval_required`,
///   `approval_unknown`, `approval_used`, `approval_mismatch` or
///   `approval_revoked`;
/// - `cart_not_open`, `sale_being_paid`, `line_unknown` and
///   `override_reason_unknown`;
/// - `setting_invalid` when the store's list cannot be read;
/// - the line's own: `price_label_not_overridable`,
///   `department_price_not_overridable`, `price_negative`,
///   `amount_discount_fixes_price`, `price_above_ceiling`,
///   `displayed_price_not_lower`, `price_no_recorded_cost` and
///   `price_below_floor`;
/// - the pricing refusals, and `storage_failed`.
pub fn override_line_price(
    register: &Register,
    change_to: &PriceChange<'_>,
    policy: &EscalationPolicy,
) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let binding = ApprovalBinding {
        entity_id: change_to.line_id.as_uuid(),
        amount_minor: change_to.unit_price_minor,
        content_hash: None,
    };
    let auth = authorize::<cap::PriceOverride>(register, change_to.approval, &binding, policy, at)?;
    let store = register.store();
    change(register, &auth, Screen::Flag, |conn, cart| {
        let reason = override_reason(conn, store, change_to.reason)?;
        let product = cart
            .lines
            .iter()
            .find(|line| line.id == change_to.line_id)
            .and_then(|line| line.product_id);
        let cost = product
            .map(|product| StockRepository::new(conn).position(product, store))
            .transpose()
            .map_err(stored)?
            .and_then(|position| position.cost_basis());
        let to = Money::from_minor(change_to.unit_price_minor, cart.currency);
        // A displayed price's label reprint has nowhere to go before `0014`;
        // the audit row names the reason it would have been queued for.
        let (cart, intent, _reprint) =
            override_price(cart, change_to.line_id, to, reason, cost, &auth)
                .map_err(|e| cart_refused(&e))?;
        Ok((cart, Some(intent)))
    })
}

/// `cart_park`: set the till's basket aside under `label`, as it is, and
/// leave the till free for the next customer. The basket keeps the business
/// day it was opened in, and expires when that day ends.
///
/// # Errors
///
/// What the context refuses; `park_label_invalid` for a label longer than
/// [`PARK_LABEL_MAX_CHARS`] or holding a control character; `cart_not_open`;
/// `sale_being_paid`, because a parked basket carries no tender;
/// `nothing_to_park` for a basket with no line, which stays on the till; the
/// pricing refusals of [`CartSnapshot`]; and `storage_failed`. A refused park
/// leaves the basket on the till.
pub fn park_cart(register: &Register, label: Option<&str>) -> IpcResult<ParkedRef> {
    let at = register.now();
    let auth = proof::<cap::SalePark>(register, at)?;
    let label = park_label(label)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let working = building(till.working.as_ref())?.clone();
    let parked = park(working).map_err(|e| cart_refused(&e))?;
    let answer = parked_ref(
        &till.db,
        register.store(),
        Listed {
            id: parked.cart().id,
            label: label.clone(),
            parked_by: parked.cart().cashier_id,
            parked_at: at,
            cart: parked.cart(),
        },
    )?;
    let tx = till.db.transaction().map_err(sqlite)?;
    ParkedCartRepository::new(&tx)
        .repark(&tx, register.process_nonce(), &parked, label.as_deref(), at)
        .map_err(stored)?;
    tx.commit().map_err(sqlite)?;
    *till.working = None;
    Ok(answer)
}

/// `cart_list_parked`: the baskets parked at this register that can still be
/// resumed on the open shift's business day, oldest first. One parked on an
/// earlier day has expired and is not offered (master plan C.2).
///
/// # Errors
///
/// What the context refuses; `shift_not_open`, because nothing can be
/// resumed outside a shift; the pricing refusals of [`CartSnapshot`]; and
/// `storage_failed`.
pub fn list_parked(register: &Register) -> IpcResult<Vec<ParkedRef>> {
    let auth = proof::<cap::SaleResume>(register, register.now())?;
    let till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let shift = open_shift(&till.db, register)?;
    ParkedCartRepository::new(&till.db)
        .parked(register.register(), shift.business_date)
        .map_err(stored)?
        .iter()
        .map(|entry| {
            parked_ref(
                &till.db,
                register.store(),
                Listed {
                    id: entry.id,
                    label: entry.label.clone(),
                    parked_by: entry.parked_by,
                    parked_at: entry.parked_at,
                    cart: entry.cart.cart(),
                },
            )
        })
        .collect()
}

/// `cart_resume`: make the parked basket `parked_id` the till's working cart,
/// on the open shift and under the signed-in cashier, at the prices its lines
/// were captured at. A basket the till already holds is set aside first, in
/// the same transaction: parked if it has a line, dropped if not. Taking over
/// another cashier's basket writes `sale.resume` with its envelope in that
/// transaction; a cashier resuming their own writes nothing.
///
/// # Errors
///
/// What the context refuses; `sale_being_paid` while the till's basket is
/// being paid for, which cannot be set aside; `shift_not_open`;
/// `parked_cart_unknown` for no such basket parked at this register;
/// `parked_cart_claimed` for one a till is working on; `parked_cart_expired`
/// for one parked on an earlier business day; `training_mode_mismatch`; the
/// pricing refusals of [`CartSnapshot`]; and `storage_failed`. A refused
/// resume changes nothing: the till keeps its basket, and the parked one stays
/// parked.
pub fn resume_cart(register: &Register, parked_id: SaleId) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = proof::<cap::SaleResume>(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let held = till
        .working
        .as_ref()
        .map(|working| building(Some(working)))
        .transpose()?;
    let shift = open_shift(&till.db, register)?;
    let context = CartContext {
        register_id: register.register(),
        shift_id: shift.id,
        cashier_id: auth.actor(),
        currency: Currency::JOD,
        is_training: false,
        opened_at: at,
    };
    let nonce = register.process_nonce();
    let tx = till.db.transaction().map_err(sqlite)?;
    let carts = ParkedCartRepository::new(&tx);
    if let Some(working) = held {
        carts.set_aside(&tx, nonce, working, at).map_err(stored)?;
    }
    let resumed = carts
        .resume(&tx, parked_id, &context, &auth, nonce)
        .map_err(|e| resume_refused(&e))?;
    let answer = snapshot(&tx, register.store(), &resumed.cart)?;
    if let Some(intent) = resumed.audit.as_ref() {
        audit(register, &tx, intent)?;
    }
    tx.commit().map_err(sqlite)?;
    *till.working = Some(Working::Building(Box::new(resumed.cart)));
    Ok(answer)
}

/// Whether a stock shortfall refuses the change, or is only flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Screen {
    /// A line is being added, or payment begins (1.10.4's note): the store's
    /// hard block refuses it.
    Refuse,
    /// Nothing is being added: a shortfall is shown, never refused.
    Flag,
}

/// Change the till's working cart with `work`, which reads the database and
/// returns the cart it becomes and any audit intent. The change is screened
/// and priced before anything is written; then the saved snapshot and the
/// audit row commit together, and only then does the till hold the new cart.
fn change<C: Capability>(
    register: &Register,
    auth: &Authorized<C>,
    screen: Screen,
    work: impl FnOnce(&Connection, Cart) -> IpcResult<(Cart, Option<AuditIntent>)>,
) -> IpcResult<CartSnapshot> {
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let working = building(till.working.as_ref())?.clone();
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
    *till.working = Some(Working::Building(Box::new(next)));
    Ok(answer)
}

/// The basket the till is building. A basket being paid for changes only
/// through the tender commands, until it goes back to the cart.
///
/// # Errors
///
/// `cart_not_open` when the till holds nothing, and `sale_being_paid` when its
/// basket is being paid for.
pub(super) fn building(working: Option<&Working>) -> IpcResult<&Cart> {
    match working {
        Some(Working::Building(cart)) => Ok(cart),
        Some(Working::Paying(_)) => Err(IpcError::new(
            "sale_being_paid",
            "error.cart.sale_being_paid",
        )),
        None => Err(IpcError::new("cart_not_open", "error.cart.not_open")),
    }
}

/// `cart`, screened and priced.
pub(super) fn snapshot(conn: &Connection, store: StoreId, cart: &Cart) -> IpcResult<CartSnapshot> {
    let flags = stock_flags(conn, store, cart, Screen::Flag)?;
    let priced = priced(conn, store, cart)?;
    Ok(CartSnapshot::of(cart, &priced, &flags))
}

/// A basket being paid for, screened, at the prices it went to payment with.
pub(super) fn paid_for(
    conn: &Connection,
    store: StoreId,
    tendering: &Tendering,
) -> IpcResult<CartSnapshot> {
    let flags = stock_flags(conn, store, tendering.cart(), Screen::Flag)?;
    Ok(CartSnapshot::of(
        tendering.cart(),
        tendering.priced(),
        &flags,
    ))
}

/// What the resume list says of a parked basket.
struct Listed<'c> {
    id: SaleId,
    label: Option<String>,
    parked_by: UserId,
    parked_at: Timestamp,
    cart: &'c Cart,
}

/// `listed` as the resume list shows it: who parked it, by name, and what it
/// comes to. Every line was priced as it was added, on the same business day
/// and under settings the schema keeps approved, so a parked basket prices
/// as it did.
fn parked_ref(conn: &Connection, store: StoreId, listed: Listed<'_>) -> IpcResult<ParkedRef> {
    let parked_by_name = UserRepository::new(conn)
        .by_id(listed.parked_by)
        .map_err(stored)?
        .ok_or_else(|| IpcError::new("storage_failed", "error.storage.failed"))?
        .display_name;
    let total_minor = priced(conn, store, listed.cart)?.total.minor();
    Ok(ParkedRef {
        parked_id: listed.id,
        label: listed.label,
        parked_by: listed.parked_by,
        parked_by_name,
        parked_at: listed.parked_at,
        line_count: listed.cart.lines.len(),
        currency: listed.cart.currency.code(),
        total_minor,
    })
}

/// `reason` as an override's reason: one of the codes the store lists for
/// it, or its default list (merchant decision 3.7). Matched exactly, so a
/// reason is never read as a code it only resembles.
fn override_reason(conn: &Connection, store: StoreId, reason: &str) -> IpcResult<OverrideReason> {
    let listed = SettingRepository::new(conn)
        .price_override_reasons(store)
        .map_err(|error| match error {
            DbError::StoredSettingInvalid { .. } => {
                IpcError::from_source("setting_invalid", "error.setting.invalid", &error)
                    .with_detail("the store's list of override reasons cannot be read")
            }
            other => stored(other),
        })?;
    if listed.iter().any(|code| code == reason) {
        Ok(OverrideReason::from(reason.to_owned()))
    } else {
        Err(IpcError::new(
            "override_reason_unknown",
            "error.cart.override_reason_unknown",
        ))
    }
}

/// A parked basket's label, trimmed; a blank one is no label. It is a name
/// for the basket, so it is short and holds no control character, which
/// would let one label pass for another on the screen.
fn park_label(label: Option<&str>) -> IpcResult<Option<String>> {
    let Some(label) = label.map(str::trim).filter(|label| !label.is_empty()) else {
        return Ok(None);
    };
    if label.chars().count() > PARK_LABEL_MAX_CHARS || label.chars().any(char::is_control) {
        return Err(
            IpcError::new("park_label_invalid", "error.cart.park_label_invalid")
                .with_detail("the label is too long, or holds a control character"),
        );
    }
    Ok(Some(label.to_owned()))
}

/// A resume the repository refused, as the screen is told it. A basket
/// parked at another register is not this register's to offer, so it is
/// answered as unknown here.
fn resume_refused(error: &DbError) -> IpcError {
    match error {
        DbError::ParkedCartNotFound(_)
        | DbError::ResumeRefused(CartError::ParkedOnAnotherRegister(_)) => {
            IpcError::from_source("parked_cart_unknown", "error.cart.parked_unknown", error)
        }
        DbError::ParkedCartClaimed(_) => {
            IpcError::from_source("parked_cart_claimed", "error.cart.parked_claimed", error)
        }
        DbError::ParkedCartExpired { .. } => {
            IpcError::from_source("parked_cart_expired", "error.cart.parked_expired", error)
        }
        DbError::ResumeRefused(refusal) => cart_refused(refusal),
        other => IpcError::from_source("storage_failed", "error.storage.failed", other),
    }
}

/// The proof a command here takes: `sale.create`, `sale.park` or
/// `sale.resume`. Each is held by every role that sells and escalates under no
/// policy, so no approval is ever asked for.
pub(super) fn proof<C: Capability>(register: &Register, at: Timestamp) -> IpcResult<Authorized<C>> {
    let unbound = ApprovalBinding {
        entity_id: Uuid::nil(),
        amount_minor: 0,
        content_hash: None,
    };
    authorize::<C>(register, None, &unbound, &EscalationPolicy::empty(), at)
}

/// Refuse the command when the proof is not the signed-in user's: the
/// session ended, or changed hands, between the proof and the lock. The
/// till's cart is always the signed-in cashier's, because every sign-out sets
/// it aside under the same lock.
pub(super) fn signed_in_as(session: Option<&Session>, actor: UserId) -> IpcResult<()> {
    match session {
        Some(session) if session.user == actor => Ok(()),
        _ => Err(not_signed_in()),
    }
}

/// The register's open shift: a sale is opened and resumed only in one,
/// because a sale outside a shift has no business date.
fn open_shift(conn: &Connection, register: &Register) -> IpcResult<Shift> {
    ShiftRepository::new(conn)
        .current(register.register())
        .map_err(stored)?
        .ok_or_else(|| IpcError::new("shift_not_open", "error.shift.not_open"))
}

/// What `cart` would leave below zero at `store`, per line as each line moves
/// stock: its quantity, or a scale label's weight. Under the store's hard
/// block a shortfall refuses a line being added; otherwise it is a flag.
pub(super) fn stock_flags(
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

/// `cart` priced under `store`'s settings: see [`pricing`].
fn priced(conn: &Connection, store: StoreId, cart: &Cart) -> IpcResult<PricedCart> {
    let (bands, settings) = pricing(conn, store, cart)?;
    price_cart(cart, &bands, &settings).map_err(|e| cart_refused(&e))
}

/// What `cart` is priced under at `store`: its price mode, its tax profile and
/// its approved policy's rule, with the components its pack charges on each
/// category the cart holds on the shift's business date. A store not
/// registered for GST charges no tax and needs no band.
pub(super) fn pricing(
    conn: &Connection,
    store: StoreId,
    cart: &Cart,
) -> IpcResult<(TaxBands, PricingSettings)> {
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
    Ok((bands, settings))
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
pub(super) fn pricing_refused(error: &DbError) -> IpcError {
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
pub(super) fn cart_refused(error: &CartError) -> IpcError {
    let (code, key) = match error {
        CartError::Permission(refusal) => return permission_refused(refusal),
        CartError::ProductInactive(_) => ("product_inactive", "error.cart.product_inactive"),
        CartError::NonPositiveQty => ("qty_not_positive", "error.cart.qty_not_positive"),
        CartError::FractionalQtyForDiscrete(..) => ("qty_not_whole", "error.cart.qty_not_whole"),
        CartError::AboveMaxPrice(..) => ("price_above_ceiling", "error.cart.price_above_ceiling"),
        CartError::LabelPriceNotOverridable(_) => (
            "price_label_not_overridable",
            "error.cart.price_label_not_overridable",
        ),
        CartError::DepartmentPriceOutsideDepartmentSale => (
            "department_price_not_overridable",
            "error.cart.department_price_not_overridable",
        ),
        CartError::NegativeLine => ("price_negative", "error.cart.price_negative"),
        CartError::AmountDiscountFixesPrice(_) => (
            "amount_discount_fixes_price",
            "error.cart.amount_discount_fixes_price",
        ),
        CartError::DisplayedPriceNotLower(..) => (
            "displayed_price_not_lower",
            "error.cart.displayed_price_not_lower",
        ),
        CartError::NoRecordedCost(..) => (
            "price_no_recorded_cost",
            "error.cart.price_no_recorded_cost",
        ),
        CartError::BelowPriceFloor(..) => ("price_below_floor", "error.cart.price_below_floor"),
        CartError::ScanNotOneProduct => ("scan_not_one_product", "error.scan.not_one_product"),
        CartError::LineNotFound(_) => ("line_unknown", "error.cart.line_unknown"),
        CartError::AgeConfirmationNotPending(_) => {
            ("age_not_pending", "error.cart.age_not_pending")
        }
        CartError::TaxBandMissing(_) => ("tax_band_missing", "error.pricing.tax_band_missing"),
        CartError::NothingToPark => ("nothing_to_park", "error.cart.nothing_to_park"),
        CartError::TrainingModeMismatch(..) => (
            "training_mode_mismatch",
            "error.cart.training_mode_mismatch",
        ),
        CartError::AgeConfirmationRequired(_) => ("age_unconfirmed", "error.cart.age_unconfirmed"),
        CartError::NothingToTender => ("nothing_to_tender", "error.tender.nothing_to_tender"),
        CartError::AlreadySettled(_) => ("sale_settled", "error.tender.sale_settled"),
        CartError::NonPositiveTender(_) => ("tender_not_positive", "error.tender.not_positive"),
        CartError::OvertenderNotCash => ("overtender_not_cash", "error.tender.overtender_not_cash"),
        CartError::Underpaid(..) => ("tender_underpaid", "error.tender.underpaid"),
        CartError::TenderNotFound(_) => ("tender_unknown", "error.tender.unknown"),
        CartError::TenderCollected(_) => ("tender_collected", "error.tender.collected"),
        _ => ("cart_refused", "error.cart.refused"),
    };
    IpcError::from_source(code, key, error)
}
