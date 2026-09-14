//! A registered chain seeded to the point where a sale may complete.
//!
//! Every row here is an `INSERT` on the schema [`pos_db::open`] builds. Nothing
//! in this file reads `ref/schema.md`, which is the whole difference from
//! `common/mod.rs`: `approval.rs`, `outbox.rs` and `sale_immutability.rs` prove
//! guarantees a register actually ships, and an overlay of reference SQL would
//! make a green run stop being evidence of that.
//!
//! **Why this exists.** Migration `0005` repaired `sale.register_id` with a
//! trigger — `ALTER TABLE` cannot retrofit a `REFERENCES` clause — and put
//! seven gates in front of `sale.status = 'completed'`: an open shift for the
//! register, store and business date; the store's tax computation policy; an
//! evidenced fiscal decision; a tax component snapshot per line matching the
//! applicable rates exactly; a document discount recap equal to the sum of line
//! allowances; an initial status event per tender; and durable outputs — an
//! original receipt artifact with a queued print job, and a `sync_commit` whose
//! manifest names every fact of the sale. Before `0005` a `pos-db` test could
//! complete a sale against no register at all. Seventeen of them did, and the
//! repair is this one fixture rather than seventeen local patches.
//!
//! **What it writes is what `0005` requires, and nothing more.** It does not
//! write `sale_tax_summary` or `sale_supply_tax_context`: `0005` requires those
//! rows to be in the manifest *if they exist*, never that they exist. A fixture
//! that seeded optional facts would silently widen what every test using it is
//! asserting. `1.9.1`'s own suite exercises them explicitly instead.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use pos_db::repo::outbox::{CommitEnvelope, FactMember, OutboxRepository};
use rusqlite::{Connection, params};

/// Ids this fixture owns start with this byte.
///
/// Every id a `pos-db` test mints is `[b; 16]`, `vec![b; 16]` or
/// `Uuid::from_u128(n)` for a small `n` — all zeros but the tail. Shaping
/// fixture ids as `[0xFC, slot, 0 … 0, tag]` puts them outside all three, so a
/// fixture row can never silently become the row a test meant to assert on.
const FIXTURE_PREFIX: u8 = 0xFC;

/// The reference rows live in slot 0; each [`Checkout`] takes a slot of its own.
const REFERENCE_SLOT: u8 = 0;

/// One fixed instant. Time is an argument here as everywhere (I-8), and a
/// fixture that read a clock could pass on Tuesday and fail on Wednesday.
pub const AT: &str = "2026-08-25T10:00:00.000Z";

/// 16% — the Jordanian general sales rate, in parts per million.
const RATE_PPM: i64 = 160_000;

const PRODUCER_VERSION: &str = "pos-db-registered-chain-fixture";
const PROTOCOL_VERSION: i64 = 1;

fn fixture_id(slot: u8, tag: u8) -> Vec<u8> {
    // An array, not a `Vec`: a constant index into a fixed-size array is
    // provably in bounds, which is what keeps `clippy::indexing_slicing` quiet
    // without an `#[allow]` that would also cover a real mistake.
    let mut id = [0u8; 16];
    id[0] = FIXTURE_PREFIX;
    id[1] = slot;
    id[15] = tag;
    id.to_vec()
}

/// The last byte of an id, which is where every fixture and test id varies.
fn tail(id: &[u8]) -> u8 {
    *id.last().expect("an id is never empty")
}

fn array(id: &[u8]) -> [u8; 16] {
    id.try_into().expect("every id in this fixture is 16 bytes")
}

/// The org, people, tax pack, computation policy and store every sale hangs
/// off. Registers and shifts are added to it, because tests name their own.
pub struct RegisteredChain {
    pub org: Vec<u8>,
    pub store: Vec<u8>,
    pub cashier: Vec<u8>,
    pub tax_category: Vec<u8>,
    pub tax_rule_pack: Vec<u8>,
    pub tax_rate: Vec<u8>,
    pub tax_policy: Vec<u8>,
}

