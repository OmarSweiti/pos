//! Registered-chain coverage for migration 0003.
//!
//! The reference-SQL rebuild tests remain valuable, but they cannot prove the
//! application actually carries 0003. Every test in this file reaches the new
//! schema through `pos_db::open`; the data-transition fixture first builds the
//! committed v2 chain, seeds its old shape, and then lets the real runner apply
//! everything after v2.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use rusqlite::{Connection, ErrorCode, params};

const KEY: &str = "test-key";

fn blob(byte: u8) -> Vec<u8> {
    vec![byte; 16]
}

struct TestDb {
    _dir: tempfile::TempDir,
    conn: Connection,
}

fn current_database(name: &str) -> TestDb {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join(name), KEY).unwrap();
    TestDb { _dir: dir, conn }
}

fn insert_inactive_product(conn: &Connection, id: u8, sku: &str) {
    conn.execute(
        "INSERT INTO product
           (id, sku, name, price_minor, currency, is_active)
         VALUES (?1, ?2, ?3, 1250, 'JOD', 0)",
        params![blob(id), sku, format!("Product {sku}")],
    )
    .unwrap();
}

fn insert_parked_sale(conn: &Connection, id: u8, receipt_number: &str) {
    conn.execute(
        "INSERT INTO sale
           (id, receipt_number, register_id, status, subtotal_minor,
            tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1, ?2, ?3, 'parked', 1250, 0, 1250, 'JOD',
                 '2026-08-27', '2026-08-27T09:30:00.000Z')",
        params![blob(id), receipt_number, blob(0xf0)],
    )
    .unwrap();
}

fn sqlite_message(error: rusqlite::Error) -> String {
    match error {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message,
        other => other.to_string(),
    }
}

#[test]
fn migration_0003_creates_all_tables() {
    let db = current_database("all-tables.db");

    for table in [
        "product",
        "sale",
        "sale_line",
        "sale_tender",
        "sync_commit",
        "fact_commit_member",
        "sync_outbox",
        "sync_cursor",
        "org",
        "store",
        "register",
        "category",
        "tax_category",
        "tax_rule_pack",
        "tax_computation_policy",
        "tax_rate",
        "barcode",
        "setting",
        "sale_line_tax",
        "sale_line_discount",
        "sale_supply_tax_context",
    ] {
        let present: bool = db
            .conn
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_schema
                    WHERE type = 'table' AND name = ?1
                 )",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            present,
            "registered migration 0003 did not create `{table}`"
        );
    }

    let version: i64 = db
        .conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, pos_db::SCHEMA_VERSION);
    assert!(
        version >= 3,
        "the registered chain stopped before migration 0003"
    );
}

#[test]
fn barcode_live_uniqueness_allows_reissue_after_tombstone() {
    let db = current_database("barcode-live-uniqueness.db");
    insert_inactive_product(&db.conn, 1, "SKU-1");
    insert_inactive_product(&db.conn, 2, "SKU-2");

    db.conn
        .execute(
            "INSERT INTO barcode (id, product_id, code)
             VALUES (?1, ?2, '6251234567890')",
            params![blob(0x11), blob(1)],
        )
        .unwrap();

    let collision = db
        .conn
        .execute(
            "INSERT INTO barcode (id, product_id, code)
             VALUES (?1, ?2, '6251234567890')",
            params![blob(0x12), blob(2)],
        )
        .expect_err("two live barcode rows must not claim the same code");
    assert_eq!(
        collision.sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    );

    db.conn
        .execute(
            "UPDATE barcode
                SET deleted_at = '2026-08-27T10:00:00.000Z'
              WHERE id = ?1",
            [blob(0x11)],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO barcode (id, product_id, code)
             VALUES (?1, ?2, '6251234567890')",
            params![blob(0x12), blob(2)],
        )
        .expect("a tombstoned barcode code must be available for reissue");

    let (total_claims, live_claims, live_product): (i64, i64, Vec<u8>) = db
        .conn
        .query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN deleted_at IS NULL THEN 1 ELSE 0 END),
                    MAX(CASE WHEN deleted_at IS NULL THEN product_id END)
               FROM barcode
              WHERE code = '6251234567890'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(total_claims, 2);
    assert_eq!(live_claims, 1);
    assert_eq!(live_product, blob(2));
}

