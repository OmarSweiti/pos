//! Persistence for the register's own assessment of its clock.
//!
//! A reboot is exactly when a wrong clock arrives, so [`ClockState`] has to
//! survive one — that is the whole reason it is a stored value rather than a
//! process-lifetime one. `trusted_time_state` holds one row per register,
//! mapping the domain value field for field.
//!
//! **This table is register-local, not a fact.** `ref/schema.md` §"Convergence"
//! names it in the exclusion list — device-owned, never synced — so unlike every
//! other repository in this crate it writes an *upsert* and takes no
//! `sync_commit` envelope. It still takes an explicit `&Transaction`:
//! `repo/mod.rs` makes the caller own the boundary, and the shell persists the
//! clock beside other work.
//!
//! **`boot_token` is stored here and interpreted elsewhere.** It is the opaque
//! shell-owned boot-continuity token. A bare monotonic counter cannot identify
//! its own boot — a new boot eventually reaches a value larger than the old
//! anchor — so the shell compares this token on startup and calls
//! [`ClockState::note_monotonic_reset`] when it changes. Deciding what a change
//! means is the shell's job; this module hands the bytes back unread.
//!
//! Nothing here reads a clock. `updated_at` is an argument, like every other
//! instant in this workspace (I-8).

use pos_domain::{ClockAnomaly, ClockState, Timestamp};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::DbError;

/// `anomaly_kind`'s three legal values, spelled as the schema's `CHECK` spells
/// them. They are the storage encoding of [`ClockAnomaly`]'s variants, and the
/// mapping is total in both directions — a row this module cannot read back is
/// an error, never a silently dropped anomaly.
const JUMPED_BACK: &str = "jumped_back";
const JUMPED_FORWARD: &str = "jumped_forward";
const MONOTONIC_RESET: &str = "monotonic_reset";

/// One register's persisted clock row.
///
/// The `boot_token` travels with the state rather than inside it: `ClockState`
/// is a pure domain value and the token is a shell concern the domain must not
/// acquire a field for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredClock {
    pub state: ClockState,
    pub boot_token: Option<Vec<u8>>,
    pub updated_at: String,
}