impl RegisteredChain {
    /// Seed the reference world. The store is `exempt`/`disabled` with an
    /// evidence reference, which is one of the two shapes `0005`'s fiscal
    /// decision gate accepts and the one a Phase-1 merchant actually has:
    /// JoFotara enablement is Phase 2 and needs a taxpayer type nobody has
    /// filed yet (#69).
    pub fn seed(conn: &Connection) -> Self {
        Self::seed_taxing(conn, None)
    }

    /// Seed against a tax category the caller already inserted, for a suite
    /// that owns its own catalogue.
    ///
    /// The category has to be known before seeding starts, not added
    /// afterwards: the rate pack is sealed at the end of this function and
    /// `tax_rate_pack_open_insert` refuses a rate against a sealed pack. A
    /// second category later would therefore need a second pack, and a store
    /// has exactly one.
    pub fn seed_for_category(conn: &Connection, tax_category: &[u8]) -> Self {
        Self::seed_taxing(conn, Some(tax_category))
    }

    fn seed_taxing(conn: &Connection, caller_category: Option<&[u8]>) -> Self {
        let chain = Self {
            org: fixture_id(REFERENCE_SLOT, 1),
            store: fixture_id(REFERENCE_SLOT, 2),
            cashier: fixture_id(REFERENCE_SLOT, 3),
            tax_category: caller_category
                .map(<[u8]>::to_vec)
                .unwrap_or_else(|| fixture_id(REFERENCE_SLOT, 4)),
            tax_rule_pack: fixture_id(REFERENCE_SLOT, 5),
            tax_rate: fixture_id(REFERENCE_SLOT, 6),
            tax_policy: fixture_id(REFERENCE_SLOT, 7),
        };

        conn.execute(
            "INSERT INTO org (id, legal_name) VALUES (?1, 'Registered Chain Fixture')",
            [&chain.org],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             VALUES (?1, ?2, 'FIX-1', 'Fixture Cashier', 'placeholder-not-a-hash', ?3)",
            params![&chain.cashier, &chain.org, AT],
        )
        .unwrap();
        if caller_category.is_none() {
            conn.execute(
                "INSERT INTO tax_category (id, code, name_ar, treatment)
                 VALUES (?1, 'FIXSTD16', 'ضريبة المبيعات العامة', 'standard')",
                [&chain.tax_category],
            )
            .unwrap();
        }

        // The pack is assembled while `pending` and sealed afterwards, in that
        // order: `tax_rate_pack_open_insert` refuses a rate against an approved
        // pack, the CHECK refuses an approved row with no approver, and
        // `tax_rule_pack_freeze_after_approval` allows exactly this transition.
        conn.execute(
            "INSERT INTO tax_rule_pack
               (id, profile_scope, pack_version, source_ref, content_hash, status)
             VALUES (?1, 'standard', 'fixture-v1', 'fixture', ?2, 'pending')",
            params![&chain.tax_rule_pack, vec![0x05u8; 32]],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tax_rate
               (id, rule_pack_id, tax_category_id, component_code, treatment,
                calculation_kind, rate_ppm, calculation_order, base_kind,
                valid_from, profile_scope)
             VALUES (?1, ?2, ?3, 'GST', 'standard', 'ad_valorem', ?4, 0,
                     'line_net', '2026-01-01', 'standard')",
            params![
                &chain.tax_rate,
                &chain.tax_rule_pack,
                &chain.tax_category,
                RATE_PPM
            ],
        )
        .unwrap();
        conn.execute(
            "UPDATE tax_rule_pack SET status = 'approved', approved_by = ?1, approved_at = ?2
              WHERE id = ?3",
            params![&chain.cashier, AT, &chain.tax_rule_pack],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tax_computation_policy
               (id, jurisdiction, policy_version, rounding_rule, cash_round_step_minor,
                cash_round_direction, cash_round_tax_treatment, source_ref, content_hash,
                approved_at)
             VALUES (?1, 'JO', 'fixture-v1', 'half_even', 5, 'nearest', 'none',
                     'fixture', ?2, ?3)",
            params![&chain.tax_policy, vec![0x09u8; 32], AT],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO store
               (id, org_id, code, name_ar, fiscal_obligation,
                fiscal_obligation_evidence_ref, fiscal_profile,
                tax_rule_pack_id, tax_computation_policy_id)
             VALUES (?1, ?2, 'FIX-S1', 'متجر الاختبار', 'exempt',
                     'fixture-evidence', 'disabled', ?3, ?4)",
            params![
                &chain.store,
                &chain.org,
                &chain.tax_rule_pack,
                &chain.tax_policy
            ],
        )
        .unwrap();

        chain
    }

