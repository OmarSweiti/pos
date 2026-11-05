//! The register's checkout journal (microstep 1.8.4, `ref/schema.md` §0005).
//!
//! A sale in `Tendering` or `Finalizing` has nothing else on disk: `sale`
//! admits only completed, voided and parked sales, and the finalize
//! transaction writes a sale whole. Without a journal, a power cut
//! mid-checkout leaves a restart nothing to resume from: no tenders taken, no
//! key to replay a finalize under. `checkout_operation` is that journal. It is
//! register-local and never syncs (master plan C.14), so nothing here writes a
//! delivery envelope, and every write still takes the caller's `&Transaction`
//! (`repo/mod.rs`).
//!
//! **One row per checkout attempt.** Its id is the attempt's idempotency key,
//! and its sale, register, shift and cashier are the snapshot's own cart.
//!
//! - [`CheckoutOperationRepository::begin`] journals a sale as payment begins,
//!   before the first irreversible effect.
//! - [`CheckoutOperationRepository::advance`] records each step after that: a
//!   tender taken, then the finalize begun. Each one names the version it
//!   read, so a writer working from a stale copy is refused rather than
//!   overwriting a newer one.
//! - **Once finalizing, only the finalize removes it.**
//!   [`CheckoutOperationRepository::remove_on_finalize`] runs inside the
//!   transaction that completes the sale, under the same key, so the row and
//!   the sale it became commit together or not at all.
//! - A checkout still being paid for can be given up:
//!   [`CheckoutOperationRepository::abandon`] removes it and returns it, for
//!   the caller to audit.
//!
//! **What this half does not do.** Deciding what a journalled checkout
//! becomes after a restart (`resume_in_flight`) is the recovery half's, with
//! the shell's start-up, after 1.8.3. A card operation is not journalled:
//! the terminal is Phase 2's, and `checkout_operation` has columns for its
//! reference and state but not for its amount or start time (1.8.4's note).

use pos_domain::{
    Finalizing, InFlight, InFlightSale, RegisterId, SaleId, ShiftId, Tendering, Timestamp, UserId,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::DbError;

/// A journalled checkout, as the register reads it back.
///
/// `Debug` is the derived one, which reaches the cart's own: the buyer's name
/// stays redacted.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckoutOperation {
    pub in_flight: InFlightSale,
    /// How many times it has been advanced since it was journalled. The next
    /// [`CheckoutOperationRepository::advance`] names it.
    pub version: i64,
    pub created_at: Timestamp,
}

/// The register's checkout journal.
pub struct CheckoutOperationRepository<'c> {
    conn: &'c Connection,
}

