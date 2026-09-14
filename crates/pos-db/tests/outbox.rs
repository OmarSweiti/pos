//! Microstep 1.8.9 — the outbox writer, against the registered chain.
//!
//! Every test here opens the database through `pos_db::open`, so it runs the
//! exact `MIGRATIONS` array the application compiles with. It deliberately does
//! not use `tests/common/mod.rs`: that helper applies the *reference* SQL from
//! `ref/schema.md` on top of the shipped chain, and a green run against
//! reference SQL is not evidence that a register carries the guarantee — which
//! is exactly how a previous microstep looked delivered when it was not
//! (phase-1 §1.2.1, "Delivery-history correction").
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/registered_chain.rs"]
mod registered_chain;

/// The module name is load-bearing. Microstep 1.8.9's Done-when command is
/// `cargo nextest run -p pos-db outbox::`, and nextest matches that filter
/// against the test's own name — not against the binary id, which is
/// `pos-db::outbox`. Without the module, the filter selects nothing and the
/// Done-when command passes by running no tests at all.
mod outbox {
    use std::collections::BTreeSet;

    use pos_db::repo::outbox::{CommitEnvelope, CommitReceipt, FactMember, OutboxRepository};
    use rusqlite::{Connection, Transaction, params};
    use uuid::Uuid;

    use crate::registered_chain::{Checkout, Member, RegisteredChain};

    const KEY: &str = "test-key";
    /// Time is an argument, here as in the writer (I-8). One fixed instant, so
    /// nothing in this file can depend on when it ran.
    const AT: &str = "2026-08-27T09:30:00.000Z";
    const BUSINESS_DATE: &str = "2026-08-27";
    const PRODUCER_VERSION: &str = "pos-db-1.8.9-test";
    const PROTOCOL_VERSION: i64 = 1;
    /// A sale, its line, its tender, the line's tax component, the tender's
    /// initial status event, the original receipt artifact and the audit row:
    /// the smallest fact graph a checkout can produce, and the one the server
    /// must accept or reject whole.
    ///
    /// It was three until migration 0005. `sale_commit_base_complete` now
    /// refuses to let a sale complete unless its commit names every one of
    /// these, so "the smallest graph" grew by four — the envelope did not
    /// change, the definition of a whole sale did.
    const COMMIT_SIZE: i64 = 7;
    /// How many of those the checkout itself writes and hand-authors payloads
    /// for; the rest come from the shared registered-chain fixture.
    const OWN_MEMBERS: usize = 3;
    /// Opening the shift a sale is rung up on is its own business transaction,
    /// with its own one-member envelope, written before the checkout opens.
    /// Whole-database counts see it; commit-scoped ones do not.
    const SHIFT_ENVELOPE_MEMBERS: i64 = 1;

    struct Register {
        _dir: tempfile::TempDir,
        conn: Connection,
    }

    impl Register {
        fn open() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let conn = pos_db::open(&dir.path().join("register.db"), KEY).unwrap();
            Self { _dir: dir, conn }
        }

        fn scalar(&self, sql: &str) -> i64 {
            self.conn.query_row(sql, [], |row| row.get(0)).unwrap()
        }

