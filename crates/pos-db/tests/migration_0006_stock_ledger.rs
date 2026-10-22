//! Migration `0006`: the stock ledger (microstep 1.10.1), and the three guards
//! #197 tracked since #179.
//!
//! Stock is a ledger (I-6): an event is never edited, it carries the cost basis
//! in force when it was captured, and the cache it projects into moves in the
//! same statement. From this migration on, a sale whose lines move stock
//! completes only with each line's exact stock fact in its own commit.
//!
//! Every refusal below is asserted by its exact message, and the guards this
//! migration adds are each proved load-bearing the way `0005`'s were: withhold
//! the precondition, watch the refusal, drop the guard, watch the same
//! statement land. A negative test alone cannot tell which object refused.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::fixtures::registered_chain::{AT, Checkout, Member, RegisteredChain};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

const KEY: &str = "migration-0006-test-key";

fn id(value: u128) -> Vec<u8> {
    Uuid::from_u128(value).as_bytes().to_vec()
}

fn message(error: rusqlite::Error) -> String {
    match error {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message,
        other => panic!("expected a refusal with a message, found {other:?}"),
    }
}

/// The register every sale here is rung up on.
fn register_id() -> Vec<u8> {
    id(0x01)
}

struct Db {
    _dir: tempfile::TempDir,
    conn: Connection,
    chain: RegisteredChain,
}

fn db() -> Db {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    chain.add_register(&conn, &register_id(), "REG01");
    Db {
        _dir: dir,
        conn,
        chain,
    }
}

/// A catalogue product: counted (`each`, a whole unit per step) or weighed
/// (`weight`, one milli-unit — a gram of a kilogram — per step).
fn product(db: &Db, product: &[u8], sku: &str, weighed: bool) {
    let (unit, step) = if weighed {
        ("weight", 1)
    } else {
        ("each", 1000)
    };
    db.conn
        .execute(
            "INSERT INTO product (id, sku, name, price_minor, currency, is_active,
                                  tax_category_id, unit, qty_step_milli, is_weighed)
             VALUES (?1, ?2, ?2, 2500, 'JOD', 1, ?3, ?4, ?5, ?6)",
            params![product, sku, &db.chain.tax_category, unit, step, weighed],
        )
        .unwrap();
}

/// A parked sale of one line of `product`, `qty_milli` of it, and a cash
/// tender — everything the caller owns before the fixture attaches the rest.
fn parked_sale(db: &Db, slot: u8, product: &[u8], qty_milli: i64, weighed: bool) -> Checkout {
    let base = u128::from(slot) << 8;
    let (sale, line, tender) = (id(base), id(base + 1), id(base + 2));
    db.conn
        .execute(
            "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                               tax_minor, total_minor, currency, business_date, completed_at)
             VALUES (?1, ?2, ?3, 'parked', 2500, 400, 2900, 'JOD', '2026-08-25', ?4)",
            params![&sale, format!("REG01-{slot:06}"), &register_id(), AT],
        )
        .unwrap();
    let step = if weighed { 1 } else { 1000 };
    db.conn
        .execute(
            "INSERT INTO sale_line (id, sale_id, product_id, qty_milli, qty_step_milli,
                                    unit_price_minor, tax_minor, total_minor, line_no,
                                    name_snapshot, net_minor, tax_category_id, is_weighed)
             VALUES (?1, ?2, ?3, ?4, ?5, 2500, 400, 2900, 1, 'Line', 2500, ?6, ?7)",
            params![
                &line,
                &sale,
                product,
                qty_milli,
                step,
                &db.chain.tax_category,
                weighed
            ],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
             VALUES (?1, ?2, 'cash', 2900, 0)",
            params![&tender, &sale],
        )
        .unwrap();
    Checkout::new(slot, &sale, &register_id(), &[&line], &[&tender])
}

