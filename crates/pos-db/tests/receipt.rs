//! `ReceiptRepository` (microstep 1.7.7) against migration `0005`'s receipt
//! and print-queue tables, on a real encrypted database and a sale the schema
//! lets complete. `ref/hardware-and-receipts.md` §3 is the contract.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pos_db::repo::outbox::{CommitEnvelope, OutboxRepository};
use pos_db::repo::receipt::{
    ArtifactKind, AttemptOutcome, Id, JobState, Lease, PrintAttempt, ReceiptArtifact,
    ReceiptRepository,
};
use pos_domain::Timestamp;
use rusqlite::{Connection, params};
use uuid::Uuid;

use pos_db::fixtures::registered_chain;
use registered_chain::{Checkout, Member, RegisteredChain};

const KEY: &str = "test-key";
const BUSINESS_DATE: &str = "2026-08-25";
const WORKER: &str = "print-worker-1";

fn id(value: u128) -> Vec<u8> {
    Uuid::from_u128(value).as_bytes().to_vec()
}

fn id16(bytes: &[u8]) -> Id {
    Id::try_from(bytes).expect("a sixteen-byte id")
}

/// An instant `seconds` after a fixed one. No test reads the wall clock.
fn at(seconds: i64) -> Timestamp {
    Timestamp::from_epoch_milliseconds(1_787_000_000_000 + seconds * 1_000).expect("an instant")
}

fn message(error: pos_db::DbError) -> String {
    match error {
        pos_db::DbError::Sqlite(rusqlite::Error::SqliteFailure(_, Some(message))) => message,
        other => panic!("expected a schema refusal, found {other:?}"),
    }
}

struct Register {
    dir: tempfile::TempDir,
    conn: Connection,
    chain: RegisteredChain,
}

const FILE: &str = "receipts.db";

fn register() -> Register {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join(FILE), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    chain.add_register(&conn, &id(0x01), "REG01");
    Register { dir, conn, chain }
}

/// The receipt bytes a sale's original is printed from. Synthetic.
fn bytes(sale: u128) -> Vec<u8> {
    let mut bytes = vec![0x1B, 0x40];
    bytes.extend_from_slice(format!("receipt for sale {sale:x}").as_bytes());
    bytes.extend_from_slice(&[0x1D, 0x56, 0x01]);
    bytes
}

fn original(checkout: &Checkout, sale: u128) -> ReceiptArtifact {
    ReceiptArtifact {
        id: id16(&checkout.artifact),
        sale_id: id16(&checkout.sale),
        kind: ArtifactKind::Original,
        source_artifact_id: None,
        format: "escpos".to_owned(),
        template_version: "receipt-v1".to_owned(),
        printer_profile: "epson-tm-t20iii-ethernet-80mm".to_owned(),
        fiscal_version: None,
        content_bytes: bytes(sale),
        generated_at: at(0),
    }
}

/// A sale the schema lets complete, whose original receipt and initial print
/// job were written through `ReceiptRepository` — not by the fixture's SQL —
/// in one transaction, beside the envelope that names the artifact.
fn completed_sale(register: &Register, slot: u8, base: u128, receipt: &str) -> Checkout {
    let checkout = parked_sale(register, slot, base, receipt);
    checkout.complete(&register.conn);
    checkout
}

