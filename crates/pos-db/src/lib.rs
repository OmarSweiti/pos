//! Encrypted local store for the register.
//! Owns: connection setup, key handling, migrations. (Blueprint §2, §7)

use rusqlite::Connection;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

pub mod auth;
#[cfg(feature = "fixtures")]
pub mod fixtures;
pub mod key;
pub mod repo;
pub mod search;
pub mod seed;
pub mod storage_version;
pub mod tax_pack;
pub mod tax_policy;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("wrong encryption key or corrupt database")]
    BadKey,
    #[error("keyring: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("could not generate a database key from the OS: {0}")]
    KeyGeneration(String),
    #[error(
        "database schema is version {found}, but this build only knows {supported}; \
         it was written by a newer version of the application"
    )]
    SchemaTooNew { found: i64, supported: usize },
    #[error("database reports an impossible schema version ({found})")]
    SchemaVersionInvalid { found: i64 },
    /// The file's schema is not the one its recorded version creates (E.58).
    /// Each list names objects as `<type> <name>`, such as
    /// `trigger audit_log_no_update`.
    #[error(
        "refusing to open: this file records schema version {recorded}, but its schema is \
         not that version's ({}). A migration was interrupted, or the schema was changed \
         outside this application, and which one cannot be known from here; nothing is \
         repaired or migrated. Restore the last verified backup",
        shape_difference(.missing, .unexpected, .altered)
    )]
    SchemaShapeMismatch {
        recorded: i64,
        /// Objects the recorded version creates that the file lacks.
        missing: Vec<String>,
        /// Objects the file holds that the recorded version does not create.
        unexpected: Vec<String>,
        /// Objects both hold, defined differently.
        altered: Vec<String>,
    },
    #[error(
        "refusing to open a register database with synchronous={found}: a completed \
         sale must survive a power cut, which requires synchronous=FULL (2)"
    )]
    DurabilityRefused { found: i64 },
    #[error(
        "refusing to open a register database in {found} journal mode: the durability \
         guarantee this build makes holds in WAL, and nowhere else"
    )]
    JournalModeRefused { found: String },
    #[error("refusing to open a register database with foreign keys disabled")]
    ForeignKeysRefused,
    #[error(
        "refusing to open on {engine} {found}: this build is reviewed for {engine} \
         {minimum} or later (microstep 1.8.0)"
    )]
    UnsupportedStorageVersion {
        engine: &'static str,
        found: String,
        minimum: String,
    },
    #[error("refusing to open: {engine} reports a version this build cannot read ({found:?})")]
    StorageVersionUnparseable { engine: &'static str, found: String },
    #[error(transparent)]
    Auth(#[from] auth::AuthError),
    #[error("stored sign-in state is unusable: {reason}")]
    AuthStateInvalid { reason: &'static str },
    #[error(
        "a {scope_kind}-scoped sequence cannot number a {kind} document: \
         migration 0005 pairs receipt and zreport with a register, and \
         fiscal_icv with a store"
    )]
    SequenceScopeInvalid {
        scope_kind: &'static str,
        kind: &'static str,
    },
    #[error(
        "refusing to allocate a {kind} number in Phase 1: the first row closes \
         the reversal window on merchant decision 6.9 (#113), and microstep \
         2.7.4 must re-check it before allocating"
    )]
    SequenceNotYetAllocatable { kind: &'static str },
    #[error(
        "cannot report {kind} gaps: the documents this counter numbers do not \
         exist yet, so an empty answer would assert soundness nobody has shown"
    )]
    SequenceEvidenceUnavailable { kind: &'static str },
    #[error("stored sequence value {found} is not a document number")]
    SequenceValueInvalid { found: i64 },
    #[error(
        "receipt number {found:?} does not carry this register\'s prefix and a \
         decimal counter, so a gap report over it would be a guess"
    )]
    SequenceNumberUnreadable { found: String },
    #[error(
        "a {kind} prefix must be non-empty and hold no whitespace or control \
         character, because a cashier types it back to find the document"
    )]
    SequencePrefixInvalid { kind: &'static str },
    #[error(
        "this {kind} counter already issued numbers with no prefix, and none can \
         be put in front of them now without making its gap report a guess"
    )]
    SequencePrefixMissing { kind: &'static str },
    #[error(
        "refusing to write a delivery envelope with no members: a business \
         transaction that produced no fact has nothing to deliver (I-9)"
    )]
    EmptyCommitRefused,
    #[error(
        "delivery envelope is incomplete: commit_size {commit_size}, but \
         {members} manifest rows and {delivery_rows} delivery rows"
    )]
    CommitEnvelopeIncomplete {
        commit_size: i64,
        members: i64,
        delivery_rows: i64,
    },
    #[error("approval handle is already consumed; a one-use handle cannot be spent twice")]
    ApprovalAlreadyConsumed,
    #[error("approval consumption did not match one bound financial effect and audit row")]
    ApprovalConsumptionUnbound,
    #[error("stored approval handle is malformed: {reason}")]
    InvalidStoredApproval { reason: String },
    #[error("stored clock state is malformed: {reason}")]
    ClockStateInvalid { reason: String },
    #[error("a print lease must expire after the moment it is taken")]
    PrintLeaseInvalid,
    #[error("stored receipt or print record is malformed: {reason}")]
    StoredPrintRecordInvalid { reason: String },
    #[error("audit payload cannot be chained as written: {reason}")]
    AuditPayloadRefused { reason: String },
    #[error(
        "refusing to append an audit fact with no delivery envelope: write the \
         commit and its manifest naming this row first, in the same \
         transaction (I-9)"
    )]
    AuditEnvelopeMissing,
    #[error(
        "audit_log holds sequence number {found}, which is no chain position: \
         `seq` is hashed into every entry and an append can only ever have \
         written a positive one"
    )]
    AuditSeqInvalid { found: i64 },
    #[error(
        "cannot append onto audit_log seq {seq}: this register's head row \
         cannot be read ({reason}), and chaining onto a head this build does \
         not understand would write a link nothing can verify"
    )]
    AuditHeadUnreadable { seq: i64, reason: String },
    #[error(
        "this build of SQLite has no {0}: the register would open, and then return \
         an empty result for every search instead of failing"
    )]
    MissingFeature(&'static str),
    #[error("{table}.{column} holds a {found}-byte id; ids are BLOB(16) (conventions §2)")]
    IdWidthInvalid {
        table: &'static str,
        column: &'static str,
        found: usize,
    },
    #[error("catalogue save refused: {0}")]
    CatalogSaveRefused(#[from] pos_domain::CatalogError),
    #[error(
        "barcode {code} is already claimed by a live code of product {holder} (E.36); \
         retire that claim before giving the code to another product"
    )]
    BarcodeAlreadyClaimed {
        code: String,
        holder: pos_domain::ProductId,
    },
    #[error(
        "product {0} has been retired; saving it again does not bring it back, \
         because reviving a retired product is a decision of its own"
    )]
    ProductRetired(pos_domain::ProductId),
    #[error("stored product is malformed: {reason}")]
    StoredProductInvalid { reason: String },
    #[error("stored role limit is malformed: {reason}")]
    StoredRoleLimitInvalid { reason: String },
    /// A role-grid cell, or a user's role, this build cannot read as a grant.
    /// Read as nothing, it would be a capability lost or one given away, so it
    /// is refused by name (1.8.2c).
    #[error("stored role grant is unreadable: {reason}")]
    StoredRoleGrantInvalid { reason: String },
    #[error(
        "refusing to record scale verification evidence with no delivery \
         envelope naming it: write the commit and its manifest first, in the \
         same transaction (I-9)"
    )]
    ScaleEvidenceEnvelopeMissing,
    #[error(
        "the delivery envelope naming this scale verification event carries a \
         different payload, so what travels would not be what was recorded"
    )]
    ScaleEvidenceEnvelopeMismatch,
    #[error("stored trade-scale record is malformed: {reason}")]
    StoredScaleRecordInvalid { reason: String },
    #[error("stock movement refused: {0}")]
    StockRefused(#[from] pos_domain::StockError),
    #[error("no store {0} to move stock in")]
    StockStoreUnknown(pos_domain::StoreId),
    #[error("no product {0} to move stock of")]
    StockProductUnknown(pos_domain::ProductId),
    #[error(
        "a stock movement of {qty_milli} milli-units is not a whole number of the \
         product's {step_milli}-milli-unit steps"
    )]
    StockQtyOffStep { qty_milli: i64, step_milli: i64 },
    #[error(
        "refusing to append a stock event with no delivery envelope naming it: \
         write the commit and its manifest first, in the same transaction (I-9)"
    )]
    StockEnvelopeMissing,
    #[error(
        "the delivery envelope naming this stock event carries a different \
         payload, so what travels would not be what was recorded"
    )]
    StockEnvelopeMismatch,
    #[error(
        "this store's stock ledger holds more than one register's events, and no \
         owned sequence orders them against each other (I-7), so the cache is not \
         rebuilt from them"
    )]
    StockReplayAcrossRegisters,
    #[error("stored stock record is malformed: {reason}")]
    StoredStockRecordInvalid { reason: String },
    #[error("register {register} already has shift {open} open; close it before opening another")]
    ShiftAlreadyOpen {
        register: pos_domain::RegisterId,
        open: pos_domain::ShiftId,
    },
    #[error("a shift's float is counted in its store's currency")]
    ShiftFloatCurrencyMismatch,
    #[error(
        "refusing to write a {entity} with no delivery envelope naming it: write the \
         commit and its manifest first, in the same transaction (I-9)"
    )]
    ShiftEnvelopeMissing { entity: &'static str },
    #[error(
        "the delivery envelope naming this {entity} carries a different payload, so what \
         travels would not be what was recorded"
    )]
    ShiftEnvelopeMismatch { entity: &'static str },
    #[error("no shift {0}")]
    ShiftNotFound(pos_domain::ShiftId),
    #[error("shift {0} is not open")]
    ShiftNotOpen(pos_domain::ShiftId),
    #[error(
        "shift opened by {opened_by} is closed on the ordinary path only by its opener, \
         not {closed_by}"
    )]
    ShiftClosedByAnother {
        opened_by: pos_domain::UserId,
        closed_by: pos_domain::UserId,
    },
    #[error("stored shift record is malformed: {reason}")]
    StoredShiftRecordInvalid { reason: String },
    #[error("opening-stock file line {line}: {reason}")]
    OpeningStockFileInvalid { line: usize, reason: &'static str },
    #[error("no prepared stock adjustment {0}")]
    StockAdjustmentNotFound(pos_domain::StockEventId),
    #[error(
        "prepared stock adjustment {0} no longer matches the digest it was stored with, \
         so it is not what a manager was shown"
    )]
    StockAdjustmentAltered(pos_domain::StockEventId),
    #[error("the approval does not authorise this operation: {0}")]
    ApprovalRefused(pos_domain::PermissionError),
    #[error("no approval {0} is stored")]
    ApprovalMissing(pos_domain::ApprovalId),
    #[error("no store {0} to compute tax for")]
    TaxPolicyStoreUnknown(pos_domain::StoreId),
    #[error(
        "store {0} has no tax computation policy this register holds, so no line can \
         be taxed and no sale finished until an approved one is assigned"
    )]
    TaxPolicyMissing(pos_domain::StoreId),
    #[error(
        "store {store}'s tax computation policy {jurisdiction} {policy_version} is not \
         approved, so no line can be taxed and no sale finished under it"
    )]
    TaxPolicyUnapproved {
        store: pos_domain::StoreId,
        jurisdiction: String,
        policy_version: String,
    },
    #[error("stored tax computation policy is malformed: {reason}")]
    StoredTaxPolicyInvalid { reason: String },
    #[error("tax pack refused: {0}")]
    TaxPackRefused(#[from] tax_pack::TaxPackRefusal),
    #[error("no store {0} to read a tax pack for")]
    TaxPackStoreUnknown(pos_domain::StoreId),
    #[error(
        "store {0} has no tax pack this register holds, so no line can be taxed and no \
         sale finished until an approved one is assigned"
    )]
    TaxPackMissing(pos_domain::StoreId),
    #[error("store {0}'s tax pack is not approved, so no line can be taxed under it")]
    TaxPackUnapproved(pos_domain::StoreId),
    #[error(
        "store {store}'s approved tax pack charges nothing on category {category} on {on}: \
         an unconfigured band fails closed rather than being sold at a familiar rate"
    )]
    TaxBandUnconfigured {
        store: pos_domain::StoreId,
        category: pos_domain::TaxCategoryId,
        on: String,
    },
    #[error("stored tax pack is malformed: {reason}")]
    StoredTaxPackInvalid { reason: String },
    #[error("no register {0} on this database to complete a sale on")]
    SaleRegisterUnknown(pos_domain::RegisterId),
    #[error("this completed sale cannot be stored exactly: {reason}")]
    SaleNotStorable { reason: String },
    #[error(
        "refusing to write a sale's {entity} with no delivery envelope naming it: write the \
         commit and its manifest first, in the same transaction (I-9)"
    )]
    SaleEnvelopeMissing { entity: &'static str },
    #[error(
        "the delivery envelope naming this sale's {entity} carries another commit or another \
         payload, so what travels would not be what was recorded"
    )]
    SaleEnvelopeMismatch { entity: &'static str },
    #[error("stored sale is malformed: {reason}")]
    StoredSaleInvalid { reason: String },
    #[error("no shift {0} on this register to date a parked cart by")]
    ParkedCartShiftUnknown(pos_domain::ShiftId),
    #[error("cart {0} is already set aside or being worked on")]
    ParkedCartAlreadyParked(pos_domain::SaleId),
    #[error("no parked cart {0} on this register")]
    ParkedCartNotFound(pos_domain::SaleId),
    #[error("parked cart {0} is already being worked on, and cannot be claimed twice")]
    ParkedCartClaimed(pos_domain::SaleId),
    #[error("this session holds no claim on cart {0}")]
    ParkedCartNotClaimed(pos_domain::SaleId),
    /// Only a basket with nothing in it is ever dropped; one with lines is
    /// set aside (the owner's decision of 5 November 2026, 1.4.11).
    #[error("cart {0} still holds lines, and is set aside rather than dropped")]
    ParkedCartNotEmpty(pos_domain::SaleId),
    #[error(
        "parked cart {id} expired at the end of business day {expires_on}, and can no longer \
         be resumed"
    )]
    ParkedCartExpired {
        id: pos_domain::SaleId,
        expires_on: String,
    },
    #[error("resume refused: {0}")]
    ResumeRefused(#[from] pos_domain::CartError),
    #[error("stored parked cart is malformed: {reason}")]
    StoredParkedCartInvalid { reason: String },
    /// A stored setting is not the shape its key promises. The value is never
    /// quoted: a merchant typed it.
    #[error("stored setting {key} is malformed: {reason}")]
    StoredSettingInvalid {
        key: &'static str,
        reason: &'static str,
    },
    #[error("the checkout of sale {0} is already journalled")]
    CheckoutAlreadyJournalled(pos_domain::SaleId),
    #[error(
        "the checkout of sale {0} names an idempotency key another checkout used, and a key is \
         never reused"
    )]
    CheckoutKeyReused(pos_domain::SaleId),
    #[error("no checkout of sale {0} is journalled on this register")]
    CheckoutNotJournalled(pos_domain::SaleId),
    #[error("the checkout of sale {0} is journalled under another attempt's key")]
    CheckoutKeyMismatch(pos_domain::SaleId),
    #[error("the checkout of sale {0} has begun its finalize, which only completes or replays it")]
    CheckoutFinalizing(pos_domain::SaleId),
    #[error("the checkout of sale {0} never began its finalize")]
    CheckoutNotFinalizing(pos_domain::SaleId),
    #[error(
        "the checkout of sale {sale} was advanced to version {found} since this copy was read \
         at version {expected}"
    )]
    CheckoutStale {
        sale: pos_domain::SaleId,
        expected: i64,
        found: i64,
    },
    #[error(
        "the checkout of sale {0} carries a card operation, which this build cannot journal: the \
         terminal is Phase 2's"
    )]
    CheckoutCardOpNotJournalled(pos_domain::SaleId),
    #[error("stored checkout is malformed: {reason}")]
    StoredCheckoutInvalid { reason: String },
}

