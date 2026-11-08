//! The canonical form a fact travels in (`ref/sync-protocol.md` §2 and §3):
//! sorted keys, no whitespace, UTF-8; ids as lower-case hyphenated UUID text,
//! instants as ISO-8601 UTC with milliseconds, binary values as lower-case
//! hex.
//!
//! 1.7.7's print queue defined the payloads a `receipt_artifact` and an
//! `audit_log` row travel as, for the duplicate a person's answer writes.
//! 1.8.3's finalize writes the original and its audit row in the same form,
//! so both share these functions rather than each keeping a copy.

use std::collections::BTreeMap;

use pos_db::repo::receipt::{ArtifactKind, Id, ReceiptArtifact};
use pos_domain::audit::VERSION as AUDIT_CANONICAL_VERSION;
use pos_domain::{AuditIntent, RegisterId};
use serde_json::{Map, Value, json};
use uuid::Uuid;

/// A receipt artifact as it travels (`ref/sync-protocol.md` §3, "The payload a
/// `receipt_artifact` travels as"): its own columns in the canonical form —
/// sorted keys, no whitespace, ids as lower-case hyphenated UUIDs, instants as
/// ISO-8601 UTC with milliseconds, bytes as lower-case hex.
pub(crate) fn artifact_payload(
    artifact: &ReceiptArtifact,
    content_hash: &[u8; 32],
) -> Result<String, serde_json::Error> {
    canonical(&json!({
        "artifact_kind": artifact_kind_text(artifact.kind),
        "content_bytes": hex(&artifact.content_bytes),
        "content_hash": hex(content_hash),
        "fiscal_version": artifact.fiscal_version,
        "format": artifact.format,
        "generated_at": artifact.generated_at.to_iso8601(),
        "id": uuid_text(&artifact.id),
        "printer_profile": artifact.printer_profile,
        "sale_id": uuid_text(&artifact.sale_id),
        "source_artifact_id": artifact.source_artifact_id.as_ref().map(uuid_text),
        "template_version": artifact.template_version,
    }))
}

/// An audit row as it travels (`ref/sync-protocol.md` §3, "The payload an
/// `audit_log` row travels as"): every column known before the row is chained.
/// `seq`, `prev_hash` and `hash` are absent because `0004` refuses the row until
/// this envelope exists, so they cannot be known when it is written.
pub(crate) fn audit_payload(
    id: Uuid,
    register: RegisterId,
    intent: &AuditIntent,
) -> Result<String, serde_json::Error> {
    canonical(&json!({
        "action": intent.action,
        "actor_id": intent.actor.as_uuid().hyphenated().to_string(),
        "approval_handle_id": intent.approval.map(|a| a.as_uuid().hyphenated().to_string()),
        "approver_id": intent.approver.map(|a| a.as_uuid().hyphenated().to_string()),
        "at": intent.at.to_iso8601(),
        "canonical_version": AUDIT_CANONICAL_VERSION,
        "entity": intent.entity,
        "entity_id": intent.entity_id.hyphenated().to_string(),
        "id": id.hyphenated().to_string(),
        "payload": intent.payload,
        "reason": intent.reason,
        "register_id": register.as_uuid().hyphenated().to_string(),
    }))
}

/// Serialize with every object's keys sorted, at every depth, whatever order
/// `serde_json` keeps them in — no whitespace, UTF-8.
pub(crate) fn canonical(value: &Value) -> Result<String, serde_json::Error> {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(object) => {
                let ordered: BTreeMap<&String, Value> = object
                    .iter()
                    .map(|(key, item)| (key, sorted(item)))
                    .collect();
                let mut out = Map::new();
                for (key, item) in ordered {
                    out.insert(key.clone(), item);
                }
                Value::Object(out)
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    Ok(serde_json::to_string(&sorted(value))?)
}

const fn artifact_kind_text(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Original => "original",
        ArtifactKind::Duplicate => "duplicate",
        ArtifactKind::FiscalSupplement => "fiscal_supplement",
    }
}

pub(crate) fn uuid_text(id: &Id) -> String {
    Uuid::from_bytes(*id).hyphenated().to_string()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}
