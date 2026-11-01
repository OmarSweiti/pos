//! The storage engine's own versions, checked at every open before anything
//! else is set on the connection (microstep 1.8.0).
//!
//! # Why a register checks its engine
//!
//! SQLite's WAL-reset bug corrupts a database when two connections to the same
//! file write or checkpoint at the same instant in WAL mode. It is fixed in
//! SQLite `3.51.3`, and SQLCipher `4.14.0` is the first release found to carry
//! the fix. This build compiles SQLite `3.50.4` under SQLCipher `4.10.0`, which
//! does not carry it, and the bump that would is #244. Everything else about a
//! register's durability holds only if that corruption cannot happen, so the
//! versions are read from the engine itself at every open, not assumed from
//! `Cargo.lock`.
//!
//! # Two tiers
//!
//! Read literally, refusing every build below the fix would refuse today's at
//! every open. So the policy carries two floors:
//!
//! - **A floor to open at all**: the build this repository has reviewed, SQLite
//!   `3.50.4` and SQLCipher `4.10.0`. Anything older was never reviewed, and is
//!   refused before WAL is enabled.
//! - **A floor for a second source connection**: the fix. Below it the gate says
//!   [`ConnectionMode::Single`], and no backup, reporting, checkpoint or sync
//!   worker may open the register's database beside the one connection that
//!   writes it. Today's build is below it.
//!
//! The minimums are not opinions: [`STORAGE_POLICY`] names the passage of
//! `ref/plan-validation.md` §5 that records where each came from, and pins its
//! BLAKE3 digest, so the passage cannot change without this policy being read
//! again.

use std::fmt;

use rusqlite::{Connection, OptionalExtension};

use crate::DbError;

/// A storage engine's version: three numbers, ordered as versions are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    /// The version an engine reports, read strictly: three numbers separated by
    /// dots, then at most one word naming an edition, as SQLCipher writes
    /// `"4.10.0 community"`. Anything else is `None`, so a report this build does
    /// not understand is refused rather than read as some version it is not.
    #[must_use]
    pub fn parse(reported: &str) -> Option<Version> {
        let mut words = reported.split(' ');
        let number = words.next()?;
        if let Some(edition) = words.next()
            && (edition.is_empty() || !edition.chars().all(|c| c.is_ascii_alphanumeric()))
        {
            return None;
        }
        if words.next().is_some() {
            return None;
        }
        let mut parts = number.split('.');
        let mut part = || -> Option<u32> {
            let digits = parts.next()?;
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            digits.parse().ok()
        };
        let version = Version::new(part()?, part()?, part()?);
        parts.next().is_none().then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What a build may do with the engine it finds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageVersionPolicy {
    /// The oldest SQLCipher a register opens on at all.
    pub min_cipher_version: Version,
    /// The oldest SQLite a register opens on at all.
    pub min_sqlite_version: Version,
    /// The oldest SQLCipher that may serve a second source connection.
    pub concurrent_cipher_version: Version,
    /// The oldest SQLite that may serve a second source connection.
    pub concurrent_sqlite_version: Version,
    /// Where each minimum came from.
    pub source: &'static str,
    /// BLAKE3 of that passage, so it cannot change unread.
    pub source_hash: [u8; 32],
}

/// The reviewed policy (`ref/plan-validation.md` §5, #234).
pub const STORAGE_POLICY: StorageVersionPolicy = StorageVersionPolicy {
    min_cipher_version: Version::new(4, 10, 0),
    min_sqlite_version: Version::new(3, 50, 4),
    concurrent_cipher_version: Version::new(4, 14, 0),
    concurrent_sqlite_version: Version::new(3, 51, 3),
    source: "docs/implementation/ref/plan-validation.md §5, the passage \"ANSWERED on 24 September \
             2026 (#234)\": the compiled build read at runtime (SQLite 3.50.4, SQLCipher 4.10.0 \
             community), https://sqlite.org/wal.html §11 (the WAL-reset bug, fixed in 3.51.3), and \
             https://www.zetetic.net/blog/2026/03/17/sqlcipher-4.14.0-release/ (SQLCipher 4.14.0, \
             the first release on 3.51.3)",
    source_hash: SOURCE_HASH,
};

/// BLAKE3 of the passage [`STORAGE_POLICY`] names, as `source_passage` reads it.
const SOURCE_HASH: [u8; 32] = [
    0xaf, 0xe5, 0xaf, 0x8d, 0x8a, 0x13, 0xb0, 0xa3, 0x37, 0x49, 0xa0, 0x42, 0xd3, 0x4b, 0x1b, 0x86,
    0x9d, 0xaf, 0xec, 0x0c, 0xc2, 0x3c, 0x19, 0xac, 0x85, 0x49, 0x5f, 0xa9, 0xca, 0x39, 0x5e, 0x02,
];

/// What the engine says it is, exactly as it said it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedVersions {
    /// `PRAGMA cipher_version`.
    pub cipher: String,
    /// `sqlite_version()`.
    pub sqlite: String,
}

