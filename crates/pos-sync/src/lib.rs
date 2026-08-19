//! Sync protocol types shared by register (push/pull client) and server.
//! Blueprint §4: transactional outbox → idempotent batched push;
//! cursor-based pull of server-versioned reference data.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushBatch {
    pub device_id: String,
    pub batch_id: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub entity: String,
    pub entity_id: uuid::Uuid,
    pub op: String, // "insert" for facts; "upsert"/"tombstone" for reference data
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub entity: String,
    pub after: i64, // server version cursor
    pub limit: u32,
}
