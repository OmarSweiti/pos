//! The register's IPC commands, one module per area (`ref/ipc-contract.md`
//! §3). Each command is a thin Tauri wrapper over a plain function the tests
//! call, and each is named once, in `lib.rs`'s `commands!` list, with its row
//! in `ipc::registry::COMMANDS`.

pub mod auth;
pub mod cart;
pub mod sale;
pub mod shift;
pub mod stock;
pub mod tender;
pub mod user;

use pos_db::DbError;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::audit::AuditRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_domain::AuditIntent;
use rusqlite::Transaction;
use uuid::Uuid;

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
/// or travels in an envelope of its own. A fact that must travel with its row
/// is written through [`audit_fact`] instead. It travels as the payload
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
    chain(register, tx, audit_id, commit_id, None, intent)
}

/// A fact a command writes beside its audit row, as its delivery envelope
/// carries it: its entity, its id, and the payload it travels as.
pub(crate) struct Fact<'p> {
    pub entity: &'static str,
    pub id: Uuid,
    pub payload: &'p str,
}

/// [`audit`], for an effect that is a fact of its own, such as a shift's
/// opening or its close (1.9.5): `fact` and the audit row travel in one
/// envelope, `commit_id`, the fact first (I-9). The caller writes the fact's
/// row after this, inside `tx`, and its repository refuses it without this
/// envelope or with any payload but `fact.payload`. `commit_id` is the
/// caller's, because a fact can name the commit it travels in.
///
/// # Errors
///
/// As [`audit`].
pub(crate) fn audit_fact(
    register: &Register,
    tx: &Transaction<'_>,
    commit_id: Uuid,
    fact: &Fact<'_>,
    intent: &AuditIntent,
) -> Result<(), IpcError> {
    let audit_id = register.next_id();
    chain(register, tx, audit_id, commit_id, Some(fact), intent)
}

/// Write the envelope `commit_id`, `fact` first when there is one, then the
/// audit row `audit_id` and any approval it spends; then chain the row and
/// spend the approval.
fn chain(
    register: &Register,
    tx: &Transaction<'_>,
    audit_id: Uuid,
    commit_id: Uuid,
    fact: Option<&Fact<'_>>,
    intent: &AuditIntent,
) -> Result<(), IpcError> {
    let fact = fact.map(|fact| (fact, register.next_id(), *fact.id.as_bytes()));
    let change_id = register.next_id();
    // The approval's id, and the change that spends it on the wire.
    let consumption = intent
        .approval
        .map(|id| (*id.as_uuid().as_bytes(), register.next_id()));
    let created_at = intent.at.to_iso8601();
    let audited = audit_payload(audit_id, register.register(), intent)
        .map_err(|error| IpcError::from_source("storage_failed", "error.storage.failed", &error))?;
    let mut members: Vec<FactMember<'_>> = fact
        .iter()
        .map(|(fact, change, id)| FactMember {
            change_id: change.as_bytes(),
            entity: fact.entity,
            entity_id: id,
            payload: fact.payload,
        })
        .collect();
    members.push(FactMember {
        change_id: change_id.as_bytes(),
        entity: "audit_log",
        entity_id: audit_id.as_bytes(),
        payload: &audited,
    });
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
