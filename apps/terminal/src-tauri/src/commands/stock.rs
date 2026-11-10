//! A store's stock at the till: `stock_on_hand`, `stock_adjust_prepare` and
//! `stock_adjust` (`ref/ipc-contract.md` §3, "Stock"; microstep 1.10.5, its
//! terminal half).
//!
//! Each is a thin call into the domain and database half 1.10.5 delivered on
//! 4 November 2026, run in the shell state 1.8.2c created: the register's
//! database, the signed-in user and stored-approval resolution.
//!
//! Every path that increases stock arrives in Phase 4, so without an
//! adjustment every product goes negative from its first sale, and the
//! negative-stock flag means nothing. Stock is moved by hand in two steps:
//!
//! - **`stock_adjust_prepare` stores what was asked for.** A holder of
//!   `stock.adjust` names a product, a signed quantity, one of five reasons
//!   by its wire code, and an optional note. The request is stored under the
//!   id its event will carry, with the digest of exactly what was asked,
//!   including who asked and when. Nothing moves yet, and nothing is audited:
//!   the request is register-local and travels nowhere.
//! - **`stock_adjust` posts it, with a second person's approval.** The
//!   approval names that event, an amount of exactly zero, and the digest,
//!   which the repository recomputes from the stored row before the effect;
//!   `0006`'s trigger independently refuses any change to a request once an
//!   approval names it. Only the person who asked posts it. The event, its
//!   `stock.adjust` audit row and the approval's consumption travel in one
//!   envelope, and the request is removed in the same transaction (I-9).
//!
//! **An adjustment is dated by the register's open shift,** as a sale is. The
//! register trusts no clock yet, so no date is derived from one: with no
//! shift open, `stock_adjust` is refused as `shift_not_open`.
//!
//! `stock_on_hand` reads the store's projection, `stock_cache`, under
//! `sale.create`: what the shelf should hold, and the cost a unit is carried
//! at, which is unknown rather than zero when no cost was ever recorded.

use pos_db::DbError;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::product::ProductRepository;
use pos_db::repo::stock::StockRepository;
use pos_db::repo::stock_adjustment::StockAdjustmentRepository;
use pos_domain::{
    ApprovalBinding, ApprovalId, EscalationPolicy, ProductId, Qty, StockAdjustmentReason,
    StockAdjustmentRequest, StockError, StockEventId, StockPosition, cap,
};
use serde::Serialize;
use tauri::State;

use super::cart::{open_shift, proof, signed_in_as};
use super::{sqlite, stored};
use crate::ipc::context::{authorize, permission_refused};
use crate::ipc::{IpcError, IpcResult};
use crate::payload::audit_payload;
use crate::state::{PRODUCER_VERSION, Register, SYNC_PROTOCOL_VERSION, Till};

/// A product's stock in the register's store, as `stock_on_hand` and
/// `stock_adjust` answer it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StockSnapshot {
    pub product_id: ProductId,
    /// On hand, in milli-units. Below zero when more was sold than was ever
    /// recorded arriving.
    pub on_hand_milli: i64,
    /// The currency `wac_minor` is in, by its ISO code.
    pub currency: &'static str,
    /// The weighted average cost of one unit, in minor units, or `None` when
    /// no cost was ever recorded: shown as unknown, never as zero.
    pub wac_minor: Option<i64>,
    /// Whether any cost behind `wac_minor` was projected or missing.
    pub is_wac_estimated: bool,
}

impl StockSnapshot {
    fn of(product_id: ProductId, position: &StockPosition) -> StockSnapshot {
        StockSnapshot {
            product_id,
            on_hand_milli: position.on_hand.milli(),
            currency: position.wac.currency().code(),
            wac_minor: position.cost_basis().map(|wac| wac.minor()),
            is_wac_estimated: position.is_wac_estimated,
        }
    }
}

/// A stored request, as `stock_adjust_prepare` answers it: what the approver
/// is shown, and the id `stock_adjust` takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StockAdjustmentSnapshot {
    /// The id the event will carry, and the one an approval names.
    pub stock_event_id: StockEventId,
    pub product_id: ProductId,
    /// Signed, in milli-units: negative leaves the shelf, positive arrives.
    pub qty_delta_milli: i64,
    /// One of `opening_stock`, `damage`, `theft`, `expiry` and
    /// `count_correction`.
    pub reason_code: &'static str,
    pub note: Option<String>,
}

#[tauri::command]
pub fn stock_on_hand(till: State<'_, Till>, product_id: ProductId) -> IpcResult<StockSnapshot> {
    on_hand(till.register()?, product_id)
}

#[tauri::command]
pub fn stock_adjust_prepare(
    till: State<'_, Till>,
    product_id: ProductId,
    qty_delta_milli: i64,
    reason_code: String,
    note: Option<String>,
) -> IpcResult<StockAdjustmentSnapshot> {
    prepare_adjustment(
        till.register()?,
        product_id,
        qty_delta_milli,
        &reason_code,
        note,
    )
}

