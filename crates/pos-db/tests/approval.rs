#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Approval persistence against the exact registered migration chain.
//!
//! These fixtures open only through [`pos_db::open`]. In particular, they do
//! not use `tests/common`'s reference-schema overlay: approval durability and
//! the ready-commit guards must be proved against what a register ships.

use std::path::PathBuf;

use pos_db::DbError;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_domain::{
    ApprovalBinding, ApprovalHandle, ApprovalId, Capability, EscalationPolicy, GrantSet, Role,
    Timestamp, UserId, authorize, cap,
};
use rusqlite::{Connection, Transaction, params};
use uuid::Uuid;

const KEY: &str = "test-key";
const AT: &str = "2026-09-02T10:00:00.000Z";
const AT_MS: i64 = 1_788_343_200_000;
const TTL_MS: i64 = 5 * 60 * 1_000;
const AMOUNT_MINOR: i64 = 1_500;
const REASON: &str = "damaged box";

struct TestDb {
    // Field order is deliberate: the connection drops before the temporary
    // directory, including on Windows where an open database file cannot be
    // removed with its directory.
    conn: Connection,
    _dir: tempfile::TempDir,
    path: PathBuf,
}

#[derive(Clone, Copy)]
struct ConsumptionFixture {
    handle: ApprovalId,
    effect: Uuid,
    audit: Uuid,
    actor: UserId,
    approver: UserId,
    amount_minor: i64,
}

impl ConsumptionFixture {
    fn binding(self) -> ApprovalBinding {
        ApprovalBinding {
            entity_id: self.effect,
            amount_minor: self.amount_minor,
            content_hash: None,
        }
    }
}

fn fresh_database(name: &str) -> TestDb {
    let dir = tempfile::tempdir().expect("the fixture needs a private database directory");
    let path = dir.path().join(name);
    let conn = pos_db::open(&path, KEY).expect("the registered migration chain must open");
    TestDb {
        conn,
        _dir: dir,
        path,
    }
}

fn id(byte: u8) -> [u8; 16] {
    [byte; 16]
}

fn uuid(byte: u8) -> Uuid {
    Uuid::from_bytes(id(byte))
}

fn approval(byte: u8) -> ApprovalId {
    ApprovalId::from_uuid(uuid(byte))
}

fn user(byte: u8) -> UserId {
    UserId::from_uuid(uuid(byte))
}

fn timestamp(milliseconds: i64) -> Timestamp {
    Timestamp::from_epoch_milliseconds(milliseconds).unwrap()
}

fn seed_people(conn: &Connection, actor: UserId, approver: UserId) {
    conn.execute(
        "INSERT INTO org (id, legal_name) VALUES (?1, 'Test Org')",
        params![id(0xA0).as_slice()],
    )
    .unwrap();
    for (who, code) in [(actor, "C-1"), (approver, "M-1")] {
        conn.execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             VALUES (?1, ?2, ?3, ?3, 'placeholder-not-a-hash', ?4)",
            params![
                who.as_uuid().as_bytes().as_slice(),
                id(0xA0).as_slice(),
                code,
                AT,
            ],
        )
        .unwrap();
    }
}

/// Write a real envelope through the shipped writer, inside the caller's
/// transaction. Every payload is the fixture constant required by the wire
/// contract; time, ids, protocol and producer are all deterministic arguments.
fn write_envelope(conn: &Connection, tx: &Transaction<'_>, commit: u8, facts: &[(&str, [u8; 16])]) {
    let changes: Vec<[u8; 16]> = (0..facts.len())
        .map(|index| {
            let mut change_id = [0xC0; 16];
            change_id[14] = commit;
            change_id[15] = index as u8;
            change_id
        })
        .collect();
    let members: Vec<FactMember<'_>> = facts
        .iter()
        .zip(&changes)
        .map(|((entity, entity_id), change_id)| FactMember {
            change_id,
            entity,
            entity_id,
            payload: "{}",
        })
        .collect();
    let commit_id = id(commit);

    OutboxRepository::new(conn)
        .write_commit(
            tx,
            &CommitEnvelope {
                commit_id: &commit_id,
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "test",
                created_at: AT,
            },
            &members,
        )
        .unwrap();
}

