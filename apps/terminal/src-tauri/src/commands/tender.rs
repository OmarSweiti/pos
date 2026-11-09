//! Paying for a basket: `tender_begin`, `tender_add_cash`, `tender_remove`,
//! `tender_back_to_cart` and `sale_in_flight_state` (`ref/ipc-contract.md`
//! §3, "Tender and finalize"; microstep 1.8.3, its first part).
//!
//! **Payment begins journalled.** `tender_begin` screens the till's basket
//! for stock once more, because another basket may have sold it since (1.10.4's
//! note), prices it under the store's own pack, policy and coin step, and
//! journals the checkout in `checkout_operation` under a fresh idempotency key
//! before any money is taken (1.8.4's note). Each tender is journalled in the transaction that
//! answers it, naming the version the till last wrote, so a power cut never
//! loses a tender the screen showed.
//!
//! **The domain decides every amount.** `add_tender` asks a final cash tender
//! for the remainder rounded to the store's coin step, gives change only for
//! cash, and refuses what would leave the sale neither settled nor payable by
//! a coin. [`PaymentSnapshot`] carries what is due, what each tender paid,
//! what remains, the rounding and the change, in minor units, so the screen
//! computes no money (`ref/ui-spec.md` §4). A tender's amount is money handed
//! over, not a price: it pays toward the sale's own priced total and changes
//! no line, which is why the registry allows `amount_minor` here under a
//! reviewed control of its own.
//!
//! **Taking a tender back reverses money**, so `tender_remove` is audited, in
//! the transaction that journals what is left (1.4.10). Going back to the
//! basket is allowed only while nothing is taken, and abandons the journal.
//!
//! **A sale left mid-payment** by a crash, or by the forced sign-out of an
//! account no longer live, stays journalled (the owner's decision of
//! 7 November 2026). `sale_in_flight_state` names every one at the register,
//! and `sale_resume_in_flight` takes one up on the till (1.8.4's recovery
//! half, by the owner's decision of 9 November 2026). What it becomes is the
//! domain's [`resume_in_flight`], from the journal alone:
//!
//! - **Nothing collected:** the checkout is abandoned, audited as
//!   `checkout.abandon`, and the basket comes back to the till being built,
//!   exactly as `cart_resume` brings back a parked one.
//! - **Tenders taken:** the tender screen, with every one of them.
//! - **The finalize begun:** the tender screen, settled, and `sale_finalize`
//!   completes it under the journal's own idempotency key.
//!
//! Whoever can resume a basket at this register may take up another
//! cashier's sale, and doing so writes `sale.resume` on its basket, as
//! `cart_resume` does (#363). Every row written before the sale completes
//! names the basket, [`BASKET_ENTITY`], under the sale's id, because a row
//! naming the `sale` must travel in the sale's own commit (#397). A sale
//! with money taken stays its cashier's, because the journal and every
//! tender in it are theirs.

use pos_db::DbError;
use pos_db::repo::checkout_operation::{CheckoutOperation, CheckoutOperationRepository};
use pos_db::repo::parked_cart::ParkedCartRepository;
use pos_db::repo::tender_type::TenderTypeRepository;
use pos_db::tax_policy::TaxPolicyRepository;
use pos_domain::cart::{
    BASKET_ENTITY, add_tender, back_to_building, begin_tender, remove_tender, resume_in_flight,
};
use pos_domain::{
    AuditIntent, Authorized, Capability, CartContext, CartError, CashRoundingPolicy, Currency,
    InFlight, InFlightSale, Money, MoneyError, Resumption, SaleId, StoreId, Tender, TenderId,
    TenderState, Tendering, Timestamp, UserId, cap, change_due, final_tender_rounding, is_settled,
    remaining_due, standard_tender_type,
};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use tauri::State;

use super::cart::{
    CartSnapshot, Screen, StockFlag, building, cart_refused, open_shift, pricing, pricing_refused,
    proof, resume_refused, signed_in_as, snapshot, stock_flags,
};
use super::{audit, sqlite, stored};
use crate::ipc::{IpcError, IpcResult};
use crate::state::{Locked, Paying, Register, Till, Working};