impl<'c> CheckoutOperationRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Journal `tendering` as its payment begins, under `idempotency_key`, at
    /// `at`: before any tender is taken and before anything irreversible.
    ///
    /// # Errors
    ///
    /// - [`DbError::CheckoutAlreadyJournalled`]: its sale already has a
    ///   checkout journalled.
    /// - [`DbError::CheckoutKeyReused`]: another checkout already used
    ///   `idempotency_key`, which is one per attempt and never reused.
    /// - [`DbError::Sqlite`] if the write fails, including a register, shift
    ///   or cashier the database does not hold.
    pub fn begin(
        &self,
        tx: &Transaction<'_>,
        tendering: &Tendering,
        idempotency_key: Uuid,
        at: Timestamp,
    ) -> Result<(), DbError> {
        let cart = tendering.cart();
        if row_of(tx, cart.id)?.is_some() {
            return Err(DbError::CheckoutAlreadyJournalled(cart.id));
        }
        let key_used: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM checkout_operation WHERE idempotency_key = ?1",
                [idempotency_key.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        if key_used.is_some() {
            return Err(DbError::CheckoutKeyReused(cart.id));
        }
        let at = at.to_iso8601();
        tx.execute(
            "INSERT INTO checkout_operation
               (id, sale_id, register_id, shift_id, actor_id, state, priced_snapshot,
                idempotency_key, transition_version, terminal_sale_ref, terminal_state,
                created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'tendering', ?6, ?7, 0, NULL, NULL, ?8, ?8)",
            params![
                bytes(idempotency_key),
                bytes(cart.id.as_uuid()),
                bytes(cart.register_id.as_uuid()),
                bytes(cart.shift_id.as_uuid()),
                bytes(cart.cashier_id.as_uuid()),
                snapshot(tendering)?,
                idempotency_key.to_string(),
                at,
            ],
        )?;
        Ok(())
    }

    /// Record `in_flight` as the checkout's next step: a tender taken while it
    /// is `Tendering`, or the finalize begun when it is `Finalizing`. The
    /// write names `expected_version`, the version this caller read, and
    /// returns the new one.
    ///
    /// # Errors
    ///
    /// - [`DbError::CheckoutNotJournalled`]: its sale has no checkout here.
    /// - [`DbError::CheckoutKeyMismatch`]: the journal holds another attempt.
    /// - [`DbError::CheckoutFinalizing`]: the finalize has begun, after which
    ///   nothing but the finalize changes the row.
    /// - [`DbError::CheckoutStale`]: another write advanced it since this
    ///   caller read it.
    /// - [`DbError::CheckoutCardOpNotJournalled`]: it carries a card
    ///   operation, which this build cannot store.
    /// - [`DbError::StoredCheckoutInvalid`]: the journalled row cannot be read.
    pub fn advance(
        &self,
        tx: &Transaction<'_>,
        in_flight: &InFlightSale,
        expected_version: i64,
    ) -> Result<i64, DbError> {
        let sale = in_flight.sale_id();
        if in_flight.card_op.is_some() {
            return Err(DbError::CheckoutCardOpNotJournalled(sale));
        }
        let current = journalled(tx, sale, in_flight.idempotency_key)?;
        if current.state == "finalizing" {
            return Err(DbError::CheckoutFinalizing(sale));
        }
        if current.version != expected_version {
            return Err(DbError::CheckoutStale {
                sale,
                expected: expected_version,
                found: current.version,
            });
        }
        let state = match in_flight.state {
            InFlight::Tendering(_) => "tendering",
            InFlight::Finalizing(_) => "finalizing",
        };
        let version = expected_version + 1;
        tx.execute(
            "UPDATE checkout_operation
                SET state = ?2, priced_snapshot = ?3, transition_version = ?4, updated_at = ?5
              WHERE sale_id = ?1",
            params![
                bytes(sale.as_uuid()),
                state,
                snapshot(in_flight.tendering())?,
                version,
                in_flight.updated_at.to_iso8601(),
            ],
        )?;
        Ok(version)
    }

    /// Remove the checkout of `sale` inside the transaction that completes it
    /// (1.8.3), under the key it was journalled with. The row and the sale it
    /// became commit together or not at all, so a completed sale never leaves
    /// a journal that would let a restart apply it again.
    ///
    /// # Errors
    ///
    /// - [`DbError::CheckoutNotJournalled`] and
    ///   [`DbError::CheckoutKeyMismatch`], as [`Self::advance`].
    /// - [`DbError::CheckoutNotFinalizing`]: the finalize was never begun.
    pub fn remove_on_finalize(
        &self,
        tx: &Transaction<'_>,
        sale: SaleId,
        idempotency_key: Uuid,
    ) -> Result<(), DbError> {
        let current = journalled(tx, sale, idempotency_key)?;
        if current.state != "finalizing" {
            return Err(DbError::CheckoutNotFinalizing(sale));
        }
        tx.execute(
            "DELETE FROM checkout_operation WHERE sale_id = ?1",
            [bytes(sale.as_uuid())],
        )?;
        Ok(())
    }

    /// Give up the checkout of `sale` while it is still being paid for, and
    /// return what was journalled, for the caller to audit with the void or
    /// the recovery that ends it.
    ///
    /// # Errors
    ///
    /// - [`DbError::CheckoutNotJournalled`] and
    ///   [`DbError::CheckoutKeyMismatch`], as [`Self::advance`].
    /// - [`DbError::CheckoutFinalizing`]: a finalize that has begun is
    ///   completed or replayed, never abandoned.
    /// - [`DbError::StoredCheckoutInvalid`]: the journalled row cannot be read.
    pub fn abandon(
        &self,
        tx: &Transaction<'_>,
        sale: SaleId,
        idempotency_key: Uuid,
    ) -> Result<CheckoutOperation, DbError> {
        let current = journalled(tx, sale, idempotency_key)?;
        if current.state == "finalizing" {
            return Err(DbError::CheckoutFinalizing(sale));
        }
        let operation = read(current)?;
        tx.execute(
            "DELETE FROM checkout_operation WHERE sale_id = ?1",
            [bytes(sale.as_uuid())],
        )?;
        Ok(operation)
    }

    /// The checkout of `sale`, if one is journalled.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredCheckoutInvalid`] for a row this build cannot read,
    /// and [`DbError::Sqlite`] if the read fails.
    pub fn get(&self, sale: SaleId) -> Result<Option<CheckoutOperation>, DbError> {
        row_of(self.conn, sale)?.map(read).transpose()
    }

    /// Every checkout journalled at `register`, oldest first: what a restart
    /// finds still in flight.
    ///
    /// # Errors
    ///
    /// As [`Self::get`].
    pub fn in_flight(&self, register: RegisterId) -> Result<Vec<CheckoutOperation>, DbError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM checkout_operation
              WHERE register_id = ?1 ORDER BY created_at, id"
        ))?;
        let rows = statement
            .query_map([bytes(register.as_uuid())], Row::from)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(read).collect()
    }
}