fn issue_handle(fixture: ConsumptionFixture) -> ApprovalHandle {
    let binding = fixture.binding();
    let approver = authorize::<cap::SaleVoid>(
        fixture.approver,
        &GrantSet::of_role(Role::Manager),
        None,
        &binding,
        &EscalationPolicy::empty(),
        timestamp(AT_MS),
    )
    .expect("the manager fixture holds sale.void");

    ApprovalHandle::issue(
        fixture.handle,
        fixture.actor,
        &approver,
        &binding,
        REASON.to_owned(),
        timestamp(AT_MS),
        TTL_MS,
        id(fixture.handle.as_uuid().as_bytes()[0].wrapping_add(1)),
    )
    .expect("the deterministic fixture is a valid distinct-user approval")
}

fn persist_handle(conn: &Connection, commit: u8, handle: &ApprovalHandle) {
    let tx = conn.unchecked_transaction().unwrap();
    write_envelope(
        conn,
        &tx,
        commit,
        &[("approval_handle", *handle.id().as_uuid().as_bytes())],
    );
    ApprovalRepository::new(conn).insert(&tx, handle).unwrap();
    tx.commit().unwrap();
}

fn write_effect(tx: &Transaction<'_>, effect: Uuid, receipt: &str) {
    tx.execute(
        "INSERT INTO sale
           (id, receipt_number, register_id, status, subtotal_minor, tax_minor,
            total_minor, currency, business_date, completed_at)
         VALUES (?1, ?2, ?3, 'voided', 1500, 0, 1500, 'JOD',
                 '2026-09-02', ?4)",
        params![
            effect.as_bytes().as_slice(),
            receipt,
            id(0xF0).as_slice(),
            AT,
        ],
    )
    .unwrap();
}

