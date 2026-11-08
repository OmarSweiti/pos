//! The register's state: its database, which register it is, and who is
//! signed in (microstep 1.8.2c).
//!
//! It is opened once, at start-up, and held by Tauri for the life of the
//! process. Every command runs in it. A file this build will not open, or an
//! install that is not a register here, keeps the till closed: [`Till`] holds
//! the refusal instead, and every command answers with it, by name, so the
//! screen can say why rather than show an empty till.
//!
//! **Lock order.** The database, the session and what the till is working on
//! each sit behind a mutex. Code that needs more than one takes them through
//! [`Register::lock`] or [`Register::lock_till`], always database, then
//! session, then the working sale, so no two commands can each hold one and
//! wait for the other.
//!
//! **The working cart** is the basket the till is building (1.4.11). It is on
//! disk from the moment it opens, as an `active` `parked_cart` row under this
//! process's nonce, and every change is saved before it is answered (the
//! owner's decision of 5 November 2026). What the till holds in memory is that
//! row's copy, never a basket only the process knows.
//!
//! **A basket being paid for** is journalled in `checkout_operation` before
//! any money is taken, and each tender is journalled before it is answered
//! (1.8.3, 1.8.4). The till holds it as [`Working::Paying`], so nothing that
//! sets a basket aside can mistake it for one still being built.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use pos_db::DbError;
use pos_db::repo::checkout_operation::CheckoutOperationRepository;
use pos_db::repo::parked_cart::{ParkedCartRepository, SetAside};
use pos_db::repo::stock::{CacheDrift, StockRepository};
use pos_db::repo::user::{SessionEnd, UserRepository};
use pos_domain::{
    Cart, Clock, ClockConfidence, GrantSet, IdSource, RegisterId, Role, SaleId, StoreId, Tendering,
    Timestamp, UserId,
};
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

use crate::ipc::IpcError;

/// The register's database, in the app's data directory.
pub const DATABASE_FILE: &str = "register.db";

/// The install's provisioned device id, beside the database: a UUID that
/// first-run provisioning (1.8.5b) writes, never a hardware fingerprint
/// (`ref/schema.md` §0003). It names the `register` row this file serves.
pub const DEVICE_ID_FILE: &str = "device_id";

/// The wire shape this build writes delivery envelopes in
/// (`ref/sync-protocol.md` §2, "Envelope versioning").
pub const SYNC_PROTOCOL_VERSION: i64 = 1;

/// The build that produced a delivery envelope's payloads.
pub const PRODUCER_VERSION: &str = concat!("terminal/", env!("CARGO_PKG_VERSION"));

/// The till: open on a register, or closed with the reason it did not open.
pub enum Till {
    Open(Box<Register>),
    Closed(StartupError),
}

impl Till {
    /// Open the register in `data_dir`, with the key 1.8.5's policy supplies.
    /// A refusal is logged, through the scrubbing layer, and held.
    pub fn open(data_dir: &Path) -> Till {
        Self::from_result(Register::open(data_dir))
    }

    /// The till a start-up attempt leaves.
    pub fn from_result(opened: Result<Register, StartupError>) -> Till {
        match opened {
            Ok(register) => Till::Open(Box::new(register)),
            Err(refusal) => {
                tracing::error!(refusal = %refusal, "the till stays closed");
                Till::Closed(refusal)
            }
        }
    }

    /// The open register, or the refusal that keeps the till closed.
    ///
    /// # Errors
    ///
    /// `till_closed`, with the refusal's static detail.
    pub fn register(&self) -> Result<&Register, IpcError> {
        match self {
            Till::Open(register) => Ok(register),
            Till::Closed(refusal) => Err(refusal.to_ipc()),
        }
    }
}