#[tauri::command]
pub fn stock_adjust(
    till: State<'_, Till>,
    stock_event_id: StockEventId,
    approval_id: ApprovalId,
) -> IpcResult<StockSnapshot> {
    adjust(till.register()?, stock_event_id, approval_id)
}

/// `stock_on_hand`: what the store holds of `product_id`, from its
/// projection.
///
/// # Errors
///
/// What the context refuses (`not_signed_in`, `permission_denied` without
/// `sale.create`); `product_unknown` for an id that names no product; and
/// `storage_failed`.
pub fn on_hand(register: &Register, product_id: ProductId) -> IpcResult<StockSnapshot> {
    let auth = proof::<cap::SaleCreate>(register, register.now())?;
    let (db, session) = register.lock();
    signed_in_as(session.as_ref(), auth.actor())?;
    drop(session);
    ProductRepository::new(&db)
        .by_id(product_id)
        .map_err(stored)?
        .ok_or_else(product_unknown)?;
    let position = StockRepository::new(&db)
        .position(product_id, register.store())
        .map_err(stored)?;
    Ok(StockSnapshot::of(product_id, &position))
}

/// `stock_adjust_prepare`: store the signed-in user's request to move
/// `qty_delta_milli` of `product_id` for the reason `reason_code` names, with
/// `note`, under a new event id. Nothing moves until `stock_adjust`.
///
/// # Errors
///
/// What the context refuses (`not_signed_in`, `permission_denied` without
/// `stock.adjust`); `stock_reason_unknown` for a code that names none of the
/// five; `stock_adjustment_against_reason` for a movement its reason does not
/// allow, a movement of nothing among them; `product_unknown`;
/// `qty_not_whole` for a quantity that is not a whole number of the
/// product's steps; and `storage_failed`. A refused request stores nothing.
pub fn prepare_adjustment(
    register: &Register,
    product_id: ProductId,
    qty_delta_milli: i64,
    reason_code: &str,
    note: Option<String>,
) -> IpcResult<StockAdjustmentSnapshot> {
    let at = register.now();
    let auth = proof::<cap::StockAdjust>(register, at)?;
    let reason = StockAdjustmentReason::from_code(reason_code).map_err(|e| stock_refused(&e))?;
    let request = StockAdjustmentRequest {
        stock_event_id: StockEventId::from_uuid(register.next_id()),
        product_id,
        qty_delta: Qty::from_milli(qty_delta_milli),
        reason,
        note,
    };
    let (mut db, session) = register.lock();
    signed_in_as(session.as_ref(), auth.actor())?;
    drop(session);
    let tx = db.transaction().map_err(sqlite)?;
    StockAdjustmentRepository::new(&tx)
        .prepare(&tx, &request, auth.actor(), at)
        .map_err(|e| adjustment_refused(&e))?;
    tx.commit().map_err(sqlite)?;
    Ok(StockAdjustmentSnapshot {
        stock_event_id: request.stock_event_id,
        product_id: request.product_id,
        qty_delta_milli: request.qty_delta.milli(),
        reason_code: request.reason.as_str(),
        note: request.note,
    })
}

