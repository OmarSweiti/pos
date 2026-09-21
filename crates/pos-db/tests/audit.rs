#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The audit hash chain against the exact registered migration chain.
//!
//! Every fixture opens through [`pos_db::open`] and nothing here uses
//! `tests/common`'s reference-schema overlay: that overlay turns foreign keys
//! off and layers on SQL the shipped chain does not carry, and a green run
//! against it stops being evidence that a register behaves this way.
//!
//! Nothing here calls `Checkout::write_audit` either. That fixture writes an
//! `audit_log` row with a fabricated `prev_hash`/`hash` pair, which is fine for
//! the sale gates it exists for and would make every `verify_chain` in this
//! file return `Broken` at the fixture's own row.
//!
//! **What each of the three named tests would be if it were written
//! carelessly** is recorded beside it, because that is the failure this
//! repository keeps catching: a test that passes without exercising the branch
//! its name claims.

use std::path::PathBuf;
use std::sync::{Arc, Barrier};

use pos_db::DbError;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::audit::{AuditChain, AuditRepository, StoredAuditEntry};
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_domain::audit::GENESIS;
use pos_domain::{
    ApprovalBinding, ApprovalHandle, ApprovalId, AuditIntent, Capability, ChainAnchor,
    ChainVerdict, EscalationPolicy, GrantSet, RegisterId, Role, Timestamp, UserId, authorize, cap,
    verify_chain,
};
use rusqlite::{Connection, Transaction, params};
use serde_json::json;
use uuid::Uuid;

#[path = "common/registered_chain.rs"]
mod registered_chain;

use registered_chain::RegisteredChain;

const KEY: &str = "test-key";
/// One fixed instant, as text and as milliseconds. The two must agree — they
/// are the same moment, and `the_stored_hash_matches_a_pinned_golden` is what
/// noticed when they did not.
const AT: &str = "2026-08-29T09:15:00.250Z";
const AT_MS: i64 = 1_787_994_900_250;
const TTL_MS: i64 = 5 * 60 * 1_000;

/// The register every chain in this file belongs to.
const REGISTER: u8 = 0xF0;
/// A second till in the same file, for the one test that needs two chains.
const OTHER_REGISTER: u8 = 0xF1;

struct TestDb {
    // Field order is deliberate: the connection drops before the temporary
    // directory, including on Windows where an open database file cannot be
    // removed with its directory.
    conn: Connection,
    _dir: tempfile::TempDir,
    path: PathBuf,
}