/// Everything a completion needs except the stock events, so a test can write
/// its own or none at all.
fn ready_but_for_stock(db: &Db, checkout: &Checkout) {
    checkout.attach(&db.conn, &db.chain);
    db.chain
        .write_envelope(&db.conn, &checkout.commit, &checkout.members());
    checkout.write_facts_before_envelope(&db.conn);
    checkout.write_receipt(&db.conn);
    checkout.write_audit(&db.conn, &db.chain);
    checkout.write_tender_events(&db.conn);
}

/// An incoming stock event in an envelope of its own: `qty_milli` of
/// `product` received at `unit_cost`, with the post-event projection the domain
/// computed (`wac_after`, `is_wac_estimated`). `event_seq` and
/// `on_hand_after_milli` are read from the ledger and cache the way the
/// repository will.
#[allow(clippy::too_many_arguments)]
fn receive(
    db: &Db,
    event: u128,
    product: &[u8],
    qty_milli: i64,
    unit_cost: Option<i64>,
    is_cost_estimated: bool,
    wac_after: i64,
    is_wac_estimated: bool,
) -> rusqlite::Result<usize> {
    let event_id = id(event);
    db.chain.write_envelope(
        &db.conn,
        &id(event + 0x1000),
        &[Member::new(&id(event + 0x2000), "stock_ledger", &event_id)],
    );
    db.conn.execute(
        "INSERT INTO stock_ledger
           (id, register_id, event_seq, product_id, store_id, qty_delta_milli,
            qty_step_milli, kind, unit_cost_minor, is_cost_estimated,
            on_hand_after_milli, wac_after_minor, is_wac_estimated, occurred_at,
            business_date)
         SELECT ?1, ?2,
                COALESCE((SELECT MAX(event_seq) FROM stock_ledger WHERE register_id = ?2), 0) + 1,
                ?3, ?4, ?5, p.qty_step_milli, 'receive', ?6, ?7,
                COALESCE((SELECT on_hand_milli FROM stock_cache
                           WHERE product_id = ?3 AND store_id = ?4), 0) + ?5,
                ?8, ?9, ?10, '2026-08-25'
           FROM product p WHERE p.id = ?3",
        params![
            &event_id,
            &register_id(),
            product,
            &db.chain.store,
            qty_milli,
            unit_cost,
            is_cost_estimated,
            wac_after,
            is_wac_estimated,
            AT
        ],
    )
}

/// One stored stock event's cost and quantity facts.
#[derive(Debug, PartialEq, Eq)]
struct Captured {
    qty_delta_milli: i64,
    unit_cost_minor: Option<i64>,
    is_cost_estimated: bool,
    is_weight_derived: bool,
}

fn captured(db: &Db, event: &[u8]) -> Captured {
    db.conn
        .query_row(
            "SELECT qty_delta_milli, unit_cost_minor, is_cost_estimated, is_weight_derived
               FROM stock_ledger WHERE id = ?1",
            [event],
            |row| {
                Ok(Captured {
                    qty_delta_milli: row.get(0)?,
                    unit_cost_minor: row.get(1)?,
                    is_cost_estimated: row.get(2)?,
                    is_weight_derived: row.get(3)?,
                })
            },
        )
        .unwrap()
}