/// Both versions, read and understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageVersions {
    pub cipher: Version,
    pub sqlite: Version,
    /// What the engine reported, for diagnostics.
    pub reported: ReportedVersions,
}

/// How many source connections the register's database may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionMode {
    /// One connection, which writes; no worker opens another beside it.
    Single,
    /// The engine carries the WAL-reset fix, so a worker may.
    Concurrent,
}

/// The engine this connection runs on, and what that allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageGate {
    pub versions: StorageVersions,
    pub mode: ConnectionMode,
}

impl StorageGate {
    /// Whether a second source connection may be opened on the database.
    #[must_use]
    pub fn allows_a_second_connection(&self) -> bool {
        self.mode == ConnectionMode::Concurrent
    }
}

/// What the engine behind `conn` reports. A build with no SQLCipher answers
/// `PRAGMA cipher_version` with no row, which reads as an empty report.
///
/// # Errors
///
/// [`DbError::Sqlite`] when either query fails.
pub fn reported_versions(conn: &Connection) -> Result<ReportedVersions, DbError> {
    let cipher: Option<String> = conn
        .query_row("PRAGMA cipher_version", [], |row| row.get(0))
        .optional()?;
    let sqlite: String = conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    Ok(ReportedVersions {
        cipher: cipher.unwrap_or_default(),
        sqlite,
    })
}

/// Read the engine behind `conn` and hold it to `policy`.
///
/// # Errors
///
/// As [`storage_verdict`], and [`DbError::Sqlite`] when the engine cannot be
/// asked.
pub fn assert_storage_versions(
    conn: &Connection,
    policy: &StorageVersionPolicy,
) -> Result<StorageGate, DbError> {
    storage_verdict(&reported_versions(conn)?, policy)
}

/// The decision, separated from the queries that feed it, so a version no
/// build in this repository can produce can still be put to it.
///
/// # Errors
///
/// - [`DbError::StorageVersionUnparseable`]: a report this build cannot read.
/// - [`DbError::UnsupportedStorageVersion`]: an engine older than the floor to
///   open at all.
pub fn storage_verdict(
    reported: &ReportedVersions,
    policy: &StorageVersionPolicy,
) -> Result<StorageGate, DbError> {
    let cipher = read("SQLCipher", &reported.cipher)?;
    let sqlite = read("SQLite", &reported.sqlite)?;
    at_least("SQLCipher", cipher, policy.min_cipher_version)?;
    at_least("SQLite", sqlite, policy.min_sqlite_version)?;
    let fixed =
        cipher >= policy.concurrent_cipher_version && sqlite >= policy.concurrent_sqlite_version;
    Ok(StorageGate {
        versions: StorageVersions {
            cipher,
            sqlite,
            reported: reported.clone(),
        },
        mode: if fixed {
            ConnectionMode::Concurrent
        } else {
            ConnectionMode::Single
        },
    })
}