fn parked_sale(register: &Register, slot: u8, base: u128, receipt: &str) -> Checkout {
    let (sale, line, tender, product) = (id(base), id(base + 1), id(base + 2), id(base + 3));
    let conn = &register.conn;
    conn.execute(
        "INSERT INTO product (id, sku, name, price_minor, currency, is_active, tax_category_id)
         VALUES (?1, ?2, 'Espresso', 2500, 'JOD', 1, ?3)",
        params![
            &product,
            format!("SKU-{base}"),
            &register.chain.tax_category
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                           tax_minor, total_minor, currency, business_date, completed_at)
         VALUES (?1, ?5, ?2, 'parked', 2500, 400, 2900, 'JOD', ?3, ?4)",
        params![
            &sale,
            &id(0x01),
            BUSINESS_DATE,
            registered_chain::AT,
            receipt
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale_line
           (id, sale_id, product_id, qty_milli, unit_price_minor, tax_minor,
            total_minor, line_no, name_snapshot, net_minor, tax_category_id)
         VALUES (?1, ?2, ?3, 1000, 2500, 400, 2900, 1, 'Espresso', 2500, ?4)",
        params![&line, &sale, &product, &register.chain.tax_category],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
         VALUES (?1, ?2, 'cash', 2900, 0)",
        params![&tender, &sale],
    )
    .unwrap();

    let checkout = Checkout::new(slot, &sale, &id(0x01), &[&line], &[&tender]);
    checkout.attach(conn, &register.chain);
    register
        .chain
        .write_envelope(conn, &checkout.commit, &checkout.members());
    checkout.write_line_taxes(conn);
    let tx = conn.unchecked_transaction().unwrap();
    let receipts = ReceiptRepository::new(conn);
    receipts
        .append_artifact(&tx, &original(&checkout, base))
        .unwrap();
    receipts
        .queue_initial(
            &tx,
            &id16(&checkout.print_job),
            &id16(&checkout.artifact),
            at(0),
        )
        .unwrap();
    tx.commit().unwrap();
    checkout.write_audit(conn, &register.chain);
    checkout.write_tender_events(conn);
    checkout
}

fn lease(seconds: i64) -> Lease<'static> {
    Lease {
        owner: WORKER,
        at: at(seconds),
        until: at(seconds + 60),
    }
}

fn attempt(job: &Id, attempt_no: u32, outcome: AttemptOutcome, seconds: i64) -> PrintAttempt {
    PrintAttempt {
        id: id16(&id(0xA000
            + u128::from(attempt_no)
            + u128::from(job[15]) * 0x100)),
        job_id: *job,
        attempt_no,
        outcome,
        error_code: match outcome {
            AttemptOutcome::Failed => Some("paper_out".to_owned()),
            AttemptOutcome::Unknown => Some("device_silent".to_owned()),
            AttemptOutcome::Partial => Some("connection_reset".to_owned()),
            _ => None,
        },
        started_at: at(seconds),
        finished_at: Some(at(seconds + 1)),
        sent_at: (outcome == AttemptOutcome::Printed).then(|| at(seconds + 1)),
        retry_at: (outcome == AttemptOutcome::Failed).then(|| at(seconds + 30)),
    }
}

/// Lease and append in one transaction, as a worker does around one print.
fn lease_one(conn: &Connection, seconds: i64) -> Option<pos_db::repo::receipt::PrintJob> {
    let tx = conn.unchecked_transaction().unwrap();
    let job = ReceiptRepository::new(conn)
        .lease_next_job(&tx, lease(seconds))
        .unwrap();
    tx.commit().unwrap();
    job
}

fn append(conn: &Connection, attempt: &PrintAttempt) -> Result<(), pos_db::DbError> {
    let tx = conn.unchecked_transaction().unwrap();
    let result = ReceiptRepository::new(conn).append_attempt(&tx, attempt);
    if result.is_ok() {
        tx.commit().unwrap();
    }
    result
}

fn sale_row(conn: &Connection, sale: &[u8]) -> Vec<String> {
    conn.query_row(
        "SELECT status, total_minor, sync_commit_id, completed_at, receipt_number
           FROM sale WHERE id = ?1",
        [sale],
        |row| {
            Ok(vec![
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.to_string(),
                format!("{:?}", row.get::<_, Vec<u8>>(2)?),
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ])
        },
    )
    .unwrap()
}

