//! Microstep 1.8.1b — a file whose schema and recorded version disagree is
//! refused, by name, and left as it was found (E.58).
//!
//! Every fixture is built the way the runner builds a register: the exact
//! `MIGRATIONS` array the application compiles with, one transaction per file,
//! `user_version` recorded inside it. Each one is then interrupted on one side
//! of that update, at every step of the chain.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pos_db::{DbError, SCHEMA_VERSION, migrations, verify_runtime_schema};
use rusqlite::fallible_iterator::FallibleIterator;
use rusqlite::{Batch, Connection};

const KEY: &str = "test-key";

/// The steps that change rows alone, numbered as their files are. They create,
/// alter and drop nothing, so no schema can show whether they ran: `0008`
/// writes the discount caps. Rerunning it changes nothing, by its own `WHERE`
/// clauses, and a file recording it without its rows is beyond a shape to
/// see. Listed so a new one is a decision rather than a silent gap.
const ROWS_ONLY: &[usize] = &[8];

/// A keyed connection to `path`, opened the way SQLCipher needs and nothing
/// more: no migration, no check.
fn raw(path: &Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.pragma_update(None, "key", KEY).unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    conn
}

/// A file with the first `steps` migrations applied, each with its version,
/// exactly as the runner applies them.
fn built(path: &Path, steps: usize) -> Connection {
    let conn = raw(path);
    for (index, sql) in migrations().iter().enumerate().take(steps) {
        let tx = conn.unchecked_transaction().unwrap();
        tx.execute_batch(sql).unwrap();
        tx.pragma_update(None, "user_version", version(index + 1))
            .unwrap();
        tx.commit().unwrap();
    }
    conn
}

/// A count of migrations as `user_version` holds it.
fn version(applied: usize) -> i64 {
    i64::try_from(applied).unwrap()
}

fn migration(index: usize) -> &'static str {
    migrations().get(index).copied().unwrap()
}

fn user_version(conn: &Connection) -> i64 {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

/// Every object of the main schema, as written.
fn schema(conn: &Connection) -> Vec<(String, String, Option<String>)> {
    let mut statement = conn
        .prepare("SELECT type, name, sql FROM main.sqlite_master ORDER BY type, name")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// What a refusal named.
#[derive(Debug, PartialEq, Eq)]
struct Named {
    recorded: i64,
    missing: Vec<String>,
    unexpected: Vec<String>,
    altered: Vec<String>,
}

/// Open `path` as a register does, require the named refusal, and require
/// the file to be exactly as it was before the attempt.
fn refused(path: &Path) -> Named {
    let (version_before, schema_before) = {
        let conn = raw(path);
        (user_version(&conn), schema(&conn))
    };
    let error = pos_db::open(path, KEY).expect_err("a register must not open this file");
    let DbError::SchemaShapeMismatch {
        recorded,
        missing,
        unexpected,
        altered,
    } = &error
    else {
        panic!("expected SchemaShapeMismatch, found {error:?}");
    };
    assert!(
        error
            .to_string()
            .contains("Restore the last verified backup"),
        "the refusal names its recovery: {error}"
    );
    let conn = raw(path);
    assert_eq!(user_version(&conn), version_before, "no version was moved");
    assert_eq!(schema(&conn), schema_before, "no migration ran");
    // The public check says the same, on its own.
    assert!(matches!(
        verify_runtime_schema(&conn, migrations()),
        Err(DbError::SchemaShapeMismatch { .. })
    ));
    Named {
        recorded: *recorded,
        missing: missing.clone(),
        unexpected: unexpected.clone(),
        altered: altered.clone(),
    }
}

/// At every step of the chain, a file interrupted on either side of the
/// `user_version` update, or part-way through the step itself, is refused by
/// name and left untouched. The two sides name the same objects from opposite
/// ends, which is what shows the refusal reads the step and is not guessing.
#[test]
fn half_migrated_db_refuses_to_open_with_a_named_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut refused_steps = 0;
    for step in 0..migrations().len() {
        let number = step + 1;
        let sql = migration(step);

        // The step committed; its version did not.
        let ahead = dir.path().join(format!("{number}-schema-ahead.db"));
        {
            let conn = built(&ahead, step);
            let tx = conn.unchecked_transaction().unwrap();
            tx.execute_batch(sql).unwrap();
            tx.commit().unwrap();
        }
        // The version committed; its step did not.
        let behind = dir.path().join(format!("{number}-version-ahead.db"));
        {
            let conn = built(&behind, step);
            conn.pragma_update(None, "user_version", version(number))
                .unwrap();
        }

        if ROWS_ONLY.contains(&number) {
            // No schema shows a step that writes rows alone, so both open.
            assert_eq!(
                schema(&raw(&ahead)),
                schema(&built(
                    &dir.path().join(format!("{number}-before.db")),
                    step
                )),
                "{number:04} is listed as changing rows alone, and it changes the schema"
            );
            pos_db::open(&ahead, KEY).expect("a rows-only step rerun changes nothing");
            pos_db::open(&behind, KEY).expect("a rows-only step has no shape to miss");
            continue;
        }

        let schema_ahead = refused(&ahead);
        let version_ahead = refused(&behind);
        let index = i64::try_from(step).unwrap();
        assert_eq!(schema_ahead.recorded, index, "{number:04}");
        assert_eq!(version_ahead.recorded, index + 1, "{number:04}");
        assert_eq!(
            (
                &schema_ahead.unexpected,
                &schema_ahead.missing,
                &schema_ahead.altered
            ),
            (
                &version_ahead.missing,
                &version_ahead.unexpected,
                &version_ahead.altered
            ),
            "{number:04}: the two sides of one step name the same objects"
        );
        assert!(
            !(version_ahead.missing.is_empty() && version_ahead.altered.is_empty()),
            "{number:04} creates or alters something, so a refusal names it"
        );

        // Part of the step committed, and neither version is its schema.
        let statements = {
            // A statement can name what an earlier one creates, so the step is
            // counted by running it, on a copy of the chain in memory.
            let conn = Connection::open_in_memory().unwrap();
            for earlier in migrations().iter().take(step) {
                conn.execute_batch(earlier).unwrap();
            }
            let mut batch = Batch::new(&conn, sql);
            let mut count = 0_usize;
            while let Some(mut statement) = batch.next().unwrap() {
                statement.raw_execute().unwrap();
                count += 1;
            }
            count
        };
        assert!(
            statements >= 2,
            "{number:04} has {statements} statement, so it cannot stop part-way"
        );
        let partial = dir.path().join(format!("{number}-partial.db"));
        {
            let conn = built(&partial, step);
            let tx = conn.unchecked_transaction().unwrap();
            {
                let mut batch = Batch::new(&tx, sql);
                for _ in 0..statements / 2 {
                    let mut statement = batch.next().unwrap().unwrap();
                    statement.raw_execute().unwrap();
                }
            }
            tx.commit().unwrap();
        }
        assert_eq!(refused(&partial).recorded, index, "{number:04}");
        raw(&partial)
            .pragma_update(None, "user_version", version(number))
            .unwrap();
        assert_eq!(refused(&partial).recorded, index + 1, "{number:04}");
        refused_steps += 1;
    }
    assert_eq!(
        refused_steps + ROWS_ONLY.len(),
        migrations().len(),
        "every step was interrupted"
    );
}

/// A file whose version is above this build's chain was written by a newer
/// build: refused by name, before any check of its shape, and left as it was.
/// A negative version is no version at all.
#[test]
fn schema_from_a_newer_build_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("newer-build.db");
    drop(pos_db::open(&path, KEY).expect("this build's own file opens"));
    {
        // What the newer build did: one more migration, and its version.
        let conn = raw(&path);
        conn.execute_batch("CREATE TABLE added_by_a_newer_build (id BLOB PRIMARY KEY) STRICT;")
            .unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
    }
    let before = schema(&raw(&path));

    let error = pos_db::open(&path, KEY).expect_err("an older build must not open it");
    assert!(
        matches!(error, DbError::SchemaTooNew { found, supported }
            if found == SCHEMA_VERSION + 1 && supported == migrations().len()),
        "{error:?}"
    );
    assert!(error.to_string().contains("newer version"), "{error}");
    let conn = raw(&path);
    assert_eq!(user_version(&conn), SCHEMA_VERSION + 1);
    assert_eq!(
        schema(&conn),
        before,
        "nothing of the newer build's was touched"
    );
    assert!(matches!(
        verify_runtime_schema(&conn, migrations()),
        Err(DbError::SchemaTooNew { .. })
    ));

    conn.pragma_update(None, "user_version", -1).unwrap();
    assert!(matches!(
        pos_db::open(&path, KEY).expect_err("a negative version is refused"),
        DbError::SchemaVersionInvalid { found: -1 }
    ));
}

