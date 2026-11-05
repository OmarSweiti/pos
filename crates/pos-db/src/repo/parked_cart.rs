//! The register's parked carts (microstep 1.8.2b, `ref/schema.md` §0005).
//!
//! A parked cart is a basket set aside so the cashier can serve the next
//! customer, held exactly as it was (E.3). `parked_cart` is register-local and
//! never syncs (master plan C.14), so nothing here writes a delivery envelope.
//! Every write still takes the caller's `&Transaction` (`repo/mod.rs`).
//!
//! **One row, three states.** A row's id is its sale's id, so the cart that is
//! parked, resumed, changed, parked again and finally completed is one row
//! throughout:
//!
//! - **parked**: set aside, with no claim. [`ParkedCartRepository::park`]
//!   writes it.
//! - **active**: being worked on, claimed by one session's nonce. A cart is
//!   active from the moment the till opens it
//!   ([`ParkedCartRepository::hold`], 1.4.11), and again when
//!   [`ParkedCartRepository::resume`] claims it. Only that claim can replace
//!   its working snapshot ([`ParkedCartRepository::save_active`]), set it
//!   aside ([`ParkedCartRepository::repark`]) or remove it with the sale that
//!   completes it ([`ParkedCartRepository::consume_on_finalize`]).
//! - **gone**: once the sale it became is completed, in that sale's own
//!   transaction, or, holding no line at all, when the till drops it
//!   ([`ParkedCartRepository::discard`]). Nothing deletes the only durable copy
//!   of a basket with something in it.
//!
//! **Every working basket is on disk** (the owner's decision of 5 November
//! 2026): the till holds a cart from the moment it opens and saves every
//! change, so neither a crash nor a power cut loses one that was never parked.
//! When the till lets go of it without completing it (a sign-out, a crash
//! found at start-up, or a resume onto a till already holding a basket),
//! [`ParkedCartRepository::set_aside`] parks it, or drops it if it is empty.
//!
//! **A restart loses neither the basket nor the claim.** A claim committed
//! before a process died is still `active` when the register starts again.
//! [`ParkedCartRepository::reclaim_after_restart`] hands it to the new
//! process's session, because the old session died with the process. A second
//! live session cannot take it: a resume refuses an `active` row.
//!
//! **Who may resume, and until when.** Whoever holds `sale.resume` at the
//! cart's own register may resume it, whoever parked it (the owner's decision
//! on #363); the domain's `resume` checks the proof and returns the audit
//! intent for another cashier's cart, which the caller writes with its envelope
//! in the same transaction. A cart expires at the end of the business day it
//! was parked in (master plan C.2): resuming it on a later business day is
//! refused, and it is listed among the expired carts instead.

use pos_domain::cap;
use pos_domain::cart::{ParkedCart, park, resume};
use pos_domain::{
    AuditIntent, Authorized, BusinessDate, Cart, CartContext, RegisterId, SaleId, ShiftId,
    Timestamp, UserId,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::DbError;

/// A cart resumed: the cart, now the resuming till's, and the audit intent
/// the caller must write with its envelope in the same transaction (I-9) when
/// the cart was parked by another cashier.
#[derive(Debug, Clone, PartialEq)]
#[must_use = "a resumed cart may carry an audit intent that must be written"]
pub struct Resumed {
    pub cart: Cart,
    pub audit: Option<AuditIntent>,
}

/// What [`ParkedCartRepository::set_aside`] did with a working cart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetAside {
    /// It held a line, and is in the parked list.
    Parked,
    /// It held none, and is gone.
    Dropped,
}

/// A parked cart as the register lists it.
///
/// `Debug` is the derived one, which reaches [`Cart`]'s own: the buyer's name
/// stays redacted.
#[derive(Debug, Clone, PartialEq)]
pub struct ParkedEntry {
    pub id: SaleId,
    /// The label the cashier gave it, if any.
    pub label: Option<String>,
    /// The cashier who parked it.
    pub parked_by: UserId,
    pub parked_at: Timestamp,
    /// The business day it belongs to; it expires when that day ends.
    pub expires_on: BusinessDate,
    pub cart: ParkedCart,
}

