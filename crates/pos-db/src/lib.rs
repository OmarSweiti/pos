//! Encrypted local store for the register.
//! Owns: connection setup, key handling, migrations. (Blueprint §2, §7)

use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

pub mod key;

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

    migrate(&conn)?;
    Ok(conn)
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