/// The check is not in the way of an ordinary upgrade: a file stopped cleanly
/// at any version opens, migrates forward through the check, and ends at this
/// build's version, holding the schema a new file holds.
#[test]
fn a_file_at_every_earlier_version_migrates_through_the_check() {
    let dir = tempfile::tempdir().unwrap();
    let fresh = dir.path().join("fresh.db");
    let new_file = pos_db::open(&fresh, KEY).unwrap();
    let full = schema(&new_file);
    assert_eq!(
        verify_runtime_schema(&new_file, migrations())
            .unwrap()
            .applied(),
        migrations().len()
    );
    // The engine makes objects of its own, which no migration creates: an
    // `ANALYZE` writes `sqlite_stat1`. They are not the schema's difference.
    new_file.execute_batch("ANALYZE;").unwrap();
    assert!(
        schema(&new_file)
            .iter()
            .any(|(_, name, _)| name == "sqlite_stat1")
    );
    drop(new_file);
    drop(pos_db::open(&fresh, KEY).expect("an analysed file still opens"));
    for step in 0..migrations().len() {
        let path = dir.path().join(format!("at-{step}.db"));
        drop(built(&path, step));
        let conn = pos_db::open(&path, KEY)
            .unwrap_or_else(|error| panic!("a clean file at {step} must open: {error}"));
        assert_eq!(user_version(&conn), SCHEMA_VERSION, "from {step}");
        assert_eq!(schema(&conn), full, "from {step}");
    }
}

/// An append-only trigger weakened under its own name rather than dropped: the
/// name is still there and its definition is not the version's. That is
/// refused too, as altered, because the definition is what keeps the rows
/// append-only, not the name.
#[test]
fn a_trigger_redefined_under_its_own_name_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("weakened.db");
    drop(pos_db::open(&path, KEY).unwrap());
    raw(&path)
        .execute_batch(
            "DROP TRIGGER audit_log_no_update;
             CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log BEGIN SELECT 1; END;",
        )
        .unwrap();
    assert_eq!(
        refused(&path),
        Named {
            recorded: SCHEMA_VERSION,
            missing: Vec::new(),
            unexpected: Vec::new(),
            altered: vec!["trigger audit_log_no_update".to_owned()],
        }
    );
}
