//! A store's stock at the till: `stock_on_hand`, `stock_adjust_now`,
//! `stock_adjust_prepare` and `stock_adjust` (`ref/ipc-contract.md` §3,
//! "Stock"; microstep 1.10.5, its terminal half).
//!
//! Each is a thin call into the domain and database half 1.10.5 delivered on
//! 4 November 2026, run in the shell state 1.8.2c created: the register's
//! database, the signed-in user and stored-approval resolution.
//!
//! Every path that increases stock arrives in Phase 4, so without an
//! adjustment every product goes negative from its first sale, and the
//! negative-stock flag means nothing.
//!
//! **One person moves stock by hand, unless the merchant's policy names
//! `stock.adjust`** (the owner's decision of 10 November 2026, as for a price
//! override and a PIN reset). `stock_adjust_now` posts the movement at once,
//! under the actor's own `stock.adjust`: the event and its audit row travel
//! in one envelope, and no request is stored. Otherwise a shop with one
//! manager could never correct its stock, since nobody approves their own
//! request. Where the policy names `stock.adjust`, `stock_adjust_now` is
//! refused, and stock moves in two steps:
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
//! `sale.create` or `stock.adjust`: what the shelf should hold, and the cost
//! a unit is carried at, which is unknown rather than zero when no cost was
//! ever recorded. A cashier reads it to sell, and the owner, who holds
//! `stock.adjust` and sells nothing, to adjust it (the owner's decision of 11
//! November 2026).
//!
//! **Only a holder of `stock.adjust` is shown the cost** (the owner's
//! decision of 10 November 2026). A product's cost is what the shop paid for
//! it, and beside the shelf price it is the shop's margin, so a cashier is
//! shown what the shelf holds and never what it cost. What the signed-in user
//! holds decides, never their role's name, so a merchant who grants a
//! cashier `stock.adjust` shows them the cost as well.

use pos_db::DbError;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::product::ProductRepository;
use pos_db::repo::stock;
use pos_db::repo::stock::StockRepository;
use pos_db::repo::stock_adjustment::StockAdjustmentRepository;
use pos_domain::{
    ApprovalBinding, ApprovalId, AuditIntent, Capability, EscalationPolicy, ProductId, Qty,
    StockAdjustmentReason, StockAdjustmentRequest, StockError, StockEventId, StockPosition,
    Timestamp, UserId, cap,
};
use serde::Serialize;
use serde_json::json;
use tauri::State;

use super::cart::{open_shift, proof, signed_in_as};
use super::{Fact, audit_fact, sqlite, stored};
use crate::ipc::context::{authorize, merchant_policy, permission_refused};
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
    /// What a unit is carried at, or `None` for anyone who does not hold
    /// `stock.adjust`, who is never shown it. A cost that was never recorded
    /// is shown inside it as unknown, so a hidden cost is never mistaken for
    /// an unknown one.
    pub cost: Option<StockCost>,
}

/// The cost a unit of stock is carried at, as a holder of `stock.adjust` is
/// shown it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StockCost {
    /// The currency `wac_minor` is in, by its ISO code.
    pub currency: &'static str,
    /// The weighted average cost of one unit, in minor units, or `None` when
    /// no cost was ever recorded: shown as unknown, never as zero.
    pub wac_minor: Option<i64>,
    /// Whether any cost behind `wac_minor` was projected or missing.
    pub is_wac_estimated: bool,
}