/// `PRAGMA synchronous = FULL`, as SQLite reports it back. The pragma accepts a
/// name on the way in and answers with a number on the way out.
const SQLITE_SYNCHRONOUS_FULL: i64 = 2;

/// Ordered, forward-only migrations (blueprint §8). `PRAGMA user_version`
/// tracks how many have been applied. Append new files; never edit old ones.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_sale_integrity.sql"),
    include_str!("../migrations/0003_strict_rebuild_and_catalog_depth.sql"),
    include_str!("../migrations/0004_people_and_audit.sql"),
    include_str!("../migrations/0005_sale_columns_and_sequences.sql"),
    include_str!("../migrations/0006_stock_ledger.sql"),
    include_str!("../migrations/0007_search_and_seed.sql"),
    include_str!("../migrations/0008_discount_caps.sql"),
    include_str!("../migrations/0009_sale_capture.sql"),
];

/// The schema version this build understands: the number of migrations it
/// carries, and what `PRAGMA user_version` reads after a successful open.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// The migrations this build carries, in the order the runner applies them.
#[must_use]
pub fn migrations() -> &'static [&'static str] {
    MIGRATIONS
}

/// A schema version a file was checked at: how many migrations it records as
/// applied, and whose schema it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchemaVersion(usize);

impl SchemaVersion {
    /// The number of migrations applied.
    #[must_use]
    pub const fn applied(self) -> usize {
        self.0
    }
}

