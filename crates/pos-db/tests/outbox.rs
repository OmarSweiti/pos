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

    const KEY: &str = "test-key";
    /// Time is an argument, here as in the writer (I-8). One fixed instant, so
    /// nothing in this file can depend on when it ran.
    const AT: &str = "2026-08-27T09:30:00.000Z";
    const BUSINESS_DATE: &str = "2026-08-27";
    const PRODUCER_VERSION: &str = "pos-db-1.8.9-test";
    const PROTOCOL_VERSION: i64 = 1;
    /// A sale, its line and its tender: the smallest fact graph a checkout can
    /// produce, and the one the server must accept or reject whole.
    const COMMIT_SIZE: i64 = 3;

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
        changes: [Uuid; 3],
        payloads: [String; 3],
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
            Self {
                commit,
                sale,
                line,
                tender,
                product,
                tax_category,
                register,
                changes,
                payloads,
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
        }
    }

    /// The `(entity, entity_id)` pairs the manifest must contain, read from the
    /// fact tables themselves rather than restated.
    fn facts_in_the_database(conn: &Connection) -> BTreeSet<(String, Vec<u8>)> {
        let mut facts = BTreeSet::new();
        for (entity, sql) in [
            ("sale", "SELECT id FROM sale"),
            ("sale_line", "SELECT id FROM sale_line"),
            ("sale_tender", "SELECT id FROM sale_tender"),
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

    /// The catalogue rows a sale line points at. Reference data, not facts, so
    /// it is written before the business transaction opens.
    fn seed_catalog(conn: &Connection, sale: &Sale) {
        conn.execute(
            "INSERT INTO tax_category (id, code, name_ar, treatment)
             VALUES (?1, 'STD16', 'ضريبة عامة', 'standard')",
            [sale.tax_category.as_bytes().as_slice()],
        )
        .unwrap();
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
    }

    /// The fact graph: header, line, tender, then the parked → completed
    /// transition that seals it. A sale inserted as `completed` could never
    /// take a line — I-4's insert guard refuses one — so this is the order a
    /// real checkout writes in too.
    fn write_sale_facts(tx: &Transaction<'_>, sale: &Sale) {
        tx.execute(
            "INSERT INTO sale
               (id, receipt_number, register_id, status, subtotal_minor,
                tax_minor, total_minor, currency, business_date, completed_at)
             VALUES (?1, '000123', ?2, 'parked', 2500, 400, 2900, 'JOD', ?3, ?4)",
            params![
                sale.sale.as_bytes().as_slice(),
                sale.register.as_bytes().as_slice(),
                BUSINESS_DATE,
                AT,
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
        tx.execute(
            "UPDATE sale SET status = 'completed' WHERE id = ?1",
            [sale.sale.as_bytes().as_slice()],
        )
        .unwrap();
    }

    /// A whole checkout: the fact graph and its delivery envelope, one `BEGIN`,
    /// one `COMMIT` (I-9).
    fn checkout(register: &Register, sale: &Sale) -> CommitReceipt {
        let tx = register.conn.unchecked_transaction().unwrap();
        write_sale_facts(&tx, sale);
        let receipt = OutboxRepository::new(&register.conn)
            .write_commit(&tx, &sale.envelope(), &sale.members())
            .expect("the envelope is written beside the facts, not after them");
        tx.commit().unwrap();
        receipt
    }

    #[test]
    fn a_completed_sale_has_one_ready_sync_commit() {
        let register = Register::open();
        let sale = Sale::new();
        seed_catalog(&register.conn, &sale);

        let receipt = checkout(&register, &sale);

        assert_eq!(
            register.scalar("SELECT count(*) FROM sale WHERE status = 'completed'"),
            1,
            "the fixture must actually have completed a sale"
        );
        assert_eq!(
            register.scalar("SELECT count(*) FROM sync_commit"),
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
            register.scalar("SELECT count(*) FROM sync_outbox WHERE state = 'pending'"),
            COMMIT_SIZE,
            "every member is deliverable the moment the transaction commits"
        );
        let seqs: i64 = register.scalar("SELECT count(DISTINCT seq) FROM sync_outbox");
        assert_eq!(seqs, COMMIT_SIZE, "push order comes from sync_outbox.seq");
    }

    #[test]
    fn every_fact_member_is_in_the_commit_manifest() {
        let register = Register::open();
        let sale = Sale::new();
        seed_catalog(&register.conn, &sale);
        checkout(&register, &sale);

        let outbox = OutboxRepository::new(&register.conn);
        let manifest = outbox.manifest(sale.commit.as_bytes()).unwrap();
        let commit_size = register.scalar("SELECT commit_size FROM sync_commit");

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
            facts_in_the_database(&register.conn),
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
            "three different payloads must not share one digest"
        );
    }

    #[test]
    fn delivery_rows_can_be_pruned_without_losing_the_manifest() {
        let register = Register::open();
        let sale = Sale::new();
        seed_catalog(&register.conn, &sale);
        let receipt = checkout(&register, &sale);

        let outbox = OutboxRepository::new(&register.conn);
        let before = outbox.manifest(sale.commit.as_bytes()).unwrap();
        assert_eq!(before.len() as i64, COMMIT_SIZE);

        register
            .conn
            .execute("DELETE FROM sync_outbox", [])
            .expect_err("an unacknowledged delivery row is not queue litter");

        let acknowledged = register
            .conn
            .execute(
                "UPDATE sync_outbox
                    SET state = 'acknowledged', acknowledged_at = ?1, pushed_at = ?1",
                [AT],
            )
            .unwrap();
        assert_eq!(acknowledged as i64, COMMIT_SIZE);
        let pruned = register
            .conn
            .execute("DELETE FROM sync_outbox", [])
            .unwrap();
        assert_eq!(pruned as i64, COMMIT_SIZE);
        assert_eq!(register.scalar("SELECT count(*) FROM sync_outbox"), 0);

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
        seed_catalog(&register.conn, &sale);

        let tx = register.conn.unchecked_transaction().unwrap();
        write_sale_facts(&tx, &sale);
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
            "sync_commit",
            "fact_commit_member",
            "sync_outbox",
        ] {
            assert_eq!(
                register.scalar(&format!("SELECT count(*) FROM {table}")),
                0,
                "{table} kept a row after the business transaction rolled back"
            );
        }
        assert_eq!(
            register.scalar("SELECT count(*) FROM product"),
            1,
            "the catalogue was written before the transaction and is unaffected"
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
        seed_catalog(&register.conn, &sale);
        checkout(&register, &sale);

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

        assert_eq!(register.scalar("SELECT count(*) FROM sync_commit"), 1);
        assert_eq!(
            register.scalar("SELECT count(*) FROM fact_commit_member"),
            COMMIT_SIZE
        );
    }
}
