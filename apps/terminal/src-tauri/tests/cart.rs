//! Building a basket through the cart commands, and setting it aside and
//! bringing it back (microstep 1.4.11, its first two parts).
//!
//! Every fixture is a fresh register file built through `pos_db::open`, the
//! registered migration chain: the reference chain's store, registered for
//! GST, with this install's register, an open shift, a synthetic cashier,
//! four synthetic products with opening stock, and a verified deli scale. The
//! register is opened the way `run` opens it, and every command is driven
//! through the plain function its Tauri wrapper calls. A crash is a register
//! dropped without signing out, and the next one opened on the same file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::parked_cart::ParkedCartRepository;
use pos_db::repo::product::ProductRepository;
use pos_db::repo::trade_scale::{ENTITY, TradeScaleRepository, VerificationEvent};
use pos_db::seed::opening_stock::{OpeningStockLoad, OpeningStockRow, load_opening_stock};
use pos_domain::{
    Barcode, BarcodeKind, BusinessDate, Cart, ClockConfidence, Currency, FixedClock, Money,
    Product, ProductId, Qty, RegisterId, RegulatedSaleForm, SaleForm, SaleId, SeqIdSource, StoreId,
    TaxCategoryId, Timestamp, UnitOfMeasure, UserId, VerificationState,
};
use rusqlite::{Connection, params};
use terminal_lib::commands::auth::{login_pin, logout};
use terminal_lib::commands::cart::{
    CartSnapshot, PARK_LABEL_MAX_CHARS, ParkedRef, add_product, add_scan, answer_age, current,
    list_parked, open_cart, park_cart, resume_cart,
};
use terminal_lib::ipc::IpcError;
use terminal_lib::state::{DATABASE_FILE, DEVICE_ID_FILE, Register, V7Ids};
use uuid::Uuid;

const KEY: &str = "cart-suite-key";
const AT: &str = "2026-11-05T09:00:00.000Z";
const DAY: &str = "2026-11-05";
const DEVICE_ID: &str = "device-REG01";
const REGISTER_ID: [u8; 16] = [0x52; 16];
const SHIFT_ID: [u8; 16] = [0x5F; 16];
const CASHIER_ID: [u8; 16] = [0xC1; 16];
const CASHIER_CODE: &str = "C-100";
const CASHIER_PIN: &str = "482916";
const SECOND_ID: [u8; 16] = [0xC2; 16];
const SECOND_CODE: &str = "C-200";
const OWNER_ID: [u8; 16] = [0x0E; 16];
const OWNER_CODE: &str = "O-100";

const OIL: u128 = 1;
const CHEESE: u128 = 2;
const TOBACCO: u128 = 3;
const BREAD: u128 = 4;
const SCALE: u128 = 0x40;
const OIL_BOTTLE: &str = "6291041500213";
const OIL_SIX: &str = "6291041500220";
const CHEESE_PLU: &str = "34567";

fn at() -> Timestamp {
    Timestamp::parse_iso8601(AT).unwrap()
}

fn id(value: u128) -> [u8; 16] {
    Uuid::from_u128(value).into_bytes()
}

fn product_id(value: u128) -> ProductId {
    ProductId::from_uuid(Uuid::from_u128(value))
}

fn jod(minor: i64) -> Money {
    Money::from_minor(minor, Currency::JOD)
}

/// GS1's check digit, written out here rather than borrowed from the code
/// under test: from the rightmost data digit leftwards, weights 3, 1, 3, 1…
fn with_check(first_twelve: &str) -> String {
    let sum: u32 = first_twelve
        .chars()
        .rev()
        .enumerate()
        .map(|(i, c)| c.to_digit(10).unwrap() * if i % 2 == 0 { 3 } else { 1 })
        .sum();
    format!("{first_twelve}{}", (10 - sum % 10) % 10)
}

/// A deli label: `prefix`, five digits of item code, five of value, check.
fn label(prefix: &str, item: &str, value: i64) -> String {
    with_check(&format!("{prefix}{item}{value:05}"))
}

fn pin_hash() -> String {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| pos_db::auth::hash_pin(CASHIER_PIN).unwrap())
        .clone()
}

fn product(chain: &RegisteredChain, n: u128, sku: &str, name_ar: &str, price: i64) -> Product {
    Product {
        id: product_id(n),
        sku: sku.to_owned(),
        name_ar: name_ar.to_owned(),
        name_en: None,
        category_id: None,
        tax_category_id: TaxCategoryId::from_uuid(Uuid::from_slice(&chain.tax_category).unwrap()),
        unit: UnitOfMeasure::Each,
        unit_price: jod(price),
        is_weighed: false,
        is_service: false,
        is_active: true,
        min_age: None,
        max_price: None,
        reorder_point: None,
        regulated_sale_form: RegulatedSaleForm::UNREGULATED_SEALED_PACK,
    }
}

