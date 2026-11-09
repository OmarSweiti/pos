//! A register's shift: `shift_open`, `shift_current` and `shift_close`
//! (`ref/ipc-contract.md` §3, "Shift and cash"; microstep 1.9.5, its terminal
//! half).
//!
//! Each is a thin call into the domain and database half 1.9.5 delivered on
//! 4 November 2026, run in the shell state 1.8.2c created: the register's
//! database and the signed-in user.
//!
//! **Opening.** The float is counted in the store's own pieces
//! ([`count_float`]), and the business date is decided once, here, by
//! [`opening_business_date`]. When the clock is trusted it is derived from
//! the register's clock and the store's zone and cutover, and a date the
//! caller supplies is refused. When it is not, the date is the operator's,
//! required and marked so in the audit row. The register trusts no clock yet
//! ([`Register::clock_confidence`]), so until trusted time lands every
//! opening asks the operator to confirm its date. The opening fact and its
//! `shift.open` row travel in one envelope (I-9).
//!
//! A float is cash counted into the drawer, not a price. It pays toward no
//! sale and changes no line, and each piece's face value must be one of the
//! currency's own (1.5.4), so no argument here carries a price.
//!
//! **Closing.** Only the shift's opener closes it on this path, with no
//! approval: closing your own work is not an escalation. Another user's shift
//! waits for Phase 2's `shift_force_close_stale`. The close is a fact of its
//! own, with its `shift.close` row in its envelope, and the opening is never
//! changed.
//!
//! - **A sale in flight keeps the shift open.** A sale journalled
//!   mid-payment in this shift could never complete once the shift closed,
//!   because a sale completes only in its own open shift. So the close is
//!   refused while one is, and the sale is resumed first: completed, or, with
//!   nothing taken, given back to its basket.
//! - **The till's basket is set aside,** as a sign-out sets it aside (the
//!   owner's decision of 5 November 2026): parked when it has lines, dropped
//!   when it has none. One being paid for with nothing taken goes back to
//!   being a basket first. The close's audit row names it, and a shift opened
//!   on the same business day can resume it.
//! - **A new shift is a new till** (E.54). The shell keeps no training flag
//!   of its own: every cart opens out of training, so nothing of a training
//!   session reaches the next shift.
//!
//! **`shift_current` needs no one signed in.** The lock screen names whose
//! shift is open before anyone signs in (1.11.4). It answers the opening
//! fact, which carries no grant, PIN or key.

use pos_db::DbError;
use pos_db::repo::checkout_operation::CheckoutOperationRepository;
use pos_db::repo::parked_cart::{ParkedCartRepository, SetAside};
use pos_db::repo::shift::{self, ShiftRepository, close_payload, opening_payload};
use pos_db::repo::store::StoreRepository;
use pos_domain::{
    BusinessDate, ClockConfidence, Currency, DayBoundary, Money, SaleId, Shift, ShiftError,
    ShiftId, ShiftOpening, Timestamp, UserId, cap, count_float, open_shift, opening_business_date,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;
use uuid::Uuid;

use super::cart::{open_shift as the_open_shift, proof, signed_in_as};
use super::tender::journal_refused;
use super::{Fact, audit_fact, sqlite, stored};
use crate::ipc::{IpcError, IpcResult};
use crate::state::{Register, Till, Working};
use crate::time::resolve_utc_offset_minutes;

/// A shift, as its opening fact records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShiftSnapshot {
    /// The id `shift_close` takes.
    pub shift_id: ShiftId,
    /// The trading day every sale in it is dated by, `YYYY-MM-DD`.
    pub business_date: String,
    /// Who opened it, and so who closes it.
    pub opened_by: UserId,
    /// When it opened, in milliseconds since the Unix epoch.
    pub opened_at: Timestamp,
    /// The currency the float is in, by its ISO code.
    pub currency: &'static str,
    /// The opening float.
    pub float_minor: i64,
}

