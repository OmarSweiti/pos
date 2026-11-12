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
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use pos_db::DbError;
use pos_db::repo::setting::{PrinterBinding, SettingRepository};
use pos_domain::{
    Cart, Clock, ClockConfidence, GrantSet, IdSource, RegisterId, Role, StoreId, Tendering,
    Timestamp, UserId,
};
use pos_hardware::ReceiptPrinter;
use pos_hardware::net::{Deadlines, NetworkPrinter};
use pos_hardware::profile::PrinterProfile;
use pos_hardware::profile::data::{self as profiles, BindingError};
use pos_hardware::render::PaperWidth;
use pos_hardware::transport::TcpTransport;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

use crate::ipc::IpcError;
use crate::recovery;
// What start-up recovery reports lives with it, and is named here too, where
// the register that holds it is.
pub use crate::recovery::{CacheCheck, Recovered};

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
    /// Shared, because the print worker `run` starts holds it too (1.7.14c).
    Open(Arc<Register>),
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
            Ok(register) => Till::Open(Arc::new(register)),
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

/// The printer a register renders its receipts for, and the device itself
/// when one is attached.
///
/// It comes from the register's printer binding, read when the till opens
/// (1.7.14b; [`PrinterBindingState`]). With no binding, or one the register
/// cannot honour, the register renders for the 80 mm simulator profile with
/// no device attached, as every register did by the owner's decision of 8
/// November 2026: each sale's receipt is still rendered and queued in its own
/// transaction, and simply never reaches a device
/// (`ref/hardware-and-receipts.md` §3a, selling never stops).
pub struct ReceiptDevice {
    /// The profile every original receipt is rendered for.
    pub profile: PrinterProfile,
    /// The printer the finalize sends a receipt to, and opens the drawer
    /// through, once the sale has committed.
    pub printer: Option<Arc<dyn ReceiptPrinter>>,
}

impl ReceiptDevice {
    /// What a register renders for until a printer is bound: the 80 mm
    /// simulator profile, and no device.
    #[must_use]
    pub fn unbound() -> Self {
        Self {
            profile: PrinterProfile::simulator(PaperWidth::Mm80),
            printer: None,
        }
    }
}

/// What a register's printer binding came to when the till opened (1.7.14b),
/// as `health_status` tells it.
///
/// A binding the register cannot honour never keeps the till closed (the
/// owner's decision of 11 November 2026): the register opens as an unbound
/// one, and the refusal is an alarm on its health, naming why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PrinterBindingState {
    /// None is set. The register renders for the simulator's profile and
    /// attaches no device.
    Unbound,
    /// The register renders for this profile and prints to the device at
    /// this address.
    Bound {
        profile_id: String,
        version: u32,
        address: String,
    },
    /// A binding is set and the register cannot honour it, so the till opened
    /// as an unbound one. `profile_id` and `version` are what the binding
    /// names, when it could be read; `unresolved` the parameters the driver
    /// needs that the profile leaves unknown.
    Refused {
        reason: BindingRefusal,
        profile_id: Option<String>,
        version: Option<u32>,
        unresolved: Vec<&'static str>,
    },
}

/// Why a register could not honour its printer binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingRefusal {
    /// The stored value is not the shape `printer.binding` promises, or it
    /// could not be read.
    SettingUnreadable,
    /// The build ships no such profile, or no such version of it.
    ProfileNotShipped,
    /// The profile leaves a parameter the driver needs unknown: every real
    /// device's does until milestone 5.8 measures it.
    ProfileUnresolved,
    /// The profile's device is not reached over TCP 9100, the one transport
    /// a register binds today.
    NotANetworkPrinter,
}

/// The register's printer, from its binding: the device and what the binding
/// came to. Nothing here is written, and nothing refused closes the till.
fn bind_printer(conn: &Connection, register: RegisterId) -> (ReceiptDevice, PrinterBindingState) {
    let refused =
        |reason: BindingRefusal, named: Option<&PrinterBinding>, unresolved: Vec<&'static str>| {
            tracing::warn!(
                reason = ?reason,
                profile_id = named.map(|binding| binding.profile_id.as_str()),
                "the register's printer binding was refused; the till opens with no device"
            );
            (
                ReceiptDevice::unbound(),
                PrinterBindingState::Refused {
                    reason,
                    profile_id: named.map(|binding| binding.profile_id.clone()),
                    version: named.map(|binding| binding.version.get()),
                    unresolved,
                },
            )
        };
    let binding = match SettingRepository::new(conn).printer_binding(register) {
        Ok(Some(binding)) => binding,
        Ok(None) => return (ReceiptDevice::unbound(), PrinterBindingState::Unbound),
        Err(_) => return refused(BindingRefusal::SettingUnreadable, None, Vec::new()),
    };
    let profile = match profiles::printer(&binding.profile_id, binding.version) {
        Ok(profile) => profile,
        Err(BindingError::NotShipped { .. }) => {
            return refused(
                BindingRefusal::ProfileNotShipped,
                Some(&binding),
                Vec::new(),
            );
        }
        Err(BindingError::Unresolved { parameters, .. }) => {
            let unresolved = parameters.iter().map(|parameter| parameter.key()).collect();
            return refused(
                BindingRefusal::ProfileUnresolved,
                Some(&binding),
                unresolved,
            );
        }
    };
    match NetworkPrinter::new(
        TcpTransport::new(binding.address),
        profile.clone(),
        Deadlines::default(),
    ) {
        Ok(printer) => (
            ReceiptDevice {
                profile,
                printer: Some(Arc::new(printer)),
            },
            PrinterBindingState::Bound {
                profile_id: binding.profile_id,
                version: binding.version.get(),
                address: binding.address.to_string(),
            },
        ),
        Err(_) => refused(
            BindingRefusal::NotANetworkPrinter,
            Some(&binding),
            Vec::new(),
        ),
    }
}

