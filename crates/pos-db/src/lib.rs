//! Encrypted local store for the register.
//! Owns: connection setup, key handling, migrations. (Blueprint §2, §7)

use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

pub mod key;
pub mod repo;

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
];

/// The schema version this build understands: the number of migrations it
/// carries, and what `PRAGMA user_version` reads after a successful open.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Open (or create) an encrypted database and bring it to the latest schema.
pub fn open(path: &Path, key: &str) -> Result<Connection, DbError> {
    let conn = Connection::open(path)?;

    // SQLCipher: the key pragma MUST be the first statement on the connection.
    conn.pragma_update(None, "key", key)?;

    // Touch the schema now so a wrong key fails HERE, loudly, not mid-sale.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|_| DbError::BadKey)?;

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

    migrate(&conn)?;
    Ok(conn)
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

fn migrate(conn: &Connection) -> Result<(), DbError> {
    let found: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let supported = MIGRATIONS.len();

    // Conventions §9.5 (E.58): refuse a database this build cannot account for,
    // and say why. A counter above `supported` was written by a newer build whose
    // schema this code does not know how to read; a negative one is corruption.
    // `try_from` rather than `as usize`, which would wrap -1 to a huge number and
    // silently skip every migration.
    let applied = usize::try_from(found).map_err(|_| DbError::SchemaVersionInvalid { found })?;
    if applied > supported {
        return Err(DbError::SchemaTooNew { found, supported });
    }

    for (idx, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (idx + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
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
