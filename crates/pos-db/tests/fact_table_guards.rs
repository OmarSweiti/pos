//! I-4 and I-6, proven by attempting the mutation — not by reading trigger text.
//!
//! The first version of this file searched `sqlite_master` for a `BEFORE <verb>`
//! trigger containing `RAISE(ABORT)`. That was a presence check masquerading as a
//! proof: a trigger guarded by `WHEN 0` would have satisfied it, and future fact
//! tables absent from the shipped chain were silently skipped.
//!
//! These tests apply the executable schema reference, create domain-valid facts
//! through their real lifecycle and commit-group boundaries, then perform the
//! forbidden writes. Each error must be `SQLITE_CONSTRAINT_TRIGGER` with the
//! exact guard message; a missing column, unrelated CHECK or foreign-key failure
//! cannot impersonate immutability. The inventory comes from the
//! `<!-- fact-tables: ... -->` marker beside the normative schema prose, so a
//! newly declared fact fails until its real write surface is exercised here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/fact_fixture.rs"]
mod fact_fixture;

use std::collections::HashSet;

use common::{declared_fact_tables, full_schema};
use fact_fixture::{
    COMPLETED_LINE_ID, COMPLETED_SALE_ID, COMPLETED_TENDER_ID, PARKED_LINE_ID, PARKED_SALE_ID,
    PARKED_TENDER_ID, append_tender_collection, bytes, fact_entity_id, identity_column,
    ready_commit, seed_fact_world,
};
use rusqlite::{
    Connection, ErrorCode,
    ffi::{SQLITE_CONSTRAINT_CHECK, SQLITE_CONSTRAINT_TRIGGER},
    params,
};

struct FactGuardCase {
    table: &'static str,
    update_error: &'static str,
    delete_error: &'static str,
}

const fn guard(
    table: &'static str,
    update_error: &'static str,
    delete_error: &'static str,
) -> FactGuardCase {
    FactGuardCase {
        table,
        update_error,
        delete_error,
    }
}

