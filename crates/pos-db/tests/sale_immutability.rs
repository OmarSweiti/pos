//! I-4: a completed sale is immutable, enforced by the storage engine.
//!
//! Conventions §1 named three enforcement points for I-4 — review, a test that
//! greps the repositories, and the absence of a method that could do it. The
//! first is the weakest and, until 0002, was the only one that existed. These
//! tests hold the storage-level guard: they pass against a database opened by
//! the real migration runner, so they fail if a future migration drops a
//! trigger — which is exactly what the 0003 `sale_line` rebuild would do.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::{Connection, params};
use uuid::Uuid;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

/// Every trigger 0002 installs. A rebuild of a guarded table silently takes its
/// triggers with it, so the list is asserted rather than assumed.
const REQUIRED_TRIGGERS: &[&str] = &[
    "sale_no_update_once_completed",
    "sale_no_delete_once_completed",
    "sale_line_no_insert_once_completed",
    "sale_line_no_update_once_completed",
    "sale_line_no_delete_once_completed",
    "sale_tender_no_insert_once_completed",
    "sale_tender_no_delete_once_completed",
    "sale_tender_amount_frozen_once_completed",
];

struct Fixture {
    _dir: tempfile::TempDir,
    conn: Connection,
    sale: Uuid,
    product: Uuid,
    register: Uuid,
}

/// A register holding one parked sale with one line and one tender.
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), "test-key").unwrap();

    let (product, sale, register) = (id(1), id(2), id(3));
    conn.execute(
        "INSERT INTO product (id, sku, name, price_minor, currency) VALUES (?1,?2,?3,?4,?5)",
        params![
            product.as_bytes().as_slice(),
            "SKU-1",
            "Espresso",
            2500_i64,
            "JOD"
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1,'000123',?2,'parked',2500,400,2900,'JOD','2026-08-20','2026-08-20T10:00:00.000Z')",
        params![sale.as_bytes().as_slice(), register.as_bytes().as_slice()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale_line (id, sale_id, product_id, qty_milli, unit_price_minor, total_minor)
         VALUES (?1,?2,?3,1000,2500,2500)",
        params![
            id(4).as_bytes().as_slice(),
            sale.as_bytes().as_slice(),
            product.as_bytes().as_slice()
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale_tender (id, sale_id, method, amount_minor) VALUES (?1,?2,'cash',2900)",
        params![id(5).as_bytes().as_slice(), sale.as_bytes().as_slice()],
    )
    .unwrap();

    Fixture {
        _dir: dir,
        conn,
        sale,
        product,
        register,
    }
}

impl Fixture {
    fn complete(&self) {
        self.conn
            .execute(
                "UPDATE sale SET status='completed' WHERE id=?1",
                params![self.sale.as_bytes().as_slice()],
            )
            .expect("parked → completed is the ordinary lifecycle and must be allowed");
    }
}

#[test]
fn every_immutability_trigger_survives_the_migration_runner() {
    let f = fixture();
    let present: Vec<String> = f
        .conn
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    for want in REQUIRED_TRIGGERS {
        assert!(
            present.iter().any(|p| p == want),
            "trigger `{want}` is gone. A migration that rebuilds a table takes its \
             triggers with it — recreate it in that migration (ref/schema.md 0003)."
        );
    }
}

#[test]
fn a_parked_sale_is_still_editable() {
    let f = fixture();
    f.conn
        .execute(
            "UPDATE sale SET total_minor=3000 WHERE id=?1",
            params![f.sale.as_bytes().as_slice()],
        )
        .expect("a parked sale is work in progress, not a fact");
    f.complete();
}

#[test]
fn a_completed_sale_refuses_update_and_delete() {
    let f = fixture();
    f.complete();
    let id = f.sale.as_bytes().to_vec();

    assert!(
        f.conn
            .execute("UPDATE sale SET total_minor=1 WHERE id=?1", params![id])
            .is_err(),
        "I-4: a completed sale must refuse UPDATE"
    );
    let id = f.sale.as_bytes().to_vec();
    assert!(
        f.conn
            .execute("DELETE FROM sale WHERE id=?1", params![id])
            .is_err(),
        "I-4: a completed sale must refuse DELETE"
    );
}

#[test]
fn the_lines_of_a_completed_sale_are_frozen() {
    let f = fixture();
    f.complete();
    let sale = f.sale.as_bytes().to_vec();

    assert!(
        f.conn
            .execute(
                "INSERT INTO sale_line (id, sale_id, product_id, qty_milli, unit_price_minor, total_minor)
                 VALUES (?1,?2,?3,1000,100,100)",
                params![
                    id(6).as_bytes().as_slice(),
                    sale,
                    f.product.as_bytes().as_slice()
                ],
            )
            .is_err(),
        "a line cannot be added to a completed sale"
    );
    let sale = f.sale.as_bytes().to_vec();
    assert!(
        f.conn
            .execute(
                "UPDATE sale_line SET total_minor=1 WHERE sale_id=?1",
                params![sale]
            )
            .is_err(),
        "a line of a completed sale cannot be edited"
    );
    let sale = f.sale.as_bytes().to_vec();
    assert!(
        f.conn
            .execute("DELETE FROM sale_line WHERE sale_id=?1", params![sale])
            .is_err(),
        "a line of a completed sale cannot be removed"
    );
}

/// The deliberate exception: a semi-integrated card capture settles after the
/// sale closes, so 0004's tender_state/captured_at must remain writable. The
/// money must not.
#[test]
fn a_tender_settles_after_completion_but_its_amount_does_not_move() {
    let f = fixture();
    f.complete();
    let sale = f.sale.as_bytes().to_vec();

    f.conn
        .execute(
            "UPDATE sale_tender SET psp_ref='PSP-9' WHERE sale_id=?1",
            params![sale],
        )
        .expect("settlement columns must stay writable — a capture confirms late");

    let sale = f.sale.as_bytes().to_vec();
    assert!(
        f.conn
            .execute(
                "UPDATE sale_tender SET amount_minor=1 WHERE sale_id=?1",
                params![sale]
            )
            .is_err(),
        "I-4: the amount tendered is a fact"
    );
    let sale = f.sale.as_bytes().to_vec();
    assert!(
        f.conn
            .execute("DELETE FROM sale_tender WHERE sale_id=?1", params![sale])
            .is_err(),
        "a payment cannot be removed from a completed sale"
    );
}

#[test]
fn a_receipt_number_is_unique_per_register_but_not_across_them() {
    let f = fixture();

    let clash = f.conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1,'000123',?2,'parked',1,0,1,'JOD','2026-08-20','2026-08-20T11:00:00.000Z')",
        params![
            id(7).as_bytes().as_slice(),
            f.register.as_bytes().as_slice()
        ],
    );
    assert!(clash.is_err(), "one register must not print 000123 twice");

    f.conn
        .execute(
            "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                               tax_minor, total_minor, currency, business_date, completed_at)
             VALUES (?1,'000123',?2,'parked',1,0,1,'JOD','2026-08-20','2026-08-20T11:00:00.000Z')",
            params![id(8).as_bytes().as_slice(), id(9).as_bytes().as_slice()],
        )
        .expect("a different register legitimately prints 000123 too");
}