impl StockSnapshot {
    /// What `position` holds of `product_id`, its cost only when
    /// `sees_cost`.
    fn of(product_id: ProductId, position: &StockPosition, sees_cost: bool) -> StockSnapshot {
        StockSnapshot {
            product_id,
            on_hand_milli: position.on_hand.milli(),
            cost: sees_cost.then(|| StockCost {
                currency: position.wac.currency().code(),
                wac_minor: position.cost_basis().map(|wac| wac.minor()),
                is_wac_estimated: position.is_wac_estimated,
            }),
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
pub fn stock_adjust_now(
    till: State<'_, Till>,
    product_id: ProductId,
    qty_delta_milli: i64,
    reason_code: String,
    note: Option<String>,
) -> IpcResult<StockSnapshot> {
    adjust_now(
        till.register()?,
        &Adjustment {
            product_id,
            qty_delta_milli,
            reason_code: &reason_code,
            note,
        },
        &merchant_policy(),
    )
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
/// projection. Its cost is `None` unless the signed-in user holds
/// `stock.adjust`.
///
/// # Errors
///
/// What the context refuses (`not_signed_in`, `permission_denied` holding
/// neither `sale.create` nor `stock.adjust`); `product_unknown` for an id
/// that names no product; and `storage_failed`.
pub fn on_hand(register: &Register, product_id: ProductId) -> IpcResult<StockSnapshot> {
    let actor = stock_reader(register, register.now())?;
    let (db, session) = register.lock();
    signed_in_as(session.as_ref(), actor)?;
    // What the signed-in user holds decides, read from the grants they
    // signed in with, as every proof is.
    let sees_cost = session
        .as_ref()
        .is_some_and(|session| session.grants.holds(cap::StockAdjust::NAME));
    drop(session);
    ProductRepository::new(&db)
        .by_id(product_id)
        .map_err(stored)?
        .ok_or_else(product_unknown)?;
    let position = StockRepository::new(&db)
        .position(product_id, register.store())
        .map_err(stored)?;
    Ok(StockSnapshot::of(product_id, &position, sees_cost))
}

/// `stock_on_hand`'s proof, as its registry row names it: `sale.create`,
/// which every role that sells holds, or else `stock.adjust`, which the
/// owner holds without selling. Answers who proved it. Holding neither is
/// `permission_denied`, as it was.
fn stock_reader(register: &Register, at: Timestamp) -> IpcResult<UserId> {
    match proof::<cap::SaleCreate>(register, at) {
        Ok(auth) => Ok(auth.actor()),
        Err(refused) if refused.code == "permission_denied" => {
            proof::<cap::StockAdjust>(register, at).map(|auth| auth.actor())
        }
        Err(other) => Err(other),
    }
}

/// A movement of stock by hand, as `stock_adjust_now` is asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adjustment<'a> {
    pub product_id: ProductId,
    /// Signed, in milli-units: negative leaves the shelf, positive arrives.
    pub qty_delta_milli: i64,
    /// One of the five reasons, by its wire code.
    pub reason_code: &'a str,
    pub note: Option<String>,
}

/// `stock_adjust_now`: post `asked` at once, under the signed-in user's
/// `stock.adjust`, where `policy` does not name `stock.adjust`. Answers the
/// position the event leaves, with its cost, as `stock_adjust` does.
///
/// The event is the one an approved request posts: an `adjust` event naming
/// its reason and note, by the person signed in, dated by the register's open
/// shift, and stock that arrives is valued at the cost basis in force, marked
/// projected. The event and its `stock.adjust` audit row, whose reason is the
/// reason code, travel in one envelope (I-9). Nothing is stored for anyone to
/// approve.
///
/// # Errors
///
/// - What the context refuses: `not_signed_in`, `permission_denied` without
///   `stock.adjust`, and `approval_required` where `policy` names
///   `stock.adjust`, whose way is `stock_adjust_prepare` then `stock_adjust`.
/// - `stock_reason_unknown` for a code that names none of the five, and
///   `stock_adjustment_against_reason` for a movement its reason does not
///   allow, a movement of nothing among them.
/// - `shift_not_open` when the register has no open shift to date it by.
/// - `product_unknown`; `qty_not_whole` for a quantity that is not a whole
///   number of the product's steps; and `storage_failed`.
///
/// A refused adjustment writes nothing.
pub fn adjust_now(
    register: &Register,
    asked: &Adjustment<'_>,
    policy: &EscalationPolicy,
) -> IpcResult<StockSnapshot> {
    let at = register.now();
    let stock_event_id = StockEventId::from_uuid(register.next_id());
    // What an approval would name, were one asked for: this event, and an
    // amount of zero. Where the policy names `stock.adjust` none can be
    // given here, so the context refuses it as `approval_required`.
    let binding = ApprovalBinding {
        entity_id: stock_event_id.as_uuid(),
        amount_minor: 0,
        content_hash: None,
    };
    let auth = authorize::<cap::StockAdjust>(register, None, &binding, policy, at)?;
    let reason =
        StockAdjustmentReason::from_code(asked.reason_code).map_err(|e| stock_refused(&e))?;
    let request = StockAdjustmentRequest {
        stock_event_id,
        product_id: asked.product_id,
        qty_delta: Qty::from_milli(asked.qty_delta_milli),
        reason,
        note: asked.note.clone(),
    };
    // A movement its reason does not allow is refused as the event is
    // captured (`StockEvent::capture`), with the error `stock_adjust_prepare`
    // gives, so the rule is held in one place.
    let (mut db, session) = register.lock();
    signed_in_as(session.as_ref(), auth.actor())?;
    drop(session);
    let shift = open_shift(&db, register)?;

    let tx = db.transaction().map_err(sqlite)?;
    let ledger = StockRepository::new(&tx);
    let before = ledger
        .position(request.product_id, register.store())
        .map_err(stored)?;
    let movement = request.movement(
        register.store(),
        &before,
        auth.actor(),
        at,
        shift.business_date,
    );
    let prepared = ledger
        .prepare(&tx, register.register(), &[movement])
        .map_err(|e| adjustment_refused(&e))?;
    let Some(event) = prepared.first() else {
        return Err(IpcError::new("storage_failed", "error.storage.failed"));
    };
    let payload = event.payload();
    audit_fact(
        register,
        &tx,
        register.next_id(),
        &Fact {
            entity: stock::ENTITY,
            id: stock_event_id.as_uuid(),
            payload: &payload,
        },
        &AuditIntent {
            actor: auth.actor(),
            approver: None,
            approval: None,
            action: cap::StockAdjust::NAME,
            entity: stock::ENTITY,
            entity_id: stock_event_id.as_uuid(),
            reason: Some(reason.as_str().to_owned()),
            payload: json!({
                "amount_minor": 0,
                "product_id": request.product_id.as_uuid().hyphenated().to_string(),
                "qty_delta_milli": request.qty_delta.milli(),
                "reason_code": reason.as_str(),
            }),
            at,
        },
    )?;
    ledger.append(&tx, &prepared).map_err(stored)?;
    let after = event.event().after;
    tx.commit().map_err(sqlite)?;
    // Only a holder of `stock.adjust` posts, so the cost is shown.
    Ok(StockSnapshot::of(request.product_id, &after, true))
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
    // Only a holder of `stock.adjust` posts, so the cost is shown.
    Ok(StockSnapshot::of(
        approved.event().event().product_id,
        &after,
        true,
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

#[cfg(test)]
mod tests {
    //! 1.10.5's terminal half, through the shell, on a register file built by
    //! `pos_db::open`. The shop is the integration suites'
    //! (`tests/shop/mod.rs`): ten bottles of oil, five packs of tobacco and
    //! half a kilogram of cheese, each at a cost of 0.100, and no bread. Every
    //! figure is worked out by hand, never read back from the code under test.
    //!
    //! Every approval here is stored as the approval screen will store one,
    //! behind its envelope: `auth_verify_pin`, which issues them at the
    //! counter, is 1.11.4b's.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use pos_db::repo::approval::ApprovalRepository;
    use pos_domain::{ApprovalHandle, GrantSet, Role, UserId};
    use uuid::Uuid;

    use super::*;
    use crate::commands::auth::{login_pin, logout};
    use crate::commands::cart::{add_product, open_cart};
    use crate::commands::sale::finalize;
    use crate::commands::tender::{add_cash, begin_payment};
    use crate::shop::{
        AT, BREAD, CASHIER_CODE, CASHIER_PIN, CHEESE, LEAD_CODE, LEAD_ID, MANAGER_CODE, MANAGER_ID,
        OIL, OWNER_CODE, OWNER_ID, add_staff, at, field, open, product_id, refusal, seed, shop,
    };

    /// A second manager, who can ask for an adjustment and approve another's.
    const DEPUTY_ID: [u8; 16] = [0xAB; 16];
    const DEPUTY_CODE: &str = "M-200";
    const REASON: &str = "synthetic shelf check";
    const NOTE: &str = "synthetic: one bottle dropped";

    fn user(id: [u8; 16]) -> UserId {
        UserId::from_uuid(Uuid::from_bytes(id))
    }

    /// The managers and the owner beside the shop's cashier.
    fn staff(register: &Register) {
        add_staff(
            register,
            MANAGER_ID,
            MANAGER_CODE,
            "Synthetic Manager",
            "manager",
        );
        add_staff(
            register,
            DEPUTY_ID,
            DEPUTY_CODE,
            "Synthetic Deputy",
            "manager",
        );
        add_staff(register, OWNER_ID, OWNER_CODE, "Synthetic Owner", "owner");
    }

    /// The shop, its cashier signed out and the manager signed in.
    fn back_office() -> (tempfile::TempDir, Register) {
        let (dir, register) = shop();
        logout(&register).unwrap();
        staff(&register);
        login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();
        (dir, register)
    }

    /// An approval from `approver`, holding `role`, for `actor` to post the
    /// request `event`: bound to it as it is stored now, with `amount_minor`,
    /// stored behind its envelope.
    fn approve(
        register: &Register,
        actor: [u8; 16],
        (approver, role): ([u8; 16], Role),
        event: StockEventId,
        amount_minor: i64,
    ) -> ApprovalId {
        let bound = register.with_db(|conn| {
            let tx = conn.transaction().unwrap();
            StockAdjustmentRepository::new(&tx)
                .binding(&tx, event)
                .unwrap()
        });
        let binding = ApprovalBinding {
            amount_minor,
            ..bound
        };
        let id = ApprovalId::from_uuid(register.next_id());
        let proof = pos_domain::authorize::<cap::StockAdjust>(
            user(approver),
            &GrantSet::of_role(role),
            None,
            &binding,
            &EscalationPolicy::empty(),
            at(),
        )
        .unwrap();
        // The nonce is derived from a fresh id, never written as a literal.
        let nonce = *register.next_id().as_bytes();
        let handle = ApprovalHandle::issue::<cap::StockAdjust>(
            id,
            user(actor),
            &proof,
            &binding,
            REASON.to_owned(),
            at(),
            60_000,
            nonce,
        )
        .unwrap();
        register.with_db(|conn| {
            let tx = conn.transaction().unwrap();
            let (change, commit) = (register.next_id(), register.next_id());
            OutboxRepository::new(&tx)
                .write_commit(
                    &tx,
                    &CommitEnvelope {
                        commit_id: commit.as_bytes(),
                        protocol_version: 1,
                        schema_version: pos_db::SCHEMA_VERSION,
                        producer_version: "stock-suite",
                        created_at: AT,
                    },
                    &[FactMember {
                        change_id: change.as_bytes(),
                        entity: "approval_handle",
                        entity_id: id.as_uuid().as_bytes(),
                        payload: "{}",
                    }],
                )
                .unwrap();
            ApprovalRepository::new(&tx).insert(&tx, &handle).unwrap();
            tx.commit().unwrap();
        });
        id
    }

    /// The owner's approval of the manager's request `event`.
    fn owners(register: &Register, event: StockEventId) -> ApprovalId {
        approve(register, MANAGER_ID, (OWNER_ID, Role::Owner), event, 0)
    }

    /// The manager asks to write off one bottle of oil as damaged.
    fn one_bottle_damaged(register: &Register) -> StockAdjustmentSnapshot {
        prepare_adjustment(
            register,
            product_id(OIL),
            -1_000,
            "damage",
            Some(NOTE.to_owned()),
        )
        .unwrap()
    }

    fn requests(register: &Register) -> i64 {
        count(register, "SELECT count(*) FROM stock_adjustment_request")
    }

    fn count(register: &Register, sql: &str) -> i64 {
        register.with_db(|conn| conn.query_row(sql, [], |r| r.get(0)).unwrap())
    }

    fn changes(register: &Register) -> u64 {
        register.with_db(|conn| conn.total_changes())
    }

    fn consumed(register: &Register, approval: ApprovalId) -> bool {
        register.with_db(|conn| ApprovalRepository::new(conn).is_consumed(approval).unwrap())
    }

    /// One bottle of oil, sold for cash by whoever is signed in.
    fn sell_one_bottle(register: &Register) {
        open_cart(register).unwrap();
        add_product(register, product_id(OIL), None).unwrap();
        let paying = begin_payment(register).unwrap();
        add_cash(register, paying.cash_due_minor).unwrap();
        finalize(register).unwrap();
    }

    /// The oil sale's stock event: what it moved, and the cost it captured.
    fn sale_event(register: &Register) -> (i64, Option<i64>, i64) {
        register.with_db(|conn| {
            conn.query_row(
                "SELECT qty_delta_milli, unit_cost_minor, is_cost_estimated
                   FROM stock_ledger WHERE kind = 'sale' AND product_id = ?1",
                [product_id(OIL).as_uuid().as_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        })
    }

    fn hyphenated(id: [u8; 16]) -> String {
        Uuid::from_bytes(id).hyphenated().to_string()
    }

    /// The cost a holder of `stock.adjust` is shown, in dinars.
    fn carried_at(wac_minor: Option<i64>, is_wac_estimated: bool) -> Option<StockCost> {
        Some(StockCost {
            currency: "JOD",
            wac_minor,
            is_wac_estimated,
        })
    }

    /// The `Done when` through the shell: the shop's ten bottles of oil, one
    /// sold, nine on hand; one written off as damaged with the owner's
    /// approval, eight on hand; and the sale's event still carries the cost
    /// it captured. The adjustment's event, its audit row and the approval's
    /// consumption travel in one envelope, and the request is gone. The
    /// cashier is shown the shelf, the manager its cost as well.
    #[test]
    fn a_sale_and_an_approved_adjustment_move_stock_and_keep_the_sales_cost() {
        let (_dir, register) = shop();
        let oil = product_id(OIL);
        assert_eq!(
            on_hand(&register, oil).unwrap(),
            StockSnapshot {
                product_id: oil,
                on_hand_milli: 10_000,
                cost: None,
            }
        );
        sell_one_bottle(&register);
        assert_eq!(on_hand(&register, oil).unwrap().on_hand_milli, 9_000);
        let sold = sale_event(&register);
        assert_eq!(sold, (-1_000, Some(100), 0));

        logout(&register).unwrap();
        staff(&register);
        login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();
        let asked = one_bottle_damaged(&register);
        assert_eq!(
            (
                asked.product_id,
                asked.qty_delta_milli,
                asked.reason_code,
                asked.note.as_deref()
            ),
            (oil, -1_000, "damage", Some(NOTE))
        );
        let stocked = StockSnapshot {
            product_id: oil,
            on_hand_milli: 9_000,
            cost: carried_at(Some(100), false),
        };
        assert_eq!(
            on_hand(&register, oil).unwrap(),
            stocked,
            "asking moves nothing"
        );
        let approval = owners(&register, asked.stock_event_id);
        let after = adjust(&register, asked.stock_event_id, approval).unwrap();
        assert_eq!(
            after,
            StockSnapshot {
                on_hand_milli: 8_000,
                ..stocked
            }
        );
        assert_eq!(on_hand(&register, oil).unwrap(), after);
        assert_eq!(
            sale_event(&register),
            sold,
            "the sale's event is never revalued"
        );

        let event = asked.stock_event_id.as_uuid();
        let posted: (
            String,
            i64,
            Option<i64>,
            String,
            Option<String>,
            Vec<u8>,
            String,
        ) = register.with_db(|conn| {
            conn.query_row(
                "SELECT kind, qty_delta_milli, unit_cost_minor, reason_code, note,
                            actor_id, business_date
                       FROM stock_ledger WHERE id = ?1",
                [event.as_bytes().as_slice()],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .unwrap()
        });
        assert_eq!(
            posted,
            (
                "adjust".to_owned(),
                -1_000,
                Some(100),
                "damage".to_owned(),
                Some(NOTE.to_owned()),
                MANAGER_ID.to_vec(),
                "2026-11-05".to_owned(),
            ),
            "posted under the request's id, by the one who asked, on the shift's day"
        );

        // The audit row: who, with whose approval, why, and what moved.
        let who: [Vec<u8>; 4] = register.with_db(|conn| {
            conn.query_row(
                "SELECT actor_id, approver_id, approval_handle_id, entity_id
                   FROM audit_log WHERE action = 'stock.adjust'",
                [],
                |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]),
            )
            .unwrap()
        });
        assert_eq!(
            who,
            [
                MANAGER_ID.to_vec(),
                OWNER_ID.to_vec(),
                approval.as_uuid().as_bytes().to_vec(),
                event.as_bytes().to_vec(),
            ]
        );
        let (entity, reason, payload, audit_id): (String, String, String, Vec<u8>) = register
            .with_db(|conn| {
                conn.query_row(
                    "SELECT entity, reason, payload, id
                       FROM audit_log WHERE action = 'stock.adjust'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .unwrap()
            });
        assert_eq!((entity.as_str(), reason.as_str()), ("stock_ledger", REASON));
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload,
            serde_json::json!({
                "amount_minor": 0,
                "product_id": oil.as_uuid().hyphenated().to_string(),
                "qty_delta_milli": -1_000,
                "reason_code": "damage",
            })
        );
        assert!(consumed(&register, approval));
        let consumption: (Vec<u8>, Vec<u8>) = register.with_db(|conn| {
            conn.query_row(
                "SELECT effect_id, audit_log_id FROM approval_consumption WHERE handle_id = ?1",
                [approval.as_uuid().as_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        });
        assert_eq!(consumption, (event.as_bytes().to_vec(), audit_id.clone()));
        assert_eq!(requests(&register), 0, "the request is gone once posted");

        // One envelope: the audit row, travelling as the row it chains, the
        // event, and the consumption.
        let members: Vec<(String, Vec<u8>, String)> = register.with_db(|conn| {
            conn.prepare(
                "SELECT entity, entity_id, payload FROM fact_commit_member
                  WHERE commit_id = (SELECT commit_id FROM fact_commit_member
                                      WHERE entity = 'stock_ledger' AND entity_id = ?1)
                  ORDER BY commit_index",
            )
            .unwrap()
            .query_map([event.as_bytes().as_slice()], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
        });
        let entities: Vec<(&str, &[u8])> = members
            .iter()
            .map(|(entity, id, _)| (entity.as_str(), id.as_slice()))
            .collect();
        assert_eq!(
            entities,
            vec![
                ("audit_log", audit_id.as_slice()),
                ("stock_ledger", event.as_bytes().as_slice()),
                (
                    "approval_consumption",
                    approval.as_uuid().as_bytes().as_slice()
                ),
            ]
        );
        let travelling: serde_json::Value = members
            .first()
            .map(|(_, _, payload)| serde_json::from_str(payload).unwrap())
            .unwrap();
        assert_eq!(field(&travelling, "action"), "stock.adjust");
        assert_eq!(
            field(&travelling, "actor_id").as_str(),
            Some(hyphenated(MANAGER_ID).as_str())
        );
        assert_eq!(
            field(&travelling, "approver_id").as_str(),
            Some(hyphenated(OWNER_ID).as_str())
        );
        assert_eq!(
            field(&travelling, "approval_handle_id").as_str(),
            Some(approval.as_uuid().hyphenated().to_string().as_str())
        );
        assert_eq!(field(&travelling, "reason"), REASON);
        assert_eq!(
            field(&travelling, "payload"),
            &payload,
            "what travels is what is chained"
        );
        let delivering: i64 = register.with_db(|conn| {
            conn.query_row(
                "SELECT count(*) FROM fact_commit_member m
                   JOIN sync_outbox o ON o.change_id = m.change_id
                  WHERE o.state = 'pending'
                    AND m.commit_id = (SELECT commit_id FROM fact_commit_member
                                        WHERE entity = 'stock_ledger' AND entity_id = ?1)",
                [event.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .unwrap()
        });
        assert_eq!(delivering, 3, "each member waits to travel");
    }

    /// A manager reads stock and what it cost. A product never stocked holds
    /// nothing, at a cost nobody knows, which is never shown as zero.
    #[test]
    fn stock_on_hand_answers_the_stores_projection() {
        let (_dir, register) = back_office();
        assert_eq!(
            on_hand(&register, product_id(CHEESE)).unwrap(),
            StockSnapshot {
                product_id: product_id(CHEESE),
                on_hand_milli: 500,
                cost: carried_at(Some(100), false),
            }
        );
        assert_eq!(
            on_hand(&register, product_id(BREAD)).unwrap(),
            StockSnapshot {
                product_id: product_id(BREAD),
                on_hand_milli: 0,
                cost: carried_at(None, true),
            }
        );
        assert_eq!(
            refusal(on_hand(&register, product_id(0x99))).code,
            "product_unknown"
        );
        logout(&register).unwrap();
        assert_eq!(
            refusal(on_hand(&register, product_id(OIL))).code,
            "not_signed_in"
        );
    }

    /// The owner's decision of 10 November 2026 (D5): a cashier reads what
    /// the shelf holds, under `sale.create`, and never what it cost, and
    /// neither does a shift lead. A manager, who holds `stock.adjust`, is
    /// shown it. What the user holds decides, not their role's name: once
    /// the merchant grants the cashier `stock.adjust`, the cashier's next
    /// sign-in is shown the cost too.
    #[test]
    fn a_cashier_reads_stock_but_never_its_cost() {
        let (_dir, register) = shop();
        staff(&register);
        add_staff(
            &register,
            LEAD_ID,
            LEAD_CODE,
            "Synthetic Lead",
            "shift_lead",
        );
        let costs = |register: &Register| -> [Option<StockCost>; 3] {
            [OIL, CHEESE, BREAD].map(|product| on_hand(register, product_id(product)).unwrap().cost)
        };
        let hidden: [Option<StockCost>; 3] = [None, None, None];
        let shown = [
            carried_at(Some(100), false),
            carried_at(Some(100), false),
            carried_at(None, true),
        ];

        assert_eq!(costs(&register), hidden, "the cashier");
        // On the wire the cost is absent as a whole, its currency with it.
        let wire = serde_json::to_value(on_hand(&register, product_id(OIL)).unwrap()).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "product_id": product_id(OIL).as_uuid().hyphenated().to_string(),
                "on_hand_milli": 10_000,
                "cost": null,
            })
        );
        for (code, seen) in [(LEAD_CODE, hidden.clone()), (MANAGER_CODE, shown.clone())] {
            logout(&register).unwrap();
            login_pin(&register, code, CASHIER_PIN).unwrap();
            assert_eq!(costs(&register), seen, "{code}");
        }
        let wire = serde_json::to_value(on_hand(&register, product_id(BREAD)).unwrap()).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "product_id": product_id(BREAD).as_uuid().hyphenated().to_string(),
                "on_hand_milli": 0,
                "cost": { "currency": "JOD", "wac_minor": null, "is_wac_estimated": true },
            }),
            "a cost never recorded is shown as unknown, never as hidden"
        );

        register.with_db(|conn| {
            conn.execute(
                "UPDATE role_capability SET decision = 'granted', limit_json = NULL
                  WHERE capability = 'stock.adjust'
                    AND role_id = (SELECT id FROM role WHERE code = 'cashier')",
                [],
            )
            .unwrap()
        });
        logout(&register).unwrap();
        login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
        assert_eq!(costs(&register), shown, "the cashier, granted stock.adjust");
    }

    /// The owner's decision of 11 November 2026: stock is read under
    /// `sale.create` or `stock.adjust`. The owner, who holds `stock.adjust`
    /// and not `sale.create`, reads it, and its cost. One holding neither is
    /// refused, and the grants decide, not the role's name: where the
    /// merchant withholds `sale.create` from the cashier, who holds no
    /// `stock.adjust`, the cashier's next sign-in is refused.
    #[test]
    fn the_owner_reads_stock_and_one_holding_neither_capability_is_refused() {
        let (_dir, register) = shop();
        staff(&register);
        logout(&register).unwrap();
        login_pin(&register, OWNER_CODE, CASHIER_PIN).unwrap();
        assert_eq!(
            on_hand(&register, product_id(OIL)).unwrap(),
            StockSnapshot {
                product_id: product_id(OIL),
                on_hand_milli: 10_000,
                cost: carried_at(Some(100), false),
            }
        );
        assert_eq!(
            on_hand(&register, product_id(BREAD)).unwrap().cost,
            carried_at(None, true)
        );

        register.with_db(|conn| {
            conn.execute(
                "UPDATE role_capability SET decision = 'withheld', limit_json = NULL
                  WHERE capability = 'sale.create'
                    AND role_id = (SELECT id FROM role WHERE code = 'cashier')",
                [],
            )
            .unwrap()
        });
        logout(&register).unwrap();
        login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
        let before = changes(&register);
        assert_eq!(
            refusal(on_hand(&register, product_id(OIL))).code,
            "permission_denied"
        );
        assert_eq!(changes(&register), before, "the refusal wrote nothing");
    }

    /// `product` moved by `qty` for `reason`, as `stock_adjust_now` is asked.
    fn by_hand<'a>(product: u128, qty: i64, reason: &'a str, note: Option<&str>) -> Adjustment<'a> {
        Adjustment {
            product_id: product_id(product),
            qty_delta_milli: qty,
            reason_code: reason,
            note: note.map(str::to_owned),
        }
    }

    /// The owner's decision of 10 November 2026 (D1 B): one manager writes
    /// off a bottle of oil at once, under their own `stock.adjust`. The event
    /// names its reason, its note and the manager, on the shift's day. Its
    /// audit row names the manager, no second person, and the reason code,
    /// and the two travel in one envelope, the event first. Nothing is stored
    /// for anyone to approve, and no approval is spent. Stock found by a count
    /// arrives at the cost basis in force, marked projected.
    #[test]
    fn one_manager_adjusts_stock_at_once_and_it_is_audited() {
        let (_dir, register) = back_office();
        let oil = product_id(OIL);
        let after = adjust_now(
            &register,
            &by_hand(OIL, -1_000, "damage", Some(NOTE)),
            &merchant_policy(),
        )
        .unwrap();
        assert_eq!(
            after,
            StockSnapshot {
                product_id: oil,
                on_hand_milli: 9_000,
                cost: carried_at(Some(100), false),
            }
        );
        assert_eq!(on_hand(&register, oil).unwrap(), after);
        assert_eq!(requests(&register), 0, "nothing is stored for approval");

        type Posted = (String, i64, Option<i64>, Option<String>, Vec<u8>, String);
        let (event, posted): (Vec<u8>, Posted) = register.with_db(|conn| {
            conn.query_row(
                "SELECT id, kind, qty_delta_milli, unit_cost_minor, note, actor_id, business_date
                       FROM stock_ledger WHERE reason_code = 'damage'",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        (
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ),
                    ))
                },
            )
            .unwrap()
        });
        assert_eq!(
            posted,
            (
                "adjust".to_owned(),
                -1_000,
                Some(100),
                Some(NOTE.to_owned()),
                MANAGER_ID.to_vec(),
                "2026-11-05".to_owned(),
            ),
            "posted by the manager, on the shift's day"
        );

        // The audit row: the manager, no second person, and the reason code.
        type Audited = (
            Vec<u8>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            String,
            Vec<u8>,
            String,
        );
        let (who, payload, audit_id): (Audited, String, Vec<u8>) = register.with_db(|conn| {
            conn.query_row(
                "SELECT actor_id, approver_id, approval_handle_id, entity, entity_id, reason,
                        payload, id
                   FROM audit_log WHERE action = 'stock.adjust'",
                [],
                |r| {
                    Ok((
                        (
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                        ),
                        r.get(6)?,
                        r.get(7)?,
                    ))
                },
            )
            .unwrap()
        });
        assert_eq!(
            who,
            (
                MANAGER_ID.to_vec(),
                None,
                None,
                "stock_ledger".to_owned(),
                event.clone(),
                "damage".to_owned(),
            )
        );
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload,
            serde_json::json!({
                "amount_minor": 0,
                "product_id": oil.as_uuid().hyphenated().to_string(),
                "qty_delta_milli": -1_000,
                "reason_code": "damage",
            })
        );
        assert_eq!(
            count(&register, "SELECT count(*) FROM approval_consumption"),
            0
        );

