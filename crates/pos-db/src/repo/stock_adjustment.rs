//! Stock moved by hand, with a manager's approval (microstep 1.10.5,
//! `ref/schema.md` §0006, `ref/domain-api.md` §8.1).
//!
//! A person who holds `stock.adjust` asks for an adjustment: a product, a
//! signed quantity, one of five reasons and an optional note. The request is
//! stored first, in register-local `stock_adjustment_request`, under the id the
//! ledger event will carry, with the digest of exactly what was asked. A
//! different manager approves *that* digest; nothing else can be spent against
//! the approval. The effect then posts the event, its audit row and the
//! approval's consumption in one transaction, and only that transaction may
//! remove the request.
//!
//! The four steps, each the caller's transaction:
//!
//! 1. **[`StockAdjustmentRepository::prepare`]** checks the request against the
//!    domain and the product, and stores it with its [`PreparedIntentHash`].
//! 2. **[`StockAdjustmentRepository::binding`]** is what an approval must
//!    carry: the event's id, an amount of 0, and the digest recomputed from the
//!    stored row. The shell issues the handle against it.
//! 3. **[`StockAdjustmentRepository::prepare_effect`]** reloads the row,
//!    recomputes its digest and refuses one that no longer matches. It loads
//!    the approval the caller's proof names and holds it to the same binding,
//!    then prepares the stock event. The caller writes the envelope with
//!    [`ApprovedAdjustment::facts`].
//! 4. **[`StockAdjustmentRepository::commit`]** writes the audit row, the
//!    event, the consumption, and removes the request, in the order `0004`'s
//!    and `0006`'s triggers require.
//!
//! The digest is checked twice, deliberately. This module recomputes it from
//! the row before the effect. `0006`'s `stock_adjustment_request_no_update_after_approval`
//! independently refuses every `UPDATE` once an approval names the request,
//! so a bug in either cannot by itself turn an approved quantity, reason or
//! product into another.

use pos_domain::{
    ApprovalBinding, ApprovalHandle, AuditIntent, Authorized, BusinessDate, Capability,
    IntentColumn, IntentValue, PermissionError, PreparedIntent, PreparedIntentHash, ProductId, Qty,
    RegisterId, StockAdjustmentReason, StockAdjustmentRequest, StockEventId, StockPosition,
    StoreId, Timestamp, UserId, cap,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::json;
use uuid::Uuid;

use crate::DbError;
use crate::repo::approval::ApprovalRepository;
use crate::repo::audit::AuditRepository;
use crate::repo::stock::{self, PreparedStockEvent, StockRepository};

/// Writes and reads prepared stock adjustments.
pub struct StockAdjustmentRepository<'c> {
    conn: &'c Connection,
}

/// An approved adjustment, checked and prepared, waiting for its envelope.
///
/// Only [`StockAdjustmentRepository::prepare_effect`] makes one, so the event,
/// the approval and the audit row it carries are the ones that were checked.
#[derive(Debug)]
pub struct ApprovedAdjustment {
    request: StockAdjustmentRequest,
    event: PreparedStockEvent,
    handle: ApprovalHandle,
    actor: UserId,
    audit_id: Uuid,
    at: Timestamp,
}

impl ApprovedAdjustment {
    /// The stock event the effect posts.
    #[must_use]
    pub const fn event(&self) -> &PreparedStockEvent {
        &self.event
    }

    /// Every fact the effect writes, as `(entity, entity_id, payload)`, in
    /// the order they are written: the audit row, the stock event, and the
    /// approval's consumption. The caller's envelope names exactly these.
    #[must_use]
    pub fn facts(&self) -> Vec<(&'static str, [u8; 16], String)> {
        vec![
            ("audit_log", *self.audit_id.as_bytes(), "{}".to_owned()),
            (
                stock::ENTITY,
                *self.event.event().id.as_uuid().as_bytes(),
                self.event.payload(),
            ),
            (
                "approval_consumption",
                *self.handle.id().as_uuid().as_bytes(),
                "{}".to_owned(),
            ),
        ]
    }
}