/// Check that the file's schema is the one its recorded version creates
/// (microstep 1.8.1b, E.58).
///
/// The recorded version is `PRAGMA user_version`, which the runner updates in
/// the same transaction as each migration. The schema expected at that
/// version is derived from `migrations` themselves, replayed in memory with
/// the runner's per-file transactions, and compared object by object: every
/// table, index, trigger and view, by type, name, table and definition.
/// SQLite's own `sqlite_` objects are left out, because the engine creates
/// some on its own, such as `sqlite_stat1` after an `ANALYZE`.
///
/// It never decides which half of an interrupted step committed. A file whose
/// schema is ahead of its version and one whose version is ahead of its schema
/// are refused alike, because business writes against either can make the
/// next migration destroy facts.
///
/// # Errors
///
/// - [`DbError::SchemaShapeMismatch`]: the schema is not the recorded
///   version's, naming every object that differs.
/// - [`DbError::SchemaTooNew`]: a version above `migrations.len()`, written by
///   a newer build.
/// - [`DbError::SchemaVersionInvalid`]: a negative version.
/// - [`DbError::Sqlite`]: the schema cannot be read, or `migrations` cannot be
///   replayed.
pub fn verify_runtime_schema(
    conn: &Connection,
    migrations: &[&str],
) -> Result<SchemaVersion, DbError> {
    verify_against(conn, &expected_shapes(migrations)?, migrations.len())
}

