//! Microstep 1.9.1 — migration `0005`, against the registered chain.
//!
//! Every test here opens through [`pos_db::open`], so it runs the exact
//! `MIGRATIONS` array the application compiles with. `ref/schema.md` is not
//! replayed on top: `0005` is *shipped*, and a green run against the reference
//! document would not be evidence that a register carries any of it.
//!
//! `0005` is the largest migration in Phase 1 and most of it is scaffolding for
//! later microsteps. What is proved here is what `0005` itself decides:
//!
//! * one open shift per register, held by a partial unique index rather than by
//!   a repository nobody has written yet;
//! * a completed sale must name the open shift for its own register, store and
//!   business date;
//! * every tender of a completed sale must carry an initial status event, so
//!   settlement is an appended fact and never an `UPDATE`;
//! * the completed-sale immutability guards `0002` and `0003` built are still
//!   standing after fourteen `ALTER TABLE`s and sixty new triggers;
//! * and the `exchange` tender seed carries the `is_internal` contract that
//!   keeps an exchange out of expected drawer cash.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::{Connection, params};
use uuid::Uuid;

#[path = "common/registered_chain.rs"]
mod registered_chain;

use registered_chain::{AT, Checkout, RegisteredChain};

const KEY: &str = "test-key";
const BUSINESS_DATE: &str = "2026-08-25";

fn id(value: u128) -> Vec<u8> {
    Uuid::from_u128(value).as_bytes().to_vec()
}

fn message(error: rusqlite::Error) -> String {
    match error {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message,
        other => panic!("expected a trigger refusal, found {other:?}"),
    }
}

struct Register {
    _dir: tempfile::TempDir,
    conn: Connection,
    chain: RegisteredChain,
}

fn register(name: &str) -> Register {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join(name), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    chain.add_register(&conn, &id(0x01), "REG01");
    Register {
        _dir: dir,
        conn,
        chain,
    }
}

/// A sale on `REG01` with one line and one tender, and everything `0005`
/// requires of a completed one *except* the completion itself: the store, the
/// open shift, the policy snapshot, the tax component, the receipt and its
/// queued job, the delivery envelope, the audit row and the tender event.
///
/// Each test then removes exactly one of those and watches the matching gate
/// fire, which is only meaningful because the untouched fixture completes.
fn parked_sale(register: &Register) -> Checkout {
    let (sale, line, tender, product) = (id(0x10), id(0x11), id(0x12), id(0x13));
    register
        .conn
        .execute(
            "INSERT INTO product
               (id, sku, name, price_minor, currency, is_active, tax_category_id)
             VALUES (?1, 'SKU-1', 'Espresso', 2500, 'JOD', 1, ?2)",
            params![&product, &register.chain.tax_category],
        )
        .unwrap();
    register
        .conn
        .execute(
            "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                               tax_minor, total_minor, currency, business_date, completed_at)
             VALUES (?1, '000123', ?2, 'parked', 2500, 400, 2900, 'JOD', ?3, ?4)",
            params![&sale, &id(0x01), BUSINESS_DATE, AT],
        )
        .unwrap();
    register
        .conn
        .execute(
            "INSERT INTO sale_line
               (id, sale_id, product_id, qty_milli, unit_price_minor, tax_minor,
                total_minor, line_no, name_snapshot, net_minor, tax_category_id)
             VALUES (?1, ?2, ?3, 1000, 2500, 400, 2900, 1, 'Espresso', 2500, ?4)",
            params![&line, &sale, &product, &register.chain.tax_category],
        )
        .unwrap();
    register
        .conn
        .execute(
            "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
             VALUES (?1, ?2, 'cash', 2900, 0)",
            params![&tender, &sale],
        )
        .unwrap();

    let checkout = Checkout::new(1, &sale, &id(0x01), &[&line], &[&tender]);
    checkout.attach(&register.conn, &register.chain);
    register
        .chain
        .write_envelope(&register.conn, &checkout.commit, &checkout.members());
    checkout.write_facts_before_envelope(&register.conn);
    checkout.write_audit(&register.conn, &register.chain);
    checkout.write_tender_events(&register.conn);
    checkout
}