const FACT_GUARDS: &[FactGuardCase] = &[
    guard(
        "sync_commit",
        "a durable sync commit envelope is immutable",
        "a sync commit cannot be pruned while it still owns members",
    ),
    guard(
        "fact_commit_member",
        "a fact commit member and its canonical bytes are immutable",
        "fact membership survives outbox acknowledgement and pruning",
    ),
    guard(
        "sale",
        "I-4: a completed sale is immutable — issue a correcting document",
        "I-4: a completed sale cannot be deleted — issue a correcting document",
    ),
    guard(
        "sale_supply_tax_context",
        "I-4: the supply tax context of a completed sale is immutable",
        "I-4: the supply tax context of a completed sale cannot be deleted",
    ),
    guard(
        "sale_line",
        "I-4: a line of a completed sale is immutable",
        "I-4: a line of a completed sale cannot be deleted",
    ),
    guard(
        "sale_tender",
        "I-4: a tender on a completed sale is immutable — append a status event",
        "I-4: a tender of a completed sale cannot be deleted",
    ),
    guard(
        "sale_line_tax",
        "I-4: the tax detail of a completed sale is immutable",
        "I-4: the tax detail of a completed sale cannot be deleted",
    ),
    guard(
        "sale_line_discount",
        "I-4: the discount on a completed sale is immutable",
        "I-4: the discount on a completed sale cannot be deleted",
    ),
    guard(
        "sale_tax_summary",
        "I-4: the tax summary of a completed sale is immutable",
        "I-4: the tax summary of a completed sale cannot be deleted",
    ),
    guard(
        "receipt_artifact",
        "receipt bytes are immutable — create a linked artifact",
        "receipt evidence cannot be deleted",
    ),
    guard(
        "print_attempt",
        "print attempts are append-only",
        "print attempt history cannot be deleted",
    ),
    guard(
        "tender_status_event",
        "tender settlement is append-only",
        "tender settlement history cannot be deleted",
    ),
    guard(
        "shift",
        "a shift opening fact is immutable",
        "a shift opening fact cannot be deleted",
    ),
    guard(
        "shift_close_event",
        "a shift close event is immutable",
        "a shift close event cannot be deleted",
    ),
    guard(
        "shift_count_line",
        "a closed shift count is immutable",
        "a closed shift count cannot be deleted",
    ),
    guard(
        "approval_handle",
        "ApprovalHandle is immutable — consume it once with the effect",
        "ApprovalHandle is audit evidence and cannot be deleted",
    ),
    guard(
        "approval_consumption",
        "an approval consumption fact is immutable",
        "an approval consumption fact cannot be deleted",
    ),
    guard(
        "audit_log",
        "I-4: audit_log is append-only — no UPDATE, ever",
        "I-4: audit_log is append-only — no DELETE, ever",
    ),
    guard(
        "audit_checkpoint",
        "an audit checkpoint is append-only",
        "an audit checkpoint cannot be deleted",
    ),
    guard(
        "stock_ledger",
        "I-6: stock_ledger is append-only — post a correcting event",
        "I-6: stock_ledger is append-only — post a correcting event",
    ),
    guard(
        "trade_scale_verification",
        "scale verification evidence is append-only",
        "scale verification history cannot be deleted",
    ),
    guard(
        "cash_movement",
        "I-4: cash_movement is append-only — post a correcting movement",
        "I-4: cash_movement is append-only — post a correcting movement",
    ),
    guard(
        "cash_count",
        "a submitted cash count is immutable",
        "a submitted cash count cannot be deleted",
    ),
    guard(
        "z_report",
        "I-4: a Z report is immutable once taken",
        "I-4: a Z report cannot be deleted",
    ),
    guard(
        "drawer_event",
        "I-4: drawer_event is append-only — the no-sale trail is evidence",
        "I-4: drawer_event is append-only — the no-sale trail is evidence",
    ),
    guard(
        "credit_note_context",
        "original credit-note facts are immutable",
        "original credit-note facts cannot be deleted",
    ),
    guard(
        "refund_line_link",
        "the refund trail is append-only",
        "the refund trail cannot be deleted",
    ),
    guard(
        "defect_resolution_event",
        "consumer-selected defect resolution is immutable evidence",
        "consumer-selected defect resolution cannot be deleted",
    ),
    guard(
        "document_link",
        "document lineage is immutable",
        "document lineage cannot be deleted",
    ),
    guard(
        "stored_value_ledger",
        "stored value is a ledger — append a correcting event",
        "stored-value history cannot be deleted",
    ),
    guard(
        "fiscal_document",
        "local fiscal UUID and document identity are immutable",
        "fiscal identity cannot be deleted while clearance waits",
    ),
    guard(
        "fiscal_payload_event",
        "allocated ICV and submitted payload are immutable",
        "allocated fiscal payload evidence cannot be deleted",
    ),
    guard(
        "fiscal_queue_event",
        "fiscal queue transitions are append-only",
        "fiscal queue transition history cannot be deleted",
    ),
    guard(
        "fiscal_result",
        "an accepted fiscal artifact is immutable",
        "accepted fiscal evidence cannot be deleted",
    ),
    guard(
        "fiscal_reconciliation_issue",
        "a fiscal reconciliation issue is immutable — append a resolution event",
        "fiscal reconciliation history cannot be deleted",
    ),
    guard(
        "fiscal_resolution_event",
        "fiscal resolutions are append-only",
        "fiscal resolution history cannot be deleted",
    ),
    guard(
        "consent_event",
        "consent evidence is append-only",
        "consent evidence cannot be deleted",
    ),
    guard(
        "consent_acceptance",
        "server ordering of consent evidence is append-only",
        "accepted consent ordering evidence cannot be deleted",
    ),
    guard(
        "privacy_request_case",
        "a privacy request is immutable — append a case event",
        "privacy request history cannot be deleted",
    ),
    guard(
        "privacy_request_event",
        "privacy case events are append-only",
        "privacy case history cannot be deleted",
    ),
    guard(
        "privacy_tombstone",
        "a privacy tombstone is immutable",
        "a privacy tombstone cannot be deleted",
    ),
    guard(
        "loyalty_ledger",
        "I-4: loyalty_ledger is append-only — post an adjust row",
        "I-4: loyalty_ledger is append-only — post an adjust row",
    ),
    guard(
        "promotion_version",
        "published promotion terms are immutable — create a new version",
        "promotion evidence cannot be deleted",
    ),
    guard(
        "promotion_regulated_exclusion",
        "regulated-product exclusion evidence is immutable",
        "a published promotion cannot lose its regulated-product exclusion",
    ),
    guard(
        "promotion_publication",
        "published offer wording is immutable",
        "published offer evidence cannot be deleted",
    ),
    guard(
        "promotion_attribution",
        "charged-price attribution is immutable",
        "charged-price attribution cannot be deleted",
    ),
    guard(
        "regulated_display_approval",
        "regulated display approval evidence is immutable",
        "regulated display approval evidence cannot be deleted",
    ),
    guard(
        "supplier_invoice",
        "a supplier tax invoice is immutable — post a supplier credit",
        "supplier tax evidence cannot be deleted",
    ),
    guard(
        "supplier_invoice_line",
        "a supplier invoice line is immutable",
        "supplier invoice lines cannot be deleted",
    ),
    guard(
        "supplier_invoice_line_tax",
        "supplier tax components are immutable",
        "supplier tax components cannot be deleted",
    ),
    guard(
        "supplier_invoice_post_event",
        "supplier-invoice posting evidence is immutable",
        "a posted supplier invoice cannot be unsealed",
    ),
    guard(
        "goods_receipt",
        "a posted goods receipt header is immutable",
        "a posted goods receipt cannot be deleted",
    ),
    guard(
        "goods_receipt_line",
        "posted receipt cost evidence is immutable",
        "posted receipt cost evidence cannot be deleted",
    ),
    guard(
        "goods_receipt_post_event",
        "goods receipt posting is an immutable transition fact",
        "goods receipt posting cannot be deleted",
    ),
    guard(
        "stock_count",
        "a posted stock-count header is immutable",
        "a posted stock count cannot be deleted",
    ),
    guard(
        "stock_count_line",
        "posted stock-count evidence is immutable",
        "posted stock-count evidence cannot be deleted",
    ),
    guard(
        "stock_count_post_event",
        "stock-count posting is an immutable transition fact",
        "stock-count posting cannot be deleted",
    ),
    guard(
        "transfer",
        "a shipped or cancelled transfer header is immutable",
        "a shipped or cancelled transfer cannot be deleted",
    ),
    guard(
        "transfer_line",
        "shipped or cancelled transfer lines are immutable",
        "shipped or cancelled transfer lines cannot be deleted",
    ),
    guard(
        "transfer_ship_event",
        "ship transition is immutable",
        "ship transition cannot be deleted",
    ),
    guard(
        "transfer_receipt_line",
        "a transfer receipt line is immutable",
        "a transfer receipt line cannot be deleted",
    ),
    guard(
        "transfer_receive_event",
        "receive transition is immutable",
        "receive transition cannot be deleted",
    ),
    guard(
        "transfer_cancel_event",
        "cancel transition is immutable",
        "cancel transition cannot be deleted",
    ),
    guard(
        "tax_filing_event",
        "filing status is append-only",
        "filing history cannot be deleted",
    ),
    guard(
        "tax_period_adjustment",
        "tax adjustments are correction facts, not editable rows",
        "tax-adjustment evidence cannot be deleted",
    ),
    guard(
        "common_input_allocation",
        "common-input allocation evidence is immutable",
        "common-input allocation evidence cannot be deleted",
    ),
    guard(
        "tax_credit_ledger",
        "tax credit is a ledger — append a correction",
        "tax-credit history cannot be deleted",
    ),
    guard(
        "tax_filing_election",
        "a filing election is immutable — append superseding evidence",
        "filing-election evidence cannot be deleted",
    ),
    guard(
        "credit_note_period_assignment",
        "filed-period lineage is immutable",
        "filed-period lineage cannot be deleted",
    ),
];