/// A register file in `dir`: the store registered for GST, this install's
/// register with an open shift when `shift` is set, the cashier, four
/// products and their opening stock, and a verified deli scale whose layout
/// "22" reads price labels.
fn seed(dir: &Path, shift: bool) {
    let conn = pos_db::open(&dir.join(DATABASE_FILE), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    chain.add_register(&conn, &REGISTER_ID, "REG01");
    if shift {
        chain.open_shift(&conn, &SHIFT_ID, &REGISTER_ID, DAY);
    }
    conn.execute(
        "UPDATE store SET gst_profile = 'general_sales' WHERE id = ?1",
        [&chain.store],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
         VALUES (?1, ?2, ?3, 'Synthetic Cashier', ?4, ?5)",
        params![
            CASHIER_ID.as_slice(),
            &chain.org,
            CASHIER_CODE,
            pin_hash(),
            AT
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO user_role (user_id, role_id, store_id)
         SELECT ?1, id, NULL FROM role WHERE code = 'cashier'",
        [CASHIER_ID.as_slice()],
    )
    .unwrap();

    let tx = conn.unchecked_transaction().unwrap();
    let products = ProductRepository::new(&conn);
    products
        .upsert(&tx, &product(&chain, OIL, "SYN-OIL-1L", "زيت زيتون", 1_250))
        .unwrap();
    products
        .upsert(
            &tx,
            &Product {
                unit: UnitOfMeasure::Kilogram,
                is_weighed: true,
                regulated_sale_form: RegulatedSaleForm::Unregulated(SaleForm::Bulk),
                ..product(&chain, CHEESE, "SYN-CHEESE", "جبنة بيضاء", 8_500)
            },
        )
        .unwrap();
    products
        .upsert(
            &tx,
            &Product {
                min_age: Some(18),
                ..product(&chain, TOBACCO, "SYN-TOBACCO", "تبغ اصطناعي", 2_500)
            },
        )
        .unwrap();
    products
        .upsert(&tx, &product(&chain, BREAD, "SYN-BREAD", "خبز", 350))
        .unwrap();
    for (row, code, units) in [(0x100, OIL_BOTTLE, 1), (0x101, OIL_SIX, 6)] {
        products
            .add_barcode(
                &tx,
                &id(row),
                &Barcode {
                    product_id: product_id(OIL),
                    code: code.to_owned(),
                    kind: BarcodeKind::Ean13,
                    is_primary: units == 1,
                    pack_qty: Qty::from_units(units).unwrap(),
                },
            )
            .unwrap();
    }
    tx.commit().unwrap();
    conn.execute(
        "INSERT INTO plu_code (code, product_id) VALUES (?1, ?2)",
        params![CHEESE_PLU, id(CHEESE).as_slice()],
    )
    .unwrap();

    // Ten bottles of oil, five packs of tobacco and half a kilogram of cheese.
    // No bread at all.
    let ids = SeqIdSource::new(1_788_000_000_000, 0x0B);
    let tx = conn.unchecked_transaction().unwrap();
    load_opening_stock(
        &tx,
        &OpeningStockLoad {
            register: RegisterId::from_uuid(Uuid::from_bytes(REGISTER_ID)),
            store: StoreId::from_uuid(Uuid::from_slice(&chain.store).unwrap()),
            actor: None,
            occurred_at: at(),
            business_date: BusinessDate::parse(DAY).unwrap(),
            envelope: CommitEnvelope {
                commit_id: &[0x0B; 16],
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "cart-suite",
                created_at: AT,
            },
            ids: &ids,
        },
        [(OIL, 10_000), (TOBACCO, 5_000), (CHEESE, 500)].map(|(n, qty_milli)| OpeningStockRow {
            product_id: product_id(n),
            qty_milli,
            unit_cost_minor: Some(100),
        }),
    )
    .unwrap();
    tx.commit().unwrap();

    deli_scale(&conn, &chain);
    drop(conn);
    fs::write(dir.join(DEVICE_ID_FILE), format!("{DEVICE_ID}\n")).unwrap();
}

/// A deli scale, verified for the year, whose layout "22" reads price labels
/// (`ref/schema.md` §0007). Its evidence travels in its own envelope (I-9).
fn deli_scale(conn: &Connection, chain: &RegisteredChain) {
    conn.execute(
        "INSERT INTO trade_scale (id, store_id, maker, model, serial_number)
         VALUES (?1, ?2, 'Synthetic Scales', 'Deli-15', 'SN-0001')",
        params![id(SCALE).as_slice(), chain.store],
    )
    .unwrap();
    let evidence_ref = "JSMO-CERT-SYNTHETIC-1".to_owned();
    let event = VerificationEvent {
        id: id(0x41),
        trade_scale_id: id(SCALE),
        event_no: 1,
        state: VerificationState::Verified,
        evidence_hash: *blake3::hash(evidence_ref.as_bytes()).as_bytes(),
        evidence_ref,
        seal_or_mark_ref: None,
        effective_at: Timestamp::parse_iso8601("2026-01-01T00:00:00.000Z").unwrap(),
        valid_until: Some(Timestamp::parse_iso8601("2027-01-01T00:00:00.000Z").unwrap()),
    };
    let tx = conn.unchecked_transaction().unwrap();
    let payload = event.payload();
    OutboxRepository::new(&tx)
        .write_commit(
            &tx,
            &CommitEnvelope {
                commit_id: &id(0x9041),
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "cart-suite",
                created_at: AT,
            },
            &[FactMember {
                change_id: &id(0xA041),
                entity: ENTITY,
                entity_id: &event.id,
                payload: &payload,
            }],
        )
        .unwrap();
    TradeScaleRepository::new(conn)
        .record_verification(&tx, &event)
        .unwrap();
    tx.commit().unwrap();
    conn.execute(
        "INSERT INTO embedded_barcode_rule
           (id, store_id, trade_scale_id, prefix, item_code_start, item_code_len,
            value_start, value_len, value_kind, value_scale, verify_checksum, is_active)
         VALUES (?1, ?2, ?3, '22', 2, 5, 7, 5, 'price_minor', 1, 1, 1)",
        params![id(0x50).as_slice(), chain.store, id(SCALE).as_slice()],
    )
    .unwrap();
}

fn open(dir: &Path) -> Register {
    Register::open_with(
        dir,
        KEY,
        Box::new(FixedClock::new(at())),
        Box::new(V7Ids::default()),
    )
    .unwrap()
}

/// A shop whose cashier is signed in, with no cart open yet.
fn shop() -> (tempfile::TempDir, Register) {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    (dir, register)
}

/// A second synthetic account in the cashier's organisation, under `role`,
/// with the same synthetic PIN.
fn add_staff(register: &Register, id: [u8; 16], code: &str, name: &str, role: &str) {
    register.with_db(|conn| {
        conn.execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             SELECT ?1, org_id, ?2, ?3, ?4, ?5 FROM app_user WHERE id = ?6",
            params![
                id.as_slice(),
                code,
                name,
                pin_hash(),
                AT,
                CASHIER_ID.as_slice()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO user_role (user_id, role_id, store_id)
             SELECT ?1, id, NULL FROM role WHERE code = ?2",
            params![id.as_slice(), role],
        )
        .unwrap();
    });
}

/// Every parked-cart row, as `(state, its cart's id)`, in the order `saved`
/// reads them.
fn states(register: &Register) -> Vec<(String, SaleId)> {
    saved(register)
        .into_iter()
        .map(|(state, cart)| (state, cart.id))
        .collect()
}

fn refusal<T: std::fmt::Debug>(result: Result<T, IpcError>) -> IpcError {
    match result {
        Ok(answer) => panic!("the command was expected to be refused, and answered {answer:?}"),
        Err(refusal) => refusal,
    }
}

/// Every row of the parked-cart table, as `(state, its cart)`.
fn saved(register: &Register) -> Vec<(String, Cart)> {
    register.with_db(|conn| {
        conn.prepare("SELECT state, snapshot FROM parked_cart ORDER BY parked_at, id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get::<_, String>(1)?)))
            .unwrap()
            .map(|row| {
                let (state, snapshot) = row.unwrap();
                (state, serde_json::from_str(&snapshot).unwrap())
            })
            .collect()
    })
}

/// The audit rows written for `action`, as their JSON payloads.
fn audited(register: &Register, action: &str) -> Vec<serde_json::Value> {
    register.with_db(|conn| {
        conn.prepare("SELECT payload FROM audit_log WHERE action = ?1 ORDER BY rowid")
            .unwrap()
            .query_map([action], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|payload| serde_json::from_str(&payload.unwrap()).unwrap())
            .collect()
    })
}

/// One field of an audit payload, which must be there.
fn field<'p>(payload: &'p serde_json::Value, name: &str) -> &'p serde_json::Value {
    payload.get(name).unwrap()
}

fn the_line(snapshot: &CartSnapshot) -> &terminal_lib::commands::cart::LineSnapshot {
    assert_eq!(snapshot.lines.len(), 1, "the cart holds one line");
    snapshot.lines.first().unwrap()
}

#[test]
fn a_cart_opens_on_the_open_shift_and_is_held_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());

    // No one is signed in: nothing opens.
    assert_eq!(refusal(open_cart(&register)).code, "not_signed_in");
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(current(&register).unwrap(), None);

    let opened = open_cart(&register).unwrap();
    assert!(opened.lines.is_empty());
    assert_eq!(
        (opened.currency, opened.total_minor, opened.tax_total_minor),
        ("JOD", 0, 0)
    );
    // Asking again answers with the same cart; a second press opens nothing.
    assert_eq!(open_cart(&register).unwrap(), opened);
    assert_eq!(current(&register).unwrap(), Some(opened.clone()));

    // It is on disk from its first moment, claimed, on the open shift.
    let rows = saved(&register);
    assert_eq!(rows.len(), 1);
    let (state, cart) = rows.first().unwrap();
    assert_eq!(state, "active");
    assert_eq!(cart.id, opened.sale_id);
    assert_eq!(cart.shift_id.as_uuid().as_bytes(), &SHIFT_ID);
    assert_eq!(cart.cashier_id.as_uuid().as_bytes(), &CASHIER_ID);
    assert!(!cart.is_training);
}