/// The tender kind `tender_add_cash` takes.
const CASH: &str = "cash";

/// The audit action a checkout abandoned at its resumption is recorded under:
/// the act's own verb, because `sale.resume`, which the command takes, names
/// the hand-over and not the discarding (1.4.10).
pub const ABANDON_ACTION: &str = "checkout.abandon";

/// A sale being paid for, as the tender screen shows it: every figure is the
/// domain's, in the sale's minor units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaymentSnapshot {
    pub sale_id: SaleId,
    /// The currency every `*_minor` is in, by its ISO code.
    pub currency: &'static str,
    /// What the sale costs, as priced when payment began.
    pub total_minor: i64,
    /// The tenders taken, in the order they were taken.
    pub tenders: Vec<TenderLine>,
    /// What they come to.
    pub collected_minor: i64,
    /// What is still owed: the total, with the final cash tender's rounding
    /// once one has made it, less what is collected. Never below nothing.
    pub remaining_minor: i64,
    /// What a cash tender is asked for now to settle the sale: the remainder
    /// at the store's coin step. Less cash is a part payment.
    pub cash_due_minor: i64,
    /// The rounding the final cash tender settled the sale under, which the
    /// screen shows as its own line.
    pub cash_rounding: Option<RoundingLine>,
    /// What the drawer hands back.
    pub change_minor: i64,
    /// Whether the tenders settle the sale, so it may be finalized.
    pub settled: bool,
    /// Products the basket would leave below zero now, which the screen shows
    /// loudly. Once payment has begun a shortfall never blocks it.
    pub stock_flags: Vec<StockFlag>,
}

/// One tender taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TenderLine {
    /// The id `tender_remove` takes.
    pub tender_id: TenderId,
    /// Its kind, by the grid's code: `cash`.
    pub type_code: String,
    pub amount_minor: i64,
}

/// The cash rounding a sale was settled under (master plan B.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RoundingLine {
    /// The exact remainder the earlier tenders left.
    pub original_minor: i64,
    /// What the final cash tender was asked for, at the coin step.
    pub rounded_minor: i64,
    /// `rounded_minor − original_minor`: negative when the customer was
    /// asked for less than was owed.
    pub adjustment_minor: i64,
}

/// A sale journalled mid-payment at this register, as `sale_in_flight_state`
/// names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InFlightSummary {
    pub sale_id: SaleId,
    /// `tendering`, or `finalizing` once its finalize has begun.
    pub state: &'static str,
    /// The cashier whose sale it is.
    pub cashier_id: UserId,
    /// Whether this till is the one paying for it now. Any other was left by
    /// a crash or a forced sign-out, for recovery.
    pub on_this_till: bool,
    /// The currency every `*_minor` is in, by its ISO code.
    pub currency: &'static str,
    pub total_minor: i64,
    pub collected_minor: i64,
    pub remaining_minor: i64,
    pub tender_count: usize,
    /// Whether a card operation was outstanding when it was last journalled.
    /// No Phase-1 checkout has one.
    pub card_outstanding: bool,
    /// When it was last journalled, in milliseconds since the Unix epoch.
    pub updated_at: Timestamp,
}

/// A journalled sale taken up on the till, as `sale_resume_in_flight` answers
/// it: what the till now holds, by the stage the sale is at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum ResumedSale {
    /// Nothing had been collected. The checkout was abandoned, and its basket
    /// is on the till, being built.
    Building { cart: CartSnapshot },
    /// Tenders had been taken: the tender screen, with every one.
    Tendering { payment: PaymentSnapshot },
    /// The finalize had begun. `sale_finalize` completes it under the
    /// checkout's own idempotency key, without taking another tender.
    Finalizing { payment: PaymentSnapshot },
}

#[tauri::command]
pub fn tender_begin(till: State<'_, Till>) -> IpcResult<PaymentSnapshot> {
    begin_payment(till.register()?)
}

#[tauri::command]
pub fn tender_add_cash(till: State<'_, Till>, amount_minor: i64) -> IpcResult<PaymentSnapshot> {
    add_cash(till.register()?, amount_minor)
}