    /// A register on this chain's store. The caller owns the id, because every
    /// repaired test already names one in its `sale` rows.
    pub fn add_register(&self, conn: &Connection, register: &[u8], code: &str) {
        conn.execute(
            "INSERT INTO register
               (id, store_id, code, name, device_id, credential_key_id,
                credential_algorithm, credential_public_key, credential_issued_at)
             VALUES (?1, ?2, ?3, ?3, ?4, ?5, 'ed25519', ?6, ?7)",
            params![
                register,
                &self.store,
                code,
                format!("device-{code}"),
                format!("key-{code}"),
                vec![0x99u8; 32],
                AT,
            ],
        )
        .unwrap();
    }

    /// An open shift, with the delivery envelope `shift_open_has_ready_commit`
    /// demands. `shift_project_open` writes `shift_state` itself, so the one
    /// open row per register comes from the migration and not from here.
    pub fn open_shift(&self, conn: &Connection, shift: &[u8], register: &[u8], date: &str) {
        let commit = fixture_id(REFERENCE_SLOT, 0x80 | tail(shift));
        let change = fixture_id(REFERENCE_SLOT, 0xC0 | tail(shift));
        self.write_envelope(conn, &commit, &[Member::new(&change, "shift", shift)]);
        conn.execute(
            "INSERT INTO shift
               (id, register_id, store_id, business_date, opened_by, opened_at, float_minor)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![shift, register, &self.store, date, &self.cashier, AT],
        )
        .unwrap();
    }

    /// One delivery envelope through the shipped writer, in its own
    /// transaction. Using [`OutboxRepository`] rather than hand-written SQL is
    /// deliberate: a fixture that assembled its own manifest could drift from
    /// the writer and hide the drift from every test that leans on it.
    /// A caller already inside a transaction must not call this — SQLite has no
    /// nested `BEGIN`. Open the shift and seal the sale as the separate
    /// business transactions they are.
    pub fn write_envelope(&self, conn: &Connection, commit: &[u8], members: &[Member]) {
        let commit_id = array(commit);
        let facts: Vec<FactMember<'_>> = members.iter().map(Member::as_fact).collect();

        let tx = conn.unchecked_transaction().unwrap();
        OutboxRepository::new(conn)
            .write_commit(
                &tx,
                &CommitEnvelope {
                    commit_id: &commit_id,
                    protocol_version: PROTOCOL_VERSION,
                    schema_version: pos_db::SCHEMA_VERSION,
                    producer_version: PRODUCER_VERSION,
                    created_at: AT,
                },
                &facts,
            )
            .expect("a fixture envelope must be complete when it is written");
        tx.commit().unwrap();
    }
}

/// One member of a delivery envelope, owning its bytes so a caller can hold a
/// whole manifest and hand out [`FactMember`] borrows from it.
///
/// The payload is a stand-in, not a canonical projection: `pos-db` has no
/// canonical serializer yet — `crates/pos-sync/src/canonical.rs` arrives with
/// the sync engine — and the writer hashes exactly the bytes it is given. A
/// suite whose subject *is* the envelope authors its own payloads instead;
/// `outbox.rs` does.
pub struct Member {
    change_id: [u8; 16],
    entity: String,
    entity_id: [u8; 16],
    payload: String,
}

