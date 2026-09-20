//! Microstep 1.9.2 — the owned document counters, against a real register.
//!
//! Gap G-2, and `ref/plan-validation.md:338` is the sentence these tests are
//! written to: *"Per-register counters must be crash-safe and gap-detectable.
//! A gap in a receipt sequence is what an auditor asks about first."*
//!
//! **Crash-safe and gap-detectable are two different claims**, and the tests
//! split along that line. `rollback_does_not_consume_a_number` and
//! `sequence_is_gap_free_under_crash_injection` are the first: a number is
//! spent only by a transaction that commits. `gaps_finds_the_number_whose
//! _document_never_arrived` is the second: when a gap does happen, it is
//! visible.
//!
//! Every test opens through [`pos_db::open`], so it runs the `MIGRATIONS` array
//! the application compiles with — `doc_sequence` and its three triggers as
//! `0005` shipped them, not as `ref/schema.md` describes them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Barrier};

use pos_db::DbError;
use pos_db::repo::sequence::{SeqKind, SequenceRepository, SequenceScope};
use pos_domain::{RegisterId, StoreId};
use rusqlite::{Connection, params};
use uuid::Uuid;

#[path = "common/registered_chain.rs"]
mod registered_chain;

use registered_chain::{AT, RegisteredChain};

const KEY: &str = "test-key";
const BUSINESS_DATE: &str = "2026-08-25";
const REGISTER: u128 = 0x01;

struct Register {
    _dir: tempfile::TempDir,
    conn: Connection,
    path: std::path::PathBuf,
    scope: SequenceScope,
    store: SequenceScope,
}

fn register(name: &str) -> Register {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    let conn = pos_db::open(&path, KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    let register_id = Uuid::from_u128(REGISTER);
    chain.add_register(&conn, register_id.as_bytes(), "REG01");
    let store = Uuid::from_slice(&chain.store).unwrap();
    Register {
        _dir: dir,
        conn,
        path,
        scope: SequenceScope::Register(RegisterId::from_uuid(register_id)),
        store: SequenceScope::Store(StoreId::from_uuid(store)),
    }
}

/// A parked sale carrying `number` as its receipt number.
///
/// Parked, not completed: `gaps()` reads `sale.receipt_number` whatever the
/// status, and completing one would drag in the seven gates `0005` puts in
/// front of it — none of which this microstep is about.
fn write_sale(conn: &Connection, seq: u64, number: &str) {
    conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1, ?2, ?3, 'parked', 2500, 400, 2900, 'JOD', ?4, ?5)",
        params![
            Uuid::from_u128(0x1000 + u128::from(seq))
                .as_bytes()
                .to_vec(),
            number,
            Uuid::from_u128(REGISTER).as_bytes().to_vec(),
            BUSINESS_DATE,
            AT
        ],
    )
    .unwrap();
}

fn message(error: &DbError) -> String {
    error.to_string()
}

#[test]
fn rollback_does_not_consume_a_number() {
    let r = register("rollback.db");
    let sequences = SequenceRepository::new(&r.conn);

    let tx = r.conn.unchecked_transaction().unwrap();
    let abandoned = sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
    // `Transaction` rolls back on drop, which is the whole mechanism under test:
    // the number was in hand and the transaction that held it never committed.
    drop(tx);
    assert_eq!(abandoned, 1);

    let tx = r.conn.unchecked_transaction().unwrap();
    let issued = sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
    tx.commit().unwrap();

    assert_eq!(
        issued, 1,
        "the abandoned transaction spent nothing, so 1 is still on offer"
    );
}

