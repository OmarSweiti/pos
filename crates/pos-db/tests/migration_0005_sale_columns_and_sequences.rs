//! Microstep 1.9.1 — migration `0005`, against the registered chain.
//!
//! Every test here opens through [`pos_db::open`], so it runs the exact
//! `MIGRATIONS` array the application compiles with. `ref/schema.md` is not
//! replayed on top: `0005` is *shipped*, and a green run against the reference
//! document would not be evidence that a register carries any of it.
//!
//! `0005` is the largest migration in Phase 1 and most of it is scaffolding for
//! later microsteps. What is proved here is what `0005` itself decides:
//!
//! * one open shift per register, held by a partial unique index rather than by
//!   a repository nobody has written yet;
//! * a completed sale must name the open shift for its own register, store and
//!   business date;
//! * every tender of a completed sale must carry an initial status event, so
//!   settlement is an appended fact and never an `UPDATE`;
//! * the five remaining gates in front of `status = 'completed'` — the policy
//!   snapshot, the evidenced fiscal decision, the per-line tax components, the
//!   discount recap and the durable outputs — each refuse a completion that
//!   withholds exactly one of their preconditions;
//! * the completed-sale immutability guards `0002` and `0003` built are still
//!   standing after fourteen `ALTER TABLE`s and sixty new triggers;
//! * and the `exchange` tender seed carries the `is_internal` contract that
//!   keeps an exchange out of expected drawer cash.
//!
//! `1.9.1` shipped negative tests for two of the seven completion gates. The
//! other five were unproved until issue #179: each could have been dropped by a
//! later migration and the whole suite would have stayed green. Because `0005`
//! is committed and migrations are forward-only, a gate that is never exercised
//! is a gate nobody would discover had stopped working.
//!
//! Every one of the five is written the same way, and the shape is the point:
//! take the fixture that completes, withhold exactly one precondition, and
//! assert the **exact** refusal message with `assert_eq!`. Where a withholding
//! could not be isolated — because a second gate refuses the same statement and
//! SQLite does not document which trigger fires first — the test says so in a
//! comment instead of asserting something ambiguous.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::{Connection, params};
use uuid::Uuid;

#[path = "common/registered_chain.rs"]
mod registered_chain;

use registered_chain::{AT, Checkout, Member, RegisteredChain};

const KEY: &str = "test-key";
const BUSINESS_DATE: &str = "2026-08-25";

fn id(value: u128) -> Vec<u8> {
    Uuid::from_u128(value).as_bytes().to_vec()
}

fn message(error: rusqlite::Error) -> String {
    match error {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message,
        other => panic!("expected a trigger refusal, found {other:?}"),
    }
}

struct Register {
    _dir: tempfile::TempDir,
    conn: Connection,
    chain: RegisteredChain,
}

fn register(name: &str) -> Register {
    let dir = tempfile::tempdir().unwrap();
    let conn = pos_db::open(&dir.path().join(name), KEY).unwrap();
    let chain = RegisteredChain::seed(&conn);
    chain.add_register(&conn, &id(0x01), "REG01");
    Register {
        _dir: dir,
        conn,
        chain,
    }
}

/// A sale on `REG01` with one line and one tender, and everything `0005`
/// requires of a completed one *except* the completion itself: the store, the
/// open shift, the policy snapshot, the tax component, the receipt and its
/// queued job, the delivery envelope, the audit row and the tender event.
///
/// Each test then removes exactly one of those and watches the matching gate
/// fire, which is only meaningful because the untouched fixture completes.
fn parked_sale(register: &Register) -> Checkout {
    parked_sale_in_slot(register, 1, 0x10, "000123")
}

fn parked_sale_in_slot(register: &Register, slot: u8, base: u128, receipt: &str) -> Checkout {
    parked_sale_omitting(register, slot, base, receipt, None)
}