/// Open (or create) an encrypted database and bring it to the latest schema.
///
/// The storage engine is held to [`storage_version::STORAGE_POLICY`] before
/// anything else is set on the connection (1.8.0). Today's build opens in
/// single-connection mode: no second source connection may be opened beside
/// this one until the engine carries the WAL-reset fix (#244).
///
/// The schema is checked by [`verify_runtime_schema`] before the first
/// migration and after each one, and a file whose schema is not its recorded
/// version's is refused untouched, as [`DbError::SchemaShapeMismatch`].
pub fn open(path: &Path, key: &str) -> Result<Connection, DbError> {
    let (conn, schema) = open_checked(
        path,
        key,
        &storage_version::STORAGE_POLICY,
        storage_version::reported_versions,
    )?;
    schema?;
    Ok(conn)
}

/// Open a database to read as evidence, whatever its schema turned out to be.
///
/// Every check [`open`] makes is made, and a file whose schema is its
/// recorded version's is migrated forward as [`open`] would. A file whose
/// schema is not is returned as found, unmigrated, with the
/// [`DbError::SchemaShapeMismatch`] that names the difference beside it. Only
/// the forensic `verify-audit` uses this: a register never runs on such a
/// file, and an investigator must still be able to read one.
///
/// # Errors
///
/// Everything [`open`] refuses except a schema that differs from its version.
pub fn open_for_inspection(
    path: &Path,
    key: &str,
) -> Result<(Connection, Result<SchemaVersion, DbError>), DbError> {
    open_checked(
        path,
        key,
        &storage_version::STORAGE_POLICY,
        storage_version::reported_versions,
    )
}

