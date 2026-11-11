#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The merchant's settings, read against the exact registered migration
//! chain: a store's own list of price-override reasons, or the default; and a
//! register's printer binding (1.7.14b).

use pos_db::DbError;
use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::setting::{
    DEFAULT_PRICE_OVERRIDE_REASONS, PRICE_OVERRIDE_REASONS, PRINTER_BINDING, PrinterBinding,
    SettingRepository, is_reason_code,
};
use pos_domain::{RegisterId, StoreId};
use rusqlite::{Connection, params};
use uuid::Uuid;

const KEY: &str = "setting-suite-key";

/// A register file holding one registered store, and the store.
fn store() -> (tempfile::TempDir, Connection, StoreId) {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    let store = StoreId::from_uuid(Uuid::from_slice(&chain.store).unwrap());
    (dir, conn, store)
}

/// Set `key` to `value_json` at `scope`.
fn set(conn: &Connection, scope: &str, scope_id: Uuid, key: &str, value_json: &str) {
    conn.execute(
        "INSERT INTO setting (scope, scope_id, key, value_json) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (scope, scope_id, key) DO UPDATE SET value_json = excluded.value_json",
        params![scope, scope_id.as_bytes().as_slice(), key, value_json],
    )
    .unwrap();
}

fn reasons(conn: &Connection, store: StoreId) -> Result<Vec<String>, DbError> {
    SettingRepository::new(conn).price_override_reasons(store)
}

