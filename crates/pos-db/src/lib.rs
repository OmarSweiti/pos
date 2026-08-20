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
    #[error(
        "database schema is version {found}, but this build only knows {supported}; \
         it was written by a newer version of the application"
    )]
    SchemaTooNew { found: i64, supported: usize },
    #[error("database reports an impossible schema version ({found})")]
    SchemaVersionInvalid { found: i64 },
}

/// Ordered, forward-only migrations (blueprint §8). `PRAGMA user_version`
/// tracks how many have been applied. Append new files; never edit old ones.
const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_init.sql")];

/// Open (or create) an encrypted database and bring it to the latest schema.
pub fn open(path: &Path, key: &str) -> Result<Connection, DbError> {
    let conn = Connection::open(path)?;

    // SQLCipher: the key pragma MUST be the first statement on the connection.
    conn.pragma_update(None, "key", key)?;

    // Touch the schema now so a wrong key fails HERE, loudly, not mid-sale.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|_| DbError::BadKey)?;

    // Durability + concurrency settings for a register (blueprint appendix: WAL).
    let _mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.busy_timeout(Duration::from_secs(5))?;

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