#[test]
fn a_register_with_no_open_shift_opens_no_cart() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), false);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(refusal(open_cart(&register)).code, "shift_not_open");
    assert!(saved(&register).is_empty());
    // And no line can be added to a cart that is not open.
    assert_eq!(
        refusal(add_product(&register, product_id(OIL), None)).code,
        "cart_not_open"
    );
}

#[test]
fn a_line_is_the_catalogues_price_and_a_refused_change_changes_nothing() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();

    // One bottle by default, at the catalogue's 1.250, named as the catalogue
    // names it, and priced under the store's own settings: prices include
    // tax, so the line's total is its price and its tax is inside it.
    let one = add_product(&register, product_id(OIL), None).unwrap();
    let line = the_line(&one);
    assert_eq!(
        (
            line.product_id,
            line.name.as_str(),
            line.qty_milli,
            line.unit_price_minor
        ),
        (Some(product_id(OIL)), "زيت زيتون", 1_000, 1_250)
    );
    assert_eq!((line.gross_minor, line.line_total_minor), (1_250, 1_250));
    assert!(line.tax_minor > 0, "a registered store charges GST");
    assert_eq!(one.total_minor, 1_250);
    assert_eq!(one.tax_total_minor, line.tax_minor);
    assert_eq!(
        one.tax_rows.iter().map(|row| row.tax_minor).sum::<i64>(),
        one.tax_total_minor
    );

    // Three more, as a quantity.
    let four = add_product(&register, product_id(OIL), Some(3_000)).unwrap();
    assert_eq!(four.lines.len(), 2);
    assert_eq!(four.subtotal_minor, 5_000);
    assert_eq!(four.total_minor, 5_000);

    // Each refusal names itself, and leaves the cart and its saved copy as
    // they were.
    register.with_db(|conn| {
        conn.execute(
            "UPDATE product SET is_active = 0 WHERE id = ?1",
            [id(BREAD).as_slice()],
        )
        .unwrap();
    });
    for (product, qty, code) in [
        (product_id(0xDEAD), None, "product_unknown"),
        (product_id(OIL), Some(1_500), "qty_not_whole"),
        (product_id(OIL), Some(0), "qty_not_positive"),
        (product_id(BREAD), None, "product_inactive"),
    ] {
        assert_eq!(
            refusal(add_product(&register, product, qty)).code,
            code,
            "{code}"
        );
    }
    assert_eq!(current(&register).unwrap(), Some(four.clone()));
    let rows = saved(&register);
    let (_, cart) = rows.first().unwrap();
    assert_eq!(cart.lines.len(), 2);
}