fn fact_connection(name: &str) -> (tempfile::TempDir, Connection) {
    let directory = tempfile::tempdir().unwrap();
    let connection = full_schema(&directory, name);
    seed_fact_world(&connection);
    (directory, connection)
}

fn target_rowid(conn: &Connection, table: &str) -> i64 {
    let column = identity_column(table);
    let id = fact_entity_id(table);
    let rowids: Vec<i64> = conn
        .prepare(&format!("SELECT rowid FROM {table} WHERE {column} = ?1"))
        .unwrap()
        .query_map([id], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rowids.len(),
        1,
        "{table} guard fixture must identify exactly one target row: {rowids:?}"
    );
    rowids
        .into_iter()
        .next()
        .expect("the target count assertion proved one row exists")
}

fn assert_trigger_error(table: &str, verb: &str, expected: &str, error: rusqlite::Error) {
    match error {
        rusqlite::Error::SqliteFailure(sqlite, Some(message)) => {
            assert_eq!(
                sqlite.code,
                ErrorCode::ConstraintViolation,
                "{table} {verb} failed with the wrong SQLite class: {message}"
            );
            assert_eq!(
                sqlite.extended_code, SQLITE_CONSTRAINT_TRIGGER,
                "{table} {verb} was stopped by something other than its history guard: {message}"
            );
            assert_eq!(message, expected, "{table} {verb} hit the wrong trigger");
        }
        other => panic!("{table} {verb} failed for the wrong reason: {other:?}"),
    }
}

