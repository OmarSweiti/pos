//! Microstep 1.10.3 — the stock cache is a cache, and is held to its ledger.
//!
//! On-hand is the sum of the ledger's deltas (I-6). `stock_cache` holds that
//! sum, projected one event at a time by `0006`'s trigger, and a rebuild writes
//! it again from the whole ledger at once. These tests hold the two to each
//! other, and hold the check that finds a cache that has drifted:
//!
//! * after any sequence of events, the projection is the replay and the
//!   domain's sum, a rebuild leaves every row byte for byte as it was, and a
//!   change to any one column of the cache is found and named;
//! * a corrupted cache is found at verification, rebuilt in the verifying
//!   transaction, and left equal to a full replay of the ledger — over the
//!   events of real completed sales, a scale label's derived weight and stock
//!   sold below zero among them;
//! * a rebuild that fails part-way leaves the old cache whole, and a true
//!   cache is not written at all.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod sale_world;

use std::cell::Cell;

use pos_db::DbError;
use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::stock::{CacheDrift, PreparedStockEvent, StockRepository};
use pos_domain::{
    Currency, IncomingCost, Money, NewStockEvent, ProductId, Qty, RegisterId, StockError,
    StockEvent, StockEventId, StockEventKind, StoreId, on_hand,
};
use pos_test_support::io_proptest_config;
use proptest::prelude::*;
use rusqlite::types::Value;
use rusqlite::{Connection, params};
use sale_world::{
    OTHER_REGISTER, REGISTER, World, at, day, jod, no_sabotage, uuid, write_envelope,
};
use uuid::Uuid;