#[test]
fn a_scan_adds_what_its_code_names_and_names_every_other_answer() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let trusted = ClockConfidence::Trusted;

    // The bottle's own code adds one; the six-pack's outer code adds six of
    // the same product, at its catalogue price (E.78).
    add_scan(&register, OIL_BOTTLE, trusted).unwrap();
    let both = add_scan(&register, OIL_SIX, trusted).unwrap();
    let quantities: Vec<(Option<ProductId>, i64, i64)> = both
        .lines
        .iter()
        .map(|line| (line.product_id, line.qty_milli, line.unit_price_minor))
        .collect();
    assert_eq!(
        quantities,
        vec![
            (Some(product_id(OIL)), 1_000, 1_250),
            (Some(product_id(OIL)), 6_000, 1_250)
        ]
    );
    assert_eq!(both.total_minor, 8_750);

    // Every scan that adds no line says why, each its own code.
    let good = label("22", CHEESE_PLU, 416);
    let damaged: String = good
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 8 {
                if c == '9' { '8' } else { '9' }
            } else {
                c
            }
        })
        .collect();
    for (raw, code) in [
        ("6290000000006", "scan_unknown"),
        (damaged.as_str(), "scan_checksum_failed"),
        ("", "scan_unreadable"),
    ] {
        assert_eq!(
            refusal(add_scan(&register, raw, trusted)).code,
            code,
            "{raw:?}"
        );
    }
    assert_eq!(current(&register).unwrap(), Some(both));
}