/// Why the till did not open. Each names one cause, and [`StartupError::detail`]
/// is what the screen is told.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("the app's data directory is unavailable")]
    DataDirUnavailable,
    #[error("this install has no device id")]
    DeviceIdMissing,
    #[error("this install's device id cannot be read")]
    DeviceIdUnreadable,
    #[error("the database key is unavailable: {0}")]
    Key(#[source] DbError),
    #[error("the register database did not open: {0}")]
    Database(#[source] DbError),
    #[error("no register in this database has this install's device id")]
    RegisterUnknown,
    #[error("this install's register row cannot be read")]
    RegisterUnreadable,
    #[error("this install's register has been retired")]
    RegisterRetired,
    #[error("start-up recovery failed: {0}")]
    Recovery(#[source] DbError),
}

impl StartupError {
    /// The refusal, from a fixed set, as the screen is told it. Never the
    /// source error's text, which goes to the scrubbed log.
    pub fn detail(&self) -> &'static str {
        match self {
            StartupError::DataDirUnavailable => "the app's data directory is unavailable",
            StartupError::DeviceIdMissing => "this install has no device id",
            StartupError::DeviceIdUnreadable => "this install's device id cannot be read",
            StartupError::Key(_) => "the database key is unavailable",
            StartupError::Database(refusal) => database_refusal(refusal),
            StartupError::RegisterUnknown => "no register here is this install's",
            StartupError::RegisterUnreadable => "this install's register cannot be read",
            StartupError::RegisterRetired => "this install's register has been retired",
            StartupError::Recovery(_) => "start-up recovery failed",
        }
    }

    /// What a command answers while the till is closed.
    pub fn to_ipc(&self) -> IpcError {
        IpcError::new("till_closed", "error.till.closed").with_detail(self.detail())
    }
}

/// What `pos_db::open` refused, as the screen is told it: 1.8.0's engine
/// check, 1.8.1b's schema checks, and the key.
fn database_refusal(refusal: &DbError) -> &'static str {
    match refusal {
        DbError::BadKey => "wrong key or corrupt database",
        DbError::SchemaTooNew { .. } => "the database was written by a newer build",
        DbError::SchemaVersionInvalid { .. } => "the database reports an impossible version",
        DbError::SchemaShapeMismatch { .. } => "the database was altered by another build",
        DbError::UnsupportedStorageVersion { .. } | DbError::StorageVersionUnparseable { .. } => {
            "the storage engine has not been reviewed"
        }
        DbError::DurabilityRefused { .. }
        | DbError::JournalModeRefused { .. }
        | DbError::ForeignKeysRefused => "the database refused its storage settings",
        _ => "the register database did not open",
    }
}

/// One signed-in user, and the grants of their one role at this store.
///
/// It never reaches the webview: [`crate::commands::auth::SessionState`] is
/// what the screen is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The `user_session` row this session is recorded under.
    pub id: Uuid,
    pub user: UserId,
    pub code: String,
    pub display_name: String,
    pub role: Role,
    pub grants: GrantSet,
    pub signed_in_at: Timestamp,
}

/// What the till is working on for the signed-in cashier: a basket being
/// built, or that basket being paid for (1.8.3). Each is boxed, so the till
/// moves a pointer, not a sale, from one state to the next.
#[derive(Debug, Clone, PartialEq)]
pub enum Working {
    /// A basket being built, held by its `active` `parked_cart` row.
    Building(Box<Cart>),
    /// The basket being paid for, journalled in `checkout_operation`. Its
    /// `parked_cart` row still holds the basket as payment began, which the
    /// finalize consumes; the journal holds the tenders.
    Paying(Box<Paying>),
}

impl Working {
    /// The basket, whether it is being built or paid for.
    #[must_use]
    pub fn cart(&self) -> &Cart {
        match self {
            Working::Building(cart) => cart,
            Working::Paying(paying) => paying.tendering.cart(),
        }
    }
}

/// A sale being paid for, as the till holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Paying {
    pub tendering: Tendering,
    /// The checkout attempt's idempotency key, which its journal row is
    /// written under and its finalize will replay under (1.8.4).
    pub idempotency_key: Uuid,
    /// The journal's version this till last wrote. The next step names it,
    /// so a copy another write has overtaken is refused, never written.
    pub version: i64,
}

/// What start-up recovery found and did, before the first command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovered {
    /// Sessions a crash left open, now ended with `end_reason = 'crash'`.
    pub sessions_closed: usize,
    /// Baskets a crash left being worked on, now waiting in the parked list.
    /// The session that was building each one died with the process, so none
    /// is handed to whoever signs in next without being resumed.
    pub carts_set_aside: usize,
    /// Working carts a crash left with nothing in them, dropped.
    pub empty_carts_dropped: usize,
    /// Sales a restart found journalled mid-payment. Each is left where it
    /// is, its basket not parked, for 1.8.4's recovery half to resume (the
    /// owner's decision of 7 November 2026); `sale_in_flight_state` names
    /// them.
    pub sales_in_flight: usize,
    pub stock_cache: CacheCheck,
}