/// [`open`], with the policy and the version reader as arguments, so a test can
/// put an engine no build here produces to the same open. It returns a
/// schema that differs from its version beside the connection, which [`open`]
/// refuses and [`open_for_inspection`] reports.
pub(crate) fn open_checked(
    path: &Path,
    key: &str,
    policy: &storage_version::StorageVersionPolicy,
    report: fn(&Connection) -> Result<storage_version::ReportedVersions, DbError>,
) -> Result<(Connection, Result<SchemaVersion, DbError>), DbError> {
    let conn = Connection::open(path)?;

    // SQLCipher: the key pragma MUST be the first statement on the connection.
    conn.pragma_update(None, "key", key)?;

    // Touch the schema now so a wrong key fails HERE, loudly, not mid-sale.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|_| DbError::BadKey)?;

    // The engine, before WAL: an unreviewed SQLite or SQLCipher is refused while
    // the file is still in the journal mode it was found in (1.8.0).
    storage_version::storage_verdict(&report(&conn)?, policy)?;

    // Durability + concurrency settings for a register (blueprint appendix: WAL).
    //
    // The returned mode is checked, not discarded. `PRAGMA journal_mode` answers
    // with the mode now in force, and SQLite returns the PREVIOUS one when the
    // transition cannot be made — a read-only directory, or an open connection
    // holding the old mode. Ignoring the answer matters here because the
    // durability guarantee below is mode-dependent: `synchronous = FULL` is
    // last-commit-durable in WAL, and in rollback-journal mode FULL is weaker
    // than the EXTRA that mode would need. Refusing on the wrong mode is what
    // stops `DurabilityRefused` from passing on a connection that is not durable.
    let mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(DbError::JournalModeRefused { found: mode });
    }

    // FULL, not NORMAL. In WAL mode SQLite documents `synchronous = NORMAL` as
    // losing the most recent commits after a power loss — the WAL is not fsynced
    // on every commit, only at checkpoint. On a server that is a good trade. On a
    // register it is the wrong one, because I-9 commits the sale, its stock event,
    // its outbox row and its fiscal-queue row in ONE transaction: losing that
    // commit loses all four together, cleanly and invisibly. There is no
    // half-written state to detect and no alarm to raise. The cashier took the
    // cash, the customer left with a printed receipt, and at Z time the drawer is
    // over with no document to explain it.
    //
    // The cost is an fsync per commit. A register commits a few transactions a
    // minute, so the cost is irrelevant and the failure it prevents is money.
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.busy_timeout(Duration::from_secs(5))?;

    // Assert what we just set, rather than trusting that we set it. A pragma is
    // advisory: an unknown value leaves the old one in place silently, and this
    // one is exactly the setting a future benchmark is tempted to relax. Reading
    // it back turns "someone changed it for a throughput run" into a failed open.
    let synchronous: i64 = conn.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
    if synchronous != SQLITE_SYNCHRONOUS_FULL {
        return Err(DbError::DurabilityRefused { found: synchronous });
    }

    let foreign_keys: bool = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    if !foreign_keys {
        return Err(DbError::ForeignKeysRefused);
    }

    assert_fts5(&conn)?;

    match migrate(&conn) {
        Ok(version) => Ok((conn, Ok(version))),
        Err(mismatch @ DbError::SchemaShapeMismatch { .. }) => Ok((conn, Err(mismatch))),
        Err(error) => Err(error),
    }
}

