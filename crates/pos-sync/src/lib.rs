//! Sync protocol types shared by register (push/pull client) and server.
//! Blueprint §4: transactional outbox → idempotent batched push;
//! cursor-based pull of server-versioned reference data.

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct PushBatch {
    pub device_id: String,
    pub batch_id: String,
    pub changes: Vec<Change>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Change {
    pub entity: String,
    pub entity_id: uuid::Uuid,
    pub op: String, // "insert" for facts; "upsert"/"tombstone" for reference data
    pub payload: serde_json::Value,
}

/// `Debug` is hand-written on both types so a canonical payload is never
/// printed.
///
/// `Change.payload` is the same canonical fact JSON `pos-db` carries, at the
/// far end of the wire. A sale payload holds `buyer_name`, `buyer_id_value` and
/// a customer `phone` — three entries on the registry in
/// `ref/security-compliance.md` §6, which redacts them **at any nesting
/// depth**, and a JSON value holding them is exactly that nesting.
/// `.claude/rules/security.md` names "test fixtures that print" among the
/// surfaces the rule reaches, which is what a derived `Debug` is.
///
/// `pos-db`'s `FactMember` and `ManifestEntry` were redacted first; this crate
/// was missed, and an audit found it. Serialization is deliberately untouched:
/// these types exist to be serialized onto the wire, and redacting `Serialize`
/// would redact the payload out of the sync protocol itself.
impl core::fmt::Debug for Change {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Change")
            .field("entity", &self.entity)
            .field("entity_id", &self.entity_id)
            .field("op", &self.op)
            .field("payload", &"<redacted>")
            .finish()
    }
}

impl core::fmt::Debug for PushBatch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PushBatch")
            .field("device_id", &self.device_id)
            .field("batch_id", &self.batch_id)
            .field("changes", &self.changes)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::{Change, PushBatch};

    /// A guard nobody has seen work is a guard nobody should trust, and the
    /// first version of this one's sibling in `pos-db` was vacuous — it searched
    /// for the payload's raw text, which `Debug` escapes, so it could never
    /// match. This builds the rendering a derived `Debug` would have emitted,
    /// from the value itself.
    #[test]
    fn a_canonical_payload_never_reaches_a_debug_string() {
        let phone = "0791234567";
        let change = Change {
            entity: "sale".to_owned(),
            entity_id: uuid::Uuid::nil(),
            op: "insert".to_owned(),
            payload: serde_json::json!({
                "buyer_name": "سامية عبد الله",
                "phone": phone,
                "total_minor": 2900,
            }),
        };
        let batch = PushBatch {
            device_id: "device-1".to_owned(),
            batch_id: "batch-1".to_owned(),
            changes: vec![change],
        };

        let only = batch.changes.first().expect("the fixture holds one change");
        let derived = format!("{:?}", only.payload);

        // Nesting matters: the batch prints its changes, so a redaction that
        // only covered `Change` read directly would still leak through here.
        for printed in [format!("{only:?}"), format!("{batch:?}")] {
            assert!(
                !printed.contains(phone),
                "a payload value leaked: {printed}"
            );
            assert!(
                !printed.contains("سامية"),
                "a payload value leaked: {printed}"
            );
            assert!(
                !printed.contains(&derived),
                "the derived rendering leaked: {printed}"
            );
            assert!(printed.contains("<redacted>"), "got {printed}");
        }
        assert!(
            format!("{batch:?}").contains("batch-1"),
            "redaction must not swallow the rest of the value"
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub entity: String,
    pub after: i64, // server version cursor
    pub limit: u32,
}
