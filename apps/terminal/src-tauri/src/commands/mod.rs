//! The register's IPC commands, one module per area (`ref/ipc-contract.md`
//! §3). Each command is a thin Tauri wrapper over a plain function the tests
//! call, and each is named once, in `lib.rs`'s `commands!` list, with its row
//! in `ipc::registry::COMMANDS`.

pub mod auth;
pub mod cart;
pub mod sale;
pub mod tender;

use pos_db::DbError;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_domain::AuditIntent;
use rusqlite::Transaction;

use crate::ipc::IpcError;
use crate::payload::audit_payload;
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
/// audit row is the envelope's first member: what it audits is register-local
/// or travels in an envelope of its own. It travels as the payload
/// `ref/sync-protocol.md` §3 defines for an audit row, every column known
/// before the row is chained.
///
/// **An intent that names an approval spends it here** (`ref/ipc-contract.md`
/// §1, rule 7). Its `approval_consumption` row joins the same envelope and
/// names this audit row, and the intent's entity as the effect: the schema
/// holds a consumption to an effect that is the approval's entity and the
/// audit row's alike, so no other effect could pass. The caller's transaction
/// holds the effect, so the three commit together or not at all.
///
/// # Errors
///
/// `approval_used` for an approval spent since the command was authorized;
/// `approval_mismatch` for one the schema will not let this audit row spend,
/// which is an approval given for another reason, since the command context
/// already held it to its actor, entity, amount and moment; and
/// `storage_failed`.
pub(crate) fn audit(
    register: &Register,
    tx: &Transaction<'_>,
    intent: &AuditIntent,
) -> Result<(), IpcError> {
    let audit_id = register.next_id();
    let commit_id = register.next_id();
    let change_id = register.next_id();
    // The approval's id, and the change that spends it on the wire.
    let consumption = intent
        .approval
        .map(|id| (*id.as_uuid().as_bytes(), register.next_id()));
    let created_at = intent.at.to_iso8601();
    let audited = audit_payload(audit_id, register.register(), intent)
        .map_err(|error| IpcError::from_source("storage_failed", "error.storage.failed", &error))?;
    let mut members = vec![FactMember {
        change_id: change_id.as_bytes(),
        entity: "audit_log",
        entity_id: audit_id.as_bytes(),
        payload: &audited,
    }];
    if let Some((approval, change)) = consumption.as_ref() {
        members.push(FactMember {
            change_id: change.as_bytes(),
            entity: "approval_consumption",
            entity_id: approval,
            payload: "{}",
        });
    }
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
            &members,
        )
        .map_err(stored)?;
    AuditRepository::new(tx)
        .append(tx, register.register(), audit_id, intent)
        .map_err(stored)?;
    if let Some(approval) = intent.approval {
        ApprovalRepository::new(tx)
            .consume(tx, approval, intent.entity_id, audit_id, intent.at)
            .map_err(|error| match error {
                DbError::ApprovalAlreadyConsumed => {
                    IpcError::from_source("approval_used", "error.approval.used", &error)
                }
                DbError::ApprovalConsumptionUnbound => {
                    IpcError::from_source("approval_mismatch", "error.approval.mismatch", &error)
                        .with_detail("the approval was given for another reason")
                }
                other => stored(other),
            })?;
    }
    Ok(())
}
