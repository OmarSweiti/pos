//! The scales behind price-embedded labels (microstep 1.2.4's database half,
//! `ref/schema.md` §0007, `ref/hardware-and-receipts.md` §7).
//!
//! `pos_domain::trade_scale` decides whether a scale's labels may price a
//! line; this repository supplies what it decides on, and records the
//! evidence a person enters when a scale is verified, sent for maintenance or
//! revoked.
//!
//! * **[`TradeScaleRepository::current_verification`]** — a scale's newest
//!   verification event, as the domain's `ScaleVerification`. The newest is
//!   the highest `event_no`, the same order `0007`'s
//!   `trade_scale_current_verification` view uses — an owned sequence, never a
//!   timestamp a device clock wrote (I-7). The view itself is not read: it
//!   carries no `effective_at`, and the domain cannot judge a window without it.
//! * **[`TradeScaleRepository::active_rules`]** — a store's switched-on label
//!   layouts, each with the scale behind it, which is what the scan handler
//!   parses with and then asks the domain about.
//! * **[`TradeScaleRepository::record_verification`]** — append one event.
//!
//! ## A verification event is a fact, and travels with its envelope
//!
//! `trade_scale_verification` is append-only evidence that syncs up
//! (`ref/schema.md`'s fact and register-up lists, `ref/sync-protocol.md` §1),
//! so I-9 applies: the event and its delivery envelope commit in one
//! transaction. `0007` gave the table no `_has_ready_commit` trigger — unlike
//! `stock_ledger` and, since `0006`, `receipt_artifact` — so the storage engine
//! would accept a row that never travels. This repository refuses it instead:
//! the envelope must already be written in the caller's transaction, ready,
//! naming this event, and carrying exactly [`VerificationEvent::payload`]. A
//! migration that adds the trigger would make this the second line rather
//! than the only one.
//!
//! Scale identity (`trade_scale`) and the label layouts
//! (`embedded_barcode_rule`) are reference data the server owns and sends
//! down, so nothing here writes them.

use std::collections::BTreeMap;

use pos_domain::{
    EmbeddedBarcodeRule, EmbeddedValue, ScaleVerification, Timestamp, VerificationState,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde_json::Value;
use uuid::Uuid;

use crate::DbError;

/// Ids are a UUIDv7's sixteen bytes, as everywhere in `pos-db`.
pub type Id = [u8; 16];

/// The entity name a verification event's envelope member carries, as
/// `ref/schema.md`'s fact-table list spells it.
pub const ENTITY: &str = "trade_scale_verification";

/// One verification event, exactly as `trade_scale_verification` stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationEvent {
    pub id: Id,
    pub trade_scale_id: Id,
    /// This scale's next event number: one more than its last, starting at 1.
    /// `0007` refuses a gap; [`TradeScaleRepository::next_event_no`] reads it.
    pub event_no: i64,
    pub state: VerificationState,
    /// Where the evidence is: a certificate number, a document reference.
    pub evidence_ref: String,
    /// BLAKE3 of the evidence document, so a later reader can tell whether the
    /// document they hold is the one that was recorded.
    pub evidence_hash: [u8; 32],
    pub seal_or_mark_ref: Option<String>,
    pub effective_at: Timestamp,
    pub valid_until: Option<Timestamp>,
}

