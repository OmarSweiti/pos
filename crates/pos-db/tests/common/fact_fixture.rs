//! Deterministic, cross-row fixtures for the executable fact-table guards.
//!
//! SQL types can supply scalar defaults, but they cannot express that a shift
//! close needs a ready sync envelope, or that a posted receipt seals a confirmed
//! supplier cost and its stock event. Those relationships live here as small
//! subsystem builders. Each builder uses the same commit boundary as its domain
//! operation, so a guard never passes against a malformed sentinel row or one
//! implausible commit that claims every subsystem changed atomically.

use std::collections::HashSet;

use rusqlite::{Connection, params, types::Value};

use crate::common::declared_fact_tables;

pub const COMPLETED_SALE_ID: u16 = 20;
pub const COMPLETED_LINE_ID: u16 = 21;
pub const COMPLETED_TENDER_ID: u16 = 22;
pub const PARKED_SALE_ID: u16 = 130;
pub const PARKED_LINE_ID: u16 = 131;
pub const PARKED_TENDER_ID: u16 = 202;
pub const REFUND_SALE_ID: u16 = 70;

const FIXED_AT: &str = "2026-08-25T10:00:00.000Z";
const LATER_AT: &str = "2026-08-25T11:00:00.000Z";
const FIXED_DATE: &str = "2026-08-25";

pub fn bytes(number: u16) -> Vec<u8> {
    let mut value = vec![0; 16];
    value
        .get_mut(14..)
        .expect("a fixture id has sixteen bytes")
        .copy_from_slice(&number.to_be_bytes());
    value
}

fn blob(number: u16) -> Value {
    Value::Blob(bytes(number))
}

fn hash(number: u16) -> Value {
    Value::Blob(vec![number as u8; 32])
}

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

/// Sparse insert: optional fields not named by the fixture remain NULL and
/// defaulted fields exercise the schema default. This matters for coupled
/// checks such as fiscal leases and original-artifact lineage, where an
/// invented placeholder means something materially different from absence.
fn insert(conn: &Connection, table: &str, values: &[(&str, Value)]) {
    assert!(!values.is_empty(), "{table} fixture must name its columns");
    let columns: Vec<&str> = values.iter().map(|(column, _)| *column).collect();
    let holders: Vec<String> = (1..=values.len())
        .map(|index| format!("?{index}"))
        .collect();
    let parameters: Vec<&dyn rusqlite::ToSql> = values
        .iter()
        .map(|(_, value)| value as &dyn rusqlite::ToSql)
        .collect();
    conn.execute(
        &format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            columns.join(", "),
            holders.join(", ")
        ),
        parameters.as_slice(),
    )
    .unwrap_or_else(|error| {
        panic!(
            "could not seed {table} at {:?}: {error}",
            values.first().expect("fixture values are non-empty")
        )
    });
}