/// `(on_hand_milli, wac_minor, is_wac_estimated, event_count)` for a product.
fn cache(db: &Db, product: &[u8]) -> Option<(i64, i64, bool, i64)> {
    db.conn
        .query_row(
            "SELECT on_hand_milli, wac_minor, is_wac_estimated, event_count
               FROM stock_cache WHERE product_id = ?1 AND store_id = ?2",
            params![product, &db.chain.store],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .unwrap()
}

fn only_stock_event(checkout: &Checkout) -> &[u8] {
    match checkout.stock_events.as_slice() {
        [event] => event,
        other => panic!("expected one stock event, found {}", other.len()),
    }
}

fn only_line(checkout: &Checkout) -> &[u8] {
    match checkout.lines.as_slice() {
        [line] => line,
        other => panic!("expected one line, found {}", other.len()),
    }
}

/// A sale stock event for the checkout's one line with an explicit quantity
/// and derived-weight flag, at the cache's cost basis — the event the
/// fixture writes, with the two values a price-embedded label changes.
fn sale_event(
    db: &Db,
    checkout: &Checkout,
    qty_delta_milli: i64,
    is_weight_derived: bool,
) -> rusqlite::Result<usize> {
    db.conn.execute(
        "INSERT INTO stock_ledger
           (id, register_id, event_seq, product_id, store_id, qty_delta_milli,
            qty_step_milli, kind, ref_kind, ref_id, source_sale_line_id,
            unit_cost_minor, is_cost_estimated, is_weight_derived,
            on_hand_after_milli, wac_after_minor, is_wac_estimated,
            occurred_at, business_date)
         SELECT ?1, s.register_id,
                COALESCE((SELECT MAX(e.event_seq) FROM stock_ledger e
                           WHERE e.register_id = s.register_id), 0) + 1,
                l.product_id, s.store_id, ?2, p.qty_step_milli,
                'sale', 'sale', s.id, l.id,
                COALESCE(c.wac_minor, 0), COALESCE(c.is_wac_estimated, 1), ?3,
                COALESCE(c.on_hand_milli, 0) + ?2,
                COALESCE(c.wac_minor, 0), COALESCE(c.is_wac_estimated, 1),
                ?4, s.business_date
           FROM sale_line l
           JOIN sale s ON s.id = l.sale_id
           JOIN product p ON p.id = l.product_id
           LEFT JOIN stock_cache c ON c.product_id = l.product_id AND c.store_id = s.store_id
          WHERE l.id = ?5",
        params![
            only_stock_event(checkout),
            qty_delta_milli,
            is_weight_derived,
            AT,
            only_line(checkout)
        ],
    )
}

const APPEND_ONLY: &str = "I-6: stock_ledger is append-only — post a correcting event";
const CHAIN: &str = "stock event must extend the ledger/cache chain with the captured WAC";
const STOCK_FACT: &str =
    "each stock-bearing sale line requires its exact stock fact in the sale commit";

// ── the named tests ─────────────────────────────────────────────────────────

/// A stock event is history. Neither an update nor a delete is accepted; a
/// correction is a new event.
#[test]
fn stock_ledger_is_append_only() {
    let db = db();
    product(&db, &id(0x50), "ESPRESSO", false);
    let checkout = parked_sale(&db, 1, &id(0x50), 1000, false);
    checkout.seal(&db.conn, &db.chain);
    let event = only_stock_event(&checkout).to_vec();

    let update = db
        .conn
        .execute(
            "UPDATE stock_ledger SET qty_delta_milli = 0 WHERE id = ?1",
            [&event],
        )
        .unwrap_err();
    assert_eq!(message(update), APPEND_ONLY);
    let delete = db
        .conn
        .execute("DELETE FROM stock_ledger WHERE id = ?1", [&event])
        .unwrap_err();
    assert_eq!(message(delete), APPEND_ONLY);
    assert_eq!(captured(&db, &event).qty_delta_milli, -1000);
}

/// The sale takes the weighted-average cost in force when it was rung up, and
/// keeps it: a later delivery at a higher cost moves today's average and leaves
/// the margin already earned exactly where it was.
#[test]
fn a_sale_event_records_the_cost_basis_at_capture_time() {
    let db = db();
    product(&db, &id(0x50), "ESPRESSO", false);
    receive(
        &db,
        0x700,
        &id(0x50),
        10_000,
        Some(1_500),
        false,
        1_500,
        false,
    )
    .unwrap();

    let checkout = parked_sale(&db, 1, &id(0x50), 1000, false);
    checkout.seal(&db.conn, &db.chain);
    let sold = only_stock_event(&checkout).to_vec();
    let at_capture = Captured {
        qty_delta_milli: -1000,
        unit_cost_minor: Some(1_500),
        is_cost_estimated: false,
        is_weight_derived: false,
    };
    assert_eq!(captured(&db, &sold), at_capture);

    // Ten more at 2.100: (9 × 1.500 + 10 × 2.100) / 19 = 1.816, as the domain's
    // WAC rule computes it. The trigger checks the chain, not the arithmetic.
    receive(
        &db,
        0x710,
        &id(0x50),
        10_000,
        Some(2_100),
        false,
        1_816,
        false,
    )
    .unwrap();
    assert_eq!(cache(&db, &id(0x50)), Some((19_000, 1_816, false, 3)));
    assert_eq!(captured(&db, &sold), at_capture, "the sale kept its cost");
}

/// A delivery with no known cost is recorded as unknown — `NULL` and flagged
/// estimated — never as a zero that reads as free stock. A sale from that stock
/// reports its cost as estimated too.
#[test]
fn missing_cost_is_null_and_reported_as_estimated() {
    let db = db();
    product(&db, &id(0x50), "ESPRESSO", false);
    let unflagged = receive(&db, 0x700, &id(0x50), 10_000, None, false, 0, false).unwrap_err();
    assert!(
        message(unflagged).starts_with("CHECK constraint failed"),
        "a missing cost must be flagged estimated"
    );

    receive(&db, 0x710, &id(0x50), 10_000, None, true, 0, true).unwrap();
    let delivered = captured(&db, &id(0x710));
    assert_eq!(
        (delivered.unit_cost_minor, delivered.is_cost_estimated),
        (None, true)
    );
    assert_eq!(cache(&db, &id(0x50)), Some((10_000, 0, true, 1)));

    let checkout = parked_sale(&db, 1, &id(0x50), 1000, false);
    checkout.seal(&db.conn, &db.chain);
    let sold = captured(&db, only_stock_event(&checkout));
    assert!(sold.is_cost_estimated, "the sale's cost is an estimate too");
}

/// E.80: a price label is one package at the label's price, so its line
/// quantity is not its weight. The weight the label implies goes to the ledger
/// instead, flagged as derived — and a measured event may not claim it.
#[test]
fn price_embedded_stock_event_carries_the_derived_weight_flagged_estimated() {
    let db = db();
    product(&db, &id(0x60), "CHEESE-KG", true);
    let checkout = parked_sale(&db, 1, &id(0x60), 1000, true);
    ready_but_for_stock(&db, &checkout);

    // 49 g, as a measured event: refused, because a measurement equals its line.
    let measured = sale_event(&db, &checkout, -49, false).unwrap_err();
    assert_eq!(message(measured), CHAIN);

    sale_event(&db, &checkout, -49, true).unwrap();
    checkout.complete(&db.conn);
    let stored = captured(&db, only_stock_event(&checkout));
    assert_eq!(stored.qty_delta_milli, -49);
    assert!(
        stored.is_weight_derived,
        "the weight was derived, not measured"
    );
    assert!(stored.is_cost_estimated, "and no cost basis existed");
    assert_eq!(cache(&db, &id(0x60)), Some((-49, 0, true, 1)));
}

/// A database at v5 keeps its products' quantity steps through 0006, and the
/// ledger holds every event to them.
#[test]
fn migration_0006_preserves_quantity_steps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v5.db");
    let chain = {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "key", KEY).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        for (index, sql) in [
            include_str!("../migrations/0001_init.sql"),
            include_str!("../migrations/0002_sale_integrity.sql"),
            include_str!("../migrations/0003_strict_rebuild_and_catalog_depth.sql"),
            include_str!("../migrations/0004_people_and_audit.sql"),
            include_str!("../migrations/0005_sale_columns_and_sequences.sql"),
        ]
        .iter()
        .enumerate()
        {
            let tx = conn.unchecked_transaction().unwrap();
            tx.execute_batch(sql).unwrap();
            tx.pragma_update(None, "user_version", i64::try_from(index + 1).unwrap())
                .unwrap();
            tx.commit().unwrap();
        }
        let chain = RegisteredChain::seed(&conn);
        chain.add_register(&conn, &register_id(), "REG01");
        for (product, sku, unit, step, weighed) in [
            (id(0x50), "ESPRESSO", "each", 1000, false),
            (id(0x60), "CHEESE-KG", "weight", 5, true),
        ] {
            conn.execute(
                "INSERT INTO product (id, sku, name, price_minor, currency, is_active,
                                      tax_category_id, unit, qty_step_milli, is_weighed)
                 VALUES (?1, ?2, ?2, 2500, 'JOD', 1, ?3, ?4, ?5, ?6)",
                params![&product, sku, &chain.tax_category, unit, step, weighed],
            )
            .unwrap();
        }
        conn.close().map_err(|(_, e)| e).unwrap();
        chain
    };

    let conn = pos_db::open(&path, KEY).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 6, "0006 ran on open");
    let steps: Vec<(String, i64)> = conn
        .prepare("SELECT sku, qty_step_milli FROM product ORDER BY sku")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        steps,
        vec![("CHEESE-KG".to_owned(), 5), ("ESPRESSO".to_owned(), 1000)],
        "0006 changes no product's step"
    );

    // The ledger reads those steps: 7 g of a cheese sold in 5 g steps is
    // refused, 10 g is accepted, and an event cannot claim a step of its own.
    let db = Db {
        _dir: dir,
        conn,
        chain,
    };
    let ragged = receive(&db, 0x700, &id(0x60), 7, Some(10), false, 10, false).unwrap_err();
    assert_eq!(
        message(ragged),
        "stock quantity must respect the product milli-unit step"
    );
    receive(&db, 0x710, &id(0x60), 10, Some(10), false, 10, false).unwrap();
    db.chain.write_envelope(
        &db.conn,
        &id(0x1720),
        &[Member::new(&id(0x2720), "stock_ledger", &id(0x720))],
    );
    let own_step = db
        .conn
        .execute(
            "INSERT INTO stock_ledger
               (id, register_id, event_seq, product_id, store_id, qty_delta_milli,
                qty_step_milli, kind, unit_cost_minor, is_cost_estimated,
                on_hand_after_milli, wac_after_minor, is_wac_estimated, occurred_at,
                business_date)
             VALUES (?1, ?2, 2, ?3, ?4, 1, 1, 'receive', 10, 0, 11, 10, 0, ?5, '2026-08-25')",
            params![&id(0x720), &register_id(), &id(0x60), &db.chain.store, AT],
        )
        .unwrap_err();
    assert_eq!(
        message(own_step),
        "stock quantity must respect the product milli-unit step"
    );
    assert_eq!(cache(&db, &id(0x60)), Some((10, 10, false, 1)));
}