#[test]
fn a_store_that_set_no_reasons_gets_the_default() {
    let (_dir, conn, store) = store();
    assert_eq!(
        reasons(&conn, store).unwrap(),
        [
            "displayed_price",
            "damaged",
            "price_match",
            "catalogue_error"
        ]
    );
    assert_eq!(
        reasons(&conn, store).unwrap(),
        DEFAULT_PRICE_OVERRIDE_REASONS
    );

    // A list set for the organisation, for a register or for another store
    // is not this store's.
    let org: Vec<u8> = conn
        .query_row(
            "SELECT org_id FROM store WHERE id = ?1",
            [store.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    let narrow = r#"["damaged"]"#;
    set(
        &conn,
        "org",
        Uuid::from_slice(&org).unwrap(),
        PRICE_OVERRIDE_REASONS,
        narrow,
    );
    set(
        &conn,
        "register",
        store.as_uuid(),
        PRICE_OVERRIDE_REASONS,
        narrow,
    );
    set(
        &conn,
        "store",
        Uuid::from_u128(0x057),
        PRICE_OVERRIDE_REASONS,
        narrow,
    );
    assert_eq!(
        reasons(&conn, store).unwrap(),
        DEFAULT_PRICE_OVERRIDE_REASONS
    );
}

#[test]
fn a_stores_own_list_replaces_the_default_and_always_keeps_the_shelf_price() {
    let (_dir, conn, store) = store();
    let id = store.as_uuid();

    set(
        &conn,
        "store",
        id,
        PRICE_OVERRIDE_REASONS,
        r#"["price_match", "loyalty_2026"]"#,
    );
    assert_eq!(
        reasons(&conn, store).unwrap(),
        ["displayed_price", "price_match", "loyalty_2026"]
    );
    // Listed anywhere, the shelf's price is still first, and once.
    set(
        &conn,
        "store",
        id,
        PRICE_OVERRIDE_REASONS,
        r#"["damaged", "displayed_price"]"#,
    );
    assert_eq!(
        reasons(&conn, store).unwrap(),
        ["displayed_price", "damaged"]
    );
    // An empty list leaves the shelf's price alone.
    set(&conn, "store", id, PRICE_OVERRIDE_REASONS, "[]");
    assert_eq!(reasons(&conn, store).unwrap(), ["displayed_price"]);
}

#[test]
fn a_malformed_list_is_refused_never_read_as_the_default() {
    let (_dir, conn, store) = store();
    for (value, why) in [
        ("not json", "not JSON"),
        (r#"{"damaged": true}"#, "not a list"),
        (r#""damaged""#, "not a list"),
        (r#"["damaged", 7]"#, "an entry is not text"),
        (r#"["Damaged"]"#, "an entry is not a reason code"),
        (r#"["damaged box"]"#, "an entry is not a reason code"),
        (r#"["damaged", "damaged"]"#, "a code is listed twice"),
    ] {
        set(
            &conn,
            "store",
            store.as_uuid(),
            PRICE_OVERRIDE_REASONS,
            value,
        );
        match reasons(&conn, store) {
            Err(DbError::StoredSettingInvalid { key, reason }) => {
                assert_eq!((key, reason), (PRICE_OVERRIDE_REASONS, why), "{value}");
            }
            other => panic!("{value} was read as {other:?}"),
        }
    }
}

#[test]
fn a_reason_code_is_a_short_lowercase_identifier() {
    for code in [
        "damaged",
        "price_match",
        "catalogue_error",
        "a",
        "loyalty_2026",
    ] {
        assert!(is_reason_code(code), "{code}");
    }
    let longest = "a".repeat(32);
    assert!(is_reason_code(&longest));
    for code in [
        "",
        "Damaged",
        "2026_promo",
        "_damaged",
        "damaged box",
        "damaged-box",
        "ضرر",
        "damaged\n",
        &"a".repeat(33),
    ] {
        assert!(!is_reason_code(code), "{code:?}");
    }
}

/// 1.7.14b: a register's printer binding is its own row, and is checked as it
/// is read. None set reads as none, and so does a binding set for another
/// register, for the store or for the organisation. A value that is not
/// exactly a profile id, a version from 1 and an IP address with a port is
/// refused by name, never read as no binding: a register whose binding was
/// mistyped must say so, not quietly print nowhere.
#[test]
fn a_printer_binding_is_the_registers_own_and_is_checked_as_it_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    let store = StoreId::from_uuid(Uuid::from_slice(&chain.store).unwrap());
    let chain_register = [0x52u8; 16];
    let other_register = [0x53u8; 16];
    chain.add_register(&conn, &chain_register, "REG01");
    chain.add_register(&conn, &other_register, "REG02");
    let register = RegisterId::from_uuid(Uuid::from_bytes(chain_register));
    let binding = |conn: &Connection| SettingRepository::new(conn).printer_binding(register);
    assert_eq!(binding(&conn).unwrap(), None);

    let bound = r#"{"profile_id": "emulator-80mm", "version": 1, "address": "192.0.2.10:9100"}"#;
    set(
        &conn,
        "register",
        Uuid::from_bytes(other_register),
        PRINTER_BINDING,
        bound,
    );
    set(&conn, "store", store.as_uuid(), PRINTER_BINDING, bound);
    assert_eq!(
        binding(&conn).unwrap(),
        None,
        "another register's, or the store's, is not this one's"
    );

    set(
        &conn,
        "register",
        register.as_uuid(),
        PRINTER_BINDING,
        bound,
    );
    assert_eq!(
        binding(&conn).unwrap(),
        Some(PrinterBinding {
            profile_id: "emulator-80mm".to_owned(),
            version: core::num::NonZeroU32::MIN,
            address: "192.0.2.10:9100".parse().unwrap(),
        })
    );

    for (value, why) in [
        ("not json", "not a JSON object"),
        (r#"["emulator-80mm", 1]"#, "not a JSON object"),
        (
            r#"{"profile_id": "emulator-80mm", "version": 1}"#,
            "not exactly profile_id, version and address",
        ),
        (
            r#"{"profile_id": "emulator-80mm", "version": 1, "address": "192.0.2.10:9100", "port": 9100}"#,
            "not exactly profile_id, version and address",
        ),
        (
            r#"{"profile_id": 7, "version": 1, "address": "192.0.2.10:9100"}"#,
            "profile_id is not text",
        ),
        (
            r#"{"profile_id": "emulator-80mm", "version": 0, "address": "192.0.2.10:9100"}"#,
            "version is not a whole number from 1",
        ),
        (
            r#"{"profile_id": "emulator-80mm", "version": "1", "address": "192.0.2.10:9100"}"#,
            "version is not a whole number from 1",
        ),
        (
            r#"{"profile_id": "emulator-80mm", "version": 1, "address": "printer.local:9100"}"#,
            "address is not an IP address and a port",
        ),
        (
            r#"{"profile_id": "emulator-80mm", "version": 1, "address": "192.0.2.10"}"#,
            "address is not an IP address and a port",
        ),
    ] {
        set(
            &conn,
            "register",
            register.as_uuid(),
            PRINTER_BINDING,
            value,
        );
        match binding(&conn) {
            Err(DbError::StoredSettingInvalid { key, reason }) => {
                assert_eq!((key, reason), (PRINTER_BINDING, why), "{value}");
            }
            other => panic!("{value} was read as {other:?}"),
        }
    }
}
