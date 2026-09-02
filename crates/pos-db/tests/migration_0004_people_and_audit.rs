//! Registered-chain coverage for migration 0004.
//!
//! Every test here reaches the schema through `pos_db::open`, which applies the
//! migrations the application compiles in. It deliberately does **not** use
//! `tests/common/mod.rs`'s `full_schema`: that helper replays `ref/schema.md`'s
//! reference SQL on top of the shipped chain with foreign keys turned off, so a
//! green run there proves the *document* is executable and says nothing about
//! whether a migration shipped. `authorization_scope.rs` and
//! `fact_table_guards.rs` were green against `user_role` and `approval_handle`
//! for weeks before `0004` existed. That is microstep 1.2.1's recorded failure,
//! and repeating it is what this file exists to refuse.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_domain::permissions::cap;
use rusqlite::{Connection, ErrorCode, params};

const KEY: &str = "test-key";
const AT: &str = "2026-09-02T10:00:00.000Z";

/// The four role codes migration 0004 seeds, in id order.
const ROLE_CODES: [&str; 4] = ["cashier", "shift_lead", "manager", "owner"];

/// Their deterministic UUIDv7-shaped ids, lower-case hex, in the same order.
/// `01a05f6a5800` is 2026-09-02T00:00:00.000Z; `7` and `8` are the version and
/// variant nibbles; the last byte is the role ordinal.
const ROLE_IDS: [&str; 4] = [
    "01a05f6a580070008000000000000001",
    "01a05f6a580070008000000000000002",
    "01a05f6a580070008000000000000003",
    "01a05f6a580070008000000000000004",
];

/// Every decision the `role_capability.decision` CHECK admits.
const DECISIONS: [&str; 3] = ["granted", "withheld", "sets_the_limit"];

struct TestDb {
    _dir: tempfile::TempDir,
    conn: Connection,
}

fn current_database(name: &str) -> TestDb {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join(name), KEY).unwrap();
    TestDb { _dir: dir, conn }
}

fn id(byte: u8) -> [u8; 16] {
    [byte; 16]
}

fn sqlite_message(error: rusqlite::Error) -> String {
    match error {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message,
        other => other.to_string(),
    }
}

fn error_code(error: &rusqlite::Error) -> Option<ErrorCode> {
    match error {
        rusqlite::Error::SqliteFailure(inner, _) => Some(inner.code),
        _ => None,
    }
}

/// One org and two users, so `approval_handle`'s `actor_id <> approver_id`
/// CHECK and its two foreign keys have something real to point at.
fn seed_people(conn: &Connection, actor: &[u8; 16], approver: &[u8; 16]) {
    conn.execute(
        "INSERT INTO org (id, legal_name) VALUES (?1, 'Test Org')",
        params![id(0xA0).as_slice()],
    )
    .unwrap();
    for (who, code) in [(actor, "C-1"), (approver, "M-1")] {
        conn.execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             VALUES (?1, ?2, ?3, ?3, 'placeholder-not-a-hash', ?4)",
            params![who.as_slice(), id(0xA0).as_slice(), code, AT],
        )
        .unwrap();
    }
}

/// A real delivery envelope for the named facts, written by the shipped writer.
///
/// `approval_handle`, `approval_consumption` and `audit_log` each carry a
/// `*_has_ready_commit` trigger, so I-9 is not something these tests may step
/// around: without the manifest and its `sync_outbox` rows the insert is
/// refused, which is the point of the trigger.
fn commit_envelope(conn: &Connection, commit: u8, facts: &[(&str, [u8; 16])]) {
    let members: Vec<([u8; 16], &str, [u8; 16])> = facts
        .iter()
        .enumerate()
        .map(|(index, (entity, entity_id))| {
            (id(0xC0 + index as u8 + commit * 8), *entity, *entity_id)
        })
        .collect();
    let facts: Vec<FactMember<'_>> = members
        .iter()
        .map(|(change_id, entity, entity_id)| FactMember {
            change_id,
            entity,
            entity_id,
            payload: "{}",
        })
        .collect();

    let tx = conn.unchecked_transaction().unwrap();
    OutboxRepository::new(conn)
        .write_commit(
            &tx,
            &CommitEnvelope {
                commit_id: &id(commit),
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "test",
                created_at: AT,
            },
            &facts,
        )
        .unwrap();
    tx.commit().unwrap();
}