#[test]
fn a_price_label_prices_its_line_only_from_a_trusted_clock() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let code = label("22", CHEESE_PLU, 416);

    // The register holds no trusted time yet, so its own clock confidence
    // stops every scale label, by the scale's reason, never as an unknown
    // code to quick-add. A plain barcode still sells.
    assert_eq!(register.clock_confidence(), ClockConfidence::Untrusted);
    let refused = refusal(add_scan(&register, &code, register.clock_confidence()));
    assert_eq!(refused.code, "scan_scale_refused");
    assert_eq!(
        refused.detail,
        Some("the register's clock is not trusted, so the scale's verification cannot be judged")
    );
    add_scan(&register, OIL_BOTTLE, register.clock_confidence()).unwrap();

    // From a trusted clock, the label is what is charged: one package of
    // cheese at 0.416, whatever its weight (E.80). No argument carried that
    // price; the scan handler read it from the label.
    let priced = add_scan(&register, &code, ClockConfidence::Trusted).unwrap();
    let cheese = priced
        .lines
        .iter()
        .find(|line| line.product_id == Some(product_id(CHEESE)))
        .unwrap();
    assert_eq!(
        (
            cheese.qty_milli,
            cheese.unit,
            cheese.unit_price_minor,
            cheese.line_total_minor
        ),
        (1_000, UnitOfMeasure::Package, 416, 416)
    );
    assert_eq!(priced.total_minor, 1_250 + 416);
    // Its stock is the weight its price implies, 49 g at 8.500 a kilogram,
    // not its one package: half a kilogram of cheese covers it.
    assert!(priced.stock_flags.is_empty());
}

#[test]
fn an_age_restricted_line_waits_for_an_operator_and_each_answer_is_audited() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();

    // The tobacco goes in blocked: it stays on screen, waiting for an
    // operator's answer (E.69, 1.4.3).
    let blocked = add_product(&register, product_id(TOBACCO), None).unwrap();
    let line = the_line(&blocked).clone();
    assert_eq!((line.min_age, line.age_confirmed), (Some(18), false));
    assert_eq!(blocked.awaiting_age, vec![line.line_id]);

    // Yes unblocks it, and is audited with the cart's saving.
    let confirmed = answer_age(&register, line.line_id, true).unwrap();
    assert!(confirmed.awaiting_age.is_empty());
    assert!(the_line(&confirmed).age_confirmed);
    assert_eq!(
        refusal(answer_age(&register, line.line_id, true)).code,
        "age_not_pending",
        "a second answer would be a second audit row for one decision"
    );

    // No removes the second pack, and is audited too.
    let second = add_product(&register, product_id(TOBACCO), None).unwrap();
    let waiting = second.awaiting_age.first().copied().unwrap();
    let declined = answer_age(&register, waiting, false).unwrap();
    assert_eq!(declined.lines, confirmed.lines);
    assert!(declined.awaiting_age.is_empty());

    let answers: Vec<(String, bool)> = ["age.confirm", "age.decline"]
        .into_iter()
        .flat_map(|action| {
            audited(&register, action).into_iter().map(move |payload| {
                (
                    action.to_owned(),
                    field(&payload, "confirmed").as_bool().unwrap(),
                )
            })
        })
        .collect();
    assert_eq!(
        answers,
        vec![
            ("age.confirm".to_owned(), true),
            ("age.decline".to_owned(), false)
        ]
    );
    // Every audit row went out with its envelope (I-9).
    let unenveloped: i64 = register.with_db(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM audit_log a
              WHERE a.action LIKE 'age.%'
                AND NOT EXISTS (SELECT 1 FROM fact_commit_member m WHERE m.entity_id = a.id)",
            [],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert_eq!(unenveloped, 0);
    assert_eq!(
        refusal(answer_age(
            &register,
            pos_domain::SaleLineId::from_uuid(Uuid::from_u128(0xBAD)),
            true
        ))
        .code,
        "line_unknown"
    );
}

#[test]
fn stock_is_screened_as_a_line_is_added() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();

    // The store allows and flags by default (C.7): bread, of which there is
    // none, is sold and flagged loudly.
    let flagged = add_product(&register, product_id(BREAD), None).unwrap();
    assert_eq!(flagged.stock_flags.len(), 1);
    let flag = flagged.stock_flags.first().unwrap();
    assert_eq!(
        (flag.product_id, flag.on_hand_milli, flag.taken_milli),
        (product_id(BREAD), 0, 1_000)
    );
    // Within stock, nothing is flagged for it.
    let oil = add_product(&register, product_id(OIL), Some(10_000)).unwrap();
    assert_eq!(oil.stock_flags.len(), 1, "only the bread");

    // Under a hard block, a line that would take a product below zero is
    // refused, and the cart is as it was.
    register.with_db(|conn| {
        conn.execute("UPDATE store SET allow_negative_stock = 0", [])
            .unwrap();
    });
    assert_eq!(
        refusal(add_product(&register, product_id(OIL), None)).code,
        "stock_would_go_negative"
    );
    let unchanged = current(&register).unwrap().unwrap();
    assert_eq!(unchanged.lines, oil.lines);
    // A basket that is already short is still shown, with its flag.
    assert_eq!(unchanged.stock_flags, oil.stock_flags);
}

