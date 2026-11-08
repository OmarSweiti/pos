#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! What the finalize reads of a store (1.8.3), against the exact registered
//! migration chain: the merchant its receipt names, the language it prints
//! in, and whether it may complete a live sale at all.

use pos_db::DbError;
use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::store::{FiscalDecision, StoreRepository};
use pos_domain::{Language, StoreId};
use rusqlite::{Connection, params};
use uuid::Uuid;

const KEY: &str = "store-suite-key";

fn store() -> (tempfile::TempDir, Connection, StoreId) {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    let id = StoreId::from_uuid(Uuid::from_slice(&chain.store).unwrap());
    (dir, conn, id)
}

fn set(conn: &Connection, assignments: &str) {
    conn.execute(&format!("UPDATE store SET {assignments}"), [])
        .unwrap();
}

/// The fixture's store is exempt with its evidence: a live sale may complete.
/// Its organisation names the merchant, and its receipts are set in Arabic
/// unless it says otherwise.
#[test]
fn a_store_reads_with_its_merchant_and_language() {
    let (_dir, conn, id) = store();
    conn.execute(
        "UPDATE org SET legal_name = 'مؤسسة اصطناعية', tin = '000000000'",
        [],
    )
    .unwrap();
    set(
        &conn,
        "name_en = 'Synthetic Store', address = 'Amman', phone = 'SYN-PHONE-0000', \
         gst_profile = 'general_sales'",
    );
    let profile = StoreRepository::new(&conn).profile(id).unwrap();
    assert_eq!(profile.legal_name, "مؤسسة اصطناعية");
    assert_eq!(profile.tin.as_deref(), Some("000000000"));
    assert_eq!(profile.receipt_language, Language::Arabic);
    assert_eq!(profile.name_in(Language::Arabic), "متجر الاختبار");
    assert_eq!(profile.name_in(Language::English), "Synthetic Store");
    assert!(profile.gst_registered);
    assert_eq!(profile.fiscal, FiscalDecision::Exempt);
    assert!(
        !format!("{profile:?}").contains("SYN-PHONE-0000"),
        "the phone never reaches Debug"
    );

    set(&conn, "receipt_locale = 'en', gst_profile = 'unregistered'");
    let profile = StoreRepository::new(&conn).profile(id).unwrap();
    assert_eq!(profile.receipt_language, Language::English);
    assert!(!profile.gst_registered);
}

/// The fiscal decision is `0005`'s trigger rule: required with JoFotara
/// enabled and the taxpayer type known, or exempt with JoFotara disabled,
/// each with its evidence. Anything short of either is undecided.
#[test]
fn the_fiscal_decision_is_the_rule_the_schema_completes_a_sale_under() {
    let (_dir, conn, id) = store();
    let read = |conn: &Connection| StoreRepository::new(conn).profile(id).unwrap().fiscal;
    set(
        &conn,
        "fiscal_obligation = 'pending_evidence', fiscal_obligation_evidence_ref = NULL",
    );
    assert_eq!(read(&conn), FiscalDecision::Undecided);
    set(
        &conn,
        "fiscal_obligation = 'required', fiscal_obligation_evidence_ref = 'synthetic', \
         fiscal_profile = 'jordan_jofotara', fiscal_taxpayer_type = 'general_sales'",
    );
    assert_eq!(read(&conn), FiscalDecision::Required);
    set(
        &conn,
        "fiscal_obligation = 'exempt', fiscal_profile = 'disabled', fiscal_taxpayer_type = NULL",
    );
    assert_eq!(read(&conn), FiscalDecision::Exempt);
}

/// A store this database does not hold, and a receipt language this build
/// does not print in, are each refused by name.
#[test]
fn an_unknown_store_and_an_unprintable_language_are_refused() {
    let (_dir, conn, id) = store();
    let elsewhere = StoreId::from_uuid(Uuid::from_u128(0xE15E));
    assert!(matches!(
        StoreRepository::new(&conn).profile(elsewhere),
        Err(DbError::StoreUnknown(found)) if found == elsewhere
    ));
    conn.execute("UPDATE store SET receipt_locale = ?1", params!["fr"])
        .unwrap();
    assert!(matches!(
        StoreRepository::new(&conn).profile(id),
        Err(DbError::StoredStoreInvalid { .. })
    ));
}
