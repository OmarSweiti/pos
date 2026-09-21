#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! `verify-audit` against real databases, driven as a process (microstep 1.6.6b).
//!
//! Every test here spawns the binary rather than calling a function, because
//! the thing under test **is** the command: its exit code is what the Phase-1
//! exit gate reads (`phase-1:1479`), and an exit code has no unit test.
//! `pos_domain::verify_chain` already has one; this file proves the wrapper
//! around it — which file it opened, which registers it walked, which anchor it
//! applied to which chain, and what number the shell sees afterwards.
//!
//! **The tamper is applied to a copy, exactly as the drill says.** The original
//! is never altered: `audit_log_no_update` and `audit_log_no_delete` refuse
//! both, and `the_original_database_still_refuses_an_audit_update` is the test
//! that holds them to it after its copy has been rewritten. Dropping a trigger
//! is how a copy is made tamperable at all, and it is the honest simulation of
//! an attacker with the file and a SQL console — which is precisely the threat
//! a hash chain exists to survive rather than prevent.
//!
//! **What a careless version of each named test would miss** is recorded beside
//! it. That is the failure this repository keeps catching: a test that passes
//! without exercising the branch its name claims.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use pos_db::repo::audit::AuditRepository;
use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use pos_domain::{AuditIntent, RegisterId, Timestamp, UserId};
use rusqlite::{Connection, params};
use serde_json::json;
use uuid::Uuid;

#[path = "common/registered_chain.rs"]
mod registered_chain;

use registered_chain::RegisteredChain;

/// Deliberately not a plausible key. `the_report_never_prints_the_database_key`
/// greps the whole output for it, and a short or generic value would make that
/// assertion pass by coincidence.
const KEY: &str = "pos-db-audit-verifier-fixture-key-2f9c";

/// One fixed instant, as text and as milliseconds — the same moment twice, the
/// pairing `the_stored_hash_matches_a_pinned_golden` caught `tests/audit.rs`
/// getting wrong.
const AT: &str = "2026-08-29T09:15:00.250Z";
const AT_MS: i64 = 1_787_994_900_250;

const REGISTER: u8 = 0xF0;
const OTHER_REGISTER: u8 = 0xF1;

/// A BLAKE3 digest of nothing in particular, for an anchor that must be refused
/// or ignored before its hash is ever compared.
const UNUSED_DIGEST: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

// ─────────────────────────────────────────────────────────────────────────────
// The fixture: a real database, closed, so it can be copied byte for byte
// ─────────────────────────────────────────────────────────────────────────────

struct Evidence {
    // Dropped last, so the database file outlives every path handed out of here.
    _dir: tempfile::TempDir,
    original: PathBuf,
}

impl Evidence {
    /// Build a database, run `seed` against it, then **close it**.
    ///
    /// Closing is what makes a one-file copy honest. `pos_db::open` puts the
    /// connection in WAL mode, so until the last connection closes the newest
    /// commits live in a `-wal` sidecar; copying the database file alone at
    /// that point would silently produce a shorter chain, and every test here
    /// would then be asserting against a tamper the test itself introduced.
    fn build<T>(name: &str, seed: impl FnOnce(&Connection, &RegisteredChain) -> T) -> (Self, T) {
        let dir = tempfile::tempdir().expect("the fixture needs a private directory");
        let original = dir.path().join(name);
        let seeded = {
            let connection = pos_db::open(&original, KEY).expect("the shipped chain must open");
            let chain = RegisteredChain::seed(&connection);
            chain.add_register(&connection, id(REGISTER).as_slice(), "REG01");
            chain.add_register(&connection, id(OTHER_REGISTER).as_slice(), "REG02");
            let seeded = seed(&connection, &chain);
            connection
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
                .expect("the WAL must be folded back before the file is copied");
            seeded
        };
        (
            Self {
                _dir: dir,
                original,
            },
            seeded,
        )
    }

    /// A chain of `rows` entries for `REGISTER`, and every stored `hash` in
    /// order, so a test can anchor any position without re-reading the file.
    fn chain(name: &str, rows: u8) -> (Self, Vec<[u8; 32]>) {
        Self::build(name, |connection, _| {
            (1..=rows)
                .map(|tag| append(connection, register(REGISTER), uuid(tag), &intent(tag)))
                .collect()
        })
    }

