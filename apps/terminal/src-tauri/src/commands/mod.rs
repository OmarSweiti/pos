//! The register's IPC commands, one module per area (`ref/ipc-contract.md`
//! §3). Each command is a thin Tauri wrapper over a plain function the tests
//! call, and each is named once, in `lib.rs`'s `commands!` list, with its row
//! in `ipc::registry::COMMANDS`.

pub mod auth;
pub mod cart;

use pos_db::DbError;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_domain::AuditIntent;
use rusqlite::Transaction;

use crate::ipc::IpcError;
use crate::state::{PRODUCER_VERSION, Register, SYNC_PROTOCOL_VERSION};

/// A storage failure, as the webview receives it: the code, and the source
/// in the scrubbed log only.
pub(crate) fn stored(error: DbError) -> IpcError {
    IpcError::from_source("storage_failed", "error.storage.failed", &error)
}

/// A SQLite failure outside a repository, as [`stored`] answers it.
pub(crate) fn sqlite(error: rusqlite::Error) -> IpcError {
    stored(error.into())
}

/// Append `intent` to the register's audit chain inside `tx`, behind the
/// ready delivery envelope `audit_log_has_ready_commit` requires (I-9). The
/// audit row is the envelope's only member: what it audits is register-local
/// or travels in an envelope of its own.
pub(crate) fn audit(
    register: &Register,
    tx: &Transaction<'_>,
    intent: &AuditIntent,
) -> Result<(), IpcError> {
    let audit_id = register.next_id();
    let commit_id = register.next_id();
    let change_id = register.next_id();
    let created_at = intent.at.to_iso8601();
    OutboxRepository::new(tx)
        .write_commit(
            tx,
            &CommitEnvelope {
                commit_id: commit_id.as_bytes(),
                protocol_version: SYNC_PROTOCOL_VERSION,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: PRODUCER_VERSION,
                created_at: &created_at,
            },
            &[FactMember {
                change_id: change_id.as_bytes(),
                entity: "audit_log",
                entity_id: audit_id.as_bytes(),
                payload: "{}",
            }],
        )
        .map_err(stored)?;
    AuditRepository::new(tx)
        .append(tx, register.register(), audit_id, intent)
        .map_err(stored)?;
    Ok(())
}