/// The register's parked carts.
pub struct ParkedCartRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ParkedCartRepository<'c> {
    #[must_use]
    pub const fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Set `parked` aside under `label`, at `parked_at`. It expires at the end
    /// of its shift's business day.
    ///
    /// # Errors
    ///
    /// [`DbError::ParkedCartShiftUnknown`] for a shift this register does not
    /// hold, [`DbError::ParkedCartAlreadyParked`] for a cart already set aside
    /// or being worked on (a resumed cart is set aside again with
    /// [`Self::repark`]), and [`DbError::Sqlite`] if the write fails.
    pub fn park(
        &self,
        tx: &Transaction<'_>,
        parked: &ParkedCart,
        label: Option<&str>,
        parked_at: Timestamp,
    ) -> Result<SaleId, DbError> {
        let cart = parked.cart();
        let expires_on = business_date_of(tx, cart.shift_id)?;
        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM parked_cart WHERE id = ?1",
                [bytes(cart.id.as_uuid())],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(DbError::ParkedCartAlreadyParked(cart.id));
        }
        tx.execute(
            "INSERT INTO parked_cart
               (id, register_id, cashier_id, label, snapshot, parked_at, expires_on,
                state, session_nonce)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'parked', NULL)",
            params![
                bytes(cart.id.as_uuid()),
                bytes(cart.register_id.as_uuid()),
                bytes(cart.cashier_id.as_uuid()),
                label,
                snapshot(cart)?,
                parked_at.to_iso8601(),
                expires_on.to_iso(),
            ],
        )?;
        Ok(cart.id)
    }

    /// Hold `cart`, which the till has just opened, as the working cart the
    /// session `session_nonce` claims: an `active` row from its first moment,
    /// so every change to it can be saved ([`Self::save_active`]) and a crash
    /// leaves it for start-up to find ([`Self::reclaim_after_restart`]).
    ///
    /// A cart never parked records when it was opened as its `parked_at`, the
    /// moment its basket began. It expires at the end of its shift's business
    /// day, as a parked cart does.
    ///
    /// # Errors
    ///
    /// [`DbError::ParkedCartShiftUnknown`] for a shift this register does not
    /// hold, [`DbError::ParkedCartAlreadyParked`] for a cart the register
    /// already holds, and [`DbError::Sqlite`] if the write fails.
    pub fn hold(
        &self,
        tx: &Transaction<'_>,
        cart: &Cart,
        session_nonce: Uuid,
    ) -> Result<(), DbError> {
        let expires_on = business_date_of(tx, cart.shift_id)?;
        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM parked_cart WHERE id = ?1",
                [bytes(cart.id.as_uuid())],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(DbError::ParkedCartAlreadyParked(cart.id));
        }
        tx.execute(
            "INSERT INTO parked_cart
               (id, register_id, cashier_id, label, snapshot, parked_at, expires_on,
                state, session_nonce)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, 'active', ?7)",
            params![
                bytes(cart.id.as_uuid()),
                bytes(cart.register_id.as_uuid()),
                bytes(cart.cashier_id.as_uuid()),
                snapshot(cart)?,
                cart.opened_at.to_iso8601(),
                expires_on.to_iso(),
                bytes(session_nonce),
            ],
        )?;
        Ok(())
    }

    /// Drop the working cart `id` that this session claimed, which holds no
    /// line: a basket with nothing in it is not kept (the owner's decision of
    /// 5 November 2026). A cart with lines is set aside with [`Self::repark`]
    /// instead. The stored snapshot is what is checked, so a caller holding a
    /// stale copy cannot drop a basket that has lines on disk.
    ///
    /// # Errors
    ///
    /// [`DbError::ParkedCartNotClaimed`] when this session holds no claim on
    /// the cart, [`DbError::ParkedCartNotEmpty`] when it holds a line,
    /// [`DbError::StoredParkedCartInvalid`] for a snapshot this build cannot
    /// read, and [`DbError::Sqlite`] if a read or the write fails.
    pub fn discard(
        &self,
        tx: &Transaction<'_>,
        session_nonce: Uuid,
        id: SaleId,
    ) -> Result<(), DbError> {
        let stored: Option<String> = tx
            .query_row(
                "SELECT snapshot FROM parked_cart
                  WHERE id = ?1 AND state = 'active' AND session_nonce = ?2",
                params![bytes(id.as_uuid()), bytes(session_nonce)],
                |row| row.get(0),
            )
            .optional()?;
        let stored = stored.ok_or(DbError::ParkedCartNotClaimed(id))?;
        if !read_snapshot(id, &stored)?.cart().lines.is_empty() {
            return Err(DbError::ParkedCartNotEmpty(id));
        }
        let removed = tx.execute(
            "DELETE FROM parked_cart WHERE id = ?1 AND state = 'active' AND session_nonce = ?2",
            params![bytes(id.as_uuid()), bytes(session_nonce)],
        )?;
        claimed(removed, id)
    }

    /// Set aside the working cart this session claimed, as the till holds it
    /// now, at `at`: a basket with lines goes to the parked list, with no
    /// label and under its cashier ([`Self::repark`]), and one with nothing in
    /// it is dropped ([`Self::discard`]). A sign-out, a crash found at start-up
    /// and a resume onto a till already holding a basket each set the till's
    /// basket aside this way (the owner's decisions of 5 and 6 November 2026).
    ///
    /// # Errors
    ///
    /// As [`Self::repark`] and [`Self::discard`]. A till's copy with no line
    /// over a stored snapshot that has one is
    /// [`DbError::ParkedCartNotEmpty`]: the basket on disk is never dropped on
    /// a stale copy's word.
    pub fn set_aside(
        &self,
        tx: &Transaction<'_>,
        session_nonce: Uuid,
        cart: &Cart,
        at: Timestamp,
    ) -> Result<SetAside, DbError> {
        match park(cart.clone()) {
            Ok(parked) => {
                self.repark(tx, session_nonce, &parked, None, at)?;
                Ok(SetAside::Parked)
            }
            Err(_) => {
                self.discard(tx, session_nonce, cart.id)?;
                Ok(SetAside::Dropped)
            }
        }
    }

    /// Resume the parked cart `id` at `till`, on the till cashier's proof of
    /// `sale.resume`, and claim it for the session `session_nonce`.
    ///
    /// The claim and the working snapshot (the cart as the till now holds it)
    /// are one write, so a resume either commits whole or leaves the cart
    /// parked. The caller writes [`Resumed::audit`], when there is one, in the
    /// same transaction.
    ///
    /// # Errors
    ///
    /// - [`DbError::ParkedCartNotFound`]: no such cart on this register.
    /// - [`DbError::ParkedCartClaimed`]: a session is already working on it.
    /// - [`DbError::ParkedCartExpired`]: it was parked on an earlier business
    ///   day than the till's.
    /// - [`DbError::ParkedCartShiftUnknown`]: the till's shift is not held.
    /// - [`DbError::ResumeRefused`]: what the domain's `resume` refuses: another
    ///   register, the other training mode, another currency, or a proof that
    ///   is not the till's own cashier's.
    /// - [`DbError::StoredParkedCartInvalid`]: a snapshot this build cannot
    ///   read.
    pub fn resume(
        &self,
        tx: &Transaction<'_>,
        id: SaleId,
        till: &CartContext,
        auth: &Authorized<cap::SaleResume>,
        session_nonce: Uuid,
    ) -> Result<Resumed, DbError> {
        let row: Option<(String, String, String)> = tx
            .query_row(
                "SELECT state, snapshot, expires_on FROM parked_cart WHERE id = ?1",
                [bytes(id.as_uuid())],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (state, stored, expires_on) = row.ok_or(DbError::ParkedCartNotFound(id))?;
        if state != "parked" {
            return Err(DbError::ParkedCartClaimed(id));
        }
        let expires_on = BusinessDate::parse(&expires_on)
            .map_err(|e| invalid(format!("parked cart {id}'s expiry: {e}")))?;
        if business_date_of(tx, till.shift_id)? > expires_on {
            return Err(DbError::ParkedCartExpired {
                id,
                expires_on: expires_on.to_iso(),
            });
        }
        let parked = read_snapshot(id, &stored)?;
        let (cart, audit) = resume(parked, till, auth)?;
        let claimed = tx.execute(
            "UPDATE parked_cart
                SET state = 'active', session_nonce = ?1, cashier_id = ?2, snapshot = ?3
              WHERE id = ?4 AND state = 'parked'",
            params![
                bytes(session_nonce),
                bytes(cart.cashier_id.as_uuid()),
                snapshot(&cart)?,
                bytes(id.as_uuid()),
            ],
        )?;
        if claimed != 1 {
            return Err(DbError::ParkedCartClaimed(id));
        }
        Ok(Resumed { cart, audit })
    }

    /// Replace the working snapshot of the cart this session claimed, after a
    /// change to it: a line added, a quantity set, a discount given. The
    /// register's copy is then the basket the cashier is looking at, whatever
    /// happens to the process next.
    ///
    /// # Errors
    ///
    /// [`DbError::ParkedCartNotClaimed`] when this session holds no claim on
    /// the cart, and [`DbError::Sqlite`] if the write fails.
    pub fn save_active(
        &self,
        tx: &Transaction<'_>,
        session_nonce: Uuid,
        cart: &Cart,
    ) -> Result<(), DbError> {
        let saved = tx.execute(
            "UPDATE parked_cart SET snapshot = ?1
              WHERE id = ?2 AND state = 'active' AND session_nonce = ?3",
            params![
                snapshot(cart)?,
                bytes(cart.id.as_uuid()),
                bytes(session_nonce)
            ],
        )?;
        claimed(saved, cart.id)
    }

    /// Set aside again the cart this session claimed: it is parked as it now
    /// is, under `label`, by the cashier now holding it, with its claim
    /// released. It keeps the business day it was first parked in.
    ///
    /// # Errors
    ///
    /// As [`Self::save_active`].
    pub fn repark(
        &self,
        tx: &Transaction<'_>,
        session_nonce: Uuid,
        parked: &ParkedCart,
        label: Option<&str>,
        parked_at: Timestamp,
    ) -> Result<(), DbError> {
        let cart = parked.cart();
        let reparked = tx.execute(
            "UPDATE parked_cart
                SET state = 'parked', session_nonce = NULL, snapshot = ?1, label = ?2,
                    parked_at = ?3, cashier_id = ?4
              WHERE id = ?5 AND state = 'active' AND session_nonce = ?6",
            params![
                snapshot(cart)?,
                label,
                parked_at.to_iso8601(),
                bytes(cart.cashier_id.as_uuid()),
                bytes(cart.id.as_uuid()),
                bytes(session_nonce),
            ],
        )?;
        claimed(reparked, cart.id)
    }

    /// Remove the working cart `id` that this session claimed, inside the
    /// transaction that completes its sale (1.8.3). The cart and the sale it
    /// became commit together or not at all, so a basket is never lost and
    /// never left beside the sale it already is.
    ///
    /// # Errors
    ///
    /// As [`Self::save_active`].
    pub fn consume_on_finalize(
        &self,
        tx: &Transaction<'_>,
        id: SaleId,
        session_nonce: Uuid,
    ) -> Result<(), DbError> {
        let removed = tx.execute(
            "DELETE FROM parked_cart WHERE id = ?1 AND state = 'active' AND session_nonce = ?2",
            params![bytes(id.as_uuid()), bytes(session_nonce)],
        )?;
        claimed(removed, id)
    }

    /// At start-up, hand every cart `register` was working on when its process
    /// died to the new process's session, and return them, oldest first.
    ///
    /// Call it once, when the register starts and before any till claims a
    /// cart: every claim it finds belonged to a session that died with the
    /// process. During the life of a process a resume refuses a claimed cart,
    /// so a second live session never takes one over.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredParkedCartInvalid`] for a snapshot this build cannot
    /// read, and [`DbError::Sqlite`] if a read or the write fails.
    pub fn reclaim_after_restart(
        &self,
        tx: &Transaction<'_>,
        register: RegisterId,
        session_nonce: Uuid,
    ) -> Result<Vec<Cart>, DbError> {
        tx.execute(
            "UPDATE parked_cart SET session_nonce = ?1
              WHERE register_id = ?2 AND state = 'active'",
            params![bytes(session_nonce), bytes(register.as_uuid())],
        )?;
        let mut statement = tx.prepare(
            "SELECT id, snapshot FROM parked_cart
              WHERE register_id = ?1 AND state = 'active'
              ORDER BY parked_at, id",
        )?;
        let rows = statement
            .query_map([bytes(register.as_uuid())], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, stored)| {
                let id = sale_id(&id)?;
                Ok(read_snapshot(id, &stored)?.cart().clone())
            })
            .collect()
    }

    /// The carts parked at `register` that can still be resumed on the
    /// business day `on`, oldest first.
    ///
    /// # Errors
    ///
    /// [`DbError::StoredParkedCartInvalid`] for a row this build cannot read,
    /// and [`DbError::Sqlite`] if the read fails.
    pub fn parked(
        &self,
        register: RegisterId,
        on: BusinessDate,
    ) -> Result<Vec<ParkedEntry>, DbError> {
        self.entries(register, on, "expires_on >= ?2")
    }

    /// The carts parked at `register` whose business day ended before `on`,
    /// oldest first: what the end-of-day report lists (master plan C.2). They
    /// can no longer be resumed.
    ///
    /// # Errors
    ///
    /// As [`Self::parked`].
    pub fn expired(
        &self,
        register: RegisterId,
        on: BusinessDate,
    ) -> Result<Vec<ParkedEntry>, DbError> {
        self.entries(register, on, "expires_on < ?2")
    }

    fn entries(
        &self,
        register: RegisterId,
        on: BusinessDate,
        expiry: &str,
    ) -> Result<Vec<ParkedEntry>, DbError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT id, label, cashier_id, parked_at, expires_on, snapshot
               FROM parked_cart
              WHERE register_id = ?1 AND state = 'parked' AND {expiry}
              ORDER BY parked_at, id"
        ))?;
        let rows = statement
            .query_map(params![bytes(register.as_uuid()), on.to_iso()], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, label, parked_by, parked_at, expires_on, stored)| {
                let id = sale_id(&id)?;
                Ok(ParkedEntry {
                    id,
                    label,
                    parked_by: UserId::from_uuid(uuid(&parked_by, "cashier_id")?),
                    parked_at: Timestamp::parse_iso8601(&parked_at)
                        .map_err(|e| invalid(format!("parked cart {id}'s parked_at: {e}")))?,
                    expires_on: BusinessDate::parse(&expires_on)
                        .map_err(|e| invalid(format!("parked cart {id}'s expiry: {e}")))?,
                    cart: read_snapshot(id, &stored)?,
                })
            })
            .collect()
    }
}