// ── the guards this migration adds, each proved load-bearing ──────────────────

/// Run `statement`, expect `refusal` from `trigger`, drop `trigger`, and expect
/// the same statement to land — so the refusal was this guard and no other.
fn load_bearing(
    db: &Db,
    trigger: &str,
    refusal: &str,
    statement: &dyn Fn(&Db) -> rusqlite::Result<usize>,
) {
    let refused = statement(db).expect_err(&format!("`{trigger}` must refuse while it stands"));
    assert_eq!(
        message(refused),
        refusal,
        "`{trigger}` must be the object that refuses"
    );
    // A literal from the test, never a value read back from the database.
    db.conn
        .execute_batch(&format!("DROP TRIGGER {trigger};"))
        .unwrap_or_else(|e| panic!("`{trigger}` must exist to be the guard under test: {e}"));
    let rows = statement(db)
        .unwrap_or_else(|e| panic!("with `{trigger}` dropped the statement must land: {e}"));
    assert_eq!(rows, 1, "the statement lands once `{trigger}` is gone");
}

/// The completion gate itself: a sale whose line moves stock does not complete
/// without that line's stock fact in its commit, and nothing else refuses it.
#[test]
fn a_stock_bearing_sale_completes_only_with_its_stock_fact() {
    let db = db();
    product(&db, &id(0x50), "ESPRESSO", false);
    let checkout = parked_sale(&db, 1, &id(0x50), 1000, false);
    ready_but_for_stock(&db, &checkout);
    load_bearing(
        &db,
        "completed_sale_requires_stock_fact_update",
        STOCK_FACT,
        &|db| checkout.try_complete(&db.conn),
    );
}