/// A finalized sale has exactly one original artifact and its one initial
/// job, queued and never attempted — written here by the repository, and the
/// schema's durable-outputs gate accepts nothing less.
#[test]
fn a_sale_completes_with_its_original_and_initial_job_written_by_the_repository() {
    let r = register();
    let checkout = completed_sale(&r, 1, 0x10, "000123");
    let receipts = ReceiptRepository::new(&r.conn);

    let stored = receipts
        .artifact(&id16(&checkout.artifact))
        .unwrap()
        .expect("stored");
    assert_eq!(stored.artifact, original(&checkout, 0x10));
    assert_eq!(stored.content_hash, *blake3::hash(&bytes(0x10)).as_bytes());

    let job = receipts
        .job(&id16(&checkout.print_job))
        .unwrap()
        .expect("queued");
    assert_eq!((job.state, job.attempts), (JobState::Queued, 0));
    assert_eq!(job.artifact_id, id16(&checkout.artifact));
    assert_eq!(receipts.unprinted().unwrap(), [job]);
}

/// Due jobs are leased in the order they were queued — by the queue's own
/// order, not a clock — and each once.
#[test]
fn the_queue_leases_due_jobs_in_the_order_they_were_queued() {
    let r = register();
    let first = completed_sale(&r, 1, 0x10, "000123");
    let second = completed_sale(&r, 2, 0x20, "000124");

    let leased = lease_one(&r.conn, 5).expect("a job is due");
    assert_eq!(leased.id, id16(&first.print_job));
    assert_eq!(leased.state, JobState::Printing);
    assert_eq!(leased.lease_owner.as_deref(), Some(WORKER));
    assert_eq!(
        (leased.claimed_at, leased.lease_expires_at),
        (Some(at(5)), Some(at(65)))
    );

    assert_eq!(
        lease_one(&r.conn, 6).map(|job| job.id),
        Some(id16(&second.print_job))
    );
    assert_eq!(lease_one(&r.conn, 7), None, "both are claimed");
}

/// Only a failure is retried, and only when its retry is due (§3 rule 4).
#[test]
fn a_failed_job_is_leased_again_only_when_its_retry_is_due() {
    let r = register();
    let checkout = completed_sale(&r, 1, 0x10, "000123");
    let job = id16(&checkout.print_job);

    lease_one(&r.conn, 0).expect("queued");
    append(&r.conn, &attempt(&job, 1, AttemptOutcome::Failed, 0)).unwrap();
    let failed = ReceiptRepository::new(&r.conn).job(&job).unwrap().unwrap();
    assert_eq!((failed.state, failed.attempts), (JobState::Failed, 1));
    assert_eq!(failed.next_attempt_at, Some(at(30)));
    assert_eq!(failed.last_error.as_deref(), Some("paper_out"));

    assert_eq!(lease_one(&r.conn, 29), None, "not due until its retry time");
    let again = lease_one(&r.conn, 30).expect("due at its retry time");
    assert_eq!((again.id, again.attempts), (job, 1));
    append(&r.conn, &attempt(&job, 2, AttemptOutcome::Printed, 30)).unwrap();
    assert!(
        ReceiptRepository::new(&r.conn)
            .unprinted()
            .unwrap()
            .is_empty()
    );
}

/// An `Unknown` or `Partial` outcome may be paper in a customer's hand. The
/// queue never offers that job again, however much time passes; only a
/// person's answer closes it (§3 rules 4 and 5).
#[test]
fn an_unknown_or_partial_outcome_is_never_leased_again() {
    for (slot, outcome) in [(1, AttemptOutcome::Unknown), (2, AttemptOutcome::Partial)] {
        let r = register();
        let checkout = completed_sale(&r, slot, 0x10, "000123");
        let job = id16(&checkout.print_job);
        lease_one(&r.conn, 0).expect("queued");
        append(&r.conn, &attempt(&job, 1, outcome, 0)).unwrap();

        let receipts = ReceiptRepository::new(&r.conn);
        let stored = receipts.job(&job).unwrap().unwrap();
        assert_eq!(stored.state, JobState::Unknown, "{outcome:?}");
        assert_eq!(
            lease_one(&r.conn, 86_400 * 365),
            None,
            "{outcome:?} leased again"
        );
        assert_eq!(
            receipts.awaiting_an_answer().unwrap(),
            core::slice::from_ref(&stored)
        );
        assert_eq!(receipts.unprinted().unwrap(), [stored]);

        // A person's answer — a readable receipt did come out — closes it.
        let mut closed = attempt(&job, 2, AttemptOutcome::Cancelled, 100);
        closed.finished_at = Some(at(100));
        append(&r.conn, &closed).unwrap();
        assert!(receipts.awaiting_an_answer().unwrap().is_empty());
    }
}