fn assert_forbidden_write(
    conn: &Connection,
    table: &str,
    verb: &str,
    sql: &str,
    rowid: i64,
    expected: &str,
) {
    conn.execute_batch("SAVEPOINT fact_guard").unwrap();
    let error = expect_refusal(
        conn.execute(sql, [rowid]),
        &format!("{table}: {verb} was permitted"),
    );
    assert_trigger_error(table, verb, expected, error);
    conn.execute_batch("ROLLBACK TO fact_guard; RELEASE fact_guard")
        .unwrap();
}

fn expect_refusal(result: rusqlite::Result<usize>, permitted: &str) -> rusqlite::Error {
    match result {
        Ok(_) => panic!("{permitted}"),
        Err(error) => error,
    }
}

#[test]
fn an_actor_cannot_approve_their_own_handle() {
    let (_directory, conn) = fact_connection("self-approval.db");
    ready_commit(&conn, 300, &[(1300, "approval_handle", 230)]);
    let error = expect_refusal(
        conn.execute(
            "INSERT INTO approval_handle (
                 id, capability, actor_id, approver_id, entity_id, amount_minor,
                 content_hash, reason, issued_at, expires_at, nonce
             ) VALUES (?1, 'price.override', ?2, ?2, ?3, 100, NULL,
                       'self approval', '2026-08-25T10:00:00.000Z',
                       '2026-08-25T10:02:00.000Z', ?4)",
            params![bytes(230), bytes(5), bytes(231), bytes(232)],
        ),
        "approval_handle accepted the same actor and approver",
    );
    match error {
        rusqlite::Error::SqliteFailure(sqlite, Some(message)) => {
            assert_eq!(sqlite.code, ErrorCode::ConstraintViolation);
            assert_eq!(sqlite.extended_code, SQLITE_CONSTRAINT_CHECK);
            assert_eq!(message, "CHECK constraint failed: actor_id <> approver_id");
        }
        other => panic!("self-approval failed for the wrong reason: {other:?}"),
    }
}