#[test]
fn every_change_is_on_disk_and_a_crash_sets_the_basket_aside() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    let basket = add_product(&register, product_id(TOBACCO), None).unwrap();

    // The saved copy is the basket the cashier is looking at.
    let rows = saved(&register);
    let (state, cart) = rows.first().unwrap();
    assert_eq!(state, "active");
    let saved_lines: Vec<_> = cart.lines.iter().map(|line| line.id).collect();
    let shown_lines: Vec<_> = basket.lines.iter().map(|line| line.line_id).collect();
    assert_eq!(saved_lines, shown_lines);

    // The process dies with nobody signing out. The next one finds the
    // basket and sets it aside in the parked list, where whoever signs in
    // can resume it; the till itself opens empty.
    drop(register);
    let restarted = open(dir.path());
    assert_eq!(restarted.recovered().carts_set_aside, 1);
    assert_eq!(restarted.recovered().empty_carts_dropped, 0);
    let parked = restarted.with_db(|conn| {
        ParkedCartRepository::new(conn)
            .parked(restarted.register(), BusinessDate::parse(DAY).unwrap())
            .unwrap()
    });
    assert_eq!(parked.len(), 1);
    let entry = parked.first().unwrap();
    assert_eq!(entry.id, basket.sale_id);
    assert_eq!(entry.cart.cart().lines.len(), 2);
    login_pin(&restarted, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(current(&restarted).unwrap(), None);

    // A cart left empty by a crash is dropped instead.
    let fresh = open_cart(&restarted).unwrap();
    drop(restarted);
    let again = open(dir.path());
    assert_eq!(again.recovered().empty_carts_dropped, 1);
    assert!(
        saved(&again)
            .iter()
            .all(|(_, cart)| cart.id != fresh.sale_id)
    );
}

#[test]
fn signing_out_sets_the_basket_aside_and_drops_an_empty_one() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let basket = add_product(&register, product_id(OIL), None).unwrap();

    logout(&register).unwrap();
    let rows = saved(&register);
    assert_eq!(rows.len(), 1);
    let (state, cart) = rows.first().unwrap();
    assert_eq!((state.as_str(), cart.id), ("parked", basket.sale_id));
    let logouts = audited(&register, "auth.logout");
    let payload = logouts.first().unwrap();
    assert_eq!(*field(payload, "cause"), "requested");
    assert_eq!(
        *field(payload, "parked_cart"),
        basket.sale_id.as_uuid().to_string()
    );

    // Signed in again, the till is empty: the basket waits to be resumed.
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(current(&register).unwrap(), None);

    // An empty basket is not kept.
    let empty = open_cart(&register).unwrap();
    logout(&register).unwrap();
    assert!(
        saved(&register)
            .iter()
            .all(|(_, cart)| cart.id != empty.sale_id)
    );
    let logouts = audited(&register, "auth.logout");
    assert_eq!(
        *field(logouts.last().unwrap(), "parked_cart"),
        serde_json::Value::Null
    );
}

#[test]
fn a_deactivated_cashier_is_signed_out_and_their_basket_set_aside() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let basket = add_product(&register, product_id(OIL), None).unwrap();

    register.with_db(|conn| {
        conn.execute(
            "UPDATE app_user SET is_active = 0 WHERE id = ?1",
            [CASHIER_ID.as_slice()],
        )
        .unwrap();
    });
    // The next command finds the account no longer live, signs it out, and
    // the basket goes to the parked list rather than with the session.
    assert_eq!(
        refusal(add_product(&register, product_id(OIL), None)).code,
        "user_inactive"
    );
    let rows = saved(&register);
    let (state, cart) = rows.first().unwrap();
    assert_eq!((state.as_str(), cart.id), ("parked", basket.sale_id));
    assert_eq!(cart.lines.len(), 1);
    let logouts = audited(&register, "auth.logout");
    let payload = logouts.first().unwrap();
    assert_eq!(*field(payload, "cause"), "user_inactive");
    assert_eq!(
        *field(payload, "parked_cart"),
        basket.sale_id.as_uuid().to_string()
    );
    assert_eq!(
        current(&register).map_err(|refused| refused.code),
        Err("not_signed_in")
    );
}

// ── 1.4.11, its second part: setting a basket aside and bringing it back ──