#[test]
fn barcode_pack_qty_defaults_to_1000_milli() {
    let db = current_database("barcode-default-pack.db");
    insert_inactive_product(&db.conn, 1, "SKU-1");
    db.conn
        .execute(
            "INSERT INTO barcode (id, product_id, code)
             VALUES (?1, ?2, '6250000000001')",
            params![blob(0x11), blob(1)],
        )
        .unwrap();

    let pack_qty_milli: i64 = db
        .conn
        .query_row(
            "SELECT pack_qty_milli FROM barcode WHERE id = ?1",
            [blob(0x11)],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pack_qty_milli, 1000);
}

#[test]
fn a_pack_quantity_of_zero_is_refused_at_save() {
    let db = current_database("barcode-zero-pack.db");
    insert_inactive_product(&db.conn, 1, "SKU-1");

    let error = db
        .conn
        .execute(
            "INSERT INTO barcode (id, product_id, code, pack_qty_milli)
             VALUES (?1, ?2, '6250000000002', 0)",
            params![blob(0x11), blob(1)],
        )
        .expect_err("a zero pack quantity must fail at the storage boundary");
    assert_eq!(
        error.sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    );
    assert!(
        sqlite_message(error).contains("pack_qty_milli > 0"),
        "the pack-size CHECK, rather than an unrelated constraint, must refuse zero"
    );
}

#[test]
fn tobacco_product_must_be_a_sealed_pack() {
    let db = current_database("sealed-tobacco.db");

    let error = db
        .conn
        .execute(
            "INSERT INTO product
               (id, sku, name, price_minor, currency, is_active,
                regulated_kind, sale_form)
             VALUES (?1, 'TOBACCO-BULK', 'Tobacco', 2500, 'JOD', 0,
                     'tobacco', 'bulk')",
            [blob(1)],
        )
        .expect_err("a tobacco SKU must not be saved in a non-sealed form");
    assert_eq!(
        sqlite_message(error),
        "regulated tobacco products must be sold as sealed packs"
    );

    db.conn
        .execute(
            "INSERT INTO product
               (id, sku, name, price_minor, currency, is_active,
                regulated_kind, sale_form)
             VALUES (?1, 'TOBACCO-PACK', 'Tobacco pack', 2500, 'JOD', 0,
                     'tobacco', 'sealed_pack')",
            [blob(2)],
        )
        .expect("the sealed-pack representation is valid for tobacco");
}

