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
//! account no longer live, stays journalled until 1.8.4's recovery half
//! resumes it (the owner's decision of 7 November 2026).
//! `sale_in_flight_state` names every one at the register.

use pos_db::DbError;
use pos_db::repo::checkout_operation::{CheckoutOperation, CheckoutOperationRepository};
use pos_db::repo::tender_type::TenderTypeRepository;
use pos_db::tax_policy::TaxPolicyRepository;
use pos_domain::cart::{add_tender, back_to_building, begin_tender, remove_tender};
use pos_domain::{
    AuditIntent, CartError, CashRoundingPolicy, InFlight, InFlightSale, Money, MoneyError, SaleId,
    StoreId, Tender, TenderId, TenderState, Tendering, Timestamp, UserId, cap, change_due,
    final_tender_rounding, is_settled, remaining_due, standard_tender_type,
};
use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use super::cart::{
    CartSnapshot, Screen, StockFlag, building, cart_refused, pricing, pricing_refused, proof,
    signed_in_as, snapshot, stock_flags,
};
use super::{audit, sqlite, stored};
use crate::ipc::{IpcError, IpcResult};
use crate::state::{Locked, Paying, Register, Till, Working};

/// The tender kind `tender_add_cash` takes.
const CASH: &str = "cash";

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
        .map_err(|error| match error {
            DbError::StoredCheckoutInvalid { .. } => IpcError::from_source(
                "checkout_unreadable",
                "error.tender.checkout_unreadable",
                &error,
            ),
            other => stored(other),
        })?
        .iter()
        .map(|operation| summary(operation, on_till))
        .collect()
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