#[test]
fn opening_a_second_shift_for_the_register_is_refused() {
    let r = register("second-shift.db");
    r.chain
        .open_shift(&r.conn, &id(0x20), &id(0x01), BUSINESS_DATE);

    // The refusal comes from `idx_shift_one_open`, the partial unique index on
    // the projection — not from a repository, which is why it holds against a
    // register whose repository has not been written yet.
    let refused = r
        .chain
        .try_open_shift(&r.conn, &id(0x21), &id(0x01), BUSINESS_DATE)
        .expect_err("two open shifts are two expected-cash figures for one drawer");
    assert!(
        message(refused).contains("shift_state.register_id"),
        "the second open must be refused by idx_shift_one_open itself"
    );

    // The rule is per register, not per store: a second till on the same store
    // opens its own shift on the same business date.
    r.chain.add_register(&r.conn, &id(0x02), "REG02");
    r.chain
        .open_shift(&r.conn, &id(0x22), &id(0x02), BUSINESS_DATE);
    let open: i64 = r
        .conn
        .query_row(
            "SELECT count(*) FROM shift_state WHERE state = 'open'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(open, 2, "one open shift per register, two registers");

    // The projection is the migration's, not the fixture's: `shift_project_open`
    // writes it, and `shift_state_no_delete` refuses to let anyone unpick it.
    let deleted = r.conn.execute("DELETE FROM shift_state", []).unwrap_err();
    assert_eq!(
        message(deleted),
        "a shift projection is rebuilt, not selectively deleted"
    );
}

#[test]
fn a_completed_sale_requires_an_open_matching_shift() {
    let r = register("matching-shift.db");
    let checkout = parked_sale(&r);

    // A shift on another register, open and otherwise identical. The sale may
    // point at it while it is parked; it may not complete there.
    r.chain.add_register(&r.conn, &id(0x02), "REG02");
    r.chain
        .open_shift(&r.conn, &id(0x30), &id(0x02), BUSINESS_DATE);
    r.conn
        .execute(
            "UPDATE sale SET shift_id = ?1 WHERE id = ?2",
            params![&id(0x30), &checkout.sale],
        )
        .expect("a parked sale is work in progress");

    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("another register's shift is not this register's shift");
    assert_eq!(
        message(refused),
        "a completed sale requires the open shift for its register, store and business date"
    );

    // Nor a shift whose business date is not the sale's: the shift owns the
    // filing day, so a sale on the wrong one would file on the wrong day.
    r.conn
        .execute(
            "UPDATE sale SET shift_id = ?1, business_date = '2026-08-26' WHERE id = ?2",
            params![&checkout.shift, &checkout.sale],
        )
        .unwrap();
    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("a shift's business date is the sale's filing day");
    assert_eq!(
        message(refused),
        "a completed sale requires the open shift for its register, store and business date"
    );

    r.conn
        .execute(
            "UPDATE sale SET business_date = ?1 WHERE id = ?2",
            params![BUSINESS_DATE, &checkout.sale],
        )
        .unwrap();
    checkout.complete(&r.conn);
    let status: String = r
        .conn
        .query_row(
            "SELECT status FROM sale WHERE id = ?1",
            [&checkout.sale],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "completed", "the matching open shift completes it");
}

#[test]
fn every_completed_tender_has_an_initial_status_event() {
    let r = register("tender-events.db");
    let (sale, line, tender, product) = (id(0x40), id(0x41), id(0x42), id(0x43));
    r.conn
        .execute(
            "INSERT INTO product
               (id, sku, name, price_minor, currency, is_active, tax_category_id)
             VALUES (?1, 'SKU-2', 'Latte', 3000, 'JOD', 1, ?2)",
            params![&product, &r.chain.tax_category],
        )
        .unwrap();
    r.conn
        .execute(
            "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                               tax_minor, total_minor, currency, business_date, completed_at)
             VALUES (?1, '000124', ?2, 'parked', 3000, 480, 3480, 'JOD', ?3, ?4)",
            params![&sale, &id(0x01), BUSINESS_DATE, AT],
        )
        .unwrap();
    r.conn
        .execute(
            "INSERT INTO sale_line
               (id, sale_id, product_id, qty_milli, unit_price_minor, tax_minor,
                total_minor, line_no, name_snapshot, net_minor, tax_category_id)
             VALUES (?1, ?2, ?3, 1000, 3000, 480, 3480, 1, 'Latte', 3000, ?4)",
            params![&line, &sale, &product, &r.chain.tax_category],
        )
        .unwrap();
    r.conn
        .execute(
            "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
             VALUES (?1, ?2, 'cash', 3480, 0)",
            params![&tender, &sale],
        )
        .unwrap();

    // Everything a completion needs except the tender's own status event.
    let checkout = Checkout::new(2, &sale, &id(0x01), &[&line], &[&tender]);
    checkout.attach(&r.conn, &r.chain);
    r.chain
        .write_envelope(&r.conn, &checkout.commit, &checkout.members());
    checkout.write_facts_before_envelope(&r.conn);
    checkout.write_audit(&r.conn, &r.chain);

    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("a tender with no status event has never been collected from");
    assert_eq!(
        message(refused),
        "every completed-sale tender requires an initial status event"
    );

    checkout.write_tender_events(&r.conn);
    checkout.complete(&r.conn);

    // The projection is the migration's work: `tender_status_project_current`
    // wrote it, and it must agree with the event it came from.
    let (event_no, state): (i64, String) = r
        .conn
        .query_row(
            "SELECT event_no, state FROM tender_status_current WHERE tender_id = ?1",
            [&tender],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((event_no, state.as_str()), (1, "pending"));

    // And settlement stays an appended fact: the event is immutable, and the
    // tender it describes cannot be updated instead.
    let refused = r
        .conn
        .execute(
            "UPDATE tender_status_event SET state = 'collected' WHERE tender_id = ?1",
            [&tender],
        )
        .expect_err("settlement history is append-only");
    assert_eq!(message(refused), "tender settlement is append-only");
}

#[test]
fn migration_0005_preserves_completed_sale_guards() {
    let r = register("preserved-guards.db");
    let checkout = parked_sale(&r);
    checkout.complete(&r.conn);

    // Fourteen `ALTER TABLE`s and sixty new triggers later, the eight guards
    // 0002 wrote and 0003 rebuilt are still the ones enforcing I-4.
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
        let present: bool = r
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger' AND name=?1)",
                [trigger],
                |row| row.get(0),
            )
            .unwrap();
        assert!(present, "0005 dropped `{trigger}`");
    }

    // Present is not the same as working, so each one is exercised.
    assert!(
        r.conn
            .execute(
                "UPDATE sale SET total_minor = 1 WHERE id = ?1",
                [&checkout.sale]
            )
            .is_err(),
        "I-4: a completed sale refuses UPDATE"
    );
    assert!(
        r.conn
            .execute("DELETE FROM sale WHERE id = ?1", [&checkout.sale])
            .is_err(),
        "I-4: a completed sale refuses DELETE"
    );
    assert!(
        r.conn
            .execute(
                "UPDATE sale_line SET total_minor = 1 WHERE sale_id = ?1",
                [&checkout.sale]
            )
            .is_err(),
        "I-4: the lines of a completed sale are frozen"
    );
    let refused = r
        .conn
        .execute(
            "UPDATE sale_tender SET psp_ref = 'PSP-9' WHERE sale_id = ?1",
            [&checkout.sale],
        )
        .expect_err("settlement is an appended event, never a tender UPDATE");
    assert_eq!(
        message(refused),
        "I-4: a tender on a completed sale is immutable — append a status event"
    );

    // 0005's own additions to the completed sale are immutable too.
    r.conn
        .execute(
            "INSERT INTO sale_tax_summary
               (id, sale_id, component_code, treatment, calculation_kind, rate_ppm,
                calculation_order, base_kind, taxable_base_minor, net_minor, tax_minor,
                gross_minor)
             VALUES (?1, ?2, 'GST', 'standard', 'ad_valorem', 160000, 0, 'line_net',
                     2500, 2500, 400, 2900)",
            params![&id(0x50), &checkout.sale],
        )
        .expect_err("a tax summary cannot be added to a sale that is already a fact");
}