/// The business date of `shift`, which a parked cart belongs to and a resume
/// is measured against.
fn business_date_of(tx: &Transaction<'_>, shift: ShiftId) -> Result<BusinessDate, DbError> {
    let date: Option<String> = tx
        .query_row(
            "SELECT business_date FROM shift WHERE id = ?1",
            [bytes(shift.as_uuid())],
            |row| row.get(0),
        )
        .optional()?;
    let date = date.ok_or(DbError::ParkedCartShiftUnknown(shift))?;
    BusinessDate::parse(&date).map_err(|e| invalid(format!("shift {shift}'s business date: {e}")))
}

/// A cart's stored form: exactly its serialized form, which is also a
/// [`ParkedCart`]'s (`#[serde(transparent)]`).
fn snapshot(cart: &Cart) -> Result<String, DbError> {
    serde_json::to_string(cart)
        .map_err(|e| invalid(format!("cart {} cannot be stored: {e}", cart.id)))
}

/// A stored snapshot, read back as the parked cart it holds. The reason names
/// the cart and the decoder's complaint, never the snapshot, which can carry a
/// buyer's name.
fn read_snapshot(id: SaleId, stored: &str) -> Result<ParkedCart, DbError> {
    let parked: ParkedCart = serde_json::from_str(stored).map_err(|e| {
        invalid(format!(
            "parked cart {id}'s snapshot: {}",
            e.classify_name()
        ))
    })?;
    if parked.cart().id != id {
        return Err(invalid(format!(
            "parked cart {id} holds another sale's cart"
        )));
    }
    Ok(parked)
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
            serde_json::error::Category::Data => "not a cart this build reads",
            serde_json::error::Category::Eof => "cut short",
        }
    }
}

/// One row changed, or this session held no claim on the cart.
fn claimed(changed: usize, id: SaleId) -> Result<(), DbError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(DbError::ParkedCartNotClaimed(id))
    }
}

fn bytes(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

fn uuid(bytes: &[u8], column: &'static str) -> Result<Uuid, DbError> {
    Uuid::from_slice(bytes).map_err(|_| DbError::IdWidthInvalid {
        table: "parked_cart",
        column,
        found: bytes.len(),
    })
}

fn sale_id(bytes: &[u8]) -> Result<SaleId, DbError> {
    uuid(bytes, "id").map(SaleId::from_uuid)
}

fn invalid(reason: String) -> DbError {
    DbError::StoredParkedCartInvalid { reason }
}