/// A service — an e-recharge, a fee — moves no stock: its sale completes with
/// no stock event, and a stock event for it is refused at completion.
#[test]
fn a_service_line_moves_no_stock() {
    let db = db();
    product(&db, &id(0x70), "RECHARGE", false);
    db.conn
        .execute(
            "UPDATE product SET is_service = 1 WHERE id = ?1",
            [&id(0x70)],
        )
        .unwrap();
    let plain = parked_sale(&db, 1, &id(0x70), 1000, false);
    ready_but_for_stock(&db, &plain);
    plain.complete(&db.conn);

    let stocked = parked_sale(&db, 2, &id(0x70), 1000, false);
    ready_but_for_stock(&db, &stocked);
    sale_event(&db, &stocked, -1000, false).unwrap();
    let refused = stocked.try_complete(&db.conn).unwrap_err();
    assert_eq!(message(refused), STOCK_FACT);
}

/// #197, the first: `is_training` is a flag. The fiscal-decision gate keys on
/// it being 0, so a 2 would be a live sale with no fiscal decision behind it.
#[test]
fn sale_is_training_is_a_flag() {
    let db = db();
    let insert = |db: &Db, sale: u128, training: i64| {
        db.conn.execute(
            "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                               tax_minor, total_minor, currency, business_date,
                               completed_at, is_training)
             VALUES (?1, ?2, ?3, 'parked', 0, 0, 0, 'JOD', '2026-08-25', ?4, ?5)",
            params![&id(sale), format!("T-{sale}"), &register_id(), AT, training],
        )
    };
    insert(&db, 0x900, 0).unwrap();
    insert(&db, 0x901, 1).unwrap();
    load_bearing(
        &db,
        "sale_is_training_is_a_flag_update",
        "sale.is_training is a flag: 0 or 1",
        &|db| {
            db.conn.execute(
                "UPDATE sale SET is_training = 2 WHERE id = ?1",
                [&id(0x900)],
            )
        },
    );
    load_bearing(
        &db,
        "sale_is_training_is_a_flag_insert",
        "sale.is_training is a flag: 0 or 1",
        &|db| insert(db, 0x902, 2),
    );
}