/// What holding the stock cache to its ledger found (1.10.3's note).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheCheck {
    /// The cache is what the ledger says.
    True,
    /// It was not, and was rebuilt in the same transaction. An alarm.
    Rebuilt(Vec<CacheDrift>),
    /// The ledger holds another register's events, which a register's own
    /// database never should. The cache was left as it was. An alarm.
    SpansRegisters,
}

/// The register, open.
pub struct Register {
    db: Mutex<Connection>,
    register: RegisterId,
    store: StoreId,
    process_nonce: Uuid,
    clock: Box<dyn Clock + Send + Sync>,
    ids: Box<dyn IdSource + Send + Sync>,
    session: Mutex<Option<Session>>,
    working: Mutex<Option<Working>>,
    recovered: Recovered,
}

/// The register's identity and what recovery did; never the session's
/// grants, the connection or the nonce.
impl std::fmt::Debug for Register {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Register")
            .field("register", &self.register)
            .field("store", &self.store)
            .field("recovered", &self.recovered)
            .finish_non_exhaustive()
    }
}

impl Register {
    /// Open the register in `data_dir`, with the database key 1.8.5's policy
    /// supplies: `POS_DB_KEY` in a debug build, the OS credential store
    /// otherwise.
    ///
    /// # Errors
    ///
    /// As [`Register::open_with`], and [`StartupError::Key`].
    pub fn open(data_dir: &Path) -> Result<Register, StartupError> {
        // The device id first: an install that is not a register here never
        // creates a database file, nor a key for one.
        read_device_id(data_dir)?;
        let key = pos_db::key::get_or_create().map_err(StartupError::Key)?;
        Self::open_with(
            data_dir,
            &key,
            Box::new(SystemClock),
            Box::new(V7Ids::default()),
        )
    }

    /// Open the register in `data_dir` with `key`, reading time from `clock`
    /// and minting ids from `ids`, and run start-up recovery.
    ///
    /// # Errors
    ///
    /// [`StartupError::DeviceIdMissing`] or [`StartupError::DeviceIdUnreadable`]
    /// for the device id; [`StartupError::Database`] for everything
    /// `pos_db::open` refuses; [`StartupError::RegisterUnknown`],
    /// [`StartupError::RegisterUnreadable`] or [`StartupError::RegisterRetired`]
    /// for the register row; and [`StartupError::Recovery`].
    pub fn open_with(
        data_dir: &Path,
        key: &str,
        clock: Box<dyn Clock + Send + Sync>,
        ids: Box<dyn IdSource + Send + Sync>,
    ) -> Result<Register, StartupError> {
        let device_id = read_device_id(data_dir)?;
        let mut conn =
            pos_db::open(&data_dir.join(DATABASE_FILE), key).map_err(StartupError::Database)?;
        let (register, store) = own_register(&conn, &device_id)?;
        let process_nonce = process_nonce(ids.as_ref());
        let recovered = recover(&mut conn, register, store, process_nonce, clock.now())?;
        Ok(Register {
            db: Mutex::new(conn),
            register,
            store,
            process_nonce,
            clock,
            ids,
            session: Mutex::new(None),
            working: Mutex::new(None),
            recovered,
        })
    }

    #[must_use]
    pub fn register(&self) -> RegisterId {
        self.register
    }

    #[must_use]
    pub fn store(&self) -> StoreId {
        self.store
    }

    /// The nonce this process claims parked carts under (1.8.2b).
    #[must_use]
    pub fn process_nonce(&self) -> Uuid {
        self.process_nonce
    }

    /// What start-up recovery did.
    #[must_use]
    pub fn recovered(&self) -> &Recovered {
        &self.recovered
    }

    /// Now, by the register's clock.
    #[must_use]
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// A fresh id.
    #[must_use]
    pub fn next_id(&self) -> Uuid {
        self.ids.next()
    }