/// A worker that died holding a job may have sent the bytes. Its lapsed lease
/// is resolved as `unknown`; the job is never put back in the queue, and the
/// schema refuses anyone who tries.
#[test]
fn an_expired_lease_is_resolved_as_unknown_never_requeued() {
    let r = register();
    let checkout = completed_sale(&r, 1, 0x10, "000123");
    let job = id16(&checkout.print_job);
    lease_one(&r.conn, 0).expect("queued");
    let receipts = ReceiptRepository::new(&r.conn);

    assert!(
        receipts.expired_leases(at(60)).unwrap().is_empty(),
        "lapses after 60 s"
    );
    let lapsed = receipts.expired_leases(at(61)).unwrap();
    assert_eq!(lapsed.iter().map(|j| j.id).collect::<Vec<_>>(), [job]);

    let requeue = r.conn.execute(
        "UPDATE print_job SET state = 'queued', claimed_at = NULL, lease_owner = NULL,
                lease_expires_at = NULL WHERE id = ?1",
        [&job],
    );
    assert!(
        requeue.is_err(),
        "a printing job cannot be put back in the queue"
    );

    let mut lapse = attempt(&job, 1, AttemptOutcome::Unknown, 61);
    lapse.error_code = Some("lease_expired".to_owned());
    append(&r.conn, &lapse).unwrap();
    assert_eq!(
        receipts.job(&job).unwrap().unwrap().state,
        JobState::Unknown
    );
    assert_eq!(lease_one(&r.conn, 120), None);
    assert!(receipts.expired_leases(at(120)).unwrap().is_empty());
}

/// Print state lives on the job; a completed sale is never touched by any
/// outcome — printed, failed or unknown (§3 rules 2 and 3, I-4).
#[test]
fn a_print_outcome_never_touches_the_completed_sale() {
    for outcome in [
        AttemptOutcome::Printed,
        AttemptOutcome::Failed,
        AttemptOutcome::Unknown,
    ] {
        let r = register();
        let checkout = completed_sale(&r, 1, 0x10, "000123");
        let before = sale_row(&r.conn, &checkout.sale);
        let job = id16(&checkout.print_job);
        lease_one(&r.conn, 0).expect("queued");
        append(&r.conn, &attempt(&job, 1, outcome, 0)).unwrap();
        assert_eq!(sale_row(&r.conn, &checkout.sale), before, "{outcome:?}");
        assert_eq!(before.first().map(String::as_str), Some("completed"));
    }
}

/// The bytes, the job's state and every attempt are what the database holds:
/// reopening the file changes none of them.
#[test]
fn the_queue_and_its_history_survive_reopening_the_database() {
    let r = register();
    let checkout = completed_sale(&r, 1, 0x10, "000123");
    let job = id16(&checkout.print_job);
    lease_one(&r.conn, 0).expect("queued");
    append(&r.conn, &attempt(&job, 1, AttemptOutcome::Failed, 0)).unwrap();
    lease_one(&r.conn, 30).expect("due");
    append(&r.conn, &attempt(&job, 2, AttemptOutcome::Unknown, 30)).unwrap();

    let read = |conn: &Connection| {
        let receipts = ReceiptRepository::new(conn);
        (
            receipts.artifact(&id16(&checkout.artifact)).unwrap(),
            receipts.job(&job).unwrap(),
            receipts.attempts(&job).unwrap(),
        )
    };
    let before = read(&r.conn);
    let Register { dir, conn, .. } = r;
    drop(conn);
    let reopened = pos_db::open(&dir.path().join(FILE), KEY).unwrap();
    let after = read(&reopened);
    assert_eq!(after, before);
    let (_, job_after, attempts) = after;
    assert_eq!(
        job_after.map(|j| (j.state, j.attempts)),
        Some((JobState::Unknown, 2))
    );
    assert_eq!(
        attempts.iter().map(|a| a.outcome).collect::<Vec<_>>(),
        [AttemptOutcome::Failed, AttemptOutcome::Unknown]
    );
}