#[test]
fn altering_a_quick_add_request_after_approval_is_refused() {
    let (_directory, conn) = fact_connection("quick-add-intent.db");
    conn.execute(
        "INSERT INTO product_quick_add_request (
             product_id, barcode, name_ar, unit_price_minor, tax_category_id,
             requested_by, requested_at, content_hash
         ) VALUES (?1, '6250000000001', 'منتج', 1250, ?2, ?3,
                   '2026-08-25T10:00:00.000Z', ?4)",
        params![bytes(233), bytes(6), bytes(5), vec![7_u8; 32]],
    )
    .unwrap();
    ready_commit(&conn, 301, &[(1301, "approval_handle", 234)]);
    let hash_error = expect_refusal(
        conn.execute(
            "INSERT INTO approval_handle (
                 id, capability, actor_id, approver_id, entity_id, amount_minor,
                 content_hash, reason, issued_at, expires_at, nonce
             ) VALUES (?1, 'product.edit', ?2, ?3, ?4, 1250, ?5,
                       'emergency product', '2026-08-25T10:00:00.000Z',
                       '2026-08-25T10:02:00.000Z', ?6)",
            params![
                bytes(234),
                bytes(5),
                bytes(50),
                bytes(233),
                vec![9_u8; 32],
                bytes(235)
            ],
        ),
        "quick-add approval accepted a different content hash",
    );
    assert_trigger_error(
        "approval_handle",
        "INSERT",
        "quick-add approval must bind the prepared intent content hash",
        hash_error,
    );
    conn.execute(
        "INSERT INTO approval_handle (
             id, capability, actor_id, approver_id, entity_id, amount_minor,
             content_hash, reason, issued_at, expires_at, nonce
         ) VALUES (?1, 'product.edit', ?2, ?3, ?4, 1250, ?5,
                   'emergency product', '2026-08-25T10:00:00.000Z',
                   '2026-08-25T10:02:00.000Z', ?6)",
        params![
            bytes(234),
            bytes(5),
            bytes(50),
            bytes(233),
            vec![7_u8; 32],
            bytes(235)
        ],
    )
    .unwrap();

    let updates = [
        "UPDATE product_quick_add_request SET product_id = zeroblob(16) WHERE product_id = ?1",
        "UPDATE product_quick_add_request SET barcode = '6250000000002' WHERE product_id = ?1",
        "UPDATE product_quick_add_request SET name_ar = 'بديل' WHERE product_id = ?1",
        "UPDATE product_quick_add_request SET unit_price_minor = 2500 WHERE product_id = ?1",
        "UPDATE product_quick_add_request SET tax_category_id = zeroblob(16) WHERE product_id = ?1",
        "UPDATE product_quick_add_request SET requested_by = zeroblob(16) WHERE product_id = ?1",
        "UPDATE product_quick_add_request SET requested_at = '2026-08-25T11:00:00.000Z' WHERE product_id = ?1",
        "UPDATE product_quick_add_request SET content_hash = zeroblob(32) WHERE product_id = ?1",
    ];
    for sql in updates {
        let error = expect_refusal(
            conn.execute(sql, [bytes(233)]),
            "an approved quick-add field was mutable",
        );
        assert_trigger_error(
            "product_quick_add_request",
            "UPDATE",
            "prepared quick-add intent is immutable after approval",
            error,
        );
    }
}

#[test]
fn altering_a_stock_request_after_approval_is_refused() {
    let (_directory, conn) = fact_connection("stock-intent.db");
    conn.execute(
        "INSERT INTO stock_adjustment_request (
             stock_event_id, product_id, qty_delta_milli, reason_code, note,
             requested_by, requested_at, content_hash
         ) VALUES (?1, ?2, 1000, 'count_correction', 'counted', ?3,
                   '2026-08-25T10:00:00.000Z', ?4)",
        params![bytes(236), bytes(4), bytes(5), vec![8_u8; 32]],
    )
    .unwrap();
    ready_commit(&conn, 302, &[(1302, "approval_handle", 237)]);
    let hash_error = expect_refusal(
        conn.execute(
            "INSERT INTO approval_handle (
                 id, capability, actor_id, approver_id, entity_id, amount_minor,
                 content_hash, reason, issued_at, expires_at, nonce
             ) VALUES (?1, 'stock.adjust', ?2, ?3, ?4, 0, ?5,
                       'stock correction', '2026-08-25T10:00:00.000Z',
                       '2026-08-25T10:02:00.000Z', ?6)",
            params![
                bytes(237),
                bytes(5),
                bytes(50),
                bytes(236),
                vec![9_u8; 32],
                bytes(238)
            ],
        ),
        "stock-adjust approval accepted a different content hash",
    );
    assert_trigger_error(
        "approval_handle",
        "INSERT",
        "stock-adjust approval must bind the prepared intent content hash",
        hash_error,
    );
    conn.execute(
        "INSERT INTO approval_handle (
             id, capability, actor_id, approver_id, entity_id, amount_minor,
             content_hash, reason, issued_at, expires_at, nonce
         ) VALUES (?1, 'stock.adjust', ?2, ?3, ?4, 0, ?5,
                   'stock correction', '2026-08-25T10:00:00.000Z',
                   '2026-08-25T10:02:00.000Z', ?6)",
        params![
            bytes(237),
            bytes(5),
            bytes(50),
            bytes(236),
            vec![8_u8; 32],
            bytes(238)
        ],
    )
    .unwrap();

    let updates = [
        "UPDATE stock_adjustment_request SET stock_event_id = zeroblob(16) WHERE stock_event_id = ?1",
        "UPDATE stock_adjustment_request SET product_id = zeroblob(16) WHERE stock_event_id = ?1",
        "UPDATE stock_adjustment_request SET qty_delta_milli = 2000 WHERE stock_event_id = ?1",
        "UPDATE stock_adjustment_request SET reason_code = 'damage' WHERE stock_event_id = ?1",
        "UPDATE stock_adjustment_request SET note = 'changed' WHERE stock_event_id = ?1",
        "UPDATE stock_adjustment_request SET requested_by = zeroblob(16) WHERE stock_event_id = ?1",
        "UPDATE stock_adjustment_request SET requested_at = '2026-08-25T11:00:00.000Z' WHERE stock_event_id = ?1",
        "UPDATE stock_adjustment_request SET content_hash = zeroblob(32) WHERE stock_event_id = ?1",
    ];
    for sql in updates {
        let error = expect_refusal(
            conn.execute(sql, [bytes(236)]),
            "an approved stock-adjust field was mutable",
        );
        assert_trigger_error(
            "stock_adjustment_request",
            "UPDATE",
            "prepared stock intent is immutable after approval",
            error,
        );
    }
}