/// The compile option FTS5 is built in under.
const FTS5_COMPILE_OPTION: &str = "ENABLE_FTS5";

/// Refuse a build whose SQLite cannot do full-text search.
///
/// `rusqlite` has no `fts5` feature flag to depend on: FTS5 arrives through the
/// bundled build, and this project builds SQLCipher, so whether it is present is
/// a property of how the C library was configured rather than of anything Cargo
/// can express. Verify rather than hope.
///
/// **Why this fails the open rather than the search.** Without FTS5 a register
/// starts perfectly, sells perfectly, and returns an empty result for every
/// catalogue search — a cashier reads that as "we do not stock it" and sells
/// nothing, or rings it up by hand at the wrong price. A missing feature that
/// degrades into a plausible-looking wrong answer is worse than one that
/// refuses, so this refuses.
///
/// It runs before [`migrate`] deliberately: `0007` creates the FTS tables, and
/// a migration is a worse place to discover this than an open.
fn assert_fts5(conn: &Connection) -> Result<(), DbError> {
    let present: i64 = conn.query_row(
        "SELECT count(*) FROM pragma_compile_options WHERE compile_options = ?1",
        [FTS5_COMPILE_OPTION],
        |row| row.get(0),
    )?;
    fts5_verdict(present)
}

/// The decision, separated from the query that feeds it.
///
/// Splitting it is what makes the refusal path testable at all: every
/// connection this build can open reports FTS5, so a test that could only go
/// through [`assert_fts5`] would exercise the success branch and nothing else —
/// and a guard nobody has seen refuse is a guard nobody should trust.
fn fts5_verdict(compile_options_found: i64) -> Result<(), DbError> {
    if compile_options_found == 0 {
        return Err(DbError::MissingFeature(FTS5_COMPILE_OPTION));
    }
    Ok(())
}