    /// Who is signed in, if anyone.
    #[must_use]
    pub fn session(&self) -> Option<Session> {
        self.session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The database, for a command's transaction.
    pub fn with_db<T>(&self, work: impl FnOnce(&mut Connection) -> T) -> T {
        work(&mut self.db.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// The database and the session, in the one order every caller takes them.
    pub fn lock(&self) -> (MutexGuard<'_, Connection>, MutexGuard<'_, Option<Session>>) {
        let db = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        let session = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        (db, session)
    }

    /// The database, the session and what the till is working on, in that
    /// order.
    pub fn lock_till(&self) -> Locked<'_> {
        let (db, session) = self.lock();
        let working = self.working.lock().unwrap_or_else(PoisonError::into_inner);
        Locked {
            db,
            session,
            working,
        }
    }

    /// What the till is working on, for a caller already holding the database
    /// and the session, which it must have taken first.
    pub(crate) fn working_after(
        &self,
        _db: &MutexGuard<'_, Connection>,
        _session: &MutexGuard<'_, Option<Session>>,
    ) -> MutexGuard<'_, Option<Working>> {
        self.working.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How far the register trusts its clock. No trusted time is held yet:
    /// the state that anchors it is 1.1.9's database half, still to land. So
    /// no clock is trusted, and no scale label prices a line. Selling is never
    /// refused for a clock (`ref/ui-spec.md` §8): every other scan still sells.
    #[must_use]
    pub fn clock_confidence(&self) -> ClockConfidence {
        ClockConfidence::Untrusted
    }
}

/// Everything a cart or tender command works under, taken in the one order.
pub struct Locked<'r> {
    pub db: MutexGuard<'r, Connection>,
    pub session: MutexGuard<'r, Option<Session>>,
    pub working: MutexGuard<'r, Option<Working>>,
}

/// The install's device id, trimmed.
fn read_device_id(data_dir: &Path) -> Result<String, StartupError> {
    let text = match fs::read_to_string(data_dir.join(DEVICE_ID_FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(StartupError::DeviceIdMissing);
        }
        Err(_) => return Err(StartupError::DeviceIdUnreadable),
    };
    let device_id = text.trim();
    if device_id.is_empty() {
        return Err(StartupError::DeviceIdUnreadable);
    }
    Ok(device_id.to_owned())
}

/// The live `register` row whose `device_id` is this install's, and its store.
fn own_register(conn: &Connection, device_id: &str) -> Result<(RegisterId, StoreId), StartupError> {
    let row: Option<(Vec<u8>, Vec<u8>, bool)> = conn
        .query_row(
            "SELECT id, store_id, is_active = 1 AND deleted_at IS NULL
               FROM register WHERE device_id = ?1",
            [device_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| StartupError::Database(error.into()))?;
    let Some((id, store, is_live)) = row else {
        return Err(StartupError::RegisterUnknown);
    };
    if !is_live {
        return Err(StartupError::RegisterRetired);
    }
    let id = Uuid::from_slice(&id).map_err(|_| StartupError::RegisterUnreadable)?;
    let store = Uuid::from_slice(&store).map_err(|_| StartupError::RegisterUnreadable)?;
    Ok((RegisterId::from_uuid(id), StoreId::from_uuid(store)))
}

/// A random nonce for this process, or a fresh id if the OS has no randomness
/// to give: either is one no earlier process held.
fn process_nonce(ids: &(dyn IdSource + Send + Sync)) -> Uuid {
    let mut bytes = [0u8; 16];
    match getrandom::fill(&mut bytes) {
        Ok(()) => uuid::Builder::from_random_bytes(bytes).into_uuid(),
        Err(_) => ids.next(),
    }
}

/// 1.8.4's start-up recovery, as far as the steps before it left work for it.
///
/// Sessions and carts first, in one transaction. Every basket a crash left
/// being worked on is claimed and set aside in the parked list, where whoever
/// signs in can resume it, and one with nothing in it is dropped: the session
/// building it died with the process (1.4.11). A basket whose sale is
/// journalled mid-payment is claimed and left where it is, for the recovery
/// half to resume from its journal (the owner's decision of 7 November 2026):
/// parked, it could never be paid for again. Then the stock cache, in a
/// transaction of its own, because a cache that spans registers is an alarm
/// that leaves the cache as it was, not a reason to undo the rest.
fn recover(
    conn: &mut Connection,
    register: RegisterId,
    store: StoreId,
    nonce: Uuid,
    at: Timestamp,
) -> Result<Recovered, StartupError> {
    let failed = |error: DbError| StartupError::Recovery(error);
    let sqlite = |error: rusqlite::Error| StartupError::Recovery(error.into());

    let tx = conn.transaction().map_err(sqlite)?;
    let sessions_closed = UserRepository::new(&tx)
        .end_open_sessions(&tx, register, SessionEnd::Crash, at)
        .map_err(failed)?;
    let carts = ParkedCartRepository::new(&tx);
    let journal = CheckoutOperationRepository::new(&tx);
    let (mut carts_set_aside, mut empty_carts_dropped, mut sales_in_flight) = (0, 0, 0);
    for cart in carts
        .reclaim_after_restart(&tx, register, nonce)
        .map_err(failed)?
    {
        if journalled(&journal, cart.id).map_err(failed)? {
            sales_in_flight += 1;
            continue;
        }
        match carts.set_aside(&tx, nonce, &cart, at).map_err(failed)? {
            SetAside::Parked => carts_set_aside += 1,
            SetAside::Dropped => empty_carts_dropped += 1,
        }
    }
    tx.commit().map_err(sqlite)?;

    let tx = conn.transaction().map_err(sqlite)?;
    let checked = StockRepository::new(&tx).reconcile_cache(&tx, store);
    let stock_cache = match checked {
        Ok(drift) => {
            tx.commit().map_err(sqlite)?;
            if drift.is_empty() {
                CacheCheck::True
            } else {
                tracing::warn!(
                    drift = ?drift,
                    "start-up found the stock cache off its ledger and rebuilt it"
                );
                CacheCheck::Rebuilt(drift)
            }
        }
        Err(DbError::StockReplayAcrossRegisters) => {
            drop(tx);
            tracing::warn!(
                "start-up found another register's stock events; the cache is unchanged"
            );
            CacheCheck::SpansRegisters
        }
        Err(other) => return Err(failed(other)),
    };

    if sessions_closed > 0 {
        tracing::warn!(
            sessions_closed,
            "start-up closed sessions a crash left open"
        );
    }
    if carts_set_aside > 0 {
        tracing::warn!(
            carts_set_aside,
            "start-up set aside baskets a crash left being worked on"
        );
    }
    if sales_in_flight > 0 {
        tracing::warn!(
            sales_in_flight,
            "start-up left sales a crash interrupted mid-payment for recovery"
        );
    }
    Ok(Recovered {
        sessions_closed,
        carts_set_aside,
        empty_carts_dropped,
        sales_in_flight,
        stock_cache,
    })
}

/// Whether `sale` is journalled mid-payment. A row this build cannot read is
/// a sale in payment all the same: its basket is left for the recovery half
/// rather than parked beside a journal that still names it.
fn journalled(journal: &CheckoutOperationRepository<'_>, sale: SaleId) -> Result<bool, DbError> {
    match journal.get(sale) {
        Ok(found) => Ok(found.is_some()),
        Err(DbError::StoredCheckoutInvalid { .. }) => Ok(true),
        Err(other) => Err(other),
    }
}

/// The system's wall clock, in UTC milliseconds.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            });
        Timestamp::from_epoch_milliseconds(millis).unwrap_or(Timestamp::MAX)
    }
}