/// The register, open.
pub struct Register {
    db: Mutex<Connection>,
    register: RegisterId,
    store: StoreId,
    /// The register's code, `REG01`: its receipts' prefix (1.9.3).
    code: String,
    process_nonce: Uuid,
    clock: Box<dyn Clock + Send + Sync>,
    ids: Box<dyn IdSource + Send + Sync>,
    session: Mutex<Option<Session>>,
    working: Mutex<Option<Working>>,
    recovered: Recovered,
    receipt_device: ReceiptDevice,
    printer_binding: PrinterBindingState,
    /// The print worker's nudge, once one runs (1.7.14c).
    print_nudge: OnceLock<Sender<()>>,
}

/// The register's identity and what recovery did; never the session's
/// grants, the connection or the nonce.
impl std::fmt::Debug for Register {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Register")
            .field("register", &self.register)
            .field("store", &self.store)
            .field("code", &self.code)
            .field("printer_profile", &self.receipt_device.profile.profile_id)
            .field("printer_attached", &self.receipt_device.printer.is_some())
            .field("printer_binding", &self.printer_binding)
            .field("recovered", &self.recovered)
            .finish_non_exhaustive()
    }
}

impl Register {
    /// Open the register in `data_dir`, with the database key 1.8.5's policy
    /// supplies: `POS_DB_KEY` in a debug build, the OS credential store
    /// otherwise. A key is minted only when there is no database yet: one
    /// whose key is gone is the recovery code's to open (1.8.5b).
    ///
    /// # Errors
    ///
    /// As [`Register::open_with`], and [`StartupError::Key`].
    pub fn open(data_dir: &Path) -> Result<Register, StartupError> {
        // The device id first: an install that is not a register here never
        // creates a database file, nor a key for one.
        read_device_id(data_dir)?;
        let (key, _) =
            pos_db::key::get_or_create(&data_dir.join(DATABASE_FILE)).map_err(StartupError::Key)?;
        Self::open_with(
            data_dir,
            key.expose(),
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
        let (register, store, code) = own_register(&conn, &device_id)?;
        let process_nonce = process_nonce(ids.as_ref());
        let recovered = recovery::recover(&mut conn, register, store, process_nonce, clock.now())?;
        let (receipt_device, printer_binding) = bind_printer(&conn, register);
        Ok(Register {
            db: Mutex::new(conn),
            register,
            store,
            code,
            process_nonce,
            clock,
            ids,
            session: Mutex::new(None),
            working: Mutex::new(None),
            recovered,
            receipt_device,
            printer_binding,
            print_nudge: OnceLock::new(),
        })
    }

    /// This register, rendering its receipts for `device` instead of what its
    /// binding gave it. A device attached this way is not a binding: the
    /// register's [`PrinterBindingState`] is what was read at open.
    #[must_use]
    pub fn with_receipt_device(mut self, device: ReceiptDevice) -> Self {
        self.receipt_device = device;
        self
    }

    /// The printer the register renders its receipts for, and the device, if
    /// one is attached.
    #[must_use]
    pub fn receipt_device(&self) -> &ReceiptDevice {
        &self.receipt_device
    }

    /// What the register's printer binding came to when the till opened.
    #[must_use]
    pub fn printer_binding(&self) -> &PrinterBindingState {
        &self.printer_binding
    }

    /// Give the register its print worker's nudge. `false` when it already
    /// has one: a register has one worker.
    pub(crate) fn set_print_nudge(&self, nudge: Sender<()>) -> bool {
        self.print_nudge.set(nudge).is_ok()
    }

    /// Ask the print worker to drain the queue now. `false` when no worker
    /// runs, or it has stopped; the receipt then waits for one.
    pub fn nudge_printer(&self) -> bool {
        self.print_nudge
            .get()
            .is_some_and(|nudge| nudge.send(()).is_ok())
    }

    #[must_use]
    pub fn register(&self) -> RegisterId {
        self.register
    }

    #[must_use]
    pub fn store(&self) -> StoreId {
        self.store
    }

    /// The register's code, as its row held it when the till opened: `REG01`.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
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

/// The live `register` row whose `device_id` is this install's: its id, its
/// store and its code.
fn own_register(
    conn: &Connection,
    device_id: &str,
) -> Result<(RegisterId, StoreId, String), StartupError> {
    let row: Option<(Vec<u8>, Vec<u8>, bool, String)> = conn
        .query_row(
            "SELECT id, store_id, is_active = 1 AND deleted_at IS NULL, code
               FROM register WHERE device_id = ?1",
            [device_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| StartupError::Database(error.into()))?;
    let Some((id, store, is_live, code)) = row else {
        return Err(StartupError::RegisterUnknown);
    };
    if !is_live {
        return Err(StartupError::RegisterRetired);
    }
    let id = Uuid::from_slice(&id).map_err(|_| StartupError::RegisterUnreadable)?;
    let store = Uuid::from_slice(&store).map_err(|_| StartupError::RegisterUnreadable)?;
    Ok((RegisterId::from_uuid(id), StoreId::from_uuid(store), code))
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
