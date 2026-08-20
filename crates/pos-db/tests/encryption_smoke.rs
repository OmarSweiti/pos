//! Integration test: unwrap is the assertion style here.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use uuid::Uuid;

#[test]
fn encrypted_db_roundtrip_and_wrong_key_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("register.db");

    // 1. Create, migrate, write.
    {
        let conn = pos_db::open(&path, "correct-key").unwrap();
        let id = Uuid::now_v7();
        conn.execute(
            "INSERT INTO product (id, sku, name, price_minor, currency) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id.as_bytes().as_slice(), "SKU-001", "Espresso", 250_i64, "JOD"],
        )
        .unwrap();
    }

    // 2. Wrong key must be rejected before any query succeeds.
    assert!(matches!(
        pos_db::open(&path, "wrong-key"),
        Err(pos_db::DbError::BadKey)
    ));

    // 3. Right key reads the data back.
    let conn = pos_db::open(&path, "correct-key").unwrap();
    let n: i64 = conn
        .query_row("SELECT count(*) FROM product", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);

    // 4. Migrations are idempotent (reopen ran them again as a no-op).
    let v: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(v, pos_db::SCHEMA_VERSION);
}