/// Bring the file to [`SCHEMA_VERSION`], checking its schema before the first
/// migration and after every one (1.8.1b).
///
/// A file that fails the first check is left exactly as it was found: no
/// migration runs on a schema whose version it does not hold.
fn migrate(conn: &Connection) -> Result<SchemaVersion, DbError> {
    let shapes = expected_shapes(MIGRATIONS)?;
    let mut version = verify_against(conn, &shapes, MIGRATIONS.len())?;

    for (idx, sql) in MIGRATIONS.iter().enumerate().skip(version.applied()) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (idx + 1) as i64)?;
        tx.commit()?;
        version = verify_against(conn, &shapes, MIGRATIONS.len())?;
    }
    Ok(version)
}

/// One object of a schema, as `sqlite_master` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SchemaObject {
    kind: String,
    name: String,
    table: String,
    sql: Option<String>,
}

impl SchemaObject {
    /// How a refusal names it: `trigger audit_log_no_update`.
    fn label(&self) -> String {
        format!("{} {}", self.kind, self.name)
    }
}

/// The objects of one schema, keyed by type and name.
type Shape = BTreeMap<(String, String), SchemaObject>;

/// The shapes of this build's own chain, derived once per process.
static RUNTIME_SHAPES: OnceLock<Vec<Shape>> = OnceLock::new();

/// The schema each version of `migrations` creates, from no migration to all
/// of them: element `n` is the shape after the first `n`.
fn expected_shapes(migrations: &[&str]) -> Result<Cow<'static, [Shape]>, DbError> {
    if std::ptr::eq(migrations, MIGRATIONS) {
        if let Some(shapes) = RUNTIME_SHAPES.get() {
            return Ok(Cow::Borrowed(shapes));
        }
        let shapes = replay(migrations)?;
        return Ok(Cow::Borrowed(RUNTIME_SHAPES.get_or_init(|| shapes)));
    }
    replay(migrations).map(Cow::Owned)
}

/// Apply `migrations` to an empty in-memory database the way the runner
/// does, one transaction per file, and read the schema after each.
fn replay(migrations: &[&str]) -> Result<Vec<Shape>, DbError> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let mut shapes = Vec::with_capacity(migrations.len() + 1);
    shapes.push(shape_of(&conn)?);
    for sql in migrations {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.commit()?;
        shapes.push(shape_of(&conn)?);
    }
    Ok(shapes)
}

/// The main schema's objects, leaving out SQLite's own.
fn shape_of(conn: &Connection) -> Result<Shape, DbError> {
    let mut statement = conn.prepare(
        "SELECT type, name, tbl_name, sql FROM main.sqlite_master
          WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
    )?;
    let objects = statement.query_map([], |row| {
        Ok(SchemaObject {
            kind: row.get(0)?,
            name: row.get(1)?,
            table: row.get(2)?,
            sql: row.get(3)?,
        })
    })?;
    let mut shape = Shape::new();
    for object in objects {
        let object = object?;
        shape.insert((object.kind.clone(), object.name.clone()), object);
    }
    Ok(shape)
}