/// `stock_adjust`: post the request `stock_event_id` under the approval
/// `approval_id`, which must name that event, an amount of zero, and the
/// request exactly as it is stored, and come from someone other than the
/// person who asked. Answers the position the event leaves.
///
/// # Errors
///
/// - What the context refuses: `not_signed_in`, `permission_denied` without
///   `stock.adjust`, and the approval's own refusals (`approval_unknown`,
///   `approval_used`, `approval_mismatch`, `approval_expired`,
///   `approval_revoked`).
/// - `stock_adjustment_unknown` for no such request, or one already posted.
/// - `stock_adjustment_altered` for a stored request that no longer matches
///   its digest.
/// - `stock_adjustment_not_yours` for a request someone else asked for.
/// - `shift_not_open` when the register has no open shift to date it by.
/// - `storage_failed`.
///
/// A refused adjustment writes nothing: the request stays, and the approval
/// stays unspent.
pub fn adjust(
    register: &Register,
    stock_event_id: StockEventId,
    approval_id: ApprovalId,
) -> IpcResult<StockSnapshot> {
    let at = register.now();
    // Only a holder of `stock.adjust` learns anything of a request.
    proof::<cap::StockAdjust>(register, at)?;
    // What the approval must name, from the request as it is stored now.
    // Nothing is written: dropping the transaction ends it.
    let binding = register.with_db(|conn| -> IpcResult<ApprovalBinding> {
        let tx = conn.transaction().map_err(sqlite)?;
        StockAdjustmentRepository::new(&tx)
            .binding(&tx, stock_event_id)
            .map_err(|e| adjustment_refused(&e))
    })?;
    // Every adjustment escalates, whatever the merchant's policy names.
    let policy = EscalationPolicy::empty().with_escalation::<cap::StockAdjust>();
    let auth = authorize::<cap::StockAdjust>(register, Some(approval_id), &binding, &policy, at)?;
    let (mut db, session) = register.lock();
    signed_in_as(session.as_ref(), auth.actor())?;
    drop(session);
    let shift = open_shift(&db, register)?;

    let tx = db.transaction().map_err(sqlite)?;
    let adjustments = StockAdjustmentRepository::new(&tx);
    // The request is reloaded and held to its digest, its asker and the
    // approval once more, inside the transaction that posts it.
    let approved = adjustments
        .prepare_effect(
            &tx,
            stock_event_id,
            register.register(),
            register.store(),
            &auth,
            register.next_id(),
            shift.business_date,
        )
        .map_err(|e| adjustment_refused(&e))?;
    let (audit_id, intent) = approved.audit();
    let audited = audit_payload(audit_id, register.register(), &intent)
        .map_err(|e| IpcError::from_source("storage_failed", "error.storage.failed", &e))?;
    let facts = approved.facts(audited);
    let change_ids: Vec<[u8; 16]> = facts
        .iter()
        .map(|_| *register.next_id().as_bytes())
        .collect();
    let members: Vec<FactMember<'_>> = facts
        .iter()
        .zip(&change_ids)
        .map(|((entity, entity_id, payload), change_id)| FactMember {
            change_id,
            entity,
            entity_id,
            payload,
        })
        .collect();
    let commit_id = register.next_id();
    let created_at = at.to_iso8601();
    OutboxRepository::new(&tx)
        .write_commit(
            &tx,
            &CommitEnvelope {
                commit_id: commit_id.as_bytes(),
                protocol_version: SYNC_PROTOCOL_VERSION,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: PRODUCER_VERSION,
                created_at: &created_at,
            },
            &members,
        )
        .map_err(stored)?;
    let after = adjustments
        .commit(&tx, &approved)
        .map_err(|e| adjustment_refused(&e))?;
    tx.commit().map_err(sqlite)?;
    Ok(StockSnapshot::of(
        approved.event().event().product_id,
        &after,
    ))
}

fn product_unknown() -> IpcError {
    IpcError::new("product_unknown", "error.stock.product_unknown")
}

/// A refusal of the adjustment's repository, as the screen is told it: a
/// code to branch on, never the error's text, which names products, users
/// and quantities. The source is in the scrubbed log under the trace id.
fn adjustment_refused(error: &DbError) -> IpcError {
    match error {
        DbError::StockRefused(refusal) => stock_refused(refusal),
        DbError::StockProductUnknown(_) => {
            IpcError::from_source("product_unknown", "error.stock.product_unknown", error)
        }
        DbError::StockQtyOffStep { .. } => {
            IpcError::from_source("qty_not_whole", "error.stock.qty_not_whole", error)
        }
        DbError::StockAdjustmentNotFound(_) => IpcError::from_source(
            "stock_adjustment_unknown",
            "error.stock.adjustment_unknown",
            error,
        ),
        DbError::StockAdjustmentAltered(_) => IpcError::from_source(
            "stock_adjustment_altered",
            "error.stock.adjustment_altered",
            error,
        )
        .with_detail("the stored request is not what was asked for"),
        DbError::StockAdjustmentNotYours { .. } => IpcError::from_source(
            "stock_adjustment_not_yours",
            "error.stock.adjustment_not_yours",
            error,
        ),
        DbError::ApprovalRefused(refusal) => permission_refused(refusal),
        DbError::ApprovalMissing(_) => {
            IpcError::from_source("approval_unknown", "error.approval.unknown", error)
        }
        DbError::ApprovalAlreadyConsumed => {
            IpcError::from_source("approval_used", "error.approval.used", error)
        }
        other => stored_ref(other),
    }
}

/// A movement the domain refuses before anything is stored.
fn stock_refused(error: &StockError) -> IpcError {
    match error {
        StockError::UnknownAdjustmentReason(_) => {
            IpcError::from_source("stock_reason_unknown", "error.stock.reason_unknown", error)
        }
        StockError::AdjustmentAgainstItsReason { reason } => {
            let failure = IpcError::from_source(
                "stock_adjustment_against_reason",
                "error.stock.adjustment_against_reason",
                error,
            );
            failure.with_detail(match reason {
                StockAdjustmentReason::OpeningStock => "opening stock only arrives",
                StockAdjustmentReason::Damage
                | StockAdjustmentReason::Theft
                | StockAdjustmentReason::Expiry => "damage, theft and expiry only take stock away",
                StockAdjustmentReason::CountCorrection => {
                    "a count correction moves stock one way or the other"
                }
            })
        }
        other => IpcError::from_source("stock_refused", "error.stock.refused", other),
    }
}

/// [`stored`], for an error the caller still holds.
fn stored_ref(error: &DbError) -> IpcError {
    IpcError::from_source("storage_failed", "error.storage.failed", error)
}
