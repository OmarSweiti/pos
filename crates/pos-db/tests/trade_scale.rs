//! `TradeScaleRepository` (microstep 1.2.4's database half): the evidence a
//! scale-priced label depends on, read back as the domain decides on it, and
//! recorded only with the delivery envelope it travels in (I-9).
//!
//! Every test name carries `trade_scale_`, which is what 1.2.4's `Done when`
//! filters on. Scale identity and label layouts are reference data the server
//! sends down, so they go in by hand; verification events go in through the
//! repository, each after its envelope, as a register will write them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::DbError;
use pos_db::fixtures::registered_chain::RegisteredChain;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_db::repo::trade_scale::{ENTITY, Id, ScaleRule, TradeScaleRepository, VerificationEvent};
use pos_domain::{
    ClockConfidence, EmbeddedBarcodeRule, EmbeddedValue, ScalePricingRefusal, ScaleVerification,
    Timestamp, VerificationState, parse_scan, scale_pricing_allowed,
};
use rusqlite::{Connection, params};
use uuid::Uuid;

const KEY: &str = "trade-scale-test-key";
const SCALE: u128 = 0x40;

struct Store {
    _dir: tempfile::TempDir,
    conn: Connection,
    chain: RegisteredChain,
}

fn store() -> Store {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    let s = Store {
        _dir: dir,
        conn,
        chain,
    };
    s.conn
        .execute(
            "INSERT INTO trade_scale (id, store_id, maker, model, serial_number)
             VALUES (?1, ?2, 'Synthetic Scales', 'Deli-15', 'SN-0001')",
            params![id(SCALE).as_slice(), s.chain.store],
        )
        .unwrap();
    s
}

fn id(value: u128) -> Id {
    Uuid::from_u128(value).into_bytes()
}

fn at(iso: &str) -> Timestamp {
    Timestamp::parse_iso8601(iso).unwrap()
}

/// A verification event with synthetic evidence. The hash is a real BLAKE3
/// digest of an invented document reference, not a constant.
fn event(n: u128, event_no: i64, state: VerificationState) -> VerificationEvent {
    let evidence_ref = format!("JSMO-CERT-SYNTHETIC-{event_no}");
    VerificationEvent {
        id: id(n),
        trade_scale_id: id(SCALE),
        event_no,
        state,
        evidence_hash: *blake3::hash(evidence_ref.as_bytes()).as_bytes(),
        evidence_ref,
        seal_or_mark_ref: Some(format!("SEAL-{event_no}")),
        effective_at: at("2026-01-01T00:00:00.000Z"),
        valid_until: Some(at("2027-01-01T00:00:00.000Z")),
    }
}

/// Write the envelope naming `event` with `payload`, as the caller of
/// `record_verification` must, inside `tx`.
fn envelope(tx: &rusqlite::Transaction<'_>, event: &VerificationEvent, payload: &str) {
    let commit = id(0x9000 + u128::from(event.id[15]));
    let change = id(0xA000 + u128::from(event.id[15]));
    OutboxRepository::new(tx)
        .write_commit(
            tx,
            &CommitEnvelope {
                commit_id: &commit,
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "trade-scale-test",
                created_at: "2026-10-23T09:00:00.000Z",
            },
            &[FactMember {
                change_id: &change,
                entity: ENTITY,
                entity_id: &event.id,
                payload,
            }],
        )
        .unwrap();
}

/// Record `event` the way a register will: envelope first, then the fact, one
/// transaction.
fn record(s: &Store, event: &VerificationEvent) -> Result<(), DbError> {
    let tx = s.conn.unchecked_transaction().unwrap();
    envelope(&tx, event, &event.payload());
    TradeScaleRepository::new(&s.conn).record_verification(&tx, event)?;
    tx.commit().unwrap();
    Ok(())
}

fn rule(s: &Store, n: u128, prefix: &str, kind: &str, active: bool) {
    s.conn
        .execute(
            "INSERT INTO embedded_barcode_rule
               (id, store_id, trade_scale_id, prefix, item_code_start, item_code_len,
                value_start, value_len, value_kind, value_scale, verify_checksum, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, 5, 7, 5, ?6, 1, 1, ?7)",
            params![
                id(n).as_slice(),
                s.chain.store,
                id(SCALE).as_slice(),
                prefix,
                i64::try_from(prefix.len()).unwrap(),
                kind,
                active
            ],
        )
        .unwrap();
}

