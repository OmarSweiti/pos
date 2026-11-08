//! The shop the shell's suites sell in (`cart.rs`, `tender.rs`).
//!
//! Every fixture is a fresh register file built through `pos_db::open`, the
//! registered migration chain: the reference chain's store, registered for
//! GST, with this install's register, an open shift, a synthetic cashier,
//! four synthetic products with opening stock, and a verified deli scale. The
//! register is opened the way `run` opens it. A crash is a register dropped
//! without signing out, and the next one opened on the same file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::product::ProductRepository;
use pos_db::repo::trade_scale::{ENTITY, TradeScaleRepository, VerificationEvent};
use pos_db::seed::opening_stock::{OpeningStockLoad, OpeningStockRow, load_opening_stock};
use pos_domain::{
    Barcode, BarcodeKind, BusinessDate, Cart, Currency, FixedClock, Money, Product, ProductId, Qty,
    RegisterId, RegulatedSaleForm, SaleForm, SaleId, SeqIdSource, StoreId, TaxCategoryId,
    Timestamp, UnitOfMeasure, VerificationState,
};
use rusqlite::{Connection, params};
use terminal_lib::commands::auth::login_pin;
use terminal_lib::commands::cart::{CartSnapshot, LineSnapshot};
use terminal_lib::ipc::IpcError;
use terminal_lib::state::{DATABASE_FILE, DEVICE_ID_FILE, Register, V7Ids};
use uuid::Uuid;

pub const KEY: &str = "cart-suite-key";
pub const AT: &str = "2026-11-05T09:00:00.000Z";
pub const DAY: &str = "2026-11-05";
pub const DEVICE_ID: &str = "device-REG01";
pub const REGISTER_ID: [u8; 16] = [0x52; 16];
pub const SHIFT_ID: [u8; 16] = [0x5F; 16];
pub const CASHIER_ID: [u8; 16] = [0xC1; 16];
pub const CASHIER_CODE: &str = "C-100";
pub const CASHIER_PIN: &str = "482916";
pub const SECOND_ID: [u8; 16] = [0xC2; 16];
pub const SECOND_CODE: &str = "C-200";
pub const OWNER_ID: [u8; 16] = [0x0E; 16];
pub const OWNER_CODE: &str = "O-100";
pub const LEAD_ID: [u8; 16] = [0x5E; 16];
pub const LEAD_CODE: &str = "L-100";
pub const MANAGER_ID: [u8; 16] = [0xAA; 16];
pub const MANAGER_CODE: &str = "M-100";

pub const OIL: u128 = 1;
pub const CHEESE: u128 = 2;
pub const TOBACCO: u128 = 3;
pub const BREAD: u128 = 4;
pub const SCALE: u128 = 0x40;
pub const OIL_BOTTLE: &str = "6291041500213";
pub const OIL_SIX: &str = "6291041500220";
pub const CHEESE_PLU: &str = "34567";

pub fn at() -> Timestamp {
    Timestamp::parse_iso8601(AT).unwrap()
}

pub fn id(value: u128) -> [u8; 16] {
    Uuid::from_u128(value).into_bytes()
}

pub fn product_id(value: u128) -> ProductId {
    ProductId::from_uuid(Uuid::from_u128(value))
}

pub fn jod(minor: i64) -> Money {
    Money::from_minor(minor, Currency::JOD)
}

/// GS1's check digit, written out here rather than borrowed from the code
/// under test: from the rightmost data digit leftwards, weights 3, 1, 3, 1…
pub fn with_check(first_twelve: &str) -> String {
    let sum: u32 = first_twelve
        .chars()
        .rev()
        .enumerate()
        .map(|(i, c)| c.to_digit(10).unwrap() * if i % 2 == 0 { 3 } else { 1 })
        .sum();
    format!("{first_twelve}{}", (10 - sum % 10) % 10)
}

/// A deli label: `prefix`, five digits of item code, five of value, check.
pub fn label(prefix: &str, item: &str, value: i64) -> String {
    with_check(&format!("{prefix}{item}{value:05}"))
}

pub fn pin_hash() -> String {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| pos_db::auth::hash_pin(CASHIER_PIN).unwrap())
        .clone()
}

pub fn product(chain: &RegisteredChain, n: u128, sku: &str, name_ar: &str, price: i64) -> Product {
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
pub fn seed(dir: &Path, shift: bool) {
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
pub fn deli_scale(conn: &Connection, chain: &RegisteredChain) {
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

pub fn open(dir: &Path) -> Register {
    Register::open_with(
        dir,
        KEY,
        Box::new(FixedClock::new(at())),
        Box::new(V7Ids::default()),
    )
    .unwrap()
}

/// A shop whose cashier is signed in, with no cart open yet.
pub fn shop() -> (tempfile::TempDir, Register) {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), true);
    let register = open(dir.path());
    login_pin(&register, CASHIER_CODE, CASHIER_PIN).unwrap();
    (dir, register)
}

/// A second synthetic account in the cashier's organisation, under `role`,
/// with the same synthetic PIN.
pub fn add_staff(register: &Register, id: [u8; 16], code: &str, name: &str, role: &str) {
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
pub fn states(register: &Register) -> Vec<(String, SaleId)> {
    saved(register)
        .into_iter()
        .map(|(state, cart)| (state, cart.id))
        .collect()
}

pub fn refusal<T: std::fmt::Debug>(result: Result<T, IpcError>) -> IpcError {
    match result {
        Ok(answer) => panic!("the command was expected to be refused, and answered {answer:?}"),
        Err(refusal) => refusal,
    }
}

/// Every row of the parked-cart table, as `(state, its cart)`.
pub fn saved(register: &Register) -> Vec<(String, Cart)> {
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
pub fn audited(register: &Register, action: &str) -> Vec<serde_json::Value> {
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
pub fn field<'p>(payload: &'p serde_json::Value, name: &str) -> &'p serde_json::Value {
    payload.get(name).unwrap()
}

pub fn the_line(snapshot: &CartSnapshot) -> &LineSnapshot {
    assert_eq!(snapshot.lines.len(), 1, "the cart holds one line");
    snapshot.lines.first().unwrap()
}