fn exists(conn: &Connection, kind: &str, name: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = ?1 AND name = ?2)",
        params![kind, name],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn migration_0004_creates_all_tables() {
    let db = current_database("all-tables.db");

    for table in [
        "capability",
        "app_user",
        "role",
        "role_capability",
        "user_role",
        "user_session",
        "approval_handle",
        "approval_consumption",
        "auth_attempt_state",
        "audit_log",
        "audit_checkpoint",
    ] {
        assert!(
            exists(&db.conn, "table", table),
            "registered migration 0004 did not create `{table}`"
        );
    }

    // The indexes and triggers are asserted here rather than left to a reader's
    // faith: `ref/schema.md` §0002 already records that a table rebuild takes
    // its triggers with it silently, and a missing `*_no_delete` is the one
    // failure whose symptom is that nothing goes wrong.
    for index in [
        "idx_user_role_scoped",
        "idx_user_role_org_wide",
        "idx_audit_action_at",
        "idx_audit_actor_at",
        "idx_audit_approval_once",
    ] {
        assert!(
            exists(&db.conn, "index", index),
            "registered migration 0004 did not create `{index}`"
        );
    }
    for trigger in [
        "approval_handle_no_update",
        "approval_handle_no_delete",
        "approval_consumption_no_update",
        "approval_consumption_no_delete",
        "approval_consumption_matches_handle_and_audit",
        "approval_handle_has_ready_commit",
        "approval_consumption_has_ready_commit",
        "audit_log_has_ready_commit",
        "audit_log_no_update",
        "audit_log_no_delete",
        "audit_checkpoint_no_update",
        "audit_checkpoint_no_delete",
    ] {
        assert!(
            exists(&db.conn, "trigger", trigger),
            "registered migration 0004 did not create `{trigger}`"
        );
    }

    let version: i64 = db
        .conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, pos_db::SCHEMA_VERSION);
    assert!(
        version >= 4,
        "the registered chain stopped before migration 0004"
    );
}

#[test]
fn every_capability_in_cap_all_has_a_seeded_row() {
    let db = current_database("capability-seed.db");

    for name in cap::ALL {
        let present: bool = db
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM capability WHERE code = ?1)",
                [name],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            present,
            "`{name}` is declared in pos-domain and missing from 0004's capability catalogue"
        );
    }

    // Equality both ways. A capability the migration seeded and the domain does
    // not declare is the same defect seen from the other side: nothing can hold
    // it, and `role_capability` would carry four rows deciding a name that no
    // command will ever check.
    let seeded: i64 = db
        .conn
        .query_row("SELECT count(*) FROM capability", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        seeded,
        cap::ALL.len() as i64,
        "0004's capability catalogue and `cap::ALL` must hold exactly the same codes"
    );
}