/// One piece of the opening float, and how many of it were counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenominationCount {
    /// The piece, by its face value in minor units: `500` for a half dinar.
    pub denomination_minor: i64,
    /// How many of it were counted.
    pub count: u32,
}

/// A shift closed, as `shift_close` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShiftCloseEvent {
    /// The close fact's id.
    pub close_id: Uuid,
    pub shift_id: ShiftId,
    /// The trading day it closed.
    pub business_date: String,
    /// When it closed, in milliseconds since the Unix epoch.
    pub closed_at: Timestamp,
    /// The basket the till was building, now in the parked list, if it had
    /// any line.
    pub parked_cart: Option<SaleId>,
}

#[tauri::command]
pub fn shift_open(
    till: State<'_, Till>,
    float_by_denomination: Vec<DenominationCount>,
    business_date: Option<String>,
) -> IpcResult<ShiftSnapshot> {
    let register = till.register()?;
    begin_shift(
        register,
        register.clock_confidence(),
        &float_by_denomination,
        business_date.as_deref(),
    )
}

#[tauri::command]
pub fn shift_current(till: State<'_, Till>) -> IpcResult<Option<ShiftSnapshot>> {
    current_shift(till.register()?)
}

#[tauri::command]
pub fn shift_close(till: State<'_, Till>, shift_id: ShiftId) -> IpcResult<ShiftCloseEvent> {
    end_shift(till.register()?, shift_id)
}

/// `shift_open`: open a shift on this register, with the float `counted` and
/// a business date decided under `confidence`, the clock's as the shell reads
/// it. A command whose rule turns on the clock takes it as an argument of its
/// plain function, so its tests drive every branch; the Tauri wrapper passes
/// [`Register::clock_confidence`]. The opener is the signed-in user.
///
/// # Errors
///
/// What the context refuses; `business_date_invalid` for a date that is not
/// `YYYY-MM-DD`; `shift_already_open` while the register has an open shift;
/// `store_day_unreadable` when the store's zone or cutover cannot date a
/// shift; `float_not_a_denomination` for a piece the currency does not have;
/// `float_invalid` for a float too large to count; `business_date_required`
/// when the clock is not trusted and no date is confirmed;
/// `business_date_not_accepted` for a date supplied when the clock is
/// trusted; and `storage_failed`. A refused opening writes nothing.
pub fn begin_shift(
    register: &Register,
    confidence: ClockConfidence,
    counted: &[DenominationCount],
    business_date: Option<&str>,
) -> IpcResult<ShiftSnapshot> {
    let at = register.now();
    let auth = proof::<cap::ShiftOpen>(register, at)?;
    let supplied = business_date.map(parsed_date).transpose()?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    if ShiftRepository::new(&till.db)
        .current(register.register())
        .map_err(stored)?
        .is_some()
    {
        return Err(already_open());
    }
    let day = StoreRepository::new(&till.db)
        .trading_day(register.store())
        .map_err(|error| day_unreadable(&error))?;
    let float =
        count_float(day.currency, &pieces(day.currency, counted)).map_err(|e| shift_refused(&e))?;
    let offset =
        resolve_utc_offset_minutes(&day.time_zone, at).map_err(|error| day_unreadable(&error))?;
    let boundary =
        DayBoundary::new(offset, day.cutover_minutes).map_err(|error| day_unreadable(&error))?;
    let date =
        opening_business_date(confidence, at, boundary, supplied).map_err(|e| shift_refused(&e))?;
    let (opened, intent) = open_shift(
        &auth,
        &ShiftOpening {
            id: ShiftId::from_uuid(register.next_id()),
            register_id: register.register(),
            store_id: register.store(),
            float,
            date,
            confidence,
            opened_at: at,
        },
    );
    let payload = opening_payload(&opened);
    let tx = till.db.transaction().map_err(sqlite)?;
    audit_fact(
        register,
        &tx,
        register.next_id(),
        &Fact {
            entity: shift::ENTITY,
            id: opened.id.as_uuid(),
            payload: &payload,
        },
        &intent,
    )?;
    ShiftRepository::new(&tx)
        .open(&tx, &opened)
        .map_err(|error| match error {
            DbError::ShiftAlreadyOpen { .. } => already_open(),
            other => stored(other),
        })?;
    tx.commit().map_err(sqlite)?;
    Ok(snapshot(&opened))
}

