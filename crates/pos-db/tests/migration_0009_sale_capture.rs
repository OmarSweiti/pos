//! Migration 0009 — the facts a completed sale captured (microstep 1.8.2).
//!
//! Three things to hold: a sale completed before the migration is left exactly
//! as it was; from the migration on, a sale completes only with the facts
//! every sale captures; and each group of captured facts is written whole or
//! not at all. Every fixture runs the exact chain the application compiles
//! with, `pos_db::migrations()`, and nothing reads `ref/schema.md`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use pos_db::fixtures::registered_chain::{Checkout, RegisteredChain};
use pos_db::repo::sale::SaleRepository;
use pos_domain::SaleId;
use rusqlite::{Connection, params};
use uuid::Uuid;

const KEY: &str = "test-key";
const DAY: &str = "2026-08-25";
const REGISTER: u8 = 0x31;
const PRODUCT: u8 = 0x32;
const CAPTURE_GATE: &str = "a completed sale carries what it captured: its opening, its price \
                            mode, each line's unit, price origin and age check, and each \
                            tender's order";

fn id(byte: u8) -> [u8; 16] {
    [byte; 16]
}

/// The file as the migration before this one left it: the first eight
/// migrations, each in its own transaction, exactly as the runner applies
/// them.
fn at_v8(path: &Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.pragma_update(None, "key", KEY).unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    for (index, sql) in pos_db::migrations().iter().take(8).enumerate() {
        let tx = conn.unchecked_transaction().unwrap();
        tx.execute_batch(sql).unwrap();
        tx.pragma_update(None, "user_version", i64::try_from(index + 1).unwrap())
            .unwrap();
        tx.commit().unwrap();
    }
    conn
}

/// A register on the reference chain, with one product.
fn world(conn: &Connection) -> RegisteredChain {
    let chain = RegisteredChain::seed(conn);
    chain.add_register(conn, &id(REGISTER), "REG01");
    conn.execute(
        "INSERT INTO product (id, sku, name, price_minor, currency, is_active, tax_category_id)
         VALUES (?1, 'SYN-1', 'Synthetic item', 2500, 'JOD', 1, ?2)",
        params![id(PRODUCT).as_slice(), &chain.tax_category],
    )
    .unwrap();
    chain
}

/// A parked sale of one line and one tender, written as a register before
/// `0009` would write it, and the checkout that can complete it.
fn parked_sale(conn: &Connection, chain: &RegisteredChain, slot: u8) -> Checkout {
    let (sale, line, tender) = (id(slot), id(slot | 0x40), id(slot | 0x80));
    conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1, ?2, ?3, 'parked', 2500, 345, 2500, 'JOD', ?4, '2026-08-25T10:00:00.000Z')",
        params![
            sale.as_slice(),
            format!("REG01-{slot:06}"),
            id(REGISTER).as_slice(),
            DAY
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale_line (id, sale_id, product_id, qty_milli, unit_price_minor,
                                tax_minor, total_minor, net_minor, line_no, name_snapshot,
                                tax_category_id)
         VALUES (?1, ?2, ?3, 1000, 2500, 345, 2500, 2155, 1, 'Synthetic item', ?4)",
        params![
            line.as_slice(),
            sale.as_slice(),
            id(PRODUCT).as_slice(),
            &chain.tax_category
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale_tender (id, sale_id, method, amount_minor) VALUES (?1, ?2, 'cash', 2500)",
        params![tender.as_slice(), sale.as_slice()],
    )
    .unwrap();
    Checkout::new(slot, &sale, &id(REGISTER), &[&line], &[&tender])
}

/// `result` was refused by the guard whose message is `expected`, and not by
/// some other constraint standing in for it.
fn refused(result: rusqlite::Result<usize>, expected: &str) {
    match result.expect_err("this write must be refused") {
        rusqlite::Error::SqliteFailure(_, Some(message)) => {
            assert!(
                message.contains(expected),
                "refused by {message:?}, not {expected:?}"
            );
        }
        other => panic!("refused for the wrong kind of reason: {other:?}"),
    }
}