impl Member {
    pub fn new(change_id: &[u8], entity: &str, entity_id: &[u8]) -> Self {
        let entity_id = array(entity_id);
        Self {
            change_id: array(change_id),
            entity: entity.to_owned(),
            entity_id,
            payload: format!(r#"{{"entity":"{entity}","id":"{}"}}"#, hex(&entity_id)),
        }
    }

    pub fn change_id(&self) -> &[u8; 16] {
        &self.change_id
    }

    pub fn entity(&self) -> &str {
        &self.entity
    }

    pub fn entity_id(&self) -> &[u8; 16] {
        &self.entity_id
    }

    pub fn as_fact(&self) -> FactMember<'_> {
        FactMember {
            change_id: &self.change_id,
            entity: &self.entity,
            entity_id: &self.entity_id,
            payload: &self.payload,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// One checkout's auxiliary facts: everything `0005` requires beyond the sale
/// header, its lines and its tenders, which stay the caller's to write because
/// every repaired test asserts on their exact shape.
///
/// The auxiliary ids are derived from the slot rather than taken as arguments.
/// A test that had to name one of them would be asserting on the fixture
/// instead of on its own subject.
pub struct Checkout {
    pub sale: Vec<u8>,
    pub register: Vec<u8>,
    pub lines: Vec<Vec<u8>>,
    pub tenders: Vec<Vec<u8>>,
    pub commit: Vec<u8>,
    pub shift: Vec<u8>,
    pub line_taxes: Vec<Vec<u8>>,
    pub tender_events: Vec<Vec<u8>>,
    pub artifact: Vec<u8>,
    pub print_job: Vec<u8>,
    pub audit: Vec<u8>,
}

impl Checkout {
    /// `slot` separates one checkout's fixture ids from another's in the same
    /// database. It must not be the reference world's slot.
    pub fn new(slot: u8, sale: &[u8], register: &[u8], lines: &[&[u8]], tenders: &[&[u8]]) -> Self {
        assert!(
            slot != REFERENCE_SLOT,
            "slot {REFERENCE_SLOT} belongs to the reference world"
        );
        let tag = |offset: usize, index: usize| {
            fixture_id(
                slot,
                u8::try_from(offset + index).expect("fixture slots hold 256 ids"),
            )
        };
        Self {
            sale: sale.to_vec(),
            register: register.to_vec(),
            lines: lines.iter().map(|l| l.to_vec()).collect(),
            tenders: tenders.iter().map(|t| t.to_vec()).collect(),
            commit: fixture_id(slot, 0x04),
            shift: fixture_id(slot, 0x05),
            line_taxes: (0..lines.len()).map(|i| tag(0x10, i)).collect(),
            tender_events: (0..tenders.len()).map(|i| tag(0x30, i)).collect(),
            artifact: fixture_id(slot, 0x01),
            print_job: fixture_id(slot, 0x02),
            audit: fixture_id(slot, 0x03),
        }
    }

    /// Use a commit the caller minted. `outbox.rs` does, because the envelope
    /// is that suite's subject rather than its scaffolding.
    pub fn with_commit(mut self, commit: &[u8]) -> Self {
        self.commit = commit.to_vec();
        self
    }

    /// The facts the *caller* wrote — the sale, its lines, its tenders — in
    /// parent-before-child order.
    fn own_facts(&self) -> Vec<(&'static str, Vec<u8>)> {
        let mut facts: Vec<(&'static str, Vec<u8>)> = vec![("sale", self.sale.clone())];
        facts.extend(self.lines.iter().map(|l| ("sale_line", l.clone())));
        facts.extend(self.tenders.iter().map(|t| ("sale_tender", t.clone())));
        facts
    }

    /// The facts this fixture writes, in parent-before-child order: a tax
    /// component per line, a status event per tender, the original receipt
    /// artifact, and the audit row.
    fn auxiliary_facts(&self) -> Vec<(&'static str, Vec<u8>)> {
        let mut facts: Vec<(&'static str, Vec<u8>)> = Vec::new();
        facts.extend(self.line_taxes.iter().map(|t| ("sale_line_tax", t.clone())));
        facts.extend(
            self.tender_events
                .iter()
                .map(|e| ("tender_status_event", e.clone())),
        );
        facts.push(("receipt_artifact", self.artifact.clone()));
        facts.push(("audit_log", self.audit.clone()));
        facts
    }

    /// `(entity, entity_id)` for every fact of this sale, parents before
    /// children — the order the server applies them in, and therefore the order
    /// `commit_index` records (`ref/sync-protocol.md` §2, rule 1).
    ///
    /// `sale_commit_base_complete` refuses a completion whose manifest is
    /// missing any one of these, so this list *is* the durable-outputs
    /// contract, stated once.
    pub fn manifest(&self) -> Vec<(&'static str, Vec<u8>)> {
        let mut facts = self.own_facts();
        facts.extend(self.auxiliary_facts());
        facts
    }

    /// The whole manifest as envelope members, ready for
    /// [`RegisteredChain::write_envelope`].
    pub fn members(&self) -> Vec<Member> {
        self.members_from(self.manifest(), 0)
    }

    /// Only the members this fixture is responsible for, for a caller that
    /// writes the sale, line and tender members itself. `first_index` is how
    /// many members the caller put ahead of these, so change ids stay distinct.
    pub fn auxiliary_members(&self, first_index: usize) -> Vec<Member> {
        self.members_from(self.auxiliary_facts(), first_index)
    }

    fn members_from(&self, facts: Vec<(&'static str, Vec<u8>)>, first: usize) -> Vec<Member> {
        facts
            .into_iter()
            .enumerate()
            .map(|(offset, (entity, entity_id))| {
                let mut change = array(&entity_id);
                change[0] = FIXTURE_PREFIX;
                change[1] = 0xCE;
                change[2] =
                    u8::try_from(first + offset).expect("a fixture commit holds 256 members");
                Member::new(&change, entity, &entity_id)
            })
            .collect()
    }

    /// Point this sale and its lines at the chain: the store, the open shift,
    /// the store's computation policy and a tax category.
    ///
    /// These are the columns `0005` requires a completed sale to carry and that
    /// no pre-`0005` test had any reason to set. Attaching them here rather
    /// than rewriting each suite's `INSERT` keeps every repaired test's SQL
    /// about its own subject; `1.9.1`'s own suite proves each gate directly.
    ///
    /// The shift is opened for the sale's *own* register and business date,
    /// read back rather than taken as arguments — a shift that did not match
    /// would fail the very gate this exists to satisfy. An open shift already
    /// on the register is reused, because `idx_shift_one_open` allows exactly
    /// one.
    pub fn attach(&self, conn: &Connection, chain: &RegisteredChain) {
        let (register, business_date): (Vec<u8>, String) = conn
            .query_row(
                "SELECT register_id, business_date FROM sale WHERE id = ?1",
                [&self.sale],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("attach runs after the sale is inserted, parked");

        let open: Option<(Vec<u8>, String)> = conn
            .query_row(
                "SELECT sh.id, sh.business_date
                   FROM shift sh JOIN shift_state ss ON ss.shift_id = sh.id
                  WHERE ss.register_id = ?1 AND ss.state = 'open'",
                [&register],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        let shift = match open {
            Some((id, date)) => {
                assert_eq!(
                    date, business_date,
                    "the register already has an open shift on another business date, and \
                     idx_shift_one_open allows only one — close it before completing this sale"
                );
                id
            }
            None => {
                chain.open_shift(conn, &self.shift, &register, &business_date);
                self.shift.clone()
            }
        };

        conn.execute(
            "UPDATE sale
                SET store_id = ?1, shift_id = ?2, tax_computation_policy_id = ?3
              WHERE id = ?4",
            params![&chain.store, &shift, &chain.tax_policy, &self.sale],
        )
        .unwrap();
        // The product's category moves with the line's. A line whose category
        // differs from its product's is a supply-specific override, and
        // `sale_line_tax_category_evidenced_update` refuses one without
        // immutable `sale_supply_tax_context` evidence — which is a different
        // thing from an ordinary taxed line and must not be faked here.
        for line in &self.lines {
            conn.execute(
                "UPDATE product SET tax_category_id = ?1
                  WHERE id = (SELECT product_id FROM sale_line WHERE id = ?2)",
                params![&chain.tax_category, line],
            )
            .unwrap();
            conn.execute(
                "UPDATE sale_line SET tax_category_id = ?1 WHERE id = ?2",
                params![&chain.tax_category, line],
            )
            .unwrap();
        }
    }

    /// Everything after the envelope, in dependency order.
    pub fn write_facts(&self, conn: &Connection, chain: &RegisteredChain) {
        self.write_facts_before_envelope(conn);
        self.write_facts_after_envelope(conn, chain);
    }

    /// The auxiliary facts that need no delivery envelope: the tax components
    /// and the original receipt with its queued print job.
    pub fn write_facts_before_envelope(&self, conn: &Connection) {
        for (line, line_tax) in self.lines.iter().zip(self.line_taxes.iter()) {
            conn.execute(
                "INSERT INTO sale_line_tax
                   (id, sale_line_id, component_code, treatment, calculation_kind,
                    rate_ppm, calculation_order, base_kind, taxable_base_minor, tax_minor)
                 VALUES (?1, ?2, 'GST', 'standard', 'ad_valorem', ?3, 0, 'line_net',
                         (SELECT COALESCE(net_minor, total_minor) FROM sale_line WHERE id = ?2),
                         (SELECT tax_minor FROM sale_line WHERE id = ?2))",
                params![line_tax, line, RATE_PPM],
            )
            .expect(
                "a completed line needs exactly the components its store's rate pack \
                 applies — one here, matching the fixture's single GST rate",
            );
        }

        conn.execute(
            "INSERT INTO receipt_artifact
               (id, sale_id, artifact_kind, format, template_version, printer_profile,
                content_bytes, content_hash, generated_at)
             VALUES (?1, ?2, 'original', 'escpos', 'fixture-v1', '80mm', ?3, ?4, ?5)",
            params![
                &self.artifact,
                &self.sale,
                vec![0x01u8; 8],
                vec![0x27u8; 32],
                AT
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO print_job (id, artifact_id, state, attempts, created_at, updated_at)
             VALUES (?1, ?2, 'queued', 0, ?3, ?3)",
            params![&self.print_job, &self.artifact, AT],
        )
        .expect("the durable-outputs gate wants the job queued and never attempted");
    }

    /// The auxiliary facts that refuse to exist without one: `audit_log` and
    /// `tender_status_event` each check `sync_commit_ready` on insert.
    pub fn write_facts_after_envelope(&self, conn: &Connection, chain: &RegisteredChain) {
        conn.execute(
            "INSERT INTO audit_log
               (id, register_id, actor_id, action, entity, entity_id, payload,
                prev_hash, hash, at)
             VALUES (?1, ?2, ?3, 'sale.complete', 'sale', ?4, '{}', ?5, ?6, ?7)",
            params![
                &self.audit,
                &self.register,
                &chain.cashier,
                &self.sale,
                vec![0x00u8; 32],
                vec![0x29u8; 32],
                AT
            ],
        )
        .unwrap();

        for (tender, event) in self.tenders.iter().zip(self.tender_events.iter()) {
            conn.execute(
                "INSERT INTO tender_status_event
                   (id, tender_id, sync_commit_id, event_no, state, occurred_at)
                 VALUES (?1, ?2, ?3, 1, 'pending', ?4)",
                params![event, tender, &self.commit, AT],
            )
            .expect("a first tender event is `pending`; `reversed` has nothing to reverse");
        }
    }

    /// `parked` → `completed`, the transition the seven gates guard. The commit
    /// id lands in the same `UPDATE`: `sale_completed_requires_durable_outputs_update`
    /// reads `NEW.sync_commit_id`, so setting it afterwards would be too late.
    pub fn complete(&self, conn: &Connection) {
        conn.execute(
            "UPDATE sale SET status = 'completed', sync_commit_id = ?1 WHERE id = ?2",
            params![&self.commit, &self.sale],
        )
        .expect("the fixture must satisfy every gate 0005 put in front of completion");
    }

    /// Attach, envelope, auxiliary facts, transition — for the callers that do
    /// not own the envelope themselves. The sale, its lines and its tenders
    /// must already be in the database, `parked`.
    pub fn seal(&self, conn: &Connection, chain: &RegisteredChain) {
        self.attach(conn, chain);
        chain.write_envelope(conn, &self.commit, &self.members());
        self.write_facts(conn, chain);
        self.complete(conn);
    }
}