/// The `Done when`'s "deterministic fault schedule" over "one hundred rollback
/// points".
///
/// Deterministic means what it says: the schedule is a table, not a random
/// seed, and conventions §5 rules out an RNG outside proptest anyway. Each
/// point aborts at one of the positions a real checkout can actually die at,
/// cycling through them so all four are covered twenty-five times.
///
/// The property asserted at the end is the pair the microstep names — **no gap
/// and no duplicate** — and it is asserted against `gaps()` and against the
/// documents themselves, not against the counter, because the counter is the
/// thing under suspicion.
#[test]
fn sequence_is_gap_free_under_crash_injection() {
    /// Where a transaction dies. Every one of these is reachable on a till.
    #[derive(Clone, Copy)]
    enum Abort {
        /// The number is in hand; the document does not exist yet. This is the
        /// position G-2 is written about.
        AfterNumberBeforeDocument,
        /// Both exist in the transaction; nothing is durable.
        AfterDocumentBeforeCommit,
        /// Two numbers in flight in one transaction.
        AfterTwoNumbers,
        /// Nothing at all happened — the control, proving the harness itself
        /// consumes nothing.
        Nothing,
    }
    const SCHEDULE: [Abort; 4] = [
        Abort::AfterNumberBeforeDocument,
        Abort::AfterDocumentBeforeCommit,
        Abort::AfterTwoNumbers,
        Abort::Nothing,
    ];
    const POINTS: usize = 100;

    let r = register("crash-schedule.db");
    let sequences = SequenceRepository::new(&r.conn);
    let mut committed: Vec<u64> = Vec::new();

    // Cycled rather than indexed: `clippy::indexing_slicing` is denied
    // workspace-wide and does not care that `point % len()` is provably in
    // bounds. `.cycle().take(POINTS)` says the same thing and compiles.
    for abort in SCHEDULE.iter().copied().cycle().take(POINTS) {
        // One aborted attempt…
        let tx = r.conn.unchecked_transaction().unwrap();
        match abort {
            Abort::Nothing => {}
            Abort::AfterNumberBeforeDocument => {
                sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
            }
            Abort::AfterDocumentBeforeCommit => {
                let seq = sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
                write_sale(&tx, seq, &format!("{seq:06}"));
            }
            Abort::AfterTwoNumbers => {
                sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
                sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
            }
        }
        drop(tx);

        // …then one that commits, with its document, as a checkout does.
        let tx = r.conn.unchecked_transaction().unwrap();
        let seq = sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
        write_sale(&tx, seq, &format!("{seq:06}"));
        tx.commit().unwrap();
        committed.push(seq);
    }

    let expected: Vec<u64> = (1..=POINTS as u64).collect();
    assert_eq!(
        committed, expected,
        "a hundred commits either side of a hundred rollbacks must issue 1..=100 \
         with nothing skipped and nothing repeated"
    );
    assert_eq!(
        sequences.gaps(r.scope, SeqKind::Receipt).unwrap(),
        Vec::<u64>::new(),
        "every number issued has its document"
    );

    let documents: i64 = r
        .conn
        .query_row("SELECT count(*) FROM sale", [], |row| row.get(0))
        .unwrap();
    assert_eq!(documents, POINTS as i64, "no aborted document survived");
}