/// Insert every non-generated audit column from migration 0004's worked
/// approval fixture. The reason comes from the restored handle rather than a
/// second caller-owned value.
fn write_audit(
    tx: &Transaction<'_>,
    fixture: ConsumptionFixture,
    reason: &str,
) -> rusqlite::Result<usize> {
    let payload = format!(r#"{{"amount_minor":{}}}"#, fixture.amount_minor);
    tx.execute(
        "INSERT INTO audit_log
           (id, register_id, actor_id, approver_id, approval_handle_id, action,
            entity, entity_id, reason, payload, prev_hash, hash, at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'sale', ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            fixture.audit.as_bytes().as_slice(),
            id(0xF0).as_slice(),
            fixture.actor.as_uuid().as_bytes().as_slice(),
            fixture.approver.as_uuid().as_bytes().as_slice(),
            fixture.handle.as_uuid().as_bytes().as_slice(),
            cap::SaleVoid::NAME,
            fixture.effect.as_bytes().as_slice(),
            reason,
            payload,
            id(0x00).as_slice(),
            id(0x01).as_slice(),
            AT,
        ],
    )
}

fn consume_successfully(conn: &Connection, commit: u8, fixture: ConsumptionFixture) {
    let repo = ApprovalRepository::new(conn);
    let tx = conn.unchecked_transaction().unwrap();

    repo.ensure_unconsumed(&tx, fixture.handle).unwrap();
    let stored = repo
        .load_for_consumption(&tx, fixture.handle)
        .unwrap()
        .expect("the issued handle must still exist");
    stored
        .matches::<cap::SaleVoid>(fixture.actor, &fixture.binding(), timestamp(AT_MS))
        .expect("issuance is the inclusive validity boundary");

    write_envelope(
        conn,
        &tx,
        commit,
        &[
            ("sale", *fixture.effect.as_bytes()),
            ("audit_log", *fixture.audit.as_bytes()),
            ("approval_consumption", *fixture.handle.as_uuid().as_bytes()),
        ],
    );
    write_effect(&tx, fixture.effect, "APPROVAL-SUCCESS");
    write_audit(&tx, fixture, stored.reason()).unwrap();
    repo.consume(
        &tx,
        fixture.handle,
        fixture.effect,
        fixture.audit,
        timestamp(AT_MS),
    )
    .unwrap();
    tx.commit().unwrap();
}

fn sqlite_message(error: rusqlite::Error) -> String {
    match error {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message,
        other => other.to_string(),
    }
}

#[test]
fn a_handle_used_twice_is_refused() {
    let db = fresh_database("used-twice.db");
    let actor = user(0xA1);
    let approver = user(0xA2);
    seed_people(&db.conn, actor, approver);

    let used = ConsumptionFixture {
        handle: approval(0x21),
        effect: uuid(0x31),
        audit: uuid(0x41),
        actor,
        approver,
        amount_minor: AMOUNT_MINOR,
    };
    persist_handle(&db.conn, 0x51, &issue_handle(used));
    consume_successfully(&db.conn, 0x61, used);

    // `consume` enforces one-use itself. This call deliberately omits both the
    // early check and a new envelope, and still returns the named refusal
    // before either global manifest uniqueness or SQLite's primary key can win.
    let replay_tx = db.conn.unchecked_transaction().unwrap();
    let replay = ApprovalRepository::new(&db.conn)
        .consume(
            &replay_tx,
            used.handle,
            used.effect,
            used.audit,
            timestamp(AT_MS),
        )
        .unwrap_err();
    assert!(matches!(replay, DbError::ApprovalAlreadyConsumed));
    replay_tx.rollback().unwrap();

    let consumptions: i64 = db
        .conn
        .query_row("SELECT count(*) FROM approval_consumption", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(consumptions, 1);

    // A second audit row cannot name the handle either. A real ready envelope
    // gets the insert past the BEFORE guard, leaving the partial unique index as
    // the reason this fresh audit row is refused.
    let second_audit = ConsumptionFixture {
        audit: uuid(0x42),
        ..used
    };
    let audit_tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(
        &db.conn,
        &audit_tx,
        0x62,
        &[("audit_log", *second_audit.audit.as_bytes())],
    );
    let duplicate_audit = write_audit(&audit_tx, second_audit, REASON)
        .expect_err("one handle may name at most one audit row forever");
    assert_eq!(
        sqlite_message(duplicate_audit),
        "UNIQUE constraint failed: audit_log.approval_handle_id"
    );
    audit_tx.rollback().unwrap();

    // The storage adapter gives the bound-effect trigger a stable error. The
    // financial row deliberately carries the attempted id while the immutable
    // handle and audit still name the approved id.
    let mismatched = ConsumptionFixture {
        handle: approval(0x22),
        effect: uuid(0x32),
        audit: uuid(0x43),
        actor,
        approver,
        amount_minor: AMOUNT_MINOR,
    };
    persist_handle(&db.conn, 0x52, &issue_handle(mismatched));
    let attempted_effect = uuid(0x33);
    let mismatch_tx = db.conn.unchecked_transaction().unwrap();
    let mismatch_repo = ApprovalRepository::new(&db.conn);
    mismatch_repo
        .ensure_unconsumed(&mismatch_tx, mismatched.handle)
        .unwrap();
    let restored = mismatch_repo
        .load_for_consumption(&mismatch_tx, mismatched.handle)
        .unwrap()
        .unwrap();
    restored
        .matches::<cap::SaleVoid>(mismatched.actor, &mismatched.binding(), timestamp(AT_MS))
        .unwrap();
    write_envelope(
        &db.conn,
        &mismatch_tx,
        0x63,
        &[
            ("sale", *attempted_effect.as_bytes()),
            ("audit_log", *mismatched.audit.as_bytes()),
            (
                "approval_consumption",
                *mismatched.handle.as_uuid().as_bytes(),
            ),
        ],
    );
    write_effect(&mismatch_tx, attempted_effect, "APPROVAL-MISMATCH");
    write_audit(&mismatch_tx, mismatched, restored.reason()).unwrap();
    let mismatch = mismatch_repo
        .consume(
            &mismatch_tx,
            mismatched.handle,
            attempted_effect,
            mismatched.audit,
            timestamp(AT_MS),
        )
        .unwrap_err();
    assert!(matches!(mismatch, DbError::ApprovalConsumptionUnbound));
    mismatch_tx.rollback().unwrap();

    // SQLite enforces both sides of the half-open interval. The transaction
    // remains usable after each trigger refusal, proving both bounds against
    // one otherwise-complete effect/audit/envelope graph.
    let outside = ConsumptionFixture {
        handle: approval(0x23),
        effect: uuid(0x34),
        audit: uuid(0x44),
        actor,
        approver,
        amount_minor: AMOUNT_MINOR,
    };
    persist_handle(&db.conn, 0x53, &issue_handle(outside));
    let outside_tx = db.conn.unchecked_transaction().unwrap();
    let outside_repo = ApprovalRepository::new(&db.conn);
    outside_repo
        .ensure_unconsumed(&outside_tx, outside.handle)
        .unwrap();
    let restored = outside_repo
        .load_for_consumption(&outside_tx, outside.handle)
        .unwrap()
        .unwrap();
    restored
        .matches::<cap::SaleVoid>(outside.actor, &outside.binding(), timestamp(AT_MS))
        .unwrap();
    write_envelope(
        &db.conn,
        &outside_tx,
        0x64,
        &[
            ("sale", *outside.effect.as_bytes()),
            ("audit_log", *outside.audit.as_bytes()),
            ("approval_consumption", *outside.handle.as_uuid().as_bytes()),
        ],
    );
    write_effect(&outside_tx, outside.effect, "APPROVAL-OUTSIDE");
    write_audit(&outside_tx, outside, restored.reason()).unwrap();
    for invalid_time in [AT_MS - 1, AT_MS + TTL_MS] {
        let error = outside_repo
            .consume(
                &outside_tx,
                outside.handle,
                outside.effect,
                outside.audit,
                timestamp(invalid_time),
            )
            .unwrap_err();
        assert!(matches!(error, DbError::ApprovalConsumptionUnbound));
    }
    outside_tx.rollback().unwrap();
}

#[test]
fn a_consumed_handle_is_still_consumed_after_restart() {
    let db = fresh_database("restart.db");
    let fixture = ConsumptionFixture {
        handle: approval(0x21),
        effect: uuid(0x31),
        audit: uuid(0x41),
        actor: user(0xA1),
        approver: user(0xA2),
        amount_minor: AMOUNT_MINOR,
    };
    seed_people(&db.conn, fixture.actor, fixture.approver);
    persist_handle(&db.conn, 0x51, &issue_handle(fixture));
    consume_successfully(&db.conn, 0x61, fixture);

    let TestDb { conn, _dir, path } = db;
    drop(conn);
    let reopened = pos_db::open(&path, KEY).expect("the same encrypted path must reopen");
    assert!(
        ApprovalRepository::new(&reopened)
            .is_consumed(fixture.handle)
            .unwrap()
    );
    drop(reopened);
    drop(_dir);
}

#[test]
fn the_effect_and_the_consumption_commit_together_or_not_at_all() {
    let db = fresh_database("rollback.db");
    let fixture = ConsumptionFixture {
        handle: approval(0x21),
        effect: uuid(0x31),
        audit: uuid(0x41),
        actor: user(0xA1),
        approver: user(0xA2),
        amount_minor: AMOUNT_MINOR,
    };
    seed_people(&db.conn, fixture.actor, fixture.approver);
    persist_handle(&db.conn, 0x51, &issue_handle(fixture));

    let repo = ApprovalRepository::new(&db.conn);
    let tx = db.conn.unchecked_transaction().unwrap();
    repo.ensure_unconsumed(&tx, fixture.handle).unwrap();
    let restored = repo
        .load_for_consumption(&tx, fixture.handle)
        .unwrap()
        .unwrap();
    restored
        .matches::<cap::SaleVoid>(fixture.actor, &fixture.binding(), timestamp(AT_MS))
        .unwrap();
    write_envelope(
        &db.conn,
        &tx,
        0x61,
        &[
            ("sale", *fixture.effect.as_bytes()),
            ("audit_log", *fixture.audit.as_bytes()),
            ("approval_consumption", *fixture.handle.as_uuid().as_bytes()),
        ],
    );
    write_effect(&tx, fixture.effect, "APPROVAL-ROLLBACK");
    write_audit(&tx, fixture, restored.reason()).unwrap();
    repo.consume(
        &tx,
        fixture.handle,
        fixture.effect,
        fixture.audit,
        timestamp(AT_MS),
    )
    .unwrap();

    for (table, key) in [
        ("sale", fixture.effect),
        ("audit_log", fixture.audit),
        ("sync_commit", uuid(0x61)),
    ] {
        let sql = format!("SELECT count(*) FROM {table} WHERE id = ?1");
        let rows: i64 = tx
            .query_row(&sql, [key.as_bytes().as_slice()], |row| row.get(0))
            .unwrap();
        assert_eq!(
            rows, 1,
            "the transaction must really contain its {table} row"
        );
    }
    assert!(repo.is_consumed(fixture.handle).unwrap());

    tx.rollback().unwrap();

    for (table, key) in [
        ("sale", fixture.effect),
        ("audit_log", fixture.audit),
        ("sync_commit", uuid(0x61)),
    ] {
        let sql = format!("SELECT count(*) FROM {table} WHERE id = ?1");
        let rows: i64 = db
            .conn
            .query_row(&sql, [key.as_bytes().as_slice()], |row| row.get(0))
            .unwrap();
        assert_eq!(
            rows, 0,
            "rollback must remove this transaction's {table} row"
        );
    }
    let consumptions: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM approval_consumption WHERE handle_id = ?1",
            [fixture.handle.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(consumptions, 0);
    assert!(!repo.is_consumed(fixture.handle).unwrap());

    let retry_tx = db.conn.unchecked_transaction().unwrap();
    repo.ensure_unconsumed(&retry_tx, fixture.handle)
        .expect("rollback must leave the handle spendable by a later transaction");
    retry_tx.rollback().unwrap();
}

#[test]
fn a_stored_handle_round_trips_through_restore() {
    let db = fresh_database("roundtrip.db");
    let fixture = ConsumptionFixture {
        handle: approval(0x21),
        effect: uuid(0x31),
        audit: uuid(0x41),
        actor: user(0xA1),
        approver: user(0xA2),
        amount_minor: AMOUNT_MINOR,
    };
    seed_people(&db.conn, fixture.actor, fixture.approver);
    let original = issue_handle(fixture);
    persist_handle(&db.conn, 0x51, &original);

    let tx = db.conn.unchecked_transaction().unwrap();
    let restored = ApprovalRepository::new(&db.conn)
        .load_for_consumption(&tx, original.id())
        .unwrap()
        .expect("the inserted handle must load");
    assert_eq!(restored, original);
    tx.rollback().unwrap();
}

#[test]
fn a_malformed_stored_handle_is_refused() {
    let db = fresh_database("malformed.db");
    let handle = approval(0x21);
    let actor = user(0xA1);
    let approver = user(0xA2);
    seed_people(&db.conn, actor, approver);

    // This simulates a pre-existing corrupt row; normal writes cannot create it
    // because migration 0004 has the same interval check as the domain seam.
    db.conn
        .pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(
        &db.conn,
        &tx,
        0x51,
        &[("approval_handle", *handle.as_uuid().as_bytes())],
    );
    tx.execute(
        "INSERT INTO approval_handle
           (id, capability, actor_id, approver_id, entity_id, amount_minor,
            content_hash, reason, issued_at, expires_at, nonce)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?8, ?9)",
        params![
            handle.as_uuid().as_bytes().as_slice(),
            cap::SaleVoid::NAME,
            actor.as_uuid().as_bytes().as_slice(),
            approver.as_uuid().as_bytes().as_slice(),
            uuid(0x31).as_bytes().as_slice(),
            AMOUNT_MINOR,
            REASON,
            AT,
            id(0x22).as_slice(),
        ],
    )
    .unwrap();
    tx.commit().unwrap();
    db.conn
        .pragma_update(None, "ignore_check_constraints", false)
        .unwrap();

    let read_tx = db.conn.unchecked_transaction().unwrap();
    let error = ApprovalRepository::new(&db.conn)
        .load_for_consumption(&read_tx, handle)
        .unwrap_err();
    let DbError::InvalidStoredApproval { reason } = error else {
        panic!("expected a malformed stored approval error, got {error}");
    };
    assert_eq!(
        reason,
        "approval validity interval is non-positive: issued at \
         Timestamp(1788343200000), expires at Timestamp(1788343200000)"
    );
    read_tx.rollback().unwrap();
}