    fn beside(&self, name: &str) -> PathBuf {
        let copy = self
            .original
            .parent()
            .expect("the fixture database is inside its temporary directory")
            .join(name);
        std::fs::copy(&self.original, &copy).expect("the evidence copy must be writable");
        copy
    }

    fn path(&self) -> &Path {
        &self.original
    }

    fn anchor(&self, name: &str, register: u8, last_seq: u64, last_hash: &[u8; 32]) -> PathBuf {
        self.anchor_text(
            name,
            &json!({
                "register_id": uuid(register).to_string(),
                "last_seq": last_seq,
                "last_hash": hex(last_hash),
                "source_kind": "verified_backup",
                "anchored_at": AT,
            })
            .to_string(),
        )
    }

    fn anchor_text(&self, name: &str, text: &str) -> PathBuf {
        let path = self
            .original
            .parent()
            .expect("the fixture database is inside its temporary directory")
            .join(name);
        std::fs::write(&path, text).expect("the anchor file must be writable");
        path
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

/// A plain, unapproved audit intent. `entity_id` varies with `tag`, so two rows
/// are never the same bytes and a reader that returned one twice would show.
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
        at: Timestamp::from_epoch_milliseconds(AT_MS).unwrap(),
    }
}

/// Envelope and append in one transaction, the way a command handler does it.
///
/// `audit_log_has_ready_commit` refuses the insert without the envelope, so
/// this is not fixture ceremony: it is I-9, and `tests/audit.rs` writes the
/// same shape for the same reason.
fn append(connection: &Connection, till: RegisterId, row: Uuid, intent: &AuditIntent) -> [u8; 32] {
    let transaction = connection.unchecked_transaction().unwrap();
    let mut commit = *row.as_bytes();
    commit[0] = 0xCC;
    let mut change = *row.as_bytes();
    change[2] = 0xC0;

    OutboxRepository::new(connection)
        .write_commit(
            &transaction,
            &CommitEnvelope {
                commit_id: &commit,
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "test",
                created_at: AT,
            },
            &[FactMember {
                change_id: &change,
                entity: "audit_log",
                entity_id: row.as_bytes(),
                payload: "{}",
            }],
        )
        .expect("a fixture envelope must be complete when it is written");

    let appended = AuditRepository::new(connection)
        .append(&transaction, till, row, intent)
        .expect("an enveloped append must be accepted");
    transaction.commit().unwrap();
    appended.hash
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Rewrite a copy, with the append-only trigger named by `trigger` removed.
///
/// The `DROP` is the simulation: an attacker holding the file holds the whole
/// schema too, and a chain that could only be defeated by someone unable to
/// drop a trigger would not be worth hashing.
fn tamper(copy: &Path, trigger: &str, statements: &[&str]) {
    let connection =
        pos_db::open(copy, KEY).expect("the copy must open like any register database");
    connection
        .execute_batch(&format!("DROP TRIGGER {trigger};"))
        .expect("a copy is where a trigger may be dropped; the original refuses the write itself");
    for statement in statements {
        connection.execute(statement, []).unwrap_or_else(|error| {
            panic!("the tamper statement must apply: {statement}: {error}")
        });
    }
    connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .expect("the rewritten copy must be on disk before the verifier opens it");
}

// ─────────────────────────────────────────────────────────────────────────────
// Driving the binary
// ─────────────────────────────────────────────────────────────────────────────

struct Run {
    code: i32,
    out: String,
    err: String,
}

impl Run {
    fn says(&self, needle: &str) -> bool {
        self.out.contains(needle)
    }
}

/// Run the verifier, with the key supplied the way a debug build accepts one.
///
/// **This suite refuses to run in a release build rather than skipping.**
/// `POS_DB_KEY` is honoured only in debug (microstep 1.8.5), so a release run
/// would fall through to the machine's **real** OS credential store and, on a
/// clean machine, write a key into it — which `key.rs`'s own test calls worse
/// than the gap it would close. A silent zero-test run is the other way to get
/// this wrong: a coverage claim whose coverage has quietly disappeared. So it
/// fails, here, with the reason.
fn run_verifier(arguments: &[&std::ffi::OsStr]) -> Run {
    require_a_debug_build();
    let output: Output = Command::new(env!("CARGO_BIN_EXE_verify-audit"))
        .args(arguments)
        .env("POS_DB_KEY", KEY)
        .output()
        .expect("the verifier binary must be runnable");
    Run {
        code: output
            .status
            .code()
            .expect("the verifier exits rather than dying on a signal"),
        out: String::from_utf8_lossy(&output.stdout).into_owned(),
        err: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Refuse a release-profile run, rather than skipping one.
///
/// Two `#[cfg]` bodies rather than `assert!(cfg!(debug_assertions), …)`, which
/// clippy correctly refuses as a constant assertion, and rather than a `const`
/// block, which would make a release build fail to *compile* — breaking
/// `cargo build --release --all-targets` for everybody to protect a suite
/// nobody was running. A red test in the one profile that would misbehave is
/// the proportionate answer.
#[cfg(debug_assertions)]
fn require_a_debug_build() {}

#[cfg(not(debug_assertions))]
fn require_a_debug_build() {
    panic!(
        "this suite drives verify-audit with POS_DB_KEY, which only a debug \
         build honours (1.8.5); a release run would fall through to the \
         machine's real credential store and, on a clean machine, write a key \
         into it. Run it without --release."
    );
}

fn verify(database: &Path) -> Run {
    run_verifier(&["--database".as_ref(), database.as_ref()])
}

fn verify_anchored(database: &Path, anchor: &Path) -> Run {
    run_verifier(&[
        "--database".as_ref(),
        database.as_ref(),
        "--anchor".as_ref(),
        anchor.as_ref(),
    ])
}

// ─────────────────────────────────────────────────────────────────────────────
// The three tests the phase file names
// ─────────────────────────────────────────────────────────────────────────────

/// **What a careless version of this test would miss.** Tamper exactly one row
/// and any implementation that reports *a* break passes, including one that
/// walks backwards or reports the last disagreement it saw. Two rows are
/// rewritten here — seq 2 and seq 4 — and the assertion is that the report
/// names 2 and never 4. `verify_chain` stops at the first disagreement because
/// after one the rest is noise, and "first" is the whole forensic value: it is
/// where an investigation starts.
#[test]
fn verifier_reports_the_first_broken_sequence() {
    let (evidence, _) = Evidence::chain("broken.db", 4);
    let copy = evidence.beside("broken-copy.db");
    tamper(
        &copy,
        "audit_log_no_update",
        &[
            "UPDATE audit_log SET reason = 'edited' WHERE seq = 2",
            "UPDATE audit_log SET reason = 'edited too' WHERE seq = 4",
        ],
    );

    let run = verify(&copy);
    assert_eq!(
        run.code, 1,
        "tamper evidence is exit 1\n{}{}",
        run.out, run.err
    );
    assert!(run.says("BROKEN at seq 2"), "{}", run.out);
    assert!(
        !run.says("BROKEN at seq 4"),
        "the walk must stop at the first break, not report the last:\n{}",
        run.out
    );
    assert!(run.says("summary: TAMPERED (exit 1)"), "{}", run.out);

    // And the untouched original still verifies, so the tamper is the copy's.
    assert_eq!(verify(evidence.path()).code, 0);
}

/// **What a careless version of this test would miss.** Delete the newest rows,
/// run with an anchor, see a non-zero exit, call it detection. That proves
/// nothing about the anchor: a chain missing its tail is still internally
/// consistent, so the run *without* the anchor must be green. Both runs are
/// made here, over the same tampered copy, and the difference between them is
/// the entire contribution of the external anchor — the one tamper a local hash
/// chain cannot see (E.91).
#[test]
fn tail_deletion_is_detected_against_the_last_anchor() {
    let (evidence, hashes) = Evidence::chain("truncated.db", 4);
    let head = *hashes.get(3).expect("four rows were appended");
    let copy = evidence.beside("truncated-copy.db");
    tamper(
        &copy,
        "audit_log_no_delete",
        &["DELETE FROM audit_log WHERE seq > 2"],
    );

    let unanchored = verify(&copy);
    assert_eq!(
        unanchored.code, 0,
        "a chain cannot see its own tail: without an anchor this is invisible\n{}",
        unanchored.out
    );
    assert!(unanchored.says("INTACT SO FAR"), "{}", unanchored.out);

    let anchor = evidence.anchor("anchor.json", REGISTER, 4, &head);
    let anchored = verify_anchored(&copy, &anchor);
    assert_eq!(anchored.code, 1, "{}{}", anchored.out, anchored.err);
    // Provenance is echoed. An anchor is only as good as where it came from,
    // and a report that quoted the numbers without the source would leave the
    // reader unable to tell a Z report from a file somebody edited.
    assert!(
        anchored.says("anchor from verified_backup"),
        "{}",
        anchored.out
    );
    assert!(
        anchored.says(&format!("anchor at   {AT}")),
        "{}",
        anchored.out
    );
    assert!(
        anchored.says(
            "TRUNCATED — the anchor records seq 4 as the head and the highest \
             row present is 2"
        ),
        "the verdict names both numbers, because \"a tail was deleted\" without \
         them is not something anyone can act on:\n{}",
        anchored.out
    );
    assert!(
        anchored.says("summary: TAMPERED (exit 1)"),
        "{}",
        anchored.out
    );
}

/// **What a careless version of this test would miss.** Asserting that the
/// `UPDATE` returns an error proves the trigger exists, which
/// `audit_log_still_refuses_update_and_delete` already proves. What this adds
/// is that it is still true *after* a copy of the same database has been
/// rewritten and reported broken — that the drill's "only on that consistent
/// copy" is a real boundary and not a description of good manners.
#[test]
fn the_original_database_still_refuses_an_audit_update() {
    let (evidence, _) = Evidence::chain("original.db", 3);
    let copy = evidence.beside("original-copy.db");
    tamper(
        &copy,
        "audit_log_no_update",
        &["UPDATE audit_log SET reason = 'edited' WHERE seq = 1"],
    );
    assert_eq!(verify(&copy).code, 1, "the copy is broken");

    let connection = pos_db::open(evidence.path(), KEY).expect("the original still opens");
    let update = connection
        .execute("UPDATE audit_log SET reason = 'edited' WHERE seq = 1", [])
        .expect_err("the original refuses UPDATE");
    assert!(
        update
            .to_string()
            .contains("I-4: audit_log is append-only — no UPDATE, ever"),
        "the trigger's own words: {update}"
    );
    let delete = connection
        .execute("DELETE FROM audit_log WHERE seq = 1", [])
        .expect_err("the original refuses DELETE");
    assert!(
        delete
            .to_string()
            .contains("I-4: audit_log is append-only — no DELETE, ever"),
        "the trigger's own words: {delete}"
    );
    drop(connection);

    let original = verify(evidence.path());
    assert_eq!(original.code, 0, "{}{}", original.out, original.err);
    assert!(original.says("INTACT SO FAR"), "{}", original.out);
}

// ─────────────────────────────────────────────────────────────────────────────
// The anchor, and the three ways it can say nothing
// ─────────────────────────────────────────────────────────────────────────────

/// Exit 0 is not "nothing was deleted", and the report has to say so.
///
/// Without an anchor the strongest honest statement is *these rows agree with
/// each other*. A verifier that printed `INTACT` there would be claiming a
/// coverage it cannot have, and the merchant reading it would stop looking for
/// the external record that is the only thing which closes the gap.
#[test]
fn an_intact_chain_exits_zero_and_reports_the_tail_as_unanchored() {
    let (evidence, _) = Evidence::chain("intact.db", 3);
    let run = verify(evidence.path());

    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.says("rows read   3"), "{}", run.out);
    assert!(run.says("INTACT SO FAR — 3 entries"), "{}", run.out);
    assert!(
        run.says("nothing here rules out a deleted tail"),
        "{}",
        run.out
    );
    assert!(run.says("none supplied"), "{}", run.out);
    assert!(run.says("summary: VERIFIED (exit 0)"), "{}", run.out);
}

/// An anchor covers the register it names and no other.
///
/// `verify_chain` discards a foreign anchor itself, so the risk is not a wrong
/// verdict — it is a report whose one anchor line at the top reads as if it
/// covered everything under it. Each register therefore states its own anchor,
/// and the unanchored one keeps saying so.
#[test]
fn an_anchor_for_another_register_anchors_nothing() {
    let (evidence, heads) = Evidence::build("two-tills.db", |connection, _| {
        let first = (1..=3)
            .map(|tag| append(connection, register(REGISTER), uuid(tag), &intent(tag)))
            .collect::<Vec<_>>();
        let second = (0x21..=0x22u8)
            .map(|tag| {
                append(
                    connection,
                    register(OTHER_REGISTER),
                    uuid(tag),
                    &intent(tag),
                )
            })
            .collect::<Vec<_>>();
        (first, second)
    });
    let other_head = *heads.1.last().expect("the second till wrote two rows");

    let anchor = evidence.anchor("other.json", OTHER_REGISTER, 5, &other_head);
    let run = verify_anchored(evidence.path(), &anchor);

    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    // `seq` is table-global, so the second till's two rows are 4 and 5.
    assert!(
        run.says("INTACT — 2 entries, anchored through the head"),
        "the anchored till is fully covered:\n{}",
        run.out
    );
    assert!(
        run.says("none for this register"),
        "the other till must not inherit the anchor:\n{}",
        run.out
    );
    assert!(run.says("INTACT SO FAR — 3 entries"), "{}", run.out);
}

/// `last_seq: 0` is a register that had written nothing. It is not a break.
///
/// Passed through to `verify_chain` it becomes `Broken { at_seq: 0 }`, because
/// no row is ever numbered zero and the anchored hash is never found — a
/// verifier reporting tampering where none happened, which is the one answer
/// `repo::audit` says a forensic tool must never give.
#[test]
fn an_anchor_recording_an_empty_chain_reports_no_tamper() {
    let (evidence, _) = Evidence::chain("empty-anchor.db", 3);
    let anchor = evidence.anchor_text(
        "empty.json",
        &json!({
            "register_id": uuid(REGISTER).to_string(),
            "last_seq": 0,
            "last_hash": UNUSED_DIGEST,
        })
        .to_string(),
    );

    let run = verify_anchored(evidence.path(), &anchor);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.says("it anchors nothing"), "{}", run.out);
    assert!(!run.says("BROKEN"), "{}", run.out);
    assert!(run.says("INTACT SO FAR — 3 entries"), "{}", run.out);
    // The register's own block says the anchor is for it and still anchors
    // nothing — which "none for this register" would have got wrong.
    assert!(
        run.says("anchor      for this register, recording it as having written no audit row"),
        "{}",
        run.out
    );
}

/// Deleting **every** row of a register is the hole enumeration alone leaves.
///
/// A verifier that walked only the registers it found in `audit_log` would see
/// no register, walk nothing, print nothing and exit 0 — while holding an
/// anchor that says there were three rows. Both halves are asserted here: the
/// unanchored run really does report an empty file, and the anchored run
/// reports the truncation, because the anchor's own register joins the set.
#[test]
fn an_anchored_register_whose_rows_are_all_gone_is_still_reported() {
    let (evidence, hashes) = Evidence::chain("erased.db", 3);
    let head = *hashes.get(2).expect("three rows were appended");
    let copy = evidence.beside("erased-copy.db");
    tamper(&copy, "audit_log_no_delete", &["DELETE FROM audit_log"]);

    let unanchored = verify(&copy);
    assert_eq!(unanchored.code, 0, "{}", unanchored.out);
    assert!(
        unanchored.says("no audit rows, and no anchor"),
        "an empty file says nothing on its own:\n{}",
        unanchored.out
    );

    let anchor = evidence.anchor("erased.json", REGISTER, 3, &head);
    let anchored = verify_anchored(&copy, &anchor);
    assert_eq!(anchored.code, 1, "{}{}", anchored.out, anchored.err);
    assert!(anchored.says("rows read   0"), "{}", anchored.out);
    assert!(anchored.says("TRUNCATED"), "{}", anchored.out);
    assert!(
        anchored.says("the highest row present is 0"),
        "{}",
        anchored.out
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Everything that is not a verdict
// ─────────────────────────────────────────────────────────────────────────────

/// A row this build cannot rebuild is exit 3, and the rows below it survive.
///
/// The verdict printed above the stop covers the prefix and must not be read as
/// covering the register, which is why the stop line says so in words and why
/// the exit code is neither 0 nor 1. `canonical_version` is the realistic
/// cause: once this product has two versions in the field, a row written under
/// a layout the reading binary does not know is an upgrade, not an attack.
#[test]
fn a_row_this_build_cannot_rebuild_is_inconclusive_rather_than_intact() {
    let (evidence, _) = Evidence::chain("stopped.db", 3);
    let copy = evidence.beside("stopped-copy.db");
    tamper(
        &copy,
        "audit_log_no_update",
        &["UPDATE audit_log SET canonical_version = 99 WHERE seq = 2"],
    );

    let run = verify(&copy);
    assert_eq!(
        run.code, 3,
        "a stop is inconclusive, not a tamper verdict\n{}{}",
        run.out, run.err
    );
    assert!(run.says("rows read   1"), "{}", run.out);
    assert!(run.says("stopped     at seq 2"), "{}", run.out);
    assert!(run.says("canonical version 99"), "{}", run.out);
    assert!(
        run.says("the verdict above covers only those below"),
        "{}",
        run.out
    );
    assert!(run.says("summary: INCONCLUSIVE (exit 3)"), "{}", run.out);
}

/// An anchor above a stopped read is withheld, and that is not a nicety.
///
/// This is the one place a correct anchor and a correct stop combine into a
/// wrong answer. The read stops at seq 2, so `verify_chain` is handed one row;
/// an anchor at seq 4 then makes it answer `Truncated { anchored_seq: 4,
/// found_seq: 1 }` — *the rows between were removed*. They were not. Rows 2, 3
/// and 4 are all still on the disk, and the tool would have accused a merchant
/// of deleting them because it could not read one of them itself.
///
/// **What a careless version of this test would miss.** Assert only that the
/// exit code is not 1 and an implementation that dropped every anchor whenever
/// a read stopped would pass — throwing away the one instrument that detects a
/// tail deletion, in exactly the situation where somebody is already looking.
/// So the sibling below proves the anchor *below* a stop still applies.
#[test]
fn an_anchor_above_a_stop_is_withheld_rather_than_read_as_a_truncation() {
    let (evidence, hashes) = Evidence::chain("withheld.db", 4);
    let head = *hashes.get(3).expect("four rows were appended");
    let copy = evidence.beside("withheld-copy.db");
    tamper(
        &copy,
        "audit_log_no_update",
        &["UPDATE audit_log SET canonical_version = 99 WHERE seq = 2"],
    );

    let anchor = evidence.anchor("above.json", REGISTER, 4, &head);
    let run = verify_anchored(&copy, &anchor);

    assert_eq!(
        run.code, 3,
        "an unreadable row is inconclusive; it is not evidence of a deletion\n{}{}",
        run.out, run.err
    );
    assert!(run.says("anchor      WITHHELD"), "{}", run.out);
    assert!(
        !run.says("TRUNCATED"),
        "rows 2 to 4 are still on the disk:\n{}",
        run.out
    );
    assert!(run.says("stopped     at seq 2"), "{}", run.out);
}

/// And an anchor **below** a stop still does its whole job.
///
/// The row it names was read, so the comparison it exists for is available and
/// is made. Withholding it too would trade one wrong answer for a blind spot.
#[test]
fn an_anchor_below_a_stop_is_still_applied() {
    let (evidence, hashes) = Evidence::chain("below.db", 4);
    let first = *hashes.first().expect("four rows were appended");
    let copy = evidence.beside("below-copy.db");
    tamper(
        &copy,
        "audit_log_no_update",
        &["UPDATE audit_log SET canonical_version = 99 WHERE seq = 3"],
    );

    let anchor = evidence.anchor("below.json", REGISTER, 1, &first);
    let run = verify_anchored(&copy, &anchor);

    assert_eq!(
        run.code, 3,
        "the stop is still a stop\n{}{}",
        run.out, run.err
    );
    assert!(run.says("anchor      last_seq 1"), "{}", run.out);
    assert!(
        run.says("anchored below seq 2"),
        "the anchor was applied, and the report says exactly how far it \
         reaches:\n{}",
        run.out
    );
    assert!(run.says("stopped     at seq 3"), "{}", run.out);
}

/// Both at once: a break below a row this build cannot rebuild.
///
/// `worse` has to choose between them, and the choice is the one a reader is
/// most likely to get backwards. "This register was altered" is a finding
/// somebody must act on today; "one of its rows would not come back" is a
/// reason to look again. A run that found both and reported only the weaker
/// one would bury the stronger, and the exit code is all a shell script sees.
#[test]
fn tamper_evidence_outranks_a_row_that_cannot_be_rebuilt() {
    let (evidence, _) = Evidence::chain("both.db", 4);
    let copy = evidence.beside("both-copy.db");
    tamper(
        &copy,
        "audit_log_no_update",
        &[
            "UPDATE audit_log SET reason = 'edited' WHERE seq = 2",
            "UPDATE audit_log SET canonical_version = 99 WHERE seq = 3",
        ],
    );

    let run = verify(&copy);
    assert_eq!(
        run.code, 1,
        "tamper evidence outranks a stop\n{}{}",
        run.out, run.err
    );
    assert!(run.says("BROKEN at seq 2"), "{}", run.out);
    assert!(run.says("stopped     at seq 3"), "{}", run.out);
    assert!(run.says("summary: TAMPERED (exit 1)"), "{}", run.out);
}

/// Rows belonging to no register are counted, and counting them is exit 3.
///
/// `audit_log.register_id` carries no `REFERENCES` clause and no width check,
/// so a row can name something that is not a register at all. It is in no
/// chain, and a tool that quietly dropped it would be reporting on a subset of
/// the file while sounding like it had read the file.
#[test]
fn rows_that_belong_to_no_register_are_counted_rather_than_hidden() {
    let (evidence, _) = Evidence::build("orphan.db", |connection, chain| {
        append(connection, register(REGISTER), uuid(1), &intent(1));
        write_orphan_row(connection, chain);
    });

    let run = verify(evidence.path());
    assert_eq!(run.code, 3, "{}{}", run.out, run.err);
    assert!(
        run.says("unattributed  1 row(s) carry a register_id that is not an id"),
        "{}",
        run.out
    );
    // The honest chain is still walked and still reported.
    assert!(run.says("INTACT SO FAR — 1 entries"), "{}", run.out);
}

/// A mistyped path must not be answered with "verified".
///
/// `Connection::open` creates a missing file and `pos_db::open` migrates the
/// empty result, so without the guard this run would build a database, find no
/// audit rows in it and exit 0 — the worst wrong answer the tool can give,
/// because it is indistinguishable from a clean register.
#[test]
fn a_missing_database_is_a_tool_failure_not_a_verdict() {
    let directory = tempfile::tempdir().unwrap();
    let absent = directory.path().join("never-existed.db");

    let run = run_verifier(&["--database".as_ref(), absent.as_os_str()]);
    assert_eq!(run.code, 2, "{}{}", run.out, run.err);
    assert!(run.err.contains("is not an existing file"), "{}", run.err);
    assert!(
        !run.says("summary:"),
        "no verdict may be printed:\n{}",
        run.out
    );
    assert!(
        !absent.exists(),
        "a verifier must not create the database it was asked to read"
    );
}

/// An anchor that cannot be read is exit 2, never a silent unanchored pass.
///
/// The failure this closes is quiet: fall back to "no anchor" on a malformed
/// file and the operator gets a green run, an `INTACT SO FAR`, and no idea that
/// the external record they supplied was never consulted.
#[test]
fn a_malformed_anchor_is_a_tool_failure_not_an_unanchored_pass() {
    let (evidence, _) = Evidence::chain("bad-anchor.db", 3);

    let truncated_hash = evidence.anchor_text(
        "short.json",
        &json!({
            "register_id": uuid(REGISTER).to_string(),
            "last_seq": 3,
            "last_hash": "deadbeef",
        })
        .to_string(),
    );
    let run = verify_anchored(evidence.path(), &truncated_hash);
    assert_eq!(run.code, 2, "{}{}", run.out, run.err);
    assert!(run.err.contains("last_hash"), "{}", run.err);
    assert!(!run.says("summary:"), "{}", run.out);

    let missing = evidence
        .path()
        .parent()
        .unwrap()
        .join("no-such-anchor.json");
    let absent = verify_anchored(evidence.path(), &missing);
    assert_eq!(absent.code, 2, "{}{}", absent.out, absent.err);
    assert!(absent.err.contains("cannot read"), "{}", absent.err);
}

/// The report names where the key came from and never what it is.
///
/// `.claude/rules/security.md` puts `db_key` on the never-log list and reaches
/// anything that prints. `KeySource` exists precisely so a diagnostic can be
/// honest about provenance without the value, and this holds the report to it
/// over both streams.
#[test]
fn the_report_never_prints_the_database_key() {
    let (evidence, _) = Evidence::chain("key.db", 2);
    let run = verify(evidence.path());

    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        !run.out.contains(KEY),
        "the key reached stdout:\n{}",
        run.out
    );
    assert!(
        !run.err.contains(KEY),
        "the key reached stderr:\n{}",
        run.err
    );
    assert!(
        run.says("key source  POS_DB_KEY"),
        "the source is named, and it is the source:\n{}",
        run.out
    );

    // A refusal prints too, and a refusal is where a value most often leaks.
    let refused = run_verifier(&["--database".as_ref(), "/nonexistent/dir/x.db".as_ref()]);
    assert!(!refused.out.contains(KEY), "{}", refused.out);
    assert!(!refused.err.contains(KEY), "{}", refused.err);
}

/// Half of the `Done when`, asserted rather than run by hand once.
///
/// The exit codes are in the help text because they are the tool's actual
/// output: a Phase-1 exit gate and a shell script both branch on the number,
/// and a number whose meaning lives only in a source comment is a number
/// somebody will guess at.
#[test]
fn help_exits_zero_and_names_every_exit_code() {
    for flag in ["--help", "-h"] {
        let run = run_verifier(&[flag.as_ref()]);
        assert_eq!(run.code, 0, "{flag}: {}{}", run.out, run.err);
        for expected in [
            "USAGE",
            "--database <path>",
            "--anchor <path>",
            "Give it a COPY",
            "THE ANCHOR FILE",
            "last_seq",
            "THE DATABASE KEY",
            "never the\n    key",
            "EXIT CODES",
            "Tamper evidence",
            "Inconclusive",
        ] {
            assert!(
                run.says(expected),
                "{flag} must name {expected:?}:\n{}",
                run.out
            );
        }
    }
}

/// Every register in the file is walked, not just the first one found.
#[test]
fn every_register_in_the_database_is_verified() {
    let (evidence, _) = Evidence::build("all-tills.db", |connection, _| {
        for tag in 1..=3u8 {
            append(connection, register(REGISTER), uuid(tag), &intent(tag));
        }
        for tag in 0x21..=0x22u8 {
            append(
                connection,
                register(OTHER_REGISTER),
                uuid(tag),
                &intent(tag),
            );
        }
    });

    let run = verify(evidence.path());
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.says(&format!("register {}", uuid(REGISTER))),
        "{}",
        run.out
    );
    assert!(
        run.says(&format!("register {}", uuid(OTHER_REGISTER))),
        "{}",
        run.out
    );
    assert!(run.says("rows read   3"), "{}", run.out);
    assert!(run.says("rows read   2"), "{}", run.out);
}

// ─────────────────────────────────────────────────────────────────────────────

/// One `audit_log` row whose `register_id` is four bytes, with the delivery
/// envelope the insert trigger demands. Written by hand because no repository
/// method can produce it — which is the point: it is what a SQL console does.
fn write_orphan_row(connection: &Connection, chain: &RegisteredChain) {
    let row = uuid(0x5A);
    let mut commit = *row.as_bytes();
    commit[0] = 0xCC;
    let mut change = *row.as_bytes();
    change[2] = 0xC0;

    let transaction = connection.unchecked_transaction().unwrap();
    OutboxRepository::new(connection)
        .write_commit(
            &transaction,
            &CommitEnvelope {
                commit_id: &commit,
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "test",
                created_at: AT,
            },
            &[FactMember {
                change_id: &change,
                entity: "audit_log",
                entity_id: row.as_bytes(),
                payload: "{}",
            }],
        )
        .unwrap();
    transaction
        .execute(
            "INSERT INTO audit_log
               (id, register_id, actor_id, action, entity, entity_id, payload,
                prev_hash, hash, at)
             VALUES (?1, ?2, ?3, 'drawer.open', 'drawer_event', ?1, '{}', ?4, ?5, ?6)",
            params![
                row.as_bytes().as_slice(),
                vec![0x01u8, 0x02, 0x03, 0x04],
                chain.cashier,
                vec![0x00u8; 32],
                vec![0x7Au8; 32],
                AT
            ],
        )
        .expect("nothing in the schema constrains register_id to sixteen bytes");
    transaction.commit().unwrap();
}