/// Create the permanent manifest and live delivery rows that make a commit
/// ready. Explicit change ids keep failure output stable and let the fixture
/// assert that no entity appears twice in one commit.
pub fn ready_commit(conn: &Connection, commit_id: u16, members: &[(u16, &str, u16)]) {
    assert!(!members.is_empty(), "a ready commit cannot be empty");
    let mut entities = HashSet::new();
    let mut changes = HashSet::new();
    for (change_id, entity, entity_id) in members {
        assert!(
            changes.insert(*change_id),
            "duplicate change id {change_id}"
        );
        assert!(
            entities.insert((*entity, *entity_id)),
            "duplicate fixture member: {entity} {entity_id}"
        );
    }

    insert(
        conn,
        "sync_commit",
        &[
            ("id", blob(commit_id)),
            ("commit_size", Value::Integer(members.len() as i64)),
            ("commit_hash", text(&format!("hash-{commit_id}"))),
            ("protocol_version", Value::Integer(1)),
            ("schema_version", Value::Integer(1)),
            ("producer_version", text("fact-guard-fixture")),
            ("created_at", text(FIXED_AT)),
        ],
    );
    for (index, (change_id, entity, entity_id)) in members.iter().enumerate() {
        insert(
            conn,
            "fact_commit_member",
            &[
                ("change_id", blob(*change_id)),
                ("commit_id", blob(commit_id)),
                ("commit_index", Value::Integer(index as i64)),
                ("entity", text(entity)),
                ("entity_id", blob(*entity_id)),
                ("op", text("insert")),
                ("payload", text("{}")),
                ("payload_hash", text(&format!("payload-{change_id}"))),
                ("created_at", text(FIXED_AT)),
            ],
        );
        insert(
            conn,
            "sync_outbox",
            &[
                ("change_id", blob(*change_id)),
                ("state", text("pending")),
                ("attempts", Value::Integer(0)),
                ("created_at", text(FIXED_AT)),
            ],
        );
    }

    let ready: i64 = conn
        .query_row(
            "SELECT count(*) FROM sync_commit_ready WHERE id = ?1",
            [bytes(commit_id)],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(ready, 1, "fixture commit {commit_id} is not ready");
}

fn seed_reference_world(conn: &Connection) {
    insert(
        conn,
        "org",
        &[("id", blob(1)), ("legal_name", text("Fixture Org"))],
    );
    for (id, code) in [(5, "U1"), (50, "U2")] {
        insert(
            conn,
            "app_user",
            &[
                ("id", blob(id)),
                ("org_id", blob(1)),
                ("code", text(code)),
                ("display_name", text("Fixture User")),
                ("pin_hash", text("fixture-pin-hash")),
                ("pin_set_at", text(FIXED_AT)),
            ],
        );
    }
    insert(
        conn,
        "tax_category",
        &[
            ("id", blob(6)),
            ("code", text("STD0")),
            ("name_ar", text("ضريبة")),
            ("treatment", text("standard")),
        ],
    );
    insert(
        conn,
        "tax_rule_pack",
        &[
            ("id", blob(7)),
            ("profile_scope", text("standard")),
            ("pack_version", text("fixture-v1")),
            ("source_ref", text("fixture")),
            ("content_hash", hash(7)),
            ("status", text("pending")),
        ],
    );
    insert(
        conn,
        "tax_rate",
        &[
            ("id", blob(8)),
            ("rule_pack_id", blob(7)),
            ("tax_category_id", blob(6)),
            ("component_code", text("GST")),
            ("treatment", text("standard")),
            ("calculation_kind", text("ad_valorem")),
            ("rate_ppm", Value::Integer(0)),
            ("calculation_order", Value::Integer(0)),
            ("base_kind", text("line_net")),
            ("valid_from", text("2026-01-01")),
            ("profile_scope", text("standard")),
        ],
    );
    conn.execute(
        "UPDATE tax_rule_pack
            SET status = 'approved', approved_by = ?1, approved_at = ?2
          WHERE id = ?3",
        params![bytes(5), FIXED_AT, bytes(7)],
    )
    .unwrap();
    insert(
        conn,
        "tax_computation_policy",
        &[
            ("id", blob(9)),
            ("jurisdiction", text("JO")),
            ("policy_version", text("fixture-v1")),
            ("rounding_rule", text("half_even")),
            ("cash_round_step_minor", Value::Integer(1)),
            ("cash_round_direction", text("nearest")),
            ("cash_round_tax_treatment", text("none")),
            ("source_ref", text("fixture")),
            ("content_hash", hash(9)),
            ("approved_at", text(FIXED_AT)),
        ],
    );
    seed_store(conn, 2, "S1", "متجر");
    seed_register(conn, 3, 2, "R1", "device-1", "key-1");
    insert(
        conn,
        "product",
        &[
            ("id", blob(4)),
            ("sku", text("SKU-1")),
            ("name", text("Service")),
            ("price_minor", Value::Integer(100)),
            ("currency", text("JOD")),
            ("name_ar", text("خدمة")),
            ("tax_category_id", blob(6)),
            ("unit", text("each")),
            ("qty_step_milli", Value::Integer(1000)),
            ("is_weighed", Value::Integer(0)),
            ("is_service", Value::Integer(1)),
            ("sale_form", text("service")),
        ],
    );
}

fn seed_store(conn: &Connection, id: u16, code: &str, name_ar: &str) {
    insert(
        conn,
        "store",
        &[
            ("id", blob(id)),
            ("org_id", blob(1)),
            ("code", text(code)),
            ("name_ar", text(name_ar)),
            ("fiscal_obligation", text("exempt")),
            ("fiscal_obligation_evidence_ref", text("fixture")),
            ("fiscal_profile", text("disabled")),
            ("tax_rule_pack_id", blob(7)),
            ("tax_computation_policy_id", blob(9)),
        ],
    );
}

fn seed_register(
    conn: &Connection,
    id: u16,
    store_id: u16,
    code: &str,
    device_id: &str,
    key_id: &str,
) {
    insert(
        conn,
        "register",
        &[
            ("id", blob(id)),
            ("store_id", blob(store_id)),
            ("code", text(code)),
            ("name", text("Fixture Register")),
            ("device_id", text(device_id)),
            ("credential_key_id", text(key_id)),
            ("credential_algorithm", text("ed25519")),
            ("credential_public_key", blob(99)),
            ("credential_issued_at", text(FIXED_AT)),
        ],
    );
}

fn seed_sales(conn: &Connection) {
    ready_commit(conn, 100, &[(1000, "shift", 10)]);
    insert(
        conn,
        "shift",
        &[
            ("id", blob(10)),
            ("register_id", blob(3)),
            ("store_id", blob(2)),
            ("business_date", text(FIXED_DATE)),
            ("opened_by", blob(5)),
            ("opened_at", text(FIXED_AT)),
            ("float_minor", Value::Integer(0)),
        ],
    );

    ready_commit(
        conn,
        101,
        &[
            (1001, "sale", COMPLETED_SALE_ID),
            (1002, "sale_line", COMPLETED_LINE_ID),
            (1003, "sale_tender", COMPLETED_TENDER_ID),
            (1004, "sale_line_tax", 23),
            (1005, "sale_line_discount", 24),
            (1006, "sale_tax_summary", 25),
            (1007, "sale_supply_tax_context", COMPLETED_SALE_ID),
            (1008, "receipt_artifact", 27),
            (1009, "tender_status_event", 28),
            (1010, "audit_log", 29),
        ],
    );
    insert(
        conn,
        "sale",
        &[
            ("id", blob(COMPLETED_SALE_ID)),
            ("receipt_number", text("R-20")),
            ("register_id", blob(3)),
            ("status", text("parked")),
            ("subtotal_minor", Value::Integer(100)),
            ("tax_minor", Value::Integer(0)),
            ("total_minor", Value::Integer(99)),
            ("currency", text("JOD")),
            ("business_date", text(FIXED_DATE)),
            ("completed_at", text(FIXED_AT)),
            ("store_id", blob(2)),
            ("shift_id", blob(10)),
            ("cashier_id", blob(5)),
            ("doc_type", text("sale")),
            ("is_training", Value::Integer(0)),
            ("discount_minor", Value::Integer(1)),
            ("rounding_adj_minor", Value::Integer(0)),
            ("tax_computation_policy_id", blob(9)),
            ("sync_commit_id", blob(101)),
            ("origin_device", text("fixture")),
        ],
    );
    insert(
        conn,
        "sale_supply_tax_context",
        &[
            ("sale_id", blob(COMPLETED_SALE_ID)),
            ("destination_code", text("JO")),
            ("captured_at", text(FIXED_AT)),
        ],
    );
    seed_sale_line(conn, COMPLETED_LINE_ID, COMPLETED_SALE_ID, 4, 1, 99, 1);
    insert(
        conn,
        "sale_tender",
        &[
            ("id", blob(COMPLETED_TENDER_ID)),
            ("sale_id", blob(COMPLETED_SALE_ID)),
            ("method", text("exchange")),
            ("amount_minor", Value::Integer(99)),
            ("change_minor", Value::Integer(0)),
        ],
    );
    seed_sale_tax(conn, 23, COMPLETED_LINE_ID, 99);
    insert(
        conn,
        "sale_line_discount",
        &[
            ("id", blob(24)),
            ("sale_line_id", blob(COMPLETED_LINE_ID)),
            ("source", text("manual_line")),
            ("amount_minor", Value::Integer(1)),
        ],
    );
    seed_sale_summary(conn, 25, COMPLETED_SALE_ID, 99);
    insert(
        conn,
        "receipt_artifact",
        &[
            ("id", blob(27)),
            ("sale_id", blob(COMPLETED_SALE_ID)),
            ("artifact_kind", text("original")),
            ("format", text("escpos")),
            ("template_version", text("v1")),
            ("printer_profile", text("80mm")),
            ("content_bytes", blob(1)),
            ("content_hash", hash(27)),
            ("generated_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "print_job",
        &[
            ("id", blob(30)),
            ("artifact_id", blob(27)),
            ("state", text("queued")),
            ("attempts", Value::Integer(0)),
            ("created_at", text(FIXED_AT)),
            ("updated_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "tender_status_event",
        &[
            ("id", blob(28)),
            ("tender_id", blob(COMPLETED_TENDER_ID)),
            ("sync_commit_id", blob(101)),
            ("event_no", Value::Integer(1)),
            ("state", text("pending")),
            ("occurred_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "audit_log",
        &[
            ("id", blob(29)),
            ("canonical_version", Value::Integer(1)),
            ("register_id", blob(3)),
            ("actor_id", blob(5)),
            ("action", text("sale.complete")),
            ("entity", text("sale")),
            ("entity_id", blob(COMPLETED_SALE_ID)),
            ("payload", text("{}")),
            ("prev_hash", hash(0)),
            ("hash", hash(29)),
            ("at", text(FIXED_AT)),
        ],
    );
    conn.execute(
        "UPDATE sale SET status = 'completed' WHERE id = ?1",
        [bytes(COMPLETED_SALE_ID)],
    )
    .expect("the complete sale fixture must satisfy every final-schema precondition");

    conn.execute(
        "UPDATE print_job
            SET state = 'printing', claimed_at = ?1, lease_owner = 'worker',
                lease_expires_at = '2026-08-25T10:05:00.000Z'
          WHERE id = ?2",
        params![FIXED_AT, bytes(30)],
    )
    .unwrap();
    insert(
        conn,
        "print_attempt",
        &[
            ("id", blob(31)),
            ("job_id", blob(30)),
            ("attempt_no", Value::Integer(1)),
            ("outcome", text("printed")),
            ("started_at", text(FIXED_AT)),
            ("finished_at", text(FIXED_AT)),
            ("sent_at", text(FIXED_AT)),
        ],
    );
}

fn seed_sale_line(
    conn: &Connection,
    id: u16,
    sale_id: u16,
    product_id: u16,
    line_no: i64,
    net_minor: i64,
    discount_minor: i64,
) {
    insert(
        conn,
        "sale_line",
        &[
            ("id", blob(id)),
            ("sale_id", blob(sale_id)),
            ("product_id", blob(product_id)),
            ("qty_milli", Value::Integer(1000)),
            ("qty_step_milli", Value::Integer(1000)),
            ("unit_price_minor", Value::Integer(100)),
            ("discount_minor", Value::Integer(discount_minor)),
            ("tax_minor", Value::Integer(0)),
            ("total_minor", Value::Integer(net_minor)),
            ("line_no", Value::Integer(line_no)),
            ("name_snapshot", text("Service")),
            ("net_minor", Value::Integer(net_minor)),
            ("tax_category_id", blob(6)),
            ("is_weighed", Value::Integer(0)),
        ],
    );
}

fn seed_sale_tax(conn: &Connection, id: u16, line_id: u16, base_minor: i64) {
    insert(
        conn,
        "sale_line_tax",
        &[
            ("id", blob(id)),
            ("sale_line_id", blob(line_id)),
            ("component_code", text("GST")),
            ("treatment", text("standard")),
            ("calculation_kind", text("ad_valorem")),
            ("rate_ppm", Value::Integer(0)),
            ("calculation_order", Value::Integer(0)),
            ("base_kind", text("line_net")),
            ("taxable_base_minor", Value::Integer(base_minor)),
            ("tax_minor", Value::Integer(0)),
        ],
    );
}

fn seed_sale_summary(conn: &Connection, id: u16, sale_id: u16, base_minor: i64) {
    insert(
        conn,
        "sale_tax_summary",
        &[
            ("id", blob(id)),
            ("sale_id", blob(sale_id)),
            ("component_code", text("GST")),
            ("treatment", text("standard")),
            ("calculation_kind", text("ad_valorem")),
            ("rate_ppm", Value::Integer(0)),
            ("calculation_order", Value::Integer(0)),
            ("base_kind", text("line_net")),
            ("taxable_base_minor", Value::Integer(base_minor)),
            ("net_minor", Value::Integer(base_minor)),
            ("tax_minor", Value::Integer(0)),
            ("gross_minor", Value::Integer(base_minor)),
        ],
    );
}

fn seed_approval(conn: &Connection) {
    ready_commit(
        conn,
        102,
        &[
            (1020, "approval_handle", 40),
            (1021, "audit_log", 41),
            (1022, "approval_consumption", 40),
        ],
    );
    insert(
        conn,
        "approval_handle",
        &[
            ("id", blob(40)),
            ("capability", text("price.override")),
            ("actor_id", blob(5)),
            ("approver_id", blob(50)),
            ("entity_id", blob(42)),
            ("amount_minor", Value::Integer(100)),
            ("reason", text("fixture")),
            ("issued_at", text(FIXED_AT)),
            ("expires_at", text(LATER_AT)),
            ("nonce", blob(43)),
        ],
    );
    insert(
        conn,
        "audit_log",
        &[
            ("id", blob(41)),
            ("canonical_version", Value::Integer(1)),
            ("register_id", blob(3)),
            ("actor_id", blob(5)),
            ("approver_id", blob(50)),
            ("approval_handle_id", blob(40)),
            ("action", text("price.override")),
            ("entity", text("sale")),
            ("entity_id", blob(42)),
            ("reason", text("fixture")),
            ("payload", text("{\"amount_minor\":100}")),
            ("prev_hash", hash(29)),
            ("hash", hash(41)),
            ("at", text("2026-08-25T10:30:00.000Z")),
        ],
    );
    insert(
        conn,
        "approval_consumption",
        &[
            ("handle_id", blob(40)),
            ("effect_id", blob(42)),
            ("audit_log_id", blob(41)),
            ("consumed_at", text("2026-08-25T10:30:00.000Z")),
        ],
    );
    insert(
        conn,
        "audit_checkpoint",
        &[
            ("id", blob(200)),
            ("register_id", blob(3)),
            ("last_seq", Value::Integer(2)),
            ("last_hash", hash(41)),
            ("source_kind", text("server")),
            ("anchor_ref", text("fixture")),
            ("anchored_at", text(FIXED_AT)),
        ],
    );
}

fn seed_stock_and_scale(conn: &Connection) {
    ready_commit(conn, 103, &[(1030, "stock_ledger", 50)]);
    seed_stock_event(
        conn, 50, 3, 1, 4, 2, 1000, "adjust", None, None, 100, 1000, 100, None, None,
    );
    insert(
        conn,
        "trade_scale",
        &[
            ("id", blob(51)),
            ("store_id", blob(2)),
            ("maker", text("M")),
            ("model", text("X")),
            ("serial_number", text("S")),
        ],
    );
    insert(
        conn,
        "trade_scale_verification",
        &[
            ("id", blob(52)),
            ("trade_scale_id", blob(51)),
            ("event_no", Value::Integer(1)),
            ("state", text("verified")),
            ("evidence_ref", text("fixture")),
            ("evidence_hash", hash(52)),
            ("effective_at", text(FIXED_AT)),
        ],
    );
}

#[allow(clippy::too_many_arguments)]
fn seed_stock_event(
    conn: &Connection,
    id: u16,
    register_id: u16,
    event_seq: i64,
    product_id: u16,
    store_id: u16,
    qty_delta_milli: i64,
    kind: &str,
    ref_kind: Option<&str>,
    ref_id: Option<u16>,
    unit_cost_minor: i64,
    on_hand_after_milli: i64,
    wac_after_minor: i64,
    source_column: Option<&str>,
    source_id: Option<u16>,
) {
    let mut values = vec![
        ("id", blob(id)),
        ("register_id", blob(register_id)),
        ("event_seq", Value::Integer(event_seq)),
        ("product_id", blob(product_id)),
        ("store_id", blob(store_id)),
        ("qty_delta_milli", Value::Integer(qty_delta_milli)),
        ("qty_step_milli", Value::Integer(1000)),
        ("kind", text(kind)),
        ("unit_cost_minor", Value::Integer(unit_cost_minor)),
        ("is_cost_estimated", Value::Integer(0)),
        ("on_hand_after_milli", Value::Integer(on_hand_after_milli)),
        ("wac_after_minor", Value::Integer(wac_after_minor)),
        ("is_wac_estimated", Value::Integer(0)),
        ("actor_id", blob(5)),
        ("occurred_at", text(FIXED_AT)),
        ("business_date", text(FIXED_DATE)),
    ];
    if let Some(value) = ref_kind {
        values.push(("ref_kind", text(value)));
    }
    if let Some(value) = ref_id {
        values.push(("ref_id", blob(value)));
    }
    if let (Some(column), Some(value)) = (source_column, source_id) {
        values.push((column, blob(value)));
    }
    insert(conn, "stock_ledger", &values);
}

fn seed_cash_and_shifts(conn: &Connection) {
    insert(
        conn,
        "cash_location",
        &[
            ("id", blob(60)),
            ("store_id", blob(2)),
            ("register_id", blob(3)),
            ("kind", text("drawer")),
            ("code", text("DRAWER")),
            ("name", text("Drawer")),
        ],
    );
    insert(
        conn,
        "cash_location",
        &[
            ("id", blob(61)),
            ("store_id", blob(2)),
            ("kind", text("safe")),
            ("code", text("SAFE")),
            ("name", text("Safe")),
        ],
    );
    ready_commit(
        conn,
        104,
        &[
            (1040, "shift_count_line", 63),
            (1041, "shift_close_event", 65),
        ],
    );
    insert(
        conn,
        "shift_count_line",
        &[
            ("id", blob(63)),
            ("shift_id", blob(10)),
            ("phase", text("close")),
            ("denomination_minor", Value::Integer(100)),
            ("count", Value::Integer(1)),
        ],
    );
    insert(
        conn,
        "shift_close_event",
        &[
            ("id", blob(65)),
            ("shift_id", blob(10)),
            ("sync_commit_id", blob(104)),
            ("closed_by", blob(5)),
            ("closed_at", text("2026-08-25T12:00:00.000Z")),
            ("counted_minor", Value::Integer(100)),
            ("expected_minor", Value::Integer(90)),
            ("over_short_minor", Value::Integer(10)),
            ("close_kind", text("normal")),
        ],
    );
    ready_commit(conn, 105, &[(1050, "z_report", 66)]);
    insert(
        conn,
        "z_report",
        &[
            ("id", blob(66)),
            ("shift_id", blob(10)),
            ("register_id", blob(3)),
            ("z_number", Value::Integer(1)),
            ("payload", text("{}")),
            ("generated_at", text("2026-08-25T12:01:00.000Z")),
            ("generated_by", blob(5)),
        ],
    );
    ready_commit(conn, 106, &[(1060, "cash_count", 64)]);
    insert(
        conn,
        "cash_count",
        &[
            ("id", blob(64)),
            ("location_id", blob(60)),
            ("shift_id", blob(10)),
            ("purpose", text("closing")),
            ("total_minor", Value::Integer(100)),
            (
                "denomination_payload",
                text("[{\"denomination_minor\":100,\"count\":1}]"),
            ),
            ("hash_algorithm", text("sha256")),
            ("denomination_hash", hash(64)),
            ("counted_by", blob(5)),
            ("counted_at", text("2026-08-25T11:59:00.000Z")),
        ],
    );
    ready_commit(conn, 107, &[(1070, "drawer_event", 67)]);
    insert(
        conn,
        "drawer_event",
        &[
            ("id", blob(67)),
            ("register_id", blob(3)),
            ("shift_id", blob(10)),
            ("actor_id", blob(5)),
            ("cause", text("no_sale")),
            ("source_kind", text("software_command")),
            ("reason", text("fixture")),
            ("occurred_at", text(FIXED_AT)),
        ],
    );
    ready_commit(conn, 108, &[(1080, "cash_movement", 62)]);
    insert(
        conn,
        "cash_movement",
        &[
            ("id", blob(62)),
            ("store_id", blob(2)),
            ("shift_id", blob(10)),
            ("from_location_id", blob(60)),
            ("to_location_id", blob(61)),
            ("kind", text("drop")),
            ("amount_minor", Value::Integer(100)),
            ("reason_code", text("fixture")),
            ("actor_id", blob(5)),
            ("occurred_at", text(FIXED_AT)),
        ],
    );

    ready_commit(
        conn,
        125,
        &[(1250, "shift", 210), (1251, "shift_count_line", 211)],
    );
    insert(
        conn,
        "shift",
        &[
            ("id", blob(210)),
            ("register_id", blob(3)),
            ("store_id", blob(2)),
            ("business_date", text(FIXED_DATE)),
            ("opened_by", blob(5)),
            ("opened_at", text("2026-08-25T12:02:00.000Z")),
            ("float_minor", Value::Integer(0)),
        ],
    );
    insert(
        conn,
        "shift_count_line",
        &[
            ("id", blob(211)),
            ("shift_id", blob(210)),
            ("phase", text("open")),
            ("denomination_minor", Value::Integer(50)),
            ("count", Value::Integer(1)),
        ],
    );
}

fn seed_refunds_and_stored_value(conn: &Connection) {
    insert(
        conn,
        "sale",
        &[
            ("id", blob(REFUND_SALE_ID)),
            ("receipt_number", text("CN-70")),
            ("register_id", blob(3)),
            ("status", text("parked")),
            ("subtotal_minor", Value::Integer(-100)),
            ("tax_minor", Value::Integer(0)),
            ("total_minor", Value::Integer(-99)),
            ("currency", text("JOD")),
            ("ref_sale_id", blob(COMPLETED_SALE_ID)),
            ("business_date", text(FIXED_DATE)),
            ("completed_at", text(FIXED_AT)),
            ("store_id", blob(2)),
            ("shift_id", blob(10)),
            ("cashier_id", blob(5)),
            ("doc_type", text("refund")),
            ("is_training", Value::Integer(0)),
            ("discount_minor", Value::Integer(-1)),
            ("rounding_adj_minor", Value::Integer(0)),
            ("tax_computation_policy_id", blob(9)),
            ("origin_device", text("fixture")),
        ],
    );
    seed_sale_line(conn, 71, REFUND_SALE_ID, 4, 1, 99, 1);
    insert(
        conn,
        "credit_note_context",
        &[
            ("refund_sale_id", blob(REFUND_SALE_ID)),
            ("original_sale_id", blob(COMPLETED_SALE_ID)),
            ("original_document_id", text("R-20")),
            ("original_business_date", text(FIXED_DATE)),
            ("original_total_minor", Value::Integer(99)),
            ("created_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "refund_line_link",
        &[
            ("id", blob(72)),
            ("refund_line_id", blob(71)),
            ("original_line_id", blob(COMPLETED_LINE_ID)),
            ("qty_milli", Value::Integer(1000)),
            ("original_line_no", Value::Integer(1)),
            ("original_name_snapshot", text("Service")),
            ("original_unit_price_minor", Value::Integer(100)),
            ("original_net_minor", Value::Integer(99)),
            ("original_tax_minor", Value::Integer(0)),
            ("original_total_minor", Value::Integer(99)),
            (
                "original_tax_snapshot",
                text(
                    "[{\"component_code\":\"GST\",\"treatment\":\"standard\",\"calculation_kind\":\"ad_valorem\",\"rate_ppm\":0,\"fixed_amount_minor\":null,\"fixed_currency\":null,\"fixed_basis_qty_milli\":null,\"calculation_order\":0,\"base_kind\":\"line_net\",\"taxable_base_minor\":99,\"taxable_qty_milli\":null,\"tax_minor\":0}]",
                ),
            ),
            ("remaining_before_milli", Value::Integer(1000)),
            ("remaining_after_milli", Value::Integer(0)),
            ("refund_value_minor", Value::Integer(99)),
            ("remaining_value_before_minor", Value::Integer(99)),
            ("remaining_value_after_minor", Value::Integer(0)),
            ("restock", text("none")),
            ("reason_code", text("change_of_mind")),
            ("is_window_bypassed", Value::Integer(0)),
        ],
    );
    insert(
        conn,
        "defect_resolution_event",
        &[
            ("id", blob(73)),
            ("original_line_id", blob(COMPLETED_LINE_ID)),
            ("resolution", text("repair")),
            ("consumer_consent_ref", text("fixture")),
            ("evidence_hash_algorithm", text("sha256")),
            ("evidence_hash", hash(73)),
            ("actor_id", blob(5)),
            ("occurred_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "document_link",
        &[
            ("id", blob(74)),
            ("from_sale", blob(COMPLETED_SALE_ID)),
            ("to_sale", blob(REFUND_SALE_ID)),
            ("link_kind", text("correction")),
            ("created_at", text(FIXED_AT)),
        ],
    );

    insert(
        conn,
        "stored_value_policy_version",
        &[
            ("id", blob(80)),
            ("org_id", blob(1)),
            ("policy_version", text("fixture-v1")),
            ("approval_source_ref", text("fixture")),
            ("source_hash_algorithm", text("sha256")),
            ("source_hash", hash(80)),
            ("approved_at", text(FIXED_AT)),
            ("created_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "stored_value_policy_current",
        &[
            ("org_id", blob(1)),
            ("policy_id", blob(80)),
            ("is_enabled", Value::Integer(1)),
            ("updated_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "stored_value_instrument",
        &[
            ("id", blob(81)),
            ("org_id", blob(1)),
            ("code_hash", hash(81)),
            ("currency", text("JOD")),
            ("status", text("active")),
            ("issued_at", text(FIXED_AT)),
        ],
    );
    ready_commit(conn, 109, &[(1090, "stored_value_ledger", 82)]);
    insert(
        conn,
        "stored_value_ledger",
        &[
            ("id", blob(82)),
            ("instrument_id", blob(81)),
            ("register_id", blob(3)),
            ("event_seq", Value::Integer(1)),
            ("amount_delta_minor", Value::Integer(100)),
            ("kind", text("issue")),
            ("ref_kind", text("sale")),
            ("ref_id", blob(COMPLETED_SALE_ID)),
            ("actor_id", blob(5)),
            ("tax_policy_id", blob(80)),
            ("tax_treatment_code", text("fixture")),
            ("occurred_at", text(FIXED_AT)),
        ],
    );
}

fn seed_fiscal(conn: &Connection) {
    ready_commit(
        conn,
        110,
        &[
            (1100, "fiscal_document", 90),
            (1101, "fiscal_queue_event", 92),
        ],
    );
    insert(
        conn,
        "fiscal_document",
        &[
            ("id", blob(90)),
            ("sync_commit_id", blob(110)),
            ("sale_id", blob(COMPLETED_SALE_ID)),
            ("store_id", blob(2)),
            ("doc_kind", text("invoice")),
            ("document_id", text("R-20")),
            ("profile_id", text("fixture")),
            ("issue_date", text(FIXED_DATE)),
            ("fiscal_uuid", text("uuid-90")),
            ("created_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "fiscal_queue",
        &[
            ("id", blob(91)),
            ("document_fact_id", blob(90)),
            ("sale_id", blob(COMPLETED_SALE_ID)),
            ("store_id", blob(2)),
            ("doc_kind", text("invoice")),
            ("document_id", text("R-20")),
            ("profile_id", text("fixture")),
            ("issue_date", text(FIXED_DATE)),
            ("fiscal_uuid", text("uuid-90")),
            ("state", text("queued")),
            ("attempts", Value::Integer(0)),
            ("created_at", text(FIXED_AT)),
            ("updated_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "fiscal_queue_event",
        &[
            ("id", blob(92)),
            ("queue_id", blob(91)),
            ("document_fact_id", blob(90)),
            ("sync_commit_id", blob(110)),
            ("event_no", Value::Integer(1)),
            ("state", text("queued")),
            ("occurred_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "fiscal_spec_package",
        &[
            ("id", blob(93)),
            ("package_version", text("fixture-v1")),
            ("source_uri", text("fixture")),
            ("content_hash", hash(93)),
            ("acquired_at", text(FIXED_AT)),
            ("verified_by", blob(5)),
        ],
    );
    ready_commit(conn, 111, &[(1110, "fiscal_payload_event", 94)]);
    insert(
        conn,
        "fiscal_payload_event",
        &[
            ("id", blob(94)),
            ("document_fact_id", blob(90)),
            ("sync_commit_id", blob(111)),
            ("allocation_scope_kind", text("store")),
            ("allocation_scope_id", blob(2)),
            ("allocator_ref", text("fixture")),
            ("icv", Value::Integer(1)),
            // YYYY-MM-DD: the shape the table's own CHECK enforces (D5).
            ("issue_date", text(FIXED_DATE)),
            ("invoice_type_name", text("000")),
            ("payload_xml", text("<Invoice/>")),
            ("payload_hash", text("payload-94")),
            ("builder_version", text("fixture")),
            ("spec_package_id", blob(93)),
            ("built_at", text(FIXED_AT)),
        ],
    );
    conn.execute(
        "UPDATE fiscal_queue
            SET icv = 1, invoice_type_name = '000', payload_xml = '<Invoice/>',
                payload_hash = 'payload-94', builder_version = 'fixture',
                spec_package_id = ?1
          WHERE id = ?2",
        params![bytes(93), bytes(91)],
    )
    .unwrap();
    ready_commit(conn, 112, &[(1120, "fiscal_queue_event", 95)]);
    insert(
        conn,
        "fiscal_queue_event",
        &[
            ("id", blob(95)),
            ("queue_id", blob(91)),
            ("document_fact_id", blob(90)),
            ("sync_commit_id", blob(112)),
            ("event_no", Value::Integer(2)),
            ("state", text("sending")),
            ("lease_owner", text("worker")),
            ("claimed_at", text("2026-08-25T10:01:00.000Z")),
            ("lease_expires_at", text("2026-08-25T10:06:00.000Z")),
            ("occurred_at", text("2026-08-25T10:01:00.000Z")),
        ],
    );
    ready_commit(conn, 113, &[(1130, "fiscal_reconciliation_issue", 96)]);
    insert(
        conn,
        "fiscal_reconciliation_issue",
        &[
            ("id", blob(96)),
            ("queue_id", blob(91)),
            ("document_fact_id", blob(90)),
            ("sync_commit_id", blob(113)),
            ("issue_class", text("ambiguous_response")),
            ("error_body", text("fixture")),
            ("operator_path", text("portal_reconcile")),
            ("occurred_at", text("2026-08-25T10:02:00.000Z")),
        ],
    );
    ready_commit(conn, 114, &[(1140, "fiscal_resolution_event", 97)]);
    insert(
        conn,
        "fiscal_resolution_event",
        &[
            ("id", blob(97)),
            ("issue_id", blob(96)),
            ("sync_commit_id", blob(114)),
            ("event_no", Value::Integer(1)),
            ("action", text("reconciled")),
            ("actor_id", blob(5)),
            ("occurred_at", text("2026-08-25T10:03:00.000Z")),
        ],
    );
    ready_commit(conn, 115, &[(1150, "fiscal_result", 91)]);
    insert(
        conn,
        "fiscal_result",
        &[
            ("queue_id", blob(91)),
            ("document_fact_id", blob(90)),
            ("sync_commit_id", blob(115)),
            ("sale_id", blob(COMPLETED_SALE_ID)),
            ("document_id", text("R-20")),
            ("issue_date", text(FIXED_DATE)),
            ("invoice_type_name", text("000")),
            ("fiscal_uuid", text("uuid-90")),
            ("icv", Value::Integer(1)),
            ("submitted_xml", text("<Invoice/>")),
            ("submitted_hash", text("payload-94")),
            ("qr_payload", text("qr")),
            ("qr_payload_hash", text("qr-hash")),
            ("raw_response", text("{}")),
            ("raw_response_hash", text("response-hash")),
            ("cleared_at", text("2026-08-25T10:04:00.000Z")),
            ("environment", text("mock")),
            ("spec_package_id", blob(93)),
        ],
    );
    ready_commit(conn, 116, &[(1160, "fiscal_queue_event", 98)]);
    insert(
        conn,
        "fiscal_queue_event",
        &[
            ("id", blob(98)),
            ("queue_id", blob(91)),
            ("document_fact_id", blob(90)),
            ("sync_commit_id", blob(116)),
            ("event_no", Value::Integer(3)),
            ("state", text("cleared")),
            ("occurred_at", text("2026-08-25T10:04:00.000Z")),
        ],
    );
}

fn seed_privacy_and_loyalty(conn: &Connection) {
    insert(
        conn,
        "customer",
        &[
            ("id", blob(120)),
            ("org_id", blob(1)),
            ("name", text("Fixture Customer")),
            ("is_anonymized", Value::Integer(0)),
        ],
    );
    insert(
        conn,
        "consent_notice",
        &[
            ("id", blob(121)),
            ("org_id", blob(1)),
            ("kind", text("marketing")),
            ("text_version", text("v1")),
            ("locale", text("ar")),
            ("controller_name", text("Fixture Org")),
            ("controller_contact", text("fixture")),
            ("wording", text("fixture")),
            ("purpose_options_json", text("[]")),
            ("data_categories_json", text("[]")),
            ("recipients_json", text("[]")),
            ("transfer_destinations_json", text("[]")),
            ("transfer_safeguards_json", text("[]")),
            ("retention_wording", text("fixture")),
            ("hash_algorithm", text("sha256")),
            ("wording_hash", hash(121)),
            ("published_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "privacy_lawful_basis",
        &[
            ("id", blob(122)),
            ("org_id", blob(1)),
            ("basis_code", text("consent")),
            ("source_ref", text("fixture")),
            ("source_version", text("v1")),
            ("approved_by", text("counsel-fixture")),
            ("approved_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "consent_event",
        &[
            ("id", blob(123)),
            ("org_id", blob(1)),
            ("customer_id", blob(120)),
            ("notice_id", blob(121)),
            ("kind", text("marketing")),
            ("action", text("grant")),
            ("purpose_code", text("offers")),
            ("lawful_basis_id", blob(122)),
            ("selection_json", text("{}")),
            ("origin_register_id", blob(3)),
            ("origin_event_seq", Value::Integer(1)),
            ("captured_by", blob(5)),
            ("captured_at", text(FIXED_AT)),
            ("channel", text("register")),
            ("evidence_hash_algorithm", text("sha256")),
            ("evidence_hash", hash(123)),
        ],
    );
    insert(
        conn,
        "consent_acceptance",
        &[
            ("event_id", blob(123)),
            ("org_id", blob(1)),
            ("server_version", Value::Integer(1)),
            ("accepted_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "privacy_request_case",
        &[
            ("id", blob(124)),
            ("org_id", blob(1)),
            ("customer_id", blob(120)),
            ("request_kind", text("access")),
            ("received_at", text(FIXED_AT)),
            ("due_at", text("2026-09-25T10:00:00.000Z")),
            ("intake_channel", text("backoffice")),
            ("identity_evidence_hash", hash(124)),
        ],
    );
    insert(
        conn,
        "privacy_request_event",
        &[
            ("id", blob(125)),
            ("case_id", blob(124)),
            ("event_no", Value::Integer(1)),
            ("action", text("received")),
            ("actor_id", blob(5)),
            ("occurred_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "privacy_tombstone",
        &[
            ("id", blob(126)),
            ("org_id", blob(1)),
            ("customer_id", blob(120)),
            ("request_id", blob(124)),
            ("subject_hmac", hash(126)),
            ("hmac_key_version", text("v1")),
            ("reason_code", text("fixture")),
            ("anonymized_at", text(FIXED_AT)),
            ("actor_id", blob(5)),
        ],
    );
    insert(
        conn,
        "loyalty_tax_policy_version",
        &[
            ("id", blob(127)),
            ("org_id", blob(1)),
            ("policy_version", text("fixture-v1")),
            ("funding_source", text("merchant")),
            ("approval_source_ref", text("fixture")),
            ("source_hash_algorithm", text("sha256")),
            ("source_hash", hash(127)),
            ("approved_at", text(FIXED_AT)),
            ("created_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "loyalty_tax_policy_current",
        &[
            ("org_id", blob(1)),
            ("policy_id", blob(127)),
            ("is_enabled", Value::Integer(1)),
            ("updated_at", text(FIXED_AT)),
        ],
    );
    ready_commit(conn, 117, &[(1170, "loyalty_ledger", 128)]);
    insert(
        conn,
        "loyalty_ledger",
        &[
            ("id", blob(128)),
            ("customer_id", blob(120)),
            ("points_delta", Value::Integer(10)),
            ("kind", text("earn")),
            ("ref_kind", text("sale")),
            ("ref_id", blob(COMPLETED_SALE_ID)),
            ("funding_source", text("merchant")),
            ("reimbursed_minor", Value::Integer(0)),
            ("tax_policy_id", blob(127)),
            ("actor_id", blob(5)),
            ("occurred_at", text(FIXED_AT)),
        ],
    );
}

fn seed_promotions(conn: &Connection) {
    insert(
        conn,
        "promotion",
        &[
            ("id", blob(135)),
            ("org_id", blob(1)),
            ("code", text("PROMO")),
            ("updated_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "promotion_version",
        &[
            ("id", blob(134)),
            ("promotion_id", blob(135)),
            ("version_no", Value::Integer(1)),
            ("name_ar", text("عرض")),
            ("kind", text("amount_off")),
            ("config_json", text("{}")),
            ("eligibility_json", text("{}")),
            ("requalify_policy", text("deal_break")),
            ("content_hash", hash(134)),
            ("created_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "promotion_regulated_exclusion",
        &[
            ("promotion_version_id", blob(134)),
            ("regulated_kind", text("tobacco")),
            ("evidence_hash", hash(133)),
        ],
    );
    insert(
        conn,
        "promotion_publication",
        &[
            ("id", blob(137)),
            ("promotion_version_id", blob(134)),
            ("copy", text("fixture")),
            ("channel", text("receipt")),
            ("artifact_hash", hash(137)),
            ("published_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "sale",
        &[
            ("id", blob(PARKED_SALE_ID)),
            ("receipt_number", text("R-130")),
            ("register_id", blob(3)),
            ("status", text("parked")),
            ("subtotal_minor", Value::Integer(100)),
            ("tax_minor", Value::Integer(0)),
            ("total_minor", Value::Integer(95)),
            ("currency", text("JOD")),
            ("business_date", text(FIXED_DATE)),
            ("completed_at", text(FIXED_AT)),
            ("store_id", blob(2)),
            ("shift_id", blob(10)),
            ("cashier_id", blob(5)),
            ("doc_type", text("sale")),
            ("is_training", Value::Integer(1)),
            ("discount_minor", Value::Integer(5)),
            ("rounding_adj_minor", Value::Integer(0)),
            ("tax_computation_policy_id", blob(9)),
            ("origin_device", text("fixture")),
        ],
    );
    insert(
        conn,
        "sale_supply_tax_context",
        &[
            ("sale_id", blob(PARKED_SALE_ID)),
            ("destination_code", text("JO")),
            ("captured_at", text(FIXED_AT)),
        ],
    );
    seed_sale_line(conn, PARKED_LINE_ID, PARKED_SALE_ID, 4, 1, 95, 5);
    insert(
        conn,
        "sale_tender",
        &[
            ("id", blob(PARKED_TENDER_ID)),
            ("sale_id", blob(PARKED_SALE_ID)),
            ("method", text("exchange")),
            ("amount_minor", Value::Integer(95)),
            ("change_minor", Value::Integer(0)),
        ],
    );
    seed_sale_tax(conn, 203, PARKED_LINE_ID, 95);
    insert(
        conn,
        "sale_line_discount",
        &[
            ("id", blob(132)),
            ("sale_line_id", blob(PARKED_LINE_ID)),
            ("source", text("promotion")),
            ("amount_minor", Value::Integer(5)),
        ],
    );
    seed_sale_summary(conn, 204, PARKED_SALE_ID, 95);
    // Separate mutable rows let the positive-control test perform real DELETEs
    // without an unrelated child foreign key deciding the result first.
    seed_sale_line(conn, 218, PARKED_SALE_ID, 4, 2, 100, 0);
    insert(
        conn,
        "sale_line_discount",
        &[
            ("id", blob(219)),
            ("sale_line_id", blob(218)),
            ("source", text("manual_line")),
            ("amount_minor", Value::Integer(0)),
        ],
    );
    seed_sale_line(conn, 220, PARKED_SALE_ID, 4, 3, 100, 0);
    insert(
        conn,
        "sale",
        &[
            ("id", blob(221)),
            ("receipt_number", text("R-221")),
            ("register_id", blob(3)),
            ("status", text("parked")),
            ("subtotal_minor", Value::Integer(0)),
            ("tax_minor", Value::Integer(0)),
            ("total_minor", Value::Integer(0)),
            ("currency", text("JOD")),
            ("business_date", text(FIXED_DATE)),
            ("completed_at", text(FIXED_AT)),
            ("store_id", blob(2)),
            ("shift_id", blob(10)),
            ("cashier_id", blob(5)),
            ("doc_type", text("sale")),
            ("is_training", Value::Integer(1)),
            ("discount_minor", Value::Integer(0)),
            ("rounding_adj_minor", Value::Integer(0)),
            ("tax_computation_policy_id", blob(9)),
            ("origin_device", text("fixture")),
        ],
    );
    insert(
        conn,
        "promotion_attribution",
        &[
            ("id", blob(133)),
            ("sale_line_discount_id", blob(132)),
            ("promotion_version_id", blob(134)),
            ("eligible_input_snapshot", text("{}")),
            ("amount_minor", Value::Integer(5)),
            ("promised_terms_hash", hash(133)),
            ("applied_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "product",
        &[
            ("id", blob(136)),
            ("sku", text("TOBACCO-1")),
            ("name", text("Tobacco")),
            ("price_minor", Value::Integer(100)),
            ("currency", text("JOD")),
            ("name_ar", text("تبغ")),
            ("tax_category_id", blob(6)),
            ("unit", text("each")),
            ("qty_step_milli", Value::Integer(1000)),
            ("is_weighed", Value::Integer(0)),
            ("is_service", Value::Integer(0)),
            ("regulated_kind", text("tobacco")),
            ("sale_form", text("sealed_pack")),
        ],
    );
    insert(
        conn,
        "regulated_display_approval",
        &[
            ("id", blob(138)),
            ("product_id", blob(136)),
            ("policy_version", text("fixture-v1")),
            ("evidence_ref", text("fixture")),
            ("evidence_hash_algorithm", text("sha256")),
            ("evidence_hash", hash(138)),
            ("approved_by", blob(5)),
            ("approved_at", text(FIXED_AT)),
        ],
    );
}

fn seed_supply_and_inventory(conn: &Connection) {
    insert(
        conn,
        "supplier",
        &[
            ("id", blob(140)),
            ("org_id", blob(1)),
            ("name", text("Supplier")),
            ("updated_at", text(FIXED_AT)),
            ("version", Value::Integer(0)),
        ],
    );
    ready_commit(
        conn,
        118,
        &[
            (1180, "supplier_invoice", 141),
            (1181, "supplier_invoice_line", 142),
            (1182, "supplier_invoice_line_tax", 143),
            (1183, "supplier_invoice_post_event", 144),
        ],
    );
    insert(
        conn,
        "supplier_invoice",
        &[
            ("id", blob(141)),
            ("org_id", blob(1)),
            ("store_id", blob(2)),
            ("supplier_id", blob(140)),
            ("document_kind", text("domestic_invoice")),
            ("document_number", text("INV-1")),
            ("document_date", text(FIXED_DATE)),
            ("currency", text("JOD")),
            ("net_minor", Value::Integer(100)),
            ("tax_minor", Value::Integer(0)),
            ("gross_minor", Value::Integer(100)),
            ("evidence_hash_algorithm", text("sha256")),
            ("evidence_hash", hash(141)),
            ("captured_by", blob(5)),
            ("captured_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "supplier_invoice_line",
        &[
            ("id", blob(142)),
            ("supplier_invoice_id", blob(141)),
            ("line_no", Value::Integer(1)),
            ("product_id", blob(4)),
            ("description_snapshot", text("Supply")),
            ("qty_milli", Value::Integer(1000)),
            ("net_minor", Value::Integer(100)),
            ("tax_minor", Value::Integer(0)),
            ("gross_minor", Value::Integer(100)),
            ("deductibility_class", text("fully_deductible")),
            ("deductible_ppm", Value::Integer(1_000_000)),
            ("input_class", text("inventory")),
            ("nondeductible_tax_minor", Value::Integer(0)),
        ],
    );
    insert(
        conn,
        "supplier_invoice_line_tax",
        &[
            ("id", blob(143)),
            ("supplier_invoice_line_id", blob(142)),
            ("component_code", text("GST")),
            ("treatment", text("standard")),
            ("calculation_kind", text("ad_valorem")),
            ("rate_ppm", Value::Integer(0)),
            ("calculation_order", Value::Integer(0)),
            ("base_kind", text("line_net")),
            ("taxable_base_minor", Value::Integer(100)),
            ("tax_minor", Value::Integer(0)),
            ("return_box_code", text("fixture")),
        ],
    );
    insert(
        conn,
        "supplier_invoice_post_event",
        &[
            ("id", blob(144)),
            ("supplier_invoice_id", blob(141)),
            ("sync_commit_id", blob(118)),
            ("line_count", Value::Integer(1)),
            ("content_hash", hash(144)),
            ("posted_by", blob(5)),
            ("posted_at", text(FIXED_AT)),
        ],
    );

    ready_commit(
        conn,
        119,
        &[
            (1190, "goods_receipt", 150),
            (1191, "goods_receipt_line", 151),
            (1192, "stock_ledger", 153),
            (1193, "goods_receipt_post_event", 152),
        ],
    );
    insert(
        conn,
        "goods_receipt",
        &[
            ("id", blob(150)),
            ("store_id", blob(2)),
            ("supplier_id", blob(140)),
            ("supplier_invoice_id", blob(141)),
            ("reference", text("GR-1")),
            ("received_by", blob(5)),
            ("received_at", text(FIXED_AT)),
            ("business_date", text(FIXED_DATE)),
        ],
    );
    insert(
        conn,
        "goods_receipt_line",
        &[
            ("id", blob(151)),
            ("receipt_id", blob(150)),
            ("product_id", blob(4)),
            ("qty_milli", Value::Integer(1000)),
            ("unit_cost_minor", Value::Integer(100)),
            ("source_invoice_line_id", blob(142)),
            ("is_cost_confirmed", Value::Integer(1)),
            ("allocated_net_minor", Value::Integer(100)),
            ("allocated_nondeductible_tax_minor", Value::Integer(0)),
            ("inventory_cost_minor", Value::Integer(100)),
        ],
    );
    seed_stock_event(
        conn,
        153,
        3,
        2,
        4,
        2,
        1000,
        "receive",
        Some("goods_receipt"),
        Some(150),
        100,
        2000,
        100,
        Some("source_goods_receipt_line_id"),
        Some(151),
    );
    insert(
        conn,
        "goods_receipt_post_event",
        &[
            ("id", blob(152)),
            ("goods_receipt_id", blob(150)),
            ("sync_commit_id", blob(119)),
            ("line_count", Value::Integer(1)),
            ("content_hash", hash(152)),
            ("posted_by", blob(5)),
            ("posted_at", text(FIXED_AT)),
        ],
    );

    ready_commit(
        conn,
        120,
        &[
            (1200, "stock_count", 160),
            (1201, "stock_count_line", 161),
            (1202, "stock_count_post_event", 162),
        ],
    );
    insert(
        conn,
        "stock_count",
        &[
            ("id", blob(160)),
            ("store_id", blob(2)),
            ("started_at", text(FIXED_AT)),
            ("started_by", blob(5)),
            ("scope", text("full")),
        ],
    );
    insert(
        conn,
        "stock_count_line",
        &[
            ("id", blob(161)),
            ("count_id", blob(160)),
            ("product_id", blob(4)),
            ("expected_milli", Value::Integer(2000)),
            ("counted_milli", Value::Integer(2000)),
            ("variance_milli", Value::Integer(0)),
        ],
    );
    insert(
        conn,
        "stock_count_post_event",
        &[
            ("id", blob(162)),
            ("stock_count_id", blob(160)),
            ("sync_commit_id", blob(120)),
            ("line_count", Value::Integer(1)),
            ("content_hash", hash(162)),
            ("posted_by", blob(5)),
            ("posted_at", text(FIXED_AT)),
        ],
    );

    seed_store(conn, 170, "S2", "متجر ٢");
    seed_register(conn, 171, 170, "R2", "device-2", "key-2");
    ready_commit(
        conn,
        121,
        &[
            (1210, "transfer", 172),
            (1211, "transfer_line", 173),
            (1212, "stock_ledger", 174),
            (1213, "transfer_ship_event", 175),
        ],
    );
    seed_transfer(conn, 172);
    insert(
        conn,
        "transfer_line",
        &[
            ("id", blob(173)),
            ("transfer_id", blob(172)),
            ("product_id", blob(4)),
            ("qty_sent_milli", Value::Integer(1000)),
        ],
    );
    seed_stock_event(
        conn,
        174,
        3,
        3,
        4,
        2,
        -1000,
        "transfer_out",
        Some("transfer"),
        Some(172),
        100,
        1000,
        100,
        Some("source_transfer_line_id"),
        Some(173),
    );
    insert(
        conn,
        "transfer_ship_event",
        &[
            ("id", blob(175)),
            ("transfer_id", blob(172)),
            ("sync_commit_id", blob(121)),
            ("line_count", Value::Integer(1)),
            ("content_hash", hash(175)),
            ("sent_by", blob(5)),
            ("sent_at", text(FIXED_AT)),
        ],
    );
    ready_commit(
        conn,
        122,
        &[
            (1220, "transfer_receipt_line", 176),
            (1221, "stock_ledger", 177),
            (1222, "transfer_receive_event", 178),
        ],
    );
    insert(
        conn,
        "transfer_receipt_line",
        &[
            ("id", blob(176)),
            ("transfer_line_id", blob(173)),
            ("qty_received_milli", Value::Integer(1000)),
            ("qty_damaged_milli", Value::Integer(0)),
        ],
    );
    seed_stock_event(
        conn,
        177,
        171,
        1,
        4,
        170,
        1000,
        "transfer_in",
        Some("transfer"),
        Some(172),
        100,
        1000,
        100,
        Some("source_transfer_receipt_line_id"),
        Some(176),
    );
    insert(
        conn,
        "transfer_receive_event",
        &[
            ("id", blob(178)),
            ("transfer_id", blob(172)),
            ("sync_commit_id", blob(122)),
            ("line_count", Value::Integer(1)),
            ("content_hash", hash(178)),
            ("received_by", blob(5)),
            ("received_at", text(FIXED_AT)),
        ],
    );
    ready_commit(
        conn,
        123,
        &[
            (1230, "transfer", 179),
            (1231, "transfer_cancel_event", 180),
        ],
    );
    seed_transfer(conn, 179);
    insert(
        conn,
        "transfer_cancel_event",
        &[
            ("id", blob(180)),
            ("transfer_id", blob(179)),
            ("sync_commit_id", blob(123)),
            ("reason", text("fixture")),
            ("cancelled_by", blob(5)),
            ("cancelled_at", text(FIXED_AT)),
        ],
    );

    ready_commit(
        conn,
        126,
        &[
            (1260, "goods_receipt", 212),
            (1261, "goods_receipt_line", 213),
        ],
    );
    insert(
        conn,
        "goods_receipt",
        &[
            ("id", blob(212)),
            ("store_id", blob(2)),
            ("supplier_id", blob(140)),
            ("reference", text("GR-DRAFT")),
            ("received_by", blob(5)),
            ("received_at", text(FIXED_AT)),
            ("business_date", text(FIXED_DATE)),
        ],
    );
    insert(
        conn,
        "goods_receipt_line",
        &[
            ("id", blob(213)),
            ("receipt_id", blob(212)),
            ("product_id", blob(4)),
            ("qty_milli", Value::Integer(1000)),
            ("unit_cost_minor", Value::Integer(100)),
            ("is_cost_confirmed", Value::Integer(0)),
        ],
    );
    ready_commit(
        conn,
        127,
        &[(1270, "stock_count", 214), (1271, "stock_count_line", 215)],
    );
    insert(
        conn,
        "stock_count",
        &[
            ("id", blob(214)),
            ("store_id", blob(2)),
            ("started_at", text(FIXED_AT)),
            ("started_by", blob(5)),
            ("scope", text("full")),
        ],
    );
    insert(
        conn,
        "stock_count_line",
        &[
            ("id", blob(215)),
            ("count_id", blob(214)),
            ("product_id", blob(4)),
            ("expected_milli", Value::Integer(1000)),
        ],
    );
    ready_commit(
        conn,
        128,
        &[(1280, "transfer", 216), (1281, "transfer_line", 217)],
    );
    seed_transfer(conn, 216);
    insert(
        conn,
        "transfer_line",
        &[
            ("id", blob(217)),
            ("transfer_id", blob(216)),
            ("product_id", blob(4)),
            ("qty_sent_milli", Value::Integer(1000)),
        ],
    );
}

fn seed_transfer(conn: &Connection, id: u16) {
    insert(
        conn,
        "transfer",
        &[
            ("id", blob(id)),
            ("from_store", blob(2)),
            ("to_store", blob(170)),
            ("created_by", blob(5)),
            ("created_at", text(FIXED_AT)),
        ],
    );
}

fn seed_tax_filing(conn: &Connection) {
    insert(
        conn,
        "tax_filing_profile",
        &[
            ("id", blob(190)),
            ("store_id", blob(2)),
            ("taxpayer_number", text("TIN")),
            ("return_type", text("GST")),
            ("cycle_code", text("monthly")),
            ("jurisdiction_code", text("JO")),
            ("source_version", text("fixture-v1")),
            ("effective_from", text("2026-01-01")),
        ],
    );
    insert(
        conn,
        "tax_filing_period",
        &[
            ("id", blob(191)),
            ("filing_profile_id", blob(190)),
            ("period_start_date", text("2026-08-01")),
            ("period_end_date", text("2026-08-31")),
            ("due_date", text("2026-09-30")),
        ],
    );
    insert(
        conn,
        "tax_filing_event",
        &[
            ("id", blob(192)),
            ("filing_period_id", blob(191)),
            ("event_no", Value::Integer(1)),
            ("action", text("opened")),
            ("evidence_hash", hash(192)),
            ("actor_id", blob(5)),
            ("occurred_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "tax_period_adjustment",
        &[
            ("id", blob(193)),
            ("org_id", blob(1)),
            ("store_id", blob(2)),
            ("filing_period_id", blob(191)),
            ("adjustment_code", text("fixture")),
            ("net_delta_minor", Value::Integer(1)),
            ("tax_delta_minor", Value::Integer(0)),
            ("source_ref", text("fixture")),
            ("evidence_hash", hash(193)),
            ("policy_version", text("fixture-v1")),
            ("recorded_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "common_input_allocation",
        &[
            ("id", blob(194)),
            ("org_id", blob(1)),
            ("filing_period_id", blob(191)),
            ("allocation_method_code", text("fixture")),
            ("numerator_minor", Value::Integer(1)),
            ("denominator_minor", Value::Integer(1)),
            ("deductible_ppm", Value::Integer(1_000_000)),
            ("source_ref", text("fixture")),
            ("evidence_hash", hash(194)),
            ("policy_version", text("fixture-v1")),
            ("calculated_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "tax_credit_ledger",
        &[
            ("id", blob(195)),
            ("org_id", blob(1)),
            ("filing_period_id", blob(191)),
            ("amount_delta_minor", Value::Integer(1)),
            ("kind", text("opening_credit")),
            ("source_ref", text("fixture")),
            ("evidence_hash", hash(195)),
            ("occurred_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "tax_filing_election",
        &[
            ("id", blob(196)),
            ("org_id", blob(1)),
            ("filing_period_id", blob(191)),
            ("election_code", text("fixture")),
            ("amount_minor", Value::Integer(1)),
            ("source_ref", text("fixture")),
            ("evidence_hash", hash(196)),
            ("elected_by", blob(5)),
            ("elected_at", text(FIXED_AT)),
        ],
    );
    insert(
        conn,
        "credit_note_period_assignment",
        &[
            ("refund_sale_id", blob(REFUND_SALE_ID)),
            ("original_period_id", blob(191)),
            ("credit_note_period_id", blob(191)),
            ("return_box_code", text("fixture")),
            ("policy_version", text("fixture-v1")),
            ("assigned_at", text(FIXED_AT)),
        ],
    );
}

/// Seed every target fact through its real append or lifecycle transition.
pub fn seed_fact_world(conn: &Connection) {
    seed_reference_world(conn);
    seed_sales(conn);
    seed_approval(conn);
    seed_stock_and_scale(conn);
    seed_cash_and_shifts(conn);
    seed_refunds_and_stored_value(conn);
    seed_fiscal(conn);
    seed_privacy_and_loyalty(conn);
    seed_promotions(conn);
    seed_supply_and_inventory(conn);
    seed_tax_filing(conn);

    for table in declared_fact_tables() {
        let count: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(count > 0, "{table} has no exercised fact row");
    }

    let mut statement = conn.prepare("PRAGMA foreign_key_check").unwrap();
    let violations: Vec<String> = statement
        .query_map([], |row| {
            Ok(format!(
                "{} row {} -> {} (fk {})",
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        violations.is_empty(),
        "fact fixture contains orphan rows:\n  {}",
        violations.join("\n  ")
    );
    drop(statement);
    conn.pragma_update(None, "foreign_keys", true).unwrap();
}

/// The row each table-driven guard case targets.
pub fn fact_entity_id(table: &str) -> Vec<u8> {
    let id = match table {
        "sync_commit" => 101,
        "fact_commit_member" => 1001,
        "sale" | "sale_supply_tax_context" => COMPLETED_SALE_ID,
        "sale_line" => COMPLETED_LINE_ID,
        "sale_tender" => COMPLETED_TENDER_ID,
        "sale_line_tax" => 23,
        "sale_line_discount" => 24,
        "sale_tax_summary" => 25,
        "receipt_artifact" => 27,
        "print_attempt" => 31,
        "tender_status_event" => 28,
        "shift" => 10,
        "shift_close_event" => 65,
        "shift_count_line" => 63,
        "approval_handle" | "approval_consumption" => 40,
        "audit_log" => 41,
        "audit_checkpoint" => 200,
        "stock_ledger" => 50,
        "trade_scale_verification" => 52,
        "cash_movement" => 62,
        "cash_count" => 64,
        "z_report" => 66,
        "drawer_event" => 67,
        "credit_note_context" | "credit_note_period_assignment" => REFUND_SALE_ID,
        "refund_line_link" => 72,
        "defect_resolution_event" => 73,
        "document_link" => 74,
        "stored_value_ledger" => 82,
        "fiscal_document" => 90,
        "fiscal_payload_event" => 94,
        "fiscal_queue_event" => 92,
        "fiscal_result" => 91,
        "fiscal_reconciliation_issue" => 96,
        "fiscal_resolution_event" => 97,
        "consent_event" | "consent_acceptance" => 123,
        "privacy_request_case" => 124,
        "privacy_request_event" => 125,
        "privacy_tombstone" => 126,
        "loyalty_ledger" => 128,
        "promotion_version" | "promotion_regulated_exclusion" => 134,
        "promotion_publication" => 137,
        "promotion_attribution" => 133,
        "regulated_display_approval" => 138,
        "supplier_invoice" => 141,
        "supplier_invoice_line" => 142,
        "supplier_invoice_line_tax" => 143,
        "supplier_invoice_post_event" => 144,
        "goods_receipt" => 150,
        "goods_receipt_line" => 151,
        "goods_receipt_post_event" => 152,
        "stock_count" => 160,
        "stock_count_line" => 161,
        "stock_count_post_event" => 162,
        "transfer" => 172,
        "transfer_line" => 173,
        "transfer_ship_event" => 175,
        "transfer_receipt_line" => 176,
        "transfer_receive_event" => 178,
        "transfer_cancel_event" => 180,
        "tax_filing_event" => 192,
        "tax_period_adjustment" => 193,
        "common_input_allocation" => 194,
        "tax_credit_ledger" => 195,
        "tax_filing_election" => 196,
        other => panic!("{other} is declared as a fact but has no target fixture id"),
    };
    bytes(id)
}

pub fn identity_column(table: &str) -> &'static str {
    match table {
        "fact_commit_member" => "change_id",
        "sale_supply_tax_context" => "sale_id",
        "approval_consumption" => "handle_id",
        "fiscal_result" => "queue_id",
        "consent_acceptance" => "event_id",
        "promotion_regulated_exclusion" => "promotion_version_id",
        "credit_note_context" | "credit_note_period_assignment" => "refund_sale_id",
        _ => "id",
    }
}

pub fn append_tender_collection(conn: &Connection) {
    ready_commit(conn, 124, &[(1240, "tender_status_event", 201)]);
    insert(
        conn,
        "tender_status_event",
        &[
            ("id", blob(201)),
            ("tender_id", blob(COMPLETED_TENDER_ID)),
            ("sync_commit_id", blob(124)),
            ("event_no", Value::Integer(2)),
            ("state", text("collected")),
            ("occurred_at", text("2026-08-25T10:05:00.000Z")),
        ],
    );
}
