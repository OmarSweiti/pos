//! A shift's facts and its projection (microstep 1.9.5, `ref/schema.md`
//! §0005, `ref/ipc-contract.md`'s shift commands).
//!
//! A shift is two immutable facts: the opening, `shift`, and the close,
//! `shift_close_event`. Whether it is open is `shift_state`, a projection
//! `0005`'s triggers keep from the two, which allows one open shift per
//! register. Nothing here updates or deletes either fact.
//!
//! Every write takes the caller's transaction and comes after the envelope
//! that names it, with exactly the payload [`opening_payload`] or
//! [`close_payload`] gives (I-9). The caller writes the audit row the domain
//! returns with it, in the same envelope.
//!
//! The domain decides who may close a shift; this repository refuses it again,
//! as `ref/ipc-contract.md` asks of it: a close by anyone but the opener is
//! refused here too, so the ordinary path cannot close another user's shift
//! even if a caller skipped the domain.

use std::collections::BTreeMap;

use pos_domain::{
    BusinessDate, Currency, Money, RegisterId, Shift, ShiftClose, ShiftId, StoreId, Timestamp,
    UserId,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value;
use uuid::Uuid;

use crate::DbError;

/// The entity an opening's envelope member carries.
pub const ENTITY: &str = "shift";
/// The entity a close's envelope member carries.
pub const CLOSE_ENTITY: &str = "shift_close_event";

/// The payload an opening travels as (`ref/sync-protocol.md` §3): the row's
/// own columns in the canonical form, sorted keys and no whitespace.
#[must_use]
pub fn opening_payload(shift: &Shift) -> String {
    canonical(BTreeMap::from([
        ("business_date", Value::from(shift.business_date.to_iso())),
        ("float_minor", Value::from(shift.float.minor())),
        ("id", hyphenated(shift.id.as_uuid())),
        ("opened_at", Value::from(shift.opened_at.to_iso8601())),
        ("opened_by", hyphenated(shift.opened_by.as_uuid())),
        ("register_id", hyphenated(shift.register_id.as_uuid())),
        ("store_id", hyphenated(shift.store_id.as_uuid())),
    ]))
}

/// The payload a close travels as: its own columns, the commit it travels in
/// among them, as the sale's do.
#[must_use]
pub fn close_payload(close: &ShiftClose, commit: Uuid) -> String {
    canonical(BTreeMap::from([
        ("closed_at", Value::from(close.closed_at.to_iso8601())),
        ("closed_by", hyphenated(close.closed_by.as_uuid())),
        ("id", hyphenated(close.id)),
        ("shift_id", hyphenated(close.shift_id.as_uuid())),
        ("sync_commit_id", hyphenated(commit)),
    ]))
}

fn hyphenated(id: Uuid) -> Value {
    Value::from(id.hyphenated().to_string())
}

/// Rendered in key order from the `BTreeMap`, whether `serde_json` sorts its
/// maps or preserves insertion.
fn canonical(fields: BTreeMap<&str, Value>) -> String {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
    .to_string()
}

/// Writes a shift's facts and reads them back.
pub struct ShiftRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ShiftRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `shift_open`'s write: the opening fact, inside the caller's
    /// transaction, after the envelope that names it.
    ///
    /// # Errors
    ///
    /// [`DbError::ShiftAlreadyOpen`] when the register already has an open
    /// shift; [`DbError::ShiftFloatCurrencyMismatch`] for a float in another
    /// currency than the store's; [`DbError::ShiftEnvelopeMissing`] and
    /// [`DbError::ShiftEnvelopeMismatch`] when no ready envelope names it with
    /// exactly [`opening_payload`]; and [`DbError::Sqlite`] for what `0005`
    /// refuses, a register outside the shift's store among it.
    pub fn open(&self, tx: &Transaction<'_>, shift: &Shift) -> Result<(), DbError> {
        if let Some(open) = self.current_on(tx, shift.register_id)? {
            return Err(DbError::ShiftAlreadyOpen {
                register: shift.register_id,
                open: open.id,
            });
        }
        let currency = store_currency(tx, shift.store_id)?;
        if shift.float.currency() != currency {
            return Err(DbError::ShiftFloatCurrencyMismatch);
        }
        travelling(tx, ENTITY, shift.id.as_uuid(), &opening_payload(shift))?;
        tx.execute(
            "INSERT INTO shift
               (id, register_id, store_id, business_date, opened_by, opened_at, float_minor)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                shift.id.as_uuid().as_bytes().as_slice(),
                shift.register_id.as_uuid().as_bytes().as_slice(),
                shift.store_id.as_uuid().as_bytes().as_slice(),
                shift.business_date.to_iso(),
                shift.opened_by.as_uuid().as_bytes().as_slice(),
                shift.opened_at.to_iso8601(),
                shift.float.minor(),
            ],
        )?;
        Ok(())
    }

    /// `shift_close`'s write: the close fact, travelling in commit `commit`,
    /// inside the caller's transaction, after the envelope that names it. The
    /// projection closes the shift in the same statement.
    ///
    /// # Errors
    ///
    /// [`DbError::ShiftNotFound`] and [`DbError::ShiftNotOpen`] for a shift
    /// that does not exist or is already closed;
    /// [`DbError::ShiftClosedByAnother`] when the closer is not the opener;
    /// [`DbError::ShiftEnvelopeMissing`] and [`DbError::ShiftEnvelopeMismatch`]
    /// as for [`Self::open`]; and [`DbError::Sqlite`] for what `0005` refuses.
    pub fn close(
        &self,
        tx: &Transaction<'_>,
        close: &ShiftClose,
        commit: Uuid,
    ) -> Result<(), DbError> {
        let shift = self
            .by_id_on(tx, close.shift_id)?
            .ok_or(DbError::ShiftNotFound(close.shift_id))?;
        let state: String = tx.query_row(
            "SELECT state FROM shift_state WHERE shift_id = ?1",
            [close.shift_id.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )?;
        if state != "open" {
            return Err(DbError::ShiftNotOpen(close.shift_id));
        }
        if close.closed_by != shift.opened_by {
            return Err(DbError::ShiftClosedByAnother {
                opened_by: shift.opened_by,
                closed_by: close.closed_by,
            });
        }
        travelling(tx, CLOSE_ENTITY, close.id, &close_payload(close, commit))?;
        tx.execute(
            "INSERT INTO shift_close_event (id, shift_id, sync_commit_id, closed_by, closed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                close.id.as_bytes().as_slice(),
                close.shift_id.as_uuid().as_bytes().as_slice(),
                commit.as_bytes().as_slice(),
                close.closed_by.as_uuid().as_bytes().as_slice(),
                close.closed_at.to_iso8601(),
            ],
        )?;
        Ok(())
    }

    /// `shift_current`: the open shift on `register`, if there is one.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredShiftRecordInvalid`] for a stored value this build
    /// cannot read, and [`DbError::Sqlite`] if the read fails.
    pub fn current(&self, register: RegisterId) -> Result<Option<Shift>, DbError> {
        self.current_on(self.conn, register)
    }

    /// The shift `id`, open or closed, as its opening fact records it.
    ///
    /// # Errors
    ///
    /// As [`Self::current`].
    pub fn by_id(&self, id: ShiftId) -> Result<Option<Shift>, DbError> {
        self.by_id_on(self.conn, id)
    }

    /// Whether the shift `id` is open.
    ///
    /// # Errors
    ///
    /// [`DbError::ShiftNotFound`] for no such shift, and [`DbError::Sqlite`].
    pub fn is_open(&self, id: ShiftId) -> Result<bool, DbError> {
        let state: Option<String> = self
            .conn
            .query_row(
                "SELECT state FROM shift_state WHERE shift_id = ?1",
                [id.as_uuid().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        state
            .map(|state| state == "open")
            .ok_or(DbError::ShiftNotFound(id))
    }

    fn current_on(
        &self,
        conn: &Connection,
        register: RegisterId,
    ) -> Result<Option<Shift>, DbError> {
        read_shift(
            conn,
            "SELECT s.id, s.register_id, s.store_id, s.business_date, s.opened_by, s.opened_at,
                    s.float_minor, st.currency
               FROM shift s
               JOIN shift_state ss ON ss.shift_id = s.id AND ss.state = 'open'
               JOIN store st ON st.id = s.store_id
              WHERE s.register_id = ?1",
            register.as_uuid().as_bytes().as_slice(),
        )
    }

    fn by_id_on(&self, conn: &Connection, id: ShiftId) -> Result<Option<Shift>, DbError> {
        read_shift(
            conn,
            "SELECT s.id, s.register_id, s.store_id, s.business_date, s.opened_by, s.opened_at,
                    s.float_minor, st.currency
               FROM shift s
               JOIN store st ON st.id = s.store_id
              WHERE s.id = ?1",
            id.as_uuid().as_bytes().as_slice(),
        )
    }
}

/// Refuse a write no ready envelope names with exactly `payload`.
fn travelling(
    tx: &Transaction<'_>,
    entity: &'static str,
    id: Uuid,
    payload: &str,
) -> Result<(), DbError> {
    let carried: Option<String> = tx
        .query_row(
            "SELECT m.payload
               FROM fact_commit_member m
               JOIN sync_commit_ready ready ON ready.id = m.commit_id
              WHERE m.entity = ?1 AND m.entity_id = ?2",
            params![entity, id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    match carried {
        None => Err(DbError::ShiftEnvelopeMissing { entity }),
        Some(carried) if carried != payload => Err(DbError::ShiftEnvelopeMismatch { entity }),
        Some(_) => Ok(()),
    }
}

fn store_currency(tx: &Transaction<'_>, store: StoreId) -> Result<Currency, DbError> {
    let code: Option<String> = tx
        .query_row(
            "SELECT currency FROM store WHERE id = ?1",
            [store.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    let code = code.ok_or_else(|| invalid(&format!("no store {store}")))?;
    Currency::from_code(&code).map_err(|_| invalid(&format!("store {store} has currency {code:?}")))
}

fn read_shift(conn: &Connection, sql: &str, key: &[u8]) -> Result<Option<Shift>, DbError> {
    let row = conn
        .query_row(sql, [key], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Vec<u8>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .optional()?;
    let Some((id, register, store, date, opener, opened_at, float, currency)) = row else {
        return Ok(None);
    };
    let currency = Currency::from_code(&currency)
        .map_err(|_| invalid(&format!("a store currency {currency:?}")))?;
    Ok(Some(Shift {
        id: ShiftId::from_uuid(uuid_of(&id, "id")?),
        register_id: RegisterId::from_uuid(uuid_of(&register, "register_id")?),
        store_id: StoreId::from_uuid(uuid_of(&store, "store_id")?),
        business_date: BusinessDate::parse(&date)
            .map_err(|_| invalid("business_date is not a date"))?,
        opened_by: UserId::from_uuid(uuid_of(&opener, "opened_by")?),
        opened_at: Timestamp::parse_iso8601(&opened_at)
            .map_err(|_| invalid("opened_at is not an instant"))?,
        float: Money::from_minor(float, currency),
    }))
}

fn uuid_of(bytes: &[u8], column: &str) -> Result<Uuid, DbError> {
    let bytes: [u8; 16] = bytes
        .try_into()
        .map_err(|_| invalid(&format!("{column} is not sixteen bytes")))?;
    Ok(Uuid::from_bytes(bytes))
}

fn invalid(reason: &str) -> DbError {
    DbError::StoredShiftRecordInvalid {
        reason: reason.to_owned(),
    }
}