#[test]
fn the_frozen_row_table_covers_every_declared_fact_table() {
    let declared = declared_fact_tables();
    let covered: Vec<&str> = FACT_GUARDS.iter().map(|case| case.table).collect();
    let unique: HashSet<&str> = covered.iter().copied().collect();
    assert_eq!(
        unique.len(),
        covered.len(),
        "the guard registry contains a duplicate: {covered:?}"
    );
    assert_eq!(
        covered,
        declared.iter().map(String::as_str).collect::<Vec<_>>(),
        "the guard registry must exactly follow the executable fact marker"
    );
}

#[test]
fn every_fact_table_refuses_the_write_that_would_rewrite_history() {
    let (_directory, conn) = fact_connection("facts.db");
    for case in FACT_GUARDS {
        let rowid = target_rowid(&conn, case.table);
        assert_forbidden_write(
            &conn,
            case.table,
            "UPDATE",
            &format!(
                "UPDATE {} SET rowid = rowid + 1000000 WHERE rowid = ?1",
                case.table
            ),
            rowid,
            case.update_error,
        );
        assert_forbidden_write(
            &conn,
            case.table,
            "DELETE",
            &format!("DELETE FROM {} WHERE rowid = ?1", case.table),
            rowid,
            case.delete_error,
        );
    }
}