impl<'c> StockAdjustmentRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `stock_adjust_prepare`: store `request` under the event id it will post
    /// as, asked for by `requested_by` at `requested_at`, and return the digest
    /// an approval must bind.
    ///
    /// # Errors
    ///
    /// [`DbError::StockRefused`] for a movement its reason does not allow,
    /// [`DbError::StockProductUnknown`] and [`DbError::StockQtyOffStep`] as for
    /// any stock event, and [`DbError::Sqlite`] for what the schema refuses — a
    /// second request under the same id among them.
    pub fn prepare(
        &self,
        tx: &Transaction<'_>,
        request: &StockAdjustmentRequest,
        requested_by: UserId,
        requested_at: Timestamp,
    ) -> Result<PreparedIntentHash, DbError> {
        request.check()?;
        let step = stock::product_step(tx, request.product_id)?;
        let qty = request.qty_delta.milli();
        if qty.checked_rem(step) != Some(0) {
            return Err(DbError::StockQtyOffStep {
                qty_milli: qty,
                step_milli: step,
            });
        }
        let row = Row {
            stock_event_id: request.stock_event_id.as_uuid().as_bytes().to_vec(),
            product_id: request.product_id.as_uuid().as_bytes().to_vec(),
            qty_delta_milli: qty,
            reason_code: request.reason.as_str().to_owned(),
            note: request.note.clone(),
            requested_by: requested_by.as_uuid().as_bytes().to_vec(),
            requested_at: requested_at.to_iso8601(),
        };
        let hash = row.hash();
        tx.execute(
            "INSERT INTO stock_adjustment_request
               (stock_event_id, product_id, qty_delta_milli, reason_code, note,
                requested_by, requested_at, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.stock_event_id,
                row.product_id,
                row.qty_delta_milli,
                row.reason_code,
                row.note,
                row.requested_by,
                row.requested_at,
                hash.to_stored().as_slice(),
            ],
        )?;
        Ok(hash)
    }

    /// What an approval of the request `id` must carry: the event's id, an
    /// amount of 0 (an adjustment moves no money), and the digest recomputed
    /// from the row as it is stored now.
    ///
    /// # Errors
    ///
    /// [`DbError::StockAdjustmentNotFound`] for no such request, and
    /// [`DbError::StockAdjustmentAltered`] when the row no longer matches the
    /// digest it was stored with.
    pub fn binding(
        &self,
        tx: &Transaction<'_>,
        id: StockEventId,
    ) -> Result<ApprovalBinding, DbError> {
        let (row, _) = load_checked(tx, id)?;
        Ok(binding_of(id, row.hash()))
    }

    /// `stock_adjust`'s first half: reload the request, refuse it if its row
    /// no longer matches its digest, hold the approval `approved` names to
    /// that digest and to this event, and prepare the event the effect posts,
    /// for register `register` in `store`.
    ///
    /// # Errors
    ///
    /// Those of [`Self::binding`]; [`DbError::ApprovalRefused`] when the proof
    /// names no approval, or the approval is for another event, another actor,
    /// another digest, or is outside its validity; [`DbError::ApprovalMissing`]
    /// for an approval the database does not hold;
    /// [`DbError::ApprovalAlreadyConsumed`] for one already spent; and the
    /// errors of [`StockRepository::prepare`].
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_effect(
        &self,
        tx: &Transaction<'_>,
        id: StockEventId,
        register: RegisterId,
        store: StoreId,
        approved: &Authorized<cap::StockAdjust>,
        audit_id: Uuid,
        business_date: BusinessDate,
    ) -> Result<ApprovedAdjustment, DbError> {
        let (row, request) = load_checked(tx, id)?;
        let binding = binding_of(id, row.hash());
        let approval = approved.approval().ok_or(DbError::ApprovalRefused(
            PermissionError::EscalationRequired(cap::StockAdjust::NAME),
        ))?;
        let approvals = ApprovalRepository::new(self.conn);
        let handle = approvals
            .load_for_consumption(tx, approval)?
            .ok_or(DbError::ApprovalMissing(approval))?;
        approvals.ensure_unconsumed(tx, approval)?;
        let at = approved.at();
        handle
            .matches::<cap::StockAdjust>(approved.actor(), &binding, at)
            .map_err(DbError::ApprovalRefused)?;

        let before = stock::cached_position(tx, request.product_id, store)?;
        let movement = request.movement(store, &before, approved.actor(), at, business_date);
        let mut prepared = StockRepository::new(self.conn).prepare(tx, register, &[movement])?;
        let event = prepared
            .pop()
            .ok_or_else(|| stored_invalid("one movement prepared no event"))?;
        Ok(ApprovedAdjustment {
            request,
            event,
            handle,
            actor: approved.actor(),
            audit_id,
            at,
        })
    }

    /// `stock_adjust`'s second half, after the caller has written the envelope
    /// [`ApprovedAdjustment::facts`] names: the audit row, the stock event, the
    /// approval's consumption, and the request removed. Returns the position
    /// the event leaves, which is what `stock_adjust` answers.
    ///
    /// The order is the schema's. The consumption must name an audit row and
    /// an effect that already exist, and the request may only be removed once
    /// its event and the consumption that approved it do.
    ///
    /// # Errors
    ///
    /// Those of [`AuditRepository::append`], [`StockRepository::append`] and
    /// [`ApprovalRepository::consume`], and [`DbError::Sqlite`] for what the
    /// schema refuses. On any of them the caller's rollback leaves the request
    /// and an unspent approval behind, and no event.
    pub fn commit(
        &self,
        tx: &Transaction<'_>,
        approved: &ApprovedAdjustment,
    ) -> Result<StockPosition, DbError> {
        let event = approved.event.event();
        let event_id = event.id.as_uuid();
        let request = &approved.request;
        AuditRepository::new(self.conn).append(
            tx,
            approved.event.register(),
            approved.audit_id,
            &AuditIntent {
                actor: approved.actor,
                approver: Some(approved.handle.approver()),
                approval: Some(approved.handle.id()),
                action: cap::StockAdjust::NAME,
                entity: stock::ENTITY,
                entity_id: event_id,
                reason: Some(approved.handle.reason().to_owned()),
                payload: json!({
                    "amount_minor": 0,
                    "product_id": request.product_id.as_uuid().hyphenated().to_string(),
                    "qty_delta_milli": request.qty_delta.milli(),
                    "reason_code": request.reason.as_str(),
                }),
                at: approved.at,
            },
        )?;
        StockRepository::new(self.conn).append(tx, std::slice::from_ref(&approved.event))?;
        ApprovalRepository::new(self.conn).consume(
            tx,
            approved.handle.id(),
            event_id,
            approved.audit_id,
            approved.at,
        )?;
        tx.execute(
            "DELETE FROM stock_adjustment_request WHERE stock_event_id = ?1",
            [event_id.as_bytes().as_slice()],
        )?;
        Ok(event.after)
    }
}