const COLUMNS: &str = "sale_id, register_id, shift_id, actor_id, state, priced_snapshot, \
                       idempotency_key, transition_version, terminal_sale_ref, terminal_state, \
                       created_at, updated_at";

/// A `checkout_operation` row as stored, before it is read as a checkout.
struct Row {
    sale_id: Vec<u8>,
    register_id: Vec<u8>,
    shift_id: Vec<u8>,
    actor_id: Vec<u8>,
    state: String,
    snapshot: String,
    idempotency_key: String,
    version: i64,
    terminal_sale_ref: Option<String>,
    terminal_state: Option<String>,
    created_at: String,
    updated_at: String,
}

impl Row {
    fn from(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
        Ok(Row {
            sale_id: row.get(0)?,
            register_id: row.get(1)?,
            shift_id: row.get(2)?,
            actor_id: row.get(3)?,
            state: row.get(4)?,
            snapshot: row.get(5)?,
            idempotency_key: row.get(6)?,
            version: row.get(7)?,
            terminal_sale_ref: row.get(8)?,
            terminal_state: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }
}

fn row_of(conn: &Connection, sale: SaleId) -> Result<Option<Row>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM checkout_operation WHERE sale_id = ?1"),
            [bytes(sale.as_uuid())],
            Row::from,
        )
        .optional()?)
}

/// The row journalled for `sale` under `idempotency_key`.
fn journalled(conn: &Connection, sale: SaleId, idempotency_key: Uuid) -> Result<Row, DbError> {
    let row = row_of(conn, sale)?.ok_or(DbError::CheckoutNotJournalled(sale))?;
    if row.idempotency_key != idempotency_key.to_string() {
        return Err(DbError::CheckoutKeyMismatch(sale));
    }
    Ok(row)
}

