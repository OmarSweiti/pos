//! `audit_checkpoint`: each time a register's audit head was exported to
//! somewhere outside the database (`ref/security-compliance.md` §4, "the
//! anchored checkpoint").
//!
//! A verified backup is one such export (1.8.6): its manifest carries the head
//! the copy holds, and this row records that the export happened, under the
//! generation's name. Z close is the other Phase-1 source.
//!
//! **One head, anchored once per source.** The table is `UNIQUE (register_id,
//! last_seq, last_hash, source_kind)`, and an hour with no audit row leaves the
//! head where it was. [`CheckpointRepository::anchored`] answers whether this
//! head is already anchored by this source, so a caller writes nothing,
//! envelope included, rather than meet the constraint mid-transaction.
//!
//! The row is a synced fact (`ref/schema.md`'s fact tables), so its writer
//! takes the caller's transaction, and the caller writes its delivery envelope
//! in the same one (I-9).

use pos_domain::{RegisterId, Timestamp};
use rusqlite::{Connection, Transaction, params};
use uuid::Uuid;

use crate::DbError;

/// Where an anchor was exported to: `audit_checkpoint.source_kind`'s three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointSource {
    /// A printed Z report.
    ZReport,
    /// A backup generation that verified (1.8.6).
    VerifiedBackup,
    /// The server's last accepted checkpoint (Phase 3).
    Server,
}

impl CheckpointSource {
    /// The column's spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            CheckpointSource::ZReport => "z_report",
            CheckpointSource::VerifiedBackup => "verified_backup",
            CheckpointSource::Server => "server",
        }
    }
}

/// One `audit_checkpoint` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditCheckpoint {
    pub id: Uuid,
    pub register: RegisterId,
    /// The head's `seq`: `0` for a register that had written no audit row.
    pub last_seq: u64,
    /// The head's hash; the genesis digest when `last_seq` is `0`.
    pub last_hash: [u8; 32],
    pub source: CheckpointSource,
    /// What the export is called where it lives: a backup's generation name.
    pub anchor_ref: String,
    pub anchored_at: Timestamp,
}

/// Reads and writes `audit_checkpoint`.
pub struct CheckpointRepository<'c> {
    conn: &'c Connection,
}

impl<'c> CheckpointRepository<'c> {
    #[must_use]
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Whether `source` has already anchored `register` at this head.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`].
    pub fn anchored(
        &self,
        register: RegisterId,
        last_seq: u64,
        last_hash: &[u8; 32],
        source: CheckpointSource,
    ) -> Result<bool, DbError> {
        let seq =
            i64::try_from(last_seq).map_err(|_| DbError::AuditSeqInvalid { found: i64::MAX })?;
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM audit_checkpoint
                             WHERE register_id = ?1 AND last_seq = ?2
                               AND last_hash = ?3 AND source_kind = ?4)",
            params![
                register.as_uuid().as_bytes().as_slice(),
                seq,
                last_hash.as_slice(),
                source.as_str()
            ],
            |row| row.get(0),
        )?)
    }

    /// Append `checkpoint`, in the caller's transaction, after its delivery
    /// envelope (I-9).
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`], among them the table's constraints: a register
    /// this database does not hold, and a head this source already anchored.
    pub fn record(
        &self,
        tx: &Transaction<'_>,
        checkpoint: &AuditCheckpoint,
    ) -> Result<(), DbError> {
        let seq = i64::try_from(checkpoint.last_seq)
            .map_err(|_| DbError::AuditSeqInvalid { found: i64::MAX })?;
        tx.execute(
            "INSERT INTO audit_checkpoint
               (id, register_id, last_seq, last_hash, source_kind, anchor_ref, anchored_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                checkpoint.id.as_bytes().as_slice(),
                checkpoint.register.as_uuid().as_bytes().as_slice(),
                seq,
                checkpoint.last_hash.as_slice(),
                checkpoint.source.as_str(),
                checkpoint.anchor_ref,
                checkpoint.anchored_at.to_iso8601(),
            ],
        )?;
        Ok(())
    }
}
