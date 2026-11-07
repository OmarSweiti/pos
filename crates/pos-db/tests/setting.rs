#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The merchant's settings, read against the exact registered migration
//! chain: a store's own list of price-override reasons, or the default.

use pos_db::DbError;
use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::setting::{
    DEFAULT_PRICE_OVERRIDE_REASONS, PRICE_OVERRIDE_REASONS, SettingRepository, is_reason_code,
};
use pos_domain::StoreId;
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