        /// A count scoped to one commit. The database also holds the envelope
        /// of the shift this sale was rung up on, so an unscoped count would
        /// be answering a different question from the one being asked.
        fn scalar_for(&self, sql: &str, commit: &Uuid) -> i64 {
            self.conn
                .query_row(sql, [commit.as_bytes().as_slice()], |row| row.get(0))
                .unwrap()
        }
    }

    /// One checkout's identities and canonical payloads.
    ///
    /// The payloads are written by hand here, sorted-key and whitespace-free,
    /// because the shared canonical projection (`crates/pos-sync/src/canonical.rs`)
    /// arrives with the sync engine. UUIDs appear in them as lower-case
    /// hyphenated text, as `ref/sync-protocol.md` §"The canonical dump"
    /// requires.
    struct Sale {
        commit: Uuid,
        sale: Uuid,
        line: Uuid,
        tender: Uuid,
        product: Uuid,
        tax_category: Uuid,
        register: Uuid,
        shift: Uuid,
        changes: [Uuid; OWN_MEMBERS],
        payloads: [String; OWN_MEMBERS],
        /// The four facts 0005 added to a whole sale. Their ids and rows belong
        /// to the shared fixture; only their membership of *this* envelope is
        /// this suite's business.
        checkout: Checkout,
        auxiliary: Vec<Member>,
    }

    impl Sale {
        fn new() -> Self {
            let commit = Uuid::from_u128(0x10);
            let sale = Uuid::from_u128(0x11);
            let line = Uuid::from_u128(0x12);
            let tender = Uuid::from_u128(0x13);
            let product = Uuid::from_u128(0x14);
            let tax_category = Uuid::from_u128(0x15);
            let register = Uuid::from_u128(0x16);
            let shift = Uuid::from_u128(0x17);
            let changes = [
                Uuid::from_u128(0x21),
                Uuid::from_u128(0x22),
                Uuid::from_u128(0x23),
            ];
            // Sorted keys, no whitespace, UTF-8 — one line each, because a
            // canonical payload has no line breaks to wrap at.
            let payloads = [
                format!(
                    r#"{{"business_date":"{BUSINESS_DATE}","completed_at":"{AT}","currency":"JOD","id":"{sale}","receipt_number":"000123","register_id":"{register}","status":"completed","subtotal_minor":2500,"tax_minor":400,"total_minor":2900}}"#
                ),
                format!(
                    r#"{{"discount_minor":0,"id":"{line}","line_no":1,"name_snapshot":"Espresso","net_minor":2500,"product_id":"{product}","qty_milli":1000,"qty_step_milli":1000,"sale_id":"{sale}","tax_minor":400,"total_minor":2900,"unit_price_minor":2500}}"#
                ),
                format!(
                    r#"{{"amount_minor":2900,"change_minor":0,"id":"{tender}","method":"cash","sale_id":"{sale}"}}"#
                ),
            ];
            let checkout = Checkout::new(
                1,
                sale.as_bytes(),
                register.as_bytes(),
                &[line.as_bytes()],
                &[tender.as_bytes()],
            )
            .with_commit(commit.as_bytes());
            let auxiliary = checkout.auxiliary_members(OWN_MEMBERS);

            Self {
                commit,
                sale,
                line,
                tender,
                product,
                tax_category,
                register,
                shift,
                changes,
                payloads,
                checkout,
                auxiliary,
            }
        }

        fn envelope(&self) -> CommitEnvelope<'_> {
            CommitEnvelope {
                commit_id: self.commit.as_bytes(),
                protocol_version: PROTOCOL_VERSION,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: PRODUCER_VERSION,
                created_at: AT,
            }
        }

        /// Parents before children, which is the order the server applies them
        /// in (`ref/sync-protocol.md` §2, rule 1) and therefore the order the
        /// writer records as `commit_index`.
        fn members(&self) -> Vec<FactMember<'_>> {
            let [sale_change, line_change, tender_change] = &self.changes;
            let [sale_payload, line_payload, tender_payload] = &self.payloads;
            vec![
                FactMember {
                    change_id: sale_change.as_bytes(),
                    entity: "sale",
                    entity_id: self.sale.as_bytes(),
                    payload: sale_payload,
                },
                FactMember {
                    change_id: line_change.as_bytes(),
                    entity: "sale_line",
                    entity_id: self.line.as_bytes(),
                    payload: line_payload,
                },
                FactMember {
                    change_id: tender_change.as_bytes(),
                    entity: "sale_tender",
                    entity_id: self.tender.as_bytes(),
                    payload: tender_payload,
                },
            ]
            .into_iter()
            .chain(self.auxiliary.iter().map(Member::as_fact))
            .collect()
        }
    }

    /// The `(entity, entity_id)` pairs the manifest must contain, read from the
    /// fact tables themselves rather than restated. Seven tables since 0005,
    /// which is what `sale_commit_base_complete` means by a whole sale.
    fn sale_facts_in_the_database(conn: &Connection) -> BTreeSet<(String, Vec<u8>)> {
        let mut facts = BTreeSet::new();
        for (entity, sql) in [
            ("sale", "SELECT id FROM sale"),
            ("sale_line", "SELECT id FROM sale_line"),
            ("sale_tender", "SELECT id FROM sale_tender"),
            ("sale_line_tax", "SELECT id FROM sale_line_tax"),
            ("tender_status_event", "SELECT id FROM tender_status_event"),
            ("receipt_artifact", "SELECT id FROM receipt_artifact"),
            // Only this sale's audit row: the shift the fixture opened before
            // the transaction has none, but a future fixture that audited it
            // would otherwise make this suite red for the wrong reason.
            (
                "audit_log",
                "SELECT id FROM audit_log WHERE entity = 'sale'",
            ),
        ] {
            let mut statement = conn.prepare(sql).unwrap();
            let ids = statement
                .query_map([], |row| row.get::<_, Vec<u8>>(0))
                .unwrap();
            for id in ids {
                facts.insert((entity.to_owned(), id.unwrap()));
            }
        }
        facts
    }

    /// The catalogue rows a sale line points at, and the registered chain the
    /// sale hangs off: org, store, approved tax pack and policy, the register,
    /// and an open shift.
    ///
    /// All of it is reference data or a *prior* business transaction, written
    /// before this one opens. Opening a shift inside the checkout would need a
    /// nested `BEGIN`, which SQLite does not have — and would be a lie about
    /// the boundary besides.
    fn seed_catalog(conn: &Connection, sale: &Sale) -> RegisteredChain {
        conn.execute(
            "INSERT INTO tax_category (id, code, name_ar, treatment)
             VALUES (?1, 'STD16', 'ضريبة عامة', 'standard')",
            [sale.tax_category.as_bytes().as_slice()],
        )
        .unwrap();
        // The chain's rate pack is built around this suite's own category, so
        // the line's tax component matches a rate the store actually applies —
        // which is what 0005 checks before it lets the sale complete.
        let chain = RegisteredChain::seed_for_category(conn, sale.tax_category.as_bytes());
        chain.add_register(conn, sale.register.as_bytes(), "REG01");
        chain.open_shift(
            conn,
            sale.shift.as_bytes(),
            sale.register.as_bytes(),
            BUSINESS_DATE,
        );
        conn.execute(
            "INSERT INTO product
               (id, sku, name, name_ar, price_minor, currency, is_active,
                tax_category_id, unit, qty_step_milli, is_weighed)
             VALUES (?1, 'SKU-1', 'Espresso', 'إسبريسو', 2500, 'JOD', 1, ?2,
                     'each', 1000, 0)",
            params![
                sale.product.as_bytes().as_slice(),
                sale.tax_category.as_bytes().as_slice(),
            ],
        )
        .unwrap();
        chain
    }

    /// The fact graph a checkout writes before its envelope: header, line,
    /// tender, the line's tax component, and the original receipt with its
    /// queued print job.
    ///
    /// A sale inserted as `completed` could never take a line — I-4's insert
    /// guard refuses one — and since 0005 could not name its own receipt
    /// either, because that artifact carries `REFERENCES sale(id)`. So it is
    /// born parked, and the transition that seals it comes last, after the
    /// envelope: `sale_completed_requires_durable_outputs_update` refuses a
    /// completion whose commit is not yet whole.
    fn write_sale_facts(tx: &Transaction<'_>, sale: &Sale, chain: &RegisteredChain) {
        tx.execute(
            "INSERT INTO sale
               (id, receipt_number, register_id, status, subtotal_minor,
                tax_minor, total_minor, currency, business_date, completed_at,
                store_id, shift_id, tax_computation_policy_id)
             VALUES (?1, '000123', ?2, 'parked', 2500, 400, 2900, 'JOD', ?3, ?4,
                     ?5, ?6, ?7)",
            params![
                sale.sale.as_bytes().as_slice(),
                sale.register.as_bytes().as_slice(),
                BUSINESS_DATE,
                AT,
                &chain.store,
                sale.shift.as_bytes().as_slice(),
                &chain.tax_policy,
            ],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO sale_line
               (id, sale_id, product_id, qty_milli, qty_step_milli,
                unit_price_minor, discount_minor, tax_minor, total_minor,
                line_no, name_snapshot, net_minor, tax_category_id, is_weighed)
             VALUES (?1, ?2, ?3, 1000, 1000, 2500, 0, 400, 2900, 1, 'Espresso',
                     2500, ?4, 0)",
            params![
                sale.line.as_bytes().as_slice(),
                sale.sale.as_bytes().as_slice(),
                sale.product.as_bytes().as_slice(),
                sale.tax_category.as_bytes().as_slice(),
            ],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO sale_tender (id, sale_id, method, amount_minor, change_minor)
             VALUES (?1, ?2, 'cash', 2900, 0)",
            params![
                sale.tender.as_bytes().as_slice(),
                sale.sale.as_bytes().as_slice(),
            ],
        )
        .unwrap();
        sale.checkout.write_facts_before_envelope(tx);
    }

    /// A whole checkout: the fact graph and its delivery envelope, one `BEGIN`,
    /// one `COMMIT` (I-9).
    ///
    /// The envelope lands in the middle rather than at the end, because two of
    /// the facts refuse to exist without it — `audit_log` and
    /// `tender_status_event` each check `sync_commit_ready` on insert — and the
    /// completion refuses to happen until all of them are there. It is still
    /// one transaction: nothing is durable until `COMMIT`.
    fn checkout(register: &Register, sale: &Sale, chain: &RegisteredChain) -> CommitReceipt {
        let tx = register.conn.unchecked_transaction().unwrap();
        write_sale_facts(&tx, sale, chain);
        let receipt = OutboxRepository::new(&register.conn)
            .write_commit(&tx, &sale.envelope(), &sale.members())
            .expect("the envelope is written beside the facts, not after them");
        sale.checkout.write_facts_after_envelope(&tx, chain);
        sale.checkout.complete(&tx);
        tx.commit().unwrap();
        receipt
    }

    #[test]
    fn a_completed_sale_has_one_ready_sync_commit() {
        let register = Register::open();
        let sale = Sale::new();
        let chain = seed_catalog(&register.conn, &sale);

        let receipt = checkout(&register, &sale, &chain);

        assert_eq!(
            register.scalar("SELECT count(*) FROM sale WHERE status = 'completed'"),
            1,
            "the fixture must actually have completed a sale"
        );
        assert_eq!(
            register.scalar_for(
                "SELECT count(*) FROM sync_commit WHERE id = ?1",
                &sale.commit
            ),
            1,
            "one business transaction produces exactly one envelope"
        );
        assert_eq!(receipt.commit_size, COMMIT_SIZE);

        let outbox = OutboxRepository::new(&register.conn);
        assert!(
            outbox.is_ready(sale.commit.as_bytes()).unwrap(),
            "a completed sale's envelope must appear in sync_commit_ready: every \
             member present, indexed from zero, each with its delivery row"
        );
        assert!(outbox.is_complete(sale.commit.as_bytes()).unwrap());

        let (commit_size, commit_hash, protocol_version, schema_version, producer_version): (
            i64,
            String,
            i64,
            i64,
            String,
        ) = register
            .conn
            .query_row(
                "SELECT commit_size, commit_hash, protocol_version, schema_version,
                        producer_version
                   FROM sync_commit WHERE id = ?1",
                [sale.commit.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(commit_size, COMMIT_SIZE);
        assert_eq!(
            commit_hash, receipt.commit_hash,
            "the stored group hash is the one the writer reported"
        );
        assert_eq!(commit_hash.len(), 64, "BLAKE3 hex is 64 characters");
        assert_eq!(protocol_version, PROTOCOL_VERSION);
        assert_eq!(
            schema_version,
            pos_db::SCHEMA_VERSION,
            "the envelope records the migration version its payloads were built by"
        );
        assert_eq!(producer_version, PRODUCER_VERSION);

        assert_eq!(
            register.scalar_for(
                "SELECT count(*) FROM sync_outbox o
                   JOIN fact_commit_member m ON m.change_id = o.change_id
                  WHERE m.commit_id = ?1 AND o.state = 'pending'",
                &sale.commit
            ),
            COMMIT_SIZE,
            "every member is deliverable the moment the transaction commits"
        );
        let seqs = register.scalar_for(
            "SELECT count(DISTINCT o.seq) FROM sync_outbox o
               JOIN fact_commit_member m ON m.change_id = o.change_id
              WHERE m.commit_id = ?1",
            &sale.commit,
        );
        assert_eq!(seqs, COMMIT_SIZE, "push order comes from sync_outbox.seq");
    }

    #[test]
    fn every_fact_member_is_in_the_commit_manifest() {
        let register = Register::open();
        let sale = Sale::new();
        let chain = seed_catalog(&register.conn, &sale);
        checkout(&register, &sale, &chain);

        let outbox = OutboxRepository::new(&register.conn);
        let manifest = outbox.manifest(sale.commit.as_bytes()).unwrap();
        let commit_size = register.scalar_for(
            "SELECT commit_size FROM sync_commit WHERE id = ?1",
            &sale.commit,
        );

        assert_eq!(
            manifest.len() as i64,
            commit_size,
            "a manifest shorter than commit_size is a header without its lines"
        );
        assert_eq!(
            manifest
                .iter()
                .map(|entry| entry.commit_index)
                .collect::<Vec<_>>(),
            (0..commit_size).collect::<Vec<_>>(),
            "commit_index orders the members within the group, from zero"
        );

        let members: BTreeSet<(String, Vec<u8>)> = manifest
            .iter()
            .map(|entry| (entry.entity.clone(), entry.entity_id.to_vec()))
            .collect();
        assert_eq!(
            members,
            sale_facts_in_the_database(&register.conn),
            "every fact this transaction wrote must be in the manifest, and the \
             manifest must claim nothing else"
        );

        for (entry, expected) in manifest.iter().zip(sale.members()) {
            assert_eq!(entry.op, "insert", "facts are inserted, never upserted");
            assert_eq!(entry.created_at, AT);
            assert_eq!(
                entry.payload, expected.payload,
                "the manifest stores the canonical bytes it was given"
            );
            assert_eq!(entry.payload_hash.len(), 64);
            assert!(
                entry
                    .payload_hash
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            );
        }

        let distinct_hashes: BTreeSet<&str> = manifest
            .iter()
            .map(|entry| entry.payload_hash.as_str())
            .collect();
        assert_eq!(
            distinct_hashes.len(),
            manifest.len(),
            "no two payloads in one commit may share a digest"
        );
    }

    #[test]
    fn delivery_rows_can_be_pruned_without_losing_the_manifest() {
        let register = Register::open();
        let sale = Sale::new();
        let chain = seed_catalog(&register.conn, &sale);
        let receipt = checkout(&register, &sale, &chain);

        let outbox = OutboxRepository::new(&register.conn);
        let before = outbox.manifest(sale.commit.as_bytes()).unwrap();
        assert_eq!(before.len() as i64, COMMIT_SIZE);

        register
            .conn
            .execute("DELETE FROM sync_outbox", [])
            .expect_err("an unacknowledged delivery row is not queue litter");

        // Acknowledged and pruned one commit at a time, which is how a pusher
        // will do it: the shift's own delivery row belongs to an earlier
        // business transaction and must come through untouched.
        let acknowledged = register
            .conn
            .execute(
                "UPDATE sync_outbox
                    SET state = 'acknowledged', acknowledged_at = ?1, pushed_at = ?1
                  WHERE change_id IN
                        (SELECT change_id FROM fact_commit_member WHERE commit_id = ?2)",
                params![AT, sale.commit.as_bytes().as_slice()],
            )
            .unwrap();
        assert_eq!(acknowledged as i64, COMMIT_SIZE);
        let pruned = register
            .conn
            .execute(
                "DELETE FROM sync_outbox
                  WHERE change_id IN
                        (SELECT change_id FROM fact_commit_member WHERE commit_id = ?1)",
                [sale.commit.as_bytes().as_slice()],
            )
            .unwrap();
        assert_eq!(pruned as i64, COMMIT_SIZE);
        assert_eq!(
            register.scalar("SELECT count(*) FROM sync_outbox"),
            SHIFT_ENVELOPE_MEMBERS,
            "pruning one commit's queue leaves another commit's alone"
        );

        assert!(
            !outbox.is_ready(sale.commit.as_bytes()).unwrap(),
            "with no delivery rows there is nothing left to deliver"
        );
        assert!(
            outbox.is_complete(sale.commit.as_bytes()).unwrap(),
            "the manifest is the financial evidence and survives the queue"
        );
        assert_eq!(
            outbox.manifest(sale.commit.as_bytes()).unwrap(),
            before,
            "the original commit membership must reconstruct unchanged"
        );

        let (commit_size, commit_hash): (i64, String) = register
            .conn
            .query_row(
                "SELECT commit_size, commit_hash FROM sync_commit WHERE id = ?1",
                [sale.commit.as_bytes().as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(commit_size, COMMIT_SIZE);
        assert_eq!(
            commit_hash, receipt.commit_hash,
            "the group hash still describes the same members after pruning"
        );
    }

    #[test]
    fn outbox_commit_rolls_back_with_the_fact_graph() {
        let register = Register::open();
        let sale = Sale::new();
        let chain = seed_catalog(&register.conn, &sale);

        let tx = register.conn.unchecked_transaction().unwrap();
        write_sale_facts(&tx, &sale, &chain);
        let receipt = OutboxRepository::new(&register.conn)
            .write_commit(&tx, &sale.envelope(), &sale.members())
            .unwrap();
        assert_eq!(receipt.commit_size, COMMIT_SIZE);
        assert!(
            OutboxRepository::new(&tx)
                .is_ready(sale.commit.as_bytes())
                .unwrap(),
            "inside the transaction the envelope is whole"
        );

        tx.rollback().unwrap();

        for table in [
            "sale",
            "sale_line",
            "sale_tender",
            "sale_line_tax",
            "receipt_artifact",
            "print_job",
        ] {
            assert_eq!(
                register.scalar(&format!("SELECT count(*) FROM {table}")),
                0,
                "{table} kept a row after the business transaction rolled back"
            );
        }
        for (table, sql) in [
            (
                "sync_commit",
                "SELECT count(*) FROM sync_commit WHERE id = ?1",
            ),
            (
                "fact_commit_member",
                "SELECT count(*) FROM fact_commit_member WHERE commit_id = ?1",
            ),
            (
                "sync_outbox",
                "SELECT count(*) FROM sync_outbox o
                   JOIN fact_commit_member m ON m.change_id = o.change_id
                  WHERE m.commit_id = ?1",
            ),
        ] {
            assert_eq!(
                register.scalar_for(sql, &sale.commit),
                0,
                "{table} kept a row after the business transaction rolled back"
            );
        }
        assert_eq!(
            register.scalar("SELECT count(*) FROM product"),
            1,
            "the catalogue was written before the transaction and is unaffected"
        );
        assert_eq!(
            register.scalar("SELECT count(*) FROM sync_commit"),
            SHIFT_ENVELOPE_MEMBERS,
            "the shift open was an earlier business transaction and survives"
        );
        assert_eq!(
            register.scalar("SELECT count(*) FROM shift_state WHERE state = 'open'"),
            1,
            "and the register is still open for business"
        );
    }

    #[test]
    fn an_envelope_with_no_members_is_refused() {
        let register = Register::open();
        let sale = Sale::new();

        let tx = register.conn.unchecked_transaction().unwrap();
        let refused = OutboxRepository::new(&register.conn)
            .write_commit(&tx, &sale.envelope(), &[])
            .expect_err("a commit_size of zero is a header with no lines");
        assert!(matches!(refused, pos_db::DbError::EmptyCommitRefused));
        assert_eq!(register.scalar("SELECT count(*) FROM sync_commit"), 0);
    }

    #[test]
    fn a_second_commit_cannot_claim_the_same_fact() {
        let register = Register::open();
        let sale = Sale::new();
        let chain = seed_catalog(&register.conn, &sale);
        checkout(&register, &sale, &chain);

        let mut second = Sale::new();
        second.commit = Uuid::from_u128(0x30);
        second.changes = [
            Uuid::from_u128(0x31),
            Uuid::from_u128(0x32),
            Uuid::from_u128(0x33),
        ];

        let tx = register.conn.unchecked_transaction().unwrap();
        OutboxRepository::new(&register.conn)
            .write_commit(&tx, &second.envelope(), &second.members())
            .expect_err("one fact belongs to exactly one commit");
        tx.rollback().unwrap();

        assert_eq!(
            register.scalar_for(
                "SELECT count(*) FROM sync_commit WHERE id = ?1",
                &second.commit
            ),
            0,
            "the refused envelope left no header behind"
        );
        assert_eq!(
            register.scalar_for(
                "SELECT count(*) FROM fact_commit_member WHERE commit_id = ?1",
                &sale.commit
            ),
            COMMIT_SIZE,
            "the first commit still owns every fact of the sale"
        );
    }
}