#[test]
fn every_role_carries_an_explicit_grant_for_every_capability() {
    let db = current_database("role-matrix.db");

    let roles: Vec<(Vec<u8>, String)> = db
        .conn
        .prepare("SELECT id, code FROM role ORDER BY id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let codes: Vec<&str> = roles.iter().map(|(_, code)| code.as_str()).collect();
    assert_eq!(
        codes, ROLE_CODES,
        "0004 seeds the four standard roles, in id order"
    );

    // The ids are fixed literals, not `randomblob(16)`, and they are pinned here
    // because nothing else would notice them changing. Two registers that
    // invented different ids for "manager" push `role_capability` rows the
    // server cannot reconcile, and the failure surfaces as a merchant's
    // permission edit silently not applying on one till.
    let ids: Vec<String> = roles
        .iter()
        .map(|(id, _)| {
            id.iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        })
        .collect();
    assert_eq!(
        ids, ROLE_IDS,
        "0004's role ids are a contract, not a detail"
    );
    for id in &ids {
        assert_eq!(&id[12..13], "7", "{id} is not UUID version 7");
        assert!(
            matches!(&id[16..17], "8" | "9" | "a" | "b"),
            "{id} does not carry the RFC 9562 variant bits"
        );
    }

    // A fifth role cannot be invented beside the four: it would carry no matrix,
    // and `pos_domain::permissions::Role` is a closed enum of exactly these.
    db.conn
        .execute(
            "INSERT INTO role (id, code, name_ar) VALUES (?1, 'supervisor', 'مشرف')",
            params![id(0xEE).as_slice()],
        )
        .expect_err("role.code admits only the four codes pos-domain declares");

    for (role_id, code) in &roles {
        for name in cap::ALL {
            let decision: Option<String> = db
                .conn
                .query_row(
                    "SELECT decision FROM role_capability
                      WHERE role_id = ?1 AND capability = ?2",
                    params![role_id, name],
                    |row| row.get(0),
                )
                .ok();
            let decision = decision.unwrap_or_else(|| {
                panic!(
                    "no row decides `{name}` for `{code}`. An absent row is a capability \
                     nobody answered for, and 0004 can never be reopened to answer it"
                )
            });
            assert!(
                DECISIONS.contains(&decision.as_str()),
                "`{code}`/`{name}` holds an undeclared decision `{decision}`"
            );
        }
    }

    let cells: i64 = db
        .conn
        .query_row("SELECT count(*) FROM role_capability", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        cells,
        (ROLE_CODES.len() * cap::ALL.len()) as i64,
        "one row per (role, capability) cell and no others"
    );

    // What the column refuses, proved against a cell that is genuinely free.
    // Every one of the 128 is seeded, so an insert aimed at an occupied cell
    // would be refused by the primary key whatever `decision` said, and this
    // would pass with both CHECKs deleted. Vacating one first is what makes the
    // three refusals below evidence about `decision`.
    let (manager_id, _) = roles
        .iter()
        .find(|(_, code)| code == "manager")
        .expect("the manager role is seeded");
    db.conn
        .execute(
            "DELETE FROM role_capability WHERE role_id = ?1 AND capability = 'backup.restore'",
            params![manager_id],
        )
        .unwrap();

    // No DEFAULT, deliberately: "nobody decided" must be impossible to insert
    // rather than arriving as a silent denial. Comparing each decision with
    // `cap::DEFAULT_MATRIX` is microstep 1.6.3's deferred half; what is proved
    // here is that every cell was answered at all.
    let undecided = db.conn.execute(
        "INSERT INTO role_capability (role_id, capability)
         VALUES (?1, 'backup.restore')",
        params![manager_id],
    );
    let message = sqlite_message(undecided.expect_err("a row with no decision must be refused"));
    assert!(
        message.contains("decision"),
        "the refusal must name the missing decision, said: {message}"
    );

    let invented = db.conn.execute(
        "INSERT INTO role_capability (role_id, capability, decision)
         VALUES (?1, 'backup.restore', 'probably')",
        params![manager_id],
    );
    invented.expect_err("`decision` admits exactly granted, withheld and sets_the_limit");

    // A denial that carries a limit reads as a bounded grant to anything that
    // reaches for `limit_json` before `decision`.
    let bounded_denial = db.conn.execute(
        "INSERT INTO role_capability (role_id, capability, decision, limit_json)
         VALUES (?1, 'backup.restore', 'withheld', '{\"kind\":\"own_store\"}')",
        params![manager_id],
    );
    bounded_denial.expect_err("a withheld cell may not carry a limit");

    // And the cell the seed actually wrote goes back in, so the failure above is
    // about the CHECK rather than about anything else refusing the statement.
    db.conn
        .execute(
            "INSERT INTO role_capability (role_id, capability, decision)
             VALUES (?1, 'backup.restore', 'withheld')",
            params![manager_id],
        )
        .expect("the seeded shape of the vacated cell must still be insertable");
}

#[test]
fn audit_log_refuses_update_and_delete() {
    let db = current_database("audit-append-only.db");
    let entry = id(0x11);
    commit_envelope(&db.conn, 1, &[("audit_log", entry)]);

    db.conn
        .execute(
            "INSERT INTO audit_log
               (id, register_id, actor_id, action, entity, payload, prev_hash, hash, at)
             VALUES (?1, ?2, ?3, 'sale.void', 'sale', '{}', ?4, ?5, ?6)",
            params![
                entry.as_slice(),
                id(0xF0).as_slice(),
                id(0xA1).as_slice(),
                id(0x00).as_slice(),
                id(0x01).as_slice(),
                AT,
            ],
        )
        .unwrap();

    let update = db
        .conn
        .execute(
            "UPDATE audit_log SET action = 'sale.create' WHERE id = ?1",
            params![entry.as_slice()],
        )
        .expect_err("audit_log is append-only");
    assert_eq!(
        sqlite_message(update),
        "I-4: audit_log is append-only — no UPDATE, ever"
    );

    let delete = db
        .conn
        .execute(
            "DELETE FROM audit_log WHERE id = ?1",
            params![entry.as_slice()],
        )
        .expect_err("the deleted tail is the attack the hash chain cannot see");
    assert_eq!(
        sqlite_message(delete),
        "I-4: audit_log is append-only — no DELETE, ever"
    );

    let rows: i64 = db
        .conn
        .query_row("SELECT count(*) FROM audit_log", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 1, "neither refusal may have removed the entry");
}

#[test]
fn an_approval_handle_can_be_consumed_only_once() {
    let db = current_database("approval-once.db");
    let handle = id(0x21);
    let effect = id(0x22);
    let audit = id(0x23);
    let second_audit = id(0x24);
    let actor = id(0xA1);
    let approver = id(0xA2);

    seed_people(&db.conn, &actor, &approver);
    commit_envelope(
        &db.conn,
        2,
        &[
            ("approval_handle", handle),
            ("audit_log", audit),
            ("approval_consumption", handle),
            ("audit_log", second_audit),
        ],
    );

    // `handle_id` is the primary key of `approval_consumption`, not merely a
    // NOT NULL column beside a unique `audit_log_id`. Asserted structurally
    // because the behavioural replay below cannot tell the two apart: reusing
    // the audit row trips the unique index either way, and the property that
    // must hold is that no second consumption of this handle exists at all.
    let handle_id_is_pk: i64 = db
        .conn
        .query_row(
            "SELECT pk FROM pragma_table_info('approval_consumption') WHERE name = 'handle_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        handle_id_is_pk, 1,
        "one row per handle is the primary key's job, not a repository's"
    );

    db.conn
        .execute(
            "INSERT INTO approval_handle
               (id, capability, actor_id, approver_id, entity_id, amount_minor,
                reason, issued_at, expires_at, nonce)
             VALUES (?1, 'price.override', ?2, ?3, ?4, 1500, 'damaged box',
                     ?5, '2026-09-02T10:05:00.000Z', ?6)",
            params![
                handle.as_slice(),
                actor.as_slice(),
                approver.as_slice(),
                effect.as_slice(),
                AT,
                id(0x2F).as_slice(),
            ],
        )
        .unwrap();

    // The audit row the consumption must name. Every field the
    // `approval_consumption_matches_handle_and_audit` trigger compares is here
    // on purpose: the consumption fact proves *this* effect was authorised, not
    // that some approval once existed.
    db.conn
        .execute(
            "INSERT INTO audit_log
               (id, register_id, actor_id, approver_id, approval_handle_id, action,
                entity, entity_id, reason, payload, prev_hash, hash, at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'price.override', 'sale_line', ?6,
                     'damaged box', '{\"amount_minor\":1500}', ?7, ?8, ?9)",
            params![
                audit.as_slice(),
                id(0xF0).as_slice(),
                actor.as_slice(),
                approver.as_slice(),
                handle.as_slice(),
                effect.as_slice(),
                id(0x00).as_slice(),
                id(0x01).as_slice(),
                AT,
            ],
        )
        .unwrap();

    const CONSUME: &str = "INSERT INTO approval_consumption
           (handle_id, effect_id, audit_log_id, consumed_at)
         VALUES (?1, ?2, ?3, ?4)";

    db.conn
        .execute(
            CONSUME,
            params![handle.as_slice(), effect.as_slice(), audit.as_slice(), AT],
        )
        .expect("the first consumption commits with its effect and audit row");

    let replay = db
        .conn
        .execute(
            CONSUME,
            params![handle.as_slice(), effect.as_slice(), audit.as_slice(), AT],
        )
        .expect_err("a one-use handle spent twice is a defeated price control");
    assert_eq!(
        error_code(&replay),
        Some(ErrorCode::ConstraintViolation),
        "the replay must be refused by the schema, not by a repository"
    );

    let consumptions: i64 = db
        .conn
        .query_row("SELECT count(*) FROM approval_consumption", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(consumptions, 1);

    // Nor is there a way around it by minting a fresh audit row for the same
    // handle and consuming against that: `idx_audit_approval_once` is a unique
    // index over `approval_handle_id`, so one handle names one audit row for as
    // long as the database exists.
    let second_naming = db
        .conn
        .execute(
            "INSERT INTO audit_log
               (id, register_id, actor_id, approver_id, approval_handle_id, action,
                entity, entity_id, reason, payload, prev_hash, hash, at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'price.override', 'sale_line', ?6,
                     'damaged box', '{\"amount_minor\":1500}', ?7, ?8, ?9)",
            params![
                second_audit.as_slice(),
                id(0xF0).as_slice(),
                actor.as_slice(),
                approver.as_slice(),
                handle.as_slice(),
                effect.as_slice(),
                id(0x00).as_slice(),
                id(0x01).as_slice(),
                AT,
            ],
        )
        .expect_err("a second audit row may not name an already-consumed handle");
    assert_eq!(
        error_code(&second_naming),
        Some(ErrorCode::ConstraintViolation)
    );

    // And the handle itself is evidence: it cannot be edited or deleted to make
    // room for a second consumption.
    let deleted = db
        .conn
        .execute(
            "DELETE FROM approval_handle WHERE id = ?1",
            params![handle.as_slice()],
        )
        .expect_err("an approval handle is audit evidence");
    assert_eq!(
        sqlite_message(deleted),
        "ApprovalHandle is audit evidence and cannot be deleted"
    );
}

#[test]
fn the_user_table_is_named_app_user() {
    let db = current_database("app-user-name.db");

    assert!(
        exists(&db.conn, "table", "app_user"),
        "`user` is reserved in PostgreSQL, so both engines carry `app_user`"
    );
    assert!(
        !exists(&db.conn, "table", "user"),
        "a `user` table would mirror to a PostgreSQL keyword and force quoting \
         on every server statement that touches it"
    );

    // The name is only half of it: the other half is that `user_role` and the
    // rest point at `app_user`, so a later migration cannot quietly introduce
    // the reserved spelling beside it.
    for (table, column) in [
        ("user_role", "user_id"),
        ("user_session", "user_id"),
        ("approval_handle", "actor_id"),
        ("approval_handle", "approver_id"),
        ("auth_attempt_state", "user_id"),
    ] {
        let target: Option<String> = db
            .conn
            .prepare(&format!("PRAGMA foreign_key_list({table})"))
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(2)?, row.get::<_, String>(3)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .find(|(_, from)| from == column)
            .map(|(to, _)| to);
        assert_eq!(
            target.as_deref(),
            Some("app_user"),
            "{table}.{column} must reference app_user"
        );
    }
}