/// `shift_current`: the register's open shift, if it has one. It needs no one
/// signed in.
///
/// # Errors
///
/// `storage_failed`.
pub fn current_shift(register: &Register) -> IpcResult<Option<ShiftSnapshot>> {
    let open = register
        .with_db(|db| ShiftRepository::new(db).current(register.register()))
        .map_err(stored)?;
    Ok(open.as_ref().map(snapshot))
}

/// `shift_close`: close `shift_id`, the register's open shift, as its opener.
/// A sale journalled mid-payment in it keeps it open. What the till is
/// building is set aside in the close's own transaction: parked with a line,
/// dropped without one, and a sale being paid for with nothing taken goes
/// back to being a basket first. The close's audit row names the basket it
/// parked.
///
/// # Errors
///
/// What the context refuses; `shift_not_open` when the register has no open
/// shift; `shift_not_current` for any shift but the register's open one;
/// `shift_not_yours` when the signed-in user did not open it;
/// `shift_closed_before_opened` when the register's clock reads earlier than
/// the opening; `sale_in_flight` while a sale in the shift is journalled
/// mid-payment, on this till or left by a crash; `checkout_unreadable` for a
/// journal row this build cannot read; and `storage_failed`. A refused close
/// writes nothing and leaves the till as it was.
pub fn end_shift(register: &Register, shift_id: ShiftId) -> IpcResult<ShiftCloseEvent> {
    let at = register.now();
    let auth = proof::<cap::ShiftClose>(register, at)?;
    let mut till = register.lock_till();
    signed_in_as(till.session.as_ref(), auth.actor())?;
    let open = the_open_shift(&till.db, register)?;
    if open.id != shift_id {
        return Err(IpcError::new(
            "shift_not_current",
            "error.shift.not_current",
        ));
    }
    let (close, mut intent) = open
        .close(&auth, register.next_id(), at)
        .map_err(|e| shift_refused(&e))?;

    // A sale being paid for on this till with nothing taken goes back to
    // being a basket below. Any other sale journalled in the shift keeps it
    // open.
    let unpaid = match till.working.as_ref() {
        Some(Working::Paying(paying)) if paying.tendering.tenders().is_empty() => {
            Some(paying.tendering.cart().id)
        }
        _ => None,
    };
    let journalled = CheckoutOperationRepository::new(&till.db)
        .in_flight(register.register())
        .map_err(journal_refused)?;
    if journalled.iter().any(|operation| {
        operation.in_flight.tendering().cart().shift_id == open.id
            && Some(operation.in_flight.sale_id()) != unpaid
    }) {
        return Err(sale_in_flight());
    }

    let tx = till.db.transaction().map_err(sqlite)?;
    let basket = match till.working.as_ref() {
        None => None,
        Some(Working::Building(cart)) => Some(&**cart),
        Some(Working::Paying(paying)) if paying.tendering.tenders().is_empty() => {
            let cart = paying.tendering.cart();
            CheckoutOperationRepository::new(&tx)
                .abandon(&tx, cart.id, paying.idempotency_key)
                .map_err(stored)?;
            Some(cart)
        }
        // Money taken is journalled, and was refused above; never set aside.
        Some(Working::Paying(_)) => return Err(sale_in_flight()),
    };
    let parked = match basket {
        None => None,
        Some(cart) => match ParkedCartRepository::new(&tx)
            .set_aside(&tx, register.process_nonce(), cart, at)
            .map_err(stored)?
        {
            SetAside::Parked => Some(cart.id),
            SetAside::Dropped => None,
        },
    };
    // The domain's row names the shift and its day; the shell adds what it
    // set aside, as a sign-out's row does.
    if let Some(fields) = intent.payload.as_object_mut() {
        fields.insert(
            "parked_cart".to_owned(),
            json!(parked.map(|sale| sale.as_uuid().to_string())),
        );
    }
    let commit = register.next_id();
    let payload = close_payload(&close, commit);
    audit_fact(
        register,
        &tx,
        commit,
        &Fact {
            entity: shift::CLOSE_ENTITY,
            id: close.id,
            payload: &payload,
        },
        &intent,
    )?;
    ShiftRepository::new(&tx)
        .close(&tx, &close, commit)
        .map_err(|error| match error {
            DbError::ShiftNotOpen(_) => {
                IpcError::from_source("shift_not_open", "error.shift.not_open", &error)
            }
            DbError::ShiftClosedByAnother { .. } => {
                IpcError::from_source("shift_not_yours", "error.shift.not_yours", &error)
            }
            other => stored(other),
        })?;
    tx.commit().map_err(sqlite)?;
    *till.working = None;
    Ok(ShiftCloseEvent {
        close_id: close.id,
        shift_id: open.id,
        business_date: open.business_date.to_iso(),
        closed_at: close.closed_at,
        parked_cart: parked,
    })
}