#[test]
fn exchange_tender_seed_matches_internal_contract() {
    let r = register("exchange-seed.db");
    let (opens_drawer, allows_change, is_cash_counted, is_internal, refundable_to, is_active): (
        i64,
        i64,
        i64,
        i64,
        String,
        i64,
    ) = r
        .conn
        .query_row(
            "SELECT opens_drawer, allows_change, is_cash_counted, is_internal,
                    refundable_to, is_active
               FROM tender_type WHERE code = 'exchange'",
            [],
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
        .expect("0005 seeds the complete tender catalogue; later steps never append codes");

    assert_eq!(
        (
            opens_drawer,
            allows_change,
            is_cash_counted,
            is_internal,
            refundable_to.as_str()
        ),
        (0, 0, 0, 1, "none"),
        "domain-api.md 7.1: an exchange only offsets two linked documents"
    );
    assert_eq!(
        is_active, 0,
        "exchange is seeded inactive — it has no Phase-1 effect until a refund \
         path enables it"
    );

    // The contract's whole point: `is_internal` is NOT implied by
    // `is_cash_counted = 0`. Card and CliQ count no drawer cash and still move
    // real value through a PSP, so they are external.
    let external_but_uncounted: i64 = r
        .conn
        .query_row(
            "SELECT count(*) FROM tender_type
              WHERE is_cash_counted = 0 AND is_internal = 0 AND code IN ('card','cliq')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        external_but_uncounted, 2,
        "card and cliq are uncounted and external; reading one flag off the \
         other is the mistake this row exists to refuse"
    );
}

#[test]
fn an_exchange_tender_never_opens_or_counts_the_drawer() {
    let r = register("exchange-drawer.db");

    // Stated as a rule over the catalogue, not as one row: a tender that moves
    // no value outside the two documents it settles must never reach the
    // drawer. If it did, the offset would appear in expected cash on both
    // documents and the shift would close short by twice the exchanged value.
    let offending: Vec<String> = r
        .conn
        .prepare(
            "SELECT code FROM tender_type
              WHERE is_internal = 1 AND (opens_drawer <> 0 OR is_cash_counted <> 0
                                      OR allows_change <> 0)",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        offending.is_empty(),
        "internal tenders that touch the drawer: {offending:?}"
    );

    let internal: Vec<String> = r
        .conn
        .prepare("SELECT code FROM tender_type WHERE is_internal = 1 ORDER BY sort_order")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        internal,
        vec!["exchange".to_owned()],
        "exchange is the only internal tender 0005 seeds; a second one would \
         need its own drawer reasoning"
    );

    let seeded: i64 = r
        .conn
        .query_row("SELECT count(*) FROM tender_type", [], |row| row.get(0))
        .unwrap();
    assert_eq!(seeded, 6, "0005 seeds the catalogue complete, once");
}