struct LegacyIds {
    product: Vec<u8>,
    sale: Vec<u8>,
    line_one: Vec<u8>,
    line_two: Vec<u8>,
    tender: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
struct MigratedProduct {
    sku: String,
    name: String,
    price_minor: i64,
    currency: String,
    is_active: i64,
    deleted_at: Option<String>,
    updated_at: String,
    version: i64,
    tax_category_id: Option<Vec<u8>>,
    name_ar: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct MigratedSale {
    receipt_number: String,
    register_id: Vec<u8>,
    status: String,
    subtotal_minor: i64,
    tax_minor: i64,
    total_minor: i64,
    currency: String,
    ref_sale_id: Option<Vec<u8>>,
    business_date: String,
    completed_at: String,
}

/// Construct the committed 0001+0002 chain and nothing later, then seed values
/// using exactly that v2 shape. The connection is dropped before `open` runs so
/// the registered runner performs the v2 -> current transition itself.
fn seed_completed_sale_at_v2(path: &Path) -> LegacyIds {
    let conn = Connection::open(path).unwrap();
    conn.pragma_update(None, "key", KEY).unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    conn.execute_batch(include_str!("../migrations/0001_init.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../migrations/0002_sale_integrity.sql"))
        .unwrap();
    conn.pragma_update(None, "user_version", 2_i64).unwrap();

    let ids = LegacyIds {
        product: blob(1),
        sale: blob(2),
        line_one: blob(0x11),
        line_two: blob(0x12),
        tender: blob(0x21),
    };

    // V2 allowed this active catalogue row without a tax category. Migration
    // 0003 must carry it forward as-is; it must not invent regulatory data.
    conn.execute(
        "INSERT INTO product
           (id, sku, name, price_minor, currency, is_active, deleted_at,
            updated_at, version)
         VALUES (?1, 'LEGACY-COFFEE', 'Legacy Coffee', 1500, 'JOD', 1, NULL,
                 '2026-08-20T08:00:00.000Z', 7)",
        [&ids.product],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale
           (id, receipt_number, register_id, status, subtotal_minor, tax_minor,
            total_minor, currency, ref_sale_id, business_date, completed_at)
         VALUES (?1, 'R-000042', ?2, 'parked', 4500, 720, 4920, 'JOD', NULL,
                 '2026-08-20', '2026-08-20T09:15:00.000Z')",
        params![&ids.sale, blob(3)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale_line
           (id, sale_id, product_id, qty_milli, unit_price_minor,
            discount_minor, tax_minor, total_minor)
         VALUES (?1, ?2, ?3, 1000, 1500, 100, 224, 1624)",
        params![&ids.line_one, &ids.sale, &ids.product],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale_line
           (id, sale_id, product_id, qty_milli, unit_price_minor,
            discount_minor, tax_minor, total_minor)
         VALUES (?1, ?2, ?3, 2000, 1500, 200, 496, 3296)",
        params![&ids.line_two, &ids.sale, &ids.product],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale_tender
           (id, sale_id, method, amount_minor, psp_ref, change_minor)
         VALUES (?1, ?2, 'card', 4920, 'PSP-LEGACY-42', 0)",
        params![&ids.tender, &ids.sale],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sync_cursor (entity, server_version)
         VALUES ('product', 314)",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE sale SET status = 'completed' WHERE id = ?1",
        [&ids.sale],
    )
    .unwrap();

    drop(conn);
    ids
}

#[test]
fn the_rebuild_keeps_every_row_of_a_completed_sale() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v2-completed-sale.db");
    let ids = seed_completed_sale_at_v2(&path);
    let conn = pos_db::open(&path, KEY).expect("the registered v3 migration must accept v2 data");

    let product: MigratedProduct = conn
        .query_row(
            "SELECT sku, name, price_minor, currency, is_active, deleted_at,
                    updated_at, version, tax_category_id, name_ar
               FROM product WHERE id = ?1",
            [&ids.product],
            |row| {
                Ok(MigratedProduct {
                    sku: row.get(0)?,
                    name: row.get(1)?,
                    price_minor: row.get(2)?,
                    currency: row.get(3)?,
                    is_active: row.get(4)?,
                    deleted_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    version: row.get(7)?,
                    tax_category_id: row.get(8)?,
                    name_ar: row.get(9)?,
                })
            },
        )
        .unwrap();
    assert_eq!(
        product,
        MigratedProduct {
            sku: "LEGACY-COFFEE".into(),
            name: "Legacy Coffee".into(),
            price_minor: 1500,
            currency: "JOD".into(),
            is_active: 1,
            deleted_at: None,
            updated_at: "2026-08-20T08:00:00.000Z".into(),
            version: 7,
            tax_category_id: None,
            name_ar: Some("Legacy Coffee".into()),
        }
    );

    let sale: MigratedSale = conn
        .query_row(
            "SELECT receipt_number, register_id, status, subtotal_minor, tax_minor,
                    total_minor, currency, ref_sale_id, business_date, completed_at
               FROM sale WHERE id = ?1",
            [&ids.sale],
            |row| {
                Ok(MigratedSale {
                    receipt_number: row.get(0)?,
                    register_id: row.get(1)?,
                    status: row.get(2)?,
                    subtotal_minor: row.get(3)?,
                    tax_minor: row.get(4)?,
                    total_minor: row.get(5)?,
                    currency: row.get(6)?,
                    ref_sale_id: row.get(7)?,
                    business_date: row.get(8)?,
                    completed_at: row.get(9)?,
                })
            },
        )
        .unwrap();
    assert_eq!(
        sale,
        MigratedSale {
            receipt_number: "R-000042".into(),
            register_id: blob(3),
            status: "completed".into(),
            subtotal_minor: 4500,
            tax_minor: 720,
            total_minor: 4920,
            currency: "JOD".into(),
            ref_sale_id: None,
            business_date: "2026-08-20".into(),
            completed_at: "2026-08-20T09:15:00.000Z".into(),
        }
    );

    for (line_id, expected) in [
        (&ids.line_one, (1000, 1500, 100, 224, 1624, 1)),
        (&ids.line_two, (2000, 1500, 200, 496, 3296, 2)),
    ] {
        let preserved: (Vec<u8>, Vec<u8>, i64, i64, i64, i64, i64) = conn
            .query_row(
                "SELECT sale_id, product_id, qty_milli, unit_price_minor,
                        discount_minor, tax_minor, total_minor
                   FROM sale_line WHERE id = ?1",
                [line_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(preserved.0, ids.sale);
        assert_eq!(preserved.1, ids.product);
        assert_eq!(
            (
                preserved.2,
                preserved.3,
                preserved.4,
                preserved.5,
                preserved.6
            ),
            (expected.0, expected.1, expected.2, expected.3, expected.4)
        );

        let backfilled: (i64, i64, String, i64, Option<Vec<u8>>, i64) = conn
            .query_row(
                "SELECT qty_step_milli, line_no, name_snapshot, net_minor,
                        tax_category_id, is_weighed
                   FROM sale_line WHERE id = ?1",
                [line_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            backfilled,
            (1000, expected.5, "Legacy Coffee".into(), 0, None, 0)
        );
    }

    let tender: (Vec<u8>, String, i64, Option<String>, i64) = conn
        .query_row(
            "SELECT sale_id, method, amount_minor, psp_ref, change_minor
               FROM sale_tender WHERE id = ?1",
            [&ids.tender],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        tender,
        (
            ids.sale.clone(),
            "card".into(),
            4920,
            Some("PSP-LEGACY-42".into()),
            0
        )
    );

    let cursor: i64 = conn
        .query_row(
            "SELECT server_version FROM sync_cursor WHERE entity = 'product'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cursor, 314);

    for table in ["product", "sale", "sale_line", "sale_tender", "sync_cursor"] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        let expected = if table == "sale_line" { 2 } else { 1 };
        assert_eq!(
            count, expected,
            "migration lost or invented a `{table}` row"
        );
    }
    let outbox_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0))
        .unwrap();
    assert_eq!(outbox_rows, 0, "migration invented a sync envelope");
}

#[test]
fn the_rebuilt_tables_are_all_strict() {
    let db = current_database("strict-tables.db");

    for table in [
        "product",
        "sale",
        "sale_line",
        "sale_tender",
        "sync_outbox",
        "sync_cursor",
    ] {
        let definition: String = db
            .conn
            .query_row(
                "SELECT sql FROM sqlite_schema
                  WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            definition.to_ascii_uppercase().contains("STRICT"),
            "migration 0003 left `{table}` as a loose table"
        );
    }
}

#[test]
fn the_rebuild_restores_the_immutability_triggers() {
    let db = current_database("restored-immutability.db");
    insert_inactive_product(&db.conn, 1, "SKU-1");
    insert_parked_sale(&db.conn, 2, "R-000001");
    insert_parked_sale(&db.conn, 3, "R-000002");
    db.conn
        .execute(
            "INSERT INTO sale_line
               (id, sale_id, product_id, qty_milli, unit_price_minor, total_minor)
             VALUES (?1, ?2, ?3, 1000, 1250, 1250)",
            params![blob(0x11), blob(2), blob(1)],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO sale_tender (id, sale_id, method, amount_minor)
             VALUES (?1, ?2, 'cash', 1250)",
            params![blob(0x21), blob(2)],
        )
        .unwrap();
    db.conn
        .execute(
            "UPDATE sale SET status = 'completed' WHERE id = ?1",
            [blob(2)],
        )
        .unwrap();

    for trigger in [
        "sale_no_update_once_completed",
        "sale_no_delete_once_completed",
        "sale_line_no_insert_once_completed",
        "sale_line_no_update_once_completed",
        "sale_line_no_delete_once_completed",
        "sale_tender_no_insert_once_completed",
        "sale_tender_no_update_once_completed",
        "sale_tender_no_delete_once_completed",
    ] {
        let present: bool = db
            .conn
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_schema
                    WHERE type = 'trigger' AND name = ?1
                 )",
                [trigger],
                |row| row.get(0),
            )
            .unwrap();
        assert!(present, "migration 0003 did not restore `{trigger}`");
    }