/// #197, the second: a receipt artifact is a synced fact and waits for its
/// envelope like its siblings. `print_attempt` deliberately has no such gate —
/// it never syncs, so it has no envelope to wait for.
#[test]
fn a_receipt_artifact_requires_its_envelope() {
    let db = db();
    product(&db, &id(0x50), "ESPRESSO", false);
    let checkout = parked_sale(&db, 1, &id(0x50), 1000, false);
    load_bearing(
        &db,
        "receipt_artifact_has_ready_commit",
        "receipt artifact requires its complete delivery envelope",
        &|db| {
            db.conn.execute(
                "INSERT INTO receipt_artifact
                   (id, sale_id, artifact_kind, format, template_version, printer_profile,
                    content_bytes, content_hash, generated_at)
                 VALUES (?1, ?2, 'original', 'escpos', 'v1', '80mm', ?3, ?4, ?5)",
                params![
                    &checkout.artifact,
                    &checkout.sale,
                    vec![1u8; 8],
                    vec![2u8; 32],
                    AT
                ],
            )
        },
    );
    let gates: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM sqlite_master
              WHERE type = 'trigger' AND tbl_name = 'print_attempt'
                AND name LIKE '%has_ready_commit%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        gates, 0,
        "print_attempt is register-local and carries no envelope gate"
    );
}

/// #197, the third: a gapless counter is advanced, never removed — a deleted
/// row would restart at 1.
#[test]
fn a_document_sequence_is_never_deleted() {
    let db = db();
    db.conn
        .execute(
            "INSERT INTO doc_sequence (scope_kind, scope_id, kind, next_value)
             VALUES ('register', ?1, 'receipt', 42)",
            [&register_id()],
        )
        .unwrap();
    load_bearing(
        &db,
        "doc_sequence_no_delete",
        "a document sequence is never deleted: its next value would restart at 1",
        &|db| {
            db.conn.execute(
                "DELETE FROM doc_sequence WHERE scope_id = ?1",
                [&register_id()],
            )
        },
    );
}