/// The same, with one entity left out of the delivery manifest.
///
/// The envelope stays internally complete — `commit_size` is taken from the
/// members actually written, so `sync_commit_ready` still holds and the facts
/// that refuse to exist without a ready commit can still be written against it.
/// What is missing is a fact of the *sale* that the manifest should have named,
/// which is the only thing `sale_commit_base_complete` is looking for.
fn parked_sale_omitting(
    register: &Register,
    slot: u8,
    base: u128,
    receipt: &str,
    omit: Option<&str>,
) -> Checkout {
    let (sale, line, tender, product) = (id(base), id(base + 1), id(base + 2), id(base + 3));
    register
        .conn
        .execute(
            "INSERT INTO product
               (id, sku, name, price_minor, currency, is_active, tax_category_id)
             VALUES (?1, ?2, 'Espresso', 2500, 'JOD', 1, ?3)",
            params![
                &product,
                format!("SKU-{base}"),
                &register.chain.tax_category
            ],
        )
        .unwrap();
    register
        .conn
        .execute(
            "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                               tax_minor, total_minor, currency, business_date, completed_at)
             VALUES (?1, ?5, ?2, 'parked', 2500, 400, 2900, 'JOD', ?3, ?4)",
            params![&sale, &id(0x01), BUSINESS_DATE, AT, receipt],
        )
        .unwrap();
    register
        .conn
        .execute(
            "INSERT INTO sale_line
               (id, sale_id, product_id, qty_milli, unit_price_minor, tax_minor,
                total_minor, line_no, name_snapshot, net_minor, tax_category_id)
             VALUES (?1, ?2, ?3, 1000, 2500, 400, 2900, 1, 'Espresso', 2500, ?4)",
            params![&line, &sale, &product, &register.chain.tax_category],
        )
        .unwrap();
    register
        .conn
        .execute(
            "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
             VALUES (?1, ?2, 'cash', 2900, 0)",
            params![&tender, &sale],
        )
        .unwrap();

    let checkout = Checkout::new(slot, &sale, &id(0x01), &[&line], &[&tender]);
    checkout.attach(&register.conn, &register.chain);
    let members: Vec<Member> = checkout
        .members()
        .into_iter()
        .filter(|member| Some(member.entity()) != omit)
        .collect();
    register
        .chain
        .write_envelope(&register.conn, &checkout.commit, &members);
    checkout.write_facts_before_envelope(&register.conn);
    checkout.write_audit(&register.conn, &register.chain);
    checkout.write_tender_events(&register.conn);
    checkout
}

/// The one tax component of a single-line checkout.
///
/// `clippy::indexing_slicing` is denied workspace-wide, and rightly: a bare
/// `[0]` in a test panics with a message about a slice rather than about the
/// fixture that failed to write the row.
fn only_line_tax(checkout: &Checkout) -> &[u8] {
    checkout
        .line_taxes
        .first()
        .expect("every checkout in this suite carries exactly one line")
}

