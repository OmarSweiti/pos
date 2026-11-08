#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The tender kinds a register takes, read against the exact registered
//! migration chain: `0005`'s seed, and a row switched off or on.

use pos_db::repo::tender_type::TenderTypeRepository;
use pos_domain::standard_tender_types;
use rusqlite::Connection;

const KEY: &str = "tender-type-suite-key";

fn register() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    (dir, conn)
}

fn switch(conn: &Connection, code: &str, active: bool) {
    let changed = conn
        .execute(
            "UPDATE tender_type SET is_active = ?1 WHERE code = ?2",
            rusqlite::params![i64::from(active), code],
        )
        .unwrap();
    assert_eq!(changed, 1, "{code} has a row");
}

#[test]
fn a_fresh_register_takes_cash_and_no_other_kind() {
    let (_dir, conn) = register();
    let kinds = TenderTypeRepository::new(&conn);
    let taken: Vec<String> = standard_tender_types()
        .into_iter()
        .map(|kind| kind.code)
        .filter(|code| kinds.is_active(code).unwrap())
        .collect();
    assert_eq!(taken, vec!["cash".to_owned()]);
    // A code the seed does not hold is not taken.
    assert!(!kinds.is_active("gold").unwrap());
    assert!(!kinds.is_active("").unwrap());
}

#[test]
fn a_kind_is_taken_exactly_while_its_row_is_active() {
    let (_dir, conn) = register();
    switch(&conn, "cash", false);
    switch(&conn, "card", true);
    let kinds = TenderTypeRepository::new(&conn);
    assert!(!kinds.is_active("cash").unwrap());
    assert!(kinds.is_active("card").unwrap());
    // The code is matched exactly, never by case or by prefix.
    assert!(!kinds.is_active("CARD").unwrap());
    assert!(!kinds.is_active("car").unwrap());
}

/// 1.8.3's finalize prints each tender under its kind's name and opens the
/// drawer for a kind that says so. Every kind the seed holds is read, taken
/// or not, because a completed sale prints the kinds it was paid in.
#[test]
fn every_kind_reads_with_its_printed_names_and_whether_it_opens_the_drawer() {
    let (_dir, conn) = register();
    let kinds = TenderTypeRepository::new(&conn).kinds().unwrap();
    let codes: Vec<&str> = kinds.iter().map(|kind| kind.code.as_str()).collect();
    assert_eq!(
        codes,
        ["cash", "card", "cliq", "voucher", "store_credit", "exchange"],
        "every seeded kind, in the grid's order"
    );
    let cash = kinds.iter().find(|kind| kind.code == "cash").unwrap();
    assert_eq!(cash.name_ar, "نقدي");
    assert_eq!(cash.name_en.as_deref(), Some("Cash"));
    assert!(cash.opens_drawer, "cash opens the drawer");
    assert!(
        kinds.iter().filter(|kind| kind.opens_drawer).count() == 1,
        "only cash does"
    );
}