impl VerificationEvent {
    /// The payload this event travels as (`ref/sync-protocol.md` §3, "The
    /// payload a `trade_scale_verification` row travels as"): its own columns
    /// in the canonical form every fact uses — sorted keys, no whitespace,
    /// ids as lower-case hyphenated UUIDs, instants as ISO-8601 UTC with
    /// milliseconds, bytes as lower-case hex.
    ///
    /// The caller writes the envelope with it before the event, and
    /// [`TradeScaleRepository::record_verification`] refuses an envelope that
    /// carries anything else, so the row and what travels cannot differ.
    #[must_use]
    pub fn payload(&self) -> String {
        let fields: BTreeMap<&str, Value> = BTreeMap::from([
            ("effective_at", Value::from(self.effective_at.to_iso8601())),
            ("event_no", Value::from(self.event_no)),
            ("evidence_hash", Value::from(hex(&self.evidence_hash))),
            ("evidence_ref", Value::from(self.evidence_ref.clone())),
            ("id", Value::from(uuid_text(&self.id))),
            (
                "seal_or_mark_ref",
                self.seal_or_mark_ref
                    .clone()
                    .map_or(Value::Null, Value::from),
            ),
            ("state", Value::from(state_token(self.state))),
            (
                "trade_scale_id",
                Value::from(uuid_text(&self.trade_scale_id)),
            ),
            (
                "valid_until",
                self.valid_until
                    .map_or(Value::Null, |at| Value::from(at.to_iso8601())),
            ),
        ]);
        // Inserted in key order from the `BTreeMap`, so the object keeps that
        // order whether `serde_json` sorts its maps or preserves insertion;
        // and rendering a `Value` has no failure to hide.
        Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
        .to_string()
    }
}

/// A switched-on label layout and the scale whose labels it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaleRule {
    pub id: Id,
    pub trade_scale_id: Id,
    pub rule: EmbeddedBarcodeRule,
}

/// Reads scale evidence and label layouts, and records evidence.
pub struct TradeScaleRepository<'c> {
    conn: &'c Connection,
}