fn snapshot(shift: &Shift) -> ShiftSnapshot {
    ShiftSnapshot {
        shift_id: shift.id,
        business_date: shift.business_date.to_iso(),
        opened_by: shift.opened_by,
        opened_at: shift.opened_at,
        currency: shift.float.currency().code(),
        float_minor: shift.float.minor(),
    }
}

/// Each counted piece, in the store's currency.
fn pieces(currency: Currency, counted: &[DenominationCount]) -> Vec<(Money, u32)> {
    counted
        .iter()
        .map(|piece| {
            (
                Money::from_minor(piece.denomination_minor, currency),
                piece.count,
            )
        })
        .collect()
}

fn parsed_date(text: &str) -> IpcResult<BusinessDate> {
    BusinessDate::parse(text).map_err(|error| {
        IpcError::from_source(
            "business_date_invalid",
            "error.shift.business_date_invalid",
            &error,
        )
    })
}

fn already_open() -> IpcError {
    IpcError::new("shift_already_open", "error.shift.already_open")
}

fn sale_in_flight() -> IpcError {
    IpcError::new("sale_in_flight", "error.shift.sale_in_flight")
}

fn day_unreadable(error: &dyn std::error::Error) -> IpcError {
    IpcError::from_source(
        "store_day_unreadable",
        "error.shift.store_day_unreadable",
        error,
    )
}

/// A [`ShiftError`] as the webview receives it: a code, a key, and the source
/// in the scrubbed log only. No wildcard arm: a refusal the domain adds is a
/// decision here.
fn shift_refused(error: &ShiftError) -> IpcError {
    let (code, key) = match error {
        ShiftError::SuppliedDateRefused => (
            "business_date_not_accepted",
            "error.shift.business_date_not_accepted",
        ),
        ShiftError::ConfirmedDateRequired => (
            "business_date_required",
            "error.shift.business_date_required",
        ),
        ShiftError::NotADenomination(_) => (
            "float_not_a_denomination",
            "error.shift.float_not_a_denomination",
        ),
        ShiftError::Money(_) => ("float_invalid", "error.shift.float_invalid"),
        ShiftError::NotTheOpener { .. } => ("shift_not_yours", "error.shift.not_yours"),
        ShiftError::ApprovalNotTaken => ("approval_not_taken", "error.shift.approval_not_taken"),
        ShiftError::ClosedBeforeOpened => (
            "shift_closed_before_opened",
            "error.shift.closed_before_opened",
        ),
    };
    IpcError::from_source(code, key, error)
}