#[test]
fn trade_scale_evidence_travels_with_its_envelope() {
    // I-9. A verification event is a fact that syncs up, so it commits with
    // the envelope that delivers it. 0007 has no trigger for that, so the
    // repository is the line: no envelope, or one carrying anything but this
    // event's payload, and nothing is written.
    let s = store();
    let repo = TradeScaleRepository::new(&s.conn);
    let verified = event(1, 1, VerificationState::Verified);

    let tx = s.conn.unchecked_transaction().unwrap();
    assert!(matches!(
        repo.record_verification(&tx, &verified),
        Err(DbError::ScaleEvidenceEnvelopeMissing)
    ));
    drop(tx);

    let tx = s.conn.unchecked_transaction().unwrap();
    let forged = VerificationEvent {
        evidence_ref: "JSMO-CERT-SOMETHING-ELSE".to_owned(),
        ..verified.clone()
    };
    envelope(&tx, &verified, &forged.payload());
    assert!(matches!(
        repo.record_verification(&tx, &verified),
        Err(DbError::ScaleEvidenceEnvelopeMismatch)
    ));
    drop(tx);
    assert!(
        repo.history(&id(SCALE)).unwrap().is_empty(),
        "nothing was written"
    );

    record(&s, &verified).unwrap();
    assert_eq!(
        repo.history(&id(SCALE)).unwrap(),
        std::slice::from_ref(&verified)
    );
    let travelling: String = s
        .conn
        .query_row(
            "SELECT payload FROM fact_commit_member WHERE entity = ?1 AND entity_id = ?2",
            params![ENTITY, verified.id.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(travelling, verified.payload());
    // Pinned field by field rather than recomputed: only the digest is taken
    // from BLAKE3 itself, over the synthetic reference it was made from.
    let digest = blake3::hash(b"JSMO-CERT-SYNTHETIC-1").to_hex();
    assert_eq!(
        travelling,
        format!(
            concat!(
                r#"{{"effective_at":"2026-01-01T00:00:00.000Z","event_no":1,"#,
                r#""evidence_hash":"{}","evidence_ref":"JSMO-CERT-SYNTHETIC-1","#,
                r#""id":"00000000-0000-0000-0000-000000000001","seal_or_mark_ref":"SEAL-1","#,
                r#""state":"verified","trade_scale_id":"00000000-0000-0000-0000-000000000040","#,
                r#""valid_until":"2027-01-01T00:00:00.000Z"}}"#
            ),
            digest
        ),
        "the payload is the row's own columns, canonical: sorted keys, no whitespace"
    );
}

#[test]
fn trade_scale_current_verification_is_the_newest_event() {
    // Newest by the owned event number, never by a timestamp (I-7): a later
    // event with an earlier effective date is still the current one.
    let s = store();
    let repo = TradeScaleRepository::new(&s.conn);
    assert_eq!(repo.current_verification(&id(SCALE)).unwrap(), None);

    record(&s, &event(1, 1, VerificationState::Verified)).unwrap();
    record(&s, &event(2, 2, VerificationState::MaintenancePending)).unwrap();
    assert_eq!(
        repo.current_verification(&id(SCALE))
            .unwrap()
            .map(|v| v.state),
        Some(VerificationState::MaintenancePending)
    );

    let reverified = VerificationEvent {
        effective_at: at("2025-12-01T00:00:00.000Z"),
        valid_until: None,
        ..event(3, 3, VerificationState::Verified)
    };
    record(&s, &reverified).unwrap();
    assert_eq!(
        repo.current_verification(&id(SCALE)).unwrap(),
        Some(ScaleVerification {
            state: VerificationState::Verified,
            effective_at: at("2025-12-01T00:00:00.000Z"),
            valid_until: None,
        })
    );
    assert_eq!(
        repo.history(&id(SCALE))
            .unwrap()
            .iter()
            .map(|e| e.event_no)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );

    // The next number follows on, and a gap is refused by the schema.
    let tx = s.conn.unchecked_transaction().unwrap();
    assert_eq!(repo.next_event_no(&tx, &id(SCALE)).unwrap(), 4);
    drop(tx);
    let skipped = event(5, 5, VerificationState::Verified);
    let refused = record(&s, &skipped).unwrap_err();
    assert!(
        refused.to_string().contains("contiguous"),
        "a skipped event number must be refused: {refused}"
    );
}

#[test]
fn trade_scale_rules_read_back_as_the_parser_reads_them() {
    // The store's switched-on layouts, longest prefix first, each with its
    // scale, in exactly the shape `parse_scan` takes. A switched-off layout is
    // not among them.
    let s = store();
    record(&s, &event(1, 1, VerificationState::Verified)).unwrap();
    rule(&s, 0x50, "2", "price_minor", true);
    rule(&s, 0x51, "21", "weight_milli", true);
    rule(&s, 0x52, "29", "price_minor", false);

    let rules = TradeScaleRepository::new(&s.conn)
        .active_rules(&s.chain.store.as_slice().try_into().unwrap())
        .unwrap();
    let expected = |n: u128, prefix: &str, value_kind, is_active| ScaleRule {
        id: id(n),
        trade_scale_id: id(SCALE),
        rule: EmbeddedBarcodeRule {
            prefix: prefix.to_owned(),
            item_code_span: (prefix.len(), 5),
            value_span: (7, 5),
            value_kind,
            value_scale: 1,
            verify_checksum: true,
        },
        is_active,
    };
    assert_eq!(
        rules,
        [
            expected(0x51, "21", EmbeddedValue::WeightMilli, true),
            expected(0x50, "2", EmbeddedValue::PriceMinor, true),
        ]
    );
    // Every layout, when asked for all of them: the switched-off one is listed
    // and flagged, so a label it claims is still known to be a label.
    let store_id: Id = s.chain.store.as_slice().try_into().unwrap();
    assert_eq!(
        TradeScaleRepository::new(&s.conn).rules(&store_id).unwrap(),
        [
            expected(0x51, "21", EmbeddedValue::WeightMilli, true),
            expected(0x52, "29", EmbeddedValue::PriceMinor, false),
            expected(0x50, "2", EmbeddedValue::PriceMinor, true),
        ]
    );

    // A price label under the "2" layout: item 34567, 0.416 JOD.
    let layouts: Vec<EmbeddedBarcodeRule> = rules.into_iter().map(|r| r.rule).collect();
    let parsed = parse_scan("2345670004162", &layouts).unwrap();
    assert_eq!(parsed.item_code(), "34567");
}

#[test]
fn trade_scale_maintenance_stops_label_pricing_end_to_end() {
    // From stored evidence to the domain's decision. While the scale is
    // verified its labels may price a line; the moment a maintenance event is
    // recorded, 0007 switches its layout off and the domain refuses it.
    let s = store();
    let repo = TradeScaleRepository::new(&s.conn);
    let store_id: Id = s.chain.store.as_slice().try_into().unwrap();
    let now = at("2026-10-23T09:00:00.000Z");
    record(&s, &event(1, 1, VerificationState::Verified)).unwrap();
    rule(&s, 0x50, "2", "price_minor", true);

    let current = repo.current_verification(&id(SCALE)).unwrap();
    assert_eq!(
        scale_pricing_allowed(current.as_ref(), now, ClockConfidence::Trusted),
        Ok(())
    );
    assert_eq!(
        scale_pricing_allowed(current.as_ref(), now, ClockConfidence::Untrusted),
        Err(ScalePricingRefusal::ClockNotTrusted),
        "a register that does not trust its clock cannot judge the window"
    );
    assert_eq!(
        scale_pricing_allowed(
            current.as_ref(),
            at("2027-01-01T00:00:00.000Z"),
            ClockConfidence::Trusted
        ),
        Err(ScalePricingRefusal::Expired(at("2027-01-01T00:00:00.000Z")))
    );
    assert_eq!(repo.active_rules(&store_id).unwrap().len(), 1);

    record(&s, &event(2, 2, VerificationState::MaintenancePending)).unwrap();
    let current = repo.current_verification(&id(SCALE)).unwrap();
    assert_eq!(
        scale_pricing_allowed(current.as_ref(), now, ClockConfidence::Trusted),
        Err(ScalePricingRefusal::MaintenancePending)
    );
    assert!(
        repo.active_rules(&store_id).unwrap().is_empty(),
        "0007 switched the layout off in the same statement"
    );
}

#[test]
fn trade_scale_storage_alone_accepts_evidence_without_an_envelope() {
    // A tripwire for the gap the repository covers. 0007 gave
    // `trade_scale_verification` no `_has_ready_commit` trigger, so a row
    // written past the repository lands with nothing to deliver it. When a
    // migration adds the trigger this insert is refused and this test must be
    // rewritten to say so. Meanwhile a row the repository cannot read is
    // reported by name rather than guessed at.
    let s = store();
    s.conn
        .execute(
            "INSERT INTO trade_scale_verification
               (id, trade_scale_id, event_no, state, evidence_ref, evidence_hash, effective_at)
             VALUES (?1, ?2, 1, 'verified', 'JSMO-CERT-SYNTHETIC-1', ?3, 'yesterday')",
            params![
                id(1).as_slice(),
                id(SCALE).as_slice(),
                blake3::hash(b"JSMO-CERT-SYNTHETIC-1").as_bytes().as_slice()
            ],
        )
        .unwrap();
    assert!(matches!(
        TradeScaleRepository::new(&s.conn).current_verification(&id(SCALE)),
        Err(DbError::StoredScaleRecordInvalid { .. })
    ));
}