/// Two tills' worth of contention on one counter.
///
/// A `Barrier` releases both threads at the same instant so the writes really
/// overlap; without it the two would almost certainly serialise by arriving at
/// different times and the test would prove nothing.
///
/// This is deterministic rather than hopeful because of two facts measured in
/// this repository rather than assumed. `pos_db::open` sets
/// `busy_timeout` to five seconds (`crates/pos-db/src/lib.rs:160`), so the
/// second writer **waits** instead of failing with `SQLITE_BUSY`; and `next()`
/// is a single statement, so there is no read-then-write window in which both
/// could observe the same value. Either of those missing would make this flaky.
#[test]
fn concurrent_next_never_duplicates() {
    const THREADS: usize = 2;
    const EACH: usize = 25;

    let r = register("concurrent.db");
    let barrier = Arc::new(Barrier::new(THREADS));
    let scope = r.scope;
    let path = r.path.clone();

    let issued: Vec<u64> = std::thread::scope(|threads| {
        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let path = path.clone();
                threads.spawn(move || {
                    // A connection per thread: `rusqlite::Connection` is `Send`
                    // but not `Sync`, so one cannot be shared.
                    let conn = pos_db::open(&path, KEY).unwrap();
                    let sequences = SequenceRepository::new(&conn);
                    barrier.wait();
                    (0..EACH)
                        .map(|_| {
                            let tx = conn.unchecked_transaction().unwrap();
                            let seq = sequences.next(&tx, scope, SeqKind::Receipt).unwrap();
                            tx.commit().unwrap();
                            seq
                        })
                        .collect::<Vec<u64>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });

    let mut sorted = issued;
    sorted.sort_unstable();
    let expected: Vec<u64> = (1..=(THREADS * EACH) as u64).collect();
    assert_eq!(
        sorted,
        expected,
        "every number from 1 to {} exactly once, across both threads",
        THREADS * EACH
    );
}

#[test]
fn invalid_scope_for_sequence_kind_is_refused() {
    let r = register("scope-legality.db");
    let sequences = SequenceRepository::new(&r.conn);
    let tx = r.conn.unchecked_transaction().unwrap();

    // `0005`'s composite CHECK pairs receipt and zreport with a register, and
    // fiscal_icv with a store. The refusal is named in Rust because that CHECK
    // is table-level and unnamed: reaching it would raise a generic constraint
    // failure with no message to map, unlike the triggers `approval.rs` keys on.
    for (scope, kind) in [
        (r.store, SeqKind::Receipt),
        (r.store, SeqKind::ZReport),
        (r.scope, SeqKind::FiscalIcv),
    ] {
        let refused = sequences
            .next(&tx, scope, kind)
            .expect_err("0005 admits exactly one scope per kind");
        assert!(
            matches!(
                refused,
                DbError::SequenceScopeInvalid { .. } | DbError::SequenceNotYetAllocatable { .. }
            ),
            "expected a named refusal for {kind:?} on {scope:?}, found {refused:?}"
        );
    }

    // And the legal pairing still works, so the refusals above are about the
    // pairing rather than about the repository being broken.
    assert_eq!(sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap(), 1);
    assert_eq!(sequences.next(&tx, r.scope, SeqKind::ZReport).unwrap(), 1);
}

/// The first `fiscal_icv` row closes #113's reversal window, and `2.7.4` owns it.
#[test]
fn fiscal_icv_is_not_allocatable_in_phase_1() {
    let r = register("icv-refused.db");
    let sequences = SequenceRepository::new(&r.conn);
    let tx = r.conn.unchecked_transaction().unwrap();

    let refused = sequences
        .next(&tx, r.store, SeqKind::FiscalIcv)
        .expect_err("allocating an ICV in Phase 1 spends a decision 2.7.4 owns");
    assert!(
        matches!(refused, DbError::SequenceNotYetAllocatable { .. }),
        "found {refused:?}"
    );
    assert!(message(&refused).contains("2.7.4"), "{}", message(&refused));

    // The row must not exist: its existence is the thing that is irreversible,
    // not the number handed out.
    let rows: i64 = r
        .conn
        .query_row(
            "SELECT count(*) FROM doc_sequence WHERE kind = 'fiscal_icv'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0, "no fiscal_icv counter may exist yet");
}

#[test]
fn gaps_finds_the_number_whose_document_never_arrived() {
    let r = register("gap-visible.db");
    let sequences = SequenceRepository::new(&r.conn);

    // Three numbers; the second one's document is lost to a crash.
    for seq in 1..=3u64 {
        let tx = r.conn.unchecked_transaction().unwrap();
        let issued = sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
        if issued != 2 {
            write_sale(&tx, issued, &format!("{issued:06}"));
            tx.commit().unwrap();
        } else {
            // The bump commits on its own — which is exactly the shape a caller
            // gets wrong by bumping outside the document's transaction, and the
            // gap it leaves is what an auditor asks about.
            tx.commit().unwrap();
        }
        assert_eq!(issued, seq);
    }

    assert_eq!(
        sequences.gaps(r.scope, SeqKind::Receipt).unwrap(),
        vec![2],
        "the number with no document is reported, by value"
    );
}

/// An empty answer must mean "no gaps", never "no evidence".
#[test]
fn gaps_refuses_to_report_where_the_documents_do_not_exist() {
    let r = register("gap-unknowable.db");
    let sequences = SequenceRepository::new(&r.conn);

    // Nothing allocated: there is truthfully nothing missing.
    assert_eq!(
        sequences.gaps(r.scope, SeqKind::ZReport).unwrap(),
        Vec::<u64>::new()
    );

    let tx = r.conn.unchecked_transaction().unwrap();
    sequences.next(&tx, r.scope, SeqKind::ZReport).unwrap();
    tx.commit().unwrap();

    // Now a number has been issued and `z_report` does not exist to be counted,
    // so the honest answer is an error. Returning `Ok(vec![])` here would tell
    // an auditor the counter is sound on the strength of a table nobody wrote.
    let refused = sequences
        .gaps(r.scope, SeqKind::ZReport)
        .expect_err("no Z document table exists to compare against");
    assert!(
        matches!(refused, DbError::SequenceEvidenceUnavailable { .. }),
        "found {refused:?}"
    );
}

/// A receipt number this counter cannot read is an error, not a skipped row.
///
/// Skipping it would manufacture a gap that is not there; counting it as zero
/// would hide one that is. Both are worse than refusing.
#[test]
fn gaps_refuses_a_receipt_number_it_cannot_parse() {
    let r = register("gap-unreadable.db");
    let sequences = SequenceRepository::new(&r.conn);

    let tx = r.conn.unchecked_transaction().unwrap();
    let issued = sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
    write_sale(&tx, issued, "not-a-number");
    tx.commit().unwrap();

    let refused = sequences
        .gaps(r.scope, SeqKind::Receipt)
        .expect_err("a receipt number that is not a counter cannot be reconciled");
    assert!(
        matches!(refused, DbError::SequenceNumberUnreadable { .. }),
        "found {refused:?}"
    );
}

/// `next()` must not disturb a column it does not write, and the reason is
/// `INSERT OR REPLACE`.
///
/// This test exists because the mutation sweep found nothing else that would
/// catch it. The module doc-comment warns at length that `REPLACE` deletes the
/// row and re-inserts it, so `doc_sequence_monotonic` — a `BEFORE UPDATE OF
/// next_value` trigger — never fires, and `doc_sequence` has no delete guard
/// behind it (#179's fourth finding). But a `REPLACE` that still computes the
/// right number passes every other test in this file: the numbers come out
/// identical. A warning with no test is exactly the shape this repository keeps
/// finding in its own work.
///
/// What `REPLACE` cannot fake is the rest of the row. `prefix` is
/// `TEXT NOT NULL DEFAULT ''`, so a re-insert that does not name it silently
/// resets it — and `gaps()` reads `prefix` to strip it off a receipt number.
/// The register would lose its prefix and every gap report over it would start
/// refusing rows it should have parsed.
#[test]
fn next_preserves_the_row_it_does_not_own() {
    let r = register("prefix-survives.db");
    let sequences = SequenceRepository::new(&r.conn);

    let tx = r.conn.unchecked_transaction().unwrap();
    sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
    tx.commit().unwrap();
    r.conn
        .execute(
            "UPDATE doc_sequence SET prefix = 'REG01-' WHERE kind = 'receipt'",
            [],
        )
        .unwrap();

    let tx = r.conn.unchecked_transaction().unwrap();
    let issued = sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
    tx.commit().unwrap();
    assert_eq!(issued, 2);

    let prefix: String = r
        .conn
        .query_row(
            "SELECT prefix FROM doc_sequence WHERE kind = 'receipt'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        prefix, "REG01-",
        "advancing the counter must leave the prefix alone; a REPLACE would \
         reset it to the column default and take `gaps()` down with it"
    );
}

/// `doc_sequence_monotonic` accepts exactly `+1`, which is what makes the
/// upsert in `next()` legal and every other step illegal.
#[test]
fn the_counter_advances_by_exactly_one_or_not_at_all() {
    let r = register("monotonic.db");
    let sequences = SequenceRepository::new(&r.conn);
    let tx = r.conn.unchecked_transaction().unwrap();
    sequences.next(&tx, r.scope, SeqKind::Receipt).unwrap();
    tx.commit().unwrap();

    let refused = r
        .conn
        .execute(
            "UPDATE doc_sequence SET next_value = next_value + 5 WHERE kind = 'receipt'",
            [],
        )
        .expect_err("a jump is how a counter loses numbers silently");
    assert_eq!(
        match refused {
            rusqlite::Error::SqliteFailure(_, Some(message)) => message,
            other => panic!("expected a trigger refusal, found {other:?}"),
        },
        "document sequences advance by exactly one"
    );
}