    db.conn
        .execute("UPDATE sale SET total_minor = 1 WHERE id = ?1", [blob(2)])
        .expect_err("the rebuilt completed sale must remain immutable");
    db.conn
        .execute(
            "UPDATE sale_line SET qty_milli = 2000 WHERE id = ?1",
            [blob(0x11)],
        )
        .expect_err("the rebuilt line on a completed sale must remain immutable");
    db.conn
        .execute(
            "UPDATE sale_tender SET psp_ref = 'late' WHERE id = ?1",
            [blob(0x21)],
        )
        .expect_err("the rebuilt tender on a completed sale must remain immutable");

    // The corrected UPDATE guards inspect both parents: moving a parked row
    // into the completed sale must be refused even though its OLD parent is open.
    db.conn
        .execute(
            "INSERT INTO sale_line
               (id, sale_id, product_id, qty_milli, unit_price_minor, total_minor)
             VALUES (?1, ?2, ?3, 1000, 1250, 1250)",
            params![blob(0x12), blob(3), blob(1)],
        )
        .unwrap();
    let error = db
        .conn
        .execute(
            "UPDATE sale_line SET sale_id = ?1 WHERE id = ?2",
            params![blob(2), blob(0x12)],
        )
        .expect_err("a parked line must not be reparented into a completed sale");
    assert_eq!(
        sqlite_message(error),
        "I-4: a line of a completed sale is immutable"
    );
}

#[test]
fn no_staging_table_survives_the_rebuild() {
    let db = current_database("no-staging-tables.db");
    let leftovers: Vec<String> = db
        .conn
        .prepare(
            r"SELECT name FROM sqlite_schema
              WHERE type = 'table' AND name LIKE 'stage\_%' ESCAPE '\'
              ORDER BY name",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        leftovers.is_empty(),
        "migration 0003 left staging tables behind: {leftovers:?}"
    );
}

fn assert_strict_type_refusal(result: Result<usize, rusqlite::Error>, table: &str) {
    let error = result.expect_err(&format!("`{table}` accepted a TEXT in an INTEGER column"));
    match error {
        rusqlite::Error::SqliteFailure(sqlite, _) => {
            assert_eq!(
                sqlite.extended_code,
                rusqlite::ffi::SQLITE_CONSTRAINT_DATATYPE,
                "`{table}` failed for something other than STRICT type enforcement"
            );
        }
        other => panic!("`{table}` failed with a non-SQLite error: {other:?}"),
    }
}

#[test]
fn after_the_rebuild_the_six_tables_enforce_their_types() {
    let db = current_database("strict-type-enforcement.db");

    assert_strict_type_refusal(
        db.conn.execute(
            "INSERT INTO product
               (id, sku, name, price_minor, currency, is_active)
             VALUES (?1, 'BAD-PRODUCT', 'Bad product', 'not-an-integer', 'JOD', 0)",
            [blob(0xe1)],
        ),
        "product",
    );

    insert_inactive_product(&db.conn, 1, "SKU-1");
    insert_parked_sale(&db.conn, 2, "R-000001");
    assert_strict_type_refusal(
        db.conn.execute(
            "INSERT INTO sale
               (id, receipt_number, register_id, status, subtotal_minor,
                tax_minor, total_minor, currency, business_date, completed_at)
             VALUES (?1, 'BAD-SALE', ?2, 'parked', 0, 0, 'not-an-integer',
                     'JOD', '2026-08-27', '2026-08-27T09:30:00.000Z')",
            params![blob(0xe2), blob(0xf0)],
        ),
        "sale",
    );
    assert_strict_type_refusal(
        db.conn.execute(
            "INSERT INTO sale_line
               (id, sale_id, product_id, qty_milli, unit_price_minor, total_minor)
             VALUES (?1, ?2, ?3, 1000, 1250, 'not-an-integer')",
            params![blob(0xe3), blob(2), blob(1)],
        ),
        "sale_line",
    );
    assert_strict_type_refusal(
        db.conn.execute(
            "INSERT INTO sale_tender (id, sale_id, method, amount_minor)
             VALUES (?1, ?2, 'cash', 'not-an-integer')",
            params![blob(0xe4), blob(2)],
        ),
        "sale_tender",
    );

    db.conn
        .execute(
            "INSERT INTO sync_commit
               (id, commit_size, commit_hash, protocol_version, schema_version,
                producer_version, created_at)
             VALUES (?1, 1, 'commit-hash', 1, 3, 'test',
                     '2026-08-27T09:30:00.000Z')",
            [blob(0x30)],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO fact_commit_member
               (change_id, commit_id, commit_index, entity, entity_id, op,
                payload, payload_hash, created_at)
             VALUES (?1, ?2, 0, 'sale', ?3, 'insert', '{}', 'payload-hash',
                     '2026-08-27T09:30:00.000Z')",
            params![blob(0x31), blob(0x30), blob(2)],
        )
        .unwrap();
    assert_strict_type_refusal(
        db.conn.execute(
            "INSERT INTO sync_outbox (change_id, attempts)
             VALUES (?1, 'not-an-integer')",
            [blob(0x31)],
        ),
        "sync_outbox",
    );
    assert_strict_type_refusal(
        db.conn.execute(
            "INSERT INTO sync_cursor (entity, server_version)
             VALUES ('bad-type', 'not-an-integer')",
            [],
        ),
        "sync_cursor",
    );
}