#[test]
fn a_completed_tender_settles_by_appending_an_event_while_its_money_stays_frozen() {
    let (_directory, conn) = fact_connection("settle.db");

    append_tender_collection(&conn);
    let projection: (i64, String) = conn
        .query_row(
            "SELECT event_no, state FROM tender_status_current WHERE tender_id = ?1",
            [bytes(COMPLETED_TENDER_ID)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        projection,
        (2, "collected".to_owned()),
        "settlement must project the newly appended event"
    );

    let error = conn
        .execute(
            "UPDATE sale_tender SET amount_minor = amount_minor + 1 WHERE id = ?1",
            [bytes(COMPLETED_TENDER_ID)],
        )
        .expect_err("the money on a completed tender must not move");
    assert_trigger_error(
        "sale_tender",
        "UPDATE",
        "I-4: a tender on a completed sale is immutable — append a status event",
        error,
    );
}

#[test]
fn no_sale_fact_can_be_reparented_into_a_completed_sale() {
    let (_directory, conn) = fact_connection("sale-reparent.db");
    let attacks = [
        (
            "sale_supply_tax_context",
            "UPDATE sale_supply_tax_context SET sale_id = ?1 WHERE sale_id = ?2",
            COMPLETED_SALE_ID,
            PARKED_SALE_ID,
            "I-4: the supply tax context of a completed sale is immutable",
        ),
        (
            "sale_line",
            "UPDATE sale_line SET sale_id = ?1 WHERE id = ?2",
            COMPLETED_SALE_ID,
            PARKED_LINE_ID,
            "I-4: a line of a completed sale is immutable",
        ),
        (
            "sale_tender",
            "UPDATE sale_tender SET sale_id = ?1 WHERE id = ?2",
            COMPLETED_SALE_ID,
            PARKED_TENDER_ID,
            "I-4: a tender on a completed sale is immutable — append a status event",
        ),
        (
            "sale_line_tax",
            "UPDATE sale_line_tax SET sale_line_id = ?1 WHERE id = ?2",
            COMPLETED_LINE_ID,
            203,
            "I-4: the tax detail of a completed sale is immutable",
        ),
        (
            "sale_line_discount",
            "UPDATE sale_line_discount SET sale_line_id = ?1 WHERE id = ?2",
            COMPLETED_LINE_ID,
            132,
            "I-4: the discount on a completed sale is immutable",
        ),
        (
            "sale_tax_summary",
            "UPDATE sale_tax_summary SET sale_id = ?1 WHERE id = ?2",
            COMPLETED_SALE_ID,
            204,
            "I-4: the tax summary of a completed sale is immutable",
        ),
    ];

    for (table, sql, new_parent, child, expected) in attacks {
        conn.execute_batch("SAVEPOINT reparent").unwrap();
        let error = expect_refusal(
            conn.execute(sql, params![bytes(new_parent), bytes(child)]),
            &format!("{table} was reparented into a completed sale"),
        );
        assert_trigger_error(table, "REPARENT", expected, error);
        conn.execute_batch("ROLLBACK TO reparent; RELEASE reparent")
            .unwrap();
    }
}

#[test]
fn no_child_fact_can_be_reparented_into_a_sealed_parent() {
    let (_directory, conn) = fact_connection("lifecycle-reparent.db");
    let attacks = [
        (
            "shift_count_line",
            "UPDATE shift_count_line SET shift_id = ?1 WHERE id = ?2",
            10,
            211,
            "a closed shift count is immutable",
        ),
        (
            "goods_receipt_line",
            "UPDATE goods_receipt_line SET receipt_id = ?1 WHERE id = ?2",
            150,
            213,
            "posted receipt cost evidence is immutable",
        ),
        (
            "stock_count_line",
            "UPDATE stock_count_line SET count_id = ?1 WHERE id = ?2",
            160,
            215,
            "posted stock-count evidence is immutable",
        ),
        (
            "transfer_line",
            "UPDATE transfer_line SET transfer_id = ?1 WHERE id = ?2",
            172,
            217,
            "shipped or cancelled transfer lines are immutable",
        ),
    ];

    let mut permitted = Vec::new();
    for (table, sql, new_parent, child, expected) in attacks {
        conn.execute_batch("SAVEPOINT reparent").unwrap();
        match conn.execute(sql, params![bytes(new_parent), bytes(child)]) {
            Ok(_) => permitted.push(table),
            Err(error) => assert_trigger_error(table, "REPARENT", expected, error),
        }
        conn.execute_batch("ROLLBACK TO reparent; RELEASE reparent")
            .unwrap();
    }
    assert!(
        permitted.is_empty(),
        "rows reparented into sealed parents: {permitted:?}"
    );
}

fn assert_mutable_write(conn: &Connection, table: &str, sql: &str, id: u16) {
    conn.execute_batch("SAVEPOINT mutable_phase").unwrap();
    let affected = conn.execute(sql, [bytes(id)]).unwrap_or_else(|error| {
        panic!("{table} must stay editable before its parent is sealed: {error}")
    });
    assert_eq!(
        affected, 1,
        "{table} mutable-phase control did not target exactly one row"
    );
    conn.execute_batch("ROLLBACK TO mutable_phase; RELEASE mutable_phase")
        .unwrap();
}

fn assert_mutable_parent_delete(
    conn: &Connection,
    parent_table: &str,
    parent_id: u16,
    child_table: &str,
    child_id: u16,
) {
    conn.execute_batch("SAVEPOINT mutable_parent").unwrap();
    let child_rows = conn
        .execute(
            &format!("DELETE FROM {child_table} WHERE id = ?1"),
            [bytes(child_id)],
        )
        .unwrap_or_else(|error| {
            panic!("{child_table} must be removable before its parent is sealed: {error}")
        });
    assert_eq!(child_rows, 1, "{child_table} draft fixture drifted");
    let parent_rows = conn
        .execute(
            &format!("DELETE FROM {parent_table} WHERE id = ?1"),
            [bytes(parent_id)],
        )
        .unwrap_or_else(|error| {
            panic!("{parent_table} must be removable before its transition fact exists: {error}")
        });
    assert_eq!(parent_rows, 1, "{parent_table} draft fixture drifted");
    conn.execute_batch("ROLLBACK TO mutable_parent; RELEASE mutable_parent")
        .unwrap();
}

#[test]
fn mutable_parent_phases_remain_editable() {
    let (_directory, conn) = fact_connection("mutable.db");

    for (table, sql, id) in [
        (
            "sale",
            "UPDATE sale SET subtotal_minor = subtotal_minor WHERE id = ?1",
            PARKED_SALE_ID,
        ),
        (
            "sale_supply_tax_context",
            "UPDATE sale_supply_tax_context SET destination_code = destination_code WHERE sale_id = ?1",
            PARKED_SALE_ID,
        ),
        (
            "sale_line",
            "UPDATE sale_line SET qty_milli = qty_milli WHERE id = ?1",
            PARKED_LINE_ID,
        ),
        (
            "sale_tender",
            "UPDATE sale_tender SET amount_minor = amount_minor WHERE id = ?1",
            PARKED_TENDER_ID,
        ),
        (
            "sale_line_tax",
            "UPDATE sale_line_tax SET tax_minor = tax_minor WHERE id = ?1",
            203,
        ),
        (
            "sale_line_discount",
            "UPDATE sale_line_discount SET amount_minor = amount_minor WHERE id = ?1",
            132,
        ),
        (
            "sale_tax_summary",
            "UPDATE sale_tax_summary SET tax_minor = tax_minor WHERE id = ?1",
            204,
        ),
        (
            "shift_count_line",
            "UPDATE shift_count_line SET count = count + 1 WHERE id = ?1",
            211,
        ),
        (
            "goods_receipt",
            "UPDATE goods_receipt SET reference = reference WHERE id = ?1",
            212,
        ),
        (
            "goods_receipt_line",
            "UPDATE goods_receipt_line SET qty_milli = qty_milli WHERE id = ?1",
            213,
        ),
        (
            "stock_count",
            "UPDATE stock_count SET scope = scope WHERE id = ?1",
            214,
        ),
        (
            "stock_count_line",
            "UPDATE stock_count_line SET expected_milli = expected_milli WHERE id = ?1",
            215,
        ),
        (
            "transfer",
            "UPDATE transfer SET created_at = created_at WHERE id = ?1",
            216,
        ),
        (
            "transfer_line",
            "UPDATE transfer_line SET qty_sent_milli = qty_sent_milli WHERE id = ?1",
            217,
        ),
    ] {
        assert_mutable_write(&conn, table, sql, id);
    }

    for (table, sql, id) in [
        ("sale", "DELETE FROM sale WHERE id = ?1", 221),
        (
            "sale_supply_tax_context",
            "DELETE FROM sale_supply_tax_context WHERE sale_id = ?1",
            PARKED_SALE_ID,
        ),
        ("sale_line", "DELETE FROM sale_line WHERE id = ?1", 220),
        (
            "sale_tender",
            "DELETE FROM sale_tender WHERE id = ?1",
            PARKED_TENDER_ID,
        ),
        (
            "sale_line_tax",
            "DELETE FROM sale_line_tax WHERE id = ?1",
            203,
        ),
        (
            "sale_line_discount",
            "DELETE FROM sale_line_discount WHERE id = ?1",
            219,
        ),
        (
            "sale_tax_summary",
            "DELETE FROM sale_tax_summary WHERE id = ?1",
            204,
        ),
        (
            "shift_count_line",
            "DELETE FROM shift_count_line WHERE id = ?1",
            211,
        ),
        (
            "goods_receipt_line",
            "DELETE FROM goods_receipt_line WHERE id = ?1",
            213,
        ),
        (
            "stock_count_line",
            "DELETE FROM stock_count_line WHERE id = ?1",
            215,
        ),
        (
            "transfer_line",
            "DELETE FROM transfer_line WHERE id = ?1",
            217,
        ),
    ] {
        assert_mutable_write(&conn, table, sql, id);
    }

    for (parent_table, parent_id, child_table, child_id) in [
        ("goods_receipt", 212, "goods_receipt_line", 213),
        ("stock_count", 214, "stock_count_line", 215),
        ("transfer", 216, "transfer_line", 217),
    ] {
        assert_mutable_parent_delete(&conn, parent_table, parent_id, child_table, child_id);
    }
}