/// A stored request's columns, exactly as SQLite holds them, in `0006`'s
/// declaration order.
struct Row {
    stock_event_id: Vec<u8>,
    product_id: Vec<u8>,
    qty_delta_milli: i64,
    reason_code: String,
    note: Option<String>,
    requested_by: Vec<u8>,
    requested_at: String,
}

impl Row {
    /// The digest of every column but `content_hash`, from the stored values
    /// themselves rather than a re-rendering of them.
    fn hash(&self) -> PreparedIntentHash {
        PreparedIntentHash::of(
            PreparedIntent::StockAdjustment,
            &[
                IntentColumn::NotNull(IntentValue::Blob(&self.stock_event_id)),
                IntentColumn::NotNull(IntentValue::Blob(&self.product_id)),
                IntentColumn::NotNull(IntentValue::Integer(self.qty_delta_milli)),
                IntentColumn::NotNull(IntentValue::Text(&self.reason_code)),
                IntentColumn::Nullable(self.note.as_deref().map(IntentValue::Text)),
                IntentColumn::NotNull(IntentValue::Blob(&self.requested_by)),
                IntentColumn::NotNull(IntentValue::Text(&self.requested_at)),
            ],
        )
    }
}

fn binding_of(id: StockEventId, hash: PreparedIntentHash) -> ApprovalBinding {
    ApprovalBinding {
        entity_id: id.as_uuid(),
        amount_minor: 0,
        content_hash: Some(hash),
    }
}

/// The request `id`, its row recomputed and held to its stored digest, and
/// read as the domain's request.
fn load_checked(
    tx: &Transaction<'_>,
    id: StockEventId,
) -> Result<(Row, StockAdjustmentRequest), DbError> {
    let loaded = tx
        .query_row(
            "SELECT stock_event_id, product_id, qty_delta_milli, reason_code, note,
                    requested_by, requested_at, content_hash
               FROM stock_adjustment_request
              WHERE stock_event_id = ?1",
            [id.as_uuid().as_bytes().as_slice()],
            |r| {
                Ok((
                    Row {
                        stock_event_id: r.get(0)?,
                        product_id: r.get(1)?,
                        qty_delta_milli: r.get(2)?,
                        reason_code: r.get(3)?,
                        note: r.get(4)?,
                        requested_by: r.get(5)?,
                        requested_at: r.get(6)?,
                    },
                    r.get::<_, Vec<u8>>(7)?,
                ))
            },
        )
        .optional()?;
    let (row, stored) = loaded.ok_or(DbError::StockAdjustmentNotFound(id))?;
    if row.hash().to_stored().as_slice() != stored.as_slice() {
        return Err(DbError::StockAdjustmentAltered(id));
    }
    let product: [u8; 16] = row
        .product_id
        .as_slice()
        .try_into()
        .map_err(|_| stored_invalid("product_id is not sixteen bytes"))?;
    let reason = StockAdjustmentReason::from_code(&row.reason_code)
        .map_err(|_| stored_invalid("reason_code names no reason"))?;
    let request = StockAdjustmentRequest {
        stock_event_id: id,
        product_id: ProductId::from_uuid(Uuid::from_bytes(product)),
        qty_delta: Qty::from_milli(row.qty_delta_milli),
        reason,
        note: row.note.clone(),
    };
    Ok((row, request))
}

fn stored_invalid(reason: &str) -> DbError {
    DbError::StoredStockRecordInvalid {
        reason: reason.to_owned(),
    }
}