#[test]
fn opening_a_second_shift_for_the_register_is_refused() {
    let r = register("second-shift.db");
    r.chain
        .open_shift(&r.conn, &id(0x20), &id(0x01), BUSINESS_DATE);

    // The refusal comes from `idx_shift_one_open`, the partial unique index on
    // the projection — not from a repository, which is why it holds against a
    // register whose repository has not been written yet.
    let refused = r
        .chain
        .try_open_shift(&r.conn, &id(0x21), &id(0x01), BUSINESS_DATE)
        .expect_err("two open shifts are two expected-cash figures for one drawer");
    assert!(
        message(refused).contains("shift_state.register_id"),
        "the second open must be refused by idx_shift_one_open itself"
    );

    // The rule is per register, not per store: a second till on the same store
    // opens its own shift on the same business date.
    r.chain.add_register(&r.conn, &id(0x02), "REG02");
    r.chain
        .open_shift(&r.conn, &id(0x22), &id(0x02), BUSINESS_DATE);
    let open: i64 = r
        .conn
        .query_row(
            "SELECT count(*) FROM shift_state WHERE state = 'open'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(open, 2, "one open shift per register, two registers");

    // The projection is the migration's, not the fixture's: `shift_project_open`
    // writes it, and `shift_state_no_delete` refuses to let anyone unpick it.
    let deleted = r.conn.execute("DELETE FROM shift_state", []).unwrap_err();
    assert_eq!(
        message(deleted),
        "a shift projection is rebuilt, not selectively deleted"
    );
}

#[test]
fn a_completed_sale_requires_an_open_matching_shift() {
    let r = register("matching-shift.db");
    let checkout = parked_sale(&r);

    // A shift on another register, open and otherwise identical. The sale may
    // point at it while it is parked; it may not complete there.
    r.chain.add_register(&r.conn, &id(0x02), "REG02");
    r.chain
        .open_shift(&r.conn, &id(0x30), &id(0x02), BUSINESS_DATE);
    r.conn
        .execute(
            "UPDATE sale SET shift_id = ?1 WHERE id = ?2",
            params![&id(0x30), &checkout.sale],
        )
        .expect("a parked sale is work in progress");

    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("another register's shift is not this register's shift");
    assert_eq!(
        message(refused),
        "a completed sale requires the open shift for its register, store and business date"
    );

    // Nor a shift whose business date is not the sale's: the shift owns the
    // filing day, so a sale on the wrong one would file on the wrong day.
    r.conn
        .execute(
            "UPDATE sale SET shift_id = ?1, business_date = '2026-08-26' WHERE id = ?2",
            params![&checkout.shift, &checkout.sale],
        )
        .unwrap();
    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("a shift's business date is the sale's filing day");
    assert_eq!(
        message(refused),
        "a completed sale requires the open shift for its register, store and business date"
    );

    r.conn
        .execute(
            "UPDATE sale SET business_date = ?1 WHERE id = ?2",
            params![BUSINESS_DATE, &checkout.sale],
        )
        .unwrap();
    checkout.complete(&r.conn);
    let status: String = r
        .conn
        .query_row(
            "SELECT status FROM sale WHERE id = ?1",
            [&checkout.sale],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "completed", "the matching open shift completes it");
}

#[test]
fn every_completed_tender_has_an_initial_status_event() {
    let r = register("tender-events.db");
    let (sale, line, tender, product) = (id(0x40), id(0x41), id(0x42), id(0x43));
    r.conn
        .execute(
            "INSERT INTO product
               (id, sku, name, price_minor, currency, is_active, tax_category_id)
             VALUES (?1, 'SKU-2', 'Latte', 3000, 'JOD', 1, ?2)",
            params![&product, &r.chain.tax_category],
        )
        .unwrap();
    r.conn
        .execute(
            "INSERT INTO sale (id, receipt_number, register_id, status, subtotal_minor,
                               tax_minor, total_minor, currency, business_date, completed_at)
             VALUES (?1, '000124', ?2, 'parked', 3000, 480, 3480, 'JOD', ?3, ?4)",
            params![&sale, &id(0x01), BUSINESS_DATE, AT],
        )
        .unwrap();
    r.conn
        .execute(
            "INSERT INTO sale_line
               (id, sale_id, product_id, qty_milli, unit_price_minor, tax_minor,
                total_minor, line_no, name_snapshot, net_minor, tax_category_id)
             VALUES (?1, ?2, ?3, 1000, 3000, 480, 3480, 1, 'Latte', 3000, ?4)",
            params![&line, &sale, &product, &r.chain.tax_category],
        )
        .unwrap();
    r.conn
        .execute(
            "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
             VALUES (?1, ?2, 'cash', 3480, 0)",
            params![&tender, &sale],
        )
        .unwrap();

    // Everything a completion needs except the tender's own status event.
    let checkout = Checkout::new(2, &sale, &id(0x01), &[&line], &[&tender]);
    checkout.attach(&r.conn, &r.chain);
    r.chain
        .write_envelope(&r.conn, &checkout.commit, &checkout.members());
    checkout.write_facts_before_envelope(&r.conn);
    checkout.write_audit(&r.conn, &r.chain);

    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("a tender with no status event has never been collected from");
    assert_eq!(
        message(refused),
        "every completed-sale tender requires an initial status event"
    );

    checkout.write_tender_events(&r.conn);
    checkout.complete(&r.conn);

    // The projection is the migration's work: `tender_status_project_current`
    // wrote it, and it must agree with the event it came from.
    let (event_no, state): (i64, String) = r
        .conn
        .query_row(
            "SELECT event_no, state FROM tender_status_current WHERE tender_id = ?1",
            [&tender],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((event_no, state.as_str()), (1, "pending"));

    // And settlement stays an appended fact: the event is immutable, and the
    // tender it describes cannot be updated instead.
    let refused = r
        .conn
        .execute(
            "UPDATE tender_status_event SET state = 'collected' WHERE tender_id = ?1",
            [&tender],
        )
        .expect_err("settlement history is append-only");
    assert_eq!(message(refused), "tender settlement is append-only");
}

#[test]
fn a_completed_sale_snapshots_its_store_current_policy() {
    let r = register("policy-snapshot.db");
    let checkout = parked_sale(&r);

    // A sale that names no policy cannot explain its own arithmetic. The
    // rounding rule and the cash-rounding step it was computed under are
    // recoverable from nothing else: `tax_computation_policy` is immutable and
    // versioned precisely so a sale can point at the version it used.
    r.conn
        .execute(
            "UPDATE sale SET tax_computation_policy_id = NULL WHERE id = ?1",
            [&checkout.sale],
        )
        .expect("a parked sale is work in progress");
    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("a completed sale with no policy cannot explain its own rounding");
    assert_eq!(
        message(refused),
        "a sale snapshots the current policy; a refund preserves the original policy"
    );

    // One honest limit on what that assertion proves. For a `doc_type = 'sale'`
    // a NULL policy trips *two* arms of this gate at once — the explicit
    // `IS NULL` arm and the store-comparison arm, because SQLite's
    // `NULL IS NOT <blob>` is true. So the refusal above proves the gate
    // refuses a policy-less completion; it does not prove the `IS NULL` arm
    // carries its own weight. Isolating that arm needs a `doc_type = 'refund'`
    // whose referenced sale is also policy-less, since only then is the
    // store-comparison arm silent — and no sale document can reach it.
    //
    // Presence is not what the gate checks. A second policy — real, approved,
    // referentially valid — is still not the one this sale's store computes
    // under, and the gate compares identity with `store.tax_computation_policy_id`.
    // That is what makes the column evidence rather than decoration.
    let other_policy = id(0x70);
    r.conn
        .execute(
            "INSERT INTO tax_computation_policy
               (id, jurisdiction, policy_version, rounding_rule, cash_round_step_minor,
                cash_round_direction, cash_round_tax_treatment, source_ref, content_hash,
                approved_at)
             VALUES (?1, 'JO', 'fixture-v2', 'half_away_from_zero', 5, 'nearest', 'none',
                     'fixture', ?2, ?3)",
            params![&other_policy, vec![0x0Bu8; 32], AT],
        )
        .unwrap();
    r.conn
        .execute(
            "UPDATE sale SET tax_computation_policy_id = ?1 WHERE id = ?2",
            params![&other_policy, &checkout.sale],
        )
        .unwrap();
    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("an approved policy that is not the store's is still the wrong snapshot");
    assert_eq!(
        message(refused),
        "a sale snapshots the current policy; a refund preserves the original policy"
    );

    // Restored to the store's own, the sale completes — so both refusals were
    // this gate, and not some other precondition the withholding disturbed.
    r.conn
        .execute(
            "UPDATE sale SET tax_computation_policy_id = ?1 WHERE id = ?2",
            params![&r.chain.tax_policy, &checkout.sale],
        )
        .unwrap();
    checkout.complete(&r.conn);
}

#[test]
fn a_live_sale_requires_an_evidenced_fiscal_decision() {
    let r = register("fiscal-decision.db");
    let checkout = parked_sale(&r);

    // The obvious withholding is not available, and finding that out is half
    // the value of this test. `store_fiscal_evidence_consistent_update` (0003)
    // already refuses to let a store leave either evidenced shape by edit, so
    // an assertion written that way would be about 0003's guard and would stay
    // green with 0005's removed.
    let blocked = r
        .conn
        .execute(
            "UPDATE store SET fiscal_profile = 'jordan_jofotara' WHERE id = ?1",
            [&r.chain.store],
        )
        .expect_err("an exempt store may not claim the JoFotara profile");
    assert_eq!(
        message(blocked),
        "fiscal enablement or exemption must match merchant-specific evidence"
    );

    // What 0003 does allow is the third `fiscal_obligation` value, and it is
    // the one a real merchant sits in: registered, awaiting the ISTD paperwork,
    // fiscal output disabled. That store is internally consistent and satisfies
    // neither branch of 0005's gate — which is the point. A sale may be taken
    // on it; a sale may not be *filed* from it.
    r.conn
        .execute(
            "UPDATE store
                SET fiscal_obligation = 'pending_evidence',
                    fiscal_obligation_evidence_ref = NULL
              WHERE id = ?1",
            [&r.chain.store],
        )
        .expect("`pending_evidence` with a disabled profile is a consistent store");
    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("a store still awaiting its evidence cannot file a live sale");
    assert_eq!(
        message(refused),
        "a live sale requires evidenced fiscal obligation or exemption"
    );

    // The gate is keyed on `is_training`, and a training sale is not a fiscal
    // document. The same sale, on the same unevidenced store, completes.
    //
    // `is_training` carries no `CHECK (… IN (0,1))` — issue #179 — so a value
    // of `2` would disable this gate as effectively as `1` does. That hole is
    // closed by a later migration, not asserted here: a test of it would have
    // to be deleted the day it is fixed.
    r.conn
        .execute(
            "UPDATE sale SET is_training = 1 WHERE id = ?1",
            [&checkout.sale],
        )
        .unwrap();
    checkout.complete(&r.conn);

    // And the refusal really was the store's fiscal state: restore it, and a
    // second live sale on the same register completes untouched.
    r.conn
        .execute(
            "UPDATE store
                SET fiscal_obligation = 'exempt',
                    fiscal_obligation_evidence_ref = 'fixture-evidence'
              WHERE id = ?1",
            [&r.chain.store],
        )
        .unwrap();
    let live = parked_sale_in_slot(&r, 2, 0x71, "000124");
    live.complete(&r.conn);
}

#[test]
fn completed_lines_require_exactly_their_applicable_tax_components() {
    let r = register("tax-components.db");

    // 1 · No component at all. The I-4 guards on tax detail bind a *completed*
    //     sale, so deleting the row while the sale is parked is allowed — and
    //     leaves the line with no snapshot of what it was taxed at.
    let bare = parked_sale(&r);
    r.conn
        .execute(
            "DELETE FROM sale_line_tax WHERE id = ?1",
            [only_line_tax(&bare)],
        )
        .expect("tax detail is immutable once completed; this sale is parked");
    let refused = bare
        .try_complete(&r.conn)
        .expect_err("a completed line with no tax component cannot reproduce its own total");
    assert_eq!(
        message(refused),
        "completed lines require exactly the applicable tax component snapshots"
    );

    // 2 · A component that no longer matches the rate it claims to snapshot.
    //     The row is still there and still in the manifest, so the only gate
    //     with anything to say is this one.
    let drifted = parked_sale_in_slot(&r, 2, 0x80, "000124");
    r.conn
        .execute(
            "UPDATE sale_line_tax SET rate_ppm = 100000 WHERE id = ?1",
            [only_line_tax(&drifted)],
        )
        .unwrap();
    let refused = drifted
        .try_complete(&r.conn)
        .expect_err("10% is not the rate the store's approved pack applies to this category");
    assert_eq!(
        message(refused),
        "completed lines require exactly the applicable tax component snapshots"
    );

    // Restored to the pack's rate, it completes: the drift was the only thing
    // wrong with this sale. Case 1's deletion gets the same treatment in
    // `every_completion_gate_is_load_bearing` rather than here, because a
    // deleted component cannot be put back without re-stating the fixture's own
    // INSERT, and a fixture restated in a test is a fixture that can drift.
    //
    // The third arm of this gate — an *extra* component with no rate behind it
    // — is deliberately not asserted here. An unexpected `sale_line_tax` row is
    // also a fact the delivery manifest does not name, so
    // `sale_completed_requires_durable_outputs_update` refuses the same
    // statement, and SQLite does not document which of two eligible triggers
    // fires first. An ambiguous assertion is the failure mode this whole test
    // exists to avoid.
    r.conn
        .execute(
            "UPDATE sale_line_tax SET rate_ppm = 160000 WHERE id = ?1",
            [only_line_tax(&drifted)],
        )
        .unwrap();
    drifted.complete(&r.conn);
}

#[test]
fn a_completed_sale_recaps_exactly_its_line_allowances() {
    let r = register("discount-recap.db");
    let checkout = parked_sale(&r);

    // A document-level discount with no line allowance behind it is a number
    // nothing explains: the receipt would show a deduction the lines do not
    // account for, and a filing built from the detail would disagree with the
    // document it came from.
    r.conn
        .execute(
            "UPDATE sale SET discount_minor = 500 WHERE id = ?1",
            [&checkout.sale],
        )
        .expect("a parked sale is work in progress");
    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("a recap must be the sum of the allowances it recaps");
    assert_eq!(
        message(refused),
        "document discount recap must equal the sum of exact line allowances"
    );

    // The converse — allowances with no recap — is not asserted here, and the
    // reason is structural rather than an omission. `sale_line_discount` is one
    // of the entities `sale_commit_base_complete` requires the manifest to
    // name, so inserting one into a sale whose envelope is already sealed trips
    // the durable-outputs gate as well, and the refusal would no longer name
    // this gate unambiguously. Proving that direction needs a manifest that
    // carries the allowance from the start, which is `1.4.5`'s work.
    r.conn
        .execute(
            "UPDATE sale SET discount_minor = 0 WHERE id = ?1",
            [&checkout.sale],
        )
        .unwrap();
    checkout.complete(&r.conn);
}

#[test]
fn sale_completion_requires_a_queued_original_receipt() {
    let r = register("durable-receipt.db");
    let checkout = parked_sale(&r);

    // A job the print worker has already claimed is not a queued job. This is
    // the one transition `print_job_state_transition_allowed` permits without
    // an appended attempt, so the withholding is a state the register really
    // reaches — not a shape invented to fail.
    r.conn
        .execute(
            "UPDATE print_job
                SET state = 'printing', claimed_at = ?1, lease_owner = 'worker-1',
                    lease_expires_at = ?1, next_attempt_at = NULL, updated_at = ?1
              WHERE id = ?2",
            params![AT, &checkout.print_job],
        )
        .expect("queued -> printing under a lease is the claim transition 0005 allows");
    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("a claimed job is no longer the durable promise of a receipt");
    assert_eq!(
        message(refused),
        "sale completion atomically requires its original receipt job and complete sync commit"
    );

    // And with no job at all. `print_job` carries no delete guard, which is why
    // the gate has to check for the row rather than trust that one was written.
    r.conn
        .execute("DELETE FROM print_job WHERE id = ?1", [&checkout.print_job])
        .unwrap();
    let refused = checkout
        .try_complete(&r.conn)
        .expect_err("an artifact with no job is a receipt nobody has promised to print");
    assert_eq!(
        message(refused),
        "sale completion atomically requires its original receipt job and complete sync commit"
    );

    // Re-queued against the same original artifact, the sale completes.
    r.conn
        .execute(
            "INSERT INTO print_job (id, artifact_id, state, attempts, created_at, updated_at)
             VALUES (?1, ?2, 'queued', 0, ?3, ?3)",
            params![&id(0x72), &checkout.artifact, AT],
        )
        .unwrap();
    checkout.complete(&r.conn);
}

/// The gate that is the entire justification for `outbox.rs` carrying seven
/// members instead of three.
///
/// Five of the seven can be dropped from the manifest and the facts still
/// write, because nothing else requires them to be named; only this gate
/// notices. The remaining two cannot be dropped at all — `audit_log` and
/// `tender_status_event` each refuse their own insert unless they are already a
/// member of a ready commit (`audit_log_has_ready_commit` in 0004,
/// `tender_status_event_is_next` in 0005), so a manifest missing either never
/// gets as far as a completion to refuse. That asymmetry is why this test
/// iterates the five rather than all seven.
#[test]
fn sale_completion_requires_a_manifest_naming_every_fact() {
    let r = register("durable-manifest.db");

    for (slot, base, receipt, omitted) in [
        (1u8, 0x90u128, "000201", "sale"),
        (2, 0xA0, "000202", "sale_line"),
        (3, 0xB0, "000203", "sale_tender"),
        (4, 0xC0, "000204", "sale_line_tax"),
        (5, 0xD0, "000205", "receipt_artifact"),
    ] {
        let checkout = parked_sale_omitting(&r, slot, base, receipt, Some(omitted));

        // The envelope is internally complete — `commit_size` matches the rows
        // written, so `sync_commit_ready` holds and every fact that needs a
        // ready commit was written against it. What is missing is a fact of the
        // sale the manifest should have named.
        let size: i64 = r
            .conn
            .query_row(
                "SELECT commit_size FROM sync_commit WHERE id = ?1",
                [&checkout.commit],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(size, 6, "the envelope is one member short, and consistent");

        let refused = checkout
            .try_complete(&r.conn)
            .expect_err("a sale whose manifest omits one of its own facts cannot complete");
        assert_eq!(
            message(refused),
            "sale completion atomically requires its original receipt job and complete sync commit",
            "omitting `{omitted}` from the manifest must refuse the completion"
        );
    }

    // The seven-member manifest completes. Delete a member from `outbox.rs` and
    // every assertion above stops being a tautology and starts being red.
    let whole = parked_sale_in_slot(&r, 6, 0xE0, "000206");
    let size: i64 = r
        .conn
        .query_row(
            "SELECT commit_size FROM sync_commit WHERE id = ?1",
            [&whole.commit],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(size, 7, "sale, line, tender, tax, event, receipt, audit");
    whole.complete(&r.conn);
}

/// Each of the five gates, proved load-bearing by taking it away.
///
/// A negative test that passes tells you the statement was refused. It does not
/// tell you *which* object refused it. This repository has already shipped three
/// assertions that were refused by a foreign key and by a sibling trigger rather
/// than by the guard they named, and all three stayed green with that guard
/// removed (#180). The only thing that separates the two cases is removing the
/// guard and watching the refusal disappear.
///
/// So each gate is exercised twice against the same withheld precondition: once
/// with the trigger in place, where the refusal must carry that trigger's own
/// message, and once with it dropped, where the identical `UPDATE` must be
/// accepted. If any other object had a view on the statement, the second half
/// would still fail.
///
/// **One half at a time.** Only the `_update` trigger is dropped, never its
/// `_insert` sibling. #178's redaction check removed both halves of a two-part
/// guard at once, and the live half masked the dead one — the same mistake here
/// would let one gate stand in for another.
///
/// `DROP TRIGGER` is an error on a name that is not there, so this also fails
/// the day a later migration removes one of these gates — which is the whole of
/// issue #179's complaint about the durable-outputs gate: it could have been
/// dropped and nothing would have gone red.
#[test]
fn every_completion_gate_is_load_bearing() {
    fn withhold_the_policy(r: &Register, c: &Checkout) {
        r.conn
            .execute(
                "UPDATE sale SET tax_computation_policy_id = NULL WHERE id = ?1",
                [&c.sale],
            )
            .unwrap();
    }
    fn withhold_the_fiscal_decision(r: &Register, c: &Checkout) {
        let _ = c;
        r.conn
            .execute(
                "UPDATE store
                    SET fiscal_obligation = 'pending_evidence',
                        fiscal_obligation_evidence_ref = NULL
                  WHERE id = ?1",
                [&r.chain.store],
            )
            .unwrap();
    }
    fn withhold_the_tax_components(r: &Register, c: &Checkout) {
        r.conn
            .execute(
                "DELETE FROM sale_line_tax WHERE id = ?1",
                [only_line_tax(c)],
            )
            .unwrap();
    }
    fn withhold_the_recap(r: &Register, c: &Checkout) {
        r.conn
            .execute(
                "UPDATE sale SET discount_minor = 500 WHERE id = ?1",
                [&c.sale],
            )
            .unwrap();
    }
    fn withhold_the_queued_receipt(r: &Register, c: &Checkout) {
        r.conn
            .execute(
                "UPDATE print_job
                    SET state = 'printing', claimed_at = ?1, lease_owner = 'worker-1',
                        lease_expires_at = ?1, next_attempt_at = NULL, updated_at = ?1
                  WHERE id = ?2",
                params![AT, &c.print_job],
            )
            .unwrap();
    }

    for (index, (trigger, refusal, withhold)) in [
        (
            "sale_completed_requires_tax_policy_update",
            "a sale snapshots the current policy; a refund preserves the original policy",
            withhold_the_policy as fn(&Register, &Checkout),
        ),
        (
            "sale_completed_requires_fiscal_decision_update",
            "a live sale requires evidenced fiscal obligation or exemption",
            withhold_the_fiscal_decision,
        ),
        (
            "sale_completed_requires_tax_components_update",
            "completed lines require exactly the applicable tax component snapshots",
            withhold_the_tax_components,
        ),
        (
            "sale_completed_discount_recap_update",
            "document discount recap must equal the sum of exact line allowances",
            withhold_the_recap,
        ),
        (
            "sale_completed_requires_durable_outputs_update",
            "sale completion atomically requires its original receipt job and complete sync commit",
            withhold_the_queued_receipt,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let r = register(&format!("load-bearing-{index}.db"));
        let checkout = parked_sale(&r);
        withhold(&r, &checkout);

        let Err(refused) = checkout.try_complete(&r.conn) else {
            panic!("the withholding for `{trigger}` must be refused while the gate stands")
        };
        assert_eq!(
            message(refused),
            refusal,
            "`{trigger}` must be the object that refuses this completion"
        );

        // The name is a literal from the table above, never a value read back
        // out of the database, so there is nothing here for a quoting mistake
        // to reach. SQLite does not parameterise a schema object's name.
        r.conn
            .execute_batch(&format!("DROP TRIGGER {trigger};"))
            .unwrap_or_else(|e| panic!("`{trigger}` must exist to be the guard under test: {e}"));

        let rows = checkout.try_complete(&r.conn).unwrap_or_else(|e| {
            panic!(
                "with `{trigger}` dropped the same UPDATE must be accepted, so that the \
                 refusal above was this gate and not another: {e}"
            )
        });
        assert_eq!(rows, 1, "the completion lands once `{trigger}` is gone");
    }

    // The durable-outputs gate has a second half, and it is withheld at
    // construction rather than afterwards: a member cannot be taken out of a
    // sealed envelope without breaking `sync_commit_ready` for every fact that
    // checked it on the way in. So the manifest half gets its own round of the
    // same proof, against the same trigger.
    let r = register("load-bearing-manifest.db");
    let checkout = parked_sale_omitting(&r, 1, 0x10, "000123", Some("receipt_artifact"));
    let Err(refused) = checkout.try_complete(&r.conn) else {
        panic!("a manifest that omits the receipt must be refused while the gate stands")
    };
    assert_eq!(
        message(refused),
        "sale completion atomically requires its original receipt job and complete sync commit"
    );
    r.conn
        .execute_batch("DROP TRIGGER sale_completed_requires_durable_outputs_update;")
        .unwrap();
    let rows = checkout
        .try_complete(&r.conn)
        .expect("the incomplete manifest was refused by this gate and by nothing else");
    assert_eq!(rows, 1, "the completion lands once the gate is gone");
}

#[test]
fn migration_0005_preserves_completed_sale_guards() {
    let r = register("preserved-guards.db");
    let checkout = parked_sale(&r);
    checkout.complete(&r.conn);

    // Fourteen `ALTER TABLE`s and sixty new triggers later, the eight guards
    // 0002 wrote and 0003 rebuilt are still the ones enforcing I-4.
    for trigger in [
        "sale_no_update_once_completed",
        "sale_no_delete_once_completed",
        "sale_line_no_insert_once_completed",
        "sale_line_no_update_once_completed",
        "sale_line_no_delete_once_completed",
        "sale_tender_no_insert_once_completed",
        "sale_tender_no_update_once_completed",
        "sale_tender_no_delete_once_completed",
    ] {
        let present: bool = r
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger' AND name=?1)",
                [trigger],
                |row| row.get(0),
            )
            .unwrap();
        assert!(present, "0005 dropped `{trigger}`");
    }

    // Present is not the same as working, so each one is exercised.
    assert!(
        r.conn
            .execute(
                "UPDATE sale SET total_minor = 1 WHERE id = ?1",
                [&checkout.sale]
            )
            .is_err(),
        "I-4: a completed sale refuses UPDATE"
    );
    assert!(
        r.conn
            .execute("DELETE FROM sale WHERE id = ?1", [&checkout.sale])
            .is_err(),
        "I-4: a completed sale refuses DELETE"
    );
    assert!(
        r.conn
            .execute(
                "UPDATE sale_line SET total_minor = 1 WHERE sale_id = ?1",
                [&checkout.sale]
            )
            .is_err(),
        "I-4: the lines of a completed sale are frozen"
    );
    let refused = r
        .conn
        .execute(
            "UPDATE sale_tender SET psp_ref = 'PSP-9' WHERE sale_id = ?1",
            [&checkout.sale],
        )
        .expect_err("settlement is an appended event, never a tender UPDATE");
    assert_eq!(
        message(refused),
        "I-4: a tender on a completed sale is immutable — append a status event"
    );

    // 0005's own additions to the completed sale are immutable too.
    r.conn
        .execute(
            "INSERT INTO sale_tax_summary
               (id, sale_id, component_code, treatment, calculation_kind, rate_ppm,
                calculation_order, base_kind, taxable_base_minor, net_minor, tax_minor,
                gross_minor)
             VALUES (?1, ?2, 'GST', 'standard', 'ad_valorem', 160000, 0, 'line_net',
                     2500, 2500, 400, 2900)",
            params![&id(0x50), &checkout.sale],
        )
        .expect_err("a tax summary cannot be added to a sale that is already a fact");
}

#[test]
fn exchange_tender_seed_matches_internal_contract() {
    let r = register("exchange-seed.db");
    let (opens_drawer, allows_change, is_cash_counted, is_internal, refundable_to, is_active): (
        i64,
        i64,
        i64,
        i64,
        String,
        i64,
    ) = r
        .conn
        .query_row(
            "SELECT opens_drawer, allows_change, is_cash_counted, is_internal,
                    refundable_to, is_active
               FROM tender_type WHERE code = 'exchange'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("0005 seeds the complete tender catalogue; later steps never append codes");

    assert_eq!(
        (
            opens_drawer,
            allows_change,
            is_cash_counted,
            is_internal,
            refundable_to.as_str()
        ),
        (0, 0, 0, 1, "none"),
        "domain-api.md 7.1: an exchange only offsets two linked documents"
    );
    assert_eq!(
        is_active, 0,
        "exchange is seeded inactive — it has no Phase-1 effect until a refund \
         path enables it"
    );

    // The contract's whole point: `is_internal` is NOT implied by
    // `is_cash_counted = 0`. Card and CliQ count no drawer cash and still move
    // real value through a PSP, so they are external.
    let external_but_uncounted: i64 = r
        .conn
        .query_row(
            "SELECT count(*) FROM tender_type
              WHERE is_cash_counted = 0 AND is_internal = 0 AND code IN ('card','cliq')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        external_but_uncounted, 2,
        "card and cliq are uncounted and external; reading one flag off the \
         other is the mistake this row exists to refuse"
    );
}

#[test]
fn an_exchange_tender_never_opens_or_counts_the_drawer() {
    let r = register("exchange-drawer.db");

    // Stated as a rule over the catalogue, not as one row: a tender that moves
    // no value outside the two documents it settles must never reach the
    // drawer. If it did, the offset would appear in expected cash on both
    // documents and the shift would close short by twice the exchanged value.
    let offending: Vec<String> = r
        .conn
        .prepare(
            "SELECT code FROM tender_type
              WHERE is_internal = 1 AND (opens_drawer <> 0 OR is_cash_counted <> 0
                                      OR allows_change <> 0)",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        offending.is_empty(),
        "internal tenders that touch the drawer: {offending:?}"
    );

    let internal: Vec<String> = r
        .conn
        .prepare("SELECT code FROM tender_type WHERE is_internal = 1 ORDER BY sort_order")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        internal,
        vec!["exchange".to_owned()],
        "exchange is the only internal tender 0005 seeds; a second one would \
         need its own drawer reasoning"
    );

    let seeded: i64 = r
        .conn
        .query_row("SELECT count(*) FROM tender_type", [], |row| row.get(0))
        .unwrap();
    assert_eq!(seeded, 6, "0005 seeds the catalogue complete, once");
}

/// Not one of the six `1.9.1` names; it holds the property those six lean on.
///
/// Every microstep after this one rings up more than one sale per register —
/// `1.9.2`'s gap-free counter and `1.9.5`'s shift lifecycle both do — and both
/// the shift and the delivery manifest are shared, keyed structures where a
/// second sale is where a collision would first appear. One open shift serves
/// both sales, and nothing else about them is shared.
#[test]
fn two_sales_on_one_register_share_its_shift_and_not_their_commits() {
    let r = register("two-sales.db");
    let first = parked_sale_in_slot(&r, 1, 0x10, "000123");
    first.complete(&r.conn);
    let second = parked_sale_in_slot(&r, 2, 0x60, "000124");
    second.complete(&r.conn);

    let (open, shifts): (i64, i64) = r
        .conn
        .query_row(
            "SELECT (SELECT count(*) FROM shift_state WHERE state = 'open'),
                    (SELECT count(*) FROM shift)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (open, shifts),
        (1, 1),
        "the second sale joins the open shift rather than opening a second one"
    );
    let shared: i64 = r
        .conn
        .query_row(
            "SELECT count(DISTINCT shift_id) FROM sale WHERE status = 'completed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(shared, 1, "both completed sales name the same shift");

    // Two commits, fourteen members, fourteen delivery rows — and no id reused
    // between them. `fact_commit_member` has a primary key on `change_id` and a
    // UNIQUE on `(entity, entity_id)`, so a collision would have refused the
    // write above; this states the property the refusal would have been about.
    for (what, sql, expected) in [
        (
            "commits",
            "SELECT count(DISTINCT id) FROM sync_commit WHERE id IN (?1, ?2)",
            2,
        ),
        (
            "members",
            "SELECT count(DISTINCT change_id) FROM fact_commit_member
              WHERE commit_id IN (?1, ?2)",
            14,
        ),
        (
            "facts",
            "SELECT count(*) FROM (SELECT DISTINCT entity, entity_id FROM fact_commit_member
                                    WHERE commit_id IN (?1, ?2))",
            14,
        ),
    ] {
        let counted: i64 = r
            .conn
            .query_row(sql, params![&first.commit, &second.commit], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(counted, expected, "distinct {what} across the two sales");
    }
}
