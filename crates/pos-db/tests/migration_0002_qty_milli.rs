//! Gap G-12, the data migration: `sale_line.qty` (unit counts) becomes
//! `qty_milli` (milli-units, 1 unit = 1000).
//!
//! The add-migration skill §6 requires this shape of test whenever a migration
//! changes existing data — seed the OLD schema, run the real runner, assert the
//! NEW one. The multiplication is the reason: a rename alone would leave every
//! historical quantity understated by a factor of a thousand, and it would look
//! completely fine until someone reconciled stock.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use rusqlite::{Connection, params};
use uuid::Uuid;

/// Build a database at schema version 1 — 0001 applied, by hand, exactly as a
/// register that has been selling since before 0002 would have it.
fn database_at_v1(path: &std::path::Path) -> (Uuid, Uuid) {
    let conn = Connection::open(path).unwrap();
    conn.pragma_update(None, "key", "test-key").unwrap();
    conn.execute_batch(include_str!("../migrations/0001_init.sql"))
        .unwrap();
    conn.pragma_update(None, "user_version", 1i64).unwrap();

    let (product, sale) = (Uuid::now_v7(), Uuid::now_v7());
    conn.execute(
        "INSERT INTO product (id, sku, name, price_minor, currency) VALUES (?1,'SKU-1','Rice',990,'JOD')",
        params![product.as_bytes().as_slice()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1,'000001',?2,'completed',2970,0,2970,'JOD','2026-08-01','2026-08-01T09:00:00.000Z')",
        params![sale.as_bytes().as_slice(), Uuid::now_v7().as_bytes().as_slice()],
    )
    .unwrap();
    // Three bags of rice, in the old representation: a plain unit count.
    conn.execute(
        "INSERT INTO sale_line (id, sale_id, product_id, qty, unit_price_minor, total_minor)
         VALUES (?1,?2,?3,3,990,2970)",
        params![
            Uuid::now_v7().as_bytes().as_slice(),
            sale.as_bytes().as_slice(),
            product.as_bytes().as_slice()
        ],
    )
    .unwrap();
    (product, sale)
}

#[test]
fn quantities_are_multiplied_by_a_thousand_not_merely_renamed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let (_product, sale) = database_at_v1(&path);

    // The real runner, taking the database from version 1 to current.
    let conn = pos_db::open(&path, "test-key").expect("0002 must apply to a v1 database");

    let qty_milli: i64 = conn
        .query_row(
            "SELECT qty_milli FROM sale_line WHERE sale_id = ?1",
            params![sale.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        qty_milli, 3000,
        "3 units must become 3000 milli-units, not 3"
    );

    // Nothing else about the line may move: the money was already correct.
    let (unit_price, total): (i64, i64) = conn
        .query_row(
            "SELECT unit_price_minor, total_minor FROM sale_line WHERE sale_id = ?1",
            params![sale.as_bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((unit_price, total), (990, 2970));

    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, pos_db::SCHEMA_VERSION);
}

/// The rebuild drops and recreates `sale_line`, which takes its triggers with
/// it. The completed sale seeded above proves they are back and armed.
#[test]
fn the_rebuilt_table_is_still_guarded_by_the_immutability_triggers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let (_product, sale) = database_at_v1(&path);
    let conn = pos_db::open(&path, "test-key").unwrap();

    assert!(
        conn.execute(
            "UPDATE sale_line SET qty_milli = 1 WHERE sale_id = ?1",
            params![sale.as_bytes().as_slice()],
        )
        .is_err(),
        "the migrated line belongs to a completed sale and must be frozen"
    );
}
