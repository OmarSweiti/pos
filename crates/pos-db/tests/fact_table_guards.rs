//! I-4 and I-6, enumerated from one place.
//!
//! The schema calls eleven tables facts. Until migration 0002 there were
//! immutability triggers on three of them, and `audit_log`'s own DDL asserted
//! "Append-only: no UPDATE, no DELETE, ever" with nothing enforcing it — so the
//! only forensic control in the design was the one an insider could edit.
//!
//! The list below is the single source of truth. A fact table added by a later
//! migration and not added here is not covered, so the rule is written down in
//! `ref/schema.md` beside the triggers: a new fact table gets a row here in the
//! same commit that creates it. A table that IS listed and lacks its guards
//! fails this test the moment its migration ships.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::Connection;

/// Every fact table, and whether its UPDATE guard is unconditional.
///
/// `sale_tender` is the one deliberate exception on UPDATE: a semi-integrated
/// card capture settles asynchronously, so the row still moves after the sale
/// completes — but its `amount_minor` is frozen. Its guard is therefore
/// conditional by design, which is why this table records that fact rather than
/// asserting a blanket refusal.
const FACT_TABLES: &[(&str, &str)] = &[
    ("sale", "frozen once completed"),
    ("sale_line", "frozen once completed"),
    (
        "sale_tender",
        "amount frozen once completed; state may still settle",
    ),
    ("sale_line_tax", "frozen once the parent sale completes"),
    (
        "sale_line_discount",
        "frozen once the parent sale completes",
    ),
    ("sale_tax_summary", "frozen once the parent sale completes"),
    ("audit_log", "append-only, unconditionally"),
    ("stock_ledger", "append-only, unconditionally (I-6)"),
    ("cash_movement", "append-only, unconditionally"),
    ("z_report", "append-only, unconditionally"),
    ("drawer_event", "append-only, unconditionally"),
];

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

/// Does `table` carry a BEFORE <verb> trigger that can abort?
fn guarded(conn: &Connection, table: &str, verb: &str) -> bool {
    let mut stmt = conn
        .prepare("SELECT sql FROM sqlite_master WHERE type = 'trigger' AND tbl_name = ?1")
        .unwrap();
    let needle = format!("before {} on {}", verb.to_lowercase(), table);
    stmt.query_map([table], |r| r.get::<_, Option<String>>(0))
        .unwrap()
        .filter_map(Result::ok)
        .flatten()
        .any(|sql| {
            let lowered = sql.to_lowercase();
            let normalized: String = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
            normalized.contains(&needle) && normalized.contains("raise(abort")
        })
}

#[test]
fn every_shipped_fact_table_refuses_update_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("facts.db"), "test-key").unwrap();

    let mut checked = 0;
    let mut gaps = Vec::new();

    for (table, rule) in FACT_TABLES {
        if !table_exists(&conn, table) {
            continue; // its migration has not shipped yet
        }
        checked += 1;
        for verb in ["UPDATE", "DELETE"] {
            if !guarded(&conn, table, verb) {
                gaps.push(format!("{table}: no BEFORE {verb} guard — {rule}"));
            }
        }
    }

    assert!(
        gaps.is_empty(),
        "fact tables in the shipped schema without immutability guards:\n  {}",
        gaps.join("\n  ")
    );
    assert!(
        checked >= 3,
        "expected at least the three tables 0002 guards; checked {checked}"
    );
}

#[test]
fn the_fact_table_list_has_no_duplicates_and_names_nothing_twice() {
    let mut names: Vec<&str> = FACT_TABLES.iter().map(|(t, _)| *t).collect();
    let before = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(before, names.len(), "FACT_TABLES lists a table twice");
}

#[test]
fn a_fact_table_that_does_not_exist_yet_is_not_silently_counted() {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("absent.db"), "test-key").unwrap();

    // Guard against the test passing because it checked nothing: tables from
    // later migrations must genuinely be absent, not typos that skip silently.
    assert!(table_exists(&conn, "sale"), "sale must exist at 0002");
    assert!(
        !table_exists(&conn, "audit_log"),
        "audit_log arrives in 0004; if it exists, update this test's expectations"
    );
}
