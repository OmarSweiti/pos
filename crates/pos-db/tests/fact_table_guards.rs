//! I-4 and I-6, proven by attempting the mutation — not by reading trigger text.
//!
//! The first version of this file searched `sqlite_master` for a `BEFORE <verb>`
//! trigger containing `RAISE(ABORT)`. That was a presence check masquerading as a
//! proof: a trigger guarded by `WHEN 0` would have satisfied it, and because the
//! eight fact tables added by migrations 0003-0011 do not exist in the shipped
//! chain yet, it silently skipped every one of them — so it validated none of the
//! guards it was written to defend.
//!
//! This version applies the schema reference on top of the shipped chain and then
//! tries the forbidden writes. A vacuous trigger fails here, because the write
//! succeeds.
//!
//! The fact-table list is parsed out of `ref/schema.md` itself, from the
//! `<!-- fact-tables: ... -->` marker beside the prose that names them. The
//! document is the single source; a table added to one and not the other fails.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{blob, declared_fact_tables, full_schema, seed};
use rusqlite::{Connection, types::Value};

/// Sale A stays parked and holds an editable copy of every sale-scoped detail
/// row. Sale B gets its own copy while still parked, and is then completed — so
/// B's rows are the frozen ones. Everything is seeded before the transition,
/// because adding a row to an already-completed sale is itself refused.
fn two_sales(conn: &Connection) {
    for (id, receipt) in [(1u8, "R-A"), (2u8, "R-B")] {
        seed(
            conn,
            "sale",
            &[
                ("id", blob(id)),
                ("status", Value::Text("parked".into())),
                ("receipt_number", Value::Text(receipt.into())),
                ("currency", Value::Text("JOD".into())),
            ],
        );
    }
    // Parked sale A — the editable set, and the source for the re-parenting attacks.
    seed(
        conn,
        "sale_line",
        &[
            ("id", blob(0x11)),
            ("sale_id", blob(1)),
            ("line_no", Value::Integer(7)),
        ],
    );
    seed(
        conn,
        "sale_tender",
        &[("id", blob(0x31)), ("sale_id", blob(1))],
    );
    seed(
        conn,
        "sale_line_tax",
        &[("id", blob(0x21)), ("sale_line_id", blob(0x11))],
    );
    seed(
        conn,
        "sale_line_discount",
        &[("id", blob(0x22)), ("sale_line_id", blob(0x11))],
    );
    seed(
        conn,
        "sale_tax_summary",
        &[("id", blob(0x23)), ("sale_id", blob(1))],
    );

    // Sale B — seeded while parked, frozen by the completion below.
    seed(
        conn,
        "sale_line",
        &[
            ("id", blob(0x12)),
            ("sale_id", blob(2)),
            ("line_no", Value::Integer(1)),
        ],
    );
    seed(
        conn,
        "sale_tender",
        &[("id", blob(0x32)), ("sale_id", blob(2))],
    );
    seed(
        conn,
        "sale_line_tax",
        &[("id", blob(0x24)), ("sale_line_id", blob(0x12))],
    );
    seed(
        conn,
        "sale_line_discount",
        &[("id", blob(0x25)), ("sale_line_id", blob(0x12))],
    );
    seed(
        conn,
        "sale_tax_summary",
        &[("id", blob(0x26)), ("sale_id", blob(2))],
    );

    // The one legal transition: parked -> completed.
    conn.execute(
        "UPDATE sale SET status = 'completed' WHERE id = ?1",
        [blob(2)],
    )
    .unwrap();
}

/// What "frozen" means for each fact table, in the design's own terms.
///
/// The sale-scoped tables are frozen *because their parent sale is completed*;
/// the ledgers and trails are frozen always. `sale_tender` is the documented
/// exception: a semi-integrated capture settles after completion, so its
/// settlement columns stay open while its money, method and parent do not.
struct FrozenRow {
    table: &'static str,
    /// A row that must be frozen, or `None` for "any row in the table".
    id: Option<u8>,
    /// A write the design forbids on that row.
    forbidden_update: &'static str,
}

const FROZEN: &[FrozenRow] = &[
    FrozenRow {
        table: "sale",
        id: Some(2),
        forbidden_update: "UPDATE sale SET total_minor = total_minor + 1",
    },
    FrozenRow {
        table: "sale_line",
        id: Some(0x12),
        forbidden_update: "UPDATE sale_line SET qty_milli = 9000",
    },
    FrozenRow {
        table: "sale_tender",
        id: Some(0x32),
        forbidden_update: "UPDATE sale_tender SET amount_minor = amount_minor + 1",
    },
    FrozenRow {
        table: "sale_line_tax",
        id: Some(0x24),
        forbidden_update: "UPDATE sale_line_tax SET tax_minor = tax_minor + 1",
    },
    FrozenRow {
        table: "sale_line_discount",
        id: Some(0x25),
        forbidden_update: "UPDATE sale_line_discount SET amount_minor = amount_minor + 1",
    },
    FrozenRow {
        table: "sale_tax_summary",
        id: Some(0x26),
        forbidden_update: "UPDATE sale_tax_summary SET tax_minor = tax_minor + 1",
    },
    FrozenRow {
        table: "audit_log",
        id: None,
        forbidden_update: "UPDATE audit_log SET action = 'tampered'",
    },
    FrozenRow {
        table: "stock_ledger",
        id: None,
        forbidden_update: "UPDATE stock_ledger SET qty_delta_milli = 9000",
    },
    FrozenRow {
        table: "cash_movement",
        id: None,
        forbidden_update: "UPDATE cash_movement SET amount_minor = 0",
    },
    FrozenRow {
        table: "z_report",
        id: None,
        forbidden_update: "UPDATE z_report SET z_number = 0",
    },
    FrozenRow {
        table: "drawer_event",
        id: None,
        forbidden_update: "UPDATE drawer_event SET cause = 'refund'",
    },
    FrozenRow {
        table: "loyalty_ledger",
        id: None,
        forbidden_update: "UPDATE loyalty_ledger SET points_delta = 9999",
    },
];