#[test]
fn a_parked_basket_leaves_the_till_free_and_comes_back_as_it_was() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), Some(2_000)).unwrap();
    let basket = add_product(&register, product_id(BREAD), None).unwrap();

    // Parked under a label, the basket leaves the till, and the till is free
    // for the next customer.
    let parked = park_cart(&register, Some("  Blue bag  ")).unwrap();
    assert_eq!(
        parked,
        ParkedRef {
            parked_id: basket.sale_id,
            label: Some("Blue bag".to_owned()),
            parked_by: UserId::from_uuid(Uuid::from_bytes(CASHIER_ID)),
            parked_by_name: "Synthetic Cashier".to_owned(),
            parked_at: at(),
            line_count: 2,
            currency: "JOD",
            total_minor: basket.total_minor,
        }
    );
    assert_eq!(current(&register).unwrap(), None);
    assert_eq!(
        states(&register),
        vec![("parked".to_owned(), basket.sale_id)]
    );
    assert_eq!(list_parked(&register).unwrap(), vec![parked]);

    // A resume onto an empty cart drops it. The basket comes back exactly as
    // it was shown, and a cashier resuming their own writes nothing.
    let empty = open_cart(&register).unwrap();
    assert_eq!(resume_cart(&register, basket.sale_id).unwrap(), basket);
    assert_eq!(current(&register).unwrap(), Some(basket.clone()));
    assert_eq!(
        states(&register),
        vec![("active".to_owned(), basket.sale_id)]
    );
    assert!(list_parked(&register).unwrap().is_empty());
    assert!(
        saved(&register)
            .iter()
            .all(|(_, cart)| cart.id != empty.sale_id)
    );

    // A resume onto a basket with lines parks that basket, in the same step.
    park_cart(&register, None).unwrap();
    open_cart(&register).unwrap();
    let other = add_product(&register, product_id(OIL), None).unwrap();
    assert_eq!(resume_cart(&register, basket.sale_id).unwrap(), basket);
    let listed = list_parked(&register).unwrap();
    assert_eq!(listed.len(), 1);
    let waiting = listed.first().unwrap();
    assert_eq!(
        (
            waiting.parked_id,
            waiting.label.as_deref(),
            waiting.line_count,
            waiting.total_minor
        ),
        (other.sale_id, None, 1, other.total_minor)
    );
    assert!(audited(&register, "sale.resume").is_empty());
}

#[test]
fn parking_names_what_it_refuses_and_changes_nothing() {
    let (_dir, register) = shop();
    assert_eq!(refusal(park_cart(&register, None)).code, "cart_not_open");
    let empty = open_cart(&register).unwrap();
    assert_eq!(refusal(park_cart(&register, None)).code, "nothing_to_park");
    assert_eq!(current(&register).unwrap(), Some(empty));

    let basket = add_product(&register, product_id(OIL), None).unwrap();
    let too_long = "ب".repeat(PARK_LABEL_MAX_CHARS + 1);
    for label in [too_long.as_str(), "Blue\nbag", "Blue\u{7}bag"] {
        assert_eq!(
            refusal(park_cart(&register, Some(label))).code,
            "park_label_invalid"
        );
    }
    assert_eq!(current(&register).unwrap(), Some(basket.clone()));
    assert_eq!(
        states(&register),
        vec![("active".to_owned(), basket.sale_id)]
    );

    // At the limit a label is taken whole, counted in characters in any
    // script; a blank one is no label.
    let longest = "ب".repeat(PARK_LABEL_MAX_CHARS);
    assert_eq!(
        park_cart(&register, Some(&longest)).unwrap().label,
        Some(longest)
    );
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    assert_eq!(park_cart(&register, Some("   ")).unwrap().label, None);
}

