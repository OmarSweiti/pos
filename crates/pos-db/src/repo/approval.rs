//! Persistence for immutable, one-use manager approvals.
//!
//! The shell owns the transaction that contains the delivery envelope, the
//! financial effect, its audit row, and the consumption fact (I-9). This
//! repository neither begins nor commits that transaction, and it reconstructs
//! stored handles only through [`ApprovalHandle::restore`] so a row is never
//! treated as domain-valid merely because SQLite returned it.

use pos_domain::{ApprovalHandle, ApprovalId, StoredApprovalHandle, Timestamp, UserId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::DbError;

const CONSUMPTION_UNBOUND_MESSAGE: &str =
    "approval consumption must match one bound financial effect and audit row";

/// Reads and writes of durable approval evidence.
pub struct ApprovalRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ApprovalRepository<'c> {
    #[must_use]
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Persist a handle minted by [`ApprovalHandle::issue`], inside the
    /// caller's transaction.
    ///
    /// The caller must write a ready delivery envelope for the handle before
    /// calling this method; the schema refuses an issuance fact without it.
    pub fn insert(&self, tx: &Transaction<'_>, handle: &ApprovalHandle) -> Result<(), DbError> {
        let stored = handle.to_stored();
        tx.execute(
            "INSERT INTO approval_handle
               (id, capability, actor_id, approver_id, entity_id, amount_minor,
                content_hash, reason, issued_at, expires_at, nonce)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                stored.id.as_uuid().as_bytes().as_slice(),
                stored.capability,
                stored.actor.as_uuid().as_bytes().as_slice(),
                stored.approver.as_uuid().as_bytes().as_slice(),
                stored.entity_id.as_bytes().as_slice(),
                stored.amount_minor,
                stored.content_hash.as_ref().map(<[u8; 32]>::as_slice),
                stored.reason,
                stored.issued_at.to_iso8601(),
                stored.expires_at.to_iso8601(),
                stored.nonce.as_slice(),
            ],
        )?;
        Ok(())
    }

    /// Read one stored row back as a validated domain value.
    pub fn load_for_consumption(
        &self,
        tx: &Transaction<'_>,
        id: ApprovalId,
    ) -> Result<Option<ApprovalHandle>, DbError> {
        let row = tx
            .query_row(
                "SELECT id, capability, actor_id, approver_id, entity_id,
                        amount_minor, content_hash, reason, issued_at, expires_at,
                        nonce
                   FROM approval_handle
                  WHERE id = ?1",
                [id.as_uuid().as_bytes().as_slice()],
                |row| {
                    Ok(RawStoredApproval {
                        id: row.get(0)?,
                        capability: row.get(1)?,
                        actor: row.get(2)?,
                        approver: row.get(3)?,
                        entity_id: row.get(4)?,
                        amount_minor: row.get(5)?,
                        content_hash: row.get(6)?,
                        reason: row.get(7)?,
                        issued_at: row.get(8)?,
                        expires_at: row.get(9)?,
                        nonce: row.get(10)?,
                    })
                },
            )
            .optional()?;

        row.map(RawStoredApproval::restore).transpose()
    }

    /// Refuse a replay before the caller tries to write another delivery
    /// envelope for the globally unique consumption fact.
    pub fn ensure_unconsumed(&self, tx: &Transaction<'_>, id: ApprovalId) -> Result<(), DbError> {
        if is_consumed_on(tx, id)? {
            return Err(DbError::ApprovalAlreadyConsumed);
        }
        Ok(())
    }

    /// Append the fact that spends one approval.
    ///
    /// This method repeats the replay check itself. A caller that omitted the
    /// recommended early [`Self::ensure_unconsumed`] check still receives the
    /// named one-use refusal rather than an opaque SQLite constraint error.
    pub fn consume(
        &self,
        tx: &Transaction<'_>,
        handle_id: ApprovalId,
        effect_id: Uuid,
        audit_log_id: Uuid,
        consumed_at: Timestamp,
    ) -> Result<(), DbError> {
        self.ensure_unconsumed(tx, handle_id)?;

        let result = tx.execute(
            "INSERT INTO approval_consumption
               (handle_id, effect_id, audit_log_id, consumed_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                handle_id.as_uuid().as_bytes().as_slice(),
                effect_id.as_bytes().as_slice(),
                audit_log_id.as_bytes().as_slice(),
                consumed_at.to_iso8601(),
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(_, Some(message)))
                if message == CONSUMPTION_UNBOUND_MESSAGE =>
            {
                Err(DbError::ApprovalConsumptionUnbound)
            }
            Err(error) => Err(DbError::Sqlite(error)),
        }
    }

    /// Whether a handle has a durable consumption fact, for status and
    /// post-restart reads.
    pub fn is_consumed(&self, id: ApprovalId) -> Result<bool, DbError> {
        is_consumed_on(self.conn, id)
    }
}

struct RawStoredApproval {
    id: Vec<u8>,
    capability: String,
    actor: Vec<u8>,
    approver: Vec<u8>,
    entity_id: Vec<u8>,
    amount_minor: i64,
    content_hash: Option<Vec<u8>>,
    reason: String,
    issued_at: String,
    expires_at: String,
    nonce: Vec<u8>,
}

impl RawStoredApproval {
    fn restore(self) -> Result<ApprovalHandle, DbError> {
        let stored = StoredApprovalHandle {
            id: ApprovalId::from_uuid(uuid("approval_handle", "id", self.id)?),
            capability: self.capability,
            actor: UserId::from_uuid(uuid("approval_handle", "actor_id", self.actor)?),
            approver: UserId::from_uuid(uuid("approval_handle", "approver_id", self.approver)?),
            entity_id: uuid("approval_handle", "entity_id", self.entity_id)?,
            amount_minor: self.amount_minor,
            content_hash: self.content_hash.map(digest_bytes).transpose()?,
            reason: self.reason,
            issued_at: Timestamp::parse_iso8601(&self.issued_at).map_err(invalid_stored)?,
            expires_at: Timestamp::parse_iso8601(&self.expires_at).map_err(invalid_stored)?,
            nonce: fixed_one_use_bytes(self.nonce)?,
        };

        ApprovalHandle::restore(stored).map_err(invalid_stored)
    }
}

fn is_consumed_on(conn: &Connection, id: ApprovalId) -> Result<bool, DbError> {
    conn.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM approval_consumption WHERE handle_id = ?1
         )",
        [id.as_uuid().as_bytes().as_slice()],
        |row| row.get(0),
    )
    .map_err(DbError::from)
}

fn uuid(table: &'static str, column: &'static str, bytes: Vec<u8>) -> Result<Uuid, DbError> {
    Uuid::from_slice(&bytes).map_err(|_| DbError::IdWidthInvalid {
        table,
        column,
        found: bytes.len(),
    })
}

fn digest_bytes(bytes: Vec<u8>) -> Result<[u8; 32], DbError> {
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| DbError::InvalidStoredApproval {
            reason: format!(
                "prepared-intent evidence has an invalid byte width ({})",
                bytes.len()
            ),
        })
}

fn fixed_one_use_bytes(bytes: Vec<u8>) -> Result<[u8; 16], DbError> {
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| DbError::InvalidStoredApproval {
            reason: format!(
                "one-use evidence has an invalid byte width ({})",
                bytes.len()
            ),
        })
}

fn invalid_stored(error: impl core::fmt::Display) -> DbError {
    DbError::InvalidStoredApproval {
        reason: error.to_string(),
    }
}
