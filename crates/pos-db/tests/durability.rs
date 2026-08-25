//! A completed sale must survive the power cut that paid for it (I-4, I-9).
//!
//! In WAL mode `synchronous = NORMAL` does not fsync the log on commit, so
//! SQLite may lose the most recent transactions after a power loss. Because a
//! sale, its stock event, its outbox row and its fiscal-queue row commit
//! together, that loss is total and silent: no half-state, no alarm, and a
//! drawer that is over at Z time with no document to explain it.
//!
//! These tests are the tripwire on that setting. A benchmark that relaxes it
//! for throughput turns them red.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// `PRAGMA synchronous` answers with a number: 0 OFF, 1 NORMAL, 2 FULL, 3 EXTRA.
const FULL: i64 = 2;

#[test]
fn a_register_database_commits_durably() {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("durable.db"), "test-key").unwrap();

    let synchronous: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        synchronous, FULL,
        "a register must fsync on commit; NORMAL loses paid-for sales on power loss"
    );
}

#[test]
fn durability_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reopened.db");

    // The pragma is per-connection, not stored in the file. Every open has to
    // set it again, so the second open is the one worth asserting.
    drop(pos_db::open(&path, "test-key").unwrap());
    let conn = pos_db::open(&path, "test-key").unwrap();

    let synchronous: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    assert_eq!(synchronous, FULL, "a reopened register is still a register");
}

#[test]
fn foreign_keys_are_enforced_on_every_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fk.db");

    drop(pos_db::open(&path, "test-key").unwrap());
    let conn = pos_db::open(&path, "test-key").unwrap();

    let enforced: bool = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert!(enforced, "foreign keys are off by default in SQLite");
}

#[test]
fn a_committed_sale_is_readable_on_a_fresh_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("committed.db");

    let conn = pos_db::open(&path, "test-key").unwrap();
    let id = vec![1u8; 16];
    let register = vec![2u8; 16];
    conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1, 'R-000001', ?2, 'completed', 1500, 240, 1740, 'JOD',
                 '2026-08-25', '2026-08-25T10:00:00.000Z')",
        rusqlite::params![id, register],
    )
    .unwrap();
    drop(conn);

    // Dropping the connection is not the interesting part — a clean close
    // checkpoints regardless of the pragma. This asserts the ordinary path is
    // whole, so a failure here means something other than durability is wrong.
    let reopened = pos_db::open(&path, "test-key").unwrap();
    let total: i64 = reopened
        .query_row("SELECT total_minor FROM sale WHERE id = ?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(total, 1740);
}
