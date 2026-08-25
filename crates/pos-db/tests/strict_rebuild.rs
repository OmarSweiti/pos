//! The six shipped tables are rebuilt as STRICT by 0003, and the rebuild must not
//! lose a sale doing it.
//!
//! 0001 and 0002 created `product`, `sale`, `sale_line`, `sale_tender`,
//! `sync_outbox` and `sync_cursor` without STRICT, so `total_minor` accepted the
//! string 'ten point five' and `sale.id` accepted NULL. Those are the six tables a
//! register actually opens. STRICT cannot be added by ALTER TABLE and a committed
//! migration is never edited, so 0003 rebuilds them.
//!
//! A rebuild that drops and recreates the tables holding every completed sale is
//! the most dangerous statement in the chain. These tests seed the shipped schema
//! first, then apply the rebuild, then check the money is still there.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::reference_blocks;
use rusqlite::Connection;

/// Shipped chain only — the state a register is in before 0003 runs.
fn shipped(dir: &tempfile::TempDir, name: &str) -> Connection {
    pos_db::open(&dir.path().join(name), "test-key").unwrap()
}

fn apply_rebuild(conn: &Connection) {
    for (i, block) in reference_blocks().iter().enumerate() {
        conn.execute_batch(block)
            .unwrap_or_else(|e| panic!("reference block {i} failed: {e}"));
    }
}

/// One completed sale, two lines, one tender, an outbox row and a cursor.
fn seed_a_days_trading(conn: &Connection) {
    conn.execute(
        "INSERT INTO product (id, sku, name, price_minor, currency)
         VALUES (?1, 'SKU-1', 'Coffee', 1500, 'JOD')",
        [vec![1u8; 16]],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1, 'R-000001', ?2, 'parked', 3000, 480, 3480, 'JOD',
                 '2026-08-25', '2026-08-25T10:00:00.000Z')",
        rusqlite::params![vec![2u8; 16], vec![9u8; 16]],
    )
    .unwrap();
    for (line, no) in [(0x11u8, 1), (0x12u8, 2)] {
        conn.execute(
            "INSERT INTO sale_line (id, sale_id, product_id, qty_milli,
                                    unit_price_minor, total_minor)
             VALUES (?1, ?2, ?3, 1000, 1500, 1500)",
            rusqlite::params![vec![line; 16], vec![2u8; 16], vec![1u8; 16]],
        )
        .unwrap();
        let _ = no;
    }
    conn.execute(
        "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
         VALUES (?1, ?2, 'cash', 5000, 1520)",
        rusqlite::params![vec![0x31u8; 16], vec![2u8; 16]],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sync_outbox (entity, entity_id, op, payload)
         VALUES ('sale', ?1, 'insert', '{}')",
        [vec![2u8; 16]],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sync_cursor (entity, server_version) VALUES ('product', 42)",
        [],
    )
    .unwrap();
    // Complete it last: the point is to rebuild tables holding a frozen sale.
    conn.execute(
        "UPDATE sale SET status = 'completed' WHERE id = ?1",
        [vec![2u8; 16]],
    )
    .unwrap();
}

#[test]
fn the_rebuild_keeps_every_row_of_a_completed_sale() {
    let dir = tempfile::tempdir().unwrap();
    let conn = shipped(&dir, "keep.db");
    seed_a_days_trading(&conn);
    apply_rebuild(&conn);

    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(count("SELECT count(*) FROM product"), 1, "product lost");
    assert_eq!(count("SELECT count(*) FROM sale"), 1, "the sale was lost");
    assert_eq!(
        count("SELECT count(*) FROM sale_line"),
        2,
        "a line was lost"
    );
    assert_eq!(
        count("SELECT count(*) FROM sale_tender"),
        1,
        "the tender was lost"
    );
    assert_eq!(
        count("SELECT count(*) FROM sync_outbox"),
        1,
        "an outbox row was lost"
    );
    assert_eq!(
        count("SELECT count(*) FROM sync_cursor"),
        1,
        "the cursor was lost"
    );

    // The money, to the fil.
    assert_eq!(count("SELECT total_minor FROM sale"), 3480);
    assert_eq!(count("SELECT sum(total_minor) FROM sale_line"), 3000);
    assert_eq!(count("SELECT amount_minor FROM sale_tender"), 5000);
    assert_eq!(count("SELECT change_minor FROM sale_tender"), 1520);
    assert_eq!(count("SELECT server_version FROM sync_cursor"), 42);

    let status: String = conn
        .query_row("SELECT status FROM sale", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "completed", "the sale must still be completed");
}

#[test]
fn no_staging_table_survives_the_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let conn = shipped(&dir, "stage.db");
    seed_a_days_trading(&conn);
    apply_rebuild(&conn);

    let leftovers: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name LIKE 'stage_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leftovers, 0, "the rebuild left its scaffolding behind");
}

#[test]
fn after_the_rebuild_the_six_tables_enforce_their_types() {
    let dir = tempfile::tempdir().unwrap();
    let conn = shipped(&dir, "types.db");
    seed_a_days_trading(&conn);
    apply_rebuild(&conn);

    // The whole point: money is an integer, and identity is not optional.
    conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1, 'R-2', ?2, 'parked', 0, 0, 'ten point five', 'JOD',
                 '2026-08-25', '2026-08-25T10:00:00.000Z')",
        rusqlite::params![vec![3u8; 16], vec![9u8; 16]],
    )
    .expect_err("a string must not be storable in total_minor");

    conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (NULL, 'R-3', ?1, 'parked', 0, 0, 0, 'JOD',
                 '2026-08-25', '2026-08-25T10:00:00.000Z')",
        [vec![9u8; 16]],
    )
    .expect_err("a NULL identity must not be storable");
}

#[test]
fn the_rebuilt_tables_are_all_strict() {
    let dir = tempfile::tempdir().unwrap();
    let conn = shipped(&dir, "strict.db");
    apply_rebuild(&conn);

    for table in [
        "product",
        "sale",
        "sale_line",
        "sale_tender",
        "sync_outbox",
        "sync_cursor",
    ] {
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            sql.to_uppercase().contains("STRICT"),
            "{table} is still loose after the rebuild"
        );
    }
}

#[test]
fn the_rebuild_restores_the_immutability_triggers() {
    let dir = tempfile::tempdir().unwrap();
    let conn = shipped(&dir, "triggers.db");
    seed_a_days_trading(&conn);
    apply_rebuild(&conn);

    // Dropping the tables dropped 0002's triggers. If they were not recreated,
    // every completed sale in the fleet became editable.
    conn.execute(
        "UPDATE sale SET total_minor = 1 WHERE id = ?1",
        [vec![2u8; 16]],
    )
    .expect_err("a completed sale must still be immutable after the rebuild");
    conn.execute("DELETE FROM sale WHERE id = ?1", [vec![2u8; 16]])
        .expect_err("a completed sale must still be undeletable after the rebuild");
    conn.execute(
        "UPDATE sale_line SET qty_milli = 9000 WHERE id = ?1",
        [vec![0x11u8; 16]],
    )
    .expect_err("a line of a completed sale must still be frozen");
}
