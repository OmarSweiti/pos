//! Migration `0008`: the manual-discount caps the owner adopted (merchant
//! decisions 3.1–3.3, microstep 1.4.5), and reading them back.
//!
//! The fixture builds a database at v7, so `0008` runs over the cells `0004`
//! shipped, exactly as it will on a register that has been selling. A cell a
//! merchant has already changed must come through untouched: the migration
//! writes the vendor default, never over the merchant's own answer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::DbError;
use pos_db::repo::role::RoleRepository;
use pos_domain::permissions::{Capability, Role, cap};
use pos_domain::{DiscountCap, Percent};
use rusqlite::{Connection, params};

const KEY: &str = "migration-0008-test-key";

/// A register database at v7, the version before `0008`, at `path`.
fn at_v7(path: &std::path::Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.pragma_update(None, "key", KEY).unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    for (index, sql) in [
        include_str!("../migrations/0001_init.sql"),
        include_str!("../migrations/0002_sale_integrity.sql"),
        include_str!("../migrations/0003_strict_rebuild_and_catalog_depth.sql"),
        include_str!("../migrations/0004_people_and_audit.sql"),
        include_str!("../migrations/0005_sale_columns_and_sequences.sql"),
        include_str!("../migrations/0006_stock_ledger.sql"),
        include_str!("../migrations/0007_search_and_seed.sql"),
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
    conn
}

/// `role`'s `discount.manual` cell: its decision and its limit.
fn cell(conn: &Connection, role: Role) -> (String, Option<String>) {
    conn.query_row(
        "SELECT rc.decision, rc.limit_json
           FROM role_capability AS rc JOIN role AS r ON r.id = rc.role_id
          WHERE r.code = ?1 AND rc.capability = ?2",
        params![role.as_str(), cap::DiscountManual::NAME],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

fn set_cell(conn: &Connection, role: Role, decision: &str, limit_json: Option<&str>) {
    let changed = conn
        .execute(
            "UPDATE role_capability SET decision = ?1, limit_json = ?2
              WHERE role_id = (SELECT id FROM role WHERE code = ?3) AND capability = ?4",
            params![
                decision,
                limit_json,
                role.as_str(),
                cap::DiscountManual::NAME
            ],
        )
        .unwrap();
    assert_eq!(changed, 1, "{role:?}'s discount.manual cell");
}

fn granted(limit_json: Option<&str>) -> (String, Option<String>) {
    ("granted".to_owned(), limit_json.map(str::to_owned))
}

#[test]
fn migration_0008_writes_the_adopted_caps_over_0004_s_cells() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("register.db");
    let old = at_v7(&path);
    // The shape 0004 shipped: a cashier capped by a number nobody had given,
    // and a shift lead not capped at all.
    assert_eq!(
        cell(&old, Role::Cashier),
        granted(Some(r#"{"kind":"role_cap"}"#))
    );
    assert_eq!(cell(&old, Role::ShiftLead), granted(None));
    old.close().map_err(|(_, e)| e).unwrap();

    let conn = pos_db::open(&path, KEY).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 8);
    assert_eq!(
        cell(&conn, Role::Cashier),
        granted(Some(r#"{"kind":"role_cap","max_percent_ppm":50000}"#))
    );
    assert_eq!(
        cell(&conn, Role::ShiftLead),
        granted(Some(r#"{"kind":"role_cap","max_percent_ppm":150000}"#))
    );
    // The manager's cell is unlimited and the owner's sets the limits, as before.
    assert_eq!(cell(&conn, Role::Manager), granted(None));
    assert_eq!(
        cell(&conn, Role::Owner),
        ("sets_the_limit".to_owned(), None)
    );
}

#[test]
fn migration_0008_leaves_a_merchant_s_edited_cell_as_set() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("register.db");
    let old = at_v7(&path);
    // Before 0008 runs, the merchant has given the cashier a 2% cap and taken
    // manual discounts away from the shift lead.
    set_cell(
        &old,
        Role::Cashier,
        "granted",
        Some(r#"{"kind":"role_cap","max_percent_ppm":20000}"#),
    );
    set_cell(&old, Role::ShiftLead, "withheld", None);
    old.close().map_err(|(_, e)| e).unwrap();

    let conn = pos_db::open(&path, KEY).unwrap();
    assert_eq!(
        cell(&conn, Role::Cashier),
        granted(Some(r#"{"kind":"role_cap","max_percent_ppm":20000}"#))
    );
    assert_eq!(cell(&conn, Role::ShiftLead), ("withheld".to_owned(), None));
}

#[test]
fn each_role_reads_back_its_discount_cap() {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    let roles = RoleRepository::new(&conn);
    assert_eq!(
        roles.discount_cap(Role::Cashier).unwrap(),
        Some(DiscountCap::AtMost(Percent::from_ppm(50_000)))
    );
    assert_eq!(
        roles.discount_cap(Role::ShiftLead).unwrap(),
        Some(DiscountCap::AtMost(Percent::from_ppm(150_000)))
    );
    assert_eq!(
        roles.discount_cap(Role::Manager).unwrap(),
        Some(DiscountCap::Unlimited)
    );
    // The owner sets the caps and runs no till: no cap, because no grant.
    assert_eq!(roles.discount_cap(Role::Owner).unwrap(), None);
    // A merchant who withholds the capability withholds the discount.
    set_cell(&conn, Role::ShiftLead, "withheld", None);
    assert_eq!(roles.discount_cap(Role::ShiftLead).unwrap(), None);
}

#[test]
fn a_discount_cell_that_cannot_be_read_as_a_cap_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    let roles = RoleRepository::new(&conn);
    // Each would be a guess about how much a cashier may give away, so none is
    // read as a cap, and none falls back to "unlimited".
    for (limit_json, why) in [
        (r#"{"kind":"role_cap"}"#, "a role cap with no number"),
        (
            r#"{"kind":"role_cap","max_percent_ppm":"50000"}"#,
            "a number written as text",
        ),
        (
            r#"{"kind":"role_cap","max_percent_ppm":5.5}"#,
            "a fraction of a ppm",
        ),
        (
            r#"{"kind":"role_cap","max_percent_ppm":-1}"#,
            "a negative cap",
        ),
        (
            r#"{"kind":"role_cap","max_percent_ppm":1000001}"#,
            "a cap above 100%",
        ),
        (r#"{"kind":"own_shift"}"#, "another kind of limit"),
        ("not json", "a column that is not JSON"),
    ] {
        set_cell(&conn, Role::Cashier, "granted", Some(limit_json));
        let refused = roles.discount_cap(Role::Cashier);
        assert!(
            matches!(refused, Err(DbError::StoredRoleLimitInvalid { .. })),
            "{why}: {refused:?}"
        );
    }
    // The boundaries themselves are caps: nothing at all, and everything.
    for ppm in [0, 1_000_000] {
        let limit = format!(r#"{{"kind":"role_cap","max_percent_ppm":{ppm}}}"#);
        set_cell(&conn, Role::Cashier, "granted", Some(&limit));
        assert_eq!(
            roles.discount_cap(Role::Cashier).unwrap(),
            Some(DiscountCap::AtMost(Percent::from_ppm(ppm)))
        );
    }
    // A cell nobody decided is not a cap of zero either.
    conn.execute(
        "DELETE FROM role_capability
          WHERE role_id = (SELECT id FROM role WHERE code = ?1) AND capability = ?2",
        params![Role::Manager.as_str(), cap::DiscountManual::NAME],
    )
    .unwrap();
    assert!(matches!(
        roles.discount_cap(Role::Manager),
        Err(DbError::StoredRoleLimitInvalid { .. })
    ));
}