/// UUIDv7 ids, as the shell mints them in production (`ref/domain-api.md`
/// §2): the wall clock's milliseconds and 74 random bits. The random bits are
/// identity and index locality only, never order (I-7).
#[derive(Debug, Default)]
pub struct V7Ids {
    /// Stands in for the random bits if the OS has none to give, so two ids
    /// from one process still differ.
    fallback: AtomicU64,
}

impl IdSource for V7Ids {
    fn next(&self) -> Uuid {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
            });
        let mut random = [0u8; 10];
        if getrandom::fill(&mut random).is_err() {
            let count = self.fallback.fetch_add(1, Ordering::Relaxed);
            for (slot, byte) in random.iter_mut().zip(count.to_be_bytes()) {
                *slot = byte;
            }
        }
        uuid::Builder::from_unix_timestamp_millis(millis, &random).into_uuid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ids the shell mints are UUIDv7, and two never repeat.
    #[test]
    fn v7_ids_are_version_seven_and_distinct() {
        let ids = V7Ids::default();
        let (first, second) = (ids.next(), ids.next());
        assert_eq!(first.get_version_num(), 7);
        assert_ne!(first, second);
    }

    /// The system clock reads a moment after this code was written.
    #[test]
    fn the_system_clock_reads_the_present() {
        let written = Timestamp::parse_iso8601("2026-11-04T00:00:00.000Z");
        assert!(written.is_ok_and(|written| SystemClock.now() > written));
    }
}