fn fresh_database(name: &str) -> TestDb {
    let dir = tempfile::tempdir().expect("the fixture needs a private database directory");
    let path = dir.path().join(name);
    let conn = pos_db::open(&path, KEY).expect("the registered migration chain must open");
    let chain = RegisteredChain::seed(&conn);
    chain.add_register(&conn, id(REGISTER).as_slice(), "REG01");
    chain.add_register(&conn, id(OTHER_REGISTER).as_slice(), "REG02");
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

fn register(byte: u8) -> RegisterId {
    RegisterId::from_uuid(uuid(byte))
}

fn user(byte: u8) -> UserId {
    UserId::from_uuid(uuid(byte))
}

fn timestamp(milliseconds: i64) -> Timestamp {
    Timestamp::from_epoch_milliseconds(milliseconds).unwrap()
}

/// A plain, unapproved audit intent. `entity_id` varies with `tag` so a reader
/// that returned the same row twice would be visible.
fn intent(tag: u8) -> AuditIntent {
    AuditIntent {
        actor: user(0xA1),
        approver: None,
        approval: None,
        action: "drawer.open",
        entity: "drawer_event",
        entity_id: uuid(tag),
        reason: None,
        payload: json!({ "count": i64::from(tag) }),
        at: timestamp(AT_MS),
    }
}

/// Write a real envelope through the shipped writer, inside the caller's
/// transaction — the ordering `audit_log_has_ready_commit` requires. Copied in
/// shape from `tests/approval.rs`: `RegisteredChain::write_envelope` opens and
/// commits a transaction of its own and cannot be called from inside one.
fn write_envelope(
    conn: &Connection,
    tx: &Transaction<'_>,
    commit: &[u8; 16],
    facts: &[(&str, Uuid)],
) {
    let changes: Vec<[u8; 16]> = facts
        .iter()
        .enumerate()
        .map(|(index, (_, entity_id))| {
            let mut change_id = *entity_id.as_bytes();
            change_id[2] = 0xC0;
            change_id[3] = u8::try_from(index % 256).unwrap_or(0);
            change_id
        })
        .collect();
    let entity_ids: Vec<[u8; 16]> = facts.iter().map(|(_, id)| *id.as_bytes()).collect();
    let members: Vec<FactMember<'_>> = facts
        .iter()
        .zip(&changes)
        .zip(&entity_ids)
        .map(|(((entity, _), change_id), entity_id)| FactMember {
            change_id,
            entity,
            entity_id,
            payload: "{}",
        })
        .collect();

    OutboxRepository::new(conn)
        .write_commit(
            tx,
            &CommitEnvelope {
                commit_id: commit,
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "test",
                created_at: AT,
            },
            &members,
        )
        .expect("a fixture envelope must be complete when it is written");
}

/// Envelope and append, in one transaction, the way a command handler does it.
///
/// `commit` is derived from the row's own id so no two appends in a file can
/// collide on `sync_commit`'s primary key — `fact_commit_member` carries a
/// global `UNIQUE (entity, entity_id)` and refuses a second claim on one row.
fn append(conn: &Connection, till: RegisterId, row: Uuid, intent: &AuditIntent) -> [u8; 32] {
    let tx = conn.unchecked_transaction().unwrap();
    let hash = append_within(conn, &tx, till, row, intent);
    tx.commit().unwrap();
    hash
}

fn append_within(
    conn: &Connection,
    tx: &Transaction<'_>,
    till: RegisterId,
    row: Uuid,
    intent: &AuditIntent,
) -> [u8; 32] {
    write_envelope(conn, tx, &commit_id(row), &[("audit_log", row)]);
    AuditRepository::new(conn)
        .append(tx, till, row, intent)
        .expect("an enveloped append must be accepted")
        .hash
}

fn commit_id(row: Uuid) -> [u8; 16] {
    let mut commit = *row.as_bytes();
    commit[0] = 0xCC;
    commit
}

/// The chain as a caller reads it: rows out of SQLite, then the domain walk.
///
/// Asserts the read did not stop short, so no test in this file can mistake a
/// truncated prefix for a whole chain — which is the one way the new
/// prefix-and-stop shape could be worse than the all-or-nothing one it
/// replaced.
fn verdict(conn: &Connection, till: RegisterId, anchor: Option<ChainAnchor>) -> ChainVerdict {
    let chain = read(conn, till);
    assert_eq!(chain.stopped(), None, "the read covered the whole register");
    verify_chain(till, chain.verifier_rows(), anchor)
}

fn read(conn: &Connection, till: RegisterId) -> AuditChain {
    AuditRepository::new(conn).chain(till).unwrap()
}

fn stored(conn: &Connection, till: RegisterId) -> Vec<StoredAuditEntry> {
    read(conn, till).entries().to_vec()
}

/// `rows[n]`, but as a named refusal rather than a panic clippy refuses to
/// compile. `indexing_slicing` is denied in tests as well as in the crate, and
/// it fires at `just lint` rather than at `cargo nextest`.
fn raw(rows: &[(i64, Vec<u8>, Vec<u8>)], index: usize) -> &(i64, Vec<u8>, Vec<u8>) {
    rows.get(index)
        .unwrap_or_else(|| panic!("the chain must have a row at index {index}"))
}

/// The same, for rows that came back through the read path.
fn entry(rows: &[StoredAuditEntry], index: usize) -> &StoredAuditEntry {
    rows.get(index)
        .unwrap_or_else(|| panic!("the chain must have an entry at index {index}"))
}

/// `(seq, prev_hash, hash)` straight out of SQL, bypassing the read path.
///
/// The chain tests compare the reader against these, so a writer and a reader
/// that agreed on the same mistake could not both hide behind each other.
fn raw_rows(conn: &Connection, till: RegisterId) -> Vec<(i64, Vec<u8>, Vec<u8>)> {
    let mut statement = conn
        .prepare(
            "SELECT seq, prev_hash, hash FROM audit_log
              WHERE register_id = ?1 ORDER BY seq",
        )
        .unwrap();
    let rows = statement
        .query_map([till.as_uuid().as_bytes().as_slice()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    rows.map(Result::unwrap).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// The three tests the phase file names
// ─────────────────────────────────────────────────────────────────────────────

/// `ref/security-compliance.md:280`: *"the head is read from the database, not
/// from memory"*.
///
/// **What a careless version of this test would miss.** Append, reopen, append,
/// and verify only the rows written after the reopen: a repository that cached
/// the head in memory and fell back to `GENESIS` when its cache was empty would
/// pass, because `verify_chain` over a post-restart slice starts at `GENESIS`
/// and accepts a chain that reset. So this asserts across the boundary —
/// row 3's stored `prev_hash` is row 2's stored `hash`, read from SQL, not from
/// either append's return value.
///
/// Two more shapes of cache survive that much, and the last two blocks close
/// them: a **process-wide** cache is not emptied by dropping a connection, so a
/// second, fresh database in the same process must still start at `seq` 1 from
/// `GENESIS`; and a **per-connection** cache is not emptied by anything, so two
/// live connections appending alternately to one file must still produce one
/// chain.
#[test]
fn chain_survives_process_restart() {
    let first = fresh_database("restart.db");
    let till = register(REGISTER);
    append(&first.conn, till, uuid(0x11), &intent(0x11));
    append(&first.conn, till, uuid(0x12), &intent(0x12));
    let path = first.path.clone();
    let dir = first._dir;
    drop(first.conn);

    let conn = pos_db::open(&path, KEY).expect("the same file reopens");
    append(&conn, till, uuid(0x13), &intent(0x13));
    append(&conn, till, uuid(0x14), &intent(0x14));

    let rows = raw_rows(&conn, till);
    assert_eq!(rows.len(), 4, "four appends, four rows");
    assert_eq!(
        raw(&rows, 0).1,
        GENESIS.to_vec(),
        "the first row chains from GENESIS"
    );
    for pair in rows.windows(2) {
        assert_eq!(
            raw(pair, 1).1,
            raw(pair, 0).2,
            "every row's prev_hash is the previous row's stored hash, \
             across the reopen at seq 3"
        );
    }
    assert_eq!(
        verdict(&conn, till, None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: 4,
            unanchored_from: 0
        }
    );

    // A process-wide head cache is not emptied by a reopen, so a fresh database
    // in this same process is the only thing that catches one.
    let fresh = fresh_database("restart-second.db");
    append(&fresh.conn, till, uuid(0x21), &intent(0x21));
    let fresh_rows = raw_rows(&fresh.conn, till);
    assert_eq!(fresh_rows.len(), 1);
    assert_eq!(raw(&fresh_rows, 0).0, 1, "a new database starts at seq 1");
    assert_eq!(
        raw(&fresh_rows, 0).1,
        GENESIS.to_vec(),
        "a new database starts from GENESIS, whatever another database's head was"
    );

    // Two live connections to one file: a per-connection cache forks here.
    let second = pos_db::open(&path, KEY).expect("a second connection opens the same file");
    append(&conn, till, uuid(0x15), &intent(0x15));
    append(&second, till, uuid(0x16), &intent(0x16));
    append(&conn, till, uuid(0x17), &intent(0x17));
    assert_eq!(
        verdict(&second, till, None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: 7,
            unanchored_from: 0
        },
        "two connections append to one chain, not two"
    );
    drop(dir);
}

/// Two appenders contend for the write lock, and the chain does not fork.
///
/// **What a careless version of this test would miss.** Asserting that both
/// appends returned `Ok`, or that the row count is two, proves nothing: the
/// busy handler is five seconds, so a loser waits rather than failing, and two
/// writers that both read the same head still both return `Ok` while writing a
/// forked chain. Nothing in SQL refuses a fork — there is no `CHECK`, no unique
/// index and no trigger on `prev_hash`. So this asserts the property: no two
/// rows of the register share a `prev_hash`, and the walk comes back intact.
///
/// **And the overlap is structural rather than hoped for** (conventions §5:
/// barriers, never sleeps). The holder writes its envelope *inside* its
/// transaction before reaching the barrier, so it demonstrably holds SQLite's
/// single write lock at the moment both threads are released; the contender's
/// first statement then has to wait for the holder's commit. The mutation that
/// must turn this red is the head read moving out of the append's own
/// transaction.
#[test]
fn concurrent_appends_serialize() {
    let db = fresh_database("concurrent.db");
    let till = register(REGISTER);
    let path = db.path.clone();

    let gate = Arc::new(Barrier::new(2));
    let holder_gate = Arc::clone(&gate);
    let holder_path = path.clone();

    let holder = std::thread::spawn(move || {
        let conn = pos_db::open(&holder_path, KEY).unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        // The envelope takes the write lock before anybody is released.
        write_envelope(
            &conn,
            &tx,
            &commit_id(uuid(0x31)),
            &[("audit_log", uuid(0x31))],
        );
        holder_gate.wait();
        let appended = AuditRepository::new(&conn)
            .append(&tx, till, uuid(0x31), &intent(0x31))
            .unwrap();
        tx.commit().unwrap();
        appended.hash
    });

    let contender = std::thread::spawn(move || {
        let conn = pos_db::open(&path, KEY).unwrap();
        gate.wait();
        // Blocks here until the holder commits: the write lock is already taken.
        let tx = conn.unchecked_transaction().unwrap();
        write_envelope(
            &conn,
            &tx,
            &commit_id(uuid(0x32)),
            &[("audit_log", uuid(0x32))],
        );
        let appended = AuditRepository::new(&conn)
            .append(&tx, till, uuid(0x32), &intent(0x32))
            .unwrap();
        tx.commit().unwrap();
        appended.prev_hash
    });

    let held_hash = holder.join().expect("the holder thread must not panic");
    let contender_prev = contender
        .join()
        .expect("the contender thread must not panic");
    assert_eq!(
        contender_prev, held_hash,
        "the contender read a head that already included the holder's row"
    );

    let forks: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM (
                 SELECT prev_hash FROM audit_log WHERE register_id = ?1
                  GROUP BY prev_hash HAVING count(*) > 1)",
            [till.as_uuid().as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        forks, 0,
        "no two rows of one register claim the same parent"
    );
    assert_eq!(
        verdict(&db.conn, till, None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: 2,
            unanchored_from: 0
        }
    );
}

/// A thousand entries, read back out of SQLite, and a break the walk locates.
///
/// **What a careless version of this test would miss.** Handing `verify_chain`
/// the tuples the writer kept in memory verifies the writer's arithmetic
/// against itself and passes even if nothing reached disk — so every entry here
/// comes back through `AuditRepository::chain` on a **reopened** connection.
/// And `!matches!(verdict, Broken { .. })` is satisfied by an empty iterator,
/// so the assertion is the exact verdict, entry count included.
///
/// Two shapes are deliberate. The thousand rows are written in **four
/// transactions of 250**, so the head read has to see rows its own transaction
/// has written and not yet committed; a head read taken outside the append's
/// transaction passes a one-row-per-transaction version of this test and fails
/// here. And every field varies — `reason`, `approver` and the payload's shape
/// — so a reader that dropped a column would still be verifying a chain whose
/// hashes were taken over the column it dropped.
#[test]
fn verify_chain_over_1000_entries() {
    const ENTRIES: u16 = 1_000;
    const PER_TRANSACTION: u16 = 250;

    let db = fresh_database("thousand.db");
    let till = register(REGISTER);
    let path = db.path.clone();
    let dir = db._dir;

    let mut written = 0u16;
    while written < ENTRIES {
        let tx = db.conn.unchecked_transaction().unwrap();
        for _ in 0..PER_TRANSACTION {
            let row = row_id(written);
            write_envelope(&db.conn, &tx, &commit_id(row), &[("audit_log", row)]);
            AuditRepository::new(&db.conn)
                .append(&tx, till, row, &varied_intent(written))
                .expect("every append in the batch is enveloped");
            written = written.saturating_add(1);
        }
        tx.commit().unwrap();
    }
    drop(db.conn);

    let conn = pos_db::open(&path, KEY).expect("the file reopens");
    let chain = read(&conn, till);
    assert_eq!(chain.stopped(), None, "nothing stopped the read");
    let rows = chain.entries();
    assert_eq!(rows.len(), usize::from(ENTRIES));
    for (index, row) in rows.iter().enumerate() {
        let expected = varied_intent(u16::try_from(index).unwrap());
        assert_eq!(
            row.intent, expected,
            "row {index} rebuilt every hashed field"
        );
    }

    let head = rows.last().expect("a thousand rows have a last one");
    let anchor = ChainAnchor {
        register_id: till,
        seq: head.seq,
        hash: head.hash,
    };
    assert_eq!(
        verify_chain(till, chain.verifier_rows(), Some(anchor)),
        ChainVerdict::Intact {
            entries: u64::from(ENTRIES)
        },
        "anchored at its own head, the whole chain is intact"
    );
    assert_eq!(
        verify_chain(till, chain.verifier_rows(), None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: u64::from(ENTRIES),
            unanchored_from: 0
        },
        "with no anchor the same chain is intact but wholly unverifiable"
    );

    // The break. `audit_log_no_update` correctly refuses the edit, so the
    // tamper happens on a copy with the trigger suspended — which
    // `ref/security-compliance.md` names as the only sanctioned way to produce
    // a tampered state, and is what an investigator would do.
    let tampered_path = dir.path().join("thousand-tampered.db");
    std::fs::copy(&path, &tampered_path).expect("the database file copies");
    let tampered = pos_db::open(&tampered_path, KEY).expect("the copy opens");
    tampered
        .execute_batch(
            "DROP TRIGGER audit_log_no_update;
             UPDATE audit_log SET payload = '{\"count\":999999}' WHERE seq = 500;",
        )
        .expect("the copy's trigger is suspended and one row is edited");

    let tampered_chain = read(&tampered, till);
    assert_eq!(
        tampered_chain.stopped(),
        None,
        "an edited payload is still readable"
    );
    assert_eq!(tampered_chain.entries().len(), usize::from(ENTRIES));
    assert_eq!(
        verify_chain(till, tampered_chain.verifier_rows(), None),
        ChainVerdict::Broken { at_seq: 500 },
        "the walk names the edited row, and names it first"
    );
    assert_eq!(
        verify_chain(till, chain.verifier_rows(), None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: u64::from(ENTRIES),
            unanchored_from: 0
        },
        "and the original, untouched, still verifies — the control for the line above"
    );
    drop(dir);
}

/// A row id that is distinct per index and never collides with a fixture id.
fn row_id(index: u16) -> Uuid {
    let mut bytes = [0x7Au8; 16];
    bytes[14] = u8::try_from(index >> 8).unwrap_or(0);
    bytes[15] = u8::try_from(index & 0xFF).unwrap_or(0);
    Uuid::from_bytes(bytes)
}

/// One intent per index, varying every field a reader could drop.
fn varied_intent(index: u16) -> AuditIntent {
    let odd = index % 2 == 1;
    AuditIntent {
        actor: user(0xA1),
        approver: if odd { Some(user(0xA2)) } else { None },
        approval: None,
        action: if odd { "sale.void" } else { "drawer.open" },
        entity: if odd { "sale" } else { "drawer_event" },
        entity_id: row_id(index),
        reason: if odd {
            Some(format!("طلب العميل {index}"))
        } else {
            None
        },
        payload: if odd {
            json!({ "lines": 3, "amount_minor": -i64::from(index) })
        } else {
            json!({ "count": index })
        },
        at: timestamp(AT_MS.saturating_add(i64::from(index))),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// What the three above do not prove
// ─────────────────────────────────────────────────────────────────────────────

/// The stored digest is the digest of the bytes the specification names.
///
/// Every other test in this file routes its judgement through `verify_chain`,
/// which rebuilds an entry from the same read path that is under test — so a
/// writer and a reader that agreed on the same mistake verify clean. This is
/// the one assertion that does not: the canonical bytes are written out by hand
/// below, reviewable against `ref/security-compliance.md` §4 and against
/// `pos-domain`'s own `GOLDEN_CANONICAL_BYTES`, and hashed here rather than by
/// the encoder under test.
///
/// Changing `EXPECTED_BYTES` is a protocol change, not a test fix: every hash
/// any register ever wrote was taken over bytes of this shape.
#[test]
fn the_stored_hash_matches_a_pinned_golden() {
    // Keys sorted at both levels, no whitespace, the Arabic reason raw UTF-8
    // rather than `\u`-escaped, and `-2900` an integer — although the fixture
    // below writes the payload's two keys the other way round.
    const EXPECTED_BYTES: &str = concat!(
        r#"{"domain":"pos.audit","#,
        r#""id":"41414141-4141-4141-4141-414141414141","#,
        r#""intent":{"#,
        r#""action":"sale.void","#,
        r#""actor":"a1a1a1a1-a1a1-a1a1-a1a1-a1a1a1a1a1a1","#,
        r#""approval":null,"#,
        r#""approver":"a2a2a2a2-a2a2-a2a2-a2a2-a2a2a2a2a2a2","#,
        r#""at":"2026-08-29T09:15:00.250Z","#,
        r#""entity":"sale","#,
        r#""entity_id":"42424242-4242-4242-4242-424242424242","#,
        r#""payload":{"amount_minor":-2900,"lines":3},"#,
        r#""reason":"طلب العميل""#,
        r#"},"#,
        r#""register_id":"f0f0f0f0-f0f0-f0f0-f0f0-f0f0f0f0f0f0","#,
        r#""seq":1,"#,
        r#""version":1}"#,
    );

    let db = fresh_database("golden.db");
    let till = register(REGISTER);
    let golden = AuditIntent {
        actor: user(0xA1),
        approver: Some(user(0xA2)),
        approval: None,
        action: "sale.void",
        entity: "sale",
        entity_id: uuid(0x42),
        reason: Some("طلب العميل".to_owned()),
        payload: json!({ "lines": 3, "amount_minor": -2_900 }),
        at: timestamp(AT_MS),
    };
    append(&db.conn, till, uuid(0x41), &golden);

    let mut hasher = blake3::Hasher::new();
    hasher.update(&GENESIS);
    hasher.update(EXPECTED_BYTES.as_bytes());
    let expected = *hasher.finalize().as_bytes();

    let rows = raw_rows(&db.conn, till);
    assert_eq!(rows.len(), 1);
    assert_eq!(raw(&rows, 0).0, 1, "the first row of a chain is seq 1");
    assert_eq!(
        raw(&rows, 0).2,
        expected.to_vec(),
        "audit_log.hash is BLAKE3(GENESIS ‖ the canonical bytes written above)"
    );
}

/// `audit_log.payload`'s DDL calls the column canonical JSON. This is what
/// makes that true rather than aspirational.
///
/// `serde_json::Map` is a `BTreeMap` only while the `preserve_order` feature is
/// off, and it is off across this workspace today — a fact no gate watches. The
/// day a crate three levels away turns it on, `to_string` starts emitting
/// insertion order and this goes red, which is the whole point. Verification
/// would survive it (the encoder sorts the parsed value itself), so nothing
/// else in this file would notice.
#[test]
fn the_stored_payload_is_canonical_json() {
    let db = fresh_database("canonical-payload.db");
    let till = register(REGISTER);
    let mut out_of_order = intent(0x51);
    out_of_order.payload = json!({
        "zebra": { "second": 2, "first": 1 },
        "alpha": [3, 1, 2],
    });
    append(&db.conn, till, uuid(0x51), &out_of_order);

    let text: String = db
        .conn
        .query_row("SELECT payload FROM audit_log WHERE seq = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        text, r#"{"alpha":[3,1,2],"zebra":{"first":1,"second":2}}"#,
        "keys sorted at every level, array order preserved, no whitespace"
    );
}

/// The one refusal a correct caller still meets, by its own name.
///
/// Asserting `is_err()` would prove nothing — several unrelated constraints
/// would satisfy it — so this asserts the mapped variant, and then proves the
/// envelope is what was missing by writing one and appending the same row.
#[test]
fn an_append_without_its_delivery_envelope_is_refused() {
    let db = fresh_database("no-envelope.db");
    let till = register(REGISTER);

    let tx = db.conn.unchecked_transaction().unwrap();
    let refusal = AuditRepository::new(&db.conn)
        .append(&tx, till, uuid(0x61), &intent(0x61))
        .expect_err("an audit fact with no delivery envelope is refused");
    assert!(
        matches!(refusal, DbError::AuditEnvelopeMissing),
        "the trigger's message is mapped, not passed through as a SQLite error: {refusal:?}"
    );
    drop(tx);

    // The positive control: the same row, the same intent, with an envelope.
    append(&db.conn, till, uuid(0x61), &intent(0x61));
    assert_eq!(raw_rows(&db.conn, till).len(), 1);
}

/// A payload the canonical form cannot carry honestly never becomes a row.
///
/// `audit_log` has no `UPDATE`, so a payload that is wrong is wrong forever.
/// `canonical_bytes` is total by design and would hash this one happily; the
/// gate is `check_payload`, and this is the test that proves the repository
/// calls it.
#[test]
fn a_payload_that_is_not_canonically_encodable_is_refused() {
    let db = fresh_database("bad-payload.db");
    let till = register(REGISTER);
    let mut floating = intent(0x71);
    floating.payload = json!({ "amount_minor": 12.5 });

    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(
        &db.conn,
        &tx,
        &commit_id(uuid(0x71)),
        &[("audit_log", uuid(0x71))],
    );
    let refusal = AuditRepository::new(&db.conn)
        .append(&tx, till, uuid(0x71), &floating)
        .expect_err("a non-integer number is refused before it is chained");
    assert!(
        matches!(refusal, DbError::AuditPayloadRefused { .. }),
        "the domain's refusal is mapped to a named variant: {refusal:?}"
    );
    tx.commit().unwrap();
    assert!(
        raw_rows(&db.conn, till).is_empty(),
        "and it left no row behind, even though its envelope committed"
    );
}

/// One file, two tills, two chains — and neither is the other's parent.
///
/// This is the test that fails if the head read stops filtering by
/// `register_id`: interleaved appends would chain across the two registers, and
/// every single-register walk would report `Broken` at its second row.
#[test]
fn a_second_registers_rows_do_not_break_either_chain() {
    let db = fresh_database("two-registers.db");
    let first = register(REGISTER);
    let second = register(OTHER_REGISTER);

    append(&db.conn, first, uuid(0x81), &intent(0x81));
    append(&db.conn, second, uuid(0x82), &intent(0x82));
    append(&db.conn, first, uuid(0x83), &intent(0x83));
    append(&db.conn, second, uuid(0x84), &intent(0x84));

    let first_rows = raw_rows(&db.conn, first);
    let second_rows = raw_rows(&db.conn, second);
    assert_eq!(raw(&first_rows, 0).1, GENESIS.to_vec());
    assert_eq!(raw(&second_rows, 0).1, GENESIS.to_vec());
    assert_eq!(
        raw(&first_rows, 1).1,
        raw(&first_rows, 0).2,
        "the second row of a till chains from that till's first row"
    );
    assert_eq!(raw(&second_rows, 1).1, raw(&second_rows, 0).2);
    assert_eq!(
        first_rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        vec![1, 3],
        "seq is table-global, so one till's numbers have gaps — and the chain does not care"
    );

    for till in [first, second] {
        assert_eq!(
            verdict(&db.conn, till, None),
            ChainVerdict::IntactUnanchoredFrom {
                entries: 2,
                unanchored_from: 0
            }
        );
    }
}

/// Every column the hash covers survives the round trip, including the three a
/// plain fixture never sets.
///
/// `approver_id`, `approval_handle_id` and `reason` are nullable, and the
/// approval one carries a real foreign key — so the handle is issued and
/// persisted through the shipped writer rather than invented, and both rows
/// ride in one delivery envelope the way a privileged command's would.
#[test]
fn the_chain_read_rebuilds_every_hashed_column() {
    let db = fresh_database("round-trip.db");
    let till = register(REGISTER);
    let actor = user(0xA1);
    let approver = user(0xA2);
    let effect = uuid(0x92);
    let handle_id = ApprovalId::from_uuid(uuid(0x93));

    seed_people(&db.conn, actor, approver);
    let handle = issue_handle(handle_id, actor, approver, effect);
    let row = uuid(0x91);
    let bound = AuditIntent {
        actor,
        approver: Some(approver),
        approval: Some(handle_id),
        action: cap::SaleVoid::NAME,
        entity: "sale",
        entity_id: effect,
        reason: Some("طلب العميل".to_owned()),
        payload: json!({ "amount_minor": 1_500, "lines": 2 }),
        at: timestamp(AT_MS),
    };

    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(
        &db.conn,
        &tx,
        &commit_id(row),
        &[("approval_handle", handle_id.as_uuid()), ("audit_log", row)],
    );
    ApprovalRepository::new(&db.conn)
        .insert(&tx, &handle)
        .unwrap();
    AuditRepository::new(&db.conn)
        .append(&tx, till, row, &bound)
        .unwrap();
    tx.commit().unwrap();

    let rows = stored(&db.conn, till);
    assert_eq!(rows.len(), 1);
    assert_eq!(entry(&rows, 0).id, row);
    assert_eq!(entry(&rows, 0).seq, 1);
    assert_eq!(
        entry(&rows, 0).intent,
        bound,
        "actor, approver, approval, action, entity, entity_id, reason, payload and at all return"
    );
    assert_eq!(
        verdict(&db.conn, till, None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: 1,
            unanchored_from: 0
        }
    );

    // `Debug` on a public type that carries a payload does not print it. Built
    // from the value rather than searched for as a literal: a derived `Debug`
    // escapes the quotes canonical JSON is full of, so looking for the raw
    // string would never match and the test would be vacuous either way.
    let rendered = format!("{:?}", entry(&rows, 0));
    assert!(
        !rendered.contains("amount_minor"),
        "no payload key reaches Debug: {rendered}"
    );
    assert!(
        rendered.contains("<redacted,"),
        "and the redaction says so rather than dropping the field: {rendered}"
    );
    assert!(
        rendered.contains("seq: 1"),
        "while the rest of the struct is still legible: {rendered}"
    );
}

/// A number is spent only by a transaction that commits.
#[test]
fn a_rolled_back_append_leaves_no_row() {
    let db = fresh_database("rollback.db");
    let till = register(REGISTER);
    append(&db.conn, till, uuid(0xB1), &intent(0xB1));

    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(
        &db.conn,
        &tx,
        &commit_id(uuid(0xB2)),
        &[("audit_log", uuid(0xB2))],
    );
    AuditRepository::new(&db.conn)
        .append(&tx, till, uuid(0xB2), &intent(0xB2))
        .unwrap();
    tx.rollback().unwrap();

    assert_eq!(
        raw_rows(&db.conn, till).len(),
        1,
        "the rolled-back row is gone"
    );

    // And the number it held is available again, because `MAX(seq)` reads the
    // rows and not `sqlite_sequence`.
    append(&db.conn, till, uuid(0xB3), &intent(0xB3));
    let rows = raw_rows(&db.conn, till);
    assert_eq!(rows.iter().map(|row| row.0).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(
        raw(&rows, 1).1,
        raw(&rows, 0).2,
        "and the chain links across the rollback"
    );
}

/// I-4, from the other side: the table refuses what this module has no method
/// for.
///
/// Asserted by message rather than by `is_err()`. #179 found three I-4
/// assertions in this crate that stayed green with their trigger removed,
/// because an unrelated constraint satisfied them.
#[test]
fn audit_log_still_refuses_update_and_delete() {
    let db = fresh_database("append-only.db");
    let till = register(REGISTER);
    append(&db.conn, till, uuid(0xC1), &intent(0xC1));

    let update = db
        .conn
        .execute(
            "UPDATE audit_log SET action = 'sale.void' WHERE seq = 1",
            [],
        )
        .expect_err("audit_log refuses UPDATE");
    assert!(
        update
            .to_string()
            .contains("I-4: audit_log is append-only — no UPDATE, ever"),
        "the trigger's own words: {update}"
    );

    let delete = db
        .conn
        .execute("DELETE FROM audit_log WHERE seq = 1", [])
        .expect_err("audit_log refuses DELETE");
    assert!(
        delete
            .to_string()
            .contains("I-4: audit_log is append-only — no DELETE, ever"),
        "the trigger's own words: {delete}"
    );

    assert_eq!(raw_rows(&db.conn, till).len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────

fn seed_people(conn: &Connection, actor: UserId, approver: UserId) {
    let org = id(0xA0);
    conn.execute(
        "INSERT INTO org (id, legal_name) VALUES (?1, 'Audit Test Org')",
        params![org.as_slice()],
    )
    .unwrap();
    for (who, code) in [(actor, "C-1"), (approver, "M-1")] {
        conn.execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             VALUES (?1, ?2, ?3, ?3, 'placeholder-not-a-hash', ?4)",
            params![
                who.as_uuid().as_bytes().as_slice(),
                org.as_slice(),
                code,
                AT
            ],
        )
        .unwrap();
    }
}

fn issue_handle(
    handle: ApprovalId,
    actor: UserId,
    approver: UserId,
    effect: Uuid,
) -> ApprovalHandle {
    let binding = ApprovalBinding {
        entity_id: effect,
        amount_minor: 1_500,
        content_hash: None,
    };
    let authorized = authorize::<cap::SaleVoid>(
        approver,
        &GrantSet::of_role(Role::Manager),
        None,
        &binding,
        &EscalationPolicy::empty(),
        timestamp(AT_MS),
    )
    .expect("the manager fixture holds sale.void");

    ApprovalHandle::issue(
        handle,
        actor,
        &authorized,
        &binding,
        "طلب العميل".to_owned(),
        timestamp(AT_MS),
        TTL_MS,
        // Derived from the handle rather than written as a literal, which is
        // `tests/approval.rs`'s idiom for the same argument. A fixture nonce
        // must be deterministic — conventions §5 gives a test no randomness —
        // but deriving it keeps two handles in one file from sharing one, and
        // keeps a fixed 16-byte array out of a cryptographic parameter, which
        // CodeQL reads as a hard-coded nonce and is right to.
        id(handle.as_uuid().as_bytes()[0].wrapping_add(1)),
    )
    .expect("the deterministic fixture is a valid distinct-user approval")
}

/// `sqlite_sequence` is rewritable and the chain does not read it.
///
/// It carries no trigger guard of any kind, so a SQL console can set it to
/// anything; `MAX(seq)` reads the hashed, delete-guarded column instead. This
/// is the test that distinguishes the two, because in every ordinary sequence
/// of events they return the same number — including after a rollback, since
/// `sqlite_sequence` rolls back with its transaction.
#[test]
fn a_rewritten_sqlite_sequence_does_not_move_the_chain() {
    let db = fresh_database("rewritten-sequence.db");
    let till = register(REGISTER);
    append(&db.conn, till, uuid(0xD1), &intent(0xD1));

    let rewritten = db
        .conn
        .execute(
            "UPDATE sqlite_sequence SET seq = 9999 WHERE name = 'audit_log'",
            [],
        )
        .expect("sqlite_sequence has no guard, which is the point");
    assert_eq!(rewritten, 1, "the bookkeeping really was rewritten");

    append(&db.conn, till, uuid(0xD2), &intent(0xD2));
    let rows = raw_rows(&db.conn, till);
    assert_eq!(
        rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        vec![1, 2],
        "the next number came from MAX(seq), not from the rewritten counter"
    );
    assert_eq!(
        verdict(&db.conn, till, None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: 2,
            unanchored_from: 0
        },
        "and the seq the hash was taken over is the seq that was stored"
    );
}

/// A row this build cannot rebuild stops the walk and takes nothing with it.
///
/// This is the test for the defect the first version of this module shipped:
/// `chain()` returned `Err` on one unreadable row and threw every honest row
/// below it away, so a single inserted row silenced the whole register's
/// verification — permanently, because `audit_log_no_delete` then refuses to
/// let anyone remove the poison row.
///
/// The poison is a `canonical_version` this build does not hash, written with
/// a real delivery envelope so nothing but that column distinguishes it from
/// an honest row. Three honest rows go in first, and the assertion is that all
/// three come back, verify, and are accompanied by a stop that names the
/// fourth.
#[test]
fn a_row_this_build_cannot_rebuild_stops_the_walk_without_hiding_the_rows_below_it() {
    let db = fresh_database("poison-row.db");
    let till = register(REGISTER);
    for tag in [0xE1u8, 0xE2, 0xE3] {
        append(&db.conn, till, uuid(tag), &intent(tag));
    }

    let poison = uuid(0xE4);
    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(&db.conn, &tx, &commit_id(poison), &[("audit_log", poison)]);
    db.conn
        .execute(
            "INSERT INTO audit_log
               (seq, id, canonical_version, register_id, actor_id, action,
                entity, entity_id, payload, prev_hash, hash, at)
             VALUES (4, ?1, 2, ?2, ?3, 'drawer.open', 'drawer_event', ?4,
                     '{}', ?5, ?6, ?7)",
            params![
                poison.as_bytes().as_slice(),
                till.as_uuid().as_bytes().as_slice(),
                user(0xA1).as_uuid().as_bytes().as_slice(),
                uuid(0xE5).as_bytes().as_slice(),
                vec![0x66u8; 32],
                vec![0x77u8; 32],
                AT,
            ],
        )
        .expect("a future-version row is a row the schema accepts");
    tx.commit().unwrap();

    // One more honest row ABOVE the poison, so that stopping and skipping are
    // distinguishable. With the poison last, a read that quietly skipped the
    // bad row and carried on would return the same three entries and the same
    // stop as one that halted — and skipping is wrong: the rows above a break
    // chain from a row the caller never received, so `verify_chain` would
    // report `Broken` at an honest one.
    append(&db.conn, till, uuid(0xE9), &intent(0xE9));

    let chain = read(&db.conn, till);
    assert_eq!(
        chain.entries().len(),
        3,
        "the three honest rows below the poison are readable, and the walk \
         stopped rather than skipping to the fifth"
    );
    let stopped = chain
        .stopped()
        .expect("the read stopped, and says so rather than returning an error");
    assert_eq!(stopped.seq, 4, "and it names the row it stopped at");
    assert!(
        stopped.reason.contains("canonical version 2"),
        "and why: {}",
        stopped.reason
    );
    assert_eq!(
        verify_chain(till, chain.verifier_rows(), None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: 3,
            unanchored_from: 0
        },
        "the prefix still produces a verdict — the poison row cost three rows nothing"
    );
}

/// A negative `seq` is a stop, not a panic and not a lost register.
///
/// `audit_log.seq` is `INTEGER PRIMARY KEY` with no `CHECK`, so SQLite accepts
/// one. It is the branch behind `u64::try_from` in the read path, which no
/// other test reaches.
#[test]
fn a_negative_seq_stops_the_walk_at_that_row() {
    let db = fresh_database("negative-seq.db");
    let till = register(REGISTER);
    let row = uuid(0xE6);

    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(&db.conn, &tx, &commit_id(row), &[("audit_log", row)]);
    db.conn
        .execute(
            "INSERT INTO audit_log
               (seq, id, canonical_version, register_id, actor_id, action,
                entity, entity_id, payload, prev_hash, hash, at)
             VALUES (-5, ?1, 1, ?2, ?3, 'drawer.open', 'drawer_event', ?4,
                     '{}', ?5, ?6, ?7)",
            params![
                row.as_bytes().as_slice(),
                till.as_uuid().as_bytes().as_slice(),
                user(0xA1).as_uuid().as_bytes().as_slice(),
                uuid(0xE7).as_bytes().as_slice(),
                GENESIS.as_slice(),
                vec![0x77u8; 32],
                AT,
            ],
        )
        .expect("INTEGER PRIMARY KEY has no CHECK, so a negative rowid is accepted");
    tx.commit().unwrap();

    let chain = read(&db.conn, till);
    assert!(chain.entries().is_empty());
    let stopped = chain.stopped().expect("a negative seq stops the walk");
    assert_eq!(stopped.seq, -5);
    assert!(stopped.reason.contains("negative"), "{}", stopped.reason);

    // And `append` refuses rather than chaining onto it, because MAX(seq) over
    // the table is now negative and there is no positive number below it.
    let next = uuid(0xE8);
    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(&db.conn, &tx, &commit_id(next), &[("audit_log", next)]);
    let refusal = AuditRepository::new(&db.conn)
        .append(&tx, till, next, &intent(0xE8))
        .expect_err("a table whose highest seq is negative is no place to append");
    assert!(
        matches!(refusal, DbError::AuditSeqInvalid { found: -5 }),
        "{refusal:?}"
    );
}

/// An `action` no build of this application knows is read back, not refused.
///
/// This is the denial-of-verification hole the read path is deliberately
/// built without: if an unrecognised spelling failed, one inserted row would
/// silence the whole verifier, with no forensic signal and no located break.
/// The row here carries an action that is in no capability list and an entity
/// that is in no schema, and the chain still walks.
///
/// It also pins the interner's one claim: a second read of the same spelling
/// does not leak a second copy of it.
#[test]
fn an_unknown_action_is_read_back_rather_than_refused() {
    let db = fresh_database("unknown-action.db");
    let till = register(REGISTER);
    let mut exotic = intent(0xF5);
    exotic.action = "something.no.build.has.ever.written";
    exotic.entity = "a_table_that_does_not_exist";
    append(&db.conn, till, uuid(0xF5), &exotic);
    append(&db.conn, till, uuid(0xF6), &intent(0xF6));

    let rows = stored(&db.conn, till);
    assert_eq!(
        entry(&rows, 0).intent.action,
        "something.no.build.has.ever.written"
    );
    assert_eq!(entry(&rows, 0).intent.entity, "a_table_that_does_not_exist");
    assert_eq!(
        verdict(&db.conn, till, None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: 2,
            unanchored_from: 0
        },
        "an unrecognised spelling costs the chain nothing"
    );

    let again = stored(&db.conn, till);
    assert!(
        std::ptr::eq(
            entry(&rows, 0).intent.action,
            entry(&again, 0).intent.action
        ),
        "the second read returns the same interned string rather than leaking another"
    );
}

/// The head is read through the transaction it is about to write in.
///
/// In the ordinary call shape — `AuditRepository::new(&conn)` with a
/// transaction taken from that same `conn` — this is invisible, because
/// `Transaction` dereferences to its `Connection` and the two are one SQLite
/// handle. The first mutation sweep of this module confirmed that: moving the
/// head read from `tx` to `self.conn` killed no test at all.
///
/// So the misuse is constructed here deliberately. The repository is built
/// over one connection and handed a transaction belonging to another, and two
/// rows are appended inside it. A head read on the wrong handle cannot see the
/// first row — WAL gives it the last *committed* snapshot — so it would hand
/// the second append the same `seq` and the same parent, and the chain would
/// fork if `audit_log.seq` did not refuse it first.
#[test]
fn an_append_reads_the_head_through_its_own_transaction() {
    let db = fresh_database("own-transaction.db");
    let till = register(REGISTER);
    let other = pos_db::open(&db.path, KEY).expect("a second connection opens the same file");

    // Built over `other`; writing through a transaction that belongs to `conn`.
    let repository = AuditRepository::new(&other);
    let tx = db.conn.unchecked_transaction().unwrap();
    for row in [uuid(0xA5), uuid(0xA6)] {
        write_envelope(&db.conn, &tx, &commit_id(row), &[("audit_log", row)]);
        repository
            .append(&tx, till, row, &intent(row.as_bytes()[0]))
            .expect("both appends see the transaction's own rows");
    }
    tx.commit().unwrap();

    let rows = raw_rows(&db.conn, till);
    assert_eq!(rows.iter().map(|row| row.0).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(
        raw(&rows, 1).1,
        raw(&rows, 0).2,
        "the second row chains from the first"
    );
    assert_eq!(
        verdict(&db.conn, till, None),
        ChainVerdict::IntactUnanchoredFrom {
            entries: 2,
            unanchored_from: 0
        }
    );
}

/// A NULL `entity_id` stops the walk rather than being invented around.
///
/// The column is nullable and `AuditIntent.entity_id` is not, so there is no
/// honest `Uuid` to substitute — and substituting one would change the bytes
/// the row is hashed over, turning an unreadable row into a `Broken` verdict
/// that names tampering nobody did.
#[test]
fn a_null_entity_id_stops_the_walk_at_that_row() {
    let db = fresh_database("null-entity.db");
    let till = register(REGISTER);
    append(&db.conn, till, uuid(0xD5), &intent(0xD5));

    let row = uuid(0xD6);
    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(&db.conn, &tx, &commit_id(row), &[("audit_log", row)]);
    db.conn
        .execute(
            "INSERT INTO audit_log
               (seq, id, canonical_version, register_id, actor_id, action,
                entity, entity_id, payload, prev_hash, hash, at)
             VALUES (2, ?1, 1, ?2, ?3, 'drawer.open', 'drawer_event', NULL,
                     '{}', ?4, ?5, ?6)",
            params![
                row.as_bytes().as_slice(),
                till.as_uuid().as_bytes().as_slice(),
                user(0xA1).as_uuid().as_bytes().as_slice(),
                vec![0x11u8; 32],
                vec![0x22u8; 32],
                AT,
            ],
        )
        .expect("audit_log.entity_id is nullable");
    tx.commit().unwrap();

    let chain = read(&db.conn, till);
    assert_eq!(chain.entries().len(), 1, "the honest row below it survives");
    let stopped = chain.stopped().expect("a NULL entity_id stops the walk");
    assert_eq!(stopped.seq, 2);
    assert!(
        stopped.reason.contains("entity_id is NULL"),
        "{}",
        stopped.reason
    );
}

/// An unreadable head refuses the append rather than chaining onto it.
///
/// A digest that is not 32 bytes cannot be a parent. Padding or truncating it
/// would produce a link that verifies against nothing, written into a table
/// with no `UPDATE` — so the append refuses, and the refusal names the head
/// row rather than the table's highest `seq`, which may belong to another till.
#[test]
fn an_unreadable_head_refuses_the_append_rather_than_chaining_onto_it() {
    let db = fresh_database("short-head.db");
    let till = register(REGISTER);
    let other = register(OTHER_REGISTER);
    // A taller chain on the other till, so a refusal that reported the
    // table-global MAX(seq) would name one of these rows instead.
    for tag in [0xC5u8, 0xC6, 0xC7] {
        append(&db.conn, other, uuid(tag), &intent(tag));
    }

    let head = uuid(0xC8);
    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(&db.conn, &tx, &commit_id(head), &[("audit_log", head)]);
    db.conn
        .execute(
            "INSERT INTO audit_log
               (seq, id, canonical_version, register_id, actor_id, action,
                entity, entity_id, payload, prev_hash, hash, at)
             VALUES (4, ?1, 1, ?2, ?3, 'drawer.open', 'drawer_event', ?4,
                     '{}', ?5, X'0102030405', ?6)",
            params![
                head.as_bytes().as_slice(),
                till.as_uuid().as_bytes().as_slice(),
                user(0xA1).as_uuid().as_bytes().as_slice(),
                uuid(0xC9).as_bytes().as_slice(),
                GENESIS.as_slice(),
                AT,
            ],
        )
        .expect("nothing in the schema constrains the width of hash");
    tx.commit().unwrap();

    // And one more on the other till, so this register's head (4) is NOT the
    // table's highest seq (5) — otherwise the locator assertion below would
    // hold for the wrong reason.
    append(&db.conn, other, uuid(0xCB), &intent(0xCB));

    let next = uuid(0xCA);
    let tx = db.conn.unchecked_transaction().unwrap();
    write_envelope(&db.conn, &tx, &commit_id(next), &[("audit_log", next)]);
    let refusal = AuditRepository::new(&db.conn)
        .append(&tx, till, next, &intent(0xCA))
        .expect_err("a five-byte head is no parent");
    match refusal {
        DbError::AuditHeadUnreadable { seq, ref reason } => {
            assert_eq!(
                seq, 4,
                "the refusal names this till's head, not MAX(seq) = 5"
            );
            assert!(reason.contains("5 bytes"), "{reason}");
        }
        other => panic!("expected a located head refusal, got {other:?}"),
    }
}