#[tauri::command]
pub fn tender_remove(till: State<'_, Till>, tender_id: TenderId) -> IpcResult<PaymentSnapshot> {
    take_back(till.register()?, tender_id)
}

#[tauri::command]
pub fn tender_back_to_cart(till: State<'_, Till>) -> IpcResult<CartSnapshot> {
    back_to_cart(till.register()?)
}

#[tauri::command]
pub fn sale_in_flight_state(till: State<'_, Till>) -> IpcResult<Vec<InFlightSummary>> {
    in_flight(till.register()?)
}

#[tauri::command]
pub fn sale_resume_in_flight(till: State<'_, Till>, sale_id: SaleId) -> IpcResult<ResumedSale> {
    resume_journalled(till.register()?, sale_id)
}

/// `tender_begin`: begin paying for the till's basket. Its stock is screened
/// once more, since another basket may have sold it; it is priced under the
/// store's own pack and policy, and the store's coin step is fixed for every
/// tender of the sale; then the checkout is journalled under a fresh
/// idempotency key, before any money is taken.
///
/// # Errors
///
/// What the context refuses; `cart_not_open`; `sale_being_paid` when payment
/// has already begun; `nothing_to_tender` for a basket with no line;
/// `age_unconfirmed` while a line's age is still to be confirmed;
/// `stock_would_go_negative` under the store's hard block; the pricing
/// refusals, `pricing_unavailable` and `tax_band_missing`; and
/// `storage_failed`. A refused begin leaves the basket on the till.
pub fn begin_payment(register: &Register) -> IpcResult<PaymentSnapshot> {
    let at = register.now();
    let auth = proof::<cap::SaleCreate>(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let cart = building(till.working.as_ref())?.clone();
    stock_flags(&till.db, register.store(), &cart, Screen::Refuse)?;
    let (bands, settings) = pricing(&till.db, register.store(), &cart)?;
    let policy = TaxPolicyRepository::new(&till.db)
        .approved_for_store(register.store())
        .map_err(|e| pricing_refused(&e))?;
    let coin = CashRoundingPolicy {
        step_minor: policy.cash_round_step_minor(),
        direction: policy.cash_round_direction(),
    };
    let tendering = begin_tender(cart, &bands, &settings, coin).map_err(|e| cart_refused(&e))?;
    let answer = payment(&till.db, register.store(), &tendering)?;
    let key = register.next_id();
    let tx = till.db.transaction().map_err(sqlite)?;
    CheckoutOperationRepository::new(&tx)
        .begin(&tx, &tendering, key, at)
        .map_err(stored)?;
    tx.commit().map_err(sqlite)?;
    *till.working = Some(Working::Paying(Box::new(Paying {
        tendering,
        idempotency_key: key,
        version: 0,
    })));
    Ok(answer)
}

/// `tender_add_cash`: `amount_minor` in cash, toward what the sale owes. Cash
/// that covers the remainder at the coin step settles the sale, and anything
/// over it is change; less is a part payment.
///
/// # Errors
///
/// What the context refuses; `cart_not_open`; `payment_not_begun`;
/// `tender_type_disabled` when this register takes no cash; `sale_settled`
/// when nothing is owed; `tender_not_positive` for nothing or less;
/// `tender_underpaid` for cash that covers the exact remainder but not what
/// the coin step asks; `tender_amount_invalid` for an amount too large to
/// count; and `storage_failed`. A refused tender changes nothing.
pub fn add_cash(register: &Register, amount_minor: i64) -> IpcResult<PaymentSnapshot> {
    let at = register.now();
    let auth = proof::<cap::SaleCreate>(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let from = paying(till.working.as_ref())?.clone();
    if !TenderTypeRepository::new(&till.db)
        .is_active(CASH)
        .map_err(stored)?
    {
        return Err(IpcError::new(
            "tender_type_disabled",
            "error.tender.type_disabled",
        ));
    }
    let tender = Tender {
        id: TenderId::from_uuid(register.next_id()),
        type_code: CASH.to_owned(),
        amount: Money::from_minor(amount_minor, from.tendering.cart().currency),
        psp_ref: None,
        masked_pan: None,
        scheme: None,
        state: TenderState::Collected,
    };
    let next = add_tender(from.tendering.clone(), tender).map_err(|e| tender_refused(&e))?;
    journal(register, &mut till, next, &from, None, at)
}

/// `tender_remove`: take back the tender `tender_id`, which was taken in
/// error or which the customer withdrew. The tenders left are taken again in
/// their order, so which one settles the sale, and what it rounds, is decided
/// as if the removed one had never been taken. It is audited as
/// `tender.remove`, with what is owed once it is gone. Every Phase-1 tender is
/// cash, which the drawer hands back; a card's reversal with its provider is
/// Phase 2's (E.15).
///
/// # Errors
///
/// What the context refuses; `cart_not_open`; `payment_not_begun`;
/// `tender_unknown` for a tender the sale does not hold; whatever
/// `tender_add_cash` refuses of a later tender the removal leaves unable to
/// stand; and `storage_failed`. A refused removal changes nothing.
pub fn take_back(register: &Register, tender_id: TenderId) -> IpcResult<PaymentSnapshot> {
    let at = register.now();
    let auth = proof::<cap::SaleCreate>(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let from = paying(till.working.as_ref())?.clone();
    let (rest, intent) =
        remove_tender(from.tendering.clone(), tender_id, &auth).map_err(|e| tender_refused(&e))?;
    journal(register, &mut till, rest, &from, Some(&intent), at)
}

/// `tender_back_to_cart`: stop paying, and build the basket again. Allowed
/// only while nothing is taken, because changing the basket under a tender
/// would change what it paid for. The journal is abandoned in the transaction
/// that answers, and the basket is priced again, as it will be when payment
/// begins again.
///
/// # Errors
///
/// What the context refuses; `cart_not_open`; `payment_not_begun`;
/// `tender_collected` while a tender is taken, which must be removed first;
/// the pricing refusals; and `storage_failed`.
pub fn back_to_cart(register: &Register) -> IpcResult<CartSnapshot> {
    let at = register.now();
    let auth = proof::<cap::SaleCreate>(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let from = paying(till.working.as_ref())?.clone();
    let cart = back_to_building(from.tendering).map_err(|e| tender_refused(&e))?;
    let answer = snapshot(&till.db, register.store(), &cart)?;
    let tx = till.db.transaction().map_err(sqlite)?;
    CheckoutOperationRepository::new(&tx)
        .abandon(&tx, cart.id, from.idempotency_key)
        .map_err(stored)?;
    tx.commit().map_err(sqlite)?;
    *till.working = Some(Working::Building(Box::new(cart)));
    Ok(answer)
}

/// `sale_in_flight_state`: every sale journalled mid-payment at this
/// register, oldest first: the one this till is paying for, and any a crash or
/// a forced sign-out left for recovery.
///
/// # Errors
///
/// What the context refuses; `checkout_unreadable` for a journal row this
/// build cannot read; and `storage_failed`.
pub fn in_flight(register: &Register) -> IpcResult<Vec<InFlightSummary>> {
    let auth = proof::<cap::SaleCreate>(register, register.now())?;
    let till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let on_till = match till.working.as_ref() {
        Some(Working::Paying(paying)) => Some(paying.tendering.cart().id),
        _ => None,
    };
    CheckoutOperationRepository::new(&till.db)
        .in_flight(register.register())
        .map_err(journal_refused)?
        .iter()
        .map(|operation| summary(operation, on_till))
        .collect()
}

/// `sale_resume_in_flight`: take up `sale_id`, journalled mid-payment at this
/// register by a crash or a forced sign-out, on this till. A basket the till
/// is building is set aside first, in the same transaction, as `cart_resume`
/// sets it aside. What the sale becomes is [`resume_in_flight`]'s answer:
///
/// - **Nothing collected:** the checkout is abandoned and `checkout.abandon`
///   audited. The basket is parked and resumed onto the till in the same
///   transaction, through `cart_resume`'s own rules: on the open shift,
///   under the signed-in cashier, refused once its business day has passed,
///   and a takeover audited as `sale.resume`.
/// - **Tenders taken, or the finalize begun:** the till holds the sale under
///   the journal's key and version, and nothing is written but a takeover's
///   `sale.resume`. The journal, the basket's row and every tender stay as
///   they are, so `sale_finalize` replays the begun finalize rather than
///   begin another.
///
/// # Errors
///
/// What the context refuses; `sale_being_paid` while the till is paying for
/// a sale, which cannot be set aside; `sale_not_in_flight` for no such sale
/// journalled at this register; `checkout_unreadable` for a journal row this
/// build cannot read or resume, including a finalize its tenders do not
/// settle; `training_mode_mismatch`; for a basket brought back, what
/// `cart_resume` refuses of it, `shift_not_open` and `parked_cart_expired`
/// among them; the pricing refusals; and `storage_failed`. A refused resume
/// changes nothing.
pub fn resume_journalled(register: &Register, sale_id: SaleId) -> IpcResult<ResumedSale> {
    let at = register.now();
    let auth = proof::<cap::SaleResume>(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let held = till
        .working
        .as_ref()
        .map(|working| building(Some(working)).cloned())
        .transpose()?;
    let operation = CheckoutOperationRepository::new(&till.db)
        .get(sale_id)
        .map_err(journal_refused)?
        .filter(|operation| operation.in_flight.register_id() == register.register())
        .ok_or_else(|| IpcError::new("sale_not_in_flight", "error.tender.not_in_flight"))?;
    let journalled = operation.in_flight.tendering().clone();
    let cart = journalled.cart();
    if cart.is_training {
        return Err(cart_refused(&CartError::TrainingModeMismatch(
            cart.id, true,
        )));
    }
    let key = operation.in_flight.idempotency_key;
    let resumption = resume_in_flight(operation.in_flight).map_err(|error| {
        IpcError::from_source(
            "checkout_unreadable",
            "error.tender.checkout_unreadable",
            &error,
        )
    })?;
    let nonce = register.process_nonce();
    let (stage, tendering) = match resumption {
        Resumption::Abandon { cart, reason } => {
            let shift = open_shift(&till.db, register)?;
            let context = CartContext {
                register_id: register.register(),
                shift_id: shift.id,
                cashier_id: auth.actor(),
                currency: Currency::JOD,
                is_training: false,
                opened_at: at,
            };
            let abandon = abandoned(&auth, &journalled, reason);
            let tx = till.db.transaction().map_err(sqlite)?;
            let carts = ParkedCartRepository::new(&tx);
            if let Some(working) = held.as_ref() {
                carts.set_aside(&tx, nonce, working, at).map_err(stored)?;
            }
            CheckoutOperationRepository::new(&tx)
                .abandon(&tx, sale_id, key)
                .map_err(journal_refused)?;
            carts.set_aside(&tx, nonce, &cart, at).map_err(stored)?;
            let resumed = carts
                .resume(&tx, sale_id, &context, &auth, nonce)
                .map_err(|e| resume_refused(&e))?;
            let answer = snapshot(&tx, register.store(), &resumed.cart)?;
            audit(register, &tx, &abandon)?;
            if let Some(intent) = resumed.audit.as_ref() {
                audit(register, &tx, intent)?;
            }
            tx.commit().map_err(sqlite)?;
            *till.working = Some(Working::Building(Box::new(resumed.cart)));
            return Ok(ResumedSale::Building { cart: answer });
        }
        Resumption::ResumeTendering(t) => ("tendering", t),
        Resumption::ReplayFinalize(f) => ("finalizing", f.tendering().clone()),
        Resumption::QueryCardFirst { .. } => {
            // The journal refuses to read a row with a card operation, and
            // no Phase-1 tender has one (2.1.3).
            return Err(
                IpcError::new("checkout_unreadable", "error.tender.checkout_unreadable")
                    .with_detail("a card operation is outstanding and cannot be queried"),
            );
        }
    };
    let payment = payment(&till.db, register.store(), &tendering)?;
    let takeover = taken_over(&auth, &tendering, stage)?;
    let tx = till.db.transaction().map_err(sqlite)?;
    if let Some(working) = held.as_ref() {
        ParkedCartRepository::new(&tx)
            .set_aside(&tx, nonce, working, at)
            .map_err(stored)?;
    }
    if let Some(intent) = takeover.as_ref() {
        audit(register, &tx, intent)?;
    }
    tx.commit().map_err(sqlite)?;
    *till.working = Some(Working::Paying(Box::new(Paying {
        tendering,
        idempotency_key: key,
        version: operation.version,
    })));
    Ok(match stage {
        "finalizing" => ResumedSale::Finalizing { payment },
        _ => ResumedSale::Tendering { payment },
    })
}

/// What `checkout.abandon` records: whose checkout it was, what it was
/// priced at, and why it was discarded. No tender was taken, so none is named.
fn abandoned(
    auth: &Authorized<cap::SaleResume>,
    journalled: &Tendering,
    reason: &'static str,
) -> AuditIntent {
    let cart = journalled.cart();
    AuditIntent {
        actor: auth.actor(),
        approver: auth.approver(),
        approval: auth.approval(),
        action: ABANDON_ACTION,
        entity: BASKET_ENTITY,
        entity_id: cart.id.as_uuid(),
        reason: None,
        payload: json!({
            "cashier_id": cart.cashier_id.as_uuid().to_string(),
            "currency": cart.currency.code(),
            "reason": reason,
            "total_minor": journalled.priced().total.minor(),
        }),
        at: auth.at(),
    }
}

/// `sale.resume` when `auth`'s actor takes up another cashier's sale with
/// money taken: whose it is, the stage it was at, and what they collected.
/// A cashier taking up their own sale writes nothing.
fn taken_over(
    auth: &Authorized<cap::SaleResume>,
    t: &Tendering,
    stage: &'static str,
) -> IpcResult<Option<AuditIntent>> {
    let cart = t.cart();
    if cart.cashier_id == auth.actor() {
        return Ok(None);
    }
    let collected = counted(Money::sum(
        t.tenders().iter().map(|tender| tender.amount),
        cart.currency,
    ))?;
    Ok(Some(AuditIntent {
        actor: auth.actor(),
        approver: auth.approver(),
        approval: auth.approval(),
        action: cap::SaleResume::NAME,
        entity: BASKET_ENTITY,
        entity_id: cart.id.as_uuid(),
        reason: None,
        payload: json!({
            "cashier_id": cart.cashier_id.as_uuid().to_string(),
            "collected_minor": collected.minor(),
            "currency": cart.currency.code(),
            "stage": stage,
            "tender_count": t.tenders().len(),
        }),
        at: auth.at(),
    }))
}

/// A journal the resume could not read or change, as the screen is told it.
pub(super) fn journal_refused(error: DbError) -> IpcError {
    match error {
        DbError::StoredCheckoutInvalid { .. } => IpcError::from_source(
            "checkout_unreadable",
            "error.tender.checkout_unreadable",
            &error,
        ),
        other => stored(other),
    }
}

/// The sale the till is paying for.
pub(super) fn paying(working: Option<&Working>) -> IpcResult<&Paying> {
    match working {
        Some(Working::Paying(paying)) => Ok(paying),
        Some(Working::Building(_)) => {
            Err(IpcError::new("payment_not_begun", "error.tender.not_begun"))
        }
        None => Err(IpcError::new("cart_not_open", "error.cart.not_open")),
    }
}

/// Journal `next` as the next step of the checkout `from` holds, with
/// `intent` audited in the same transaction, then hold it on the till and
/// answer it. The step names the version the till last wrote.
fn journal(
    register: &Register,
    till: &mut Locked<'_>,
    next: Tendering,
    from: &Paying,
    intent: Option<&AuditIntent>,
    at: Timestamp,
) -> IpcResult<PaymentSnapshot> {
    let answer = payment(&till.db, register.store(), &next)?;
    let step = InFlightSale {
        state: InFlight::Tendering(next.clone()),
        card_op: None,
        idempotency_key: from.idempotency_key,
        updated_at: at,
    };
    let tx = till.db.transaction().map_err(sqlite)?;
    let version = CheckoutOperationRepository::new(&tx)
        .advance(&tx, &step, from.version)
        .map_err(stored)?;
    if let Some(intent) = intent {
        audit(register, &tx, intent)?;
    }
    tx.commit().map_err(sqlite)?;
    *till.working = Some(Working::Paying(Box::new(Paying {
        tendering: next,
        idempotency_key: from.idempotency_key,
        version,
    })));
    Ok(answer)
}

/// `t` as the tender screen shows it, its stock screened at `store`.
fn payment(conn: &Connection, store: StoreId, t: &Tendering) -> IpcResult<PaymentSnapshot> {
    let cart = t.cart();
    let flags = stock_flags(conn, store, cart, Screen::Flag)?;
    let collected = counted(Money::sum(
        t.tenders().iter().map(|tender| tender.amount),
        cart.currency,
    ))?;
    let remaining = counted(remaining_due(t))?;
    let coin = t.cash_policy();
    let cash_due = match standard_tender_type(CASH) {
        Some(cash) => counted(
            final_tender_rounding(&cash, remaining, coin.step_minor, coin.direction)
                .map(|asked| asked.map_or(remaining, |rounding| rounding.rounded)),
        )?,
        None => remaining,
    };
    Ok(PaymentSnapshot {
        sale_id: cart.id,
        currency: cart.currency.code(),
        total_minor: t.priced().total.minor(),
        tenders: t
            .tenders()
            .iter()
            .map(|tender| TenderLine {
                tender_id: tender.id,
                type_code: tender.type_code.clone(),
                amount_minor: tender.amount.minor(),
            })
            .collect(),
        collected_minor: collected.minor(),
        remaining_minor: remaining.minor(),
        cash_due_minor: cash_due.minor(),
        cash_rounding: t.cash_rounding().map(|rounding| RoundingLine {
            original_minor: rounding.original.minor(),
            rounded_minor: rounding.rounded.minor(),
            adjustment_minor: rounding.adjustment.minor(),
        }),
        change_minor: counted(change_due(t))?.minor(),
        settled: is_settled(t),
        stock_flags: flags.iter().map(StockFlag::from).collect(),
    })
}

/// A journalled checkout, as `sale_in_flight_state` names it.
fn summary(operation: &CheckoutOperation, on_till: Option<SaleId>) -> IpcResult<InFlightSummary> {
    let in_flight = &operation.in_flight;
    let t = in_flight.tendering();
    let cart = t.cart();
    Ok(InFlightSummary {
        sale_id: cart.id,
        state: match in_flight.state {
            InFlight::Tendering(_) => "tendering",
            InFlight::Finalizing(_) => "finalizing",
        },
        cashier_id: cart.cashier_id,
        on_this_till: on_till == Some(cart.id),
        currency: cart.currency.code(),
        total_minor: t.priced().total.minor(),
        collected_minor: counted(Money::sum(
            t.tenders().iter().map(|tender| tender.amount),
            cart.currency,
        ))?
        .minor(),
        remaining_minor: counted(remaining_due(t))?.minor(),
        tender_count: t.tenders().len(),
        card_outstanding: in_flight.card_op.is_some(),
        updated_at: in_flight.updated_at,
    })
}

/// An amount the domain could not count, which `add_tender` refuses before it
/// can arise.
fn counted(amount: Result<Money, MoneyError>) -> IpcResult<Money> {
    amount.map_err(|e| cart_refused(&CartError::Money(e)))
}

/// A tender's refusal, as the screen is told it. An amount too large to add
/// to what is collected is the tender's own; every other refusal is the
/// cart's.
fn tender_refused(error: &CartError) -> IpcError {
    match error {
        CartError::Money(_) => IpcError::from_source(
            "tender_amount_invalid",
            "error.tender.amount_invalid",
            error,
        ),
        other => cart_refused(other),
    }
}