#[test]
fn another_cashiers_basket_is_resumed_on_the_record() {
    let (_dir, register) = shop();
    add_staff(
        &register,
        SECOND_ID,
        SECOND_CODE,
        "Synthetic Second Cashier",
        "cashier",
    );
    open_cart(&register).unwrap();
    add_product(&register, product_id(OIL), None).unwrap();
    let basket = add_product(&register, product_id(TOBACCO), None).unwrap();
    assert_eq!(basket.awaiting_age.len(), 1);
    // The first cashier signs out, which sets the basket aside.
    logout(&register).unwrap();

    login_pin(&register, SECOND_CODE, CASHIER_PIN).unwrap();
    let listed = list_parked(&register).unwrap();
    assert_eq!(listed.len(), 1);
    let waiting = listed.first().unwrap();
    assert_eq!(
        (
            waiting.parked_id,
            waiting.parked_by_name.as_str(),
            waiting.line_count
        ),
        (basket.sale_id, "Synthetic Cashier", 2)
    );

    // The second cashier takes it over: the basket comes back as it was, its
    // age gate still waiting, and is theirs from now on.
    assert_eq!(resume_cart(&register, basket.sale_id).unwrap(), basket);
    let rows = saved(&register);
    let (state, cart) = rows.first().unwrap();
    assert_eq!(
        (state.as_str(), cart.cashier_id),
        ("active", UserId::from_uuid(Uuid::from_bytes(SECOND_ID)))
    );

    // The handover is on the record, naming who parked it and what it held,
    // and went out with its envelope (I-9).
    let takeovers = audited(&register, "sale.resume");
    assert_eq!(takeovers.len(), 1);
    let payload = takeovers.first().unwrap();
    assert_eq!(
        *field(payload, "parked_by"),
        Uuid::from_bytes(CASHIER_ID).to_string()
    );
    assert_eq!(*field(payload, "line_count"), 2);
    let (actor, entity_id, enveloped): (Vec<u8>, Vec<u8>, i64) = register.with_db(|conn| {
        conn.query_row(
            "SELECT a.actor_id, a.entity_id,
                    EXISTS (SELECT 1 FROM fact_commit_member m WHERE m.entity_id = a.id)
               FROM audit_log a WHERE a.action = 'sale.resume'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
    });
    assert_eq!(actor, SECOND_ID.to_vec());
    assert_eq!(entity_id, basket.sale_id.as_uuid().as_bytes().to_vec());
    assert_eq!(enveloped, 1);
}

#[test]
fn a_refused_resume_leaves_both_baskets_where_they_were() {
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let parked = add_product(&register, product_id(OIL), None).unwrap();
    park_cart(&register, None).unwrap();
    open_cart(&register).unwrap();
    let working = add_product(&register, product_id(BREAD), None).unwrap();
    let before = saved(&register);

    // No such basket.
    let unknown = SaleId::from_uuid(Uuid::from_u128(0xBAD));
    assert_eq!(
        refusal(resume_cart(&register, unknown)).code,
        "parked_cart_unknown"
    );
    // A basket another till is working on.
    let claim = |state: &str, nonce: Option<[u8; 16]>| {
        register.with_db(|conn| {
            conn.execute(
                "UPDATE parked_cart SET state = ?1, session_nonce = ?2 WHERE id = ?3",
                params![
                    state,
                    nonce.map(|n| n.to_vec()),
                    parked.sale_id.as_uuid().as_bytes().as_slice()
                ],
            )
            .unwrap();
        });
    };
    claim("active", Some([0xEE; 16]));
    assert_eq!(
        refusal(resume_cart(&register, parked.sale_id)).code,
        "parked_cart_claimed"
    );
    claim("parked", None);
    // A basket parked on an earlier business day has expired, and is no
    // longer offered.
    register.with_db(|conn| {
        conn.execute(
            "UPDATE parked_cart SET expires_on = '2026-11-04' WHERE id = ?1",
            [parked.sale_id.as_uuid().as_bytes().as_slice()],
        )
        .unwrap();
    });
    assert_eq!(
        refusal(resume_cart(&register, parked.sale_id)).code,
        "parked_cart_expired"
    );
    assert!(list_parked(&register).unwrap().is_empty());

    // Each refusal left the till's basket on the till, unparked, and the
    // parked one where it was.
    assert_eq!(current(&register).unwrap(), Some(working));
    let after = saved(&register);
    assert_eq!(after.len(), before.len());
    for ((state, cart), (was_state, was)) in after.iter().zip(&before) {
        assert_eq!(
            (state, cart.id, &cart.lines),
            (was_state, was.id, &was.lines)
        );
    }
    assert!(audited(&register, "sale.resume").is_empty());
}

#[test]
fn parking_and_resuming_need_a_shift_and_a_role_that_sells() {
    // Outside a shift nothing can be listed or resumed.
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), false);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(refusal(list_parked(&register)).code, "shift_not_open");
    let anything = SaleId::from_uuid(Uuid::from_u128(0xBAD));
    assert_eq!(
        refusal(resume_cart(&register, anything)).code,
        "shift_not_open"
    );

    // An owner sells nothing, so parks and resumes nothing (§8.2's grid).
    let (_dir, register) = shop();
    open_cart(&register).unwrap();
    let basket = add_product(&register, product_id(OIL), None).unwrap();
    logout(&register).unwrap();
    add_staff(&register, OWNER_ID, OWNER_CODE, "Synthetic Owner", "owner");
    login_pin(&register, OWNER_CODE, CASHIER_PIN).unwrap();
    assert_eq!(
        refusal(park_cart(&register, None)).code,
        "permission_denied"
    );
    assert_eq!(refusal(list_parked(&register)).code, "permission_denied");
    assert_eq!(
        refusal(resume_cart(&register, basket.sale_id)).code,
        "permission_denied"
    );
    assert_eq!(
        states(&register),
        vec![("parked".to_owned(), basket.sale_id)]
    );
}

#[test]
fn a_resumed_basket_changed_and_crashed_comes_back_as_it_was_left() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    open_cart(&register).unwrap();
    let basket = add_product(&register, product_id(OIL), None).unwrap();
    park_cart(&register, Some("Blue bag")).unwrap();
    resume_cart(&register, basket.sale_id).unwrap();
    // A change to the resumed basket is saved as any other.
    let changed = add_product(&register, product_id(BREAD), None).unwrap();

    // The process dies with nobody signing out. The next start-up sets the
    // basket aside again, under its label, with the line added after it was
    // resumed.
    drop(register);
    let restarted = open(dir.path());
    assert_eq!(restarted.recovered().carts_set_aside, 1);
    login_pin(&restarted, CASHIER_CODE, CASHIER_PIN).unwrap();
    let listed = list_parked(&restarted).unwrap();
    assert_eq!(listed.len(), 1);
    let waiting = listed.first().unwrap();
    assert_eq!(
        (
            waiting.parked_id,
            waiting.label.as_deref(),
            waiting.line_count,
            waiting.total_minor
        ),
        (basket.sale_id, Some("Blue bag"), 2, changed.total_minor)
    );
    assert_eq!(resume_cart(&restarted, basket.sale_id).unwrap(), changed);
}