fn read(engine: &'static str, reported: &str) -> Result<Version, DbError> {
    Version::parse(reported).ok_or_else(|| DbError::StorageVersionUnparseable {
        engine,
        found: reported.to_owned(),
    })
}

fn at_least(engine: &'static str, found: Version, minimum: Version) -> Result<(), DbError> {
    if found < minimum {
        return Err(DbError::UnsupportedStorageVersion {
            engine,
            found: found.to_string(),
            minimum: minimum.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::open_checked;

    const KEY: &str = "storage-version-test-key";

    /// The build `Cargo.lock` pins, as #234 read it at runtime.
    fn reviewed() -> ReportedVersions {
        ReportedVersions {
            cipher: "4.10.0 community".to_owned(),
            sqlite: "3.50.4".to_owned(),
        }
    }

    /// The passage of `ref/plan-validation.md` §5 the policy names: the quoted
    /// block that opens with its heading, to the first line that is not quoted.
    fn source_passage() -> String {
        let document = include_str!("../../../docs/implementation/ref/plan-validation.md");
        let heading = "> ✅ **ANSWERED on 24 September 2026 (#234)";
        document
            .lines()
            .skip_while(|line| !line.starts_with(heading))
            .take_while(|line| line.starts_with('>'))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn open_reports_compiled_storage_versions() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open(&dir.path().join("register.db"), KEY).unwrap();
        let gate = assert_storage_versions(&conn, &STORAGE_POLICY).unwrap();
        assert_eq!(gate.versions.reported, reviewed());
        assert_eq!(
            (gate.versions.cipher, gate.versions.sqlite),
            (Version::new(4, 10, 0), Version::new(3, 50, 4))
        );
        assert_eq!(gate.mode, ConnectionMode::Single);
    }

    #[test]
    fn storage_minimum_policy_has_source_and_hash() {
        let passage = source_passage();
        assert!(passage.len() > 1_000, "the passage was not found");
        assert_eq!(
            *blake3::hash(passage.as_bytes()).as_bytes(),
            STORAGE_POLICY.source_hash,
            "ref/plan-validation.md §5 changed: read it, then re-pin the policy"
        );
        for minimum in [
            STORAGE_POLICY.min_cipher_version,
            STORAGE_POLICY.min_sqlite_version,
            STORAGE_POLICY.concurrent_cipher_version,
            STORAGE_POLICY.concurrent_sqlite_version,
        ] {
            let written = minimum.to_string();
            assert!(passage.contains(&format!("`{written}")), "{written}");
            assert!(STORAGE_POLICY.source.contains(&written), "{written}");
        }
        for url in [
            "https://sqlite.org/wal.html",
            "https://www.zetetic.net/blog/2026/03/17/sqlcipher-4.14.0-release/",
        ] {
            assert!(
                passage.contains(url) && STORAGE_POLICY.source.contains(url),
                "{url}"
            );
        }
        assert!(STORAGE_POLICY.min_cipher_version <= STORAGE_POLICY.concurrent_cipher_version);
        assert!(STORAGE_POLICY.min_sqlite_version <= STORAGE_POLICY.concurrent_sqlite_version);
    }

    /// What a refused open leaves behind: a database never put in WAL mode.
    fn assert_never_in_wal(path: &std::path::Path) {
        let wal = path.with_file_name("register.db-wal");
        assert!(!wal.exists(), "a WAL file was created");
        if path.exists() {
            let raw = Connection::open(path).unwrap();
            raw.pragma_update(None, "key", KEY).unwrap();
            let mode: String = raw
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            assert_ne!(mode.to_ascii_lowercase(), "wal");
        }
    }

    fn opened_with(reporter: fn(&Connection) -> Result<ReportedVersions, DbError>) -> DbError {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("register.db");
        let refused = open_checked(&path, KEY, &STORAGE_POLICY, reporter).unwrap_err();
        assert_never_in_wal(&path);
        refused
    }

    #[test]
    fn open_refuses_cipher_below_the_reviewed_minimum() {
        let refused = opened_with(|_| {
            Ok(ReportedVersions {
                cipher: "4.9.9 community".to_owned(),
                sqlite: "3.50.4".to_owned(),
            })
        });
        assert!(matches!(
            refused,
            DbError::UnsupportedStorageVersion { engine: "SQLCipher", ref found, ref minimum }
                if found == "4.9.9" && minimum == "4.10.0"
        ));
        let at_the_floor = storage_verdict(&reviewed(), &STORAGE_POLICY);
        assert!(at_the_floor.is_ok());
    }

    #[test]
    fn open_refuses_sqlite_below_the_reviewed_minimum() {
        let refused = opened_with(|_| {
            Ok(ReportedVersions {
                cipher: "4.10.0 community".to_owned(),
                sqlite: "3.50.3".to_owned(),
            })
        });
        assert!(matches!(
            refused,
            DbError::UnsupportedStorageVersion { engine: "SQLite", ref found, ref minimum }
                if found == "3.50.3" && minimum == "3.50.4"
        ));
        // A higher number in a lower place does not make a version newer.
        let older_minor = ReportedVersions {
            cipher: "4.10.0".to_owned(),
            sqlite: "3.49.99".to_owned(),
        };
        assert!(storage_verdict(&older_minor, &STORAGE_POLICY).is_err());
    }

    #[test]
    fn open_refuses_an_unparseable_storage_version() {
        let refused = opened_with(|_| {
            Ok(ReportedVersions {
                cipher: String::new(),
                sqlite: "3.50.4".to_owned(),
            })
        });
        assert!(matches!(
            refused,
            DbError::StorageVersionUnparseable { engine: "SQLCipher", ref found } if found.is_empty()
        ));
        let refused = opened_with(|_| {
            Ok(ReportedVersions {
                cipher: "4.10.0 community".to_owned(),
                sqlite: "three point fifty".to_owned(),
            })
        });
        assert!(matches!(
            refused,
            DbError::StorageVersionUnparseable {
                engine: "SQLite",
                ..
            }
        ));
        for unreadable in [
            "",
            "4",
            "4.10",
            "4.10.0.1",
            "4.10.x",
            "v4.10.0",
            "4.10.0  community",
            "4.10.0 ",
            "4.10.0 community edition",
            "4.10.0 com-munity",
            "-4.10.0",
            "4.+10.0",
            "99999999999.0.0",
        ] {
            assert_eq!(Version::parse(unreadable), None, "{unreadable:?}");
        }
        assert_eq!(Version::parse("3.50.4"), Some(Version::new(3, 50, 4)));
        assert_eq!(
            Version::parse("4.10.0 community"),
            Some(Version::new(4, 10, 0))
        );
    }

    #[test]
    fn multi_connection_wal_is_disabled_until_the_storage_gate_closes() {
        let today = storage_verdict(&reviewed(), &STORAGE_POLICY).unwrap();
        assert!(!today.allows_a_second_connection());
        let fixed = ReportedVersions {
            cipher: "4.14.0 community".to_owned(),
            sqlite: "3.51.3".to_owned(),
        };
        assert!(
            storage_verdict(&fixed, &STORAGE_POLICY)
                .unwrap()
                .allows_a_second_connection()
        );
        // Both halves must carry the fix: one without it keeps the gate shut.
        for half in [
            ReportedVersions {
                cipher: "4.13.9 community".to_owned(),
                sqlite: "3.51.3".to_owned(),
            },
            ReportedVersions {
                cipher: "4.14.0 community".to_owned(),
                sqlite: "3.51.2".to_owned(),
            },
        ] {
            let gate = storage_verdict(&half, &STORAGE_POLICY).unwrap();
            assert_eq!(gate.mode, ConnectionMode::Single, "{half:?}");
        }
    }
}