/// A stored row, read back as the checkout it journals. Every reason names
/// the sale and what is wrong, never the snapshot, which can carry a buyer's
/// name.
fn read(row: Row) -> Result<CheckoutOperation, DbError> {
    let sale = SaleId::from_uuid(uuid(&row.sale_id, "sale_id")?);
    if row.terminal_sale_ref.is_some() || row.terminal_state.is_some() {
        return Err(invalid(format!(
            "checkout of sale {sale} holds a card operation this build cannot resume"
        )));
    }
    let state = match row.state.as_str() {
        "tendering" => InFlight::Tendering(
            serde_json::from_str::<Tendering>(&row.snapshot).map_err(|e| unreadable(sale, &e))?,
        ),
        "finalizing" => InFlight::Finalizing(
            serde_json::from_str::<Finalizing>(&row.snapshot).map_err(|e| unreadable(sale, &e))?,
        ),
        other => {
            return Err(invalid(format!(
                "checkout of sale {sale} is in a state this build does not know: {other}"
            )));
        }
    };
    let idempotency_key = Uuid::parse_str(&row.idempotency_key).map_err(|_| {
        invalid(format!(
            "checkout of sale {sale}'s idempotency key is not a UUID"
        ))
    })?;
    let in_flight = InFlightSale {
        state,
        card_op: None,
        idempotency_key,
        updated_at: timestamp(sale, "updated_at", &row.updated_at)?,
    };
    let cart = in_flight.tendering().cart();
    let columns = [
        ("sale_id", cart.id.as_uuid(), sale.as_uuid()),
        (
            "register_id",
            cart.register_id.as_uuid(),
            RegisterId::from_uuid(uuid(&row.register_id, "register_id")?).as_uuid(),
        ),
        (
            "shift_id",
            cart.shift_id.as_uuid(),
            ShiftId::from_uuid(uuid(&row.shift_id, "shift_id")?).as_uuid(),
        ),
        (
            "actor_id",
            cart.cashier_id.as_uuid(),
            UserId::from_uuid(uuid(&row.actor_id, "actor_id")?).as_uuid(),
        ),
    ];
    for (column, from_cart, stored) in columns {
        if from_cart != stored {
            return Err(invalid(format!(
                "checkout of sale {sale}'s {column} is not its cart's"
            )));
        }
    }
    Ok(CheckoutOperation {
        in_flight,
        version: row.version,
        created_at: timestamp(sale, "created_at", &row.created_at)?,
    })
}

/// A checkout's stored form: its sale as it is being paid for. A
/// `Finalizing` is stored as the `Tendering` it wraps
/// (`#[serde(transparent)]`), and the row's `state` says which it is.
fn snapshot(tendering: &Tendering) -> Result<String, DbError> {
    serde_json::to_string(tendering).map_err(|e| {
        invalid(format!(
            "checkout of sale {} cannot be stored: {}",
            tendering.cart().id,
            e.classify_name()
        ))
    })
}

fn unreadable(sale: SaleId, error: &serde_json::Error) -> DbError {
    invalid(format!(
        "checkout of sale {sale}'s snapshot: {}",
        error.classify_name()
    ))
}

fn timestamp(sale: SaleId, column: &str, stored: &str) -> Result<Timestamp, DbError> {
    Timestamp::parse_iso8601(stored)
        .map_err(|e| invalid(format!("checkout of sale {sale}'s {column}: {e}")))
}

/// What a decode error says about the snapshot, without quoting it.
trait Classify {
    fn classify_name(&self) -> &'static str;
}

impl Classify for serde_json::Error {
    fn classify_name(&self) -> &'static str {
        match self.classify() {
            serde_json::error::Category::Io => "unreadable",
            serde_json::error::Category::Syntax => "not JSON",
            serde_json::error::Category::Data => "not a sale this build reads",
            serde_json::error::Category::Eof => "cut short",
        }
    }
}

fn bytes(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

fn uuid(bytes: &[u8], column: &'static str) -> Result<Uuid, DbError> {
    Uuid::from_slice(bytes).map_err(|_| DbError::IdWidthInvalid {
        table: "checkout_operation",
        column,
        found: bytes.len(),
    })
}

fn invalid(reason: String) -> DbError {
    DbError::StoredCheckoutInvalid { reason }
}