impl<'c> TradeScaleRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The scale's newest verification event, or `None` if it has none —
    /// which the domain reads as "no evidence", and refuses.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredScaleRecordInvalid`] for an event this build cannot
    /// read back.
    pub fn current_verification(&self, scale: &Id) -> Result<Option<ScaleVerification>, DbError> {
        let row: Option<(String, String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT state, effective_at, valid_until FROM trade_scale_verification
                  WHERE trade_scale_id = ?1
                  ORDER BY event_no DESC
                  LIMIT 1",
                [scale.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        row.map(|(state, effective_at, valid_until)| {
            Ok(ScaleVerification {
                state: state_from_token(&state)?,
                effective_at: instant("effective_at", &effective_at)?,
                valid_until: valid_until
                    .as_deref()
                    .map(|at| instant("valid_until", at))
                    .transpose()?,
            })
        })
        .transpose()
    }

    /// Every event this scale has, oldest first.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredScaleRecordInvalid`] for an event this build cannot
    /// read back.
    pub fn history(&self, scale: &Id) -> Result<Vec<VerificationEvent>, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT id, trade_scale_id, event_no, state, evidence_ref, evidence_hash,
                    seal_or_mark_ref, effective_at, valid_until
               FROM trade_scale_verification
              WHERE trade_scale_id = ?1
              ORDER BY event_no",
        )?;
        let rows = statement.query_map([scale.as_slice()], StoredEvent::from_row)?;
        rows.map(|row| row?.into_event()).collect()
    }

    /// The number the scale's next event must carry: one more than its last.
    ///
    /// Read through the caller's transaction, so the number, the envelope that
    /// names the event and the event itself are one consistent write.
    ///
    /// # Errors
    ///
    /// [`DbError::Sqlite`] if the read fails.
    pub fn next_event_no(&self, tx: &Transaction<'_>, scale: &Id) -> Result<i64, DbError> {
        Ok(tx.query_row(
            "SELECT COALESCE(MAX(event_no), 0) + 1 FROM trade_scale_verification
              WHERE trade_scale_id = ?1",
            [scale.as_slice()],
            |row| row.get(0),
        )?)
    }

    /// Append one verification event, inside the caller's transaction, after
    /// its envelope.
    ///
    /// `0007`'s triggers do the rest: a number out of sequence is refused, and
    /// any state but `verified` switches the scale's label rules off in the
    /// same statement.
    ///
    /// # Errors
    ///
    /// [`DbError::ScaleEvidenceEnvelopeMissing`] when no ready envelope in this
    /// transaction names the event, [`DbError::ScaleEvidenceEnvelopeMismatch`]
    /// when the one that does carries a different payload, and
    /// [`DbError::Sqlite`] for what the schema refuses.
    pub fn record_verification(
        &self,
        tx: &Transaction<'_>,
        event: &VerificationEvent,
    ) -> Result<(), DbError> {
        let travelling: Option<String> = tx
            .query_row(
                "SELECT m.payload
                   FROM fact_commit_member m
                   JOIN sync_commit_ready ready ON ready.id = m.commit_id
                  WHERE m.entity = ?1 AND m.entity_id = ?2",
                params![ENTITY, event.id.as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        match travelling {
            None => return Err(DbError::ScaleEvidenceEnvelopeMissing),
            Some(payload) if payload != event.payload() => {
                return Err(DbError::ScaleEvidenceEnvelopeMismatch);
            }
            Some(_) => {}
        }
        tx.execute(
            "INSERT INTO trade_scale_verification
               (id, trade_scale_id, event_no, state, evidence_ref, evidence_hash,
                seal_or_mark_ref, effective_at, valid_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.id.as_slice(),
                event.trade_scale_id.as_slice(),
                event.event_no,
                state_token(event.state),
                event.evidence_ref,
                event.evidence_hash.as_slice(),
                event.seal_or_mark_ref,
                event.effective_at.to_iso8601(),
                event.valid_until.map(Timestamp::to_iso8601),
            ],
        )?;
        Ok(())
    }

    /// The store's switched-on label layouts, each with its scale, longest
    /// prefix first and then by id, so a reader sees them in the order the
    /// parser prefers them.
    ///
    /// Switched on is all this asks. Whether a label may price a line *now* is
    /// the domain's `scale_pricing_allowed`, judged with the scale's current
    /// evidence and the register's clock at the moment of the scan.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredScaleRecordInvalid`] for a layout this build cannot
    /// read back — a negative span, an unknown value kind.
    pub fn active_rules(&self, store: &Id) -> Result<Vec<ScaleRule>, DbError> {
        let mut statement = self.conn.prepare(
            "SELECT id, trade_scale_id, prefix, item_code_start, item_code_len,
                    value_start, value_len, value_kind, value_scale, verify_checksum
               FROM embedded_barcode_rule
              WHERE store_id = ?1 AND is_active = 1
              ORDER BY length(prefix) DESC, id",
        )?;
        let rows = statement.query_map([store.as_slice()], StoredRule::from_row)?;
        rows.map(|row| row?.into_rule()).collect()
    }
}

/// One `trade_scale_verification` row as stored.
struct StoredEvent {
    id: Vec<u8>,
    trade_scale_id: Vec<u8>,
    event_no: i64,
    state: String,
    evidence_ref: String,
    evidence_hash: Vec<u8>,
    seal_or_mark_ref: Option<String>,
    effective_at: String,
    valid_until: Option<String>,
}

impl StoredEvent {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            trade_scale_id: row.get(1)?,
            event_no: row.get(2)?,
            state: row.get(3)?,
            evidence_ref: row.get(4)?,
            evidence_hash: row.get(5)?,
            seal_or_mark_ref: row.get(6)?,
            effective_at: row.get(7)?,
            valid_until: row.get(8)?,
        })
    }

    fn into_event(self) -> Result<VerificationEvent, DbError> {
        Ok(VerificationEvent {
            id: id("trade_scale_verification", "id", self.id)?,
            trade_scale_id: id(
                "trade_scale_verification",
                "trade_scale_id",
                self.trade_scale_id,
            )?,
            event_no: self.event_no,
            state: state_from_token(&self.state)?,
            evidence_ref: self.evidence_ref,
            evidence_hash: self
                .evidence_hash
                .try_into()
                .map_err(|_| invalid("an evidence hash is not 32 bytes".to_owned()))?,
            seal_or_mark_ref: self.seal_or_mark_ref,
            effective_at: instant("effective_at", &self.effective_at)?,
            valid_until: self
                .valid_until
                .as_deref()
                .map(|at| instant("valid_until", at))
                .transpose()?,
        })
    }
}