/// The version the file records, held to the shape `shapes` says it creates.
fn verify_against(
    conn: &Connection,
    shapes: &[Shape],
    supported: usize,
) -> Result<SchemaVersion, DbError> {
    let found: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;

    // Conventions §9.5 (E.58): refuse a database this build cannot account for,
    // and say why. A counter above `supported` was written by a newer build whose
    // schema this code does not know how to read; a negative one is corruption.
    // `try_from` rather than `as usize`, which would wrap -1 to a huge number and
    // silently skip every migration.
    let applied = usize::try_from(found).map_err(|_| DbError::SchemaVersionInvalid { found })?;
    // `shapes` runs from no migration to all of them, so a version past its
    // end was written by a newer build.
    let Some(expected) = shapes.get(applied) else {
        return Err(DbError::SchemaTooNew { found, supported });
    };

    let actual = shape_of(conn)?;
    let missing: Vec<String> = expected
        .iter()
        .filter(|(key, _)| !actual.contains_key(*key))
        .map(|(_, object)| object.label())
        .collect();
    let unexpected: Vec<String> = actual
        .iter()
        .filter(|(key, _)| !expected.contains_key(*key))
        .map(|(_, object)| object.label())
        .collect();
    let altered: Vec<String> = expected
        .iter()
        .filter(|(key, object)| actual.get(*key).is_some_and(|found| found != *object))
        .map(|(_, object)| object.label())
        .collect();
    if missing.is_empty() && unexpected.is_empty() && altered.is_empty() {
        Ok(SchemaVersion(applied))
    } else {
        Err(DbError::SchemaShapeMismatch {
            recorded: found,
            missing,
            unexpected,
            altered,
        })
    }
}

/// A refusal's summary of a schema difference: each list's size and its
/// first object.
fn shape_difference(missing: &[String], unexpected: &[String], altered: &[String]) -> String {
    [
        ("missing", missing),
        ("unexpected", unexpected),
        ("altered", altered),
    ]
    .iter()
    .filter_map(|(what, objects)| {
        objects.first().map(|first| match objects.len() {
            1 => format!("{what}: {first}"),
            n => format!("{n} {what}, first {first}"),
        })
    })
    .collect::<Vec<_>>()
    .join("; ")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::{DbError, FTS5_COMPILE_OPTION, assert_fts5, fts5_verdict};

    /// Microstep 1.2.6. Two halves, because either alone is worth little.
    ///
    /// The refusal path cannot be reached through a real connection — every
    /// database this build opens reports FTS5 — so it is proved through the
    /// decision function, and the query that feeds it is proved separately by
    /// actually using the feature.
    ///
    /// **What this test cannot prove, stated rather than implied:** that
    /// [`crate::open`] still *calls* [`assert_fts5`]. Deleting that one line
    /// leaves every assertion below green, because on a build that has FTS5 an
    /// open that checks and an open that does not are indistinguishable. Only a
    /// build without FTS5 could tell them apart, and this workspace cannot
    /// produce one — `rusqlite` has no `fts5` feature to turn off, which is the
    /// same fact that makes the assertion necessary. The call site is therefore
    /// a reviewed one-liner, not a tested one.
    #[test]
    fn open_asserts_fts5_available() {
        // 1 · The refusal. A build without FTS5 must fail with a named error
        // rather than open and return an empty result for every search.
        let refused = fts5_verdict(0).expect_err("a build with no FTS5 must be refused at open");
        assert!(
            matches!(refused, DbError::MissingFeature(FTS5_COMPILE_OPTION)),
            "expected MissingFeature(\"{FTS5_COMPILE_OPTION}\"), found {refused:?}"
        );
        assert!(
            refused.to_string().contains("empty result"),
            "the message must say what the failure would otherwise look like to a \
             cashier, not merely name the missing option: {refused}"
        );
        assert!(fts5_verdict(1).is_ok());

        // 2 · The query, and the feature behind it. A compile-options string is
        // evidence that the library was configured for FTS5, not that FTS5
        // works — so this builds one and searches it. A `temp` table lives on
        // the connection and never reaches the register's schema.
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open(&dir.path().join("fts5.db"), "test-key")
            .expect("this build must carry FTS5, which is what open now asserts");
        assert_fts5(&conn).expect("the assertion open just made must still hold");

        conn.execute_batch(
            "CREATE VIRTUAL TABLE temp.probe USING fts5(name);
             INSERT INTO temp.probe (name) VALUES ('قهوة عربية'), ('Espresso');",
        )
        .expect("ENABLE_FTS5 in pragma_compile_options must mean a usable fts5 module");
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM temp.probe WHERE probe MATCH ?1",
                ["Espresso"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            hits, 1,
            "a compiled-in FTS5 that matches nothing is not FTS5"
        );
    }
}