/// A sale completed before `0009` carries none of its columns, and no value
/// would be true for it. The migration adds them empty and leaves the sale
/// exactly as it was: the completed row, its line and its tender, byte for
/// byte. The repository then refuses to read it as a `CompletedSale` and says
/// what it lacks, rather than filling in a unit or a supply it never captured.
#[test]
fn migration_0009_leaves_a_sale_completed_before_it_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let old = at_v8(&path);
    let chain = world(&old);
    let checkout = parked_sale(&old, &chain, 0x01);
    // What `Checkout::attach` writes, less the columns this migration adds:
    // the shift, the store and its policy.
    chain.open_shift(&old, &checkout.shift, &id(REGISTER), DAY);
    old.execute(
        "UPDATE sale SET store_id = ?1, shift_id = ?2, tax_computation_policy_id = ?3
          WHERE id = ?4",
        params![
            &chain.store,
            &checkout.shift,
            &chain.tax_policy,
            &checkout.sale
        ],
    )
    .unwrap();
    chain.write_envelope(&old, &checkout.commit, &checkout.members());
    checkout.write_facts(&old, &chain);
    checkout.complete(&old);
    let rows_before = rows(&old);
    old.close().map_err(|(_, e)| e).unwrap();

    let conn = pos_db::open(&path, KEY).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(
        version,
        pos_db::SCHEMA_VERSION,
        "every migration after v8 ran on open, 0009 among them"
    );
    assert_eq!(rows(&conn), rows_before, "the completed sale is untouched");
    let captured: (Option<String>, Option<String>, Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT s.opened_at, s.price_mode, l.unit, t.tender_no
               FROM sale s JOIN sale_line l ON l.sale_id = s.id
               JOIN sale_tender t ON t.sale_id = s.id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        captured,
        (None, None, None, None),
        "nothing is invented for it"
    );

    let read = SaleRepository::new(&conn).by_id(SaleId::from_uuid(Uuid::from_bytes(id(0x01))));
    assert!(
        matches!(read, Err(pos_db::DbError::StoredSaleInvalid { .. })),
        "a sale with no captured facts is refused by name: {read:?}"
    );
}

/// Every row of the three tables, every column the migration before this one
/// gave them, as text.
fn rows(conn: &Connection) -> Vec<String> {
    ["sale", "sale_line", "sale_tender"]
        .iter()
        .flat_map(|table| dump(conn, table))
        .collect()
}

