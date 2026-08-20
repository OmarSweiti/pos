//! The register must refuse a database it cannot account for, rather than
//! selling against a schema it does not understand (conventions §9.5, E.58).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

/// Open a fresh database, then force `user_version` to `version`.
fn database_reporting_version(dir: &tempfile::TempDir, version: i64) -> PathBuf {
    let path = dir.path().join(format!("v{version}.db"));
    let conn = pos_db::open(&path, "test-key").unwrap();
    conn.pragma_update(None, "user_version", version).unwrap();
    path
}

#[test]
fn schema_from_a_newer_build_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = database_reporting_version(&dir, 99);

    match pos_db::open(&path, "test-key") {
        Err(pos_db::DbError::SchemaTooNew { found, supported }) => {
            assert_eq!(found, 99);
            assert!(supported < 99, "supported = {supported}");
        }
        other => panic!("expected SchemaTooNew, got {other:?}"),
    }
}

#[test]
fn negative_schema_version_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = database_reporting_version(&dir, -1);

    match pos_db::open(&path, "test-key") {
        Err(pos_db::DbError::SchemaVersionInvalid { found }) => assert_eq!(found, -1),
        other => panic!("expected SchemaVersionInvalid, got {other:?}"),
    }
}

#[test]
fn current_schema_reopens_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("current.db");
    drop(pos_db::open(&path, "test-key").unwrap());

    // Reopening at the version this build wrote must remain a no-op, not an error:
    // the guard must not fire on the ordinary path.
    pos_db::open(&path, "test-key").expect("reopening at the current version must work");
}