/// Reads and writes of the register's durable clock assessment.
pub struct ClockRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ClockRepository<'c> {
    #[must_use]
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Write this register's clock state, replacing any previous row.
    ///
    /// An upsert rather than an append: this is operational state rebuilt from
    /// the world on every reading, not a fact anyone can later be asked to
    /// prove. `updated_at` is the caller's instant (I-8).
    pub fn save(
        &self,
        tx: &Transaction<'_>,
        register_id: &[u8; 16],
        stored: &StoredClock,
    ) -> Result<(), DbError> {
        let (kind, by_ms, at) = encode_anomaly(stored.state.anomaly);
        tx.execute(
            "INSERT INTO trusted_time_state
               (register_id, last_trusted_at, device_at_trust, monotonic_since_trust_ms,
                high_water, anomaly_kind, anomaly_by_ms, anomaly_at, boot_token, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(register_id) DO UPDATE SET
               last_trusted_at          = excluded.last_trusted_at,
               device_at_trust          = excluded.device_at_trust,
               monotonic_since_trust_ms = excluded.monotonic_since_trust_ms,
               high_water               = excluded.high_water,
               anomaly_kind             = excluded.anomaly_kind,
               anomaly_by_ms            = excluded.anomaly_by_ms,
               anomaly_at               = excluded.anomaly_at,
               boot_token               = excluded.boot_token,
               updated_at               = excluded.updated_at",
            params![
                register_id.as_slice(),
                stored.state.last_trusted_at.map(Timestamp::to_iso8601),
                stored.state.device_at_trust.map(Timestamp::to_iso8601),
                stored.state.monotonic_since_trust_ms,
                stored.state.high_water.to_iso8601(),
                kind,
                by_ms,
                at,
                stored.boot_token.as_deref(),
                stored.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Read this register's clock state back, or `None` before it has one.
    ///
    /// `None` is the ordinary state of a register that has never been read
    /// from, and the caller must treat it as "no anchor" rather than as an
    /// error: a register provisioned offline this morning legitimately has no
    /// row, which is what `ClockConfidence::Untrusted` describes.
    pub fn load(&self, register_id: &[u8; 16]) -> Result<Option<StoredClock>, DbError> {
        let row = self
            .conn
            .query_row(
                "SELECT last_trusted_at, device_at_trust, monotonic_since_trust_ms,
                        high_water, anomaly_kind, anomaly_by_ms, anomaly_at,
                        boot_token, updated_at
                   FROM trusted_time_state WHERE register_id = ?1",
                [register_id.as_slice()],
                |row| {
                    Ok(RawClockRow {
                        last_trusted_at: row.get(0)?,
                        device_at_trust: row.get(1)?,
                        monotonic_since_trust_ms: row.get(2)?,
                        high_water: row.get(3)?,
                        anomaly_kind: row.get(4)?,
                        anomaly_by_ms: row.get(5)?,
                        anomaly_at: row.get(6)?,
                        boot_token: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                },
            )
            .optional()?;

        row.map(RawClockRow::into_stored).transpose()
    }
}

/// The row exactly as SQLite returns it, before any of it is trusted.
struct RawClockRow {
    last_trusted_at: Option<String>,
    device_at_trust: Option<String>,
    monotonic_since_trust_ms: Option<i64>,
    high_water: String,
    anomaly_kind: Option<String>,
    anomaly_by_ms: Option<i64>,
    anomaly_at: Option<String>,
    boot_token: Option<Vec<u8>>,
    updated_at: String,
}

impl RawClockRow {
    fn into_stored(self) -> Result<StoredClock, DbError> {
        let parse = |value: Option<String>| -> Result<Option<Timestamp>, DbError> {
            value
                .map(|text| Timestamp::parse_iso8601(&text).map_err(invalid_stored))
                .transpose()
        };

        Ok(StoredClock {
            state: ClockState {
                last_trusted_at: parse(self.last_trusted_at)?,
                device_at_trust: parse(self.device_at_trust)?,
                monotonic_since_trust_ms: self.monotonic_since_trust_ms,
                high_water: Timestamp::parse_iso8601(&self.high_water).map_err(invalid_stored)?,
                anomaly: decode_anomaly(
                    self.anomaly_kind.as_deref(),
                    self.anomaly_by_ms,
                    parse(self.anomaly_at)?,
                )?,
            },
            boot_token: self.boot_token,
            updated_at: self.updated_at,
        })
    }
}

/// Flatten an anomaly onto the three columns, or three `NULL`s.
///
/// The schema's two coupled `CHECK`s hold this encoding to its shape: an
/// anomaly always carries its instant, and only the two jump variants carry a
/// magnitude. Returning `None` for `by_ms` on a reset is therefore not an
/// omission — writing a zero there would be refused.
fn encode_anomaly(
    anomaly: Option<ClockAnomaly>,
) -> (Option<&'static str>, Option<i64>, Option<String>) {
    match anomaly {
        None => (None, None, None),
        Some(ClockAnomaly::JumpedBack { by_ms, at }) => {
            (Some(JUMPED_BACK), Some(by_ms), Some(at.to_iso8601()))
        }
        Some(ClockAnomaly::JumpedForward { by_ms, at }) => {
            (Some(JUMPED_FORWARD), Some(by_ms), Some(at.to_iso8601()))
        }
        Some(ClockAnomaly::MonotonicReset { at }) => {
            (Some(MONOTONIC_RESET), None, Some(at.to_iso8601()))
        }
    }
}

/// Rebuild an anomaly from the three columns.
///
/// Every disagreement between them is an error rather than a best guess. A row
/// whose `anomaly_kind` this build does not know is the one case that matters:
/// it means a newer build wrote a variant this one cannot represent, and
/// returning `None` would silently downgrade a register from "something is
/// wrong with the clock" to "nothing is wrong with the clock".
fn decode_anomaly(
    kind: Option<&str>,
    by_ms: Option<i64>,
    at: Option<Timestamp>,
) -> Result<Option<ClockAnomaly>, DbError> {
    let (Some(kind), Some(at)) = (kind, at) else {
        return match (kind, at) {
            (None, None) => Ok(None),
            _ => Err(invalid_stored(
                "an anomaly must carry both its kind and the instant it was observed",
            )),
        };
    };
    let magnitude = || {
        by_ms
            .ok_or_else(|| invalid_stored(format!("a `{kind}` anomaly must carry `anomaly_by_ms`")))
    };
    match kind {
        JUMPED_BACK => Ok(Some(ClockAnomaly::JumpedBack {
            by_ms: magnitude()?,
            at,
        })),
        JUMPED_FORWARD => Ok(Some(ClockAnomaly::JumpedForward {
            by_ms: magnitude()?,
            at,
        })),
        MONOTONIC_RESET => Ok(Some(ClockAnomaly::MonotonicReset { at })),
        other => Err(invalid_stored(format!(
            "unknown clock anomaly kind `{other}`: this row was written by a newer build"
        ))),
    }
}

/// A stored row that cannot be read back as a domain value.
///
/// The message names the column and the shape it violated, never the value:
/// nothing here is sensitive today, but an error that quotes a stored value is
/// how one starts leaking (`.claude/rules/security.md`).
fn invalid_stored(error: impl core::fmt::Display) -> DbError {
    DbError::ClockStateInvalid {
        reason: error.to_string(),
    }
}