/// Every cache row of `store`, whole and as stored, in product order. `*`
/// rather than a column list, so a column added later is compared too: this is
/// what "byte-identical" is measured over.
fn cache_rows(conn: &Connection, store: StoreId) -> Vec<Vec<Value>> {
    let mut statement = conn
        .prepare("SELECT * FROM stock_cache WHERE store_id = ?1 ORDER BY product_id")
        .unwrap();
    let width = statement.column_count();
    statement
        .query_map([store.as_uuid().as_bytes().as_slice()], |row| {
            (0..width).map(|i| row.get::<_, Value>(i)).collect()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn verify(conn: &Connection, store: StoreId) -> Result<Vec<CacheDrift>, DbError> {
    let tx = conn.unchecked_transaction().unwrap();
    StockRepository::new(conn).verify_cache(&tx, store)
}

fn reconcile(conn: &Connection, store: StoreId) -> Result<Vec<CacheDrift>, DbError> {
    let tx = conn.unchecked_transaction().unwrap();
    let drift = StockRepository::new(conn).reconcile_cache(&tx, store)?;
    tx.commit().unwrap();
    Ok(drift)
}

// ── A register that has sold ─────────────────────────────────────────────────

fn till() -> RegisterId {
    RegisterId::from_uuid(uuid(REGISTER))
}

/// Stock arriving in the register's own ledger, in one transaction under its
/// own envelope, as 1.10.5's opening stock will write it.
fn arrive(w: &World, commit: u8, arrivals: &[(ProductId, i64, i64)]) {
    let movements: Vec<NewStockEvent> = arrivals
        .iter()
        .zip(1_u8..)
        .map(|(&(product_id, qty_milli, cost_minor), n)| NewStockEvent {
            id: StockEventId::from_uuid(Uuid::from_bytes([
                commit, 0x5E, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ])),
            product_id,
            store_id: w.store(),
            kind: StockEventKind::Adjust,
            qty_delta: Qty::from_milli(qty_milli),
            incoming_cost: Some(IncomingCost::Observed(jod(cost_minor))),
            is_weight_derived: false,
            source: None,
            actor_id: Some(w.cashier()),
            occurred_at: at(),
            business_date: day(),
        })
        .collect();
    let stock = StockRepository::new(&w.conn);
    let tx = w.conn.unchecked_transaction().unwrap();
    let prepared = stock.prepare(&tx, till(), &movements).unwrap();
    let members: Vec<(String, [u8; 16], String)> = prepared
        .iter()
        .map(|p| {
            (
                "stock_ledger".to_owned(),
                *p.event().id.as_uuid().as_bytes(),
                p.payload(),
            )
        })
        .collect();
    write_envelope(&w.conn, &tx, commit, &members);
    stock.append(&tx, &prepared).unwrap();
    tx.commit().unwrap();
}

/// A product the store stocks and the ledger has never moved.
const SPARE: u8 = 0x64;

/// Twelve items and five kilograms of cheese arrive; then two sales complete
/// through the finalize 1.8.3 will write. The first sells two items, a tobacco
/// pack nobody stocked, so its on-hand goes below zero at an estimated cost,
/// and cheese by a scale label, whose stock moves by the weight the label
/// implies. The second sells one more item.
fn sold() -> World {
    let w = World::new();
    w.conn
        .execute(
            "INSERT INTO product
               (id, sku, name, name_ar, price_minor, currency, is_active,
                tax_category_id, unit, qty_step_milli, is_weighed)
             VALUES (?1, 'SYN-SPARE', 'SYN-SPARE', 'SYN-SPARE', 1000, 'JOD', 1, ?2,
                     'each', 1000, 0)",
            params![uuid(SPARE).as_bytes().as_slice(), &w.chain.tax_category],
        )
        .unwrap();
    arrive(&w, 0x01, &[(w.item, 12_000, 380), (w.cheese, 5_000, 9_000)]);
    w.finalize(&w.everything(), 0x02, no_sabotage, false)
        .unwrap();
    w.finalize(&w.plain(0x20, "REG01-000002"), 0x03, no_sabotage, false)
        .unwrap();
    w
}

/// The items' cache row put back to the position its previous event left, as a
/// missed projection would leave it: one event behind, on-hand included.
const ONE_EVENT_BEHIND: &str = "UPDATE stock_cache
    SET (on_hand_milli, wac_minor, is_wac_estimated, last_event_seq, last_event_id,
         event_count, last_event_at) =
        (SELECT e.on_hand_after_milli, e.wac_after_minor, e.is_wac_estimated,
                e.event_seq, e.id, stock_cache.event_count - 1, e.occurred_at
           FROM stock_ledger e
          WHERE e.product_id = stock_cache.product_id
          ORDER BY e.event_seq DESC LIMIT 1 OFFSET 1)
    WHERE product_id = ?1";

#[test]
fn a_corrupted_cache_is_found_at_verification_and_rebuilt_to_the_replay() {
    let w = sold();
    let store = w.store();
    // The cache as `0006`'s trigger projected it, one event at a time.
    let projected = cache_rows(&w.conn, store);
    assert_eq!(projected.len(), 3, "the item, the tobacco and the cheese");
    assert_eq!(verify(&w.conn, store).unwrap(), []);
    let stock = StockRepository::new(&w.conn);
    assert_eq!(
        stock.on_hand(w.item, store).unwrap(),
        Qty::from_milli(9_000)
    );
    assert_eq!(
        stock.on_hand(w.tobacco, store).unwrap(),
        Qty::from_milli(-1_000)
    );
    assert_eq!(
        stock.on_hand(w.cheese, store).unwrap(),
        Qty::from_milli(4_790)
    );

    let item = w.item.as_uuid().as_bytes().to_vec();
    let cheese = w.cheese.as_uuid().as_bytes().to_vec();
    let spare = ProductId::from_uuid(uuid(SPARE));
    let corruptions: [(&str, &str, Vec<u8>, Vec<CacheDrift>); 5] = [
        (
            "an on-hand written by hand",
            "UPDATE stock_cache SET on_hand_milli = on_hand_milli + 1000 WHERE product_id = ?1",
            item.clone(),
            vec![CacheDrift::Differs {
                product_id: w.item,
                columns: vec!["on_hand_milli"],
            }],
        ),
        (
            "a projection missed, so the watermark is one event behind",
            ONE_EVENT_BEHIND,
            item,
            vec![CacheDrift::Differs {
                product_id: w.item,
                columns: vec![
                    "on_hand_milli",
                    "last_event_seq",
                    "last_event_id",
                    "event_count",
                ],
            }],
        ),
        (
            "a WAC written by hand",
            "UPDATE stock_cache SET wac_minor = 1 WHERE product_id = ?1",
            cheese.clone(),
            vec![CacheDrift::Differs {
                product_id: w.cheese,
                columns: vec!["wac_minor"],
            }],
        ),
        (
            "a row lost",
            "DELETE FROM stock_cache WHERE product_id = ?1",
            cheese,
            vec![CacheDrift::Missing(w.cheese)],
        ),
        (
            "a row no event explains",
            "INSERT INTO stock_cache (product_id, store_id, on_hand_milli)
             SELECT ?1, store_id, 5000 FROM stock_cache LIMIT 1",
            uuid(SPARE).as_bytes().to_vec(),
            vec![CacheDrift::Unexplained(spare)],
        ),
    ];
    for (what, corrupt, product, expected) in corruptions {
        assert_eq!(w.conn.execute(corrupt, [&product]).unwrap(), 1, "{what}");
        assert_ne!(cache_rows(&w.conn, store), projected, "{what}");
        assert_eq!(verify(&w.conn, store).unwrap(), expected, "{what}");

        assert_eq!(reconcile(&w.conn, store).unwrap(), expected, "{what}");
        assert_eq!(cache_rows(&w.conn, store), projected, "{what}");
        assert_eq!(verify(&w.conn, store).unwrap(), [], "{what}");
    }

    // All at once: each product named once, in product order.
    w.conn
        .execute(
            "UPDATE stock_cache SET event_count = 0 WHERE product_id = ?1",
            [w.item.as_uuid().as_bytes().as_slice()],
        )
        .unwrap();
    w.conn
        .execute(
            "DELETE FROM stock_cache WHERE product_id = ?1",
            [w.tobacco.as_uuid().as_bytes().as_slice()],
        )
        .unwrap();
    w.conn
        .execute(
            "UPDATE stock_cache SET last_event_register_id = NULL WHERE product_id = ?1",
            [w.cheese.as_uuid().as_bytes().as_slice()],
        )
        .unwrap();
    let all = vec![
        CacheDrift::Differs {
            product_id: w.item,
            columns: vec!["event_count"],
        },
        CacheDrift::Missing(w.tobacco),
        CacheDrift::Differs {
            product_id: w.cheese,
            columns: vec!["last_event_register_id"],
        },
    ];
    assert_eq!(verify(&w.conn, store).unwrap(), all);
    assert_eq!(reconcile(&w.conn, store).unwrap(), all);
    assert_eq!(cache_rows(&w.conn, store), projected);
}

#[test]
fn a_rebuild_that_fails_part_way_leaves_the_old_cache_whole() {
    let w = sold();
    let store = w.store();
    let projected = cache_rows(&w.conn, store);
    w.conn
        .execute(
            "UPDATE stock_cache SET on_hand_milli = 0 WHERE product_id = ?1",
            [w.item.as_uuid().as_bytes().as_slice()],
        )
        .unwrap();
    let corrupted = cache_rows(&w.conn, store);
    let drift = vec![CacheDrift::Differs {
        product_id: w.item,
        columns: vec!["on_hand_milli"],
    }];

    // The rebuild has deleted the store's rows when its insert meets the fault.
    w.conn
        .execute_batch(&format!(
            "CREATE TEMP TRIGGER injected_fault BEFORE INSERT ON stock_cache
             WHEN NEW.product_id = x'{}'
             BEGIN SELECT RAISE(ABORT, 'injected fault'); END;",
            w.cheese.as_uuid().simple()
        ))
        .unwrap();
    {
        let tx = w.conn.unchecked_transaction().unwrap();
        let failed = StockRepository::new(&w.conn).reconcile_cache(&tx, store);
        assert!(
            matches!(&failed, Err(DbError::Sqlite(e)) if e.to_string().contains("injected fault")),
            "{failed:?}"
        );
        // Dropped without a commit, as every caller's error path does.
    }
    assert_eq!(cache_rows(&w.conn, store), corrupted);
    assert_eq!(verify(&w.conn, store).unwrap(), drift);

    w.conn
        .execute_batch("DROP TRIGGER temp.injected_fault")
        .unwrap();
    assert_eq!(reconcile(&w.conn, store).unwrap(), drift);
    assert_eq!(cache_rows(&w.conn, store), projected);
}

#[test]
fn a_true_cache_is_verified_without_a_write() {
    let w = sold();
    let store = w.store();
    let before = cache_rows(&w.conn, store);
    let changes = w.conn.total_changes();
    assert_eq!(reconcile(&w.conn, store).unwrap(), []);
    assert_eq!(
        w.conn.total_changes(),
        changes,
        "a true cache is not rewritten"
    );
    assert_eq!(cache_rows(&w.conn, store), before);
}

#[test]
fn verification_refuses_a_ledger_holding_two_registers() {
    let w = sold();
    let store = w.store();
    // Another register's waste, recorded here: no owned sequence orders it
    // against this register's events, so no replay can place it.
    let stock = StockRepository::new(&w.conn);
    let tx = w.conn.unchecked_transaction().unwrap();
    let prepared = stock
        .prepare(
            &tx,
            RegisterId::from_uuid(uuid(OTHER_REGISTER)),
            &[NewStockEvent {
                id: StockEventId::from_uuid(uuid(0x0F)),
                product_id: w.item,
                store_id: store,
                kind: StockEventKind::Waste,
                qty_delta: Qty::from_milli(-1_000),
                incoming_cost: None,
                is_weight_derived: false,
                source: None,
                actor_id: None,
                occurred_at: at(),
                business_date: day(),
            }],
        )
        .unwrap();
    let members: Vec<(String, [u8; 16], String)> = prepared
        .iter()
        .map(|p| {
            (
                "stock_ledger".to_owned(),
                *p.event().id.as_uuid().as_bytes(),
                p.payload(),
            )
        })
        .collect();
    write_envelope(&w.conn, &tx, 0x0E, &members);
    stock.append(&tx, &prepared).unwrap();
    tx.commit().unwrap();
    let before = cache_rows(&w.conn, store);

    assert!(matches!(
        verify(&w.conn, store),
        Err(DbError::StockReplayAcrossRegisters)
    ));
    assert!(matches!(
        reconcile(&w.conn, store),
        Err(DbError::StockReplayAcrossRegisters)
    ));
    assert_eq!(cache_rows(&w.conn, store), before);
}

// ── The property ─────────────────────────────────────────────────────────────

const KEY: &str = "stock-cache-property-key";

/// The products a case moves: two sold by the unit and one weighed, each with
/// the quantity step `0006` holds its events to.
const PRODUCTS: [(u128, &str, i64, i64); 3] = [
    (0x0A, "PROP-COLA", 1_000, 0),
    (0x0B, "PROP-OIL", 1_000, 0),
    (0x0C, "PROP-CHEESE", 5, 1),
];

/// The costs an arrival is drawn from: few, so arrivals at the WAC in force are
/// common, and a blend into positive stock, which the ledger refuses, is too.
const COSTS: [i64; 3] = [0, 380, 450];

const ARRIVING: [StockEventKind; 5] = [
    StockEventKind::Receive,
    StockEventKind::RefundRestock,
    StockEventKind::Adjust,
    StockEventKind::CountCorrection,
    StockEventKind::TransferIn,
];
const LEAVING: [StockEventKind; 6] = [
    StockEventKind::Waste,
    StockEventKind::Adjust,
    StockEventKind::CountCorrection,
    StockEventKind::TransferOut,
    StockEventKind::Rtv,
    StockEventKind::KitExplode,
];
const STILL: [StockEventKind; 2] = [
    StockEventKind::RefundDamage,
    StockEventKind::CountCorrection,
];

/// One database per test thread, with a fresh store and register for every
/// case: opening an encrypted database costs most of a second, and a store is
/// what a replay is scoped to, so a case's store holds that case's ledger and
/// nothing else.
struct Shelf {
    _dir: tempfile::TempDir,
    conn: Connection,
    chain: RegisteredChain,
    cases: Cell<u128>,
}

fn shelf() -> Shelf {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("shelf.db"), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    for (id, sku, step, weighed) in PRODUCTS {
        conn.execute(
            "INSERT INTO product (id, sku, name, price_minor, currency, is_active,
                                  tax_category_id, unit, qty_step_milli, is_weighed)
             VALUES (?1, ?2, ?2, 1000, 'JOD', 1, ?3, ?4, ?5, ?6)",
            params![
                Uuid::from_u128(id).as_bytes().as_slice(),
                sku,
                &chain.tax_category,
                if weighed == 1 { "weight" } else { "each" },
                step,
                weighed
            ],
        )
        .unwrap();
    }
    Shelf {
        _dir: dir,
        conn,
        chain,
        cases: Cell::new(0),
    }
}

/// Ids a case mints, each from its case number, so no two cases share one.
const STORE_TAG: u128 = 0x5703 << 112;
const REGISTER_TAG: u128 = 0x7E61 << 112;
const COMMIT_TAG: u128 = 0xC044 << 112;
const CHANGE_TAG: u128 = 0xC4A6 << 112;
const EVENT_TAG: u128 = 0xE7E7 << 112;

impl Shelf {
    /// A new store with one register, and the case number its ids derive from.
    fn fresh_store(&self) -> (u128, StoreId, RegisterId) {
        let case = self.cases.get() + 1;
        self.cases.set(case);
        let store = Uuid::from_u128(STORE_TAG | case);
        let register = Uuid::from_u128(REGISTER_TAG | case);
        self.conn
            .execute(
                "INSERT INTO store
                   (id, org_id, code, name_ar, fiscal_obligation,
                    fiscal_obligation_evidence_ref, fiscal_profile,
                    tax_rule_pack_id, tax_computation_policy_id)
                 VALUES (?1, ?2, ?3, ?3, 'exempt', 'fixture-evidence', 'disabled', ?4, ?5)",
                params![
                    store.as_bytes().as_slice(),
                    &self.chain.org,
                    format!("PROP-S{case}"),
                    &self.chain.tax_rule_pack,
                    &self.chain.tax_policy
                ],
            )
            .unwrap();
        self.conn
            .execute(
                "INSERT INTO register
                   (id, store_id, code, name, device_id, credential_key_id,
                    credential_algorithm, credential_public_key, credential_issued_at)
                 VALUES (?1, ?2, ?3, ?3, ?3, ?3, 'ed25519', ?4, ?5)",
                params![
                    register.as_bytes().as_slice(),
                    store.as_bytes().as_slice(),
                    format!("PROP-R{case}"),
                    vec![0x99_u8; 32],
                    sale_world::AT
                ],
            )
            .unwrap();
        (
            case,
            StoreId::from_uuid(store),
            RegisterId::from_uuid(register),
        )
    }
}

/// What an arrival cost.
#[derive(Debug, Clone, Copy)]
enum Cost {
    Observed(i64),
    Projected(i64),
    Unknown,
}

/// One movement a case asks the ledger for. The kind is drawn from those that
/// move stock the movement's way, and the cost is read only for an arrival:
/// anything else carries the basis in force.
#[derive(Debug, Clone)]
struct Movement {
    product: usize,
    steps: i64,
    kind: prop::sample::Index,
    cost: Cost,
}

/// A change made to one column of every cache row in a case's store, as a
/// missed projection, an older copy or a hand would make it. Each is a change
/// on every row, whatever it held.
#[derive(Debug, Clone, Copy)]
enum Corruption {
    Set(&'static str, &'static str),
    Delete,
}

const CORRUPTIONS: [Corruption; 9] = [
    Corruption::Set("on_hand_milli", "on_hand_milli + 5"),
    Corruption::Set("wac_minor", "wac_minor + 1"),
    Corruption::Set("is_wac_estimated", "1 - is_wac_estimated"),
    Corruption::Set("last_event_register_id", "NULL"),
    Corruption::Set("last_event_seq", "last_event_seq + 1"),
    // Another event's id, or none when the database holds no other: the
    // column is a foreign key, so an id no event has would be refused.
    Corruption::Set(
        "last_event_id",
        "(SELECT e.id FROM stock_ledger e WHERE e.id <> stock_cache.last_event_id LIMIT 1)",
    ),
    Corruption::Set("event_count", "event_count + 1"),
    Corruption::Set("last_event_at", "'1970-01-01T00:00:00.000Z'"),
    Corruption::Delete,
];

/// The input space: up to eight transactions of one to four movements each,
/// over three products, of up to six steps either way or none — so a batch can
/// move one product twice, stock can go below zero and come back, and the WAC
/// resets on an arrival into stock that is not positive. Costs come from
/// `COSTS`, observed, projected or unknown. A batch the ledger refuses (a
/// different cost into positive stock) is rolled back and moves nothing.
///
/// Deliberately excluded: `sale` events. A sale event needs a whole completed
/// sale, and the projection trigger and the replay read no column that differs
/// by kind; `a_corrupted_cache_is_found_at_verification_and_rebuilt_to_the_replay`
/// replays real sales, a derived weight among them. Also excluded: a ledger of
/// two registers, which the replay refuses by name
/// (`verification_refuses_a_ledger_holding_two_registers`).
fn any_ledger() -> impl Strategy<Value = Vec<Vec<Movement>>> {
    let cost = prop_oneof![
        prop::sample::select(COSTS.to_vec()).prop_map(Cost::Observed),
        prop::sample::select(COSTS.to_vec()).prop_map(Cost::Projected),
        Just(Cost::Unknown),
    ];
    let movement = (
        0..PRODUCTS.len(),
        -6_i64..=6,
        any::<prop::sample::Index>(),
        cost,
    )
        .prop_map(|(product, steps, kind, cost)| Movement {
            product,
            steps,
            kind,
            cost,
        });
    prop::collection::vec(prop::collection::vec(movement, 1..=4), 1..=8)
}

fn product(index: usize) -> ProductId {
    ProductId::from_uuid(Uuid::from_u128(PRODUCTS.get(index).unwrap().0))
}

fn new_event(case: u128, batch: usize, slot: usize, m: &Movement, store: StoreId) -> NewStockEvent {
    let (_, _, step, _) = *PRODUCTS.get(m.product).unwrap();
    let qty = m.steps * step;
    let kind = *m.kind.get(match qty {
        q if q > 0 => &ARRIVING[..],
        q if q < 0 => &LEAVING[..],
        _ => &STILL[..],
    });
    let money = |minor: i64| Money::from_minor(minor, Currency::JOD);
    NewStockEvent {
        id: StockEventId::from_uuid(Uuid::from_u128(
            EVENT_TAG | case << 32 | (batch as u128) << 8 | slot as u128,
        )),
        product_id: product(m.product),
        store_id: store,
        kind,
        qty_delta: Qty::from_milli(qty),
        incoming_cost: (qty > 0).then_some(match m.cost {
            Cost::Observed(c) => IncomingCost::Observed(money(c)),
            Cost::Projected(c) => IncomingCost::Projected(money(c)),
            Cost::Unknown => IncomingCost::Unknown,
        }),
        is_weight_derived: false,
        source: None,
        actor_id: None,
        occurred_at: at(),
        business_date: day(),
    }
}

/// One transaction: the batch prepared, its envelope, its events. `None` when
/// the ledger refuses the batch, which then writes nothing.
fn record(
    shelf: &Shelf,
    case: u128,
    batch: usize,
    register: RegisterId,
    movements: &[NewStockEvent],
) -> Option<Vec<PreparedStockEvent>> {
    let stock = StockRepository::new(&shelf.conn);
    let tx = shelf.conn.unchecked_transaction().unwrap();
    let prepared = match stock.prepare(&tx, register, movements) {
        Ok(prepared) => prepared,
        Err(DbError::StockRefused(StockError::WacBlendNotSupported { .. })) => return None,
        Err(other) => panic!("the ledger refused {movements:?}: {other:?}"),
    };
    let ids: Vec<[u8; 16]> = prepared
        .iter()
        .map(|p| *p.event().id.as_uuid().as_bytes())
        .collect();
    let changes: Vec<[u8; 16]> = (0..prepared.len())
        .map(|slot| {
            *Uuid::from_u128(CHANGE_TAG | case << 32 | (batch as u128) << 8 | slot as u128)
                .as_bytes()
        })
        .collect();
    let payloads: Vec<String> = prepared.iter().map(PreparedStockEvent::payload).collect();
    let members: Vec<FactMember<'_>> = ids
        .iter()
        .zip(&changes)
        .zip(&payloads)
        .map(|((entity_id, change_id), payload)| FactMember {
            change_id,
            entity: "stock_ledger",
            entity_id,
            payload,
        })
        .collect();
    let commit = *Uuid::from_u128(COMMIT_TAG | case << 32 | batch as u128).as_bytes();
    OutboxRepository::new(&shelf.conn)
        .write_commit(
            &tx,
            &CommitEnvelope {
                commit_id: &commit,
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "stock-cache-property",
                created_at: sale_world::AT,
            },
            &members,
        )
        .unwrap();
    stock.append(&tx, &prepared).unwrap();
    tx.commit().unwrap();
    Some(prepared)
}

proptest! {
    // The workspace's shared I/O configuration (conventions §5.1).
    #![proptest_config(io_proptest_config())]

    /// The on-hand cache can never silently diverge from its ledger. After any
    /// sequence of events, the cache the trigger projected one event at a time
    /// is the full replay, each product's on-hand is the domain's sum of its
    /// deltas, and rebuilding leaves every row byte for byte as it was. A
    /// change to any one column of the cache is then found, named on every
    /// product it touched, and rebuilt away.
    #[test]
    fn prop_cache_rebuild_matches_ledger(
        ledger in any_ledger(),
        corruption in prop::sample::select(CORRUPTIONS.to_vec()),
    ) {
        thread_local! {
            static SHELF: Shelf = shelf();
        }
        SHELF.with(|shelf| {
            let (case, store, register) = shelf.fresh_store();
            let stock = StockRepository::new(&shelf.conn);
            let mut events: Vec<StockEvent> = Vec::new();
            for (batch, movements) in ledger.iter().enumerate() {
                let movements: Vec<NewStockEvent> = movements
                    .iter()
                    .enumerate()
                    .map(|(slot, m)| new_event(case, batch, slot, m, store))
                    .collect();
                if let Some(prepared) = record(shelf, case, batch, register, &movements) {
                    events.extend(prepared.iter().map(|p| p.event().clone()));
                }
            }

            prop_assert_eq!(verify(&shelf.conn, store).unwrap(), []);
            let mut moved: Vec<ProductId> = Vec::new();
            for index in 0..PRODUCTS.len() {
                let p = product(index);
                let theirs: Vec<StockEvent> =
                    events.iter().filter(|e| e.product_id == p).cloned().collect();
                prop_assert_eq!(stock.on_hand(p, store).unwrap(), on_hand(&theirs).unwrap());
                if !theirs.is_empty() {
                    moved.push(p);
                }
            }
            moved.sort_by_key(|p| *p.as_uuid().as_bytes());
            let projected = cache_rows(&shelf.conn, store);
            prop_assert_eq!(projected.len(), moved.len());

            let tx = shelf.conn.unchecked_transaction().unwrap();
            let written = stock.rebuild_cache(&tx, store).unwrap();
            tx.commit().unwrap();
            prop_assert_eq!(written, projected.len() as u64);
            prop_assert_eq!(&cache_rows(&shelf.conn, store), &projected);

            let (changed, expected): (String, Vec<CacheDrift>) = match corruption {
                Corruption::Set(column, to) => (
                    format!("UPDATE stock_cache SET {column} = {to} WHERE store_id = ?1"),
                    moved
                        .iter()
                        .map(|&product_id| CacheDrift::Differs {
                            product_id,
                            columns: vec![column],
                        })
                        .collect(),
                ),
                Corruption::Delete => (
                    "DELETE FROM stock_cache WHERE store_id = ?1".to_owned(),
                    moved.iter().map(|&p| CacheDrift::Missing(p)).collect(),
                ),
            };
            shelf
                .conn
                .execute(&changed, [store.as_uuid().as_bytes().as_slice()])
                .unwrap();
            prop_assert_eq!(&verify(&shelf.conn, store).unwrap(), &expected);
            prop_assert_eq!(reconcile(&shelf.conn, store).unwrap(), expected);
            prop_assert_eq!(cache_rows(&shelf.conn, store), projected);
            Ok(())
        })?;
    }
}