#[test]
fn the_frozen_row_table_covers_every_declared_fact_table() {
    let declared = declared_fact_tables();
    let covered: Vec<&str> = FROZEN.iter().map(|f| f.table).collect();
    for table in &declared {
        assert!(
            covered.contains(&table.as_str()),
            "{table} is declared a fact in ref/schema.md but this test never writes to it"
        );
    }
    assert_eq!(
        covered.len(),
        declared.len(),
        "FROZEN and the reference marker disagree: {covered:?} vs {declared:?}"
    );
}

#[test]
fn every_fact_table_refuses_the_write_that_would_rewrite_history() {
    let dir = tempfile::tempdir().unwrap();
    let conn = full_schema(&dir, "facts.db");
    two_sales(&conn);
    for table in [
        "audit_log",
        "stock_ledger",
        "cash_movement",
        "z_report",
        "drawer_event",
        "loyalty_ledger",
    ] {
        seed(&conn, table, &[]);
    }

    let mut open = Vec::new();
    for row in FROZEN {
        let where_clause = match row.id {
            Some(_) => " WHERE id = ?1",
            None => "",
        };
        let update = format!("{}{where_clause}", row.forbidden_update);
        let delete = format!("DELETE FROM {}{where_clause}", row.table);

        for (verb, sql) in [("UPDATE", &update), ("DELETE", &delete)] {
            let result = match row.id {
                Some(byte) => conn.execute(sql, [blob(byte)]),
                None => conn.execute(sql, []),
            };
            if result.is_ok() {
                open.push(format!("{}: {verb} was permitted", row.table));
            }
        }
    }
    assert!(
        open.is_empty(),
        "fact tables that accepted a forbidden write:\n  {}",
        open.join("\n  ")
    );
}

#[test]
fn a_completed_tender_still_settles_but_its_money_does_not_move() {
    let dir = tempfile::tempdir().unwrap();
    let conn = full_schema(&dir, "settle.db");
    two_sales(&conn);

    // The documented exception: the settlement columns stay open after completion.
    conn.execute(
        "UPDATE sale_tender SET tender_state = 'pending' WHERE id = ?1",
        [blob(0x32)],
    )
    .expect("a semi-integrated capture settles after completion");

    // The money does not.
    conn.execute(
        "UPDATE sale_tender SET amount_minor = amount_minor + 1 WHERE id = ?1",
        [blob(0x32)],
    )
    .expect_err("the money on a completed sale must not move");
}

#[test]
fn no_fact_row_can_be_reparented_into_a_completed_sale() {
    let dir = tempfile::tempdir().unwrap();
    let conn = full_schema(&dir, "reparent.db");
    two_sales(&conn);

    // Every one of these leaves the protected rows untouched and instead moves an
    // unprotected row INTO the completed sale, which is the same forgery.
    let attacks: [(&str, &str, [Value; 2]); 5] = [
        (
            "sale_line",
            "UPDATE sale_line SET sale_id = ?1 WHERE id = ?2",
            [blob(2), blob(0x11)],
        ),
        (
            "sale_tender",
            "UPDATE sale_tender SET sale_id = ?1 WHERE id = ?2",
            [blob(2), blob(0x31)],
        ),
        (
            "sale_line_tax",
            "UPDATE sale_line_tax SET sale_line_id = ?1 WHERE id = ?2",
            [blob(0x12), blob(0x21)],
        ),
        (
            "sale_line_discount",
            "UPDATE sale_line_discount SET sale_line_id = ?1 WHERE id = ?2",
            [blob(0x12), blob(0x22)],
        ),
        (
            "sale_tax_summary",
            "UPDATE sale_tax_summary SET sale_id = ?1 WHERE id = ?2",
            [blob(2), blob(0x23)],
        ),
    ];

    let mut open = Vec::new();
    for (table, sql, params) in attacks {
        let p: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        if conn.execute(sql, p.as_slice()).is_ok() {
            open.push(table);
        }
    }
    assert!(
        open.is_empty(),
        "rows re-parented into a completed sale: {open:?}"
    );
}

#[test]
fn a_parked_sale_is_still_fully_editable() {
    let dir = tempfile::tempdir().unwrap();
    let conn = full_schema(&dir, "parked.db");
    two_sales(&conn);

    // The guards must refuse forgery without refusing the ordinary lifecycle.
    seed(
        &conn,
        "sale_line",
        &[
            ("id", blob(0x13)),
            ("sale_id", blob(1)),
            ("line_no", Value::Integer(8)),
        ],
    );
    conn.execute(
        "UPDATE sale_line SET qty_milli = 2000 WHERE id = ?1",
        [blob(0x13)],
    )
    .expect("a parked sale's lines must stay editable");
    conn.execute("DELETE FROM sale_line WHERE id = ?1", [blob(0x13)])
        .expect("a parked sale's lines must stay removable");
}