/// One `embedded_barcode_rule` row as stored.
struct StoredRule {
    id: Vec<u8>,
    trade_scale_id: Vec<u8>,
    prefix: String,
    item_code_start: i64,
    item_code_len: i64,
    value_start: i64,
    value_len: i64,
    value_kind: String,
    value_scale: i64,
    verify_checksum: bool,
}

impl StoredRule {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            trade_scale_id: row.get(1)?,
            prefix: row.get(2)?,
            item_code_start: row.get(3)?,
            item_code_len: row.get(4)?,
            value_start: row.get(5)?,
            value_len: row.get(6)?,
            value_kind: row.get(7)?,
            value_scale: row.get(8)?,
            verify_checksum: row.get(9)?,
        })
    }

    fn into_rule(self) -> Result<ScaleRule, DbError> {
        let span = |name: &str, value: i64| {
            usize::try_from(value)
                .map_err(|_| invalid(format!("rule {:?} has {name} {value}", self.prefix)))
        };
        let rule = EmbeddedBarcodeRule {
            prefix: self.prefix.clone(),
            item_code_span: (
                span("item_code_start", self.item_code_start)?,
                span("item_code_len", self.item_code_len)?,
            ),
            value_span: (
                span("value_start", self.value_start)?,
                span("value_len", self.value_len)?,
            ),
            value_kind: match self.value_kind.as_str() {
                "weight_milli" => EmbeddedValue::WeightMilli,
                "price_minor" => EmbeddedValue::PriceMinor,
                other => {
                    return Err(invalid(format!(
                        "rule {:?} has value_kind {other:?}",
                        self.prefix
                    )));
                }
            },
            value_scale: u32::try_from(self.value_scale).map_err(|_| {
                invalid(format!(
                    "rule {:?} has value_scale {}",
                    self.prefix, self.value_scale
                ))
            })?,
            verify_checksum: self.verify_checksum,
        };
        Ok(ScaleRule {
            id: id("embedded_barcode_rule", "id", self.id)?,
            trade_scale_id: id(
                "embedded_barcode_rule",
                "trade_scale_id",
                self.trade_scale_id,
            )?,
            rule,
        })
    }
}

/// `trade_scale_verification.state`'s token for a state — `0007`'s `CHECK`
/// list, which is also the enum's wire form.
const fn state_token(state: VerificationState) -> &'static str {
    match state {
        VerificationState::Verified => "verified",
        VerificationState::Revoked => "revoked",
        VerificationState::MaintenancePending => "maintenance_pending",
    }
}

fn state_from_token(token: &str) -> Result<VerificationState, DbError> {
    [
        VerificationState::Verified,
        VerificationState::Revoked,
        VerificationState::MaintenancePending,
    ]
    .into_iter()
    .find(|state| state_token(*state) == token)
    .ok_or_else(|| invalid(format!("verification state {token:?}")))
}

fn instant(column: &str, text: &str) -> Result<Timestamp, DbError> {
    Timestamp::parse_iso8601(text)
        .map_err(|_| invalid(format!("{column} {text:?} is not an ISO-8601 instant")))
}

fn id(table: &'static str, column: &'static str, bytes: Vec<u8>) -> Result<Id, DbError> {
    let found = bytes.len();
    bytes.try_into().map_err(|_| DbError::IdWidthInvalid {
        table,
        column,
        found,
    })
}

fn uuid_text(id: &Id) -> String {
    Uuid::from_bytes(*id).hyphenated().to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn invalid(reason: String) -> DbError {
    DbError::StoredScaleRecordInvalid { reason }
}