/// A duplicate is a new artifact citing this sale's original, with its own
/// job, in its own envelope (§3 rule 5, I-9). The schema refuses one that
/// cites another sale's, and an original that cites anything.
#[test]
fn a_duplicate_cites_this_sales_original_and_gets_its_own_job() {
    let r = register();
    let first = completed_sale(&r, 1, 0x10, "000123");
    let second = completed_sale(&r, 2, 0x20, "000124");
    let receipts = ReceiptRepository::new(&r.conn);
    let duplicate = |of: &Checkout, cites: &Checkout, value: u128| ReceiptArtifact {
        id: id16(&id(value)),
        kind: ArtifactKind::Duplicate,
        source_artifact_id: Some(id16(&cites.artifact)),
        content_bytes: bytes(value),
        generated_at: at(200),
        ..original(of, 0x10)
    };

    // In its own envelope: the duplicate is a fact of its own.
    let copy = duplicate(&first, &first, 0xD1);
    let tx = r.conn.unchecked_transaction().unwrap();
    // Envelope, artifact and job in one transaction (I-9): the fixture's own
    // `write_envelope` opens a transaction of its own, so the writer is called
    // here directly, as a caller in production does.
    let member = Member::new(&id(0xC2), "receipt_artifact", &id(0xD1));
    OutboxRepository::new(&r.conn)
        .write_commit(
            &tx,
            &CommitEnvelope {
                commit_id: &id16(&id(0xC1)),
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "pos-db-receipt-test",
                created_at: registered_chain::AT,
            },
            &[member.as_fact()],
        )
        .unwrap();
    receipts.append_artifact(&tx, &copy).unwrap();
    receipts
        .queue_initial(&tx, &id16(&id(0xE1)), &copy.id, at(200))
        .unwrap();
    tx.commit().unwrap();
    let stored = receipts.artifact(&copy.id).unwrap().unwrap().artifact;
    assert_eq!(
        (stored.kind, stored.source_artifact_id),
        (ArtifactKind::Duplicate, Some(id16(&first.artifact)))
    );
    assert_eq!(
        receipts.job(&id16(&id(0xE1))).unwrap().map(|j| j.state),
        Some(JobState::Queued)
    );

    let tx = r.conn.unchecked_transaction().unwrap();
    let foreign = receipts.append_artifact(&tx, &duplicate(&first, &second, 0xD2));
    assert_eq!(
        message(foreign.unwrap_err()),
        "a duplicate or fiscal supplement must cite this sale original artifact"
    );
    let mut sourced = original(&first, 0xD3);
    sourced.id = id16(&id(0xD3));
    sourced.source_artifact_id = Some(id16(&first.artifact));
    assert!(
        receipts.append_artifact(&tx, &sourced).is_err(),
        "an original cites nothing"
    );
}

#[test]
fn a_lease_must_expire_after_it_is_taken() {
    let r = register();
    completed_sale(&r, 1, 0x10, "000123");
    let tx = r.conn.unchecked_transaction().unwrap();
    let refused = ReceiptRepository::new(&r.conn).lease_next_job(
        &tx,
        Lease {
            owner: WORKER,
            at: at(10),
            until: at(10),
        },
    );
    assert!(matches!(refused, Err(pos_db::DbError::PrintLeaseInvalid)));
}

/// Receipt bytes can carry a buyer's name and tax number, so `Debug` shows
/// how many there are and nothing else.
#[test]
fn an_artifacts_debug_never_shows_its_bytes() {
    let r = register();
    let checkout = parked_sale(&r, 1, 0x10, "000123");
    let mut artifact = original(&checkout, 0x10);
    artifact.content_bytes = b"BUYER: Leila Haddad, TIN 987654321".to_vec();
    let shown = format!("{artifact:?}");
    assert!(shown.contains("<redacted, 34 bytes>"), "{shown}");
    assert!(
        !shown.contains("Leila") && !shown.contains("987654321"),
        "{shown}"
    );
}