/// One table's rows, every column quoted, in rowid order.
fn dump(conn: &Connection, table: &str) -> Vec<String> {
    let columns: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        // The new columns are compared on their own above; this is the row
        // as the migration before it left it.
        .filter(|c| !CAPTURE_COLUMNS.contains(&c.as_str()))
        .collect();
    let select = columns
        .iter()
        .map(|c| format!("quote({c})"))
        .collect::<Vec<_>>()
        .join(" || '|' || ");
    conn.prepare(&format!("SELECT {select} FROM {table} ORDER BY rowid"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|row| format!("{table}: {}", row.unwrap()))
        .collect()
}

/// The twenty-nine columns `0009` adds.
const CAPTURE_COLUMNS: [&str; 29] = [
    "opened_at",
    "price_mode",
    "cash_rounding_original_minor",
    "basket_discount_kind",
    "basket_discount_ppm",
    "basket_discount_amount_minor",
    "basket_discount_reason",
    "basket_discount_by",
    "basket_discount_approver",
    "unit",
    "price_origin",
    "label_rule_prefix",
    "label_raw_code",
    "max_price_minor",
    "min_age",
    "is_age_confirmed",
    "label_qty_milli",
    "label_unit_price_minor",
    "line_discount_kind",
    "line_discount_ppm",
    "line_discount_amount_minor",
    "line_discount_reason",
    "line_discount_by",
    "line_discount_approver",
    "override_price_minor",
    "override_reason",
    "override_by",
    "override_approver",
    "tender_no",
];

/// The list above names exactly the columns the migration added.
#[test]
fn the_capture_columns_are_the_ones_0009_adds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("columns.db");
    let names = |conn: &Connection| -> Vec<String> {
        ["sale", "sale_line", "sale_tender"]
            .iter()
            .flat_map(|table| {
                conn.prepare(&format!("PRAGMA table_info({table})"))
                    .unwrap()
                    .query_map([], |r| r.get::<_, String>(1))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let before = names(&at_v8(&path));
    let after = names(&pos_db::open(&path, KEY).unwrap());
    let added: Vec<String> = after.into_iter().filter(|c| !before.contains(c)).collect();
    assert_eq!(added, CAPTURE_COLUMNS);
}

/// From `0009` on, a sale completes only with what every sale captures. Each
/// case withholds one of those facts from an otherwise complete sale and
/// watches the capture gate, and not another guard, refuse it; the last case
/// withholds nothing and completes.
#[test]
fn a_sale_completes_only_with_what_every_sale_captures() {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("gate.db"), KEY).unwrap();
    let chain = world(&conn);
    let withheld = [
        "UPDATE sale SET opened_at = NULL WHERE id = ?1",
        "UPDATE sale SET price_mode = NULL WHERE id = ?1",
        "UPDATE sale_line SET unit = NULL WHERE sale_id = ?1",
        "UPDATE sale_line SET price_origin = NULL WHERE sale_id = ?1",
        "UPDATE sale_line SET is_age_confirmed = NULL WHERE sale_id = ?1",
        "UPDATE sale_tender SET tender_no = NULL WHERE sale_id = ?1",
    ];
    for (statement, slot) in withheld.iter().zip(1_u8..) {
        let checkout = parked_sale(&conn, &chain, slot);
        checkout.attach(&conn, &chain);
        conn.execute(statement, [&checkout.sale]).unwrap();
        chain.write_envelope(&conn, &checkout.commit, &checkout.members());
        checkout.write_facts(&conn, &chain);
        refused(checkout.try_complete(&conn), CAPTURE_GATE);
    }
    let checkout = parked_sale(&conn, &chain, 0x0F);
    checkout.attach(&conn, &chain);
    chain.write_envelope(&conn, &checkout.commit, &checkout.members());
    checkout.write_facts(&conn, &chain);
    checkout.complete(&conn);
}

/// Each group of captured facts is written whole or not at all: a discount is
/// a share or an amount with its reason and who gave it, an override carries
/// its price, reason and who set it, a label line carries its rule, its code
/// and both halves of its weight, and the vocabularies refuse a word they do
/// not hold. Each refusal names the group.
#[test]
fn each_group_of_captured_facts_is_kept_whole() {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("groups.db"), KEY).unwrap();
    let chain = world(&conn);
    let checkout = parked_sale(&conn, &chain, 0x01);
    let sale = &checkout.sale;
    let cashier = &chain.cashier;
    let run = |sql: &str| conn.execute(sql, params![sale, cashier]);

    let basket = "a basket discount is a share or an amount";
    refused(
        run("UPDATE sale SET basket_discount_kind = 'percent' WHERE id = ?1 AND ?2 IS NOT NULL"),
        basket,
    );
    refused(
        run(
            "UPDATE sale SET basket_discount_kind = 'amount', basket_discount_ppm = 50000,
                    basket_discount_amount_minor = 100, basket_discount_reason = 'promo',
                    basket_discount_by = ?2 WHERE id = ?1",
        ),
        basket,
    );
    refused(
        run("UPDATE sale SET basket_discount_approver = ?2 WHERE id = ?1"),
        basket,
    );
    run(
        "UPDATE sale SET basket_discount_kind = 'percent', basket_discount_ppm = 50000,
                basket_discount_reason = 'promo', basket_discount_by = ?2 WHERE id = ?1",
    )
    .expect("a share with its reason and who gave it is whole");

    let origin = "a line priced from the catalogue carries no label";
    refused(
        run("UPDATE sale_line SET price_origin = 'label' WHERE sale_id = ?1 AND ?2 IS NOT NULL"),
        origin,
    );
    refused(
        run(
            "UPDATE sale_line SET price_origin = 'department' WHERE sale_id = ?1 AND ?2 IS NOT NULL",
        ),
        origin,
    );
    refused(
        run(
            "UPDATE sale_line SET price_origin = 'catalog', label_rule_prefix = '20',
                    label_raw_code = '2012345004165' WHERE sale_id = ?1 AND ?2 IS NOT NULL",
        ),
        origin,
    );
    refused(
        run("UPDATE sale_line SET label_qty_milli = 210 WHERE sale_id = ?1 AND ?2 IS NOT NULL"),
        origin,
    );
    run(
        "UPDATE sale_line SET price_origin = 'label', label_rule_prefix = '20',
                label_raw_code = '2012345004165', label_qty_milli = 210,
                label_unit_price_minor = 1999 WHERE sale_id = ?1 AND ?2 IS NOT NULL",
    )
    .expect("a label line with its rule, its code and its weight is whole");

    let discount = "a line discount is a share or an amount";
    refused(
        run(
            "UPDATE sale_line SET line_discount_kind = 'amount', line_discount_amount_minor = 100,
                    line_discount_reason = 'loyal' WHERE sale_id = ?1 AND ?2 IS NOT NULL",
        ),
        discount,
    );
    refused(
        run(
            "UPDATE sale_line SET line_discount_kind = 'amount', line_discount_ppm = 50000,
                    line_discount_reason = 'loyal', line_discount_by = ?2 WHERE sale_id = ?1",
        ),
        discount,
    );

    let override_ = "an override carries its price, its reason and who set it";
    refused(
        run(
            "UPDATE sale_line SET override_price_minor = 2350 WHERE sale_id = ?1 AND ?2 IS NOT NULL",
        ),
        override_,
    );
    refused(
        run("UPDATE sale_line SET override_approver = ?2 WHERE sale_id = ?1"),
        override_,
    );
    run(
        "UPDATE sale_line SET override_price_minor = 2350, override_reason = 'displayed_price',
                override_by = ?2 WHERE sale_id = ?1",
    )
    .expect("an override with its price, its reason and who set it is whole");

    refused(
        run("UPDATE sale_line SET unit = 'kg' WHERE sale_id = ?1 AND ?2 IS NOT NULL"),
        "CHECK constraint failed",
    );
    refused(
        run("UPDATE sale SET price_mode = 'gross' WHERE id = ?1 AND ?2 IS NOT NULL"),
        "CHECK constraint failed",
    );
    refused(
        run("UPDATE sale_line SET is_age_confirmed = 2 WHERE sale_id = ?1 AND ?2 IS NOT NULL"),
        "CHECK constraint failed",
    );

    run("UPDATE sale_tender SET tender_no = 1 WHERE sale_id = ?1 AND ?2 IS NOT NULL").unwrap();
    refused(
        conn.execute(
            "INSERT INTO sale_tender (id, sale_id, method, amount_minor, tender_no)
             VALUES (?1, ?2, 'card', 100, 1)",
            params![id(0xEE).as_slice(), sale],
        ),
        "UNIQUE constraint failed: sale_tender.sale_id, sale_tender.tender_no",
    );
}