        // One envelope: the event, then the audit row as the row it chains.
        let members: Vec<(String, Vec<u8>, String)> = register.with_db(|conn| {
            conn.prepare(
                "SELECT entity, entity_id, payload FROM fact_commit_member
                  WHERE commit_id = (SELECT commit_id FROM fact_commit_member
                                      WHERE entity = 'stock_ledger' AND entity_id = ?1)
                  ORDER BY commit_index",
            )
            .unwrap()
            .query_map([event.as_slice()], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
        });
        let entities: Vec<(&str, &[u8])> = members
            .iter()
            .map(|(entity, id, _)| (entity.as_str(), id.as_slice()))
            .collect();
        assert_eq!(
            entities,
            vec![
                ("stock_ledger", event.as_slice()),
                ("audit_log", audit_id.as_slice()),
            ]
        );
        let travelling: serde_json::Value = members
            .last()
            .map(|(_, _, payload)| serde_json::from_str(payload).unwrap())
            .unwrap();
        assert_eq!(field(&travelling, "action"), "stock.adjust");
        assert_eq!(field(&travelling, "approver_id"), &serde_json::Value::Null);
        assert_eq!(field(&travelling, "reason"), "damage");
        assert_eq!(
            field(&travelling, "payload"),
            &payload,
            "what travels is what is chained"
        );

        // Two bottles found by a count: valued at the cost in force, projected.
        let found = adjust_now(
            &register,
            &by_hand(OIL, 2_000, "count_correction", None),
            &merchant_policy(),
        )
        .unwrap();
        assert_eq!(
            found,
            StockSnapshot {
                product_id: oil,
                on_hand_milli: 11_000,
                cost: carried_at(Some(100), true),
            }
        );
        let captured: (Option<i64>, i64) = register.with_db(|conn| {
            conn.query_row(
                "SELECT unit_cost_minor, is_cost_estimated
                   FROM stock_ledger WHERE reason_code = 'count_correction'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        });
        assert_eq!(captured, (Some(100), 1));
    }

    /// Where a merchant's policy names `stock.adjust`, a direct adjustment is
    /// `approval_required` and writes nothing. The request is prepared,
    /// approved by someone else and posted instead, as before.
    #[test]
    fn a_direct_adjustment_is_refused_where_the_policy_escalates() {
        let (_dir, register) = back_office();
        let escalating = EscalationPolicy::empty().with_escalation::<cap::StockAdjust>();
        let before = changes(&register);
        assert_eq!(
            refusal(adjust_now(
                &register,
                &by_hand(OIL, -1_000, "damage", None),
                &escalating
            ))
            .code,
            "approval_required"
        );
        assert_eq!(changes(&register), before, "the refusal wrote nothing");
        let asked = one_bottle_damaged(&register);
        let approval = owners(&register, asked.stock_event_id);
        assert_eq!(
            adjust(&register, asked.stock_event_id, approval)
                .unwrap()
                .on_hand_milli,
            9_000
        );
    }

    /// A direct adjustment is held to every rule the two steps are: only a
    /// holder of `stock.adjust` moves stock, by one of the five reasons and
    /// its direction, a whole number of the product's steps, of a product
    /// that exists, dated by an open shift. Each refusal writes nothing.
    #[test]
    fn a_direct_adjustment_is_held_to_the_rules_of_the_two_steps() {
        let (_dir, register) = shop();
        staff(&register);
        let before = changes(&register);
        let now = |register: &Register, product: u128, qty: i64, reason: &str| {
            refusal(adjust_now(
                register,
                &by_hand(product, qty, reason, None),
                &merchant_policy(),
            ))
        };
        assert_eq!(
            now(&register, OIL, -1_000, "damage").code,
            "permission_denied"
        );
        assert_eq!(
            changes(&register),
            before,
            "the cashier's refusal wrote nothing"
        );
        logout(&register).unwrap();
        assert_eq!(now(&register, OIL, -1_000, "damage").code, "not_signed_in");
        login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();
        let before_manager = changes(&register);
        assert_eq!(
            now(&register, OIL, -1_000, "shrinkage").code,
            "stock_reason_unknown"
        );
        for (qty, reason, detail) in [
            (
                1_000,
                "damage",
                "damage, theft and expiry only take stock away",
            ),
            (-1_000, "opening_stock", "opening stock only arrives"),
            (
                0,
                "count_correction",
                "a count correction moves stock one way or the other",
            ),
        ] {
            let refused = now(&register, OIL, qty, reason);
            assert_eq!(
                (refused.code, refused.detail),
                ("stock_adjustment_against_reason", Some(detail))
            );
        }
        assert_eq!(
            now(&register, 0x99, -1_000, "damage").code,
            "product_unknown"
        );
        assert_eq!(now(&register, OIL, -1_500, "damage").code, "qty_not_whole");
        assert_eq!(
            changes(&register),
            before_manager,
            "no refusal wrote anything"
        );
        assert_eq!(
            on_hand(&register, product_id(OIL)).unwrap().on_hand_milli,
            10_000
        );

        // With no shift open there is no day to date it by.
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), false);
        let closed = open(dir.path());
        staff(&closed);
        login_pin(&closed, MANAGER_CODE, CASHIER_PIN).unwrap();
        let before = changes(&closed);
        assert_eq!(now(&closed, OIL, -1_000, "damage").code, "shift_not_open");
        assert_eq!(changes(&closed), before);
    }

    /// Only a holder of `stock.adjust` asks or posts; a cashier holds it
    /// not, and no one signed in holds nothing. A cashier learns nothing of a
    /// stored request.
    #[test]
    fn only_a_holder_of_stock_adjust_asks_or_posts() {
        let (_dir, register) = back_office();
        let asked = one_bottle_damaged(&register);
        let approval = owners(&register, asked.stock_event_id);
        logout(&register).unwrap();
        let before = changes(&register);
        assert_eq!(
            refusal(prepare_adjustment(
                &register,
                product_id(OIL),
                -1_000,
                "damage",
                None
            ))
            .code,
            "not_signed_in"
        );
        assert_eq!(
            refusal(adjust(&register, asked.stock_event_id, approval)).code,
            "not_signed_in"
        );
        assert_eq!(changes(&register), before);

        login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
        let before = changes(&register);
        assert_eq!(
            refusal(prepare_adjustment(
                &register,
                product_id(OIL),
                -1_000,
                "damage",
                None
            ))
            .code,
            "permission_denied"
        );
        assert_eq!(
            refusal(adjust(&register, asked.stock_event_id, approval)).code,
            "permission_denied"
        );
        assert_eq!(
            refusal(adjust(
                &register,
                StockEventId::from_uuid(register.next_id()),
                approval
            ))
            .code,
            "permission_denied",
            "the same answer whether a request exists or not"
        );
        assert_eq!(changes(&register), before, "the refusals wrote nothing");
        assert_eq!(requests(&register), 1);
        assert!(!consumed(&register, approval));
    }

    /// A request no approval could make right is refused before it is
    /// stored: a reason that is none of the five, a movement its reason does
    /// not allow, a movement of nothing, a product that does not exist, and a
    /// quantity that is not a whole number of the product's steps.
    #[test]
    fn a_request_no_approval_could_make_right_is_refused_and_stores_nothing() {
        let (_dir, register) = back_office();
        let before = changes(&register);
        let ask = |product: u128, qty: i64, reason: &str| {
            refusal(prepare_adjustment(
                &register,
                product_id(product),
                qty,
                reason,
                None,
            ))
        };
        let shrinkage = ask(OIL, -1_000, "shrinkage");
        assert_eq!(shrinkage.code, "stock_reason_unknown");
        let found_damage = ask(OIL, 1_000, "damage");
        assert_eq!(
            (found_damage.code, found_damage.detail),
            (
                "stock_adjustment_against_reason",
                Some("damage, theft and expiry only take stock away")
            )
        );
        let lost_opening = ask(OIL, -1_000, "opening_stock");
        assert_eq!(
            (lost_opening.code, lost_opening.detail),
            (
                "stock_adjustment_against_reason",
                Some("opening stock only arrives")
            )
        );
        let nothing = ask(OIL, 0, "count_correction");
        assert_eq!(
            (nothing.code, nothing.detail),
            (
                "stock_adjustment_against_reason",
                Some("a count correction moves stock one way or the other")
            )
        );
        assert_eq!(ask(0x99, -1_000, "damage").code, "product_unknown");
        assert_eq!(ask(OIL, -1_500, "damage").code, "qty_not_whole");
        assert_eq!(changes(&register), before, "nothing was stored");
        assert_eq!(requests(&register), 0);

        // A weighed product moves by the gram, and a count corrects upward.
        let found =
            prepare_adjustment(&register, product_id(CHEESE), 250, "count_correction", None)
                .unwrap();
        assert_eq!(
            (found.qty_delta_milli, found.reason_code, found.note),
            (250, "count_correction", None)
        );
        assert_eq!(requests(&register), 1);
    }

    /// The approval must name this request, exactly zero, and be given to
    /// the one posting it, unspent. Each refusal writes nothing, and the
    /// request and the approval stay as they were.
    #[test]
    fn an_adjustment_spends_only_an_approval_of_exactly_its_request() {
        let (_dir, register) = back_office();
        let first = one_bottle_damaged(&register);
        let second = one_bottle_damaged(&register);
        let for_first = owners(&register, first.stock_event_id);
        let for_ten_fils = approve(
            &register,
            MANAGER_ID,
            (OWNER_ID, Role::Owner),
            second.stock_event_id,
            10,
        );
        let for_the_deputy = approve(
            &register,
            DEPUTY_ID,
            (OWNER_ID, Role::Owner),
            second.stock_event_id,
            0,
        );
        let before = changes(&register);

        let another = refusal(adjust(&register, second.stock_event_id, for_first));
        assert_eq!(
            (another.code, another.detail),
            (
                "approval_mismatch",
                Some("the approval names something else")
            )
        );
        let amount = refusal(adjust(&register, second.stock_event_id, for_ten_fils));
        assert_eq!(
            (amount.code, amount.detail),
            (
                "approval_mismatch",
                Some("the approval is for another amount")
            )
        );
        let someone_else = refusal(adjust(&register, second.stock_event_id, for_the_deputy));
        assert_eq!(
            (someone_else.code, someone_else.detail),
            (
                "approval_mismatch",
                Some("the approval was given to someone else")
            )
        );
        assert_eq!(
            refusal(adjust(
                &register,
                second.stock_event_id,
                ApprovalId::from_uuid(register.next_id())
            ))
            .code,
            "approval_unknown"
        );
        assert_eq!(
            refusal(adjust(
                &register,
                StockEventId::from_uuid(register.next_id()),
                for_first
            ))
            .code,
            "stock_adjustment_unknown"
        );
        assert_eq!(changes(&register), before, "every refusal wrote nothing");
        for approval in [for_first, for_ten_fils, for_the_deputy] {
            assert!(!consumed(&register, approval));
        }
        assert_eq!(requests(&register), 2);

        // Spent once, on its own request; then neither that request nor any
        // other can spend it again.
        assert_eq!(
            adjust(&register, first.stock_event_id, for_first)
                .unwrap()
                .on_hand_milli,
            9_000
        );
        let before = changes(&register);
        assert_eq!(
            refusal(adjust(&register, first.stock_event_id, for_first)).code,
            "stock_adjustment_unknown",
            "a posted request is gone"
        );
        assert_eq!(
            refusal(adjust(&register, second.stock_event_id, for_first)).code,
            "approval_used"
        );
        assert_eq!(changes(&register), before);
        assert_eq!(
            on_hand(&register, product_id(OIL)).unwrap().on_hand_milli,
            9_000
        );
    }

    /// The approver was shown who asked. So a request is posted only by the
    /// person who asked for it, even with an approval given to the one
    /// posting it.
    #[test]
    fn only_the_person_who_asked_posts_a_request() {
        let (_dir, register) = back_office();
        let asked = one_bottle_damaged(&register);
        logout(&register).unwrap();
        login_pin(&register, DEPUTY_CODE, CASHIER_PIN).unwrap();
        let for_the_deputy = approve(
            &register,
            DEPUTY_ID,
            (OWNER_ID, Role::Owner),
            asked.stock_event_id,
            0,
        );
        let before = changes(&register);
        assert_eq!(
            refusal(adjust(&register, asked.stock_event_id, for_the_deputy)).code,
            "stock_adjustment_not_yours"
        );
        assert_eq!(changes(&register), before, "the refusal wrote nothing");
        assert!(!consumed(&register, for_the_deputy));

        logout(&register).unwrap();
        login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();
        let approval = owners(&register, asked.stock_event_id);
        assert_eq!(
            adjust(&register, asked.stock_event_id, approval)
                .unwrap()
                .on_hand_milli,
            9_000
        );
    }

    /// A request changed after it was stored is not what the approver would
    /// be shown, so no approval of it is spent. Once an approval names a
    /// request, `0006` refuses any change to it at all.
    #[test]
    fn a_request_changed_after_it_was_stored_is_refused() {
        let (_dir, register) = back_office();
        let asked = one_bottle_damaged(&register);
        let approval = owners(&register, asked.stock_event_id);
        let event = asked.stock_event_id.as_uuid();
        let refused = register.with_db(|conn| {
            conn.execute(
                "UPDATE stock_adjustment_request SET qty_delta_milli = -5000
                  WHERE stock_event_id = ?1",
                [event.as_bytes().as_slice()],
            )
        });
        assert!(refused.is_err(), "an approved request cannot change");

        let other = one_bottle_damaged(&register);
        register.with_db(|conn| {
            conn.execute(
                "UPDATE stock_adjustment_request SET qty_delta_milli = -5000
                  WHERE stock_event_id = ?1",
                [other.stock_event_id.as_uuid().as_bytes().as_slice()],
            )
            .unwrap()
        });
        let before = changes(&register);
        let altered = refusal(adjust(&register, other.stock_event_id, approval));
        assert_eq!(
            (altered.code, altered.detail),
            (
                "stock_adjustment_altered",
                Some("the stored request is not what was asked for")
            )
        );
        assert_eq!(changes(&register), before);
        assert!(!consumed(&register, approval));
        assert_eq!(
            on_hand(&register, product_id(OIL)).unwrap().on_hand_milli,
            10_000
        );

        assert_eq!(
            adjust(&register, asked.stock_event_id, approval)
                .unwrap()
                .on_hand_milli,
            9_000,
            "the approved request posts as it was asked"
        );
    }

    /// An adjustment is dated by the register's open shift. The register
    /// trusts no clock, so with none open it waits, and nothing is spent.
    #[test]
    fn an_adjustment_waits_for_an_open_shift() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path(), false);
        let register = open(dir.path());
        staff(&register);
        login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();
        let asked = one_bottle_damaged(&register);
        let approval = owners(&register, asked.stock_event_id);
        let before = changes(&register);
        assert_eq!(
            refusal(adjust(&register, asked.stock_event_id, approval)).code,
            "shift_not_open"
        );
        assert_eq!(changes(&register), before);
        assert!(!consumed(&register, approval));
        assert_eq!(requests(&register), 1);
        assert_eq!(
            on_hand(&register, product_id(OIL)).unwrap().on_hand_milli,
            10_000
        );
    }

    /// A request is stored, so a restart between asking and posting loses
    /// nothing: the next process on the same file posts it, once.
    #[test]
    fn a_request_survives_a_restart_and_posts_once() {
        let (dir, register) = back_office();
        let asked = one_bottle_damaged(&register);
        // A crash: the register dropped without signing out.
        drop(register);
        let register = open(dir.path());
        login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();
        let approval = owners(&register, asked.stock_event_id);
        assert_eq!(
            adjust(&register, asked.stock_event_id, approval)
                .unwrap()
                .on_hand_milli,
            9_000
        );
        drop(register);
        let register = open(dir.path());
        login_pin(&register, MANAGER_CODE, CASHIER_PIN).unwrap();
        assert_eq!(
            refusal(adjust(&register, asked.stock_event_id, approval)).code,
            "stock_adjustment_unknown"
        );
        assert_eq!(
            count(
                &register,
                "SELECT count(*) FROM stock_ledger WHERE reason_code = 'damage'"
            ),
            1
        );
        assert_eq!(
            on_hand(&register, product_id(OIL)).unwrap().on_hand_milli,
            9_000
        );
    }
}
