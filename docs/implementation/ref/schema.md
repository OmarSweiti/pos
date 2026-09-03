# Schema — every migration, SQLite and Postgres

`0001_init.sql` and `0002_sale_integrity.sql` exist and are committed. Everything below them is new, appended in order, **never edited once committed** (conventions §9).

**Every ordinary table below is `STRICT`.** The staging tables used by the 0003 rebuild and
FTS5's virtual/shadow tables are the deliberate exceptions. SQLite's default is that a declared type is a
suggestion: `INTEGER NOT NULL` accepts `'ten point five'`, and a `REAL` lands in a
`*_minor` column without complaint — which is invariant I-1 defeated by the storage
engine rather than by anyone's mistake. `STRICT` makes the declared type the rule.

It also closes the NULL-identity hole for free: **in a `STRICT` table the primary key
columns are implicitly `NOT NULL`**, composite keys included — which is a fix for the
forty-odd identity columns where a NULL was never meaningful, and a *break* anywhere a
nullable column sits in a key on purpose. `user_role.store_id` was the one such column
in this schema; see the note on that table. Before this, two rows with
a NULL `id` inserted cleanly into any table here. There is deliberately no separate
`NOT NULL` sweep over the id columns, because `STRICT` already states it once per table
instead of 57 times, and because `INTEGER PRIMARY KEY` rowid aliases (`audit_log.seq`,
`sync_outbox.seq`) must keep accepting a NULL on insert to auto-assign — `STRICT` preserves
that, an explicit sweep invites someone to "fix" it.

**All six shipped tables are now `STRICT` too.** `product`, `sale`, `sale_line`,
`sale_tender`, `sync_cursor` and `sync_outbox` were created loose by 0001/0002. `STRICT`
cannot be added by `ALTER TABLE` and a committed migration is never edited, so 0003 rebuilds
them — first in the migration, before anything below points a foreign key at one, because
rebuilding a table with inbound references is far worse later. See the head of §0003 for the
staging recipe and why SQLite's documented twelve-step procedure cannot be used inside the
migration runner's transaction.

<!-- fact-tables: sync_commit, fact_commit_member, sale, sale_supply_tax_context, sale_line, sale_tender, sale_line_tax, sale_line_discount, sale_tax_summary, receipt_artifact, print_attempt, tender_status_event, shift, shift_close_event, shift_count_line, approval_handle, approval_consumption, audit_log, audit_checkpoint, stock_ledger, trade_scale_verification, cash_movement, cash_count, z_report, drawer_event, credit_note_context, refund_line_link, defect_resolution_event, document_link, stored_value_ledger, fiscal_document, fiscal_payload_event, fiscal_queue_event, fiscal_result, fiscal_reconciliation_issue, fiscal_resolution_event, consent_event, consent_acceptance, privacy_request_case, privacy_request_event, privacy_tombstone, loyalty_ledger, promotion_version, promotion_regulated_exclusion, promotion_publication, promotion_attribution, regulated_display_approval, supplier_invoice, supplier_invoice_line, supplier_invoice_line_tax, supplier_invoice_post_event, goods_receipt, goods_receipt_line, goods_receipt_post_event, stock_count, stock_count_line, stock_count_post_event, transfer, transfer_line, transfer_ship_event, transfer_receipt_line, transfer_receive_event, transfer_cancel_event, tax_filing_event, tax_period_adjustment, common_input_allocation, tax_credit_ledger, tax_filing_election, credit_note_period_assignment -->

<!-- sync-authority-register-up: sync_commit, fact_commit_member, sale, sale_supply_tax_context, sale_line, sale_tender, sale_line_tax, sale_line_discount, sale_tax_summary, receipt_artifact, tender_status_event, shift, shift_close_event, shift_count_line, approval_handle, approval_consumption, audit_log, audit_checkpoint, stock_ledger, trade_scale_verification, cash_movement, cash_count, z_report, drawer_event, credit_note_context, refund_line_link, defect_resolution_event, document_link, stored_value_instrument, stored_value_ledger, fiscal_document, fiscal_payload_event, fiscal_queue_event, fiscal_result, fiscal_reconciliation_issue, fiscal_resolution_event, consent_event, privacy_request_case, privacy_request_event, loyalty_ledger, promotion_attribution, supplier_invoice, supplier_invoice_line, supplier_invoice_line_tax, supplier_invoice_post_event, goods_receipt, goods_receipt_line, goods_receipt_post_event, stock_count, stock_count_line, stock_count_post_event, transfer, transfer_line, transfer_ship_event, transfer_receipt_line, transfer_receive_event, transfer_cancel_event, org_recovery_envelope -->
<!-- sync-authority-server-down: consent_acceptance, consent_event, privacy_request_case, privacy_request_event, privacy_tombstone, audit_checkpoint, authorization_lease, org_recovery_envelope, stored_value_instrument, customer, transfer, transfer_line, transfer_ship_event, transfer_receipt_line, transfer_receive_event, transfer_cancel_event, promotion_version, promotion_regulated_exclusion, promotion_publication, regulated_display_approval -->
<!-- sync-authority-bidirectional: customer, consent_event, privacy_request_case, privacy_request_event, audit_checkpoint, org_recovery_envelope, stored_value_instrument, transfer, transfer_line, transfer_ship_event, transfer_receipt_line, transfer_receive_event, transfer_cancel_event -->
<!-- sync-authority-server-origin: consent_acceptance, privacy_tombstone, authorization_lease, tax_filing_event, tax_period_adjustment, common_input_allocation, tax_credit_ledger, tax_filing_election, credit_note_period_assignment -->
<!-- sync-authority-local-only: sync_outbox, sync_cursor, user_session, auth_attempt_state, parked_cart, checkout_operation, product_quick_add_request, stock_adjustment_request, print_job, print_attempt, doc_sequence, stock_cache, shift_state, tender_status_current, stored_value_balance_cache, fiscal_queue, consent_current, privacy_request_current, loyalty_balance_cache, tax_filing_current, transfer_current, label_reprint_queue, fiscal_credentials_ref, trusted_time_state, refunded_qty_cache, product_fts, product_fts_map -->
<!-- postgres-server-only-tables: reprint_bundle -->
<!-- sync-authority-migration-only: sale_line_new, stage_product, stage_sale, stage_sale_line, stage_sale_tender, stage_sync_outbox, stage_sync_cursor, assert_stage_sync_outbox_empty -->
<!-- sync-reference-tables: org, store, register, category, tax_category, tax_rule_pack, tax_rate, tax_computation_policy, product, barcode, plu_code, embedded_barcode_rule, trade_scale, tender_type, capability, app_user, role, role_capability, user_role, receipt_template, setting, tile_grid, tile, cash_location, refund_policy, stored_value_policy_version, stored_value_policy_current, fiscal_spec_package, consent_notice, privacy_lawful_basis, loyalty_tax_policy_version, loyalty_tax_policy_current, price_list, price, promotion, supplier, tax_filing_profile, tax_filing_period -->

**The fact tables, and the rule that keeps them facts.** The machine-readable comment above is
the inventory. Each table refuses `UPDATE` and `DELETE` through a `BEFORE` trigger that
`RAISE(ABORT)`s — the sale-scoped ones once the parent sale is `completed`, the append-only
ones unconditionally. Asynchronous tender settlement and shift close are separate
`tender_status_event` and `shift_close_event` facts; the current states are rebuildable
projections, so neither operation reopens a completed financial row.

The `sync-authority-*` comments are the normative direction inventory. A table may
appear in both register-up and server-down when the server fans accepted facts to other registers;
`sync-reference-tables` are server-down by definition. The migration-only names do not survive the
migration transaction. Phase 3 microstep 3.1.2 extends the schema verifier and its committed negative
fixtures to subtract all listed classes from parsed persistent `CREATE TABLE` names, reject any
remainder, and reject a synced table whose foreign-key target is local-only. The current CI tree does
not yet enforce those two classification checks, so this inventory must not be represented as
machine-proven before 3.1.2. `print_attempt` remains local
printer evidence and contributes only scrubbed device-health aggregates; cross-register reprint uses
the server-only `reprint_bundle`, never a foreign key to another register's queue or print job.

At Phase 3 microstep 3.1.2, `register_up_facts_have_one_ready_commit` parses the register-up inventory
and anti-joins every fixture fact against `fact_commit_member` plus `sync_commit_ready`; zero or two
matches fail. The current fact-table tests do not perform that cross-inventory proof. Draft
sale/supply rows are checked when their completion/post event seals them, while independently
appendable transition facts refuse their own `INSERT` unless that ready member already exists. The
repository therefore creates `sync_commit`, all permanent members and all delivery rows first in the
same transaction; any later fact guard failure rolls the envelope back too. This order is safe because
membership names an entity id but does not claim that the entity exists until the transaction commits.

0002 shipped guards for three of them. Several later designs declared facts and left them writable —
`audit_log`'s own DDL asserted "Append-only: no UPDATE, no DELETE, ever" with nothing enforcing
it, which made the only forensic control in the design the one control an insider could edit.
The guards below close that.

**A fact table added by a later migration gets its triggers in the same migration, and a row in
`FACT_TABLES` in `crates/pos-db/tests/fact_table_guards.rs`, in the same commit.** That list is
the single source of truth; anything on it that lacks its guards fails the moment its migration
ships. Enumerating it from the schema instead would be better, and is not possible while
fact-ness is a design judgement rather than something the DDL records.

**Conventions applied throughout:** UUIDv7 as 16-byte `BLOB` primary keys · money as `INTEGER` minor units named `*_minor` · quantity as `INTEGER` milli-units named `*_milli` · rates as `INTEGER` parts-per-million named `*_ppm` · timestamps as ISO-8601 UTC `TEXT` named `*_at` · store-local trading days as `YYYY-MM-DD` `TEXT` named `*_date` · booleans as `INTEGER` 0/1 named `is_*` · soft-delete tombstones (`deleted_at`) on **reference** data only, never on facts.

| # | File | Adds | Phase |
|---|---|---|---|
| 0002 | `0002_sale_integrity.sql` | **shipped** — `sale_line.qty`→`qty_milli` (G-12), FK indexes, receipt-number uniqueness, I-4 immutability triggers | 1 |
| 0003 | `0003_strict_rebuild_and_catalog_depth.sql` | **rebuilds the six 0001/0002 tables as `STRICT`**, corrects two I-4 triggers, then adds stores, registers, categories, tax categories & rates, barcodes, settings | 1 |
| 0004 | `0004_people_and_audit.sql` | users, roles, capabilities, sessions, hash-chained audit log | 1 |
| 0005 | `0005_sale_columns_and_sequences.sql` | sale identity/training/rounding columns, the minimal shift skeleton, checkout recovery, receipt artifacts, tender transition facts, scoped counters | 1 |
| 0006 | `0006_stock_ledger.sql` | stock ledger + rebuildable on-hand/WAC cache | 1 |
| 0007 | `0007_search_and_seed.sql` | FTS5 index + triggers, price-embedded barcode rules | 1 |
| 0008 | `0008_shifts_and_cash.sql` | blind-close detail, cash locations and movements, counts, drawer events, Z reports | 2 |
| 0009 | `0009_refunds_and_returns.sql` | immutable refund/exchange links, restock decisions, refund policy, minimal store credit | 2 |
| 0010 | `0010_fiscal.sql` | fiscal queue, results, dead letters, reconciliation | 2 |
| 0011 | `0011_customers_loyalty.sql` | customers, consent notices/events, privacy tombstones, loyalty ledger, offline authorization leases | 3 |
| 0012 | `0012_pricing_promotions_supply.sql` | price lists, versioned promotions, supplier tax invoices, receipts, counts, transfers, filing periods | 4 |

Each migration is complete when its file is first committed. `0003` commits at 1.2.1 with the
structural tax vocabulary and no guessed rates. Microstep 1.3.7 later imports the reviewed,
source-backed jurisdiction pack through the versioned data path; it never edits `0003` to append
evidence that was not available when the migration committed.
`0004` owns the complete planned capability catalogue **and** the four standard roles with an
explicit decision for every one of their 128 (role, capability) cells; provisioning creates the
merchant's users and their `user_role` grants against those rows rather than inventing a role per
install. `0005` owns the complete tender-type catalogue with unsupported tenders inactive. A later microstep verifies, configures or consumes those rows; it never edits the earlier
migration to append them. The three Phase-4 workstreams consume one complete `0012` schema-spine
migration rather than successively reopening it.

---

## 0002 — sale integrity  ·  Phase 1, microstep 1.1.1  ·  SHIPPED

The historical heading remains unchanged so existing section links keep
resolving. [`00-master-plan.md`](../00-master-plan.md) §4a (Errata and
concordance) records that shipped migration `0002` is now owned by microstep
1.1.7; `1.1.1` is only the frozen heading inherited from the earlier schedule.

Two things 0001 got wrong, fixed together because the first destroys the second
if they are done in the other order.

**G-12 · `sale_line.qty` → `qty_milli`.** 0001 declared `qty INTEGER`,
contradicting I-3. This was originally scheduled for catalog depth, on the
grounds that the rebuilt table wants a `tax_category_id` and `tax_category` does
not exist until then. That was the wrong trade: SQLite accepts `ALTER TABLE …
ADD COLUMN … REFERENCES`, so catalog depth adds its columns without a second
rebuild, and the schema stops contradicting its own invariant immediately rather
than eventually. The data step multiplies by 1000 — existing rows are unit
counts — and is covered by `crates/pos-db/tests/migration_0002_qty_milli.rs`.

**I-4 has enforcement now.** Conventions §1 claimed a completed sale was
immutable "by review, by a `#[test]` that greps the repositories, and by the
absence of a repository method that could do it". Only the first existed, and it
is the weakest. Triggers hold it in the storage engine instead, which works
against repositories that have not been written yet.

The shipped tender exception is temporary history, not the target design: 0002 permits settlement
columns to move after completion, but 0003 rebuilds that table and closes the exception. From 0005,
settlement is a new `tender_status_event`; the tender's **amount, parent, method and identity never
move**.

```sql
-- The file is ordered rebuild → indexes → triggers, because a rebuild drops
-- every trigger and index attached to the old table.
CREATE TABLE sale_line_new (
  id               BLOB PRIMARY KEY,
  sale_id          BLOB NOT NULL REFERENCES sale(id),
  product_id       BLOB NOT NULL REFERENCES product(id),
  qty_milli        INTEGER NOT NULL,          -- G-12: 1 unit = 1000 (I-3)
  unit_price_minor INTEGER NOT NULL,
  discount_minor   INTEGER NOT NULL DEFAULT 0,
  tax_minor        INTEGER NOT NULL DEFAULT 0,
  total_minor      INTEGER NOT NULL
);
INSERT INTO sale_line_new
  (id, sale_id, product_id, qty_milli, unit_price_minor,
   discount_minor, tax_minor, total_minor)
SELECT id, sale_id, product_id, qty * 1000, unit_price_minor,
       discount_minor, tax_minor, total_minor
FROM sale_line;
DROP TABLE sale_line;
ALTER TABLE sale_line_new RENAME TO sale_line;

-- A receipt number identifies exactly one sale. Numbers come from a per-register
-- counter (0005), so uniqueness is per register: two registers legitimately both
-- print 000123.
CREATE UNIQUE INDEX idx_sale_receipt_number ON sale(register_id, receipt_number);

-- SQLite does not index a foreign key for you, and "the lines of this sale" is
-- what a reprint, a refund and every report do.
CREATE INDEX idx_sale_line_sale   ON sale_line(sale_id);
CREATE INDEX idx_sale_tender_sale ON sale_tender(sale_id);
```

The eight triggers are in `crates/pos-db/migrations/0002_sale_integrity.sql` and
enumerated in `REQUIRED_TRIGGERS` in the test above. **Any later migration that
rebuilds `sale`, `sale_line` or `sale_tender` must recreate them in that same
migration** — a rebuild takes its triggers with it, silently. That test is what
catches it.

---

## 0003 — strict rebuild and catalog depth  ·  Phase 1, microsteps 1.2.1–1.2.3  ·  SHIPPED

Introduces the organisational spine the whole schema hangs from. `store` and `register` must exist in Phase 1 even though multi-store is Phase 4 — retrofitting a `store_id` onto a live stock ledger is a data migration nobody enjoys.

```sql
-- ── Rebuilding the six shipped tables as STRICT ────────────────────────────
--
-- 0001 and 0002 created `product`, `sale`, `sale_line`, `sale_tender`,
-- `sync_outbox` and `sync_cursor` without STRICT, so today `total_minor` accepts
-- the string 'ten point five' and `sale.id` accepts NULL. Every other table in
-- this file is STRICT; these six are the ones a register actually opens, which
-- makes them the wrong six to leave loose. STRICT cannot be added by ALTER TABLE
-- and a committed migration is never edited, so the table has to be rebuilt, and
-- it is done here — first, before anything below points a foreign key at it.
-- Rebuilding a table with inbound references is far worse later.
--
-- WHY THE STAGING TABLES, rather than SQLite's documented twelve-step procedure:
-- that procedure begins by turning foreign keys off, and `PRAGMA foreign_keys`
-- is a no-op inside a transaction. The migration runner wraps every file in one,
-- and `open()` enables foreign keys before migrating. `PRAGMA defer_foreign_keys`
-- is not a substitute: DROP TABLE performs an implicit delete that records a
-- deferred violation, and re-creating the parent afterwards does not clear it —
-- the COMMIT fails. Verified both ways.
--
-- So: copy each table into an unconstrained staging table, drop the originals
-- children-first, create the STRICT replacements, copy back parents-first, drop
-- the staging tables, then restore the indexes and the triggers. Nothing
-- constrained holds a row pointing at a dropped parent at any point, so this
-- commits with foreign keys enforced. `CREATE TABLE ... AS SELECT` is what makes
-- the staging tables constraint-free.
--
-- One deliberate consequence: if a database already contains a row that violates
-- the STRICT types — a REAL in a `*_minor` column — the copy back refuses and the
-- migration fails, loudly, with nothing changed. That is the correct outcome. It
-- has never happened, because no register has shipped.

CREATE TABLE stage_product     AS SELECT * FROM product;
CREATE TABLE stage_sale        AS SELECT * FROM sale;
CREATE TABLE stage_sale_line   AS SELECT * FROM sale_line;
CREATE TABLE stage_sale_tender AS SELECT * FROM sale_tender;
CREATE TABLE stage_sync_outbox AS SELECT * FROM sync_outbox;
CREATE TABLE stage_sync_cursor AS SELECT * FROM sync_cursor;

-- Children before parents. Dropping the originals also drops their indexes and
-- the 0002 triggers, both restored below.
DROP TABLE sale_line;
DROP TABLE sale_tender;
DROP TABLE sale;
DROP TABLE product;
DROP TABLE sync_outbox;
DROP TABLE sync_cursor;

CREATE TABLE product (
  id            BLOB PRIMARY KEY,
  sku           TEXT NOT NULL UNIQUE,
  name          TEXT NOT NULL,
  price_minor   INTEGER NOT NULL,
  currency      TEXT NOT NULL,
  is_active     INTEGER NOT NULL DEFAULT 1,
  deleted_at    TEXT,
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version       INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE sale (
  id             BLOB PRIMARY KEY,
  receipt_number TEXT NOT NULL,
  register_id    BLOB NOT NULL,
  status         TEXT NOT NULL CHECK (status IN ('completed','voided','parked')),
  subtotal_minor INTEGER NOT NULL,
  tax_minor      INTEGER NOT NULL,
  total_minor    INTEGER NOT NULL,
  currency       TEXT NOT NULL,
  ref_sale_id    BLOB,
  business_date  TEXT NOT NULL,
  completed_at   TEXT NOT NULL
) STRICT;

CREATE TABLE sale_line (
  id               BLOB PRIMARY KEY,
  sale_id          BLOB NOT NULL REFERENCES sale(id),
  product_id       BLOB NOT NULL REFERENCES product(id),
  qty_milli        INTEGER NOT NULL,          -- G-12: 1 unit = 1000 (I-3)
  qty_step_milli   INTEGER NOT NULL DEFAULT 1000 CHECK (qty_step_milli > 0),
  unit_price_minor INTEGER NOT NULL,
  discount_minor   INTEGER NOT NULL DEFAULT 0,
  tax_minor        INTEGER NOT NULL DEFAULT 0,
  total_minor      INTEGER NOT NULL,
  -- A discrete `each`/`package` line snapshots 1000. Weighed goods snapshot the
  -- configured smaller step. The database refuses one-thousandth of a can.
  CHECK (qty_milli % qty_step_milli = 0)
) STRICT;

CREATE TABLE sale_tender (
  id           BLOB PRIMARY KEY,
  sale_id      BLOB NOT NULL REFERENCES sale(id),
  method       TEXT NOT NULL,
  amount_minor INTEGER NOT NULL,
  psp_ref      TEXT,
  change_minor INTEGER NOT NULL DEFAULT 0
) STRICT;

-- One durable envelope per local business transaction. Member rows cannot
-- disagree about versions, cardinality or the canonical group hash because
-- those values live once on the parent rather than being repeated on each row.
CREATE TABLE sync_commit (
  id               BLOB PRIMARY KEY,
  commit_size      INTEGER NOT NULL CHECK (commit_size > 0),
  commit_hash      TEXT NOT NULL,
  protocol_version INTEGER NOT NULL CHECK (protocol_version > 0),
  schema_version   INTEGER NOT NULL CHECK (schema_version > 0),
  producer_version TEXT NOT NULL,
  created_at       TEXT NOT NULL
) STRICT;

-- Permanent membership is separate from delivery state. Acknowledged outbox
-- rows may be pruned, but the commit manifest and canonical bytes remain as
-- financial evidence and as the convergence oracle. Otherwise routine queue
-- retention would make it impossible to prove what the register committed.
CREATE TABLE fact_commit_member (
  change_id        BLOB PRIMARY KEY,
  commit_id        BLOB NOT NULL REFERENCES sync_commit(id),
  commit_index     INTEGER NOT NULL CHECK (commit_index >= 0),
  entity           TEXT NOT NULL,
  entity_id        BLOB NOT NULL,
  op               TEXT NOT NULL CHECK (op = 'insert'),
  payload          TEXT NOT NULL,
  payload_hash     TEXT NOT NULL,
  created_at       TEXT NOT NULL,
  UNIQUE (commit_id, commit_index),
  UNIQUE (entity, entity_id)
) STRICT;

CREATE TABLE sync_outbox (
  seq              INTEGER PRIMARY KEY AUTOINCREMENT,
  change_id        BLOB NOT NULL UNIQUE REFERENCES fact_commit_member(change_id),
  state            TEXT NOT NULL DEFAULT 'pending'
                     CHECK (state IN ('pending','in_flight','retry','acknowledged','dead')),
  attempts         INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  next_attempt_at  TEXT,
  claimed_at       TEXT,
  lease_owner      TEXT,
  lease_expires_at TEXT,
  acknowledged_at  TEXT,
  last_error       TEXT,
  created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  pushed_at        TEXT,
  CHECK ((state = 'acknowledged') = (acknowledged_at IS NOT NULL)),
  CHECK ((state = 'in_flight') = (claimed_at IS NOT NULL)),
  CHECK ((state = 'in_flight') = (lease_owner IS NOT NULL)),
  CHECK ((state = 'in_flight') = (lease_expires_at IS NOT NULL))
) STRICT;

CREATE TRIGGER fact_commit_member_within_commit
BEFORE INSERT ON fact_commit_member
WHEN NEW.commit_index >= (SELECT commit_size FROM sync_commit WHERE id = NEW.commit_id)
BEGIN
  SELECT RAISE(ABORT, 'a fact member index must fit its immutable commit envelope');
END;

CREATE VIEW fact_commit_complete AS
SELECT c.id
  FROM sync_commit c JOIN fact_commit_member m ON m.commit_id = c.id
 GROUP BY c.id, c.commit_size
HAVING COUNT(*) = c.commit_size
   AND MIN(m.commit_index) = 0
   AND MAX(m.commit_index) = c.commit_size - 1;

-- Checkout uses this stricter view: every permanent member must also have its
-- delivery row before the financial transaction may complete. After a durable
-- acknowledgement the delivery row may be pruned; `fact_commit_complete`
-- remains the historical proof and convergence source.
CREATE VIEW sync_commit_ready AS
SELECT c.id
  FROM sync_commit c
  JOIN fact_commit_member m ON m.commit_id = c.id
  JOIN sync_outbox o ON o.change_id = m.change_id
 GROUP BY c.id, c.commit_size
HAVING COUNT(*) = c.commit_size
   AND MIN(m.commit_index) = 0
   AND MAX(m.commit_index) = c.commit_size - 1;

CREATE TRIGGER sync_commit_no_update
BEFORE UPDATE ON sync_commit BEGIN
  SELECT RAISE(ABORT, 'a durable sync commit envelope is immutable');
END;
CREATE TRIGGER sync_commit_no_delete
BEFORE DELETE ON sync_commit
WHEN EXISTS (SELECT 1 FROM fact_commit_member m WHERE m.commit_id = OLD.id)
BEGIN
  SELECT RAISE(ABORT, 'a sync commit cannot be pruned while it still owns members');
END;
CREATE TRIGGER fact_commit_member_no_update
BEFORE UPDATE ON fact_commit_member
BEGIN
  SELECT RAISE(ABORT, 'a fact commit member and its canonical bytes are immutable');
END;
CREATE TRIGGER fact_commit_member_no_delete
BEFORE DELETE ON fact_commit_member
BEGIN
  SELECT RAISE(ABORT, 'fact membership survives outbox acknowledgement and pruning');
END;
CREATE TRIGGER sync_outbox_change_frozen
BEFORE UPDATE ON sync_outbox
WHEN NEW.seq IS NOT OLD.seq OR NEW.change_id IS NOT OLD.change_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN
  SELECT RAISE(ABORT, 'sync retries may change delivery state, never fact identity');
END;
CREATE TRIGGER sync_outbox_prune_acknowledged_only
BEFORE DELETE ON sync_outbox
WHEN OLD.state <> 'acknowledged'
BEGIN
  SELECT RAISE(ABORT, 'only durably acknowledged outbox rows may be pruned');
END;

CREATE TABLE sync_cursor (
  entity         TEXT PRIMARY KEY,
  server_version INTEGER NOT NULL DEFAULT 0
) STRICT;

-- Parents before children, so the foreign keys hold at every step.
INSERT INTO product     SELECT id, sku, name, price_minor, currency, is_active,
                               deleted_at, updated_at, version FROM stage_product;
INSERT INTO sale        SELECT id, receipt_number, register_id, status, subtotal_minor,
                               tax_minor, total_minor, currency, ref_sale_id,
                               business_date, completed_at FROM stage_sale;
INSERT INTO sale_line   SELECT id, sale_id, product_id, qty_milli, 1000, unit_price_minor,
                               discount_minor, tax_minor, total_minor FROM stage_sale_line;
INSERT INTO sale_tender SELECT id, sale_id, method, amount_minor, psp_ref,
                               change_minor FROM stage_sale_tender;
-- Phase 0 has never traded or synced. A pre-protocol transport row has no
-- deterministic canonical envelope, so inventing random ids or sentinel hashes
-- would turn a migration placeholder into apparently valid financial evidence.
CREATE TABLE assert_stage_sync_outbox_empty (
  row_count INTEGER NOT NULL CHECK (row_count = 0)
) STRICT;
INSERT INTO assert_stage_sync_outbox_empty
SELECT COUNT(*) FROM stage_sync_outbox;
DROP TABLE assert_stage_sync_outbox_empty;
INSERT INTO sync_cursor SELECT entity, server_version FROM stage_sync_cursor;

DROP TABLE stage_product;
DROP TABLE stage_sale;
DROP TABLE stage_sale_line;
DROP TABLE stage_sale_tender;
DROP TABLE stage_sync_outbox;
DROP TABLE stage_sync_cursor;

-- The indexes 0001 and 0002 created on these tables.
CREATE INDEX        idx_sale_business_date   ON sale(business_date);
CREATE INDEX        idx_outbox_unpushed      ON sync_outbox(state, next_attempt_at, seq)
  WHERE state IN ('pending','retry');
CREATE INDEX        idx_outbox_expired_lease ON sync_outbox(lease_expires_at, seq)
  WHERE state = 'in_flight';
CREATE UNIQUE INDEX idx_sale_receipt_number  ON sale(register_id, receipt_number);
CREATE INDEX        idx_sale_line_sale       ON sale_line(sale_id);
CREATE INDEX        idx_sale_tender_sale     ON sale_tender(sale_id);

-- ── The I-4 triggers, restored — and two of them corrected ─────────────────
--
-- Dropping the tables dropped 0002's triggers, so they are recreated here. Two
-- come back with a fix rather than a copy: 0002's UPDATE guards on `sale_line`
-- and `sale_tender` tested the OLD parent only, which refused an edit to a row
-- already on a completed sale and permitted the inbound move — take a row
-- belonging to a PARKED sale and re-point its `sale_id` at a completed one. The
-- closed document grows a line, or gains a tender, and not one protected row was
-- edited. Reproduced against the shipped chain: a completed sale went from one
-- line to two.
--
-- `sale_tender` looked protected because its guard already compared
-- `NEW.sale_id <> OLD.sale_id`, but that comparison sat behind the same
-- OLD-parent WHEN, so it only ever blocked moves OUT of a completed sale.

CREATE TRIGGER sale_no_update_once_completed
BEFORE UPDATE ON sale
WHEN OLD.status = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a completed sale is immutable — issue a correcting document');
END;

CREATE TRIGGER sale_no_delete_once_completed
BEFORE DELETE ON sale
WHEN OLD.status = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a completed sale cannot be deleted — issue a correcting document');
END;

CREATE TRIGGER sale_line_no_insert_once_completed
BEFORE INSERT ON sale_line
WHEN (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: cannot add a line to a completed sale');
END;

CREATE TRIGGER sale_line_no_update_once_completed
BEFORE UPDATE ON sale_line
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
  OR (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a line of a completed sale is immutable');
END;

CREATE TRIGGER sale_line_no_delete_once_completed
BEFORE DELETE ON sale_line
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a line of a completed sale cannot be deleted');
END;

CREATE TRIGGER sale_tender_no_insert_once_completed
BEFORE INSERT ON sale_tender
WHEN (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: cannot add a tender to a completed sale');
END;

CREATE TRIGGER sale_tender_no_delete_once_completed
BEFORE DELETE ON sale_tender
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a tender of a completed sale cannot be deleted');
END;

CREATE TRIGGER sale_tender_no_update_once_completed
BEFORE UPDATE ON sale_tender
-- 0002 allowed settlement-column updates. From 0005, settlement is an append-only
-- `tender_status_event`, so a completed tender has no mutable exception. BOTH
-- parents are checked so a parked tender cannot be reparented into a closed sale.
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
   OR (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a tender on a completed sale is immutable — append a status event');
END;

-- ── Organisation ───────────────────────────────────────────────────────────
CREATE TABLE org (
  id            BLOB PRIMARY KEY,
  legal_name    TEXT    NOT NULL,
  tin           TEXT,                     -- tax number, printed on every receipt (B.6)
  deleted_at    TEXT,
  updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version       INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE store (
  id                 BLOB PRIMARY KEY,
  org_id             BLOB NOT NULL REFERENCES org(id),
  code               TEXT NOT NULL UNIQUE,
  name_ar            TEXT NOT NULL,
  name_en            TEXT,
  address            TEXT,
  phone              TEXT,
  currency           TEXT NOT NULL DEFAULT 'JOD',
  -- Jurisdiction, GST registration and JoFotara obligation are independent
  -- axes. A below-GST-threshold income-tax taxpayer may still need JoFotara;
  -- assortment never decides either status.
  tax_profile        TEXT NOT NULL DEFAULT 'standard'
                       CHECK (tax_profile IN ('standard','asez','development_area')),
  gst_profile        TEXT NOT NULL DEFAULT 'unregistered'
                       CHECK (gst_profile IN ('unregistered','general_sales','special_sales')),
  fiscal_obligation  TEXT NOT NULL DEFAULT 'pending_evidence'
                       CHECK (fiscal_obligation IN ('pending_evidence','required','exempt')),
  fiscal_taxpayer_type TEXT
                       CHECK (fiscal_taxpayer_type IN ('income','general_sales','special_sales')),
  fiscal_obligation_evidence_ref TEXT,
  price_mode         TEXT NOT NULL DEFAULT 'inclusive'
                       CHECK (price_mode IN ('inclusive','exclusive')),
  -- Technical output mode. It does not decide legal obligation; enablement and
  -- exemption both require the evidence fields above.
  fiscal_profile     TEXT NOT NULL DEFAULT 'disabled'
                       CHECK (fiscal_profile IN ('disabled','jordan_jofotara')),
  time_zone          TEXT NOT NULL DEFAULT 'Asia/Amman',
  day_cutover_minutes INTEGER NOT NULL DEFAULT 240,  -- 04:00 local (conventions §11)
  currency_exponent  INTEGER NOT NULL DEFAULT 3 CHECK (currency_exponent = 3),
  -- JOD values render at their three-decimal exponent. Shorter display can
  -- hide fils that settlement still charges, so it is not a store preference.
  catalog_display_decimals INTEGER NOT NULL DEFAULT 3
                       CHECK (catalog_display_decimals = 3),
  allow_negative_stock INTEGER NOT NULL DEFAULT 1,   -- allow-and-flag default (C.7)
  receipt_locale     TEXT NOT NULL DEFAULT 'ar',
  deleted_at         TEXT,
  updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version            INTEGER NOT NULL DEFAULT 0,
  CHECK (fiscal_obligation = 'pending_evidence'
         OR fiscal_obligation_evidence_ref IS NOT NULL)
) STRICT;

CREATE TABLE register (
  id           BLOB PRIMARY KEY,
  store_id     BLOB NOT NULL REFERENCES store(id),
  code         TEXT NOT NULL,             -- 'REG01' → receipt prefix
  name         TEXT NOT NULL,
  device_id    TEXT NOT NULL UNIQUE,      -- provisioned UUID, never a hardware fingerprint
  credential_key_id TEXT NOT NULL UNIQUE,
  credential_algorithm TEXT NOT NULL,
  credential_public_key BLOB NOT NULL,
  credential_issued_at TEXT NOT NULL,
  hardware_fingerprint_hash BLOB,         -- anomaly signal only; never authentication
  is_active    INTEGER NOT NULL DEFAULT 1,
  deleted_at   TEXT,
  updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version      INTEGER NOT NULL DEFAULT 0,
  UNIQUE (store_id, code)
) STRICT;

-- ── Taxonomy ───────────────────────────────────────────────────────────────
CREATE TABLE category (
  id          BLOB PRIMARY KEY,
  parent_id   BLOB REFERENCES category(id),
  name_ar     TEXT NOT NULL,
  name_en     TEXT,
  sort_order  INTEGER NOT NULL DEFAULT 0,
  deleted_at  TEXT,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version     INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE tax_category (
  id          BLOB PRIMARY KEY,
  code        TEXT NOT NULL UNIQUE,       -- 'STD16','EXEMPT','ZERO','RED04'
  name_ar     TEXT NOT NULL,
  name_en     TEXT,
  treatment   TEXT NOT NULL CHECK (treatment IN ('standard','reduced','zero','exempt')),
  deleted_at  TEXT,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version     INTEGER NOT NULL DEFAULT 0
) STRICT;

-- A non-standard jurisdiction never inherits a generic standard rule. The pack
-- is complete, effective-dated evidence reviewed for that profile; a store whose
-- pack is absent stays blocked rather than silently charging the standard rate.
CREATE TABLE tax_rule_pack (
  id             BLOB PRIMARY KEY,
  profile_scope  TEXT NOT NULL
                   CHECK (profile_scope IN ('standard','asez','development_area')),
  pack_version   TEXT NOT NULL,
  source_ref     TEXT NOT NULL,
  content_hash   BLOB NOT NULL,
  approved_by    BLOB,
  approved_at    TEXT,
  retired_at     TEXT,
  status         TEXT NOT NULL DEFAULT 'pending'
                   CHECK (status IN ('pending','approved','retired')),
  UNIQUE (profile_scope, pack_version),
  CHECK ((status = 'pending' AND approved_by IS NULL AND approved_at IS NULL AND retired_at IS NULL)
      OR (status = 'approved' AND approved_by IS NOT NULL AND approved_at IS NOT NULL AND retired_at IS NULL)
      OR (status = 'retired' AND approved_by IS NOT NULL AND approved_at IS NOT NULL AND retired_at IS NOT NULL))
) STRICT;
ALTER TABLE store ADD COLUMN tax_rule_pack_id BLOB REFERENCES tax_rule_pack(id);

CREATE TRIGGER tax_rule_pack_freeze_after_approval
BEFORE UPDATE ON tax_rule_pack
WHEN NEW.id IS NOT OLD.id OR NOT (
  (OLD.status = 'pending' AND NEW.status IN ('pending','approved'))
  OR
  (OLD.status = 'approved' AND NEW.status = 'retired'
    AND NEW.profile_scope IS OLD.profile_scope
    AND NEW.pack_version IS OLD.pack_version
    AND NEW.source_ref IS OLD.source_ref
    AND NEW.content_hash IS OLD.content_hash
    AND NEW.approved_by IS OLD.approved_by
    AND NEW.approved_at IS OLD.approved_at
    AND NEW.retired_at IS NOT NULL))
BEGIN
  SELECT RAISE(ABORT, 'an approved tax pack is immutable; retire it or create a new version');
END;
CREATE TRIGGER tax_rule_pack_no_delete
BEFORE DELETE ON tax_rule_pack BEGIN
  SELECT RAISE(ABORT, 'a source-hashed tax pack cannot be deleted');
END;
CREATE TRIGGER tax_rule_pack_retire_after_reassignment
BEFORE UPDATE OF status ON tax_rule_pack
WHEN NEW.status = 'retired'
 AND EXISTS (SELECT 1 FROM store st WHERE st.tax_rule_pack_id = OLD.id)
BEGIN
  SELECT RAISE(ABORT, 'reassign every store before retiring its tax rule pack');
END;

-- Tax rounding is jurisdiction policy, not a merchant preference. The policy
-- is versioned and source-backed so changing it cannot rewrite historical sale
-- arithmetic; cash rounding remains a separate settlement rule on `store`.
CREATE TABLE tax_computation_policy (
  id               BLOB PRIMARY KEY,
  jurisdiction     TEXT NOT NULL,
  policy_version   TEXT NOT NULL,
  rounding_rule    TEXT NOT NULL
                      CHECK (rounding_rule IN ('half_away_from_zero','half_even','floor','ceil')),
  cash_round_step_minor INTEGER NOT NULL CHECK (cash_round_step_minor > 0),
  cash_round_direction TEXT NOT NULL CHECK (cash_round_direction IN ('nearest','up','down')),
  cash_round_tax_treatment TEXT NOT NULL,
  source_ref       TEXT NOT NULL,
  content_hash     BLOB NOT NULL,
  approved_at      TEXT,
  UNIQUE (jurisdiction, policy_version)
) STRICT;
ALTER TABLE store ADD COLUMN tax_computation_policy_id BLOB
  REFERENCES tax_computation_policy(id);

CREATE TRIGGER store_tax_pack_complete_insert
BEFORE INSERT ON store
WHEN NEW.tax_rule_pack_id IS NULL OR NOT EXISTS (
       SELECT 1 FROM tax_rule_pack p
        WHERE p.id = NEW.tax_rule_pack_id
          AND p.profile_scope = NEW.tax_profile
          AND p.status = 'approved')
BEGIN
  SELECT RAISE(ABORT, 'every store tax profile requires its approved rule pack');
END;
CREATE TRIGGER store_tax_pack_complete_update
BEFORE UPDATE OF tax_profile, tax_rule_pack_id ON store
WHEN NEW.tax_rule_pack_id IS NULL OR NOT EXISTS (
       SELECT 1 FROM tax_rule_pack p
        WHERE p.id = NEW.tax_rule_pack_id
          AND p.profile_scope = NEW.tax_profile
          AND p.status = 'approved')
BEGIN
  SELECT RAISE(ABORT, 'every store tax profile requires its approved rule pack');
END;

CREATE TRIGGER store_fiscal_evidence_consistent_insert
BEFORE INSERT ON store
WHEN (NEW.fiscal_obligation = 'required' AND (
       NEW.fiscal_profile <> 'jordan_jofotara'
       OR NEW.fiscal_taxpayer_type IS NULL
       OR NEW.fiscal_obligation_evidence_ref IS NULL))
  OR (NEW.fiscal_obligation = 'exempt' AND (
       NEW.fiscal_profile <> 'disabled'
       OR NEW.fiscal_obligation_evidence_ref IS NULL))
  OR (NEW.fiscal_obligation = 'pending_evidence'
       AND NEW.fiscal_profile <> 'disabled')
BEGIN
  SELECT RAISE(ABORT, 'fiscal enablement or exemption must match merchant-specific evidence');
END;
CREATE TRIGGER store_fiscal_evidence_consistent_update
BEFORE UPDATE OF fiscal_obligation, fiscal_profile, fiscal_taxpayer_type,
                 fiscal_obligation_evidence_ref ON store
WHEN (NEW.fiscal_obligation = 'required' AND (
       NEW.fiscal_profile <> 'jordan_jofotara'
       OR NEW.fiscal_taxpayer_type IS NULL
       OR NEW.fiscal_obligation_evidence_ref IS NULL))
  OR (NEW.fiscal_obligation = 'exempt' AND (
       NEW.fiscal_profile <> 'disabled'
       OR NEW.fiscal_obligation_evidence_ref IS NULL))
  OR (NEW.fiscal_obligation = 'pending_evidence'
       AND NEW.fiscal_profile <> 'disabled')
BEGIN
  SELECT RAISE(ABORT, 'fiscal enablement or exemption must match merchant-specific evidence');
END;

CREATE TRIGGER store_tax_policy_complete_insert
BEFORE INSERT ON store
WHEN NEW.tax_computation_policy_id IS NULL OR NOT EXISTS (
  SELECT 1 FROM tax_computation_policy p
   WHERE p.id = NEW.tax_computation_policy_id AND p.approved_at IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'a store requires an approved jurisdiction computation policy');
END;
CREATE TRIGGER store_tax_policy_complete_update
BEFORE UPDATE OF tax_computation_policy_id ON store
WHEN NEW.tax_computation_policy_id IS NULL OR NOT EXISTS (
  SELECT 1 FROM tax_computation_policy p
   WHERE p.id = NEW.tax_computation_policy_id AND p.approved_at IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'a store requires an approved jurisdiction computation policy');
END;

CREATE TRIGGER tax_computation_policy_no_update
BEFORE UPDATE ON tax_computation_policy BEGIN
  SELECT RAISE(ABORT, 'a jurisdiction computation policy is immutable — create a new version');
END;
CREATE TRIGGER tax_computation_policy_no_delete
BEFORE DELETE ON tax_computation_policy BEGIN
  SELECT RAISE(ABORT, 'a computation policy used by financial facts cannot be deleted');
END;

-- Rates are DATA with effective dates. Jordan changes reduced rates by Cabinet
-- decree; a rate in code is a re-release (master plan B.1).
-- A component is a discriminated union: ad-valorem or fixed per quantity. The
-- ordering/base snapshot represents tax-on-prior-component arithmetic without
-- hard-coding a Jordanian rate or assuming every component shares one base.
-- `line_net_plus_prior_components` means every component with a lower
-- `calculation_order`; there is no dangling component-code dependency that can
-- self-reference or point at a component outside this rule pack/category.
CREATE TABLE tax_rate (
  id               BLOB PRIMARY KEY,
  rule_pack_id     BLOB NOT NULL REFERENCES tax_rule_pack(id),
  tax_category_id  BLOB NOT NULL REFERENCES tax_category(id),
  component_code   TEXT NOT NULL DEFAULT 'GST',
  treatment        TEXT NOT NULL CHECK (treatment IN ('standard','reduced','zero','exempt')),
  calculation_kind TEXT NOT NULL
                       CHECK (calculation_kind IN ('ad_valorem','fixed_per_quantity')),
  rate_ppm         INTEGER CHECK (rate_ppm >= 0),      -- 16% = 160000
  fixed_amount_minor INTEGER,
  fixed_currency   TEXT,
  fixed_basis_qty_milli INTEGER,
  calculation_order INTEGER NOT NULL DEFAULT 0,
  base_kind        TEXT NOT NULL
                       CHECK (base_kind IN ('line_net','line_net_plus_prior_components','quantity')),
  valid_from       TEXT NOT NULL,         -- inclusive
  valid_to         TEXT,                  -- exclusive; NULL = open
  profile_scope    TEXT NOT NULL DEFAULT 'standard'
                       CHECK (profile_scope IN ('standard','asez','development_area')),
  deleted_at       TEXT,
  updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version          INTEGER NOT NULL DEFAULT 0,
  CHECK (valid_to IS NULL OR valid_to > valid_from),
  CHECK (
    (calculation_kind = 'ad_valorem'
      AND rate_ppm IS NOT NULL
      AND fixed_amount_minor IS NULL
      AND fixed_currency IS NULL
      AND fixed_basis_qty_milli IS NULL
      AND base_kind IN ('line_net','line_net_plus_prior_components'))
    OR
    (calculation_kind = 'fixed_per_quantity'
      AND rate_ppm IS NULL
      AND fixed_amount_minor > 0
      AND fixed_currency IS NOT NULL
      AND fixed_basis_qty_milli > 0
      AND base_kind = 'quantity')
  )
) STRICT;
CREATE INDEX idx_tax_rate_lookup
  ON tax_rate(rule_pack_id, tax_category_id, component_code, valid_from);
CREATE UNIQUE INDEX idx_tax_rate_component_order
  ON tax_rate(rule_pack_id, tax_category_id, calculation_order, valid_from);

CREATE TRIGGER tax_rate_order_no_overlap_insert
BEFORE INSERT ON tax_rate
WHEN EXISTS (
  SELECT 1 FROM tax_rate r
   WHERE r.rule_pack_id = NEW.rule_pack_id
     AND r.tax_category_id = NEW.tax_category_id
     AND r.calculation_order = NEW.calculation_order
     AND COALESCE(r.valid_to, '9999-12-31') > NEW.valid_from
     AND COALESCE(NEW.valid_to, '9999-12-31') > r.valid_from)
BEGIN
  SELECT RAISE(ABORT, 'a tax component order is unique at every effective instant');
END;
CREATE TRIGGER tax_rate_order_no_overlap_update
BEFORE UPDATE ON tax_rate
WHEN EXISTS (
  SELECT 1 FROM tax_rate r
   WHERE r.id <> OLD.id
     AND r.rule_pack_id = NEW.rule_pack_id
     AND r.tax_category_id = NEW.tax_category_id
     AND r.calculation_order = NEW.calculation_order
     AND COALESCE(r.valid_to, '9999-12-31') > NEW.valid_from
     AND COALESCE(NEW.valid_to, '9999-12-31') > r.valid_from)
BEGIN
  SELECT RAISE(ABORT, 'a tax component order is unique at every effective instant');
END;

CREATE TRIGGER tax_rate_pack_scope_insert
BEFORE INSERT ON tax_rate
WHEN NOT EXISTS (
  SELECT 1 FROM tax_rule_pack p
   WHERE p.id = NEW.rule_pack_id AND p.profile_scope = NEW.profile_scope)
BEGIN
  SELECT RAISE(ABORT, 'a tax rate must belong to the rule pack for its profile');
END;
CREATE TRIGGER tax_rate_pack_scope_update
BEFORE UPDATE OF rule_pack_id, profile_scope ON tax_rate
WHEN NOT EXISTS (
  SELECT 1 FROM tax_rule_pack p
   WHERE p.id = NEW.rule_pack_id AND p.profile_scope = NEW.profile_scope)
BEGIN
  SELECT RAISE(ABORT, 'a tax rate must belong to the rule pack for its profile');
END;

CREATE TRIGGER tax_rate_pack_open_insert
BEFORE INSERT ON tax_rate
WHEN (SELECT status FROM tax_rule_pack WHERE id = NEW.rule_pack_id) <> 'pending'
BEGIN
  SELECT RAISE(ABORT, 'rates are assembled before their pack is approved');
END;
CREATE TRIGGER tax_rate_pack_open_update
BEFORE UPDATE ON tax_rate
WHEN (SELECT status FROM tax_rule_pack WHERE id = OLD.rule_pack_id) <> 'pending'
  OR (SELECT status FROM tax_rule_pack WHERE id = NEW.rule_pack_id) <> 'pending'
BEGIN
  SELECT RAISE(ABORT, 'an approved tax-pack member is immutable');
END;
CREATE TRIGGER tax_rate_pack_open_delete
BEFORE DELETE ON tax_rate
WHEN (SELECT status FROM tax_rule_pack WHERE id = OLD.rule_pack_id) <> 'pending'
BEGIN
  SELECT RAISE(ABORT, 'an approved tax-pack member cannot be deleted');
END;

-- ── Product depth ──────────────────────────────────────────────────────────
ALTER TABLE product ADD COLUMN name_ar          TEXT;
ALTER TABLE product ADD COLUMN name_en          TEXT;
ALTER TABLE product ADD COLUMN category_id      BLOB REFERENCES category(id);
ALTER TABLE product ADD COLUMN tax_category_id  BLOB REFERENCES tax_category(id);
ALTER TABLE product ADD COLUMN unit             TEXT NOT NULL DEFAULT 'each'
  CHECK (unit IN ('each','package','weight','volume','length'));
ALTER TABLE product ADD COLUMN qty_step_milli   INTEGER NOT NULL DEFAULT 1000
  CHECK (qty_step_milli > 0);
ALTER TABLE product ADD COLUMN is_weighed       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE product ADD COLUMN is_service       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE product ADD COLUMN regulated_kind   TEXT
  CHECK (regulated_kind IN ('tobacco'));
ALTER TABLE product ADD COLUMN sale_form        TEXT NOT NULL DEFAULT 'sealed_pack'
  CHECK (sale_form IN ('sealed_pack','bulk','service'));
ALTER TABLE product ADD COLUMN min_age          INTEGER;         -- E.69
ALTER TABLE product ADD COLUMN max_price_minor  INTEGER;         -- ministry ceiling (J.3, E.71)
ALTER TABLE product ADD COLUMN reorder_point_milli INTEGER;
UPDATE product SET name_ar = name WHERE name_ar IS NULL;

CREATE TRIGGER product_regulated_sale_form_insert
BEFORE INSERT ON product
WHEN NEW.regulated_kind = 'tobacco' AND NEW.sale_form <> 'sealed_pack'
BEGIN
  SELECT RAISE(ABORT, 'regulated tobacco products must be sold as sealed packs');
END;

CREATE TRIGGER product_regulated_sale_form_update
BEFORE UPDATE OF regulated_kind, sale_form ON product
WHEN NEW.regulated_kind = 'tobacco' AND NEW.sale_form <> 'sealed_pack'
BEGIN
  SELECT RAISE(ABORT, 'regulated tobacco products must be sold as sealed packs');
END;

CREATE TRIGGER product_quantity_kind_insert
BEFORE INSERT ON product
WHEN (NEW.unit IN ('each','package') AND (NEW.qty_step_milli <> 1000 OR NEW.is_weighed <> 0))
  OR (NEW.unit IN ('weight','volume','length') AND NEW.is_weighed <> 1)
BEGIN
  SELECT RAISE(ABORT, 'discrete products use 1000-milli steps; measured goods are marked weighed');
END;

CREATE TRIGGER product_quantity_kind_update
BEFORE UPDATE OF unit, qty_step_milli, is_weighed ON product
WHEN (NEW.unit IN ('each','package') AND (NEW.qty_step_milli <> 1000 OR NEW.is_weighed <> 0))
  OR (NEW.unit IN ('weight','volume','length') AND NEW.is_weighed <> 1)
BEGIN
  SELECT RAISE(ABORT, 'discrete products use 1000-milli steps; measured goods are marked weighed');
END;

CREATE TRIGGER product_active_requires_tax_category_insert
BEFORE INSERT ON product
WHEN NEW.is_active = 1 AND NEW.deleted_at IS NULL AND NEW.tax_category_id IS NULL
BEGIN
  SELECT RAISE(ABORT, 'an active product requires a configured tax category');
END;
CREATE TRIGGER product_active_requires_tax_category_update
BEFORE UPDATE OF is_active, deleted_at, tax_category_id ON product
WHEN NEW.is_active = 1 AND NEW.deleted_at IS NULL AND NEW.tax_category_id IS NULL
BEGIN
  SELECT RAISE(ABORT, 'an active product requires a configured tax category');
END;

-- ── Arabic search folding ──────────────────────────────────────────────────
--
-- `unicode61 remove_diacritics 2` folds LATIN diacritics only. Arabic tashkeel
-- are treated as token separators, so "قَهْوَة" indexes as four single-letter
-- tokens and a search for "قهوة" finds nothing. Verified on SQLite 3.51.
--
-- Search is the fallback for every unbarcoded item — produce, bakery, damaged
-- labels — and the only path a cashier has when the scanner fails, so this is
-- not cosmetic. The fold is a generated column so the expression exists ONCE;
-- the 0007 triggers index it and never restate it.
--
-- VIRTUAL, not STORED: `name_ar` itself arrives by ALTER above, and a generated
-- column added by ALTER TABLE is only dependably VIRTUAL. Nothing is stored —
-- the tokens live in the FTS index, and this column is only read to build them.
--
-- Stripped (removed entirely):
--     U+064B  fathatan
--     U+064C  dammatan
--     U+064D  kasratan
--     U+064E  fatha
--     U+064F  damma
--     U+0650  kasra
--     U+0651  shadda
--     U+0652  sukun
--     U+0653  maddah above
--     U+0654  hamza above
--     U+0655  hamza below
--     U+0670  superscript alef
--     U+0640  tatweel
-- Mapped (spelling variants that must collide):
--     U+0623 -> U+0627  alef hamza above -> alef
--     U+0625 -> U+0627  alef hamza below -> alef
--     U+0622 -> U+0627  alef maddah -> alef
--     U+0671 -> U+0627  alef wasla -> alef
--     U+0649 -> U+064A  alef maqsura -> yaa
--     U+0629 -> U+0647  taa marbuta -> haa
--     U+0624 -> U+0648  waw hamza -> waw
--     U+0626 -> U+064A  yaa hamza -> yaa
--
-- THE QUERY MUST BE FOLDED THE SAME WAY. A folded index searched with an
-- unfolded string still returns zero rows — that is the trap this whole column
-- exists to close. `prop_sql_and_rust_folding_agree` pins the two together.
-- TWO derived columns, because recall and precision are different jobs.
--
-- `name_ar_exact` strips only the marks nobody types into a search box: tashkeel
-- and tatweel. Spelling is PRESERVED — ة stays ة, أ stays أ. This is the column
-- that lets the spelling a cashier actually typed outrank a near-miss.
--
-- `name_ar_fold` below goes further and collapses the spelling variants too. That
-- is recall: it finds لبنه when you typed لبنة. On its own it also *destroys*
-- precision, because once the query is folded, both spellings look identical and
-- the exact match has no advantage left — the row the cashier meant can sort
-- second. Searching both columns fixes that for free: the exactly-spelled row
-- matches BOTH branches and a variant-only row matches one, so BM25 puts the
-- exact match first without any manual score arithmetic.
ALTER TABLE product ADD COLUMN name_ar_exact TEXT
  GENERATED ALWAYS AS (replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(name_ar,char(1611),''),char(1612),''),char(1613),''),char(1614),''),char(1615),''),char(1616),''),char(1617),''),char(1618),''),char(1619),''),char(1620),''),char(1621),''),char(1648),''),char(1600),'')) VIRTUAL;

ALTER TABLE product ADD COLUMN name_ar_fold TEXT
  GENERATED ALWAYS AS (replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(name_ar,char(1611),''),char(1612),''),char(1613),''),char(1614),''),char(1615),''),char(1616),''),char(1617),''),char(1618),''),char(1619),''),char(1620),''),char(1621),''),char(1648),''),char(1600),''),char(1571),char(1575)),char(1573),char(1575)),char(1570),char(1575)),char(1649),char(1575)),char(1609),char(1610)),char(1577),char(1607)),char(1572),char(1608)),char(1574),char(1610))) VIRTUAL;

-- A product often carries several codes: multipacks, supplier relabels.
-- The barcode is a LOOKUP KEY; identity is the UUID (master plan C.1).
CREATE TABLE barcode (
  id          BLOB PRIMARY KEY,
  product_id  BLOB NOT NULL REFERENCES product(id),
  code        TEXT NOT NULL,
  kind        TEXT NOT NULL DEFAULT 'ean13'
                CHECK (kind IN ('ean13','ean8','upca','code128','internal','price_embedded','weight_embedded')),
  pack_qty_milli INTEGER NOT NULL DEFAULT 1000 CHECK (pack_qty_milli > 0),
  is_primary  INTEGER NOT NULL DEFAULT 0,
  deleted_at  TEXT,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version     INTEGER NOT NULL DEFAULT 0
) STRICT;
-- Partial unique: a tombstoned code may be reissued, and collisions among LIVE
-- codes are caught. E.36 resolves scans to the newest active + a warning.
CREATE UNIQUE INDEX idx_barcode_code_live ON barcode(code) WHERE deleted_at IS NULL;
CREATE INDEX idx_barcode_product ON barcode(product_id);

-- Ordinary outer packs must land on the product's captured quantity step.
-- Embedded layouts keep the 1000 sentinel because their parsed price/weight is
-- carried by typed scan output and is validated again when the line is built.
CREATE TRIGGER barcode_quantity_shape_insert
BEFORE INSERT ON barcode
WHEN (NEW.kind NOT IN ('price_embedded','weight_embedded')
       AND NEW.pack_qty_milli % (SELECT qty_step_milli FROM product WHERE id = NEW.product_id) <> 0)
  OR (NEW.kind IN ('price_embedded','weight_embedded')
       AND (NEW.pack_qty_milli <> 1000
         OR (SELECT is_weighed FROM product WHERE id = NEW.product_id) <> 1))
BEGIN
  SELECT RAISE(ABORT, 'barcode quantity must match product steps; embedded layouts carry parsed scan data');
END;
CREATE TRIGGER barcode_quantity_shape_update
BEFORE UPDATE OF product_id, kind, pack_qty_milli ON barcode
WHEN (NEW.kind NOT IN ('price_embedded','weight_embedded')
       AND NEW.pack_qty_milli % (SELECT qty_step_milli FROM product WHERE id = NEW.product_id) <> 0)
  OR (NEW.kind IN ('price_embedded','weight_embedded')
       AND (NEW.pack_qty_milli <> 1000
         OR (SELECT is_weighed FROM product WHERE id = NEW.product_id) <> 1))
BEGIN
  SELECT RAISE(ABORT, 'barcode quantity must match product steps; embedded layouts carry parsed scan data');
END;

-- ── Settings ───────────────────────────────────────────────────────────────
-- Key/value so a new policy toggle is a row, not a migration. Every merchant
-- decision in merchant-decisions.md lands here.
CREATE TABLE setting (
  scope       TEXT NOT NULL CHECK (scope IN ('org','store','register')),
  scope_id    BLOB NOT NULL,
  key         TEXT NOT NULL,
  value_json  TEXT NOT NULL,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version     INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (scope, scope_id, key)
) STRICT;
```

> ⚠️ **OPEN — blocks 1.3.4.** Which tie rule, cash-rounding step/direction and tax treatment are required for the current Jordan jurisdiction policy? Default until answered: no `tax_computation_policy` row is approved and store provisioning/finalization remains blocked.
> Owner: 1.3.4. Source that settles it: the current official ISTD arithmetic/business-rule package or a written ISTD clarification reviewed by the merchant's tax advisor.

> ⚠️ **OPEN — blocks 1.3.7.** Which current effective-dated tax categories, percentage/fixed components and jurisdiction packs apply to the merchant's actual assortment? Default until answered: `0003` contains no guessed regulatory rate rows, no `tax_rule_pack` is approved, and unknown or unconfigured categories fail closed.
> Owner: 1.3.7. Source that settles it: the current official ISTD tax-rate catalogue plus the merchant's accountant-approved product classification.

### Filling out `sale_line` — the capture-time columns  ·  SHIPPED

G-12 is already fixed: `0002_sale_integrity.sql` rebuilt the table and
`qty_milli` has been correct since. What is left is the columns this migration's
tax and discount machinery needs, and they are `ALTER TABLE ADD COLUMN` — SQLite
accepts a `REFERENCES` clause there, so there is no second rebuild.

**The triggers have to come off first.** `0002` froze every line of a completed
sale, and the backfill below is an `UPDATE` on exactly those rows. Dropping and
recreating them in this same migration is the rule stated in 0002: a guard is
either restored in the migration that suspends it, or it is gone — and it is
restored in the form the rebuild above established, not the form 0002 shipped. The trigger
test in `crates/pos-db/tests/sale_immutability.rs` is what notices if it is not.

```sql
DROP TRIGGER sale_line_no_insert_once_completed;
DROP TRIGGER sale_line_no_update_once_completed;
DROP TRIGGER sale_line_no_delete_once_completed;

ALTER TABLE sale_line ADD COLUMN line_no         INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sale_line ADD COLUMN name_snapshot   TEXT    NOT NULL DEFAULT '';
ALTER TABLE sale_line ADD COLUMN net_minor       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sale_line ADD COLUMN tax_category_id BLOB REFERENCES tax_category(id);
ALTER TABLE sale_line ADD COLUMN is_weighed      INTEGER NOT NULL DEFAULT 0;

CREATE TRIGGER sale_line_quantity_snapshot_insert
BEFORE INSERT ON sale_line
WHEN NEW.qty_step_milli <> (SELECT qty_step_milli FROM product WHERE id = NEW.product_id)
  OR NEW.is_weighed <> (SELECT is_weighed FROM product WHERE id = NEW.product_id)
BEGIN
  SELECT RAISE(ABORT, 'sale-line quantity semantics must snapshot the selected product');
END;
CREATE TRIGGER sale_line_quantity_snapshot_update
BEFORE UPDATE OF product_id, qty_step_milli, is_weighed ON sale_line
WHEN NEW.qty_step_milli <> (SELECT qty_step_milli FROM product WHERE id = NEW.product_id)
  OR NEW.is_weighed <> (SELECT is_weighed FROM product WHERE id = NEW.product_id)
BEGIN
  SELECT RAISE(ABORT, 'sale-line quantity semantics must snapshot the selected product');
END;

-- I-5: the name is copied onto the line at capture time. Backfilling from
-- today's catalog is the best available answer for rows that predate the column
-- and is wrong for any product renamed since — which is precisely why the
-- column exists, and why this is the last moment it can be approximate.
UPDATE sale_line
   SET name_snapshot = COALESCE(
         (SELECT name FROM product WHERE product.id = sale_line.product_id), '');

UPDATE sale_line
   SET line_no = (SELECT n FROM (
         SELECT id, ROW_NUMBER() OVER (PARTITION BY sale_id ORDER BY rowid) AS n
           FROM sale_line
       ) ordered WHERE ordered.id = sale_line.id);

CREATE UNIQUE INDEX idx_sale_line_no ON sale_line(sale_id, line_no);

-- Restored as the REBUILD at the head of this migration defines them, not as
-- 0002 did. 0002's UPDATE guard tested the OLD parent only, which permitted a row
-- on a parked sale to be re-pointed at a completed one; the rebuild fixed that,
-- and this block runs afterwards, so restoring 0002's wording here would quietly
-- undo the fix. It did, once — `no_fact_row_can_be_reparented_into_a_completed_sale`
-- is what caught it.
CREATE TRIGGER sale_line_no_insert_once_completed
BEFORE INSERT ON sale_line
WHEN (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: cannot add a line to a completed sale');
END;

CREATE TRIGGER sale_line_no_update_once_completed
BEFORE UPDATE ON sale_line
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
  OR (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a line of a completed sale is immutable');
END;

CREATE TRIGGER sale_line_no_delete_once_completed
BEFORE DELETE ON sale_line
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a line of a completed sale cannot be deleted');
END;
```

```sql
-- Per-component tax on a line. Every input needed to reproduce the component is
-- captured: fixed and percentage taxes, their quantity/base, and dependency
-- order. A refund never reads today's rule pack.
CREATE TABLE sale_line_tax (
  id                    BLOB PRIMARY KEY,
  sale_line_id          BLOB NOT NULL REFERENCES sale_line(id),
  component_code        TEXT NOT NULL,
  treatment             TEXT NOT NULL
                            CHECK (treatment IN ('standard','reduced','zero','exempt')),
  calculation_kind      TEXT NOT NULL
                            CHECK (calculation_kind IN ('ad_valorem','fixed_per_quantity')),
  rate_ppm              INTEGER CHECK (rate_ppm >= 0),
  fixed_amount_minor    INTEGER,
  fixed_currency        TEXT,
  fixed_basis_qty_milli INTEGER,
  calculation_order     INTEGER NOT NULL DEFAULT 0,
  base_kind             TEXT NOT NULL
                            CHECK (base_kind IN ('line_net','line_net_plus_prior_components','quantity')),
  taxable_base_minor    INTEGER,
  taxable_qty_milli     INTEGER,
  tax_minor             INTEGER NOT NULL,
  UNIQUE (sale_line_id, calculation_order),
  CHECK (
    (calculation_kind = 'ad_valorem'
      AND rate_ppm IS NOT NULL
      AND fixed_amount_minor IS NULL
      AND fixed_currency IS NULL
      AND fixed_basis_qty_milli IS NULL
      AND taxable_base_minor IS NOT NULL
      AND taxable_qty_milli IS NULL
      AND base_kind IN ('line_net','line_net_plus_prior_components'))
    OR
    (calculation_kind = 'fixed_per_quantity'
      AND rate_ppm IS NULL
      AND fixed_amount_minor > 0
      AND fixed_currency IS NOT NULL
      AND fixed_basis_qty_milli > 0
      AND taxable_base_minor IS NULL
      AND taxable_qty_milli IS NOT NULL
      AND base_kind = 'quantity')
  )
) STRICT;
CREATE INDEX idx_sale_line_tax_line ON sale_line_tax(sale_line_id);

-- Discount attributions. Campaign-cost reporting (C.9) AND JoFotara's per-line
-- discount requirement (correction C-2) both read this table. A basket discount
-- that has not been attributed to lines cannot become a fiscal document.
CREATE TABLE sale_line_discount (
  id             BLOB PRIMARY KEY,
  sale_line_id   BLOB NOT NULL REFERENCES sale_line(id),
  source         TEXT NOT NULL CHECK (source IN ('manual_line','manual_basket','promotion','loyalty','price_override')),
  authorized_by  BLOB,
  reason         TEXT,
  amount_minor   INTEGER NOT NULL CHECK (amount_minor >= 0),
  -- Provenance only. Fiscal eligibility and recap equality use the exact line
  -- amount; an integer percentage is never round-tripped back into money.
  percent_ppm    INTEGER
) STRICT;
CREATE INDEX idx_sale_line_discount_line ON sale_line_discount(sale_line_id);

```

> ⚠️ **OPEN — blocks 2.7.0.** How many decimal places may the current JoFotara discount percentage carry when the pinned profile requires one? Default until answered: exact line allowance amounts and their exact document recap are authoritative; an entered percentage is provenance only, `DISCOUNT_PERCENT_DECIMALS` is the single emission constant, and percentage round-trip never gates fiscal eligibility.
> Owner: 2.7.0. Source that settles it: the pinned official ISTD Technical Integration Guide, XSD, business rules and accepted boundary vectors.

```sql
-- Zero-rating can be a property of this supply rather than this SKU. The sale
-- snapshots destination, reason and evidence so a later filing can distinguish
-- an export/free-zone/eligible-body supply without changing the catalog.
CREATE TABLE sale_supply_tax_context (
  sale_id                    BLOB PRIMARY KEY REFERENCES sale(id),
  destination_code           TEXT NOT NULL,
  zero_tax_reason_code       TEXT,
  eligible_entity_authority  TEXT,
  evidence_ref               TEXT,
  evidence_hash              BLOB CHECK (evidence_hash IS NULL OR length(evidence_hash) = 32),
  captured_at                TEXT NOT NULL,
  CHECK (zero_tax_reason_code IS NULL OR evidence_ref IS NOT NULL)
) STRICT;

CREATE TRIGGER sale_line_tax_category_evidenced_insert
BEFORE INSERT ON sale_line
WHEN NEW.tax_category_id IS NOT (SELECT tax_category_id FROM product WHERE id = NEW.product_id)
 AND NOT EXISTS (
   SELECT 1 FROM sale_supply_tax_context c
    WHERE c.sale_id = NEW.sale_id AND c.zero_tax_reason_code IS NOT NULL
      AND c.evidence_ref IS NOT NULL AND c.evidence_hash IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'a supply-specific tax-category override requires immutable evidence');
END;
CREATE TRIGGER sale_line_tax_category_evidenced_update
BEFORE UPDATE OF sale_id, product_id, tax_category_id ON sale_line
WHEN NEW.tax_category_id IS NOT (SELECT tax_category_id FROM product WHERE id = NEW.product_id)
 AND NOT EXISTS (
   SELECT 1 FROM sale_supply_tax_context c
    WHERE c.sale_id = NEW.sale_id AND c.zero_tax_reason_code IS NOT NULL
      AND c.evidence_ref IS NOT NULL AND c.evidence_hash IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'a supply-specific tax-category override requires immutable evidence');
END;

-- ── I-4 on the tax and discount detail ─────────────────────────────────────
--
-- These rows ARE the arithmetic of what the customer was charged. Change one
-- after the fact and the receipt stops explaining its own total, while every
-- report built from the detail silently disagrees with the sale it came from.
-- Same rule as `sale_line` in 0002, one hop further out: the parent sale is
-- reached through `sale_line`.

CREATE TRIGGER sale_line_tax_no_insert_once_completed
BEFORE INSERT ON sale_line_tax
WHEN (SELECT s.status FROM sale s JOIN sale_line l ON l.sale_id = s.id
       WHERE l.id = NEW.sale_line_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: cannot add tax detail to a completed sale');
END;

CREATE TRIGGER sale_line_tax_no_update_once_completed
BEFORE UPDATE ON sale_line_tax
-- BOTH parents, not just the old one. Checking OLD alone stops an edit to a row
-- that already belongs to a completed sale, and lets a row be MOVED onto one
-- from a parked sale — which changes what a closed document says by re-pointing
-- a foreign key rather than by editing a protected row.
WHEN (SELECT s.status FROM sale s JOIN sale_line l ON l.sale_id = s.id
       WHERE l.id = OLD.sale_line_id) = 'completed'
  OR (SELECT s.status FROM sale s JOIN sale_line l ON l.sale_id = s.id
       WHERE l.id = NEW.sale_line_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: the tax detail of a completed sale is immutable');
END;

CREATE TRIGGER sale_line_tax_no_delete_once_completed
BEFORE DELETE ON sale_line_tax
WHEN (SELECT s.status FROM sale s JOIN sale_line l ON l.sale_id = s.id
       WHERE l.id = OLD.sale_line_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: the tax detail of a completed sale cannot be deleted');
END;

CREATE TRIGGER sale_line_discount_no_insert_once_completed
BEFORE INSERT ON sale_line_discount
WHEN (SELECT s.status FROM sale s JOIN sale_line l ON l.sale_id = s.id
       WHERE l.id = NEW.sale_line_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: cannot add a discount to a completed sale');
END;

CREATE TRIGGER sale_line_discount_no_update_once_completed
BEFORE UPDATE ON sale_line_discount
-- BOTH parents, not just the old one. Checking OLD alone stops an edit to a row
-- that already belongs to a completed sale, and lets a row be MOVED onto one
-- from a parked sale — which changes what a closed document says by re-pointing
-- a foreign key rather than by editing a protected row.
WHEN (SELECT s.status FROM sale s JOIN sale_line l ON l.sale_id = s.id
       WHERE l.id = OLD.sale_line_id) = 'completed'
  OR (SELECT s.status FROM sale s JOIN sale_line l ON l.sale_id = s.id
       WHERE l.id = NEW.sale_line_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: the discount on a completed sale is immutable');
END;

CREATE TRIGGER sale_line_discount_no_delete_once_completed
BEFORE DELETE ON sale_line_discount
WHEN (SELECT s.status FROM sale s JOIN sale_line l ON l.sale_id = s.id
       WHERE l.id = OLD.sale_line_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: the discount on a completed sale cannot be deleted');
END;

CREATE TRIGGER sale_supply_tax_context_no_insert_once_completed
BEFORE INSERT ON sale_supply_tax_context
WHEN (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: cannot add tax context to a completed sale');
END;

CREATE TRIGGER sale_supply_tax_context_no_update_once_completed
BEFORE UPDATE ON sale_supply_tax_context
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
  OR (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: the supply tax context of a completed sale is immutable');
END;

CREATE TRIGGER sale_supply_tax_context_no_delete_once_completed
BEFORE DELETE ON sale_supply_tax_context
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: the supply tax context of a completed sale cannot be deleted');
END;

```

---

## 0004 — people and audit  ·  Phase 1, microsteps 1.6.1–1.6.4  ·  SHIPPED

```sql
CREATE TABLE capability (
  code          TEXT PRIMARY KEY,
  description   TEXT NOT NULL
) STRICT;

INSERT INTO capability (code, description) VALUES
  ('sale.create', 'Create and finalize an ordinary sale'),
  ('sale.park', 'Park a cart'),
  ('sale.resume', 'Resume a parked cart'),
  ('sale.void', 'Void a non-completed sale'),
  ('sale.reprint', 'Create a linked duplicate receipt'),
  ('sale.department', 'Create an audited open-price department line'),
  ('line.void', 'Remove a priced line before completion'),
  ('discount.manual', 'Apply a deliberate manual discount'),
  ('price.override', 'Override a catalog price with bound approval'),
  ('drawer.open', 'Open the drawer outside an automatic cash-sale effect'),
  ('cash.movement', 'Post a double-entry cash movement'),
  ('shift.open', 'Open a register shift'),
  ('shift.close', 'Close a register shift'),
  ('xreport.run', 'Run a non-closing X report'),
  ('zreport.run', 'Create the immutable closing Z report'),
  ('refund.receipted', 'Refund against an original document'),
  ('refund.receiptless', 'Create a policy-limited receiptless refund'),
  ('refund.above_threshold', 'Approve a refund above the configured threshold'),
  ('refund.cash_for_card', 'Approve cash settlement of an original card tender'),
  ('refund.outside_window', 'Approve a defective claim outside the store goodwill window'),
  ('stock.adjust', 'Post a stock correction event'),
  ('product.edit', 'Edit mutable catalog reference data'),
  ('tax.rate.edit', 'Create a new source-backed tax-rate version'),
  ('fiscal.remediate', 'Rebuild a locally failed fiscal payload after correction'),
  ('customer.lookup', 'Look up minimized customer data'),
  ('journal.view', 'Read the sales journal'),
  ('reports.own', 'Read the actor own reports'),
  ('reports.all', 'Read store-wide reports'),
  ('training_mode.toggle', 'Enter or leave visibly marked training mode'),
  ('settings.edit', 'Edit merchant settings'),
  ('user.admin', 'Administer merchant users and roles'),
  ('backup.restore', 'Enter the out-of-band recovery flow');

CREATE TABLE app_user (                    -- `user` is reserved in Postgres
  id            BLOB PRIMARY KEY,
  org_id        BLOB NOT NULL REFERENCES org(id),
  code          TEXT NOT NULL UNIQUE,      -- staff number
  display_name  TEXT NOT NULL,
  pin_hash      TEXT NOT NULL,             -- Argon2id PHC string. Never the PIN.
  pin_set_at    TEXT NOT NULL,
  is_active     INTEGER NOT NULL DEFAULT 1,
  deleted_at    TEXT,
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version       INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE role (
  id          BLOB PRIMARY KEY,
  code        TEXT NOT NULL UNIQUE         -- cashier|shift_lead|manager|owner
                CHECK (code IN ('cashier','shift_lead','manager','owner')),
  name_ar     TEXT NOT NULL,
  name_en     TEXT,
  deleted_at  TEXT,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version     INTEGER NOT NULL DEFAULT 0
) STRICT;

-- The ids are deterministic UUIDv7-shaped literals, not `randomblob(16)`: the
-- same logical role must carry the same id on every register, because `role`
-- and `role_capability` are server-wins reference tables and two registers that
-- invented different ids for "manager" cannot be reconciled centrally. The
-- byte layout — 0x01A05F6A5800 = 2026-09-02T00:00:00.000Z, version nibble 7,
-- variant bits 0b10, and the role ordinal in the last byte — is set out in the
-- migration file's header.
INSERT INTO role (id, code, name_ar, name_en) VALUES
  (X'01A05F6A580070008000000000000001', 'cashier', 'أمين صندوق', 'Cashier'),
  (X'01A05F6A580070008000000000000002', 'shift_lead', 'مسؤول وردية', 'Shift lead'),
  (X'01A05F6A580070008000000000000003', 'manager', 'مدير', 'Manager'),
  (X'01A05F6A580070008000000000000004', 'owner', 'مالك', 'Owner');

CREATE TABLE role_capability (
  role_id     BLOB NOT NULL REFERENCES role(id),
  capability  TEXT NOT NULL REFERENCES capability(code),
  -- ONE ROW PER CELL — all 128, not only the 75 a role holds. NOT NULL with no
  -- DEFAULT, so an undecided cell is an absent row rather than a silent denial
  -- no query can tell apart from a deliberate one, and the forward-only law
  -- would make that permanent. Three values because `Grant::SetsTheLimit` in
  -- pos-domain is a different answer from `Grant::Withheld`: the owner runs no
  -- till, and what they do is configure the ceiling the roles that can work
  -- under. A boolean collapses the two into the same blank.
  decision    TEXT NOT NULL
                CHECK (decision IN ('granted','withheld','sets_the_limit')),
  -- The limit's kind, spelled as `pos_domain::permissions::Limit::as_str`
  -- spells it: {"kind":"own_shift"}. An object, so the merchant-configured
  -- value can join it later as a sibling key — which is where
  -- e.g. {"max_percent_ppm":50000} lands, once merchant decisions 3.1–3.3 are
  -- answered at microstep 1.4.5. 0004 does not guess that number.
  limit_json  TEXT,
  -- A denial carrying a limit is nonsense, and would read as a bounded grant to
  -- anything that reached for `limit_json` before `decision`.
  CHECK (decision = 'granted' OR limit_json IS NULL),
  PRIMARY KEY (role_id, capability)
) STRICT;

-- The 128 seeded cells are ref/domain-api.md §8.2 row for row, and they live in
-- `crates/pos-db/migrations/0004_people_and_audit.sql` rather than being copied
-- here: a second transcription of a grid is a second thing to drift, and this
-- section is skipped by verify-schema.py now that it is SHIPPED, so nothing
-- would execute the copy. `crates/pos-db/tests/migration_0004_people_and_audit.rs`
-- proves every role carries an explicit decision for every capability, and
-- microstep 1.6.3's `crates/pos-db/tests/role_matrix.rs` compares each seeded
-- cell with `cap::DEFAULT_MATRIX`, in both directions.
-- That comparison, not a duplicate table, is the check.

-- `store_id` is NULL for an org-wide grant, and that NULL is load-bearing: it is
-- how an owner or an area manager holds a role across every store.
--
-- It therefore cannot be part of the PRIMARY KEY of a STRICT table. SQLite makes
-- every primary-key component of a STRICT table implicitly NOT NULL — composite
-- keys included — so `PRIMARY KEY (user_id, role_id, store_id)` under STRICT
-- makes an org-wide grant impossible to insert. That is exactly what happened
-- when STRICT was applied here, and it is why the key is gone.
--
-- Two partial unique indexes give back what the key was for, without forbidding
-- the NULL: one grant per (user, role, store), and one org-wide grant per
-- (user, role). A plain unique index over the triple would not do, because SQLite
-- treats NULLs as distinct and would allow the same org-wide grant twice. Same
-- idiom as `idx_barcode_code_live` and `idx_shift_one_open` above.
CREATE TABLE user_role (
  user_id   BLOB NOT NULL REFERENCES app_user(id),
  role_id   BLOB NOT NULL REFERENCES role(id),
  store_id  BLOB REFERENCES store(id)      -- NULL = org-wide
) STRICT;
CREATE UNIQUE INDEX idx_user_role_scoped ON user_role(user_id, role_id, store_id)
  WHERE store_id IS NOT NULL;
CREATE UNIQUE INDEX idx_user_role_org_wide ON user_role(user_id, role_id)
  WHERE store_id IS NULL;

CREATE TABLE user_session (
  id           BLOB PRIMARY KEY,
  user_id      BLOB NOT NULL REFERENCES app_user(id),
  register_id  BLOB NOT NULL REFERENCES register(id),
  started_at   TEXT NOT NULL,
  ended_at     TEXT,
  end_reason   TEXT CHECK (end_reason IN ('logout','idle_lock','switch_user','shift_close','crash'))
) STRICT;

-- A runtime approval is one-use evidence bound to the exact effect. Keeping the
-- handle and its separate consumption fact makes restart safe: replay is refused
-- by the primary key, and consumption commits in the same transaction as the
-- financial effect and its audit row.
CREATE TABLE approval_handle (
  id            BLOB PRIMARY KEY,
  capability    TEXT NOT NULL REFERENCES capability(code),
  actor_id      BLOB NOT NULL REFERENCES app_user(id),
  approver_id   BLOB NOT NULL REFERENCES app_user(id),
  entity_id     BLOB NOT NULL,
  amount_minor  INTEGER NOT NULL,
  -- Present for a prepared-intent command and absent otherwise. This is the
  -- BLAKE3 digest of the versioned, domain-separated canonical intent bytes;
  -- the webview never supplies it.
  content_hash  BLOB,
  reason        TEXT NOT NULL,
  issued_at     TEXT NOT NULL,
  expires_at    TEXT NOT NULL,
  nonce         BLOB NOT NULL UNIQUE,
  CHECK (actor_id <> approver_id),
  CHECK (content_hash IS NULL OR length(content_hash) = 32),
  CHECK (expires_at > issued_at)
) STRICT;

CREATE TABLE approval_consumption (
  handle_id     BLOB PRIMARY KEY REFERENCES approval_handle(id),
  effect_id     BLOB NOT NULL,
  audit_log_id  BLOB NOT NULL UNIQUE REFERENCES audit_log(id),
  consumed_at   TEXT NOT NULL
) STRICT;

CREATE TRIGGER approval_handle_no_update
BEFORE UPDATE ON approval_handle BEGIN
  SELECT RAISE(ABORT, 'ApprovalHandle is immutable — consume it once with the effect');
END;
CREATE TRIGGER approval_handle_no_delete
BEFORE DELETE ON approval_handle BEGIN
  SELECT RAISE(ABORT, 'ApprovalHandle is audit evidence and cannot be deleted');
END;
CREATE TRIGGER approval_consumption_no_update
BEFORE UPDATE ON approval_consumption BEGIN
  SELECT RAISE(ABORT, 'an approval consumption fact is immutable');
END;
CREATE TRIGGER approval_consumption_no_delete
BEFORE DELETE ON approval_consumption BEGIN
  SELECT RAISE(ABORT, 'an approval consumption fact cannot be deleted');
END;

-- Brute-force throttling survives process restart. This is mutable security
-- state, not a fact; successful recovery resets it through a named repository
-- operation that also writes the audit event.
CREATE TABLE auth_attempt_state (
  user_id             BLOB NOT NULL REFERENCES app_user(id),
  register_id         BLOB NOT NULL REFERENCES register(id),
  failed_attempts     INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
  delay_until_at      TEXT,
  locked_until_at     TEXT,
  last_attempt_at     TEXT NOT NULL,
  PRIMARY KEY (user_id, register_id)
) STRICT;

-- Hash-chained (G-7). hash = BLAKE3(prev_hash ‖ canonical_bytes(entry)).
-- Canonical bytes cover every immutable persisted field, including `id`,
-- `register_id` and `canonical_version`; changing row identity is therefore a
-- chain break rather than an unhashed reattribution. Append-only forever.
CREATE TABLE audit_log (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  id          BLOB NOT NULL UNIQUE,
  canonical_version INTEGER NOT NULL DEFAULT 1 CHECK (canonical_version > 0),
  register_id BLOB NOT NULL,
  actor_id    BLOB NOT NULL,
  approver_id BLOB,                        -- distinct from actor on escalation (E.52)
  approval_handle_id BLOB REFERENCES approval_handle(id),
  action      TEXT NOT NULL,
  entity      TEXT NOT NULL,
  entity_id   BLOB,
  reason      TEXT,
  payload     TEXT NOT NULL,               -- canonical JSON. NEVER PII or card data.
  prev_hash   BLOB NOT NULL,
  hash        BLOB NOT NULL,
  at          TEXT NOT NULL
) STRICT;
CREATE INDEX idx_audit_action_at ON audit_log(action, at);
CREATE INDEX idx_audit_actor_at  ON audit_log(actor_id, at);
CREATE UNIQUE INDEX idx_audit_approval_once ON audit_log(approval_handle_id)
  WHERE approval_handle_id IS NOT NULL;

-- Consumption is not a free-standing "used" bit. It names the one audit row
-- that proves the same actor, approver, capability, entity, amount and reason
-- were consumed with the financial effect. The command transaction inserts the
-- effect, audit row and this row together; a rollback removes all three.
CREATE TRIGGER approval_consumption_matches_handle_and_audit
BEFORE INSERT ON approval_consumption
WHEN NOT EXISTS (
  SELECT 1
    FROM approval_handle h JOIN audit_log a ON a.id = NEW.audit_log_id
   WHERE h.id = NEW.handle_id
     AND NEW.effect_id = h.entity_id
     AND NEW.consumed_at >= h.issued_at
     AND NEW.consumed_at < h.expires_at
     AND a.approval_handle_id = h.id
     AND a.actor_id = h.actor_id
     AND a.approver_id = h.approver_id
     AND a.entity_id = h.entity_id
     AND a.action = h.capability
     AND a.reason = h.reason
     AND json_type(a.payload, '$.amount_minor') = 'integer'
     AND json_extract(a.payload, '$.amount_minor') = h.amount_minor)
BEGIN
  SELECT RAISE(ABORT, 'approval consumption must match one bound financial effect and audit row');
END;

-- A chain alone cannot detect deletion of its current tail or a full re-chain.
-- Z close and each verified backup export this checkpoint; Phase 3 also stores
-- it on the server. Verification compares the database head with an anchor that
-- was already outside the database before claiming the chain is intact.
CREATE TABLE audit_checkpoint (
  id            BLOB PRIMARY KEY,
  register_id   BLOB NOT NULL REFERENCES register(id),
  last_seq      INTEGER NOT NULL CHECK (last_seq >= 0),
  last_hash     BLOB NOT NULL,
  source_kind   TEXT NOT NULL
                  CHECK (source_kind IN ('z_report','verified_backup','server')),
  anchor_ref    TEXT NOT NULL,
  anchored_at   TEXT NOT NULL,
  UNIQUE (register_id, last_seq, last_hash, source_kind)
) STRICT;

CREATE TRIGGER approval_handle_has_ready_commit
BEFORE INSERT ON approval_handle
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'approval_handle' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'approval issuance requires its complete delivery envelope'); END;
CREATE TRIGGER approval_consumption_has_ready_commit
BEFORE INSERT ON approval_consumption
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'approval_consumption' AND m.entity_id = NEW.handle_id)
BEGIN SELECT RAISE(ABORT, 'approval consumption requires its complete delivery envelope'); END;
CREATE TRIGGER audit_log_has_ready_commit
BEFORE INSERT ON audit_log
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'audit_log' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'audit fact requires its complete delivery envelope'); END;

-- ── audit_log is append-only, and now says so in something other than prose ──
--
-- The DDL above already asserts "Append-only: no UPDATE, no DELETE, ever".
-- Nothing enforced it, which made the only forensic control in the design the
-- one control an insider could edit. The hash chain detects a modified row but
-- cannot detect a deleted tail, so DELETE is the attack that matters most.

CREATE TRIGGER audit_log_no_update
BEFORE UPDATE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'I-4: audit_log is append-only — no UPDATE, ever');
END;

CREATE TRIGGER audit_log_no_delete
BEFORE DELETE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'I-4: audit_log is append-only — no DELETE, ever');
END;

CREATE TRIGGER audit_checkpoint_no_update
BEFORE UPDATE ON audit_checkpoint BEGIN
  SELECT RAISE(ABORT, 'an audit checkpoint is append-only');
END;
CREATE TRIGGER audit_checkpoint_no_delete
BEFORE DELETE ON audit_checkpoint BEGIN
  SELECT RAISE(ABORT, 'an audit checkpoint cannot be deleted');
END;

-- This migration owns the capability catalogue, the four standard roles and
-- every one of their 128 (role, capability) decisions. Provisioning creates the
-- merchant's users and their `user_role` grants against these rows; it does not
-- invent a role, because a role invented per install cannot be reconciled across
-- registers. A later authorization microstep verifies the typed registry against
-- what is seeded here; it never edits 0004 to append a capability the plan
-- already knew about.

```

---

## 0005 — sale columns and sequences  ·  Phase 1, microsteps 1.4.11, 1.9.1

```sql
-- The opening row is an immutable fact. Closing is a separate fact and
-- `shift_state` is a rebuildable operational projection. This keeps one-open
-- enforcement without mutating a row the server classifies as append-only.
CREATE TABLE shift (
  id            BLOB PRIMARY KEY,
  register_id   BLOB NOT NULL REFERENCES register(id),
  store_id      BLOB NOT NULL REFERENCES store(id),
  business_date TEXT NOT NULL,              -- conventions §11
  opened_by     BLOB NOT NULL REFERENCES app_user(id),
  opened_at     TEXT NOT NULL,
  float_minor   INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE shift_close_event (
  id             BLOB PRIMARY KEY,
  shift_id       BLOB NOT NULL UNIQUE REFERENCES shift(id),
  sync_commit_id BLOB NOT NULL REFERENCES sync_commit(id),
  closed_by      BLOB NOT NULL REFERENCES app_user(id),
  closed_at      TEXT NOT NULL
) STRICT;

CREATE TRIGGER shift_open_has_ready_commit
BEFORE INSERT ON shift
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'shift' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'shift open requires its complete delivery envelope'); END;

CREATE TABLE shift_state (
  shift_id            BLOB PRIMARY KEY REFERENCES shift(id),
  register_id         BLOB NOT NULL REFERENCES register(id),
  state               TEXT NOT NULL CHECK (state IN ('open','closed')),
  last_close_event_id BLOB REFERENCES shift_close_event(id),
  closed_at           TEXT,
  CHECK (
    (state = 'open' AND last_close_event_id IS NULL AND closed_at IS NULL)
    OR
    (state = 'closed' AND last_close_event_id IS NOT NULL AND closed_at IS NOT NULL)
  )
) STRICT;
CREATE UNIQUE INDEX idx_shift_one_open ON shift_state(register_id) WHERE state = 'open';

CREATE TRIGGER shift_register_belongs_to_store
BEFORE INSERT ON shift
WHEN NOT EXISTS (
  SELECT 1 FROM register r WHERE r.id = NEW.register_id AND r.store_id = NEW.store_id)
BEGIN
  SELECT RAISE(ABORT, 'a shift register must belong to the shift store');
END;

CREATE TRIGGER shift_project_open
AFTER INSERT ON shift
BEGIN
  INSERT INTO shift_state (shift_id, register_id, state)
  VALUES (NEW.id, NEW.register_id, 'open');
END;

CREATE TRIGGER shift_project_close
AFTER INSERT ON shift_close_event
BEGIN
  UPDATE shift_state
     SET state = 'closed', last_close_event_id = NEW.id, closed_at = NEW.closed_at
   WHERE shift_id = NEW.shift_id;
  SELECT CASE WHEN changes() <> 1
    THEN RAISE(ABORT, 'shift close requires its open-state projection') END;
END;

CREATE TRIGGER shift_close_has_ready_commit
BEFORE INSERT ON shift_close_event
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.commit_id = NEW.sync_commit_id
     AND m.entity = 'shift_close_event' AND m.entity_id = NEW.id)
BEGIN
  SELECT RAISE(ABORT, 'shift close requires its complete immutable delivery envelope');
END;

-- Only the event projection triggers may write `shift_state`. These guards make
-- a manually forged "closed" row fail even if a repository bypasses the event.
CREATE TRIGGER shift_state_matches_open_insert
BEFORE INSERT ON shift_state
WHEN NEW.state <> 'open'
  OR NOT EXISTS (SELECT 1 FROM shift sh
                  WHERE sh.id = NEW.shift_id AND sh.register_id = NEW.register_id)
BEGIN
  SELECT RAISE(ABORT, 'a shift state starts from its immutable opening fact');
END;
CREATE TRIGGER shift_state_matches_event_update
BEFORE UPDATE ON shift_state
WHEN NEW.state <> 'closed'
  OR NOT EXISTS (
       SELECT 1 FROM shift_close_event e JOIN shift sh ON sh.id = e.shift_id
        WHERE e.id = NEW.last_close_event_id
          AND e.shift_id = NEW.shift_id
          AND e.closed_at = NEW.closed_at
          AND sh.register_id = NEW.register_id)
BEGIN
  SELECT RAISE(ABORT, 'a closed shift projection must match its close event');
END;
CREATE TRIGGER shift_state_no_delete
BEFORE DELETE ON shift_state BEGIN
  SELECT RAISE(ABORT, 'a shift projection is rebuilt, not selectively deleted');
END;

ALTER TABLE sale ADD COLUMN store_id            BLOB REFERENCES store(id);
ALTER TABLE sale ADD COLUMN shift_id            BLOB REFERENCES shift(id);
ALTER TABLE sale ADD COLUMN cashier_id          BLOB REFERENCES app_user(id);
ALTER TABLE sale ADD COLUMN customer_id         BLOB;
ALTER TABLE sale ADD COLUMN doc_type            TEXT NOT NULL DEFAULT 'sale'
                                                  CHECK (doc_type IN ('sale','refund'));
-- UBL `cbc:ID` is the immutable register-prefixed `receipt_number`. UUID and ICV
-- do not replace it. Buyer identifiers snapshot scheme and value, but the scheme
-- token list remains provisional until 2.7.0 pins the official package.
ALTER TABLE sale ADD COLUMN buyer_id_scheme     TEXT;
ALTER TABLE sale ADD COLUMN buyer_id_value      TEXT;
ALTER TABLE sale ADD COLUMN buyer_name          TEXT;
ALTER TABLE sale ADD COLUMN is_training         INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sale ADD COLUMN discount_minor      INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sale ADD COLUMN rounding_adj_minor  INTEGER NOT NULL DEFAULT 0;  -- B.5
ALTER TABLE sale ADD COLUMN tax_computation_policy_id BLOB
  REFERENCES tax_computation_policy(id);
ALTER TABLE sale ADD COLUMN sync_commit_id BLOB REFERENCES sync_commit(id);
ALTER TABLE sale ADD COLUMN origin_device       TEXT;
CREATE INDEX idx_sale_shift ON sale(shift_id);

CREATE TRIGGER sale_buyer_identifier_complete_insert
BEFORE INSERT ON sale
WHEN (NEW.buyer_id_scheme IS NULL) <> (NEW.buyer_id_value IS NULL)
BEGIN
  SELECT RAISE(ABORT, 'buyer identifier scheme and value must be captured together');
END;
CREATE TRIGGER sale_buyer_identifier_complete_update
BEFORE UPDATE OF buyer_id_scheme, buyer_id_value ON sale
WHEN (NEW.buyer_id_scheme IS NULL) <> (NEW.buyer_id_value IS NULL)
BEGIN
  SELECT RAISE(ABORT, 'buyer identifier scheme and value must be captured together');
END;

-- Historical Phase-0 fixture rows may be NULL. Every Phase-1 finalize is a new
-- insert or parked→completed transition and must name the open shift whose
-- business date it carries.
CREATE TRIGGER sale_completed_requires_shift_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND NOT EXISTS (
  SELECT 1 FROM shift sh JOIN shift_state ss ON ss.shift_id = sh.id
   WHERE sh.id = NEW.shift_id
     AND sh.register_id = NEW.register_id
     AND sh.store_id = NEW.store_id
     AND sh.business_date = NEW.business_date
     AND ss.state = 'open')
BEGIN
  SELECT RAISE(ABORT, 'a completed sale requires the open shift for its register, store and business date');
END;
CREATE TRIGGER sale_completed_requires_shift_update
BEFORE UPDATE OF status, shift_id, register_id, store_id, business_date ON sale
WHEN NEW.status = 'completed' AND NOT EXISTS (
  SELECT 1 FROM shift sh JOIN shift_state ss ON ss.shift_id = sh.id
   WHERE sh.id = NEW.shift_id
     AND sh.register_id = NEW.register_id
     AND sh.store_id = NEW.store_id
     AND sh.business_date = NEW.business_date
     AND ss.state = 'open')
BEGIN
  SELECT RAISE(ABORT, 'a completed sale requires the open shift for its register, store and business date');
END;

CREATE TRIGGER sale_completed_requires_tax_policy_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND (
  NEW.tax_computation_policy_id IS NULL
  OR (NEW.doc_type = 'sale' AND NEW.tax_computation_policy_id IS NOT (
       SELECT tax_computation_policy_id FROM store WHERE id = NEW.store_id))
  OR (NEW.doc_type = 'refund' AND (
       NEW.ref_sale_id IS NULL OR NEW.tax_computation_policy_id IS NOT (
         SELECT tax_computation_policy_id FROM sale WHERE id = NEW.ref_sale_id))))
BEGIN
  SELECT RAISE(ABORT, 'a sale snapshots the current policy; a refund preserves the original policy');
END;
CREATE TRIGGER sale_completed_requires_tax_policy_update
BEFORE UPDATE OF status, store_id, doc_type, ref_sale_id, tax_computation_policy_id ON sale
WHEN NEW.status = 'completed' AND (
  NEW.tax_computation_policy_id IS NULL
  OR (NEW.doc_type = 'sale' AND NEW.tax_computation_policy_id IS NOT (
       SELECT tax_computation_policy_id FROM store WHERE id = NEW.store_id))
  OR (NEW.doc_type = 'refund' AND (
       NEW.ref_sale_id IS NULL OR NEW.tax_computation_policy_id IS NOT (
         SELECT tax_computation_policy_id FROM sale WHERE id = NEW.ref_sale_id))))
BEGIN
  SELECT RAISE(ABORT, 'a sale snapshots the current policy; a refund preserves the original policy');
END;

CREATE TRIGGER sale_completed_requires_fiscal_decision_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND NEW.is_training = 0 AND NOT EXISTS (
  SELECT 1 FROM store st WHERE st.id = NEW.store_id
   AND ((st.fiscal_obligation = 'required'
          AND st.fiscal_profile = 'jordan_jofotara'
          AND st.fiscal_taxpayer_type IS NOT NULL
          AND st.fiscal_obligation_evidence_ref IS NOT NULL)
     OR (st.fiscal_obligation = 'exempt'
          AND st.fiscal_profile = 'disabled'
          AND st.fiscal_obligation_evidence_ref IS NOT NULL)))
BEGIN
  SELECT RAISE(ABORT, 'a live sale requires evidenced fiscal obligation or exemption');
END;
CREATE TRIGGER sale_completed_requires_fiscal_decision_update
BEFORE UPDATE OF status, is_training, store_id ON sale
WHEN NEW.status = 'completed' AND NEW.is_training = 0 AND NOT EXISTS (
  SELECT 1 FROM store st WHERE st.id = NEW.store_id
   AND ((st.fiscal_obligation = 'required'
          AND st.fiscal_profile = 'jordan_jofotara'
          AND st.fiscal_taxpayer_type IS NOT NULL
          AND st.fiscal_obligation_evidence_ref IS NOT NULL)
     OR (st.fiscal_obligation = 'exempt'
          AND st.fiscal_profile = 'disabled'
          AND st.fiscal_obligation_evidence_ref IS NOT NULL)))
BEGIN
  SELECT RAISE(ABORT, 'a live sale requires evidenced fiscal obligation or exemption');
END;

CREATE TRIGGER sale_completed_requires_tax_components_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND EXISTS (
  SELECT 1 FROM sale_line l
   WHERE l.sale_id = NEW.id
     AND (l.tax_category_id IS NULL
       OR NOT EXISTS (SELECT 1 FROM sale_line_tax t WHERE t.sale_line_id = l.id)
       OR (NEW.doc_type = 'sale' AND NOT EXISTS (
            SELECT 1 FROM store st JOIN tax_rate r
              ON r.rule_pack_id = st.tax_rule_pack_id
             AND r.tax_category_id = l.tax_category_id
             AND r.valid_from <= NEW.business_date
             AND (r.valid_to IS NULL OR r.valid_to > NEW.business_date)
             AND r.deleted_at IS NULL
           WHERE st.id = NEW.store_id))
       OR (NEW.doc_type = 'sale' AND EXISTS (
            SELECT 1 FROM store st JOIN tax_rate r
              ON r.rule_pack_id = st.tax_rule_pack_id
             AND r.tax_category_id = l.tax_category_id
             AND r.valid_from <= NEW.business_date
             AND (r.valid_to IS NULL OR r.valid_to > NEW.business_date)
             AND r.deleted_at IS NULL
             LEFT JOIN sale_line_tax t
               ON t.sale_line_id = l.id
              AND t.component_code = r.component_code
              AND t.calculation_order = r.calculation_order
              AND t.calculation_kind = r.calculation_kind
              AND t.treatment = r.treatment
              AND t.rate_ppm IS r.rate_ppm
              AND t.fixed_amount_minor IS r.fixed_amount_minor
              AND t.fixed_currency IS r.fixed_currency
              AND t.fixed_basis_qty_milli IS r.fixed_basis_qty_milli
              AND t.base_kind = r.base_kind
           WHERE st.id = NEW.store_id AND t.id IS NULL))
       OR (NEW.doc_type = 'sale' AND EXISTS (
            SELECT 1 FROM sale_line_tax t
             WHERE t.sale_line_id = l.id
               AND NOT EXISTS (
                 SELECT 1 FROM store st JOIN tax_rate r
                   ON r.rule_pack_id = st.tax_rule_pack_id
                  AND r.tax_category_id = l.tax_category_id
                  AND r.valid_from <= NEW.business_date
                  AND (r.valid_to IS NULL OR r.valid_to > NEW.business_date)
                  AND r.deleted_at IS NULL
                  AND r.component_code = t.component_code
                  AND r.calculation_order = t.calculation_order
                  AND r.calculation_kind = t.calculation_kind
                  AND r.treatment = t.treatment
                  AND r.rate_ppm IS t.rate_ppm
                  AND r.fixed_amount_minor IS t.fixed_amount_minor
                  AND r.fixed_currency IS t.fixed_currency
                  AND r.fixed_basis_qty_milli IS t.fixed_basis_qty_milli
                  AND r.base_kind = t.base_kind
                WHERE st.id = NEW.store_id)))))
BEGIN
  SELECT RAISE(ABORT, 'completed lines require exactly the applicable tax component snapshots');
END;
CREATE TRIGGER sale_completed_requires_tax_components_update
BEFORE UPDATE OF status, store_id, doc_type, business_date ON sale
WHEN NEW.status = 'completed' AND EXISTS (
  SELECT 1 FROM sale_line l
   WHERE l.sale_id = NEW.id
     AND (l.tax_category_id IS NULL
       OR NOT EXISTS (SELECT 1 FROM sale_line_tax t WHERE t.sale_line_id = l.id)
       OR (NEW.doc_type = 'sale' AND NOT EXISTS (
            SELECT 1 FROM store st JOIN tax_rate r
              ON r.rule_pack_id = st.tax_rule_pack_id
             AND r.tax_category_id = l.tax_category_id
             AND r.valid_from <= NEW.business_date
             AND (r.valid_to IS NULL OR r.valid_to > NEW.business_date)
             AND r.deleted_at IS NULL
           WHERE st.id = NEW.store_id))
       OR (NEW.doc_type = 'sale' AND EXISTS (
            SELECT 1 FROM store st JOIN tax_rate r
              ON r.rule_pack_id = st.tax_rule_pack_id
             AND r.tax_category_id = l.tax_category_id
             AND r.valid_from <= NEW.business_date
             AND (r.valid_to IS NULL OR r.valid_to > NEW.business_date)
             AND r.deleted_at IS NULL
             LEFT JOIN sale_line_tax t
               ON t.sale_line_id = l.id
              AND t.component_code = r.component_code
              AND t.calculation_order = r.calculation_order
              AND t.calculation_kind = r.calculation_kind
              AND t.treatment = r.treatment
              AND t.rate_ppm IS r.rate_ppm
              AND t.fixed_amount_minor IS r.fixed_amount_minor
              AND t.fixed_currency IS r.fixed_currency
              AND t.fixed_basis_qty_milli IS r.fixed_basis_qty_milli
              AND t.base_kind = r.base_kind
           WHERE st.id = NEW.store_id AND t.id IS NULL))
       OR (NEW.doc_type = 'sale' AND EXISTS (
            SELECT 1 FROM sale_line_tax t
             WHERE t.sale_line_id = l.id
               AND NOT EXISTS (
                 SELECT 1 FROM store st JOIN tax_rate r
                   ON r.rule_pack_id = st.tax_rule_pack_id
                  AND r.tax_category_id = l.tax_category_id
                  AND r.valid_from <= NEW.business_date
                  AND (r.valid_to IS NULL OR r.valid_to > NEW.business_date)
                  AND r.deleted_at IS NULL
                  AND r.component_code = t.component_code
                  AND r.calculation_order = t.calculation_order
                  AND r.calculation_kind = t.calculation_kind
                  AND r.treatment = t.treatment
                  AND r.rate_ppm IS t.rate_ppm
                  AND r.fixed_amount_minor IS t.fixed_amount_minor
                  AND r.fixed_currency IS t.fixed_currency
                  AND r.fixed_basis_qty_milli IS t.fixed_basis_qty_milli
                  AND r.base_kind = t.base_kind
                WHERE st.id = NEW.store_id)))))
BEGIN
  SELECT RAISE(ABORT, 'completed lines require exactly the applicable tax component snapshots');
END;

CREATE TRIGGER sale_completed_discount_recap_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND NEW.discount_minor <> COALESCE((
  SELECT SUM(d.amount_minor)
    FROM sale_line_discount d JOIN sale_line l ON l.id = d.sale_line_id
   WHERE l.sale_id = NEW.id), 0)
BEGIN
  SELECT RAISE(ABORT, 'document discount recap must equal the sum of exact line allowances');
END;
CREATE TRIGGER sale_completed_discount_recap_update
BEFORE UPDATE OF status, discount_minor ON sale
WHEN NEW.status = 'completed' AND NEW.discount_minor <> COALESCE((
  SELECT SUM(d.amount_minor)
    FROM sale_line_discount d JOIN sale_line l ON l.id = d.sale_line_id
   WHERE l.sale_id = NEW.id), 0)
BEGIN
  SELECT RAISE(ABORT, 'document discount recap must equal the sum of exact line allowances');
END;

-- Receipt tax summary, stored not derived. A refund six months later, a reprint,
-- and the fiscal document all read the SAME numbers the customer saw.
CREATE TABLE sale_tax_summary (
  id                    BLOB PRIMARY KEY,
  sale_id               BLOB NOT NULL REFERENCES sale(id),
  component_code        TEXT NOT NULL,
  treatment             TEXT NOT NULL
                            CHECK (treatment IN ('standard','reduced','zero','exempt')),
  calculation_kind      TEXT NOT NULL
                            CHECK (calculation_kind IN ('ad_valorem','fixed_per_quantity')),
  rate_ppm              INTEGER CHECK (rate_ppm >= 0),
  fixed_amount_minor    INTEGER,
  fixed_currency        TEXT,
  fixed_basis_qty_milli INTEGER,
  calculation_order     INTEGER NOT NULL DEFAULT 0,
  base_kind             TEXT NOT NULL
                            CHECK (base_kind IN ('line_net','line_net_plus_prior_components','quantity')),
  taxable_base_minor    INTEGER,
  taxable_qty_milli     INTEGER,
  net_minor             INTEGER NOT NULL,
  tax_minor             INTEGER NOT NULL,
  gross_minor           INTEGER NOT NULL,
  CHECK (
    (calculation_kind = 'ad_valorem'
      AND rate_ppm IS NOT NULL
      AND fixed_amount_minor IS NULL
      AND fixed_currency IS NULL
      AND fixed_basis_qty_milli IS NULL
      AND taxable_base_minor IS NOT NULL
      AND taxable_qty_milli IS NULL
      AND base_kind IN ('line_net','line_net_plus_prior_components'))
    OR
    (calculation_kind = 'fixed_per_quantity'
      AND rate_ppm IS NULL
      AND fixed_amount_minor > 0
      AND fixed_currency IS NOT NULL
      AND fixed_basis_qty_milli > 0
      AND taxable_base_minor IS NULL
      AND taxable_qty_milli IS NOT NULL
      AND base_kind = 'quantity')
  )
) STRICT;
CREATE INDEX idx_sale_tax_summary_sale ON sale_tax_summary(sale_id);

-- Settlement is a transition fact, not an UPDATE to `sale_tender`. `event_no`
-- gives the register-owned order without relying on a device clock.
CREATE TABLE tender_status_event (
  id             BLOB PRIMARY KEY,
  tender_id      BLOB NOT NULL REFERENCES sale_tender(id),
  sync_commit_id BLOB NOT NULL REFERENCES sync_commit(id),
  event_no       INTEGER NOT NULL CHECK (event_no > 0),
  state          TEXT NOT NULL
                   CHECK (state IN ('pending','collected','reversed','unknown','failed')),
  psp_ref        TEXT,
  masked_pan     TEXT,                       -- receipt-only value from the PSP
  scheme         TEXT,
  reason_code    TEXT,
  occurred_at    TEXT NOT NULL,
  UNIQUE (tender_id, event_no)
) STRICT;

CREATE TABLE tender_status_current (
  tender_id      BLOB PRIMARY KEY REFERENCES sale_tender(id),
  event_no       INTEGER NOT NULL CHECK (event_no > 0),
  state          TEXT NOT NULL
                   CHECK (state IN ('pending','collected','reversed','unknown','failed')),
  latest_event_id BLOB NOT NULL REFERENCES tender_status_event(id),
  psp_ref        TEXT,
  occurred_at    TEXT NOT NULL
) STRICT;

CREATE TRIGGER tender_status_event_is_next
BEFORE INSERT ON tender_status_event
WHEN NOT EXISTS (
       SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'tender_status_event' AND m.entity_id = NEW.id)
  OR NEW.event_no <> COALESCE((
  SELECT MAX(event_no) + 1 FROM tender_status_event WHERE tender_id = NEW.tender_id), 1)
BEGIN
  SELECT RAISE(ABORT, 'tender status events require a complete commit and append in register order');
END;

CREATE TRIGGER tender_status_event_transition_allowed
BEFORE INSERT ON tender_status_event
WHEN (NEW.event_no = 1 AND NEW.state = 'reversed')
  OR (NEW.event_no > 1 AND NOT EXISTS (
       SELECT 1 FROM tender_status_event prior
        WHERE prior.tender_id = NEW.tender_id
          AND prior.event_no = NEW.event_no - 1
          AND ((prior.state = 'pending' AND NEW.state IN ('collected','unknown','failed'))
            OR (prior.state = 'unknown' AND NEW.state IN ('collected','failed'))
            OR (prior.state = 'failed' AND NEW.state = 'pending')
            OR (prior.state = 'collected' AND NEW.state = 'reversed'))))
BEGIN
  SELECT RAISE(ABORT, 'invalid tender status transition');
END;

CREATE TRIGGER tender_status_project_current
AFTER INSERT ON tender_status_event
BEGIN
  INSERT INTO tender_status_current
    (tender_id, event_no, state, latest_event_id, psp_ref, occurred_at)
  VALUES
    (NEW.tender_id, NEW.event_no, NEW.state, NEW.id, NEW.psp_ref, NEW.occurred_at)
  ON CONFLICT(tender_id) DO UPDATE SET
    event_no = excluded.event_no,
    state = excluded.state,
    latest_event_id = excluded.latest_event_id,
    psp_ref = excluded.psp_ref,
    occurred_at = excluded.occurred_at;
END;

CREATE TRIGGER tender_status_current_matches_event_insert
BEFORE INSERT ON tender_status_current
WHEN NOT EXISTS (
  SELECT 1 FROM tender_status_event e
   WHERE e.id = NEW.latest_event_id AND e.tender_id = NEW.tender_id
     AND e.event_no = NEW.event_no AND e.state = NEW.state
     AND e.occurred_at = NEW.occurred_at
     AND e.event_no = (SELECT MAX(x.event_no) FROM tender_status_event x
                        WHERE x.tender_id = NEW.tender_id))
BEGIN
  SELECT RAISE(ABORT, 'tender projection must match its latest event');
END;
CREATE TRIGGER tender_status_current_matches_event_update
BEFORE UPDATE ON tender_status_current
WHEN NOT EXISTS (
  SELECT 1 FROM tender_status_event e
   WHERE e.id = NEW.latest_event_id AND e.tender_id = NEW.tender_id
     AND e.event_no = NEW.event_no AND e.state = NEW.state
     AND e.occurred_at = NEW.occurred_at
     AND e.event_no = (SELECT MAX(x.event_no) FROM tender_status_event x
                        WHERE x.tender_id = NEW.tender_id))
BEGIN
  SELECT RAISE(ABORT, 'tender projection must match its latest event');
END;
CREATE TRIGGER tender_status_current_no_delete
BEFORE DELETE ON tender_status_current BEGIN
  SELECT RAISE(ABORT, 'a tender projection is rebuilt, not selectively deleted');
END;

CREATE TRIGGER sale_completed_requires_tender_events_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND EXISTS (
  SELECT 1 FROM sale_tender t
   WHERE t.sale_id = NEW.id AND NOT EXISTS (
     SELECT 1 FROM tender_status_current c
      WHERE c.tender_id = t.id AND c.event_no >= 1))
BEGIN
  SELECT RAISE(ABORT, 'every completed-sale tender requires an initial status event');
END;
CREATE TRIGGER sale_completed_requires_tender_events_update
BEFORE UPDATE OF status ON sale
WHEN NEW.status = 'completed' AND EXISTS (
  SELECT 1 FROM sale_tender t
   WHERE t.sale_id = NEW.id AND NOT EXISTS (
     SELECT 1 FROM tender_status_current c
      WHERE c.tender_id = t.id AND c.event_no >= 1))
BEGIN
  SELECT RAISE(ABORT, 'every completed-sale tender requires an initial status event');
END;

```

> ⚠️ **OPEN — blocks 2.1.1.** Which exact PCI SAQ applies to the selected acquirer, terminal model and firmware, PTS/P2PE listing, integration protocol, store network and support model? Default until answered: design and operate to the SAQ C baseline, reject any integration that exposes a full PAN to this process, and make no P2PE-eligibility claim anywhere.
> Owner: `2.1.1` collects the evidence; `5.3.3` determines the SAQ. Source that settles it: the acquirer's written responsibility matrix and a QSA determination against the current PCI SSC eligibility criteria.

```sql
CREATE TABLE tender_type (
  code            TEXT PRIMARY KEY,
  name_ar         TEXT NOT NULL,
  name_en         TEXT,
  opens_drawer    INTEGER NOT NULL DEFAULT 0,
  allows_change   INTEGER NOT NULL DEFAULT 0,
  is_cash_counted INTEGER NOT NULL DEFAULT 0,
  refundable_to   TEXT NOT NULL DEFAULT 'same'
                    CHECK (refundable_to IN ('same','cash','store_credit','none')),
  sort_order      INTEGER NOT NULL DEFAULT 0,
  is_active       INTEGER NOT NULL DEFAULT 1
) STRICT;

-- Complete initial seed. Later payment/refund microsteps enable behavior; they
-- never reopen 0005 to append codes. `exchange` only offsets two linked
-- documents and therefore never opens or counts a drawer.
INSERT INTO tender_type
  (code, name_ar, name_en, opens_drawer, allows_change, is_cash_counted,
   refundable_to, sort_order, is_active)
VALUES
  ('cash',         'نقدي',          'Cash',         1, 1, 1, 'cash',         10, 1),
  ('card',         'بطاقة',         'Card',         0, 0, 0, 'same',         20, 0),
  ('cliq',         'كليك',          'CliQ',         0, 0, 0, 'same',         30, 0),
  ('voucher',      'قسيمة',         'Voucher',      0, 0, 0, 'none',         40, 0),
  ('store_credit', 'رصيد المتجر',   'Store credit', 0, 0, 0, 'store_credit', 50, 0),
  ('exchange',     'تسوية استبدال', 'Exchange',     0, 0, 0, 'none',         60, 0);

-- Parked carts are register-local and NEVER sync (master plan C.14).
CREATE TABLE parked_cart (
  id           BLOB PRIMARY KEY,
  register_id  BLOB NOT NULL REFERENCES register(id),
  cashier_id   BLOB NOT NULL REFERENCES app_user(id),
  label        TEXT,
  snapshot     TEXT NOT NULL,           -- serialized Cart
  parked_at    TEXT NOT NULL,
  expires_on   TEXT NOT NULL            -- end of business day (C.2)
) STRICT;

-- Register-local recovery journal. It exists before the sale fact and before an
-- external terminal call, so `sale_id` deliberately has no FK. The row is
-- removed only in the transaction that commits the complete fact graph,
-- grouped outbox rows, receipt artifact and print job.
CREATE TABLE checkout_operation (
  id                 BLOB PRIMARY KEY,
  sale_id            BLOB NOT NULL UNIQUE,
  register_id        BLOB NOT NULL REFERENCES register(id),
  shift_id           BLOB NOT NULL REFERENCES shift(id),
  actor_id           BLOB NOT NULL REFERENCES app_user(id),
  state              TEXT NOT NULL CHECK (state IN ('tendering','finalizing')),
  priced_snapshot    TEXT NOT NULL,
  idempotency_key    TEXT NOT NULL UNIQUE,
  transition_version INTEGER NOT NULL DEFAULT 0,
  terminal_sale_ref  TEXT,
  terminal_state     TEXT,
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL
) STRICT;

CREATE TABLE receipt_template (
  id               BLOB PRIMARY KEY,
  org_id           BLOB NOT NULL REFERENCES org(id),
  template_version TEXT NOT NULL,
  locale           TEXT NOT NULL CHECK (locale IN ('ar','en','bilingual')),
  format           TEXT NOT NULL,
  body_json        TEXT NOT NULL,
  content_hash     BLOB NOT NULL,
  created_at       TEXT NOT NULL,
  UNIQUE (org_id, template_version, locale, format)
) STRICT;

CREATE TABLE receipt_artifact (
  id                 BLOB PRIMARY KEY,
  sale_id            BLOB NOT NULL REFERENCES sale(id),
  artifact_kind      TEXT NOT NULL
                       CHECK (artifact_kind IN ('original','duplicate','fiscal_supplement')),
  source_artifact_id BLOB REFERENCES receipt_artifact(id),
  format             TEXT NOT NULL,
  template_version   TEXT NOT NULL,
  printer_profile    TEXT NOT NULL,
  fiscal_version     TEXT,
  content_bytes      BLOB NOT NULL,
  content_hash       BLOB NOT NULL,
  generated_at       TEXT NOT NULL,
  CHECK (
    (artifact_kind = 'original' AND source_artifact_id IS NULL)
    OR (artifact_kind IN ('duplicate','fiscal_supplement') AND source_artifact_id IS NOT NULL)
  )
) STRICT;
CREATE INDEX idx_receipt_artifact_sale ON receipt_artifact(sale_id, generated_at);
CREATE UNIQUE INDEX idx_receipt_artifact_original
  ON receipt_artifact(sale_id) WHERE artifact_kind = 'original';

CREATE TRIGGER receipt_artifact_source_matches_sale
BEFORE INSERT ON receipt_artifact
WHEN NEW.source_artifact_id IS NOT NULL AND NOT EXISTS (
  SELECT 1 FROM receipt_artifact source
   WHERE source.id = NEW.source_artifact_id
     AND source.sale_id = NEW.sale_id
     AND source.artifact_kind = 'original')
BEGIN
  SELECT RAISE(ABORT, 'a duplicate or fiscal supplement must cite this sale original artifact');
END;

CREATE TABLE print_job (
  id              BLOB PRIMARY KEY,
  artifact_id     BLOB NOT NULL UNIQUE REFERENCES receipt_artifact(id),
  state           TEXT NOT NULL DEFAULT 'queued'
                    CHECK (state IN ('queued','printing','unknown','printed','failed','cancelled')),
  attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  next_attempt_at TEXT,
  claimed_at      TEXT,
  lease_owner     TEXT,
  lease_expires_at TEXT,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  last_error      TEXT,
  CHECK (
    (state = 'printing' AND claimed_at IS NOT NULL
      AND lease_owner IS NOT NULL AND lease_expires_at IS NOT NULL)
    OR (state <> 'printing' AND claimed_at IS NULL
      AND lease_owner IS NULL AND lease_expires_at IS NULL)
  )
) STRICT;
CREATE INDEX idx_print_job_pending ON print_job(state, next_attempt_at)
  WHERE state IN ('queued','failed');
CREATE INDEX idx_print_job_expired_claim ON print_job(lease_expires_at)
  WHERE state = 'printing';

CREATE TABLE print_attempt (
  id            BLOB PRIMARY KEY,
  job_id        BLOB NOT NULL REFERENCES print_job(id),
  attempt_no    INTEGER NOT NULL CHECK (attempt_no > 0),
  outcome       TEXT NOT NULL
                  CHECK (outcome IN ('printed','unknown','partial','failed','cancelled')),
  error_code    TEXT,
  started_at    TEXT NOT NULL,
  finished_at   TEXT,
  sent_at       TEXT,
  retry_at      TEXT,
  UNIQUE (job_id, attempt_no)
) STRICT;

CREATE TRIGGER print_attempt_is_next_transition
BEFORE INSERT ON print_attempt
WHEN NOT EXISTS (
       SELECT 1 FROM print_job j
        WHERE j.id = NEW.job_id
          AND NEW.attempt_no = j.attempts + 1
          AND ((NEW.outcome IN ('printed','unknown','partial','failed')
                 AND j.state = 'printing')
            OR (NEW.outcome = 'cancelled'
                 AND j.state IN ('queued','failed','unknown'))))
  OR (NEW.outcome = 'printed' AND (NEW.finished_at IS NULL OR NEW.sent_at IS NULL))
  OR (NEW.outcome = 'failed' AND NEW.finished_at IS NULL)
  OR (NEW.outcome <> 'failed' AND NEW.retry_at IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'print attempts must append once and match the claimed job transition');
END;

CREATE TRIGGER print_attempt_projects_job
AFTER INSERT ON print_attempt
BEGIN
  UPDATE print_job
     SET state = CASE NEW.outcome
                   WHEN 'printed' THEN 'printed'
                   WHEN 'failed' THEN 'failed'
                   WHEN 'cancelled' THEN 'cancelled'
                   ELSE 'unknown'
                 END,
         attempts = NEW.attempt_no,
         next_attempt_at = CASE WHEN NEW.outcome = 'failed' THEN NEW.retry_at END,
         claimed_at = NULL,
         lease_owner = NULL,
         lease_expires_at = NULL,
         updated_at = COALESCE(NEW.finished_at, NEW.started_at),
         last_error = NEW.error_code
   WHERE id = NEW.job_id;
  SELECT CASE WHEN changes() <> 1
    THEN RAISE(ABORT, 'print transition requires its job projection') END;
END;

CREATE TRIGGER print_job_identity_frozen
BEFORE UPDATE ON print_job
WHEN NEW.id IS NOT OLD.id OR NEW.artifact_id IS NOT OLD.artifact_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN
  SELECT RAISE(ABORT, 'print job identity and artifact are immutable');
END;

CREATE TRIGGER print_job_state_transition_allowed
BEFORE UPDATE ON print_job
WHEN NOT (
  -- Claim or renew a lease. Unknown is deliberately absent: an ambiguous
  -- hardware effect is never replayed automatically.
  (OLD.state IN ('queued','failed') AND NEW.state = 'printing'
    AND NEW.attempts = OLD.attempts AND NEW.claimed_at IS NOT NULL
    AND NEW.lease_owner IS NOT NULL AND NEW.lease_expires_at IS NOT NULL
    AND NEW.next_attempt_at IS NULL)
  OR (OLD.state = 'printing' AND NEW.state = 'printing'
    AND NEW.attempts = OLD.attempts AND NEW.lease_owner = OLD.lease_owner
    AND NEW.claimed_at = OLD.claimed_at AND NEW.lease_expires_at >= OLD.lease_expires_at)
  -- The AFTER INSERT projection is accepted only when the just-appended event
  -- explains every state field. A direct printed/failed/unknown update has no
  -- matching event and is refused.
  OR EXISTS (
    SELECT 1 FROM print_attempt a
     WHERE a.job_id = NEW.id AND a.attempt_no = OLD.attempts + 1
       AND NEW.attempts = a.attempt_no
       AND NEW.state = CASE a.outcome
                         WHEN 'printed' THEN 'printed'
                         WHEN 'failed' THEN 'failed'
                         WHEN 'cancelled' THEN 'cancelled'
                         ELSE 'unknown'
                       END
       AND NEW.next_attempt_at IS CASE WHEN a.outcome = 'failed' THEN a.retry_at END
       AND NEW.claimed_at IS NULL AND NEW.lease_owner IS NULL
       AND NEW.lease_expires_at IS NULL
       AND NEW.updated_at = COALESCE(a.finished_at, a.started_at)
       AND NEW.last_error IS a.error_code))
BEGIN
  SELECT RAISE(ABORT, 'print job state changes only by claim or appended attempt');
END;

CREATE VIEW sale_commit_base_complete AS
SELECT s.id AS sale_id, root.commit_id AS sync_commit_id
  FROM sale s
  JOIN fact_commit_member root ON root.entity = 'sale' AND root.entity_id = s.id
  JOIN sync_commit_ready ready ON ready.id = root.commit_id
 WHERE 1 = 1
   AND NOT EXISTS (
       SELECT 1 FROM sale_line l WHERE l.sale_id = s.id AND NOT EXISTS (
         SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
          AND m.entity = 'sale_line' AND m.entity_id = l.id))
   AND NOT EXISTS (
       SELECT 1 FROM sale_tender t WHERE t.sale_id = s.id AND NOT EXISTS (
         SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
          AND m.entity = 'sale_tender' AND m.entity_id = t.id))
   AND NOT EXISTS (
       SELECT 1 FROM sale_line_tax t JOIN sale_line l ON l.id = t.sale_line_id
        WHERE l.sale_id = s.id AND NOT EXISTS (
          SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
           AND m.entity = 'sale_line_tax' AND m.entity_id = t.id))
   AND NOT EXISTS (
       SELECT 1 FROM sale_line_discount d JOIN sale_line l ON l.id = d.sale_line_id
        WHERE l.sale_id = s.id AND NOT EXISTS (
          SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
           AND m.entity = 'sale_line_discount' AND m.entity_id = d.id))
   AND NOT EXISTS (
       SELECT 1 FROM sale_tax_summary t WHERE t.sale_id = s.id AND NOT EXISTS (
         SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
          AND m.entity = 'sale_tax_summary' AND m.entity_id = t.id))
   AND NOT EXISTS (
       SELECT 1 FROM sale_supply_tax_context c WHERE c.sale_id = s.id AND NOT EXISTS (
         SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
          AND m.entity = 'sale_supply_tax_context' AND m.entity_id = c.sale_id))
   AND NOT EXISTS (
       SELECT 1 FROM receipt_artifact a WHERE a.sale_id = s.id AND NOT EXISTS (
         SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
          AND m.entity = 'receipt_artifact' AND m.entity_id = a.id))
   AND NOT EXISTS (
       SELECT 1 FROM tender_status_event e JOIN sale_tender t ON t.id = e.tender_id
        WHERE t.sale_id = s.id AND NOT EXISTS (
          SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
           AND m.entity = 'tender_status_event' AND m.entity_id = e.id))
   AND EXISTS (
       SELECT 1 FROM audit_log a WHERE a.entity = 'sale' AND a.entity_id = s.id)
   AND NOT EXISTS (
       SELECT 1 FROM audit_log a WHERE a.entity = 'sale' AND a.entity_id = s.id
        AND NOT EXISTS (
          SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
           AND m.entity = 'audit_log' AND m.entity_id = a.id))
   AND NOT EXISTS (
       SELECT 1 FROM approval_consumption c WHERE c.effect_id = s.id AND (
         NOT EXISTS (SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
                      AND m.entity = 'approval_consumption' AND m.entity_id = c.handle_id)
         OR NOT EXISTS (SELECT 1 FROM fact_commit_member m WHERE m.commit_id = root.commit_id
                        AND m.entity = 'approval_handle' AND m.entity_id = c.handle_id)));

CREATE TRIGGER sale_completed_requires_durable_outputs_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND (
  NOT EXISTS (
    SELECT 1 FROM receipt_artifact a JOIN print_job j ON j.artifact_id = a.id
     WHERE a.sale_id = NEW.id AND a.artifact_kind = 'original'
       AND j.state = 'queued' AND j.attempts = 0)
  OR NOT EXISTS (
    SELECT 1 FROM sale_commit_base_complete c
     WHERE c.sale_id = NEW.id AND c.sync_commit_id = NEW.sync_commit_id))
BEGIN
  SELECT RAISE(ABORT, 'sale completion atomically requires its original receipt job and complete sync commit');
END;
CREATE TRIGGER sale_completed_requires_durable_outputs_update
BEFORE UPDATE OF status ON sale
WHEN NEW.status = 'completed' AND (
  NOT EXISTS (
    SELECT 1 FROM receipt_artifact a JOIN print_job j ON j.artifact_id = a.id
     WHERE a.sale_id = NEW.id AND a.artifact_kind = 'original'
       AND j.state = 'queued' AND j.attempts = 0)
  OR NOT EXISTS (
    SELECT 1 FROM sale_commit_base_complete c
     WHERE c.sale_id = NEW.id AND c.sync_commit_id = NEW.sync_commit_id))
BEGIN
  SELECT RAISE(ABORT, 'sale completion atomically requires its original receipt job and complete sync commit');
END;

-- `unknown`/`partial` is deliberately absent from the worker index: the printer
-- may already have produced paper. Only an operator action may create a linked
-- DUPLICATE artifact and a new job; an automatic byte replay would issue a
-- second original and could repeat non-idempotent hardware effects.
-- An expired `printing` lease is reclaimed by appending an `unknown` attempt
-- and projecting the job to `unknown`; it is never changed back to `queued`.

-- Register-local prepared intent for manager-approved emergency catalogue work.
-- The eventual product id is allocated before approval, so the handle names the
-- exact effect. `content_hash` is BLAKE3 over a version byte, the
-- `product_quick_add` domain separator, and length-prefixed canonical encodings
-- of every other column in declaration order. The request is not a merchant
-- fact and never syncs; deletion is allowed only after the matching product,
-- barcode and approval consumption are visible in the same transaction.
CREATE TABLE product_quick_add_request (
  product_id       BLOB PRIMARY KEY,
  barcode          TEXT NOT NULL,
  name_ar          TEXT NOT NULL,
  unit_price_minor INTEGER NOT NULL CHECK (unit_price_minor >= 0),
  tax_category_id  BLOB NOT NULL REFERENCES tax_category(id),
  requested_by     BLOB NOT NULL REFERENCES app_user(id),
  requested_at     TEXT NOT NULL,
  content_hash     BLOB NOT NULL CHECK (length(content_hash) = 32)
) STRICT;

CREATE TRIGGER product_quick_add_approval_hash_matches
BEFORE INSERT ON approval_handle
WHEN NEW.capability = 'product.edit'
 AND EXISTS (SELECT 1 FROM product_quick_add_request r WHERE r.product_id = NEW.entity_id)
 AND NOT EXISTS (
   SELECT 1 FROM product_quick_add_request r
    WHERE r.product_id = NEW.entity_id AND NEW.content_hash IS r.content_hash)
BEGIN
  SELECT RAISE(ABORT, 'quick-add approval must bind the prepared intent content hash');
END;

CREATE TRIGGER product_quick_add_request_no_update_after_approval
BEFORE UPDATE ON product_quick_add_request
WHEN EXISTS (
  SELECT 1 FROM approval_handle h
   WHERE h.capability = 'product.edit' AND h.entity_id = OLD.product_id)
BEGIN
  SELECT RAISE(ABORT, 'prepared quick-add intent is immutable after approval');
END;

CREATE TRIGGER product_quick_add_request_delete_only_with_effect
BEFORE DELETE ON product_quick_add_request
WHEN NOT EXISTS (
  SELECT 1
    FROM product p
    JOIN barcode b ON b.product_id = p.id
                  AND b.code = OLD.barcode AND b.deleted_at IS NULL
    JOIN approval_consumption c ON c.effect_id = p.id
   WHERE p.id = OLD.product_id
     AND COALESCE(p.name_ar, p.name) = OLD.name_ar
     AND p.price_minor = OLD.unit_price_minor
     AND p.tax_category_id = OLD.tax_category_id)
BEGIN
  SELECT RAISE(ABORT, 'prepared quick-add intent is removed only with its approved product effect');
END;

-- Sequence integrity (G-2). Counters, never derived from time (E.6).
-- Receipt and Z counters are bumped in the SAME transaction as the document
-- they number. `fiscal_icv` is different: the sale transaction queues a local
-- `fiscal_uuid` with `icv IS NULL`. In Phase 2 the single register locks its own
-- store-scoped row in-process at first submission and records that register in
-- `allocator_ref`. From Phase 3 the server owns allocation and issues one-value
-- leases; a register without a lease leaves ICV NULL. Either outage delays
-- clearance, never selling.
CREATE TABLE doc_sequence (
  scope_kind  TEXT NOT NULL CHECK (scope_kind IN ('register','store')),
  scope_id    BLOB NOT NULL,
  kind        TEXT NOT NULL CHECK (kind IN ('receipt','zreport','fiscal_icv')),
  next_value  INTEGER NOT NULL DEFAULT 1 CHECK (next_value > 0),
  prefix      TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (scope_kind, scope_id, kind),
  CHECK (
    (scope_kind = 'register' AND kind IN ('receipt','zreport'))
    OR (scope_kind = 'store' AND kind = 'fiscal_icv')
  )
) STRICT;

CREATE TRIGGER doc_sequence_scope_exists_insert
BEFORE INSERT ON doc_sequence
WHEN (NEW.scope_kind = 'register' AND NOT EXISTS (
        SELECT 1 FROM register WHERE id = NEW.scope_id))
  OR (NEW.scope_kind = 'store' AND NOT EXISTS (
        SELECT 1 FROM store WHERE id = NEW.scope_id))
BEGIN
  SELECT RAISE(ABORT, 'doc_sequence scope_id does not exist for scope_kind');
END;
CREATE TRIGGER doc_sequence_scope_exists_update
BEFORE UPDATE OF scope_kind, scope_id ON doc_sequence
WHEN (NEW.scope_kind = 'register' AND NOT EXISTS (
        SELECT 1 FROM register WHERE id = NEW.scope_id))
  OR (NEW.scope_kind = 'store' AND NOT EXISTS (
        SELECT 1 FROM store WHERE id = NEW.scope_id))
BEGIN
  SELECT RAISE(ABORT, 'doc_sequence scope_id does not exist for scope_kind');
END;

CREATE TRIGGER doc_sequence_monotonic
BEFORE UPDATE OF next_value ON doc_sequence
WHEN NEW.next_value <> OLD.next_value + 1
BEGIN
  SELECT RAISE(ABORT, 'document sequences advance by exactly one');
END;

-- Time confidence is a Phase-1 sale input, not a Phase-3 sync feature. Tax-rule
-- choice, business date and fiscal issue date may branch only when confidence
-- is sufficient; UUID identity and outbox order never come from this clock.
CREATE TABLE trusted_time_state (
  register_id             BLOB PRIMARY KEY REFERENCES register(id),
  authenticated_server_at TEXT,
  monotonic_elapsed_milli INTEGER NOT NULL DEFAULT 0 CHECK (monotonic_elapsed_milli >= 0),
  confidence              TEXT NOT NULL DEFAULT 'never_trusted'
                            CHECK (confidence IN ('never_trusted','trusted','degraded','anomalous')),
  observed_skew_milli     INTEGER,
  updated_at              TEXT NOT NULL
) STRICT;

CREATE TRIGGER shift_no_update
BEFORE UPDATE ON shift BEGIN
  SELECT RAISE(ABORT, 'a shift opening fact is immutable');
END;
CREATE TRIGGER shift_no_delete
BEFORE DELETE ON shift BEGIN
  SELECT RAISE(ABORT, 'a shift opening fact cannot be deleted');
END;
CREATE TRIGGER shift_close_event_no_update
BEFORE UPDATE ON shift_close_event BEGIN
  SELECT RAISE(ABORT, 'a shift close event is immutable');
END;
CREATE TRIGGER shift_close_event_no_delete
BEFORE DELETE ON shift_close_event BEGIN
  SELECT RAISE(ABORT, 'a shift close event cannot be deleted');
END;

CREATE TRIGGER tender_status_event_no_update
BEFORE UPDATE ON tender_status_event BEGIN
  SELECT RAISE(ABORT, 'tender settlement is append-only');
END;
CREATE TRIGGER tender_status_event_no_delete
BEFORE DELETE ON tender_status_event BEGIN
  SELECT RAISE(ABORT, 'tender settlement history cannot be deleted');
END;

CREATE TRIGGER receipt_template_no_update
BEFORE UPDATE ON receipt_template BEGIN
  SELECT RAISE(ABORT, 'a receipt template version is immutable');
END;
CREATE TRIGGER receipt_template_no_delete
BEFORE DELETE ON receipt_template BEGIN
  SELECT RAISE(ABORT, 'a referenced receipt template cannot be deleted');
END;
CREATE TRIGGER receipt_artifact_no_update
BEFORE UPDATE ON receipt_artifact BEGIN
  SELECT RAISE(ABORT, 'receipt bytes are immutable — create a linked artifact');
END;
CREATE TRIGGER receipt_artifact_no_delete
BEFORE DELETE ON receipt_artifact BEGIN
  SELECT RAISE(ABORT, 'receipt evidence cannot be deleted');
END;
CREATE TRIGGER print_attempt_no_update
BEFORE UPDATE ON print_attempt BEGIN
  SELECT RAISE(ABORT, 'print attempts are append-only');
END;
CREATE TRIGGER print_attempt_no_delete
BEFORE DELETE ON print_attempt BEGIN
  SELECT RAISE(ABORT, 'print attempt history cannot be deleted');
END;

-- ── I-4 on the per-sale tax summary ────────────────────────────────────────
--
-- This is the table the filing report reads. If it can move after the sale
-- completes, the return and the receipts stop agreeing, and the exempt versus
-- zero-rated distinction the whole tax design protects becomes editable.

CREATE TRIGGER sale_tax_summary_no_insert_once_completed
BEFORE INSERT ON sale_tax_summary
WHEN (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: cannot add a tax summary row to a completed sale');
END;

CREATE TRIGGER sale_tax_summary_no_update_once_completed
BEFORE UPDATE ON sale_tax_summary
-- BOTH parents — see the note on sale_line_tax above.
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
  OR (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: the tax summary of a completed sale is immutable');
END;

CREATE TRIGGER sale_tax_summary_no_delete_once_completed
BEFORE DELETE ON sale_tax_summary
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: the tax summary of a completed sale cannot be deleted');
END;

```

---

## 0006 — stock ledger  ·  Phase 1, microstep 1.10.1

```sql
-- Stock is a LEDGER, not a column (I-6). On-hand = Σ qty_delta_milli.
CREATE TABLE stock_ledger (
  id              BLOB PRIMARY KEY,
  register_id     BLOB NOT NULL REFERENCES register(id),
  event_seq       INTEGER NOT NULL CHECK (event_seq > 0),
  product_id      BLOB NOT NULL REFERENCES product(id),
  store_id        BLOB NOT NULL REFERENCES store(id),
  qty_delta_milli INTEGER NOT NULL,       -- negative on sale
  qty_step_milli  INTEGER NOT NULL CHECK (qty_step_milli > 0),
  kind            TEXT NOT NULL CHECK (kind IN (
                    'sale','refund_restock','refund_damage','receive','adjust',
                    'count_correction','transfer_out','transfer_in','waste','rtv','kit_explode')),
  reason_code     TEXT,                    -- damage|theft|expiry|correction
  note            TEXT,
  ref_kind        TEXT,                    -- 'sale','goods_receipt','stock_count'
  ref_id          BLOB,
  source_sale_line_id BLOB REFERENCES sale_line(id),
  -- Every event captures the cost basis in force. A sale therefore keeps its
  -- historical COGS when today's WAC changes. NULL means no basis existed and
  -- requires `is_cost_estimated = 1`; a projected non-NULL value may also be
  -- estimated, while a legitimate observed zero remains non-NULL zero.
  unit_cost_minor INTEGER,
  is_cost_estimated INTEGER NOT NULL DEFAULT 0 CHECK (is_cost_estimated IN (0,1)),
  -- Set only when the quantity came from a price-embedded label rather than a
  -- measurement. It is provenance, not a licence to recompute the sale line.
  is_weight_derived INTEGER NOT NULL DEFAULT 0 CHECK (is_weight_derived IN (0,1)),
  -- The pure domain calculation writes the post-event projection into the
  -- immutable event. That makes a cache rebuild replayable without today's
  -- WAC and lets the database reject a skipped/out-of-order cache update.
  on_hand_after_milli INTEGER NOT NULL,
  wac_after_minor     INTEGER NOT NULL CHECK (wac_after_minor >= 0),
  is_wac_estimated    INTEGER NOT NULL CHECK (is_wac_estimated IN (0,1)),
  actor_id        BLOB,
  occurred_at     TEXT NOT NULL,
  business_date   TEXT NOT NULL,
  UNIQUE (register_id, event_seq),
  CHECK ((unit_cost_minor IS NULL AND is_cost_estimated = 1)
      OR (unit_cost_minor IS NOT NULL AND unit_cost_minor >= 0)),
  CHECK ((kind = 'sale' AND ref_kind = 'sale' AND ref_id IS NOT NULL
           AND source_sale_line_id IS NOT NULL AND qty_delta_milli < 0)
      OR kind <> 'sale')
) STRICT;
CREATE INDEX idx_stock_ledger_product_store ON stock_ledger(product_id, store_id, occurred_at);
CREATE INDEX idx_stock_ledger_ref           ON stock_ledger(ref_kind, ref_id);
CREATE UNIQUE INDEX idx_stock_sale_line
  ON stock_ledger(source_sale_line_id) WHERE kind = 'sale';

-- Prepared financial intent is register-local and cannot be edited after a
-- manager sees it. `content_hash` is BLAKE3 over a version byte, the
-- `stock_adjustment` domain separator, and length-prefixed canonical encodings
-- of every other column in declaration order. The eventual ledger id is
-- preallocated, so the consuming command accepts only that id plus approval;
-- deletion is possible only after the matching immutable event and consumption
-- fact exist in the transaction.
CREATE TABLE stock_adjustment_request (
  stock_event_id  BLOB PRIMARY KEY,
  product_id      BLOB NOT NULL REFERENCES product(id),
  qty_delta_milli INTEGER NOT NULL CHECK (qty_delta_milli <> 0),
  reason_code     TEXT NOT NULL CHECK (reason_code IN (
                    'opening_stock','damage','theft','expiry','count_correction')),
  note            TEXT,
  requested_by    BLOB NOT NULL REFERENCES app_user(id),
  requested_at    TEXT NOT NULL,
  content_hash    BLOB NOT NULL CHECK (length(content_hash) = 32)
) STRICT;

CREATE TRIGGER stock_adjustment_approval_hash_matches
BEFORE INSERT ON approval_handle
WHEN NEW.capability = 'stock.adjust'
 AND EXISTS (SELECT 1 FROM stock_adjustment_request r WHERE r.stock_event_id = NEW.entity_id)
 AND NOT EXISTS (
   SELECT 1 FROM stock_adjustment_request r
    WHERE r.stock_event_id = NEW.entity_id AND NEW.content_hash IS r.content_hash)
BEGIN
  SELECT RAISE(ABORT, 'stock-adjust approval must bind the prepared intent content hash');
END;

CREATE TRIGGER stock_adjustment_request_no_update_after_approval
BEFORE UPDATE ON stock_adjustment_request
WHEN EXISTS (
  SELECT 1 FROM approval_handle h
   WHERE h.capability = 'stock.adjust' AND h.entity_id = OLD.stock_event_id)
BEGIN
  SELECT RAISE(ABORT, 'prepared stock intent is immutable after approval');
END;

CREATE TRIGGER stock_adjustment_request_delete_only_with_effect
BEFORE DELETE ON stock_adjustment_request
WHEN NOT EXISTS (
  SELECT 1
    FROM stock_ledger e
    JOIN approval_consumption c ON c.effect_id = e.id
   WHERE e.id = OLD.stock_event_id
     AND e.product_id = OLD.product_id
     AND e.qty_delta_milli = OLD.qty_delta_milli
     AND e.reason_code = OLD.reason_code
     AND e.note IS OLD.note
     AND e.kind = 'adjust')
BEGIN
  SELECT RAISE(ABORT, 'prepared stock intent is removed only with its approved ledger effect');
END;

-- A CACHE, not a truth. `stock_cache_rebuild` regenerates it from the ledger
-- and CI asserts the rebuild is a no-op on the seeded fixture (I-6).
CREATE TABLE stock_cache (
  product_id             BLOB NOT NULL REFERENCES product(id),
  store_id               BLOB NOT NULL REFERENCES store(id),
  on_hand_milli          INTEGER NOT NULL DEFAULT 0,
  wac_minor              INTEGER NOT NULL DEFAULT 0,
  is_wac_estimated       INTEGER NOT NULL DEFAULT 1 CHECK (is_wac_estimated IN (0,1)),
  last_event_register_id BLOB REFERENCES register(id),
  last_event_seq         INTEGER,
  last_event_id          BLOB REFERENCES stock_ledger(id),
  event_count            INTEGER NOT NULL DEFAULT 0 CHECK (event_count >= 0),
  last_event_at          TEXT,
  PRIMARY KEY (product_id, store_id)
) STRICT;
CREATE INDEX idx_stock_cache_negative ON stock_cache(store_id) WHERE on_hand_milli < 0;  -- C.7

-- ── I-6: stock is a ledger, and a ledger is append-only ────────────────────
--
-- On-hand is SUM(qty_delta), cached and rebuildable. That only holds if the
-- events are never edited: an UPDATE here changes history retroactively, and
-- the cache rebuild "correctly" reproduces the altered past, so the two agree
-- and nothing looks wrong. Corrections are new events, which is what a ledger
-- is for. Each append advances the cache watermark in the same transaction;
-- startup and periodic verification compare count, last event and SUM(qty) and
-- atomically rebuild on any mismatch, because a green property test cannot
-- detect a missed update in a live register.

-- `append_stock_event(tx, event)` calls domain `recompute_wac` first, then
-- inserts the event. For a positive event that function applies the named WAC
-- rule (including the non-positive-on-hand reset); for an outgoing event WAC
-- cannot move. The trigger verifies the ledger chain and outgoing case before
-- advancing the cache. A crash rolls back both writes.
CREATE TRIGGER stock_ledger_projection_input_matches
BEFORE INSERT ON stock_ledger
WHEN NEW.event_seq <> COALESCE((
       SELECT MAX(e.event_seq) FROM stock_ledger e
        WHERE e.register_id = NEW.register_id), 0) + 1
  OR NEW.on_hand_after_milli <> COALESCE((
       SELECT c.on_hand_milli FROM stock_cache c
        WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 0)
       + NEW.qty_delta_milli
  OR (NEW.qty_delta_milli < 0 AND (
       NEW.wac_after_minor <> COALESCE((
         SELECT c.wac_minor FROM stock_cache c
          WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 0)
       OR NEW.unit_cost_minor <> COALESCE((
         SELECT c.wac_minor FROM stock_cache c
          WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 0)
       OR NEW.is_cost_estimated <> COALESCE((
         SELECT c.is_wac_estimated FROM stock_cache c
          WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 1)
       OR NEW.is_wac_estimated <> COALESCE((
         SELECT c.is_wac_estimated FROM stock_cache c
          WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 1)))
  OR (NEW.qty_delta_milli = 0 AND (
       NEW.wac_after_minor <> COALESCE((
         SELECT c.wac_minor FROM stock_cache c
          WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 0)
       OR NEW.is_wac_estimated <> COALESCE((
         SELECT c.is_wac_estimated FROM stock_cache c
          WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 1)
       OR (NEW.kind <> 'refund_damage' AND (
         NEW.unit_cost_minor <> COALESCE((
           SELECT c.wac_minor FROM stock_cache c
            WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 0)
         OR NEW.is_cost_estimated <> COALESCE((
           SELECT c.is_wac_estimated FROM stock_cache c
            WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 1)))))
  OR (NEW.qty_delta_milli > 0 AND NEW.is_wac_estimated <>
       CASE WHEN COALESCE((
         SELECT c.on_hand_milli FROM stock_cache c
          WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 0) <= 0
       THEN NEW.is_cost_estimated
       ELSE max(NEW.is_cost_estimated, COALESCE((
         SELECT c.is_wac_estimated FROM stock_cache c
          WHERE c.product_id = NEW.product_id AND c.store_id = NEW.store_id), 1)) END)
  OR (NEW.kind = 'sale' AND NOT EXISTS (
       SELECT 1 FROM sale_line l JOIN sale s ON s.id = l.sale_id
        WHERE l.id = NEW.source_sale_line_id
          AND s.id = NEW.ref_id
          AND s.store_id = NEW.store_id
          AND l.product_id = NEW.product_id
          AND NEW.qty_delta_milli = -l.qty_milli))
BEGIN
  SELECT RAISE(ABORT, 'stock event must extend the ledger/cache chain with the captured WAC');
END;

CREATE TRIGGER stock_ledger_quantity_matches_product_step
BEFORE INSERT ON stock_ledger
WHEN NOT EXISTS (
  SELECT 1 FROM product p
   WHERE p.id = NEW.product_id
     AND NEW.qty_step_milli = p.qty_step_milli
     AND NEW.qty_delta_milli % NEW.qty_step_milli = 0)
BEGIN
  SELECT RAISE(ABORT, 'stock quantity must respect the product milli-unit step');
END;

CREATE TRIGGER stock_ledger_has_ready_commit
BEFORE INSERT ON stock_ledger
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'stock_ledger' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'stock fact requires its complete delivery envelope'); END;

CREATE TRIGGER stock_ledger_projects_cache
AFTER INSERT ON stock_ledger
BEGIN
  INSERT INTO stock_cache (
    product_id, store_id, on_hand_milli, wac_minor, is_wac_estimated,
    last_event_register_id, last_event_seq, last_event_id, event_count, last_event_at)
  VALUES (
    NEW.product_id, NEW.store_id, NEW.on_hand_after_milli, NEW.wac_after_minor,
    NEW.is_wac_estimated,
    NEW.register_id, NEW.event_seq, NEW.id, 1, NEW.occurred_at)
  ON CONFLICT(product_id, store_id) DO UPDATE SET
    on_hand_milli = excluded.on_hand_milli,
    wac_minor = excluded.wac_minor,
    is_wac_estimated = excluded.is_wac_estimated,
    last_event_register_id = excluded.last_event_register_id,
    last_event_seq = excluded.last_event_seq,
    last_event_id = excluded.last_event_id,
    event_count = stock_cache.event_count + 1,
    last_event_at = excluded.last_event_at;
END;

CREATE TRIGGER completed_sale_requires_stock_fact_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND (
  EXISTS (
    SELECT 1 FROM sale_line l JOIN product p ON p.id = l.product_id
     WHERE l.sale_id = NEW.id AND p.is_service = 0
       AND NOT EXISTS (
         SELECT 1 FROM stock_ledger e JOIN fact_commit_member m
           ON m.entity = 'stock_ledger' AND m.entity_id = e.id
          AND m.commit_id = NEW.sync_commit_id
          WHERE e.kind = 'sale' AND e.source_sale_line_id = l.id
            AND e.product_id = l.product_id AND e.store_id = NEW.store_id
            AND e.qty_delta_milli = -l.qty_milli))
  OR EXISTS (
    SELECT 1 FROM stock_ledger e
     WHERE e.kind = 'sale' AND e.ref_id = NEW.id
       AND NOT EXISTS (
         SELECT 1 FROM sale_line l JOIN product p ON p.id = l.product_id
          WHERE l.id = e.source_sale_line_id AND l.sale_id = NEW.id
            AND p.is_service = 0)))
BEGIN
  SELECT RAISE(ABORT, 'each stock-bearing sale line requires its exact stock fact in the sale commit');
END;

CREATE TRIGGER completed_sale_requires_stock_fact_update
BEFORE UPDATE OF status, sync_commit_id ON sale
WHEN NEW.status = 'completed' AND (
  EXISTS (
    SELECT 1 FROM sale_line l JOIN product p ON p.id = l.product_id
     WHERE l.sale_id = NEW.id AND p.is_service = 0
       AND NOT EXISTS (
         SELECT 1 FROM stock_ledger e JOIN fact_commit_member m
           ON m.entity = 'stock_ledger' AND m.entity_id = e.id
          AND m.commit_id = NEW.sync_commit_id
          WHERE e.kind = 'sale' AND e.source_sale_line_id = l.id
            AND e.product_id = l.product_id AND e.store_id = NEW.store_id
            AND e.qty_delta_milli = -l.qty_milli))
  OR EXISTS (
    SELECT 1 FROM stock_ledger e
     WHERE e.kind = 'sale' AND e.ref_id = NEW.id
       AND NOT EXISTS (
         SELECT 1 FROM sale_line l JOIN product p ON p.id = l.product_id
          WHERE l.id = e.source_sale_line_id AND l.sale_id = NEW.id
            AND p.is_service = 0)))
BEGIN
  SELECT RAISE(ABORT, 'each stock-bearing sale line requires its exact stock fact in the sale commit');
END;

CREATE TRIGGER stock_ledger_no_update
BEFORE UPDATE ON stock_ledger
BEGIN
  SELECT RAISE(ABORT, 'I-6: stock_ledger is append-only — post a correcting event');
END;

CREATE TRIGGER stock_ledger_no_delete
BEFORE DELETE ON stock_ledger
BEGIN
  SELECT RAISE(ABORT, 'I-6: stock_ledger is append-only — post a correcting event');
END;

```

---

## 0007 — search and scan rules  ·  Phase 1, microsteps 1.2.5–1.2.7

```sql
-- FTS5 over Arabic AND English names plus SKU. Budget: <50 ms over 50k SKUs.
--
-- `remove_diacritics 2` does NOT fold Arabic — it folds Latin diacritics only,
-- and treats tashkeel as separators. It stays because the English/SKU columns
-- still want it. Arabic matching is carried by the two generated columns defined
-- in 0003: `name_ar_exact` for precision, `name_ar_fold` for recall.
--
-- Raw `name_ar` is deliberately NOT indexed. Because the tokenizer treats
-- tashkeel as separators, a vocalized name shreds into single letters — "قَهْوَة"
-- contributes ق, ه, و, ة — and those tokens make a one-character prefix query
-- match unrelated products. Since 1.2.7 benchmarks search from one character,
-- that is a false-positive source, and it buys nothing: nobody types tashkeel
-- into a search box, so an exact match against a vocalized name never fires.
--
-- prefix='2 3' because 1.2.7 benchmarks search at 1–3 characters. Without a
-- prefix index every such query is a full scan of the term list, and the 50 ms
-- budget is not reachable — the declaration must exist before the table does,
-- and this migration is forward-only.
CREATE VIRTUAL TABLE product_fts USING fts5(
  name_ar_exact, name_ar_fold, name_en, sku, barcodes,
  content='',                                  -- external content, manually synced
  contentless_delete=1,                        -- delete by rowid alone (SQLite 3.43+)
  tokenize="unicode61 remove_diacritics 2",
  prefix='2 3'                                 -- search-as-you-type from 2 chars
);

CREATE TABLE product_fts_map (rowid INTEGER PRIMARY KEY, product_id BLOB NOT NULL UNIQUE) STRICT;

-- Triggers keep FTS in step with product and barcode writes.
--
-- `contentless_delete=1` is what makes these writable as ordinary SQL. Without
-- it, removing a row from a contentless FTS5 table means re-supplying every
-- original indexed value to a 'delete' command — and `barcodes` is an aggregate
-- over another table, so the old value is not reconstructable from OLD.* at all.
-- With it, a plain DELETE by rowid is enough, and every trigger below reduces to
-- the same "drop the row, re-index the product" shape.

CREATE TRIGGER product_ai AFTER INSERT ON product BEGIN
  INSERT INTO product_fts_map (product_id) VALUES (new.id);
  INSERT INTO product_fts (rowid, name_ar_exact, name_ar_fold, name_en, sku, barcodes)
  SELECT (SELECT rowid FROM product_fts_map WHERE product_id = new.id),
         new.name_ar_exact, new.name_ar_fold, new.name_en, new.sku,
         (SELECT COALESCE(group_concat(code, ' '), '')
            FROM barcode WHERE product_id = new.id AND deleted_at IS NULL)
   WHERE new.deleted_at IS NULL;
END;

CREATE TRIGGER product_au AFTER UPDATE ON product BEGIN
  DELETE FROM product_fts
   WHERE rowid = (SELECT rowid FROM product_fts_map WHERE product_id = new.id);
  INSERT INTO product_fts (rowid, name_ar_exact, name_ar_fold, name_en, sku, barcodes)
  SELECT (SELECT rowid FROM product_fts_map WHERE product_id = new.id),
         new.name_ar_exact, new.name_ar_fold, new.name_en, new.sku,
         (SELECT COALESCE(group_concat(code, ' '), '')
            FROM barcode WHERE product_id = new.id AND deleted_at IS NULL)
   WHERE new.deleted_at IS NULL;
END;

CREATE TRIGGER product_ad AFTER DELETE ON product BEGIN
  DELETE FROM product_fts
   WHERE rowid = (SELECT rowid FROM product_fts_map WHERE product_id = old.id);
  DELETE FROM product_fts_map WHERE product_id = old.id;
END;

-- Triggers cover future writes; this block makes the catalogue that already
-- exists when 0007 lands searchable immediately. Map tombstones too so a later
-- restore can reuse the stable rowid, but index only live products.
INSERT INTO product_fts_map (product_id)
SELECT id FROM product;
INSERT INTO product_fts (rowid, name_ar_exact, name_ar_fold, name_en, sku, barcodes)
SELECT m.rowid, p.name_ar_exact, p.name_ar_fold, p.name_en, p.sku,
       (SELECT COALESCE(group_concat(code, ' '), '')
          FROM barcode WHERE product_id = p.id AND deleted_at IS NULL)
  FROM product p JOIN product_fts_map m ON m.product_id = p.id
 WHERE p.deleted_at IS NULL;

-- A barcode write changes the parent product's `barcodes` column, so each one
-- re-indexes that product. Reference data is tombstoned rather than deleted
-- (deleted_at), which arrives as an UPDATE — hence barcode_au, without which a
-- retired code stays scannable for as long as the index lives.
CREATE TRIGGER barcode_ai AFTER INSERT ON barcode BEGIN
  DELETE FROM product_fts
   WHERE rowid = (SELECT rowid FROM product_fts_map WHERE product_id = new.product_id);
  INSERT INTO product_fts (rowid, name_ar_exact, name_ar_fold, name_en, sku, barcodes)
  SELECT m.rowid, p.name_ar_exact, p.name_ar_fold, p.name_en, p.sku,
         (SELECT COALESCE(group_concat(code, ' '), '')
            FROM barcode WHERE product_id = p.id AND deleted_at IS NULL)
    FROM product p JOIN product_fts_map m ON m.product_id = p.id
   WHERE p.id = new.product_id AND p.deleted_at IS NULL;
END;

CREATE TRIGGER barcode_au AFTER UPDATE ON barcode BEGIN
  -- Reassignment changes TWO aggregate strings. Rebuilding only NEW leaves the
  -- old product searchable by a code it no longer owns.
  DELETE FROM product_fts
   WHERE rowid = (SELECT rowid FROM product_fts_map WHERE product_id = old.product_id);
  DELETE FROM product_fts
   WHERE new.product_id <> old.product_id
     AND rowid = (SELECT rowid FROM product_fts_map WHERE product_id = new.product_id);
  INSERT INTO product_fts (rowid, name_ar_exact, name_ar_fold, name_en, sku, barcodes)
  SELECT m.rowid, p.name_ar_exact, p.name_ar_fold, p.name_en, p.sku,
         (SELECT COALESCE(group_concat(code, ' '), '')
            FROM barcode WHERE product_id = p.id AND deleted_at IS NULL)
    FROM product p JOIN product_fts_map m ON m.product_id = p.id
   WHERE p.id = old.product_id AND p.deleted_at IS NULL;
  INSERT INTO product_fts (rowid, name_ar_exact, name_ar_fold, name_en, sku, barcodes)
  SELECT m.rowid, p.name_ar_exact, p.name_ar_fold, p.name_en, p.sku,
         (SELECT COALESCE(group_concat(code, ' '), '')
            FROM barcode WHERE product_id = p.id AND deleted_at IS NULL)
    FROM product p JOIN product_fts_map m ON m.product_id = p.id
   WHERE new.product_id <> old.product_id
     AND p.id = new.product_id
     AND p.deleted_at IS NULL;
END;

CREATE TRIGGER barcode_ad AFTER DELETE ON barcode BEGIN
  DELETE FROM product_fts
   WHERE rowid = (SELECT rowid FROM product_fts_map WHERE product_id = old.product_id);
  INSERT INTO product_fts (rowid, name_ar_exact, name_ar_fold, name_en, sku, barcodes)
  SELECT m.rowid, p.name_ar_exact, p.name_ar_fold, p.name_en, p.sku,
         (SELECT COALESCE(group_concat(code, ' '), '')
            FROM barcode WHERE product_id = p.id AND deleted_at IS NULL)
    FROM product p JOIN product_fts_map m ON m.product_id = p.id
   WHERE p.id = old.product_id AND p.deleted_at IS NULL;
END;

-- Deli-scale barcodes: prefix means "the digits that follow are a weight/price".
-- Store-configured because every scale vendor picks a different layout (C.1).
CREATE TABLE trade_scale (
  id            BLOB PRIMARY KEY,
  store_id      BLOB NOT NULL REFERENCES store(id),
  maker         TEXT NOT NULL,
  model         TEXT NOT NULL,
  serial_number TEXT NOT NULL,
  UNIQUE (store_id, serial_number)
) STRICT;

CREATE TABLE trade_scale_verification (
  id                    BLOB PRIMARY KEY,
  trade_scale_id        BLOB NOT NULL REFERENCES trade_scale(id),
  event_no              INTEGER NOT NULL CHECK (event_no > 0),
  state                 TEXT NOT NULL
                          CHECK (state IN ('verified','revoked','maintenance_pending')),
  evidence_ref          TEXT NOT NULL,
  evidence_hash         BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  seal_or_mark_ref      TEXT,
  effective_at          TEXT NOT NULL,
  valid_until           TEXT,
  UNIQUE (trade_scale_id, event_no)
) STRICT;

CREATE VIEW trade_scale_current_verification AS
SELECT trade_scale_id, id AS verification_event_id, state, valid_until
FROM (
  SELECT v.*,
         ROW_NUMBER() OVER (PARTITION BY trade_scale_id ORDER BY event_no DESC) AS rank_no
    FROM trade_scale_verification v
) WHERE rank_no = 1;

-- Checkout also compares `valid_until` with the trusted business-time input
-- from 0005. An untrusted device wall clock never keeps an expired certificate
-- active; insufficient clock confidence fails embedded pricing closed.

CREATE TRIGGER trade_scale_verification_is_next
BEFORE INSERT ON trade_scale_verification
WHEN NEW.event_no <> COALESCE((
  SELECT MAX(event_no) + 1 FROM trade_scale_verification
   WHERE trade_scale_id = NEW.trade_scale_id), 1)
BEGIN
  SELECT RAISE(ABORT, 'scale verification events must be contiguous');
END;

CREATE TABLE embedded_barcode_rule (
  id               BLOB PRIMARY KEY,
  store_id         BLOB NOT NULL REFERENCES store(id),
  trade_scale_id   BLOB NOT NULL REFERENCES trade_scale(id),
  prefix           TEXT NOT NULL,
  item_code_start  INTEGER NOT NULL,
  item_code_len    INTEGER NOT NULL,
  value_start      INTEGER NOT NULL,
  value_len        INTEGER NOT NULL,
  value_kind       TEXT NOT NULL CHECK (value_kind IN ('weight_milli','price_minor')),
  value_scale      INTEGER NOT NULL DEFAULT 1,
  verify_checksum  INTEGER NOT NULL DEFAULT 1,   -- E.40: reject, never guess
  is_active        INTEGER NOT NULL DEFAULT 0,
  updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version          INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TRIGGER embedded_rule_requires_verified_scale_insert
BEFORE INSERT ON embedded_barcode_rule
WHEN NEW.is_active = 1 AND NOT EXISTS (
  SELECT 1 FROM trade_scale s JOIN trade_scale_current_verification v
    ON v.trade_scale_id = s.id AND v.state = 'verified'
   WHERE s.id = NEW.trade_scale_id AND s.store_id = NEW.store_id)
BEGIN
  SELECT RAISE(ABORT, 'embedded pricing requires verified trade-scale evidence');
END;
CREATE TRIGGER embedded_rule_requires_verified_scale_update
BEFORE UPDATE OF store_id, trade_scale_id, is_active ON embedded_barcode_rule
WHEN NEW.is_active = 1 AND NOT EXISTS (
  SELECT 1 FROM trade_scale s JOIN trade_scale_current_verification v
    ON v.trade_scale_id = s.id AND v.state = 'verified'
   WHERE s.id = NEW.trade_scale_id AND s.store_id = NEW.store_id)
BEGIN
  SELECT RAISE(ABORT, 'embedded pricing requires verified trade-scale evidence');
END;

CREATE TRIGGER trade_scale_disable_rules_on_status_loss
AFTER INSERT ON trade_scale_verification
WHEN NEW.state <> 'verified'
BEGIN
  UPDATE embedded_barcode_rule SET is_active = 0
   WHERE trade_scale_id = NEW.trade_scale_id AND is_active = 1;
END;

CREATE TRIGGER trade_scale_no_update
BEFORE UPDATE ON trade_scale BEGIN
  SELECT RAISE(ABORT, 'trade-scale identity is immutable — append verification evidence');
END;
CREATE TRIGGER trade_scale_no_delete
BEFORE DELETE ON trade_scale BEGIN
  SELECT RAISE(ABORT, 'commissioned trade-scale identity cannot be deleted');
END;
CREATE TRIGGER trade_scale_verification_no_update
BEFORE UPDATE ON trade_scale_verification BEGIN
  SELECT RAISE(ABORT, 'scale verification evidence is append-only');
END;
CREATE TRIGGER trade_scale_verification_no_delete
BEFORE DELETE ON trade_scale_verification BEGIN
  SELECT RAISE(ABORT, 'scale verification history cannot be deleted');
END;

-- PLU quick codes + the tile grid for unbarcoded goods (C.1).
CREATE TABLE plu_code (
  code        TEXT PRIMARY KEY,
  product_id  BLOB NOT NULL REFERENCES product(id),
  deleted_at  TEXT
) STRICT;

CREATE TABLE tile_grid (
  id          BLOB PRIMARY KEY,
  store_id    BLOB REFERENCES store(id),
  name_ar     TEXT NOT NULL,
  name_en     TEXT,
  sort_order  INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE tile (
  id          BLOB PRIMARY KEY,
  grid_id     BLOB NOT NULL REFERENCES tile_grid(id),
  product_id  BLOB REFERENCES product(id),
  category_id BLOB REFERENCES category(id),
  label_ar    TEXT,
  color       TEXT,
  position    INTEGER NOT NULL
) STRICT;
```

The migration tests the lifecycle it implements: `fts_row_removed_on_tombstone`
proves a soft-deleted product disappears, and `fts_survives_product_update`
includes barcode reassignment and proves the old and new aggregate tokens both
rebuild. A trigger-only test on an empty catalogue is insufficient; the
migration fixture also begins with products and barcodes created before `0007`.

> **FTS5 must be verified, not assumed.** `rusqlite` has no `fts5` feature flag; FTS5 arrives through the bundled SQLite build, and this project uses `bundled-sqlcipher-vendored-openssl`. Microstep 1.2.6 adds a startup assertion (`SELECT * FROM pragma_compile_options WHERE compile_options = 'ENABLE_FTS5'`) that fails loudly at open rather than letting search silently return nothing.

> ⚠️ **OPEN — blocks 1.2.4.** What current JSMO mark/certificate proves a trade scale is verified, when does it expire, and which maintenance events require reverification? Default until answered: `embedded_barcode_rule.is_active` remains `0` and no scale-derived price reaches checkout.
> Owner: 1.2.4. Source that settles it: current JSMO metrology instructions or written confirmation from JSMO for the commissioned scale.

---

## 0008 — shifts and cash  ·  Phase 2, microstep 2.4.1

```sql
-- 0005 already owns the immutable opening fact, minimal close event and one-open
-- projection. Phase 2 extends the close FACT with blind-count reconciliation;
-- it does not add mutable close columns back onto `shift`.
ALTER TABLE shift_close_event ADD COLUMN counted_minor INTEGER;
ALTER TABLE shift_close_event ADD COLUMN expected_minor INTEGER;
ALTER TABLE shift_close_event ADD COLUMN over_short_minor INTEGER;
ALTER TABLE shift_close_event ADD COLUMN close_kind TEXT NOT NULL DEFAULT 'normal'
  CHECK (close_kind IN ('normal','forced_stale'));       -- E.53
ALTER TABLE shift_close_event ADD COLUMN ack_by BLOB REFERENCES app_user(id);

CREATE TRIGGER shift_close_reconciliation_complete
BEFORE INSERT ON shift_close_event
WHEN (NEW.close_kind = 'normal' AND (
       NEW.counted_minor IS NULL OR NEW.expected_minor IS NULL
       OR NEW.over_short_minor IS NULL
       OR NEW.over_short_minor <> NEW.counted_minor - NEW.expected_minor
       OR NEW.counted_minor <> COALESCE((
         SELECT SUM(denomination_minor * count) FROM shift_count_line
          WHERE shift_id = NEW.shift_id AND phase = 'close'), 0)))
  OR (NEW.close_kind = 'forced_stale' AND (
       NEW.counted_minor IS NOT NULL OR NEW.expected_minor IS NOT NULL
       OR NEW.over_short_minor IS NOT NULL OR NEW.ack_by IS NULL))
  OR EXISTS (
       SELECT 1 FROM shift_count_line l
        WHERE l.shift_id = NEW.shift_id AND l.phase = 'close'
          AND NOT EXISTS (
            SELECT 1 FROM fact_commit_member m
             WHERE m.commit_id = NEW.sync_commit_id
               AND m.entity = 'shift_count_line' AND m.entity_id = l.id))
BEGIN
  SELECT RAISE(ABORT, 'normal close requires a reconciled blind count; forced-stale close requires acknowledgement');
END;

CREATE TABLE cash_location (
  id           BLOB PRIMARY KEY,
  store_id     BLOB NOT NULL REFERENCES store(id),
  register_id  BLOB REFERENCES register(id),
  kind         TEXT NOT NULL CHECK (kind IN ('drawer','safe','bank_in_transit')),
  code         TEXT NOT NULL,
  name         TEXT NOT NULL,
  is_active    INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
  UNIQUE (store_id, id),
  UNIQUE (store_id, code),
  CHECK ((kind = 'drawer' AND register_id IS NOT NULL)
      OR (kind IN ('safe','bank_in_transit') AND register_id IS NULL))
) STRICT;

CREATE TABLE cash_movement (
  id                BLOB PRIMARY KEY,
  store_id          BLOB NOT NULL REFERENCES store(id),
  shift_id          BLOB REFERENCES shift(id),
  from_location_id  BLOB NOT NULL,
  to_location_id    BLOB NOT NULL,
  kind              TEXT NOT NULL
                      CHECK (kind IN ('paid_in','paid_out','drop','bank_deposit','float_add','transfer')),
  amount_minor      INTEGER NOT NULL CHECK (amount_minor > 0),
  reason_code       TEXT NOT NULL,
  note              TEXT,
  actor_id          BLOB NOT NULL REFERENCES app_user(id),
  approver_id       BLOB REFERENCES app_user(id),
  occurred_at       TEXT NOT NULL,
  FOREIGN KEY (store_id, from_location_id) REFERENCES cash_location(store_id, id),
  FOREIGN KEY (store_id, to_location_id) REFERENCES cash_location(store_id, id),
  CHECK (from_location_id <> to_location_id)
) STRICT;
CREATE INDEX idx_cash_movement_shift ON cash_movement(shift_id);
CREATE INDEX idx_cash_movement_from ON cash_movement(from_location_id, occurred_at);
CREATE INDEX idx_cash_movement_to ON cash_movement(to_location_id, occurred_at);

CREATE TRIGGER cash_location_register_belongs_to_store_insert
BEFORE INSERT ON cash_location
WHEN NEW.register_id IS NOT NULL AND NOT EXISTS (
  SELECT 1 FROM register r WHERE r.id = NEW.register_id AND r.store_id = NEW.store_id)
BEGIN
  SELECT RAISE(ABORT, 'a drawer register must belong to its cash-location store');
END;
CREATE TRIGGER cash_location_register_belongs_to_store_update
BEFORE UPDATE OF store_id, register_id ON cash_location
WHEN NEW.register_id IS NOT NULL AND NOT EXISTS (
  SELECT 1 FROM register r WHERE r.id = NEW.register_id AND r.store_id = NEW.store_id)
BEGIN
  SELECT RAISE(ABORT, 'a drawer register must belong to its cash-location store');
END;

CREATE TRIGGER cash_location_identity_frozen
BEFORE UPDATE OF store_id, register_id, kind, code ON cash_location
WHEN NEW.store_id IS NOT OLD.store_id
  OR NEW.register_id IS NOT OLD.register_id
  OR NEW.kind IS NOT OLD.kind
  OR NEW.code IS NOT OLD.code
BEGIN
  SELECT RAISE(ABORT, 'cash-location identity is immutable — deactivate and create a new location');
END;
CREATE TRIGGER cash_movement_shift_belongs_to_store_insert
BEFORE INSERT ON cash_movement
WHEN NEW.shift_id IS NOT NULL AND (
  NOT EXISTS (
    SELECT 1 FROM shift s WHERE s.id = NEW.shift_id AND s.store_id = NEW.store_id)
  OR NOT EXISTS (
    SELECT 1 FROM shift s JOIN cash_location l
      ON l.id IN (NEW.from_location_id, NEW.to_location_id)
     AND l.kind = 'drawer' AND l.register_id = s.register_id
     WHERE s.id = NEW.shift_id)
  OR EXISTS (
    SELECT 1 FROM shift s JOIN cash_location l
      ON l.id IN (NEW.from_location_id, NEW.to_location_id)
     AND l.kind = 'drawer' AND l.register_id <> s.register_id
     WHERE s.id = NEW.shift_id))
BEGIN
  SELECT RAISE(ABORT, 'a shift cash movement must touch only its register drawer');
END;

CREATE TABLE shift_count_line (          -- the denomination grid (D screen 8/9)
  id                 BLOB PRIMARY KEY,
  shift_id           BLOB NOT NULL REFERENCES shift(id),
  phase              TEXT NOT NULL CHECK (phase IN ('open','close')),
  denomination_minor INTEGER NOT NULL CHECK (denomination_minor > 0),
  count              INTEGER NOT NULL CHECK (count >= 0),
  UNIQUE (shift_id, phase, denomination_minor)
) STRICT;

CREATE TRIGGER shift_count_line_has_ready_commit
BEFORE INSERT ON shift_count_line
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'shift_count_line' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'shift count line requires its complete delivery envelope'); END;

-- Safe and bank-in-transit reconciliation uses the same blind-count principle
-- as the drawer. These rows are inserted only when the count is submitted, so
-- evidence never changes from "draft" to a more convenient number afterwards.
CREATE TABLE cash_count (
  id             BLOB PRIMARY KEY,
  location_id    BLOB NOT NULL REFERENCES cash_location(id),
  shift_id       BLOB REFERENCES shift(id),
  purpose        TEXT NOT NULL CHECK (purpose IN ('opening','closing','reconciliation')),
  total_minor    INTEGER NOT NULL,
  denomination_payload TEXT NOT NULL CHECK (json_valid(denomination_payload)),
                                      -- canonical array of integer denomination/count pairs
  hash_algorithm TEXT NOT NULL CHECK (hash_algorithm IN ('blake3','sha256')),
  denomination_hash BLOB NOT NULL CHECK (length(denomination_hash) = 32),
  counted_by     BLOB NOT NULL REFERENCES app_user(id),
  counted_at     TEXT NOT NULL
) STRICT;

CREATE TRIGGER cash_count_payload_equals_total
BEFORE INSERT ON cash_count
WHEN EXISTS (
       SELECT 1 FROM json_each(NEW.denomination_payload)
        WHERE json_type(value, '$.denomination_minor') <> 'integer'
           OR json_extract(value, '$.denomination_minor') <= 0
           OR json_type(value, '$.count') <> 'integer'
           OR json_extract(value, '$.count') < 0)
  OR NEW.total_minor <> COALESCE((
       SELECT SUM(json_extract(value, '$.denomination_minor')
                * json_extract(value, '$.count'))
         FROM json_each(NEW.denomination_payload)), 0)
BEGIN
  SELECT RAISE(ABORT, 'cash-count total must equal its sealed denomination payload');
END;

-- The Z report is an immutable stored DOCUMENT: reprintable, synced,
-- sequentially numbered per register (C.6).
CREATE TABLE z_report (
  id           BLOB PRIMARY KEY,
  shift_id     BLOB NOT NULL UNIQUE REFERENCES shift(id),
  register_id  BLOB NOT NULL REFERENCES register(id),
  z_number     INTEGER NOT NULL,
  payload      TEXT NOT NULL,            -- the full ZReport model, frozen
  generated_at TEXT NOT NULL,
  generated_by BLOB NOT NULL REFERENCES app_user(id),
  UNIQUE (register_id, z_number)
) STRICT;

CREATE TRIGGER z_report_matches_closed_shift
BEFORE INSERT ON z_report
WHEN NOT EXISTS (
  SELECT 1 FROM shift sh JOIN shift_close_event e ON e.shift_id = sh.id
   WHERE sh.id = NEW.shift_id AND sh.register_id = NEW.register_id)
BEGIN
  SELECT RAISE(ABORT, 'a Z report belongs to one closed shift on the same register');
END;

CREATE TABLE drawer_event (              -- no-sale opens are the classic theft tell (E.35)
  id           BLOB PRIMARY KEY,
  register_id  BLOB NOT NULL REFERENCES register(id),
  shift_id     BLOB REFERENCES shift(id),
  actor_id     BLOB NOT NULL REFERENCES app_user(id),
  approver_id  BLOB REFERENCES app_user(id),
  cause        TEXT NOT NULL CHECK (cause IN ('sale','refund','no_sale','cash_movement','shift_open','shift_close')),
  source_kind  TEXT NOT NULL DEFAULT 'software_command'
                 CHECK (source_kind IN ('software_command','sensor_observation')),
  sale_id      BLOB REFERENCES sale(id),
  reason       TEXT,
  occurred_at  TEXT NOT NULL
) STRICT;
CREATE UNIQUE INDEX idx_drawer_event_sale_command
  ON drawer_event(sale_id)
  WHERE sale_id IS NOT NULL AND source_kind = 'software_command'
    AND cause IN ('sale','refund');

CREATE TRIGGER cash_movement_has_ready_commit
BEFORE INSERT ON cash_movement
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'cash_movement' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'cash movement requires its complete delivery envelope'); END;
CREATE TRIGGER cash_count_has_ready_commit
BEFORE INSERT ON cash_count
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'cash_count' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'submitted cash count requires its complete delivery envelope'); END;
CREATE TRIGGER z_report_has_ready_commit
BEFORE INSERT ON z_report
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'z_report' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'Z report requires its complete delivery envelope'); END;
CREATE TRIGGER drawer_event_has_ready_commit
BEFORE INSERT ON drawer_event
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'drawer_event' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'drawer evidence requires its complete delivery envelope'); END;

-- A sale/refund drawer command is part of the same financial commit as the
-- tender that caused it. Recording an unrelated sale id would make the no-sale
-- report hide a drawer opening, which defeats the control precisely when cash
-- is missing. Sensor observations may follow asynchronously, but the software
-- command that opens the drawer is durable before the sale completes.
CREATE TRIGGER drawer_event_sale_context
BEFORE INSERT ON drawer_event
WHEN (NEW.cause IN ('sale','refund') AND NOT EXISTS (
       SELECT 1 FROM sale s
        WHERE s.id = NEW.sale_id
          AND s.register_id = NEW.register_id
          AND s.shift_id = NEW.shift_id
          AND ((NEW.cause = 'sale' AND s.doc_type = 'sale')
            OR (NEW.cause = 'refund' AND s.doc_type = 'refund'))))
  OR (NEW.cause NOT IN ('sale','refund') AND NEW.sale_id IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'drawer event cause, sale, register and shift must agree');
END;

CREATE TRIGGER completed_sale_requires_drawer_event_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND (
  (EXISTS (
     SELECT 1 FROM sale_tender t JOIN tender_type tt ON tt.code = t.method
      WHERE t.sale_id = NEW.id AND tt.opens_drawer = 1)
   AND NOT EXISTS (
     SELECT 1 FROM drawer_event e JOIN fact_commit_member m
       ON m.entity = 'drawer_event' AND m.entity_id = e.id
      AND m.commit_id = NEW.sync_commit_id
      WHERE e.sale_id = NEW.id AND e.register_id = NEW.register_id
        AND e.shift_id = NEW.shift_id AND e.source_kind = 'software_command'
        AND e.cause = CASE WHEN NEW.doc_type = 'refund' THEN 'refund' ELSE 'sale' END))
  OR (NOT EXISTS (
        SELECT 1 FROM sale_tender t JOIN tender_type tt ON tt.code = t.method
         WHERE t.sale_id = NEW.id AND tt.opens_drawer = 1)
      AND EXISTS (
        SELECT 1 FROM drawer_event e
         WHERE e.sale_id = NEW.id AND e.source_kind = 'software_command'
           AND e.cause IN ('sale','refund'))))
BEGIN
  SELECT RAISE(ABORT, 'drawer-opening tenders require exactly attributable command evidence in the sale commit');
END;

CREATE TRIGGER completed_sale_requires_drawer_event_update
BEFORE UPDATE OF status, sync_commit_id, register_id, shift_id, doc_type ON sale
WHEN NEW.status = 'completed' AND (
  (EXISTS (
     SELECT 1 FROM sale_tender t JOIN tender_type tt ON tt.code = t.method
      WHERE t.sale_id = NEW.id AND tt.opens_drawer = 1)
   AND NOT EXISTS (
     SELECT 1 FROM drawer_event e JOIN fact_commit_member m
       ON m.entity = 'drawer_event' AND m.entity_id = e.id
      AND m.commit_id = NEW.sync_commit_id
      WHERE e.sale_id = NEW.id AND e.register_id = NEW.register_id
        AND e.shift_id = NEW.shift_id AND e.source_kind = 'software_command'
        AND e.cause = CASE WHEN NEW.doc_type = 'refund' THEN 'refund' ELSE 'sale' END))
  OR (NOT EXISTS (
        SELECT 1 FROM sale_tender t JOIN tender_type tt ON tt.code = t.method
         WHERE t.sale_id = NEW.id AND tt.opens_drawer = 1)
      AND EXISTS (
        SELECT 1 FROM drawer_event e
         WHERE e.sale_id = NEW.id AND e.source_kind = 'software_command'
           AND e.cause IN ('sale','refund'))))
BEGIN
  SELECT RAISE(ABORT, 'drawer-opening tenders require exactly attributable command evidence in the sale commit');
END;

-- ── The cash trail is append-only ──────────────────────────────────────────
--
-- Every fraud tell the X/Z design produces — over/short, no-sale opens, the
-- movement history behind expected cash — is only evidence if the rows cannot
-- be tidied up afterwards by the person they incriminate. A Z report is the
-- immutable end-of-day summary by definition; re-running one is a new row.

CREATE TRIGGER cash_movement_no_update
BEFORE UPDATE ON cash_movement
BEGIN
  SELECT RAISE(ABORT, 'I-4: cash_movement is append-only — post a correcting movement');
END;

CREATE TRIGGER cash_movement_no_delete
BEFORE DELETE ON cash_movement
BEGIN
  SELECT RAISE(ABORT, 'I-4: cash_movement is append-only — post a correcting movement');
END;

CREATE TRIGGER cash_count_no_update
BEFORE UPDATE ON cash_count BEGIN
  SELECT RAISE(ABORT, 'a submitted cash count is immutable');
END;
CREATE TRIGGER cash_count_no_delete
BEFORE DELETE ON cash_count BEGIN
  SELECT RAISE(ABORT, 'a submitted cash count cannot be deleted');
END;
CREATE TRIGGER shift_count_line_no_insert_after_close
BEFORE INSERT ON shift_count_line
WHEN EXISTS (SELECT 1 FROM shift_close_event WHERE shift_id = NEW.shift_id)
BEGIN
  SELECT RAISE(ABORT, 'a closed shift cannot gain count lines');
END;
CREATE TRIGGER shift_count_line_no_update_after_close
BEFORE UPDATE ON shift_count_line
WHEN EXISTS (SELECT 1 FROM shift_close_event WHERE shift_id = OLD.shift_id)
  OR EXISTS (SELECT 1 FROM shift_close_event WHERE shift_id = NEW.shift_id)
BEGIN
  SELECT RAISE(ABORT, 'a closed shift count is immutable');
END;
CREATE TRIGGER shift_count_line_no_delete_after_close
BEFORE DELETE ON shift_count_line
WHEN EXISTS (SELECT 1 FROM shift_close_event WHERE shift_id = OLD.shift_id)
BEGIN
  SELECT RAISE(ABORT, 'a closed shift count cannot be deleted');
END;

CREATE TRIGGER z_report_no_update
BEFORE UPDATE ON z_report
BEGIN
  SELECT RAISE(ABORT, 'I-4: a Z report is immutable once taken');
END;

CREATE TRIGGER z_report_no_delete
BEFORE DELETE ON z_report
BEGIN
  SELECT RAISE(ABORT, 'I-4: a Z report cannot be deleted');
END;

CREATE TRIGGER drawer_event_no_update
BEFORE UPDATE ON drawer_event
BEGIN
  SELECT RAISE(ABORT, 'I-4: drawer_event is append-only — the no-sale trail is evidence');
END;

CREATE TRIGGER drawer_event_no_delete
BEFORE DELETE ON drawer_event
BEGIN
  SELECT RAISE(ABORT, 'I-4: drawer_event is append-only — the no-sale trail is evidence');
END;

```

---

## 0009 — refunds and returns  ·  Phase 2, microstep 2.3.1

```sql
-- Post-completion "void" DOES NOT EXIST. It is a same-day full refund document
-- referencing the original (master plan C.5). `ref_sale_id` already exists on
-- `sale`; these tables carry the immutable return-specific facts. The snapshot
-- is deliberate: a refund six months later must use the buyer, line, price and
-- tax facts on the original document, not today's customer or catalogue rows.
CREATE TABLE credit_note_context (
  refund_sale_id          BLOB PRIMARY KEY REFERENCES sale(id),
  original_sale_id        BLOB NOT NULL REFERENCES sale(id),
  original_document_id    TEXT NOT NULL,
  original_fiscal_uuid    TEXT,
  original_business_date  TEXT NOT NULL,
  original_total_minor    INTEGER NOT NULL,
  buyer_id_scheme         TEXT,
  buyer_id_value          TEXT,
  buyer_name              TEXT,
  created_at              TEXT NOT NULL,
  CHECK ((buyer_id_scheme IS NULL) = (buyer_id_value IS NULL))
) STRICT;
CREATE INDEX idx_credit_note_original ON credit_note_context(original_sale_id);

CREATE TRIGGER credit_note_context_matches_original
BEFORE INSERT ON credit_note_context
WHEN NOT EXISTS (
  SELECT 1 FROM sale refund JOIN sale original ON original.id = NEW.original_sale_id
   WHERE refund.id = NEW.refund_sale_id AND refund.doc_type = 'refund'
     AND refund.ref_sale_id = original.id
     AND NEW.original_document_id = original.receipt_number
     AND NEW.original_business_date = original.business_date
     AND NEW.original_total_minor = original.total_minor
     AND NEW.buyer_id_scheme IS original.buyer_id_scheme
     AND NEW.buyer_id_value IS original.buyer_id_value
     AND NEW.buyer_name IS original.buyer_name)
BEGIN
  SELECT RAISE(ABORT, 'credit-note header facts must match the immutable original sale');
END;

CREATE TABLE refund_line_link (
  id                        BLOB PRIMARY KEY,
  refund_line_id            BLOB NOT NULL UNIQUE REFERENCES sale_line(id),
  original_line_id          BLOB NOT NULL REFERENCES sale_line(id),
  qty_milli                 INTEGER NOT NULL CHECK (qty_milli > 0),
  original_line_no          INTEGER NOT NULL,
  original_name_snapshot    TEXT NOT NULL,
  original_unit_price_minor INTEGER NOT NULL,
  original_net_minor        INTEGER NOT NULL,
  original_tax_minor        INTEGER NOT NULL,
  original_total_minor      INTEGER NOT NULL,
  original_tax_snapshot     TEXT NOT NULL,
  remaining_before_milli    INTEGER NOT NULL CHECK (remaining_before_milli >= 0),
  remaining_after_milli     INTEGER NOT NULL CHECK (remaining_after_milli >= 0),
  refund_value_minor        INTEGER NOT NULL CHECK (refund_value_minor >= 0),
  remaining_value_before_minor INTEGER NOT NULL CHECK (remaining_value_before_minor >= 0),
  remaining_value_after_minor  INTEGER NOT NULL CHECK (remaining_value_after_minor >= 0),
  restock                   TEXT NOT NULL CHECK (restock IN ('to_stock','damaged','none')),
  reason_code               TEXT NOT NULL
                              CHECK (reason_code IN ('change_of_mind','defective','damaged','wrong_item')),
  is_window_bypassed        INTEGER NOT NULL DEFAULT 0 CHECK (is_window_bypassed IN (0,1)),
  CHECK (remaining_after_milli = remaining_before_milli - qty_milli),
  CHECK (remaining_value_after_minor = remaining_value_before_minor - refund_value_minor),
  CHECK (reason_code <> 'defective' OR is_window_bypassed = 1),
  CHECK (is_window_bypassed = 0 OR reason_code = 'defective')
) STRICT;
CREATE INDEX idx_refund_link_original ON refund_line_link(original_line_id);

ALTER TABLE stock_ledger ADD COLUMN source_refund_link_id BLOB
  REFERENCES refund_line_link(id);
CREATE UNIQUE INDEX idx_stock_refund_link
  ON stock_ledger(source_refund_link_id)
  WHERE kind IN ('refund_restock','refund_damage');

-- Repair/replacement is not disguised as a financial refund. It is a separate
-- remedy fact, and is available for a defect only after the consumer's written
-- choice is retained. A requested refund remains `refund_line_link` and always
-- bypasses the merchant goodwill window.
CREATE TABLE defect_resolution_event (
  id                       BLOB PRIMARY KEY,
  original_line_id         BLOB NOT NULL REFERENCES sale_line(id),
  resolution               TEXT NOT NULL CHECK (resolution IN ('repair','replacement')),
  consumer_consent_ref     TEXT NOT NULL,
  evidence_hash_algorithm  TEXT NOT NULL CHECK (evidence_hash_algorithm IN ('blake3','sha256')),
  evidence_hash            BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  actor_id                 BLOB NOT NULL REFERENCES app_user(id),
  occurred_at              TEXT NOT NULL
) STRICT;

CREATE TRIGGER refund_line_link_conserves_original
BEFORE INSERT ON refund_line_link
WHEN NOT EXISTS (
       SELECT 1
         FROM sale_line rl
         JOIN sale rs ON rs.id = rl.sale_id AND rs.doc_type = 'refund'
         JOIN credit_note_context c ON c.refund_sale_id = rs.id
         JOIN sale_line ol ON ol.id = NEW.original_line_id
        WHERE rl.id = NEW.refund_line_id
          AND ol.sale_id = c.original_sale_id
          AND NEW.original_line_no = ol.line_no
          AND NEW.original_name_snapshot = ol.name_snapshot
          AND NEW.original_unit_price_minor = ol.unit_price_minor
          AND NEW.original_net_minor = ol.net_minor
          AND NEW.original_tax_minor = ol.tax_minor
          AND NEW.original_total_minor = ol.total_minor
          AND NEW.original_tax_snapshot = (
            SELECT json_group_array(json(component_json)) FROM (
              SELECT json_object(
                       'component_code', t.component_code,
                       'treatment', t.treatment,
                       'calculation_kind', t.calculation_kind,
                       'rate_ppm', t.rate_ppm,
                       'fixed_amount_minor', t.fixed_amount_minor,
                       'fixed_currency', t.fixed_currency,
                       'fixed_basis_qty_milli', t.fixed_basis_qty_milli,
                       'calculation_order', t.calculation_order,
                       'base_kind', t.base_kind,
                       'taxable_base_minor', t.taxable_base_minor,
                       'taxable_qty_milli', t.taxable_qty_milli,
                       'tax_minor', t.tax_minor) AS component_json
                FROM sale_line_tax t WHERE t.sale_line_id = ol.id
               ORDER BY t.calculation_order, t.component_code, t.id)))
  OR NEW.remaining_before_milli <> (
       SELECT ol.qty_milli - COALESCE(SUM(prior.qty_milli), 0)
         FROM sale_line ol LEFT JOIN refund_line_link prior
           ON prior.original_line_id = ol.id
        WHERE ol.id = NEW.original_line_id)
  OR NEW.remaining_value_before_minor <> (
       SELECT ol.total_minor - COALESCE(SUM(prior.refund_value_minor), 0)
         FROM sale_line ol LEFT JOIN refund_line_link prior
           ON prior.original_line_id = ol.id
        WHERE ol.id = NEW.original_line_id)
  OR NEW.qty_milli > NEW.remaining_before_milli
  OR NEW.refund_value_minor > NEW.remaining_value_before_minor
BEGIN
  SELECT RAISE(ABORT, 'a refund link must conserve the original line quantity and value');
END;

CREATE TRIGGER refund_stock_event_matches_link
BEFORE INSERT ON stock_ledger
-- `IS` is deliberate: an unknown original cost is NULL and a valid refund must
-- preserve that unknown basis rather than fail because SQL `NULL = NULL` is not true.
WHEN NEW.kind IN ('refund_restock','refund_damage') AND NOT EXISTS (
  SELECT 1 FROM refund_line_link link
  JOIN sale_line refund_line ON refund_line.id = link.refund_line_id
  JOIN sale refund ON refund.id = refund_line.sale_id
  JOIN sale_line original_line ON original_line.id = link.original_line_id
  JOIN stock_ledger original_stock
    ON original_stock.kind = 'sale' AND original_stock.source_sale_line_id = original_line.id
   WHERE link.id = NEW.source_refund_link_id
     AND NEW.ref_kind = 'sale' AND NEW.ref_id = refund.id
     AND NEW.product_id = refund_line.product_id
     AND NEW.store_id = refund.store_id
     AND NEW.unit_cost_minor IS original_stock.unit_cost_minor
     AND NEW.is_cost_estimated = original_stock.is_cost_estimated
     AND NEW.is_weight_derived = original_stock.is_weight_derived
     AND ((link.restock = 'to_stock' AND NEW.kind = 'refund_restock'
            AND NEW.qty_delta_milli = link.qty_milli)
       OR (link.restock = 'damaged' AND NEW.kind = 'refund_damage'
            AND NEW.qty_delta_milli = 0)))
BEGIN
  SELECT RAISE(ABORT, 'refund stock fact must match the return decision and original captured cost');
END;

CREATE TRIGGER completed_refund_requires_linked_facts_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND NEW.doc_type = 'refund' AND (
  NOT EXISTS (
    SELECT 1 FROM credit_note_context c JOIN fact_commit_member m
      ON m.entity = 'credit_note_context' AND m.entity_id = c.refund_sale_id
     AND m.commit_id = NEW.sync_commit_id
     WHERE c.refund_sale_id = NEW.id)
  OR EXISTS (
    SELECT 1 FROM sale_line l WHERE l.sale_id = NEW.id AND NOT EXISTS (
      SELECT 1 FROM refund_line_link link JOIN fact_commit_member m
        ON m.entity = 'refund_line_link' AND m.entity_id = link.id
       AND m.commit_id = NEW.sync_commit_id
       WHERE link.refund_line_id = l.id))
  OR EXISTS (
    SELECT 1 FROM refund_line_link link JOIN sale_line l ON l.id = link.refund_line_id
     WHERE l.sale_id = NEW.id AND (
       (link.restock IN ('to_stock','damaged') AND NOT EXISTS (
         SELECT 1 FROM stock_ledger e JOIN fact_commit_member m
           ON m.entity = 'stock_ledger' AND m.entity_id = e.id
          AND m.commit_id = NEW.sync_commit_id
          WHERE e.source_refund_link_id = link.id
            AND e.kind = CASE link.restock WHEN 'to_stock' THEN 'refund_restock' ELSE 'refund_damage' END))
       OR (link.restock = 'none' AND EXISTS (
         SELECT 1 FROM stock_ledger e WHERE e.source_refund_link_id = link.id)))))
BEGIN
  SELECT RAISE(ABORT, 'refund completion requires its original trail and exact stock decisions in one commit');
END;

CREATE TRIGGER completed_refund_requires_linked_facts_update
BEFORE UPDATE OF status, sync_commit_id ON sale
WHEN NEW.status = 'completed' AND NEW.doc_type = 'refund' AND (
  NOT EXISTS (
    SELECT 1 FROM credit_note_context c JOIN fact_commit_member m
      ON m.entity = 'credit_note_context' AND m.entity_id = c.refund_sale_id
     AND m.commit_id = NEW.sync_commit_id
     WHERE c.refund_sale_id = NEW.id)
  OR EXISTS (
    SELECT 1 FROM sale_line l WHERE l.sale_id = NEW.id AND NOT EXISTS (
      SELECT 1 FROM refund_line_link link JOIN fact_commit_member m
        ON m.entity = 'refund_line_link' AND m.entity_id = link.id
       AND m.commit_id = NEW.sync_commit_id
       WHERE link.refund_line_id = l.id))
  OR EXISTS (
    SELECT 1 FROM refund_line_link link JOIN sale_line l ON l.id = link.refund_line_id
     WHERE l.sale_id = NEW.id AND (
       (link.restock IN ('to_stock','damaged') AND NOT EXISTS (
         SELECT 1 FROM stock_ledger e JOIN fact_commit_member m
           ON m.entity = 'stock_ledger' AND m.entity_id = e.id
          AND m.commit_id = NEW.sync_commit_id
          WHERE e.source_refund_link_id = link.id
            AND e.kind = CASE link.restock WHEN 'to_stock' THEN 'refund_restock' ELSE 'refund_damage' END))
       OR (link.restock = 'none' AND EXISTS (
         SELECT 1 FROM stock_ledger e WHERE e.source_refund_link_id = link.id)))))
BEGIN
  SELECT RAISE(ABORT, 'refund completion requires its original trail and exact stock decisions in one commit');
END;

-- Denormalised guard for the invariant that must never break:
-- cumulative refunds per line never exceed sold qty (E.16).
-- Maintained in the same transaction as the refund; rebuildable from the links.
CREATE TABLE refunded_qty_cache (
  original_line_id BLOB PRIMARY KEY REFERENCES sale_line(id),
  refunded_milli   INTEGER NOT NULL DEFAULT 0,
  refunded_value_minor INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TRIGGER refund_line_link_project_cache
AFTER INSERT ON refund_line_link
BEGIN
  INSERT INTO refunded_qty_cache
    (original_line_id, refunded_milli, refunded_value_minor)
  VALUES
    (NEW.original_line_id, NEW.qty_milli, NEW.refund_value_minor)
  ON CONFLICT(original_line_id) DO UPDATE SET
    refunded_milli = refunded_milli + NEW.qty_milli,
    refunded_value_minor = refunded_value_minor + NEW.refund_value_minor;
END;

CREATE TRIGGER refunded_qty_cache_matches_links_insert
BEFORE INSERT ON refunded_qty_cache
WHEN NEW.refunded_milli <> COALESCE((
       SELECT SUM(qty_milli) FROM refund_line_link
        WHERE original_line_id = NEW.original_line_id), 0)
  OR NEW.refunded_value_minor <> COALESCE((
       SELECT SUM(refund_value_minor) FROM refund_line_link
        WHERE original_line_id = NEW.original_line_id), 0)
BEGIN
  SELECT RAISE(ABORT, 'refund cache must equal immutable refund links');
END;
CREATE TRIGGER refunded_qty_cache_matches_links_update
BEFORE UPDATE ON refunded_qty_cache
WHEN NEW.refunded_milli <> COALESCE((
       SELECT SUM(qty_milli) FROM refund_line_link
        WHERE original_line_id = NEW.original_line_id), 0)
  OR NEW.refunded_value_minor <> COALESCE((
       SELECT SUM(refund_value_minor) FROM refund_line_link
        WHERE original_line_id = NEW.original_line_id), 0)
BEGIN
  SELECT RAISE(ABORT, 'refund cache must equal immutable refund links');
END;

CREATE TABLE refund_policy (
  store_id                     BLOB PRIMARY KEY REFERENCES store(id),
  window_days                  INTEGER NOT NULL DEFAULT 14, -- merchant goodwill only; never limits defects
  allow_receiptless            INTEGER NOT NULL DEFAULT 0,
  receiptless_max_minor        INTEGER,
  receiptless_store_credit_only INTEGER NOT NULL DEFAULT 1,
  allow_cash_for_card          INTEGER NOT NULL DEFAULT 0,   -- laundering vector (C.5)
  cash_for_card_max_minor      INTEGER,
  escalate_above_minor         INTEGER NOT NULL DEFAULT 20000,
  ban_self_approval            INTEGER NOT NULL DEFAULT 1,   -- E.52
  requalify_policy             TEXT NOT NULL DEFAULT 'deal_break'
                                 CHECK (requalify_policy IN ('deal_break','proportional_share')),
  updated_at                   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version                      INTEGER NOT NULL DEFAULT 0
) STRICT;

-- Exchange = return + new sale, settling only the difference. Under the hood it
-- is exactly those two documents, linked (C.5). The chain matters because
-- refundable qty follows it (E.30).
CREATE TABLE document_link (
  id         BLOB PRIMARY KEY,
  from_sale  BLOB NOT NULL REFERENCES sale(id),
  to_sale    BLOB NOT NULL REFERENCES sale(id),
  link_kind  TEXT NOT NULL CHECK (link_kind IN ('exchange','correction','reprint_of')),
  created_at TEXT NOT NULL
) STRICT;

-- Store credit is needed by Phase 2 receiptless-return policy, so its minimum
-- ledger belongs here rather than waiting for the customer migration. Identity
-- may be anonymous; 0011 adds the optional customer link after `customer` exists.
CREATE TABLE stored_value_instrument (
  id             BLOB PRIMARY KEY,
  org_id         BLOB NOT NULL REFERENCES org(id),
  code_hash      BLOB NOT NULL,
  currency       TEXT NOT NULL,
  status         TEXT NOT NULL DEFAULT 'active'
                   CHECK (status IN ('active','suspended','closed')),
  issued_at      TEXT NOT NULL,
  UNIQUE (org_id, code_hash)
) STRICT;

CREATE TABLE stored_value_policy_version (
  id                   BLOB PRIMARY KEY,
  org_id               BLOB NOT NULL REFERENCES org(id),
  policy_version       TEXT NOT NULL,
  approval_source_ref  TEXT NOT NULL,
  source_hash_algorithm TEXT NOT NULL CHECK (source_hash_algorithm IN ('blake3','sha256')),
  source_hash          BLOB NOT NULL CHECK (length(source_hash) = 32),
  approved_at          TEXT NOT NULL,
  created_at           TEXT NOT NULL,
  UNIQUE (org_id, policy_version),
  UNIQUE (org_id, id)
) STRICT;

-- Selection is a mutable projection; an approved version is not. Disabling
-- stored value clears the selection without rewriting the policy named by old
-- ledger events.
CREATE TABLE stored_value_policy_current (
  org_id               BLOB PRIMARY KEY REFERENCES org(id),
  policy_id            BLOB,
  is_enabled           INTEGER NOT NULL DEFAULT 0 CHECK (is_enabled IN (0,1)),
  updated_at           TEXT NOT NULL,
  FOREIGN KEY (org_id, policy_id)
    REFERENCES stored_value_policy_version(org_id, id),
  CHECK ((is_enabled = 1) = (policy_id IS NOT NULL))
) STRICT;

CREATE TABLE stored_value_ledger (
  id                   BLOB PRIMARY KEY,
  instrument_id        BLOB NOT NULL REFERENCES stored_value_instrument(id),
  register_id          BLOB NOT NULL REFERENCES register(id),
  event_seq            INTEGER NOT NULL CHECK (event_seq > 0),
  amount_delta_minor   INTEGER NOT NULL,
  kind                 TEXT NOT NULL CHECK (kind IN ('issue','top_up','redeem','adjust','expire')),
  ref_kind             TEXT NOT NULL,
  ref_id               BLOB NOT NULL,
  actor_id             BLOB NOT NULL REFERENCES app_user(id),
  reason               TEXT,
  tax_policy_id        BLOB NOT NULL REFERENCES stored_value_policy_version(id),
  tax_treatment_code   TEXT NOT NULL,
  occurred_at          TEXT NOT NULL,
  UNIQUE (register_id, event_seq),
  CHECK ((kind IN ('issue','top_up') AND amount_delta_minor > 0)
      OR (kind IN ('redeem','expire') AND amount_delta_minor < 0)
      OR (kind = 'adjust' AND amount_delta_minor <> 0))
) STRICT;
CREATE TRIGGER stored_value_ledger_has_ready_commit
BEFORE INSERT ON stored_value_ledger
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'stored_value_ledger' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'stored-value fact requires its complete delivery envelope'); END;
CREATE INDEX idx_stored_value_instrument
  ON stored_value_ledger(instrument_id, register_id, event_seq);

CREATE TRIGGER stored_value_requires_approved_policy
BEFORE INSERT ON stored_value_ledger
WHEN NOT EXISTS (
  SELECT 1 FROM stored_value_instrument i
  JOIN stored_value_policy_current current ON current.org_id = i.org_id
  JOIN stored_value_policy_version p
    ON p.id = current.policy_id AND p.org_id = i.org_id
   WHERE i.id = NEW.instrument_id AND current.is_enabled = 1
     AND p.id = NEW.tax_policy_id)
BEGIN
  SELECT RAISE(ABORT, 'stored value remains disabled until its approved tax policy is selected');
END;

CREATE TRIGGER stored_value_policy_version_no_update
BEFORE UPDATE ON stored_value_policy_version BEGIN
  SELECT RAISE(ABORT, 'an approved stored-value policy version is immutable');
END;
CREATE TRIGGER stored_value_policy_version_no_delete
BEFORE DELETE ON stored_value_policy_version BEGIN
  SELECT RAISE(ABORT, 'stored-value policy evidence referenced by a ledger cannot be deleted');
END;

CREATE TABLE stored_value_balance_cache (
  instrument_id       BLOB PRIMARY KEY REFERENCES stored_value_instrument(id),
  balance_minor       INTEGER NOT NULL DEFAULT 0,
  last_event_id       BLOB REFERENCES stored_value_ledger(id),
  event_count         INTEGER NOT NULL DEFAULT 0 CHECK (event_count >= 0),
  updated_at          TEXT NOT NULL
) STRICT;

CREATE TRIGGER stored_value_project_balance
AFTER INSERT ON stored_value_ledger
BEGIN
  INSERT INTO stored_value_balance_cache
    (instrument_id, balance_minor, last_event_id, event_count, updated_at)
  VALUES
    (NEW.instrument_id, NEW.amount_delta_minor, NEW.id, 1, NEW.occurred_at)
  ON CONFLICT(instrument_id) DO UPDATE SET
    balance_minor = balance_minor + NEW.amount_delta_minor,
    last_event_id = NEW.id,
    event_count = event_count + 1,
    updated_at = NEW.occurred_at;
END;

CREATE TRIGGER credit_note_context_no_update
BEFORE UPDATE ON credit_note_context BEGIN
  SELECT RAISE(ABORT, 'original credit-note facts are immutable');
END;
CREATE TRIGGER credit_note_context_no_delete
BEFORE DELETE ON credit_note_context BEGIN
  SELECT RAISE(ABORT, 'original credit-note facts cannot be deleted');
END;
CREATE TRIGGER refund_line_link_no_update
BEFORE UPDATE ON refund_line_link BEGIN
  SELECT RAISE(ABORT, 'the refund trail is append-only');
END;
CREATE TRIGGER refund_line_link_no_delete
BEFORE DELETE ON refund_line_link BEGIN
  SELECT RAISE(ABORT, 'the refund trail cannot be deleted');
END;
CREATE TRIGGER defect_resolution_event_no_update
BEFORE UPDATE ON defect_resolution_event BEGIN
  SELECT RAISE(ABORT, 'consumer-selected defect resolution is immutable evidence');
END;
CREATE TRIGGER defect_resolution_event_no_delete
BEFORE DELETE ON defect_resolution_event BEGIN
  SELECT RAISE(ABORT, 'consumer-selected defect resolution cannot be deleted');
END;
CREATE TRIGGER document_link_no_update
BEFORE UPDATE ON document_link BEGIN
  SELECT RAISE(ABORT, 'document lineage is immutable');
END;
CREATE TRIGGER document_link_no_delete
BEFORE DELETE ON document_link BEGIN
  SELECT RAISE(ABORT, 'document lineage cannot be deleted');
END;
CREATE TRIGGER stored_value_ledger_no_update
BEFORE UPDATE ON stored_value_ledger BEGIN
  SELECT RAISE(ABORT, 'stored value is a ledger — append a correcting event');
END;
CREATE TRIGGER stored_value_ledger_no_delete
BEFORE DELETE ON stored_value_ledger BEGIN
  SELECT RAISE(ABORT, 'stored-value history cannot be deleted');
END;
CREATE TRIGGER stored_value_never_negative
BEFORE INSERT ON stored_value_ledger
WHEN COALESCE((SELECT SUM(amount_delta_minor) FROM stored_value_ledger
                WHERE instrument_id = NEW.instrument_id), 0)
     + NEW.amount_delta_minor < 0
BEGIN
  SELECT RAISE(ABORT, 'stored-value redemption cannot make the ledger balance negative');
END;
```

> ⚠️ **OPEN — blocks 2.3.2.** What tax point and JoFotara document apply to each enabled store-credit or stored-value issue, top-up, redeem, adjustment and expiry model? Default until answered: the tables ship, but stored value remains disabled and no `stored_value_ledger` event may be posted.
> Owner: 2.3.2. Source that settles it: a written ISTD ruling and the merchant's tax advisor for the exact funded-value model.

---

## 0010 — fiscal  ·  Phase 2, microstep 2.7.4

Full pipeline design in [`fiscal-jofotara.md`](fiscal-jofotara.md).

Microstep `2.7.0` is a precondition to creating this migration, freezing fiscal
goldens or accepting any `fiscal_spec_package` row: it obtains the official
guide/XSD/code lists, records their package version and hash, then resolves or
preserves every provisional field below. A reconstruction is not an approvable
package.

> ⚠️ **OPEN — blocks 2.7.0.** Is the authoritative ICV namespace per register, store/income source, or one TIN across stores? Default until answered: allocate from one store-scoped counter keyed as `('store', store_id, 'fiscal_icv')`; Phase 2 uses the single register's in-process allocator, Phase 3 uses a server-issued one-value lease, and no register advances an independent register-scoped ICV counter.
> Owner: 2.7.0. Source that settles it: the official ISTD business rules or a written ISTD E-Invoicing Directorate ruling.

> ⚠️ **OPEN — blocks 2.7.0.** Does ISTD permit asynchronous reporting during an outage, what artifact may be handed to the customer, when is the legal issuance event, what is the submission deadline, and how are backdating and later rejection handled? Default until answered: complete the sale, print only a non-fiscal payment acknowledgement, and issue the fiscal invoice only through the approved clearance path.
> Owner: 2.7.0. Source that settles it: the official ISTD outage procedure or a written ruling from the ISTD E-Invoicing Directorate.

> ⚠️ **OPEN — blocks 2.7.0.** What tolerance, if any, does the current ISTD validator apply to transmitted line and document equations? Default until answered: enforce the half-fil per-line projection check and exact identities over the document's own carried values; do not implement an invoice-level tolerance or claim an ISTD tolerance.
> Owner: 2.7.0. Source that settles it: the pinned official ISTD business rules and Schematron/XSD package, plus credentialed accepted boundary vectors.

```sql
CREATE TABLE fiscal_spec_package (
  id             BLOB PRIMARY KEY,
  package_version TEXT NOT NULL,
  source_uri     TEXT NOT NULL,
  content_hash   BLOB NOT NULL,
  acquired_at    TEXT NOT NULL,
  verified_by    BLOB NOT NULL REFERENCES app_user(id),
  UNIQUE (package_version, content_hash)
) STRICT;

-- Immutable checkout identity is a fact, separate from the mutable local queue
-- projection. The server can therefore reconstruct UUID idempotency and
-- document lineage from register facts even before ICV allocation is possible.
CREATE TABLE fiscal_document (
  id                BLOB PRIMARY KEY,
  sync_commit_id    BLOB NOT NULL REFERENCES sync_commit(id),
  sale_id           BLOB NOT NULL UNIQUE REFERENCES sale(id),
  store_id          BLOB NOT NULL REFERENCES store(id),
  doc_kind          TEXT NOT NULL CHECK (doc_kind IN ('invoice','credit_note')),
  document_id       TEXT NOT NULL,
  profile_id        TEXT NOT NULL,
  issue_date        TEXT,
  fiscal_uuid       TEXT NOT NULL UNIQUE,
  depends_on        BLOB REFERENCES fiscal_document(id),
  created_at        TEXT NOT NULL,
  UNIQUE (store_id, document_id),
  CHECK (issue_date IS NULL OR (length(issue_date) = 10
         AND substr(issue_date, 5, 1) = '-'
         AND substr(issue_date, 8, 1) = '-'))
) STRICT;

CREATE TRIGGER fiscal_document_matches_sale
BEFORE INSERT ON fiscal_document
WHEN NOT EXISTS (
       SELECT 1 FROM sale s
        WHERE s.id = NEW.sale_id AND s.store_id = NEW.store_id
          AND s.receipt_number = NEW.document_id
          AND ((s.doc_type = 'sale' AND NEW.doc_kind = 'invoice')
            OR (s.doc_type = 'refund' AND NEW.doc_kind = 'credit_note')))
  OR NEW.fiscal_uuid = ''
  OR (NEW.doc_kind = 'invoice' AND NEW.depends_on IS NOT NULL)
  OR (NEW.doc_kind = 'credit_note' AND (
       NEW.depends_on IS NULL OR NOT EXISTS (
         SELECT 1 FROM fiscal_document parent JOIN sale refund ON refund.id = NEW.sale_id
          WHERE parent.id = NEW.depends_on
            AND parent.store_id = NEW.store_id
            AND parent.doc_kind = 'invoice'
            AND parent.sale_id = refund.ref_sale_id)))
  OR NOT EXISTS (
       SELECT 1 FROM fact_commit_member m
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'fiscal_document' AND m.entity_id = NEW.id)
BEGIN
  SELECT RAISE(ABORT, 'fiscal document identity must match its sale, original and durable commit');
END;

-- Durable queue. The sale transaction generates `fiscal_uuid` and this row, but
-- deliberately leaves ICV and XML NULL. The first submission worker validates
-- the carried sale values, allocates one store-scoped ICV and freezes the XML in
-- one transaction. A register that cannot reach that allocator keeps ICV NULL;
-- the sale is complete and clearance waits. When `ClockState` is Suspect or
-- Untrusted, `issue_date` is also NULL. Merely reaching the clearance endpoint
-- does not authenticate time: allocation and payload freeze wait for a new
-- authenticated time anchor, and a never-synchronised register stays visibly
-- queued while the sale remains complete. ICV and the eventual date are never
-- regenerated.
CREATE TABLE fiscal_queue (
  id                   BLOB PRIMARY KEY,
  document_fact_id     BLOB NOT NULL UNIQUE REFERENCES fiscal_document(id),
  sale_id              BLOB NOT NULL REFERENCES sale(id),
  store_id             BLOB NOT NULL REFERENCES store(id),
  doc_kind             TEXT NOT NULL CHECK (doc_kind IN ('invoice','credit_note')),
  document_id          TEXT NOT NULL,       -- immutable register-prefixed `cbc:ID`
  profile_id           TEXT NOT NULL,       -- authoritative `cbc:ProfileID`
  issue_date           TEXT,                -- NULL until a permitted authenticated source; then UBL `xs:date`
  invoice_type_name    TEXT,                -- composite from pinned scope/settlement/taxpayer tables
  fiscal_uuid          TEXT NOT NULL UNIQUE,
  icv                  INTEGER CHECK (icv > 0),
  payload_xml          TEXT,                -- built once; never rebuilt after allocation
  payload_hash         TEXT,
  builder_version      TEXT,
  spec_package_id      BLOB REFERENCES fiscal_spec_package(id),
  state                TEXT NOT NULL DEFAULT 'queued'
                         CHECK (state IN (
                           'queued','sending','cleared','build_failed',
                           'rejected','dead','skipped')),
  attempts             INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  next_attempt_at      TEXT,
  lease_owner          TEXT,
  claimed_at           TEXT,
  lease_expires_at     TEXT,
  last_error_code      TEXT,
  last_error           TEXT,
  -- A credit note may not clear before its invoice does (E.26).
  depends_on           BLOB REFERENCES fiscal_queue(id),
  created_at           TEXT NOT NULL,
  updated_at           TEXT NOT NULL,
  CHECK (issue_date IS NULL OR (length(issue_date) = 10
         AND substr(issue_date, 5, 1) = '-'
         AND substr(issue_date, 8, 1) = '-')),
  CHECK ((icv IS NULL) = (payload_xml IS NULL)),
  CHECK ((payload_xml IS NULL) = (payload_hash IS NULL)),
  CHECK ((payload_xml IS NULL) = (invoice_type_name IS NULL)),
  CHECK ((payload_xml IS NULL) = (spec_package_id IS NULL)),
  CHECK (payload_xml IS NULL OR issue_date IS NOT NULL),
  CHECK (state <> 'build_failed' OR payload_xml IS NULL),
  CHECK (state NOT IN ('sending','cleared','rejected','dead')
         OR (issue_date IS NOT NULL AND icv IS NOT NULL AND payload_xml IS NOT NULL)),
  CHECK (state <> 'sending'
         OR (lease_owner IS NOT NULL AND claimed_at IS NOT NULL AND lease_expires_at IS NOT NULL)),
  CHECK (state = 'sending'
         OR (lease_owner IS NULL AND claimed_at IS NULL AND lease_expires_at IS NULL))
) STRICT;
CREATE INDEX idx_fiscal_queue_pending ON fiscal_queue(state, next_attempt_at)
  WHERE state IN ('queued','sending');
CREATE INDEX idx_fiscal_queue_expired_lease ON fiscal_queue(lease_expires_at)
  WHERE state = 'sending';
CREATE UNIQUE INDEX idx_fiscal_queue_sale_kind ON fiscal_queue(sale_id, doc_kind);
CREATE UNIQUE INDEX idx_fiscal_queue_document_id ON fiscal_queue(store_id, document_id);
CREATE UNIQUE INDEX idx_fiscal_queue_store_icv ON fiscal_queue(store_id, icv)
  WHERE icv IS NOT NULL;

-- Allocation/build is another immutable fact. In Phase 2 the local single
-- register allocates in-process from its store row and writes its register id in
-- `allocator_ref`; from Phase 3 a server-issued one-value lease supplies both
-- the ICV and allocator reference. Without the applicable allocator the event
-- is absent. Once appended, its ICV, stamped issue date and exact XML can be
-- replayed but never regenerated. `sync_commit_id` groups this later mutation
-- with its permanent manifest and delivery row.
CREATE TABLE fiscal_payload_event (
  id                    BLOB PRIMARY KEY,
  document_fact_id      BLOB NOT NULL UNIQUE REFERENCES fiscal_document(id),
  sync_commit_id        BLOB NOT NULL REFERENCES sync_commit(id),
  allocation_scope_kind TEXT NOT NULL CHECK (allocation_scope_kind = 'store'),
  allocation_scope_id   BLOB NOT NULL REFERENCES store(id),
  allocator_ref         TEXT NOT NULL,
  icv                   INTEGER NOT NULL CHECK (icv > 0),
  issue_date            TEXT NOT NULL,
  invoice_type_name     TEXT NOT NULL,
  payload_xml           TEXT NOT NULL,
  payload_hash          TEXT NOT NULL,
  builder_version       TEXT NOT NULL,
  spec_package_id       BLOB NOT NULL REFERENCES fiscal_spec_package(id),
  built_at              TEXT NOT NULL,
  UNIQUE (allocation_scope_id, icv),
  CHECK (length(issue_date) = 10
         AND substr(issue_date, 5, 1) = '-'
         AND substr(issue_date, 8, 1) = '-')
) STRICT;

CREATE TRIGGER fiscal_payload_event_matches_document
BEFORE INSERT ON fiscal_payload_event
WHEN NOT EXISTS (
       SELECT 1 FROM fiscal_document d
        WHERE d.id = NEW.document_fact_id
          AND d.store_id = NEW.allocation_scope_id)
  OR NOT EXISTS (
       SELECT 1 FROM fact_commit_member m
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'fiscal_payload_event' AND m.entity_id = NEW.id)
  OR NOT EXISTS (SELECT 1 FROM sync_commit_ready ready WHERE ready.id = NEW.sync_commit_id)
BEGIN
  SELECT RAISE(ABORT, 'fiscal payload allocation must match its store identity and durable commit');
END;

-- `credit_note_context` is created in 0009 so receiptless/non-fiscal returns can
-- exist without the fiscal migration. Once 0010 is installed, a fiscal source
-- must copy its locally generated UUID exactly; a non-fiscal source must not
-- invent one.
CREATE TRIGGER credit_note_original_fiscal_uuid_matches
BEFORE INSERT ON credit_note_context
WHEN (EXISTS (
       SELECT 1 FROM fiscal_queue original
        WHERE original.sale_id = NEW.original_sale_id
          AND original.doc_kind = 'invoice'
          AND NEW.original_fiscal_uuid IS NOT original.fiscal_uuid))
   OR (NOT EXISTS (
       SELECT 1 FROM fiscal_queue original
        WHERE original.sale_id = NEW.original_sale_id
          AND original.doc_kind = 'invoice')
       AND NEW.original_fiscal_uuid IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'credit note must preserve the original local fiscal UUID or remain explicitly non-fiscal');
END;

CREATE TRIGGER fiscal_queue_matches_sale_insert
BEFORE INSERT ON fiscal_queue
WHEN NOT EXISTS (
       SELECT 1 FROM sale s JOIN fiscal_document d ON d.sale_id = s.id
        WHERE s.id = NEW.sale_id
          AND d.id = NEW.document_fact_id
          AND s.store_id = NEW.store_id
          AND s.receipt_number = NEW.document_id
          AND d.store_id = NEW.store_id
          AND d.document_id = NEW.document_id
          AND d.profile_id = NEW.profile_id
          AND d.issue_date IS NEW.issue_date
          AND d.fiscal_uuid = NEW.fiscal_uuid
          AND d.doc_kind = NEW.doc_kind
          AND ((NEW.depends_on IS NULL AND d.depends_on IS NULL)
            OR d.depends_on = (SELECT parent.document_fact_id FROM fiscal_queue parent
                                WHERE parent.id = NEW.depends_on))
          AND ((s.doc_type = 'sale' AND NEW.doc_kind = 'invoice')
            OR (s.doc_type = 'refund' AND NEW.doc_kind = 'credit_note')))
  OR NEW.icv IS NOT NULL OR NEW.payload_xml IS NOT NULL
  OR (NEW.doc_kind = 'invoice' AND NEW.depends_on IS NOT NULL)
  OR (NEW.doc_kind = 'credit_note' AND (
       NEW.depends_on IS NULL OR NEW.depends_on = NEW.id OR NOT EXISTS (
         SELECT 1 FROM fiscal_queue parent JOIN sale refund ON refund.id = NEW.sale_id
          WHERE parent.id = NEW.depends_on
            AND parent.store_id = NEW.store_id
            AND parent.doc_kind = 'invoice'
            AND parent.sale_id = refund.ref_sale_id)))
BEGIN
  SELECT RAISE(ABORT, 'fiscal queue identity, document kind, store and dependency must match the sale');
END;
CREATE TRIGGER fiscal_queue_matches_sale_update
BEFORE UPDATE OF document_fact_id, sale_id, store_id, doc_kind, document_id, depends_on ON fiscal_queue
WHEN NOT EXISTS (
       SELECT 1 FROM sale s JOIN fiscal_document d ON d.sale_id = s.id
        WHERE s.id = NEW.sale_id
          AND d.id = NEW.document_fact_id
          AND s.store_id = NEW.store_id
          AND s.receipt_number = NEW.document_id
          AND d.store_id = NEW.store_id
          AND d.document_id = NEW.document_id
          AND d.profile_id = NEW.profile_id
          AND (d.issue_date IS NEW.issue_date
            OR (d.issue_date IS NULL AND NEW.issue_date IS NOT NULL
                AND NEW.payload_xml IS NOT NULL))
          AND d.fiscal_uuid = NEW.fiscal_uuid
          AND d.doc_kind = NEW.doc_kind
          AND ((NEW.depends_on IS NULL AND d.depends_on IS NULL)
            OR d.depends_on = (SELECT parent.document_fact_id FROM fiscal_queue parent
                                WHERE parent.id = NEW.depends_on))
          AND ((s.doc_type = 'sale' AND NEW.doc_kind = 'invoice')
            OR (s.doc_type = 'refund' AND NEW.doc_kind = 'credit_note')))
  OR (NEW.doc_kind = 'invoice' AND NEW.depends_on IS NOT NULL)
  OR (NEW.doc_kind = 'credit_note' AND (
       NEW.depends_on IS NULL OR NEW.depends_on = NEW.id OR NOT EXISTS (
         SELECT 1 FROM fiscal_queue parent JOIN sale refund ON refund.id = NEW.sale_id
          WHERE parent.id = NEW.depends_on
            AND parent.store_id = NEW.store_id
            AND parent.doc_kind = 'invoice'
            AND parent.sale_id = refund.ref_sale_id)))
BEGIN
  SELECT RAISE(ABORT, 'fiscal queue identity, document kind, store and dependency must match the sale');
END;

-- Every transition is evidence. `fiscal_queue.state` is the operational
-- projection used by the worker; this event ledger explains how it got there.
CREATE TABLE fiscal_queue_event (
  id             BLOB PRIMARY KEY,
  queue_id       BLOB NOT NULL,                 -- origin-register projection id; no server FK
  document_fact_id BLOB NOT NULL REFERENCES fiscal_document(id),
  sync_commit_id BLOB NOT NULL REFERENCES sync_commit(id),
  event_no       INTEGER NOT NULL CHECK (event_no > 0),
  state          TEXT NOT NULL CHECK (state IN (
                   'queued','sending','cleared','build_failed','rejected','dead','skipped')),
  reason_code    TEXT,
  actor_id       BLOB REFERENCES app_user(id),
  lease_owner    TEXT,
  claimed_at     TEXT,
  lease_expires_at TEXT,
  next_attempt_at TEXT,
  error_code     TEXT,
  error_detail   TEXT,
  occurred_at    TEXT NOT NULL,
  UNIQUE (queue_id, event_no),
  CHECK ((state = 'sending') = (lease_owner IS NOT NULL)),
  CHECK ((state = 'sending') = (claimed_at IS NOT NULL)),
  CHECK ((state = 'sending') = (lease_expires_at IS NOT NULL))
) STRICT;

CREATE TRIGGER fiscal_queue_event_is_next
BEFORE INSERT ON fiscal_queue_event
WHEN NOT EXISTS (
       SELECT 1 FROM fiscal_queue q
        WHERE q.id = NEW.queue_id AND q.document_fact_id = NEW.document_fact_id)
  OR NOT EXISTS (
       SELECT 1 FROM fact_commit_member m
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'fiscal_queue_event' AND m.entity_id = NEW.id)
  OR NOT EXISTS (SELECT 1 FROM sync_commit_ready ready WHERE ready.id = NEW.sync_commit_id)
  OR NEW.event_no <> COALESCE((
  SELECT MAX(event_no) + 1 FROM fiscal_queue_event WHERE queue_id = NEW.queue_id), 1)
BEGIN
  SELECT RAISE(ABORT, 'fiscal queue events must be contiguous');
END;

CREATE TRIGGER fiscal_queue_payload_matches_event
BEFORE UPDATE OF issue_date, icv, invoice_type_name, payload_xml, payload_hash,
                 builder_version, spec_package_id ON fiscal_queue
WHEN (NEW.issue_date IS NOT OLD.issue_date
      OR NEW.icv IS NOT OLD.icv
      OR NEW.invoice_type_name IS NOT OLD.invoice_type_name
      OR NEW.payload_xml IS NOT OLD.payload_xml
      OR NEW.payload_hash IS NOT OLD.payload_hash
      OR NEW.builder_version IS NOT OLD.builder_version
      OR NEW.spec_package_id IS NOT OLD.spec_package_id)
 AND NOT EXISTS (
  SELECT 1 FROM fiscal_payload_event p
   WHERE p.document_fact_id = NEW.document_fact_id
     AND p.icv = NEW.icv
     AND p.issue_date = NEW.issue_date
     AND p.invoice_type_name = NEW.invoice_type_name
     AND p.payload_xml = NEW.payload_xml
     AND p.payload_hash = NEW.payload_hash
     AND p.builder_version = NEW.builder_version
     AND p.spec_package_id = NEW.spec_package_id)
BEGIN
  SELECT RAISE(ABORT, 'queue allocation and XML must project one immutable fiscal payload event');
END;

CREATE TRIGGER fiscal_queue_event_transition_allowed
BEFORE INSERT ON fiscal_queue_event
WHEN (NEW.event_no = 1 AND NEW.state <> 'queued')
  OR (NEW.event_no > 1 AND NOT EXISTS (
       SELECT 1 FROM fiscal_queue_event prior
        WHERE prior.queue_id = NEW.queue_id AND prior.event_no = NEW.event_no - 1
          AND ((prior.state = 'queued' AND NEW.state IN ('sending','build_failed','skipped'))
            OR (prior.state = 'build_failed' AND NEW.state = 'queued')
            OR (prior.state = 'sending' AND NEW.state IN ('queued','cleared','rejected','dead'))
            OR (prior.state = 'dead' AND NEW.state = 'queued' AND EXISTS (
                 SELECT 1 FROM fiscal_reconciliation_issue i
                 JOIN fiscal_resolution_event r ON r.issue_id = i.id
                  WHERE i.queue_id = NEW.queue_id AND i.issue_class = 'dead'
                    AND r.action = 'requeued')))))
  OR (NEW.state = 'cleared' AND NOT EXISTS (
       SELECT 1 FROM fiscal_result r WHERE r.queue_id = NEW.queue_id))
BEGIN
  SELECT RAISE(ABORT, 'invalid fiscal queue transition or missing accepted result');
END;

CREATE TRIGGER fiscal_queue_project_event
AFTER INSERT ON fiscal_queue_event
BEGIN
  UPDATE fiscal_queue
     SET state = NEW.state,
         attempts = attempts + CASE WHEN NEW.state = 'sending' THEN 1 ELSE 0 END,
         next_attempt_at = NEW.next_attempt_at,
         lease_owner = NEW.lease_owner,
         claimed_at = NEW.claimed_at,
         lease_expires_at = NEW.lease_expires_at,
         last_error_code = NEW.error_code,
         last_error = NEW.error_detail,
         updated_at = NEW.occurred_at
   WHERE id = NEW.queue_id;
  SELECT CASE WHEN changes() <> 1
    THEN RAISE(ABORT, 'fiscal event requires its queue projection') END;
END;

CREATE TABLE fiscal_result (
  queue_id          BLOB PRIMARY KEY,          -- origin-register projection id; no server FK
  document_fact_id  BLOB NOT NULL UNIQUE REFERENCES fiscal_document(id),
  sync_commit_id    BLOB NOT NULL REFERENCES sync_commit(id),
  sale_id           BLOB NOT NULL UNIQUE REFERENCES sale(id),
  document_id       TEXT NOT NULL,
  issue_date        TEXT NOT NULL,
  invoice_type_name TEXT NOT NULL,
  fiscal_uuid       TEXT NOT NULL UNIQUE,
  icv               INTEGER NOT NULL,
  submitted_xml     TEXT NOT NULL,
  submitted_hash    TEXT NOT NULL,
  qr_payload        TEXT NOT NULL,          -- exact persisted QR for reprint
  qr_payload_hash   TEXT NOT NULL,
  raw_response      TEXT NOT NULL,
  raw_response_hash TEXT NOT NULL,
  cleared_at        TEXT NOT NULL,
  environment       TEXT NOT NULL CHECK (environment IN ('production','mock')),
  spec_package_id   BLOB NOT NULL REFERENCES fiscal_spec_package(id)
) STRICT;

CREATE TRIGGER fiscal_result_matches_sending_queue
BEFORE INSERT ON fiscal_result
WHEN NOT EXISTS (
  SELECT 1 FROM fiscal_queue q
   WHERE q.id = NEW.queue_id AND q.state = 'sending'
     AND q.document_fact_id = NEW.document_fact_id
     AND q.sale_id = NEW.sale_id
     AND q.document_id = NEW.document_id
     AND q.issue_date = NEW.issue_date
     AND q.invoice_type_name = NEW.invoice_type_name
     AND q.fiscal_uuid = NEW.fiscal_uuid
     AND q.icv = NEW.icv
     AND q.payload_xml = NEW.submitted_xml
     AND q.payload_hash = NEW.submitted_hash
     AND q.spec_package_id = NEW.spec_package_id)
  OR NOT EXISTS (
     SELECT 1 FROM fact_commit_member m
      WHERE m.commit_id = NEW.sync_commit_id
        AND m.entity = 'fiscal_result' AND m.entity_id = NEW.queue_id)
  OR NOT EXISTS (SELECT 1 FROM sync_commit_ready ready WHERE ready.id = NEW.sync_commit_id)
BEGIN
  SELECT RAISE(ABORT, 'accepted fiscal evidence must byte-match the queue and its durable result commit');
END;

-- One reconciliation class per failure source. `build_failed` means the local
-- pre-submit identity or line check failed; `rejected` means ISTD answered no;
-- `dead` means retry policy was exhausted. Operators must not conflate them.
CREATE TABLE fiscal_reconciliation_issue (
  id             BLOB PRIMARY KEY,
  queue_id       BLOB NOT NULL,                 -- origin-register projection id; no server FK
  document_fact_id BLOB NOT NULL REFERENCES fiscal_document(id),
  sync_commit_id BLOB NOT NULL REFERENCES sync_commit(id),
  issue_class    TEXT NOT NULL
                   CHECK (issue_class IN ('build_failed','rejected','dead','ambiguous_response')),
  error_code     TEXT,
  error_body     TEXT NOT NULL,
  failed_check   TEXT,
  operator_path  TEXT NOT NULL
                   CHECK (operator_path IN (
                     'correct_configuration_and_rebuild','deploy_builder_fix_and_rebuild',
                     'requeue_identical',
                     'portal_reconcile','credit_and_reinvoice','escalate_to_istd')),
  occurred_at    TEXT NOT NULL
) STRICT;
CREATE INDEX idx_fiscal_reconciliation_issue_queue
  ON fiscal_reconciliation_issue(queue_id, occurred_at);

CREATE TABLE fiscal_resolution_event (
  id             BLOB PRIMARY KEY,
  issue_id       BLOB NOT NULL REFERENCES fiscal_reconciliation_issue(id),
  sync_commit_id BLOB NOT NULL REFERENCES sync_commit(id),
  event_no       INTEGER NOT NULL CHECK (event_no > 0),
  action         TEXT NOT NULL
                   CHECK (action IN ('requeued','superseded','reconciled','escalated','written_off')),
  actor_id       BLOB NOT NULL REFERENCES app_user(id),
  note           TEXT,
  occurred_at    TEXT NOT NULL,
  UNIQUE (issue_id, event_no)
) STRICT;

CREATE TABLE fiscal_credentials_ref (       -- POINTER only. Secrets live in the keyring.
  store_id       BLOB PRIMARY KEY REFERENCES store(id),
  keyring_entry  TEXT NOT NULL,
  credential_version TEXT NOT NULL,
  credential_scope_kind TEXT NOT NULL
                     CHECK (credential_scope_kind IN ('store','income_source','tin')),
  credential_scope_id TEXT NOT NULL,
  client_id_hint TEXT,                      -- last 4 chars, for the diagnostics screen
  tin            TEXT NOT NULL,
  income_source_sequence TEXT NOT NULL,
  environment    TEXT NOT NULL CHECK (environment IN ('production','mock')),
  provisioned_at TEXT NOT NULL,
  rotated_at     TEXT,
  revoked_at     TEXT,
  updated_at     TEXT NOT NULL
) STRICT;

CREATE TRIGGER fiscal_queue_identity_frozen
BEFORE UPDATE ON fiscal_queue
WHEN NEW.document_fact_id IS NOT OLD.document_fact_id
  OR NEW.sale_id IS NOT OLD.sale_id
  OR NEW.store_id IS NOT OLD.store_id
  OR NEW.doc_kind IS NOT OLD.doc_kind
  OR NEW.document_id IS NOT OLD.document_id
  OR NEW.profile_id IS NOT OLD.profile_id
  OR (OLD.issue_date IS NOT NULL AND NEW.issue_date IS NOT OLD.issue_date)
  OR NEW.fiscal_uuid IS NOT OLD.fiscal_uuid
  OR NEW.depends_on IS NOT OLD.depends_on
  OR (OLD.icv IS NOT NULL AND NEW.icv IS NOT OLD.icv)
  OR (OLD.payload_xml IS NOT NULL AND NEW.payload_xml IS NOT OLD.payload_xml)
  OR (OLD.payload_hash IS NOT NULL AND NEW.payload_hash IS NOT OLD.payload_hash)
  OR (OLD.invoice_type_name IS NOT NULL AND NEW.invoice_type_name IS NOT OLD.invoice_type_name)
  OR (OLD.builder_version IS NOT NULL AND NEW.builder_version IS NOT OLD.builder_version)
  OR (OLD.spec_package_id IS NOT NULL AND NEW.spec_package_id IS NOT OLD.spec_package_id)
BEGIN
  SELECT RAISE(ABORT, 'fiscal identity, allocated ICV and payload are write-once');
END;

CREATE TRIGGER fiscal_document_no_update
BEFORE UPDATE ON fiscal_document BEGIN
  SELECT RAISE(ABORT, 'local fiscal UUID and document identity are immutable');
END;
CREATE TRIGGER fiscal_document_no_delete
BEFORE DELETE ON fiscal_document BEGIN
  SELECT RAISE(ABORT, 'fiscal identity cannot be deleted while clearance waits');
END;
CREATE TRIGGER fiscal_payload_event_no_update
BEFORE UPDATE ON fiscal_payload_event BEGIN
  SELECT RAISE(ABORT, 'allocated ICV and submitted payload are immutable');
END;
CREATE TRIGGER fiscal_payload_event_no_delete
BEFORE DELETE ON fiscal_payload_event BEGIN
  SELECT RAISE(ABORT, 'allocated fiscal payload evidence cannot be deleted');
END;

CREATE TRIGGER fiscal_queue_state_matches_event
BEFORE UPDATE OF state, attempts, next_attempt_at, lease_owner, claimed_at,
                 lease_expires_at, last_error_code, last_error, updated_at ON fiscal_queue
WHEN NOT EXISTS (
  SELECT 1 FROM fiscal_queue_event e
   WHERE e.queue_id = NEW.id
     AND e.event_no = (SELECT MAX(x.event_no) FROM fiscal_queue_event x
                        WHERE x.queue_id = NEW.id)
     AND e.state = NEW.state
     AND e.next_attempt_at IS NEW.next_attempt_at
     AND e.lease_owner IS NEW.lease_owner
     AND e.claimed_at IS NEW.claimed_at
     AND e.lease_expires_at IS NEW.lease_expires_at
     AND e.error_code IS NEW.last_error_code
     AND e.error_detail IS NEW.last_error
     AND e.occurred_at = NEW.updated_at
     AND NEW.attempts = OLD.attempts + CASE WHEN e.state = 'sending' THEN 1 ELSE 0 END)
BEGIN
  SELECT RAISE(ABORT, 'fiscal queue state changes only by appending its next event');
END;

CREATE TRIGGER completed_sale_requires_fiscal_queue_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND NEW.is_training = 0
 AND EXISTS (SELECT 1 FROM store st WHERE st.id = NEW.store_id
              AND st.fiscal_obligation = 'required'
              AND st.fiscal_profile = 'jordan_jofotara')
 AND NOT EXISTS (
   SELECT 1 FROM fiscal_queue q
   JOIN fiscal_document d ON d.id = q.document_fact_id
   JOIN fiscal_queue_event e ON e.queue_id = q.id
   JOIN fact_commit_member m ON m.entity = 'fiscal_queue_event'
                            AND m.entity_id = e.id
                            AND m.commit_id = NEW.sync_commit_id
   JOIN fact_commit_member identity_member
     ON identity_member.entity = 'fiscal_document'
    AND identity_member.entity_id = d.id
    AND identity_member.commit_id = NEW.sync_commit_id
    WHERE q.sale_id = NEW.id
      AND d.sync_commit_id = NEW.sync_commit_id
      AND q.fiscal_uuid <> ''
      AND q.doc_kind = CASE WHEN NEW.doc_type = 'refund' THEN 'credit_note' ELSE 'invoice' END
      AND e.event_no = 1 AND e.state = 'queued')
BEGIN
  SELECT RAISE(ABORT, 'a fiscal-required sale must atomically create its local UUID queue fact');
END;
CREATE TRIGGER completed_sale_requires_fiscal_queue_update
BEFORE UPDATE OF status, is_training, store_id, doc_type, sync_commit_id ON sale
WHEN NEW.status = 'completed' AND NEW.is_training = 0
 AND EXISTS (SELECT 1 FROM store st WHERE st.id = NEW.store_id
              AND st.fiscal_obligation = 'required'
              AND st.fiscal_profile = 'jordan_jofotara')
 AND NOT EXISTS (
   SELECT 1 FROM fiscal_queue q
   JOIN fiscal_document d ON d.id = q.document_fact_id
   JOIN fiscal_queue_event e ON e.queue_id = q.id
   JOIN fact_commit_member m ON m.entity = 'fiscal_queue_event'
                            AND m.entity_id = e.id
                            AND m.commit_id = NEW.sync_commit_id
   JOIN fact_commit_member identity_member
     ON identity_member.entity = 'fiscal_document'
    AND identity_member.entity_id = d.id
    AND identity_member.commit_id = NEW.sync_commit_id
    WHERE q.sale_id = NEW.id
      AND d.sync_commit_id = NEW.sync_commit_id
      AND q.fiscal_uuid <> ''
      AND q.doc_kind = CASE WHEN NEW.doc_type = 'refund' THEN 'credit_note' ELSE 'invoice' END
      AND e.event_no = 1 AND e.state = 'queued')
BEGIN
  SELECT RAISE(ABORT, 'a fiscal-required sale must atomically create its local UUID queue fact');
END;

CREATE TRIGGER fiscal_queue_event_no_update
BEFORE UPDATE ON fiscal_queue_event BEGIN
  SELECT RAISE(ABORT, 'fiscal queue transitions are append-only');
END;
CREATE TRIGGER fiscal_queue_event_no_delete
BEFORE DELETE ON fiscal_queue_event BEGIN
  SELECT RAISE(ABORT, 'fiscal queue transition history cannot be deleted');
END;
CREATE TRIGGER fiscal_result_no_update
BEFORE UPDATE ON fiscal_result BEGIN
  SELECT RAISE(ABORT, 'an accepted fiscal artifact is immutable');
END;
CREATE TRIGGER fiscal_result_no_delete
BEFORE DELETE ON fiscal_result BEGIN
  SELECT RAISE(ABORT, 'accepted fiscal evidence cannot be deleted');
END;
CREATE TRIGGER fiscal_spec_package_no_update
BEFORE UPDATE ON fiscal_spec_package BEGIN
  SELECT RAISE(ABORT, 'a pinned fiscal specification package is immutable');
END;
CREATE TRIGGER fiscal_spec_package_no_delete
BEFORE DELETE ON fiscal_spec_package BEGIN
  SELECT RAISE(ABORT, 'a specification package referenced by fiscal evidence cannot be deleted');
END;
CREATE TRIGGER fiscal_reconciliation_issue_no_update
BEFORE UPDATE ON fiscal_reconciliation_issue BEGIN
  SELECT RAISE(ABORT, 'a fiscal reconciliation issue is immutable — append a resolution event');
END;
CREATE TRIGGER fiscal_reconciliation_issue_no_delete
BEFORE DELETE ON fiscal_reconciliation_issue BEGIN
  SELECT RAISE(ABORT, 'fiscal reconciliation history cannot be deleted');
END;
CREATE TRIGGER fiscal_reconciliation_issue_path
BEFORE INSERT ON fiscal_reconciliation_issue
WHEN NOT EXISTS (
       SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'fiscal_reconciliation_issue' AND m.entity_id = NEW.id)
  OR (NEW.issue_class = 'build_failed'
       AND NEW.operator_path NOT IN ('correct_configuration_and_rebuild','deploy_builder_fix_and_rebuild'))
  OR (NEW.issue_class = 'rejected'
       AND NEW.operator_path NOT IN ('credit_and_reinvoice','escalate_to_istd'))
  OR (NEW.issue_class = 'dead' AND NEW.operator_path <> 'requeue_identical')
  OR (NEW.issue_class = 'ambiguous_response'
       AND NEW.operator_path NOT IN ('portal_reconcile','escalate_to_istd'))
  OR NOT EXISTS (
       SELECT 1 FROM fiscal_queue q WHERE q.id = NEW.queue_id
         AND q.document_fact_id = NEW.document_fact_id
         AND ((NEW.issue_class = 'build_failed' AND q.state = 'build_failed')
           OR (NEW.issue_class = 'rejected' AND q.state = 'rejected')
           OR (NEW.issue_class = 'dead' AND q.state = 'dead')
           OR (NEW.issue_class = 'ambiguous_response' AND q.state IN ('sending','queued'))))
BEGIN
  SELECT RAISE(ABORT, 'fiscal issue class must match queue state and its operator remediation path');
END;
CREATE TRIGGER fiscal_resolution_event_is_next
BEFORE INSERT ON fiscal_resolution_event
WHEN NOT EXISTS (
       SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'fiscal_resolution_event' AND m.entity_id = NEW.id)
  OR NEW.event_no <> COALESCE((
  SELECT MAX(event_no) + 1 FROM fiscal_resolution_event WHERE issue_id = NEW.issue_id), 1)
BEGIN
  SELECT RAISE(ABORT, 'fiscal resolution events must be contiguous');
END;
CREATE TRIGGER fiscal_resolution_event_no_update
BEFORE UPDATE ON fiscal_resolution_event BEGIN
  SELECT RAISE(ABORT, 'fiscal resolutions are append-only');
END;
CREATE TRIGGER fiscal_resolution_event_no_delete
BEFORE DELETE ON fiscal_resolution_event BEGIN
  SELECT RAISE(ABORT, 'fiscal resolution history cannot be deleted');
END;
```

Phase 2 proves `single_register_local_allocator_assigns_store_scoped_icv_at_first_submission`: the
register completes a sale before allocation, then locks its own store-scoped row in-process, records
the allocating register in `allocator_ref`, and never changes the UUID or allocated ICV on replay.
Phase 3 owns `two_registers_offline_then_reconnect_allocate_distinct_icvs`: both registers complete
sales with `icv IS NULL`, reconnect in either order, and server-issued one-value leases assign
distinct monotonic ICVs without changing either `fiscal_uuid`.

---

## 0011 — customers and loyalty  ·  Phase 3, microstep 3.4.1

PDPL is the spec for this migration (master plan B.3).

```sql
CREATE TABLE customer (
  id            BLOB PRIMARY KEY,
  org_id        BLOB NOT NULL REFERENCES org(id),
  name          TEXT,
  phone         TEXT,                      -- primary lookup at the register
  email         TEXT,
  -- Erasure is ANONYMIZATION: blank the person, keep the immutable financial
  -- facts (master plan B.3). Never a hard delete.
  is_anonymized INTEGER NOT NULL DEFAULT 0,
  anonymized_at TEXT,
  deleted_at    TEXT,
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version       INTEGER NOT NULL DEFAULT 0,
  CHECK ((is_anonymized = 0 AND anonymized_at IS NULL)
      OR (is_anonymized = 1 AND anonymized_at IS NOT NULL
          AND name IS NULL AND phone IS NULL AND email IS NULL))
) STRICT;
CREATE UNIQUE INDEX idx_customer_phone_live ON customer(org_id, phone)
  WHERE phone IS NOT NULL AND deleted_at IS NULL AND is_anonymized = 0;

-- The notice is immutable wording; consent is an append-only event ledger.
-- Withdrawal therefore does not overwrite the grant that preceded it, and
-- offline registers never field-level-LWW legal evidence.
CREATE TABLE consent_notice (
  id             BLOB PRIMARY KEY,
  org_id         BLOB NOT NULL REFERENCES org(id),
  kind           TEXT NOT NULL
                   CHECK (kind IN ('loyalty_terms','marketing','data_processing')),
  text_version   TEXT NOT NULL,
  locale         TEXT NOT NULL CHECK (locale IN ('ar','en')),
  controller_name TEXT NOT NULL,
  controller_contact TEXT NOT NULL,
  wording        TEXT NOT NULL,
  purpose_options_json TEXT NOT NULL,
  data_categories_json TEXT NOT NULL,
  recipients_json TEXT NOT NULL,
  transfer_destinations_json TEXT NOT NULL,
  transfer_safeguards_json TEXT NOT NULL,
  retention_wording TEXT NOT NULL,
  hash_algorithm TEXT NOT NULL CHECK (hash_algorithm IN ('blake3','sha256')),
  wording_hash   BLOB NOT NULL CHECK (length(wording_hash) = 32),
  published_at   TEXT NOT NULL,
  UNIQUE (org_id, kind, text_version, locale)
) STRICT;

-- Counsel-approved legal bases are versioned reference evidence, not free-text
-- guesses embedded in events. The table is deliberately empty until the OPEN
-- item below is settled for the actual controller/processor deployment.
CREATE TABLE privacy_lawful_basis (
  id             BLOB PRIMARY KEY,
  org_id         BLOB NOT NULL REFERENCES org(id),
  basis_code     TEXT NOT NULL,
  source_ref     TEXT NOT NULL,
  source_version TEXT NOT NULL,
  approved_by    TEXT NOT NULL,
  approved_at    TEXT NOT NULL,
  UNIQUE (org_id, basis_code, source_version)
) STRICT;

CREATE TABLE consent_event (
  id             BLOB PRIMARY KEY,
  org_id         BLOB NOT NULL REFERENCES org(id),
  customer_id    BLOB NOT NULL REFERENCES customer(id),
  notice_id      BLOB NOT NULL REFERENCES consent_notice(id),
  kind           TEXT NOT NULL
                   CHECK (kind IN ('loyalty_terms','marketing','data_processing')),
  action         TEXT NOT NULL CHECK (action IN ('grant','withdraw')),
  purpose_code   TEXT NOT NULL,
  lawful_basis_id BLOB NOT NULL REFERENCES privacy_lawful_basis(id),
  selection_json TEXT NOT NULL,
  supersedes_event_id BLOB REFERENCES consent_event(id),
  origin_register_id BLOB REFERENCES register(id),
  origin_event_seq INTEGER CHECK (origin_event_seq IS NULL OR origin_event_seq > 0),
  captured_by    BLOB REFERENCES app_user(id),
  captured_at    TEXT NOT NULL,
  channel        TEXT NOT NULL CHECK (channel IN ('register','backoffice','web')),
  evidence_hash_algorithm TEXT NOT NULL CHECK (evidence_hash_algorithm IN ('blake3','sha256')),
  evidence_hash  BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  CHECK (action = 'grant' OR supersedes_event_id IS NOT NULL),
  CHECK ((origin_register_id IS NULL) = (origin_event_seq IS NULL))
) STRICT;
CREATE INDEX idx_consent_event_customer_kind
  ON consent_event(customer_id, kind, purpose_code, captured_at);

-- Server ordering is an appended acceptance fact. Device time never resolves
-- two offline branches, and accepting an event does not UPDATE legal evidence.
CREATE TABLE consent_acceptance (
  event_id       BLOB PRIMARY KEY REFERENCES consent_event(id),
  org_id         BLOB NOT NULL REFERENCES org(id),
  server_version INTEGER NOT NULL CHECK (server_version > 0),
  accepted_at    TEXT NOT NULL,
  UNIQUE (org_id, server_version)
) STRICT;

-- Rebuildable authority projection. An unaccepted local withdrawal dominates
-- an accepted grant conservatively; a pending local grant never becomes
-- effective until the server assigns its version.
CREATE VIEW consent_current AS
WITH candidates AS (
  SELECT e.org_id, e.customer_id, e.kind, e.purpose_code, e.id AS latest_event_id,
         e.action, a.server_version, a.accepted_at AS effective_at, 0 AS pending_withdrawal
    FROM consent_event e JOIN consent_acceptance a ON a.event_id = e.id
  UNION ALL
  SELECT e.org_id, e.customer_id, e.kind, e.purpose_code, e.id,
         e.action, NULL, e.captured_at, 1
    FROM consent_event e LEFT JOIN consent_acceptance a ON a.event_id = e.id
   WHERE e.action = 'withdraw' AND a.event_id IS NULL
), ranked AS (
  SELECT candidates.*,
         ROW_NUMBER() OVER (
           PARTITION BY org_id, customer_id, kind, purpose_code
           ORDER BY pending_withdrawal DESC, server_version DESC, latest_event_id DESC
         ) AS rank_no
    FROM candidates
)
SELECT org_id, customer_id, kind, purpose_code, latest_event_id, action,
       server_version, effective_at,
       CASE WHEN pending_withdrawal = 1 THEN 'pending_server_acceptance' ELSE 'accepted' END AS status
  FROM ranked WHERE rank_no = 1;

CREATE TRIGGER consent_event_scope_and_evidence
BEFORE INSERT ON consent_event
WHEN NOT EXISTS (
       SELECT 1 FROM customer c JOIN consent_notice n
         ON n.id = NEW.notice_id AND n.org_id = c.org_id AND n.kind = NEW.kind
        JOIN privacy_lawful_basis b
          ON b.id = NEW.lawful_basis_id AND b.org_id = c.org_id
        WHERE c.id = NEW.customer_id AND c.org_id = NEW.org_id)
  OR (NEW.action = 'grant' AND NEW.supersedes_event_id IS NOT NULL)
  OR (NEW.action = 'withdraw' AND NOT EXISTS (
       SELECT 1 FROM consent_event prior
        WHERE prior.id = NEW.supersedes_event_id
          AND prior.org_id = NEW.org_id
          AND prior.customer_id = NEW.customer_id
          AND prior.kind = NEW.kind
          AND prior.purpose_code = NEW.purpose_code))
  OR (NEW.origin_register_id IS NOT NULL AND NOT EXISTS (
       SELECT 1 FROM register r JOIN store st ON st.id = r.store_id
        WHERE r.id = NEW.origin_register_id AND st.org_id = NEW.org_id))
BEGIN
  SELECT RAISE(ABORT, 'consent evidence, notice, basis, subject and superseded event must share one scope');
END;

CREATE TRIGGER consent_acceptance_matches_event
BEFORE INSERT ON consent_acceptance
WHEN NOT EXISTS (
  SELECT 1 FROM consent_event e WHERE e.id = NEW.event_id AND e.org_id = NEW.org_id)
BEGIN
  SELECT RAISE(ABORT, 'server acceptance must preserve consent-event organization');
END;

CREATE TABLE privacy_request_case (
  id             BLOB PRIMARY KEY,
  org_id         BLOB NOT NULL REFERENCES org(id),
  customer_id    BLOB NOT NULL REFERENCES customer(id),
  request_kind   TEXT NOT NULL
                   CHECK (request_kind IN (
                     'access','correction','erasure','objection','restriction','portability','complaint')),
  received_at    TEXT NOT NULL,
  due_at         TEXT NOT NULL,
  intake_channel TEXT NOT NULL,
  identity_evidence_hash BLOB NOT NULL,
  CHECK (due_at > received_at)
) STRICT;

CREATE TABLE privacy_request_event (
  id             BLOB PRIMARY KEY,
  case_id        BLOB NOT NULL REFERENCES privacy_request_case(id),
  event_no       INTEGER NOT NULL CHECK (event_no > 0),
  action         TEXT NOT NULL
                   CHECK (action IN (
                     'received','identity_verified','restricted','exported','corrected',
                     'anonymized','objected','complaint_escalated','denied','closed')),
  actor_id       BLOB REFERENCES app_user(id),
  evidence_ref   TEXT,
  occurred_at    TEXT NOT NULL,
  UNIQUE (case_id, event_no)
) STRICT;

CREATE TABLE privacy_request_current (     -- rebuildable projection only
  case_id         BLOB PRIMARY KEY REFERENCES privacy_request_case(id),
  latest_event_id BLOB NOT NULL REFERENCES privacy_request_event(id),
  action          TEXT NOT NULL,
  updated_at      TEXT NOT NULL
) STRICT;

CREATE TRIGGER privacy_request_event_is_next
BEFORE INSERT ON privacy_request_event
WHEN NEW.event_no <> COALESCE((
  SELECT MAX(event_no) + 1 FROM privacy_request_event WHERE case_id = NEW.case_id), 1)
  OR (NEW.event_no = 1 AND NEW.action <> 'received')
BEGIN
  SELECT RAISE(ABORT, 'privacy request events begin at received and remain contiguous');
END;

CREATE TRIGGER privacy_request_current_matches_event_insert
BEFORE INSERT ON privacy_request_current
WHEN NOT EXISTS (
  SELECT 1 FROM privacy_request_event e
   WHERE e.id = NEW.latest_event_id AND e.case_id = NEW.case_id
     AND e.action = NEW.action AND e.occurred_at = NEW.updated_at
     AND e.event_no = (SELECT MAX(x.event_no) FROM privacy_request_event x
                        WHERE x.case_id = NEW.case_id))
BEGIN
  SELECT RAISE(ABORT, 'privacy request projection must name the latest event');
END;
CREATE TRIGGER privacy_request_current_matches_event_update
BEFORE UPDATE ON privacy_request_current
WHEN NOT EXISTS (
  SELECT 1 FROM privacy_request_event e
   WHERE e.id = NEW.latest_event_id AND e.case_id = NEW.case_id
     AND e.action = NEW.action AND e.occurred_at = NEW.updated_at
     AND e.event_no = (SELECT MAX(x.event_no) FROM privacy_request_event x
                        WHERE x.case_id = NEW.case_id))
BEGIN
  SELECT RAISE(ABORT, 'privacy request projection must name the latest event');
END;
CREATE TRIGGER privacy_request_event_projects_current
AFTER INSERT ON privacy_request_event
BEGIN
  INSERT INTO privacy_request_current(case_id, latest_event_id, action, updated_at)
  VALUES (NEW.case_id, NEW.id, NEW.action, NEW.occurred_at)
  ON CONFLICT(case_id) DO UPDATE SET
    latest_event_id = excluded.latest_event_id,
    action = excluded.action,
    updated_at = excluded.updated_at;
END;

-- Anonymisation changes the mutable customer projection, then appends this
-- non-PII tombstone. Syncing the tombstone prevents an older offline profile
-- update from resurrecting erased personal data.
CREATE TABLE privacy_tombstone (
  id             BLOB PRIMARY KEY,
  org_id         BLOB NOT NULL REFERENCES org(id),
  customer_id    BLOB NOT NULL REFERENCES customer(id),
  request_id     BLOB NOT NULL REFERENCES privacy_request_case(id),
  subject_hmac   BLOB NOT NULL,
  hmac_key_version TEXT NOT NULL,
  reason_code    TEXT NOT NULL,
  anonymized_at  TEXT NOT NULL,
  actor_id       BLOB REFERENCES app_user(id),
  UNIQUE (org_id, request_id)
) STRICT;

-- Append-only ledger. Balance = Σ points_delta. Conflict-free across offline
-- registers, exactly like stock and cash (master plan C.8).
CREATE TABLE loyalty_tax_policy_version (
  id                    BLOB PRIMARY KEY,
  org_id                BLOB NOT NULL REFERENCES org(id),
  policy_version        TEXT NOT NULL,
  funding_source        TEXT NOT NULL CHECK (funding_source IN ('merchant','third_party','mixed')),
  approval_source_ref   TEXT NOT NULL,
  source_hash_algorithm TEXT NOT NULL CHECK (source_hash_algorithm IN ('blake3','sha256')),
  source_hash           BLOB NOT NULL CHECK (length(source_hash) = 32),
  approved_at           TEXT NOT NULL,
  created_at            TEXT NOT NULL,
  UNIQUE (org_id, policy_version, funding_source),
  UNIQUE (org_id, id)
) STRICT;

CREATE TABLE loyalty_tax_policy_current (
  org_id       BLOB PRIMARY KEY REFERENCES org(id),
  policy_id    BLOB,
  is_enabled   INTEGER NOT NULL DEFAULT 0 CHECK (is_enabled IN (0,1)),
  updated_at   TEXT NOT NULL,
  FOREIGN KEY (org_id, policy_id) REFERENCES loyalty_tax_policy_version(org_id, id),
  CHECK ((is_enabled = 1) = (policy_id IS NOT NULL))
) STRICT;

CREATE TABLE loyalty_ledger (
  id            BLOB PRIMARY KEY,
  customer_id   BLOB NOT NULL REFERENCES customer(id),
  points_delta  INTEGER NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('earn','redeem','adjust','expire')),
  ref_kind      TEXT, ref_id BLOB,
  funding_source TEXT NOT NULL CHECK (funding_source IN ('merchant','third_party','mixed')),
  reimbursed_minor INTEGER NOT NULL DEFAULT 0 CHECK (reimbursed_minor >= 0),
  tax_policy_id BLOB NOT NULL REFERENCES loyalty_tax_policy_version(id),
  actor_id      BLOB REFERENCES app_user(id),
  reason        TEXT,
  occurred_at   TEXT NOT NULL
) STRICT;
CREATE TRIGGER loyalty_ledger_has_ready_commit
BEFORE INSERT ON loyalty_ledger
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'loyalty_ledger' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'loyalty fact requires its complete delivery envelope'); END;
CREATE INDEX idx_loyalty_customer ON loyalty_ledger(customer_id, occurred_at);

CREATE TRIGGER loyalty_requires_approved_policy
BEFORE INSERT ON loyalty_ledger
WHEN NOT EXISTS (
  SELECT 1 FROM customer c
  JOIN loyalty_tax_policy_current current ON current.org_id = c.org_id
  JOIN loyalty_tax_policy_version p
    ON p.id = current.policy_id AND p.org_id = c.org_id
   WHERE c.id = NEW.customer_id AND current.is_enabled = 1
     AND p.id = NEW.tax_policy_id AND p.funding_source = NEW.funding_source)
BEGIN
  SELECT RAISE(ABORT, 'loyalty remains disabled until the exact funding model has an approved tax policy');
END;

CREATE TRIGGER loyalty_tax_policy_version_no_update
BEFORE UPDATE ON loyalty_tax_policy_version BEGIN
  SELECT RAISE(ABORT, 'an approved loyalty tax-policy version is immutable');
END;
CREATE TRIGGER loyalty_tax_policy_version_no_delete
BEFORE DELETE ON loyalty_tax_policy_version BEGIN
  SELECT RAISE(ABORT, 'loyalty policy evidence referenced by a ledger cannot be deleted');
END;

-- ── loyalty_ledger is a ledger, so it is append-only too ───────────────────
--
-- Missed in the first pass over the fact tables, and caught by the Postgres
-- mirror further down this file, which already REVOKEs UPDATE and DELETE on it.
-- A points balance is SUM(points_delta) over this table; editing an event
-- rewrites a customer's entitlement retroactively and the balance cache then
-- faithfully reproduces the altered past. Corrections are 'adjust' rows, which
-- is why 'adjust' is in the kind CHECK.

CREATE TRIGGER loyalty_ledger_no_update
BEFORE UPDATE ON loyalty_ledger
BEGIN
  SELECT RAISE(ABORT, 'I-4: loyalty_ledger is append-only — post an adjust row');
END;

CREATE TRIGGER loyalty_ledger_no_delete
BEFORE DELETE ON loyalty_ledger
BEGIN
  SELECT RAISE(ABORT, 'I-4: loyalty_ledger is append-only — post an adjust row');
END;

CREATE TABLE loyalty_balance_cache (        -- rebuildable, like stock_cache
  customer_id   BLOB PRIMARY KEY REFERENCES customer(id),
  points        INTEGER NOT NULL DEFAULT 0,
  last_event_id BLOB NOT NULL REFERENCES loyalty_ledger(id),
  event_count   INTEGER NOT NULL CHECK (event_count > 0),
  updated_at    TEXT NOT NULL
) STRICT;

CREATE TRIGGER loyalty_balance_cache_matches_insert
BEFORE INSERT ON loyalty_balance_cache
WHEN NEW.points <> (SELECT COALESCE(SUM(points_delta), 0) FROM loyalty_ledger
                     WHERE customer_id = NEW.customer_id)
  OR NEW.event_count <> (SELECT COUNT(*) FROM loyalty_ledger
                          WHERE customer_id = NEW.customer_id)
BEGIN
  SELECT RAISE(ABORT, 'loyalty cache must equal the append-only ledger');
END;
CREATE TRIGGER loyalty_balance_cache_matches_update
BEFORE UPDATE ON loyalty_balance_cache
WHEN NEW.points <> (SELECT COALESCE(SUM(points_delta), 0) FROM loyalty_ledger
                     WHERE customer_id = NEW.customer_id)
  OR NEW.event_count <> (SELECT COUNT(*) FROM loyalty_ledger
                          WHERE customer_id = NEW.customer_id)
BEGIN
  SELECT RAISE(ABORT, 'loyalty cache must equal the append-only ledger');
END;
CREATE TRIGGER loyalty_ledger_projects_balance
AFTER INSERT ON loyalty_ledger
BEGIN
  INSERT INTO loyalty_balance_cache
    (customer_id, points, last_event_id, event_count, updated_at)
  VALUES (NEW.customer_id, NEW.points_delta, NEW.id, 1, NEW.occurred_at)
  ON CONFLICT(customer_id) DO UPDATE SET
    points = loyalty_balance_cache.points + NEW.points_delta,
    last_event_id = NEW.id,
    event_count = loyalty_balance_cache.event_count + 1,
    updated_at = NEW.occurred_at;
END;

-- 0009 owns the minimal stored-value ledger. Phase 3 may associate an
-- instrument with a customer without moving or rewriting any ledger entry.
ALTER TABLE stored_value_instrument
  ADD COLUMN customer_id BLOB REFERENCES customer(id);

-- Offline permissions are time-bounded leases signed by the server. Revocation
-- is distributed as a newer reference fact; the register may honor only an
-- unexpired lease whose capability and org/store scope match the action.
CREATE TABLE authorization_lease (
  id              BLOB PRIMARY KEY,
  org_id          BLOB NOT NULL REFERENCES org(id),
  user_id         BLOB NOT NULL REFERENCES app_user(id),
  capability      TEXT NOT NULL,
  store_id        BLOB REFERENCES store(id),
  issued_at       TEXT NOT NULL,
  expires_at      TEXT NOT NULL,
  server_version  INTEGER NOT NULL,
  signature       BLOB NOT NULL,
  revoked_at      TEXT,
  CHECK (expires_at > issued_at)
) STRICT;

-- The recovery code never appears here. This is only the wrapped data-key
-- envelope that Phase 3 stores with the org and syncs to authorized recovery
-- tooling; every backup carries the same versioned envelope beside its bytes.
CREATE TABLE org_recovery_envelope (
  id                 BLOB PRIMARY KEY,
  org_id             BLOB NOT NULL REFERENCES org(id),
  data_key_id        TEXT NOT NULL,
  wrap_algorithm     TEXT NOT NULL,
  kdf_algorithm      TEXT NOT NULL,
  kdf_parameters     TEXT NOT NULL,
  wrapped_data_key   BLOB NOT NULL,
  created_at         TEXT NOT NULL,
  retired_at         TEXT,
  UNIQUE (org_id, data_key_id)
) STRICT;

CREATE TRIGGER consent_notice_no_update
BEFORE UPDATE ON consent_notice BEGIN
  SELECT RAISE(ABORT, 'published consent wording is immutable — publish a new version');
END;
CREATE TRIGGER consent_notice_no_delete
BEFORE DELETE ON consent_notice BEGIN
  SELECT RAISE(ABORT, 'consent wording referenced by evidence cannot be deleted');
END;
CREATE TRIGGER privacy_lawful_basis_no_update
BEFORE UPDATE ON privacy_lawful_basis BEGIN
  SELECT RAISE(ABORT, 'an approved lawful-basis version is immutable');
END;
CREATE TRIGGER privacy_lawful_basis_no_delete
BEFORE DELETE ON privacy_lawful_basis BEGIN
  SELECT RAISE(ABORT, 'a lawful basis referenced by consent evidence cannot be deleted');
END;
CREATE TRIGGER consent_event_no_update
BEFORE UPDATE ON consent_event BEGIN
  SELECT RAISE(ABORT, 'consent evidence is append-only');
END;
CREATE TRIGGER consent_acceptance_no_update
BEFORE UPDATE ON consent_acceptance BEGIN
  SELECT RAISE(ABORT, 'server ordering of consent evidence is append-only');
END;
CREATE TRIGGER consent_acceptance_no_delete
BEFORE DELETE ON consent_acceptance BEGIN
  SELECT RAISE(ABORT, 'accepted consent ordering evidence cannot be deleted');
END;
CREATE TRIGGER customer_no_delete
BEFORE DELETE ON customer BEGIN
  SELECT RAISE(ABORT, 'customer erasure anonymizes the projection; it never deletes financial lineage');
END;
CREATE TRIGGER consent_event_no_delete
BEFORE DELETE ON consent_event BEGIN
  SELECT RAISE(ABORT, 'consent evidence cannot be deleted');
END;
CREATE TRIGGER privacy_tombstone_no_update
BEFORE UPDATE ON privacy_tombstone BEGIN
  SELECT RAISE(ABORT, 'a privacy tombstone is immutable');
END;
CREATE TRIGGER privacy_tombstone_no_delete
BEFORE DELETE ON privacy_tombstone BEGIN
  SELECT RAISE(ABORT, 'a privacy tombstone cannot be deleted');
END;
CREATE TRIGGER privacy_request_case_no_update
BEFORE UPDATE ON privacy_request_case BEGIN
  SELECT RAISE(ABORT, 'a privacy request is immutable — append a case event');
END;
CREATE TRIGGER privacy_request_case_no_delete
BEFORE DELETE ON privacy_request_case BEGIN
  SELECT RAISE(ABORT, 'privacy request history cannot be deleted');
END;
CREATE TRIGGER privacy_request_event_no_update
BEFORE UPDATE ON privacy_request_event BEGIN
  SELECT RAISE(ABORT, 'privacy case events are append-only');
END;
CREATE TRIGGER privacy_request_event_no_delete
BEFORE DELETE ON privacy_request_event BEGIN
  SELECT RAISE(ABORT, 'privacy case history cannot be deleted');
END;
CREATE TRIGGER org_recovery_envelope_no_update
BEFORE UPDATE ON org_recovery_envelope BEGIN
  SELECT RAISE(ABORT, 'a recovery envelope is immutable — append a rotated envelope');
END;
CREATE TRIGGER org_recovery_envelope_no_delete
BEFORE DELETE ON org_recovery_envelope BEGIN
  SELECT RAISE(ABORT, 'recovery envelopes remain available for retained backups');
END;
```

> ⚠️ **OPEN — blocks 3.4.1.** Which versioned lawful bases may this deployment use for each customer-data purpose, and what evidence must be retained? Default until answered: `privacy_lawful_basis` has no approved rows, so no consent event or customer PII collection is enabled.
> Owner: `3.4.1`. Source that settles it: Jordanian privacy counsel applying the current PDPL and implementing instructions to the signed controller/processor matrix.

> ⚠️ **OPEN — blocks 3.4.1.** For this deployment, which entity is controller, which is processor, who is a recipient, is a DPO required, and is the Personal Data Processing Register entry required and complete? Default until answered: the schema may migrate, but customer capture, consent collection and customer-PII sync remain disabled.
> Owner: 3.4.1. Source that settles it: the current MoDEE Personal Data Processing Register instructions and dated Jordanian counsel advice for the deployed roles.

> ⚠️ **OPEN — blocks 3.4.3.** Is a loyalty redemption a discount that reduces the taxable base, or consideration settled by a tender, and is any part of the reward funded by a third party? Default until answered: loyalty ships **disabled**, and enabling it requires a recorded funding source and an advisor-approved tax treatment persisted against every ledger event.
> Owner: 3.4.3. Source that settles it: a written ISTD ruling and the merchant's tax advisor for the exact reward funding flow.

---

## 0012 — pricing, promotions, supply  ·  Phase 4, microsteps 4.1.1, 4.2.1, 4.4.1

```sql
CREATE TABLE price_list (
  id         BLOB PRIMARY KEY,
  store_id   BLOB REFERENCES store(id),     -- NULL = org base
  name       TEXT NOT NULL,
  valid_from TEXT, valid_to TEXT,
  priority   INTEGER NOT NULL DEFAULT 0,
  deleted_at TEXT, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE TABLE price (
  id            BLOB PRIMARY KEY,
  price_list_id BLOB NOT NULL REFERENCES price_list(id),
  product_id    BLOB NOT NULL REFERENCES product(id),
  unit_minor    INTEGER NOT NULL,
  deleted_at TEXT, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 0,
  UNIQUE (price_list_id, product_id)
) STRICT;
-- Resolution order: promotion > store price list > base price (C.1).

CREATE TABLE promotion (
  id          BLOB PRIMARY KEY,
  org_id      BLOB NOT NULL REFERENCES org(id),
  code        TEXT NOT NULL,
  deleted_at  TEXT,
  updated_at  TEXT NOT NULL,
  version     INTEGER NOT NULL DEFAULT 0,
  UNIQUE (org_id, code)
) STRICT;

-- Terms are versions, never edits. An inspection-day attribution therefore
-- resolves to the offer actually applied, not the promotion as configured now.
CREATE TABLE promotion_version (
  id                  BLOB PRIMARY KEY,
  promotion_id        BLOB NOT NULL REFERENCES promotion(id),
  version_no          INTEGER NOT NULL CHECK (version_no > 0),
  name_ar             TEXT NOT NULL,
  name_en             TEXT,
  kind                TEXT NOT NULL CHECK (kind IN (
                        'percent_off','amount_off','multibuy','mix_match','basket_threshold')),
  config_json         TEXT NOT NULL,
  eligibility_json    TEXT NOT NULL,
  priority            INTEGER NOT NULL DEFAULT 0,
  requalify_policy    TEXT NOT NULL DEFAULT 'deal_break'
                        CHECK (requalify_policy IN ('deal_break','proportional_share')),
  valid_from          TEXT,
  valid_to            TEXT,
  time_of_day_json    TEXT,
  store_scope         BLOB REFERENCES store(id),
  customer_group      TEXT,
  content_hash        BLOB NOT NULL,
  created_at          TEXT NOT NULL,
  UNIQUE (promotion_id, version_no)
) STRICT;

CREATE TABLE promotion_regulated_exclusion (
  promotion_version_id BLOB NOT NULL REFERENCES promotion_version(id),
  regulated_kind       TEXT NOT NULL CHECK (regulated_kind IN ('tobacco')),
  evidence_hash        BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  PRIMARY KEY (promotion_version_id, regulated_kind)
) STRICT;

CREATE TABLE promotion_publication (
  id                   BLOB PRIMARY KEY,
  promotion_version_id BLOB NOT NULL REFERENCES promotion_version(id),
  copy                 TEXT NOT NULL,
  channel              TEXT NOT NULL,
  artifact_hash        BLOB NOT NULL CHECK (length(artifact_hash) = 32),
  published_at         TEXT NOT NULL
) STRICT;

CREATE TRIGGER promotion_publication_excludes_tobacco
BEFORE INSERT ON promotion_publication
WHEN NOT EXISTS (
  SELECT 1 FROM promotion_regulated_exclusion x
   WHERE x.promotion_version_id = NEW.promotion_version_id
     AND x.regulated_kind = 'tobacco')
BEGIN
  SELECT RAISE(ABORT, 'customer-facing promotions require tested tobacco exclusion');
END;

CREATE TABLE promotion_attribution (
  id                        BLOB PRIMARY KEY,
  sale_line_discount_id     BLOB NOT NULL UNIQUE REFERENCES sale_line_discount(id),
  promotion_version_id      BLOB NOT NULL REFERENCES promotion_version(id),
  eligible_input_snapshot   TEXT NOT NULL,
  amount_minor              INTEGER NOT NULL,
  promised_terms_hash       BLOB NOT NULL,
  applied_at                TEXT NOT NULL
) STRICT;
CREATE INDEX idx_promotion_attribution_version
  ON promotion_attribution(promotion_version_id, applied_at);

CREATE TRIGGER promotion_attribution_matches_discount
BEFORE INSERT ON promotion_attribution
WHEN NOT EXISTS (
  SELECT 1 FROM sale_line_discount d
  JOIN sale_line l ON l.id = d.sale_line_id
  JOIN product p ON p.id = l.product_id
   WHERE d.id = NEW.sale_line_discount_id
     AND d.source = 'promotion'
     AND d.amount_minor = NEW.amount_minor
     AND p.regulated_kind IS NULL
     AND (SELECT status FROM sale WHERE id = l.sale_id) <> 'completed')
BEGIN
  SELECT RAISE(ABORT, 'promotion attribution must match the pre-completion discount and cannot target tobacco');
END;

CREATE TRIGGER completed_sale_requires_promotion_attribution_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND EXISTS (
  SELECT 1 FROM sale_line l JOIN sale_line_discount d ON d.sale_line_id = l.id
   WHERE l.sale_id = NEW.id AND d.source = 'promotion'
     AND NOT EXISTS (
       SELECT 1 FROM promotion_attribution a JOIN fact_commit_member m
         ON m.entity = 'promotion_attribution' AND m.entity_id = a.id
        AND m.commit_id = NEW.sync_commit_id
        WHERE a.sale_line_discount_id = d.id))
BEGIN
  SELECT RAISE(ABORT, 'every promotion discount requires immutable version attribution in the sale commit');
END;

CREATE TRIGGER completed_sale_requires_promotion_attribution_update
BEFORE UPDATE OF status, sync_commit_id ON sale
WHEN NEW.status = 'completed' AND EXISTS (
  SELECT 1 FROM sale_line l JOIN sale_line_discount d ON d.sale_line_id = l.id
   WHERE l.sale_id = NEW.id AND d.source = 'promotion'
     AND NOT EXISTS (
       SELECT 1 FROM promotion_attribution a JOIN fact_commit_member m
         ON m.entity = 'promotion_attribution' AND m.entity_id = a.id
        AND m.commit_id = NEW.sync_commit_id
        WHERE a.sale_line_discount_id = d.id))
BEGIN
  SELECT RAISE(ABORT, 'every promotion discount requires immutable version attribution in the sale commit');
END;

CREATE TABLE supplier (
  id BLOB PRIMARY KEY, org_id BLOB NOT NULL REFERENCES org(id),
  name TEXT NOT NULL, phone TEXT, email TEXT, tin TEXT,
  deleted_at TEXT, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 0
) STRICT;

-- The sales-side tax report cannot prepare a return. Purchase and import tax
-- are separate immutable facts; WAC consumes net plus only the tax classified
-- as nondeductible, never an undefined tax-inclusive `unit_cost_minor`.
CREATE TABLE supplier_invoice (
  id                    BLOB PRIMARY KEY,
  org_id                BLOB NOT NULL REFERENCES org(id),
  store_id              BLOB NOT NULL REFERENCES store(id),
  supplier_id           BLOB REFERENCES supplier(id),
  document_kind         TEXT NOT NULL
                          CHECK (document_kind IN ('domestic_invoice','supplier_credit','import','imported_service')),
  document_number       TEXT NOT NULL,
  document_date         TEXT NOT NULL,
  original_invoice_id   BLOB REFERENCES supplier_invoice(id),
  import_reference      TEXT,
  currency              TEXT NOT NULL,
  net_minor             INTEGER NOT NULL,
  tax_minor             INTEGER NOT NULL,
  gross_minor           INTEGER NOT NULL,
  evidence_hash_algorithm TEXT NOT NULL CHECK (evidence_hash_algorithm IN ('blake3','sha256')),
  evidence_hash         BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  captured_by           BLOB NOT NULL REFERENCES app_user(id),
  captured_at           TEXT NOT NULL,
  CHECK (gross_minor = net_minor + tax_minor),
  UNIQUE (org_id, supplier_id, document_number)
) STRICT;
CREATE INDEX idx_supplier_invoice_period
  ON supplier_invoice(org_id, store_id, document_date);

CREATE TABLE supplier_invoice_line (
  id                    BLOB PRIMARY KEY,
  supplier_invoice_id   BLOB NOT NULL REFERENCES supplier_invoice(id),
  line_no               INTEGER NOT NULL,
  product_id            BLOB REFERENCES product(id),
  description_snapshot  TEXT NOT NULL,
  qty_milli             INTEGER NOT NULL,
  net_minor             INTEGER NOT NULL,
  tax_minor             INTEGER NOT NULL,
  gross_minor           INTEGER NOT NULL,
  deductibility_class   TEXT NOT NULL
                          CHECK (deductibility_class IN (
                            'fully_deductible','partly_deductible','non_deductible','common_input')),
  deductible_ppm        INTEGER NOT NULL CHECK (deductible_ppm BETWEEN 0 AND 1000000),
  input_class           TEXT NOT NULL
                          CHECK (input_class IN (
                            'inventory','expense','asset','import','imported_service',
                            'exempt_purchase','exempt_import')),
  nondeductible_tax_minor INTEGER NOT NULL,
  UNIQUE (supplier_invoice_id, line_no),
  CHECK (gross_minor = net_minor + tax_minor),
  CHECK ((tax_minor >= 0 AND nondeductible_tax_minor BETWEEN 0 AND tax_minor)
      OR (tax_minor < 0 AND nondeductible_tax_minor BETWEEN tax_minor AND 0))
) STRICT;

CREATE TABLE supplier_invoice_line_tax (
  id                    BLOB PRIMARY KEY,
  supplier_invoice_line_id BLOB NOT NULL REFERENCES supplier_invoice_line(id),
  component_code        TEXT NOT NULL,
  treatment             TEXT NOT NULL
                          CHECK (treatment IN ('standard','reduced','zero','exempt')),
  calculation_kind      TEXT NOT NULL
                          CHECK (calculation_kind IN ('ad_valorem','fixed_per_quantity')),
  rate_ppm              INTEGER CHECK (rate_ppm >= 0),
  fixed_amount_minor    INTEGER,
  fixed_currency        TEXT,
  fixed_basis_qty_milli INTEGER,
  calculation_order     INTEGER NOT NULL DEFAULT 0,
  base_kind             TEXT NOT NULL
                          CHECK (base_kind IN ('line_net','line_net_plus_prior_components','quantity')),
  taxable_base_minor    INTEGER,
  taxable_qty_milli     INTEGER,
  tax_minor             INTEGER NOT NULL,
  return_box_code       TEXT,
  CHECK (
    (calculation_kind = 'ad_valorem'
      AND rate_ppm IS NOT NULL
      AND fixed_amount_minor IS NULL
      AND fixed_currency IS NULL
      AND fixed_basis_qty_milli IS NULL
      AND taxable_base_minor IS NOT NULL
      AND taxable_qty_milli IS NULL
      AND base_kind IN ('line_net','line_net_plus_prior_components'))
    OR
    (calculation_kind = 'fixed_per_quantity'
      AND rate_ppm IS NULL
      AND fixed_amount_minor > 0
      AND fixed_currency IS NOT NULL
      AND fixed_basis_qty_milli > 0
      AND taxable_base_minor IS NULL
      AND taxable_qty_milli IS NOT NULL
      AND base_kind = 'quantity')
  )
) STRICT;
CREATE UNIQUE INDEX idx_supplier_line_tax_order
  ON supplier_invoice_line_tax(supplier_invoice_line_id, calculation_order);

CREATE TABLE supplier_invoice_post_event (
  id                  BLOB PRIMARY KEY,
  supplier_invoice_id BLOB NOT NULL UNIQUE REFERENCES supplier_invoice(id),
  sync_commit_id      BLOB NOT NULL REFERENCES sync_commit(id),
  line_count          INTEGER NOT NULL CHECK (line_count > 0),
  content_hash        BLOB NOT NULL CHECK (length(content_hash) = 32),
  posted_by           BLOB NOT NULL REFERENCES app_user(id),
  posted_at           TEXT NOT NULL
) STRICT;

CREATE TRIGGER supplier_invoice_post_is_complete
BEFORE INSERT ON supplier_invoice_post_event
WHEN NOT EXISTS (SELECT 1 FROM sync_commit_ready ready WHERE ready.id = NEW.sync_commit_id)
  OR NOT EXISTS (
       SELECT 1 FROM fact_commit_member m
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'supplier_invoice_post_event' AND m.entity_id = NEW.id)
  OR NOT EXISTS (
       SELECT 1 FROM fact_commit_member m
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'supplier_invoice' AND m.entity_id = NEW.supplier_invoice_id)
  OR NOT EXISTS (
       SELECT 1 FROM supplier_invoice i
        WHERE i.id = NEW.supplier_invoice_id
          AND i.net_minor = (SELECT COALESCE(SUM(l.net_minor), 0)
                               FROM supplier_invoice_line l
                              WHERE l.supplier_invoice_id = i.id)
          AND i.tax_minor = (SELECT COALESCE(SUM(l.tax_minor), 0)
                               FROM supplier_invoice_line l
                              WHERE l.supplier_invoice_id = i.id)
          AND i.gross_minor = (SELECT COALESCE(SUM(l.gross_minor), 0)
                                 FROM supplier_invoice_line l
                                WHERE l.supplier_invoice_id = i.id))
  OR NEW.line_count <> (SELECT COUNT(*) FROM supplier_invoice_line l
                         WHERE l.supplier_invoice_id = NEW.supplier_invoice_id)
  OR EXISTS (
       SELECT 1 FROM supplier_invoice_line l
        WHERE l.supplier_invoice_id = NEW.supplier_invoice_id
          AND (NOT EXISTS (SELECT 1 FROM supplier_invoice_line_tax t
                            WHERE t.supplier_invoice_line_id = l.id)
            OR NOT EXISTS (SELECT 1 FROM fact_commit_member m
                            WHERE m.commit_id = NEW.sync_commit_id
                              AND m.entity = 'supplier_invoice_line' AND m.entity_id = l.id)
            OR EXISTS (SELECT 1 FROM supplier_invoice_line_tax t
                        WHERE t.supplier_invoice_line_id = l.id
                          AND NOT EXISTS (SELECT 1 FROM fact_commit_member m
                                           WHERE m.commit_id = NEW.sync_commit_id
                                             AND m.entity = 'supplier_invoice_line_tax'
                                             AND m.entity_id = t.id))))
BEGIN
  SELECT RAISE(ABORT, 'posted supplier invoice must seal all lines, tax components and header totals');
END;

CREATE TRIGGER supplier_invoice_line_no_insert_after_post
BEFORE INSERT ON supplier_invoice_line
WHEN EXISTS (SELECT 1 FROM supplier_invoice_post_event e
              WHERE e.supplier_invoice_id = NEW.supplier_invoice_id)
BEGIN
  SELECT RAISE(ABORT, 'a posted supplier invoice cannot gain lines');
END;
CREATE TRIGGER supplier_invoice_tax_no_insert_after_post
BEFORE INSERT ON supplier_invoice_line_tax
WHEN EXISTS (
  SELECT 1 FROM supplier_invoice_line l JOIN supplier_invoice_post_event e
    ON e.supplier_invoice_id = l.supplier_invoice_id
   WHERE l.id = NEW.supplier_invoice_line_id)
BEGIN
  SELECT RAISE(ABORT, 'a posted supplier invoice cannot gain tax components');
END;

CREATE TABLE goods_receipt (
  id BLOB PRIMARY KEY, store_id BLOB NOT NULL REFERENCES store(id),
  supplier_id BLOB REFERENCES supplier(id),
  supplier_invoice_id BLOB REFERENCES supplier_invoice(id),
  reference TEXT, received_by BLOB NOT NULL REFERENCES app_user(id),
  received_at TEXT NOT NULL, business_date TEXT NOT NULL
) STRICT;
CREATE TABLE goods_receipt_line (
  id BLOB PRIMARY KEY, receipt_id BLOB NOT NULL REFERENCES goods_receipt(id),
  product_id BLOB NOT NULL REFERENCES product(id),
  qty_milli INTEGER NOT NULL CHECK (qty_milli > 0),
  unit_cost_minor INTEGER NOT NULL CHECK (unit_cost_minor >= 0),
  source_invoice_line_id BLOB REFERENCES supplier_invoice_line(id),
  is_cost_confirmed INTEGER NOT NULL DEFAULT 0 CHECK (is_cost_confirmed IN (0,1)),
  allocated_net_minor INTEGER,
  allocated_nondeductible_tax_minor INTEGER,
  inventory_cost_minor INTEGER,
  CHECK (is_cost_confirmed = 0 OR (
    source_invoice_line_id IS NOT NULL
    AND allocated_net_minor IS NOT NULL
    AND allocated_nondeductible_tax_minor IS NOT NULL
    AND inventory_cost_minor = allocated_net_minor + allocated_nondeductible_tax_minor
    AND inventory_cost_minor >= 0
    AND abs(unit_cost_minor * qty_milli - inventory_cost_minor * 1000) * 2 <= qty_milli))
) STRICT;

CREATE TRIGGER goods_receipt_line_quantity_matches_product_step
BEFORE INSERT ON goods_receipt_line
WHEN NOT EXISTS (
  SELECT 1 FROM product p
   WHERE p.id = NEW.product_id AND NEW.qty_milli % p.qty_step_milli = 0)
BEGIN SELECT RAISE(ABORT, 'received quantity must respect the product milli-unit step'); END;
CREATE TRIGGER goods_receipt_line_quantity_matches_product_step_update
BEFORE UPDATE OF product_id, qty_milli ON goods_receipt_line
WHEN NOT EXISTS (
  SELECT 1 FROM product p
   WHERE p.id = NEW.product_id AND NEW.qty_milli % p.qty_step_milli = 0)
BEGIN SELECT RAISE(ABORT, 'received quantity must respect the product milli-unit step'); END;

ALTER TABLE stock_ledger ADD COLUMN source_goods_receipt_line_id BLOB
  REFERENCES goods_receipt_line(id);
CREATE UNIQUE INDEX idx_stock_goods_receipt_line
  ON stock_ledger(source_goods_receipt_line_id) WHERE kind = 'receive';

CREATE TRIGGER stock_receive_matches_goods_receipt_line
BEFORE INSERT ON stock_ledger
WHEN NEW.kind = 'receive' AND NOT EXISTS (
  SELECT 1 FROM goods_receipt_line l JOIN goods_receipt r ON r.id = l.receipt_id
   WHERE l.id = NEW.source_goods_receipt_line_id
     AND l.is_cost_confirmed = 1
     AND NEW.ref_kind = 'goods_receipt' AND NEW.ref_id = r.id
     AND NEW.product_id = l.product_id AND NEW.store_id = r.store_id
     AND NEW.qty_delta_milli = l.qty_milli
     AND NEW.unit_cost_minor = l.unit_cost_minor
     AND NEW.is_cost_estimated = 0)
BEGIN
  SELECT RAISE(ABORT, 'receive stock fact must match confirmed net-plus-nondeductible receipt cost');
END;

CREATE TRIGGER goods_receipt_confirmed_cost_matches_invoice
BEFORE INSERT ON goods_receipt_line
WHEN NEW.is_cost_confirmed = 1 AND NOT EXISTS (
  SELECT 1 FROM goods_receipt r
  JOIN supplier_invoice_line l ON l.id = NEW.source_invoice_line_id
  JOIN supplier_invoice i ON i.id = l.supplier_invoice_id
  JOIN supplier_invoice_post_event posted ON posted.supplier_invoice_id = i.id
   WHERE r.id = NEW.receipt_id
     AND r.supplier_invoice_id = i.id
     AND i.document_kind IN ('domestic_invoice','import')
     AND l.product_id = NEW.product_id
     AND NEW.allocated_net_minor BETWEEN 0 AND l.net_minor
     AND NEW.allocated_nondeductible_tax_minor BETWEEN 0 AND l.nondeductible_tax_minor
     AND NEW.allocated_net_minor + COALESCE((
           SELECT SUM(prior.allocated_net_minor) FROM goods_receipt_line prior
            WHERE prior.source_invoice_line_id = l.id AND prior.is_cost_confirmed = 1), 0) <= l.net_minor
     AND NEW.allocated_nondeductible_tax_minor + COALESCE((
           SELECT SUM(prior.allocated_nondeductible_tax_minor) FROM goods_receipt_line prior
            WHERE prior.source_invoice_line_id = l.id AND prior.is_cost_confirmed = 1), 0)
         <= l.nondeductible_tax_minor)
BEGIN
  SELECT RAISE(ABORT, 'confirmed WAC cost is invoice net plus only allocated nondeductible tax');
END;

CREATE TABLE goods_receipt_post_event (
  id               BLOB PRIMARY KEY,
  goods_receipt_id BLOB NOT NULL UNIQUE REFERENCES goods_receipt(id),
  sync_commit_id   BLOB NOT NULL REFERENCES sync_commit(id),
  line_count       INTEGER NOT NULL CHECK (line_count > 0),
  content_hash     BLOB NOT NULL CHECK (length(content_hash) = 32),
  posted_by        BLOB NOT NULL REFERENCES app_user(id),
  posted_at        TEXT NOT NULL
) STRICT;

CREATE TRIGGER goods_receipt_post_is_complete
BEFORE INSERT ON goods_receipt_post_event
WHEN NOT EXISTS (SELECT 1 FROM sync_commit_ready ready WHERE ready.id = NEW.sync_commit_id)
  OR NEW.line_count <> (SELECT COUNT(*) FROM goods_receipt_line l
                         WHERE l.receipt_id = NEW.goods_receipt_id)
  OR EXISTS (SELECT 1 FROM goods_receipt_line l
              WHERE l.receipt_id = NEW.goods_receipt_id AND l.is_cost_confirmed <> 1)
  OR NOT EXISTS (
       SELECT 1 FROM fact_commit_member m
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'goods_receipt' AND m.entity_id = NEW.goods_receipt_id)
  OR NOT EXISTS (
       SELECT 1 FROM fact_commit_member m
        WHERE m.commit_id = NEW.sync_commit_id
          AND m.entity = 'goods_receipt_post_event' AND m.entity_id = NEW.id)
  OR EXISTS (
       SELECT 1 FROM goods_receipt_line l
        WHERE l.receipt_id = NEW.goods_receipt_id AND (
          NOT EXISTS (SELECT 1 FROM fact_commit_member m
                       WHERE m.commit_id = NEW.sync_commit_id
                         AND m.entity = 'goods_receipt_line' AND m.entity_id = l.id)
          OR NOT EXISTS (
            SELECT 1 FROM stock_ledger e JOIN fact_commit_member m
              ON m.entity = 'stock_ledger' AND m.entity_id = e.id
             AND m.commit_id = NEW.sync_commit_id
             WHERE e.kind = 'receive' AND e.source_goods_receipt_line_id = l.id)))
BEGIN
  SELECT RAISE(ABORT, 'posting seals every confirmed cost and its stock fact in one commit');
END;
CREATE TRIGGER goods_receipt_line_no_insert_after_post
BEFORE INSERT ON goods_receipt_line
WHEN EXISTS (SELECT 1 FROM goods_receipt_post_event e
              WHERE e.goods_receipt_id = NEW.receipt_id)
BEGIN
  SELECT RAISE(ABORT, 'a posted goods receipt cannot gain lines');
END;
CREATE TRIGGER goods_receipt_no_update_after_post
BEFORE UPDATE ON goods_receipt
WHEN EXISTS (SELECT 1 FROM goods_receipt_post_event e
              WHERE e.goods_receipt_id = OLD.id)
BEGIN
  SELECT RAISE(ABORT, 'a posted goods receipt header is immutable');
END;
CREATE TRIGGER goods_receipt_no_delete_after_post
BEFORE DELETE ON goods_receipt
WHEN EXISTS (SELECT 1 FROM goods_receipt_post_event e
              WHERE e.goods_receipt_id = OLD.id)
BEGIN
  SELECT RAISE(ABORT, 'a posted goods receipt cannot be deleted');
END;
CREATE TRIGGER goods_receipt_line_no_update_after_post
BEFORE UPDATE ON goods_receipt_line
-- BOTH parents are checked. Guarding only OLD stops an edit to a posted receipt
-- and still lets a draft line be reparented INTO one, which adds cost evidence to
-- a document already posted to the stock ledger and the supplier's account.
WHEN EXISTS (SELECT 1 FROM goods_receipt_post_event e
              WHERE e.goods_receipt_id = OLD.receipt_id)
   OR EXISTS (SELECT 1 FROM goods_receipt_post_event e
              WHERE e.goods_receipt_id = NEW.receipt_id)
BEGIN
  SELECT RAISE(ABORT, 'posted receipt cost evidence is immutable');
END;
CREATE TRIGGER goods_receipt_line_no_delete_after_post
BEFORE DELETE ON goods_receipt_line
WHEN EXISTS (SELECT 1 FROM goods_receipt_post_event e
              WHERE e.goods_receipt_id = OLD.receipt_id)
BEGIN
  SELECT RAISE(ABORT, 'posted receipt cost evidence cannot be deleted');
END;
CREATE TRIGGER goods_receipt_post_event_no_update
BEFORE UPDATE ON goods_receipt_post_event BEGIN
  SELECT RAISE(ABORT, 'goods receipt posting is an immutable transition fact');
END;
CREATE TRIGGER goods_receipt_post_event_no_delete
BEFORE DELETE ON goods_receipt_post_event BEGIN
  SELECT RAISE(ABORT, 'goods receipt posting cannot be deleted');
END;

CREATE TABLE tax_filing_profile (
  id                    BLOB PRIMARY KEY,
  store_id              BLOB NOT NULL REFERENCES store(id),
  taxpayer_number       TEXT NOT NULL,
  return_type           TEXT NOT NULL,
  cycle_code            TEXT NOT NULL,
  jurisdiction_code     TEXT NOT NULL,
  source_version        TEXT NOT NULL,
  effective_from        TEXT NOT NULL,
  effective_to          TEXT
) STRICT;

CREATE TABLE tax_filing_period (
  id                    BLOB PRIMARY KEY,
  filing_profile_id     BLOB NOT NULL REFERENCES tax_filing_profile(id),
  period_start_date     TEXT NOT NULL,
  period_end_date       TEXT NOT NULL,
  due_date              TEXT NOT NULL,
  UNIQUE (filing_profile_id, period_start_date, period_end_date)
) STRICT;

CREATE TRIGGER tax_filing_profile_no_update
BEFORE UPDATE ON tax_filing_profile BEGIN
  SELECT RAISE(ABORT, 'an assigned filing calendar is immutable — create a new effective version');
END;
CREATE TRIGGER tax_filing_profile_no_delete
BEFORE DELETE ON tax_filing_profile BEGIN
  SELECT RAISE(ABORT, 'filing-calendar evidence cannot be deleted');
END;
CREATE TABLE tax_filing_event (
  id                    BLOB PRIMARY KEY,
  filing_period_id      BLOB NOT NULL REFERENCES tax_filing_period(id),
  event_no              INTEGER NOT NULL CHECK (event_no > 0),
  action                TEXT NOT NULL
                          CHECK (action IN ('opened','prepared','filed','nil_filed','amended')),
  return_reference      TEXT,
  evidence_hash         BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  actor_id              BLOB NOT NULL REFERENCES app_user(id),
  occurred_at           TEXT NOT NULL,
  UNIQUE (filing_period_id, event_no),
  CHECK (action IN ('opened','prepared') OR return_reference IS NOT NULL)
) STRICT;

CREATE VIEW tax_filing_current AS
SELECT filing_period_id, action, return_reference, occurred_at
FROM (
  SELECT e.*,
         ROW_NUMBER() OVER (PARTITION BY filing_period_id ORDER BY event_no DESC) AS rank_no
    FROM tax_filing_event e
) WHERE rank_no = 1;

CREATE TRIGGER tax_filing_event_is_next
BEFORE INSERT ON tax_filing_event
WHEN NEW.event_no <> COALESCE((
  SELECT MAX(event_no) + 1 FROM tax_filing_event WHERE filing_period_id = NEW.filing_period_id), 1)
BEGIN
  SELECT RAISE(ABORT, 'tax filing events must be contiguous');
END;
CREATE TRIGGER tax_filing_event_transition_allowed
BEFORE INSERT ON tax_filing_event
WHEN (NEW.event_no = 1 AND NEW.action <> 'opened')
  OR (NEW.event_no > 1 AND NOT EXISTS (
       SELECT 1 FROM tax_filing_event prior
        WHERE prior.filing_period_id = NEW.filing_period_id
          AND prior.event_no = NEW.event_no - 1
          AND ((prior.action = 'opened' AND NEW.action IN ('prepared','nil_filed'))
            OR (prior.action = 'prepared' AND NEW.action IN ('filed','nil_filed'))
            OR (prior.action IN ('filed','nil_filed','amended') AND NEW.action = 'amended'))))
BEGIN
  SELECT RAISE(ABORT, 'invalid filing-status transition');
END;

CREATE TABLE tax_period_adjustment (
  id                    BLOB PRIMARY KEY,
  org_id                BLOB NOT NULL REFERENCES org(id),
  store_id              BLOB NOT NULL REFERENCES store(id),
  filing_period_id      BLOB NOT NULL REFERENCES tax_filing_period(id),
  adjustment_code       TEXT NOT NULL,
  net_delta_minor       INTEGER NOT NULL,
  tax_delta_minor       INTEGER NOT NULL,
  source_ref            TEXT NOT NULL,
  evidence_hash         BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  policy_version        TEXT NOT NULL,
  recorded_at           TEXT NOT NULL
) STRICT;

CREATE TABLE common_input_allocation (
  id                    BLOB PRIMARY KEY,
  org_id                BLOB NOT NULL REFERENCES org(id),
  filing_period_id      BLOB NOT NULL REFERENCES tax_filing_period(id),
  allocation_method_code TEXT NOT NULL,
  numerator_minor       INTEGER NOT NULL CHECK (numerator_minor >= 0),
  denominator_minor     INTEGER NOT NULL CHECK (denominator_minor > 0),
  deductible_ppm        INTEGER NOT NULL CHECK (deductible_ppm BETWEEN 0 AND 1000000),
  source_ref            TEXT NOT NULL,
  evidence_hash         BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  policy_version        TEXT NOT NULL,
  calculated_at         TEXT NOT NULL,
  CHECK (numerator_minor <= denominator_minor)
) STRICT;

CREATE TABLE tax_credit_ledger (
  id                    BLOB PRIMARY KEY,
  org_id                BLOB NOT NULL REFERENCES org(id),
  filing_period_id      BLOB NOT NULL REFERENCES tax_filing_period(id),
  amount_delta_minor    INTEGER NOT NULL,
  kind                  TEXT NOT NULL
                          CHECK (kind IN ('opening_credit','generated','applied','refunded','adjustment')),
  source_ref            TEXT NOT NULL,
  evidence_hash         BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  occurred_at           TEXT NOT NULL
) STRICT;

CREATE TABLE tax_filing_election (
  id                    BLOB PRIMARY KEY,
  org_id                BLOB NOT NULL REFERENCES org(id),
  filing_period_id      BLOB NOT NULL REFERENCES tax_filing_period(id),
  election_code         TEXT NOT NULL,
  amount_minor          INTEGER NOT NULL CHECK (amount_minor >= 0),
  source_ref            TEXT NOT NULL,
  evidence_hash         BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  elected_by            BLOB NOT NULL REFERENCES app_user(id),
  elected_at            TEXT NOT NULL
) STRICT;

CREATE TABLE credit_note_period_assignment (
  refund_sale_id          BLOB PRIMARY KEY REFERENCES sale(id),
  original_period_id      BLOB NOT NULL REFERENCES tax_filing_period(id),
  credit_note_period_id   BLOB NOT NULL REFERENCES tax_filing_period(id),
  return_box_code         TEXT NOT NULL,
  policy_version          TEXT NOT NULL,
  assigned_at             TEXT NOT NULL
) STRICT;

CREATE TABLE stock_count (
  id BLOB PRIMARY KEY, store_id BLOB NOT NULL REFERENCES store(id),
  started_at TEXT NOT NULL, started_by BLOB NOT NULL REFERENCES app_user(id),
  scope TEXT NOT NULL DEFAULT 'full' CHECK (scope IN ('full','category','partial'))
) STRICT;
CREATE TABLE stock_count_line (
  id BLOB PRIMARY KEY, count_id BLOB NOT NULL REFERENCES stock_count(id),
  product_id BLOB NOT NULL REFERENCES product(id),
  expected_milli INTEGER NOT NULL,      -- snapshot at count START; sales mid-count are fine (E.42)
  counted_milli INTEGER,
  variance_milli INTEGER
) STRICT;

CREATE TRIGGER stock_count_line_quantity_matches_product_step
BEFORE INSERT ON stock_count_line
WHEN NOT EXISTS (
  SELECT 1 FROM product p WHERE p.id = NEW.product_id
    AND NEW.expected_milli % p.qty_step_milli = 0
    AND (NEW.counted_milli IS NULL OR NEW.counted_milli % p.qty_step_milli = 0)
    AND (NEW.variance_milli IS NULL OR NEW.variance_milli % p.qty_step_milli = 0))
BEGIN SELECT RAISE(ABORT, 'stock-count quantities must respect the product milli-unit step'); END;
CREATE TRIGGER stock_count_line_quantity_matches_product_step_update
BEFORE UPDATE OF product_id, expected_milli, counted_milli, variance_milli ON stock_count_line
WHEN NOT EXISTS (
  SELECT 1 FROM product p WHERE p.id = NEW.product_id
    AND NEW.expected_milli % p.qty_step_milli = 0
    AND (NEW.counted_milli IS NULL OR NEW.counted_milli % p.qty_step_milli = 0)
    AND (NEW.variance_milli IS NULL OR NEW.variance_milli % p.qty_step_milli = 0))
BEGIN SELECT RAISE(ABORT, 'stock-count quantities must respect the product milli-unit step'); END;

CREATE TABLE stock_count_post_event (
  id             BLOB PRIMARY KEY,
  stock_count_id BLOB NOT NULL UNIQUE REFERENCES stock_count(id),
  sync_commit_id BLOB NOT NULL REFERENCES sync_commit(id),
  line_count     INTEGER NOT NULL CHECK (line_count > 0),
  content_hash   BLOB NOT NULL CHECK (length(content_hash) = 32),
  posted_by      BLOB NOT NULL REFERENCES app_user(id),
  posted_at      TEXT NOT NULL
) STRICT;
ALTER TABLE stock_ledger ADD COLUMN source_stock_count_line_id BLOB
  REFERENCES stock_count_line(id);
CREATE UNIQUE INDEX idx_stock_count_line_event
  ON stock_ledger(source_stock_count_line_id) WHERE kind = 'count_correction';
CREATE TRIGGER stock_count_event_matches_line
BEFORE INSERT ON stock_ledger
WHEN NEW.kind = 'count_correction' AND NOT EXISTS (
  SELECT 1 FROM stock_count_line l JOIN stock_count c ON c.id = l.count_id
   WHERE l.id = NEW.source_stock_count_line_id
     AND l.counted_milli IS NOT NULL
     AND l.variance_milli = l.counted_milli - l.expected_milli
     AND NEW.ref_kind = 'stock_count' AND NEW.ref_id = c.id
     AND NEW.product_id = l.product_id AND NEW.store_id = c.store_id
     AND NEW.qty_delta_milli = l.variance_milli)
BEGIN SELECT RAISE(ABORT, 'count correction must equal the sealed physical variance'); END;
CREATE TRIGGER stock_count_post_is_complete
BEFORE INSERT ON stock_count_post_event
WHEN NOT EXISTS (SELECT 1 FROM sync_commit_ready ready WHERE ready.id = NEW.sync_commit_id)
  OR NEW.line_count <> (SELECT COUNT(*) FROM stock_count_line l
                         WHERE l.count_id = NEW.stock_count_id)
  OR EXISTS (SELECT 1 FROM stock_count_line l
              WHERE l.count_id = NEW.stock_count_id
                AND (l.counted_milli IS NULL
                  OR l.variance_milli <> l.counted_milli - l.expected_milli))
  OR NOT EXISTS (SELECT 1 FROM fact_commit_member m
                  WHERE m.commit_id = NEW.sync_commit_id
                    AND m.entity = 'stock_count' AND m.entity_id = NEW.stock_count_id)
  OR NOT EXISTS (SELECT 1 FROM fact_commit_member m
                  WHERE m.commit_id = NEW.sync_commit_id
                    AND m.entity = 'stock_count_post_event' AND m.entity_id = NEW.id)
  OR EXISTS (
       SELECT 1 FROM stock_count_line l WHERE l.count_id = NEW.stock_count_id AND (
         NOT EXISTS (SELECT 1 FROM fact_commit_member m
                      WHERE m.commit_id = NEW.sync_commit_id
                        AND m.entity = 'stock_count_line' AND m.entity_id = l.id)
         OR (l.variance_milli <> 0 AND NOT EXISTS (
           SELECT 1 FROM stock_ledger e JOIN fact_commit_member m
             ON m.entity = 'stock_ledger' AND m.entity_id = e.id
            AND m.commit_id = NEW.sync_commit_id
            WHERE e.kind = 'count_correction' AND e.source_stock_count_line_id = l.id))
         OR (l.variance_milli = 0 AND EXISTS (
           SELECT 1 FROM stock_ledger e WHERE e.source_stock_count_line_id = l.id))))
BEGIN
  SELECT RAISE(ABORT, 'posting a stock count seals every counted variance');
END;
CREATE TRIGGER stock_count_line_no_insert_after_post
BEFORE INSERT ON stock_count_line
WHEN EXISTS (SELECT 1 FROM stock_count_post_event e WHERE e.stock_count_id = NEW.count_id)
BEGIN SELECT RAISE(ABORT, 'a posted stock count cannot gain lines'); END;
CREATE TRIGGER stock_count_no_update_after_post
BEFORE UPDATE ON stock_count
WHEN EXISTS (SELECT 1 FROM stock_count_post_event e WHERE e.stock_count_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'a posted stock-count header is immutable'); END;
CREATE TRIGGER stock_count_no_delete_after_post
BEFORE DELETE ON stock_count
WHEN EXISTS (SELECT 1 FROM stock_count_post_event e WHERE e.stock_count_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'a posted stock count cannot be deleted'); END;
CREATE TRIGGER stock_count_line_no_update_after_post
BEFORE UPDATE ON stock_count_line
-- BOTH parents, for the reason given on goods_receipt_line: reparenting a draft
-- line into a posted count rewrites the variance the correction was derived from.
WHEN EXISTS (SELECT 1 FROM stock_count_post_event e WHERE e.stock_count_id = OLD.count_id)
   OR EXISTS (SELECT 1 FROM stock_count_post_event e WHERE e.stock_count_id = NEW.count_id)
BEGIN SELECT RAISE(ABORT, 'posted stock-count evidence is immutable'); END;
CREATE TRIGGER stock_count_line_no_delete_after_post
BEFORE DELETE ON stock_count_line
WHEN EXISTS (SELECT 1 FROM stock_count_post_event e WHERE e.stock_count_id = OLD.count_id)
BEGIN SELECT RAISE(ABORT, 'posted stock-count evidence cannot be deleted'); END;
CREATE TRIGGER stock_count_post_event_no_update
BEFORE UPDATE ON stock_count_post_event
BEGIN SELECT RAISE(ABORT, 'stock-count posting is an immutable transition fact'); END;
CREATE TRIGGER stock_count_post_event_no_delete
BEFORE DELETE ON stock_count_post_event
BEGIN SELECT RAISE(ABORT, 'stock-count posting cannot be deleted'); END;

CREATE TABLE transfer (
  id BLOB PRIMARY KEY,
  from_store BLOB NOT NULL REFERENCES store(id),
  to_store   BLOB NOT NULL REFERENCES store(id),
  created_by BLOB NOT NULL REFERENCES app_user(id),
  created_at TEXT NOT NULL,
  CHECK (from_store <> to_store)
) STRICT;
CREATE TABLE transfer_line (
  id BLOB PRIMARY KEY, transfer_id BLOB NOT NULL REFERENCES transfer(id),
  product_id BLOB NOT NULL REFERENCES product(id),
  qty_sent_milli INTEGER NOT NULL CHECK (qty_sent_milli > 0)
) STRICT;

CREATE TRIGGER transfer_line_quantity_matches_product_step
BEFORE INSERT ON transfer_line
WHEN NOT EXISTS (
  SELECT 1 FROM product p
   WHERE p.id = NEW.product_id AND NEW.qty_sent_milli % p.qty_step_milli = 0)
BEGIN SELECT RAISE(ABORT, 'transfer quantity must respect the product milli-unit step'); END;
CREATE TRIGGER transfer_line_quantity_matches_product_step_update
BEFORE UPDATE OF product_id, qty_sent_milli ON transfer_line
WHEN NOT EXISTS (
  SELECT 1 FROM product p
   WHERE p.id = NEW.product_id AND NEW.qty_sent_milli % p.qty_step_milli = 0)
BEGIN SELECT RAISE(ABORT, 'transfer quantity must respect the product milli-unit step'); END;

CREATE TABLE transfer_ship_event (
  id          BLOB PRIMARY KEY,
  transfer_id BLOB NOT NULL UNIQUE REFERENCES transfer(id),
  sync_commit_id BLOB NOT NULL REFERENCES sync_commit(id),
  line_count  INTEGER NOT NULL CHECK (line_count > 0),
  content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),
  sent_by     BLOB NOT NULL REFERENCES app_user(id),
  sent_at     TEXT NOT NULL
) STRICT;
CREATE TABLE transfer_receipt_line (
  id                 BLOB PRIMARY KEY,
  transfer_line_id   BLOB NOT NULL UNIQUE REFERENCES transfer_line(id),
  qty_received_milli INTEGER NOT NULL CHECK (qty_received_milli >= 0),
  qty_damaged_milli  INTEGER NOT NULL DEFAULT 0 CHECK (qty_damaged_milli >= 0),
  reason_code        TEXT
) STRICT;
CREATE TRIGGER transfer_receipt_quantity_matches_product_step
BEFORE INSERT ON transfer_receipt_line
WHEN NOT EXISTS (
  SELECT 1 FROM transfer_line l JOIN product p ON p.id = l.product_id
   WHERE l.id = NEW.transfer_line_id
     AND NEW.qty_received_milli % p.qty_step_milli = 0
     AND NEW.qty_damaged_milli % p.qty_step_milli = 0)
BEGIN SELECT RAISE(ABORT, 'transfer receipt quantities must respect the product milli-unit step'); END;
CREATE TABLE transfer_receive_event (
  id          BLOB PRIMARY KEY,
  transfer_id BLOB NOT NULL UNIQUE REFERENCES transfer(id),
  sync_commit_id BLOB NOT NULL REFERENCES sync_commit(id),
  line_count  INTEGER NOT NULL CHECK (line_count > 0),
  content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),
  received_by BLOB NOT NULL REFERENCES app_user(id),
  received_at TEXT NOT NULL
) STRICT;
CREATE TABLE transfer_cancel_event (
  id          BLOB PRIMARY KEY,
  transfer_id BLOB NOT NULL UNIQUE REFERENCES transfer(id),
  sync_commit_id BLOB NOT NULL REFERENCES sync_commit(id),
  reason      TEXT NOT NULL,
  cancelled_by BLOB NOT NULL REFERENCES app_user(id),
  cancelled_at TEXT NOT NULL
) STRICT;
CREATE VIEW transfer_current AS
SELECT t.id AS transfer_id,
       CASE WHEN received.id IS NOT NULL THEN 'received'
            WHEN cancelled.id IS NOT NULL THEN 'cancelled'
            WHEN shipped.id IS NOT NULL THEN 'in_transit'
            ELSE 'draft' END AS state,
       shipped.sent_at, received.received_at
  FROM transfer t
  LEFT JOIN transfer_ship_event shipped ON shipped.transfer_id = t.id
  LEFT JOIN transfer_receive_event received ON received.transfer_id = t.id
  LEFT JOIN transfer_cancel_event cancelled ON cancelled.transfer_id = t.id;

ALTER TABLE stock_ledger ADD COLUMN source_transfer_line_id BLOB
  REFERENCES transfer_line(id);
ALTER TABLE stock_ledger ADD COLUMN source_transfer_receipt_line_id BLOB
  REFERENCES transfer_receipt_line(id);
CREATE UNIQUE INDEX idx_stock_transfer_out_line
  ON stock_ledger(source_transfer_line_id) WHERE kind = 'transfer_out';
CREATE UNIQUE INDEX idx_stock_transfer_in_line
  ON stock_ledger(source_transfer_receipt_line_id) WHERE kind = 'transfer_in';
CREATE TRIGGER stock_transfer_out_matches_line
BEFORE INSERT ON stock_ledger
WHEN NEW.kind = 'transfer_out' AND NOT EXISTS (
  SELECT 1 FROM transfer_line l JOIN transfer t ON t.id = l.transfer_id
   WHERE l.id = NEW.source_transfer_line_id
     AND NEW.ref_kind = 'transfer' AND NEW.ref_id = t.id
     AND NEW.product_id = l.product_id AND NEW.store_id = t.from_store
     AND NEW.qty_delta_milli = -l.qty_sent_milli)
BEGIN SELECT RAISE(ABORT, 'transfer-out stock fact must equal the shipped line'); END;
CREATE TRIGGER stock_transfer_in_matches_receipt
BEFORE INSERT ON stock_ledger
-- Preserve nullable cost and derived-weight provenance exactly across stores;
-- `IS` is the null-safe comparison for the unknown-cost case.
WHEN NEW.kind = 'transfer_in' AND NOT EXISTS (
  SELECT 1 FROM transfer_receipt_line receipt
  JOIN transfer_line l ON l.id = receipt.transfer_line_id
  JOIN transfer t ON t.id = l.transfer_id
  JOIN stock_ledger source
    ON source.kind = 'transfer_out' AND source.source_transfer_line_id = l.id
   WHERE receipt.id = NEW.source_transfer_receipt_line_id
     AND receipt.qty_received_milli > 0
     AND NEW.ref_kind = 'transfer' AND NEW.ref_id = t.id
     AND NEW.product_id = l.product_id AND NEW.store_id = t.to_store
     AND NEW.qty_delta_milli = receipt.qty_received_milli
     AND NEW.unit_cost_minor IS source.unit_cost_minor
     AND NEW.is_cost_estimated = source.is_cost_estimated
     AND NEW.is_weight_derived = source.is_weight_derived)
BEGIN SELECT RAISE(ABORT, 'transfer-in stock fact must equal the sealed destination receipt'); END;

CREATE TRIGGER transfer_ship_is_complete
BEFORE INSERT ON transfer_ship_event
WHEN NOT EXISTS (SELECT 1 FROM sync_commit_ready ready WHERE ready.id = NEW.sync_commit_id)
  OR EXISTS (SELECT 1 FROM transfer_cancel_event c WHERE c.transfer_id = NEW.transfer_id)
  OR NEW.line_count <> (SELECT COUNT(*) FROM transfer_line l WHERE l.transfer_id = NEW.transfer_id)
  OR NOT EXISTS (SELECT 1 FROM fact_commit_member m
                  WHERE m.commit_id = NEW.sync_commit_id
                    AND m.entity = 'transfer' AND m.entity_id = NEW.transfer_id)
  OR NOT EXISTS (SELECT 1 FROM fact_commit_member m
                  WHERE m.commit_id = NEW.sync_commit_id
                    AND m.entity = 'transfer_ship_event' AND m.entity_id = NEW.id)
  OR EXISTS (
       SELECT 1 FROM transfer_line l WHERE l.transfer_id = NEW.transfer_id AND (
         NOT EXISTS (SELECT 1 FROM fact_commit_member m
                      WHERE m.commit_id = NEW.sync_commit_id
                        AND m.entity = 'transfer_line' AND m.entity_id = l.id)
         OR NOT EXISTS (
           SELECT 1 FROM stock_ledger e JOIN fact_commit_member m
             ON m.entity = 'stock_ledger' AND m.entity_id = e.id
            AND m.commit_id = NEW.sync_commit_id
            WHERE e.kind = 'transfer_out' AND e.source_transfer_line_id = l.id)))
BEGIN SELECT RAISE(ABORT, 'shipping seals every transfer line and cannot follow cancellation'); END;
CREATE TRIGGER transfer_receive_is_complete
BEFORE INSERT ON transfer_receive_event
WHEN NOT EXISTS (SELECT 1 FROM sync_commit_ready ready WHERE ready.id = NEW.sync_commit_id)
  OR NOT EXISTS (SELECT 1 FROM transfer_ship_event s WHERE s.transfer_id = NEW.transfer_id)
  OR EXISTS (SELECT 1 FROM transfer_cancel_event c WHERE c.transfer_id = NEW.transfer_id)
  OR NEW.line_count <> (SELECT COUNT(*) FROM transfer_line l WHERE l.transfer_id = NEW.transfer_id)
  OR EXISTS (
       SELECT 1 FROM transfer_line l WHERE l.transfer_id = NEW.transfer_id
         AND NOT EXISTS (SELECT 1 FROM transfer_receipt_line r WHERE r.transfer_line_id = l.id))
  OR NOT EXISTS (SELECT 1 FROM fact_commit_member m
                  WHERE m.commit_id = NEW.sync_commit_id
                    AND m.entity = 'transfer_receive_event' AND m.entity_id = NEW.id)
  OR EXISTS (
       SELECT 1 FROM transfer_receipt_line receipt
       JOIN transfer_line l ON l.id = receipt.transfer_line_id
        WHERE l.transfer_id = NEW.transfer_id AND (
          NOT EXISTS (SELECT 1 FROM fact_commit_member m
                       WHERE m.commit_id = NEW.sync_commit_id
                         AND m.entity = 'transfer_receipt_line' AND m.entity_id = receipt.id)
          OR (receipt.qty_received_milli > 0 AND NOT EXISTS (
            SELECT 1 FROM stock_ledger e JOIN fact_commit_member m
              ON m.entity = 'stock_ledger' AND m.entity_id = e.id
             AND m.commit_id = NEW.sync_commit_id
             WHERE e.kind = 'transfer_in'
               AND e.source_transfer_receipt_line_id = receipt.id))
          OR (receipt.qty_received_milli = 0 AND EXISTS (
            SELECT 1 FROM stock_ledger e
             WHERE e.source_transfer_receipt_line_id = receipt.id))))
BEGIN SELECT RAISE(ABORT, 'receiving seals one destination result for every shipped line'); END;
CREATE TRIGGER transfer_cancel_before_ship
BEFORE INSERT ON transfer_cancel_event
WHEN NOT EXISTS (SELECT 1 FROM sync_commit_ready ready WHERE ready.id = NEW.sync_commit_id)
  OR EXISTS (SELECT 1 FROM transfer_ship_event s WHERE s.transfer_id = NEW.transfer_id)
  OR NOT EXISTS (SELECT 1 FROM fact_commit_member m
                  WHERE m.commit_id = NEW.sync_commit_id
                    AND m.entity = 'transfer' AND m.entity_id = NEW.transfer_id)
  OR NOT EXISTS (SELECT 1 FROM fact_commit_member m
                  WHERE m.commit_id = NEW.sync_commit_id
                    AND m.entity = 'transfer_cancel_event' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'an in-transit transfer is corrected by receipt facts, not cancellation'); END;
CREATE TRIGGER transfer_line_no_insert_after_ship
BEFORE INSERT ON transfer_line
WHEN EXISTS (SELECT 1 FROM transfer_ship_event s WHERE s.transfer_id = NEW.transfer_id)
  OR EXISTS (SELECT 1 FROM transfer_cancel_event c WHERE c.transfer_id = NEW.transfer_id)
BEGIN SELECT RAISE(ABORT, 'shipped or cancelled transfer lines are sealed'); END;
CREATE TRIGGER transfer_no_update_after_transition
BEFORE UPDATE ON transfer
WHEN EXISTS (SELECT 1 FROM transfer_ship_event s WHERE s.transfer_id = OLD.id)
  OR EXISTS (SELECT 1 FROM transfer_cancel_event c WHERE c.transfer_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'a shipped or cancelled transfer header is immutable'); END;
CREATE TRIGGER transfer_no_delete_after_transition
BEFORE DELETE ON transfer
WHEN EXISTS (SELECT 1 FROM transfer_ship_event s WHERE s.transfer_id = OLD.id)
  OR EXISTS (SELECT 1 FROM transfer_cancel_event c WHERE c.transfer_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'a shipped or cancelled transfer cannot be deleted'); END;
CREATE TRIGGER transfer_line_no_update_after_ship
BEFORE UPDATE ON transfer_line
-- BOTH parents, for the reason given on goods_receipt_line: a line moved into a
-- shipped transfer changes what the destination is owed after the stock left.
WHEN EXISTS (SELECT 1 FROM transfer_ship_event s WHERE s.transfer_id = OLD.transfer_id)
  OR EXISTS (SELECT 1 FROM transfer_cancel_event c WHERE c.transfer_id = OLD.transfer_id)
  OR EXISTS (SELECT 1 FROM transfer_ship_event s WHERE s.transfer_id = NEW.transfer_id)
  OR EXISTS (SELECT 1 FROM transfer_cancel_event c WHERE c.transfer_id = NEW.transfer_id)
BEGIN SELECT RAISE(ABORT, 'shipped or cancelled transfer lines are immutable'); END;
CREATE TRIGGER transfer_line_no_delete_after_ship
BEFORE DELETE ON transfer_line
WHEN EXISTS (SELECT 1 FROM transfer_ship_event s WHERE s.transfer_id = OLD.transfer_id)
  OR EXISTS (SELECT 1 FROM transfer_cancel_event c WHERE c.transfer_id = OLD.transfer_id)
BEGIN SELECT RAISE(ABORT, 'shipped or cancelled transfer lines cannot be deleted'); END;
CREATE TRIGGER transfer_receipt_line_after_ship
BEFORE INSERT ON transfer_receipt_line
WHEN NOT EXISTS (
  SELECT 1 FROM transfer_line l JOIN transfer_ship_event s ON s.transfer_id = l.transfer_id
   WHERE l.id = NEW.transfer_line_id
     AND NEW.qty_received_milli + NEW.qty_damaged_milli <= l.qty_sent_milli)
BEGIN SELECT RAISE(ABORT, 'destination quantities exist only for a shipped transfer'); END;
CREATE TRIGGER transfer_receipt_line_no_update
BEFORE UPDATE ON transfer_receipt_line
BEGIN SELECT RAISE(ABORT, 'a transfer receipt line is immutable'); END;
CREATE TRIGGER transfer_receipt_line_no_delete
BEFORE DELETE ON transfer_receipt_line
BEGIN SELECT RAISE(ABORT, 'a transfer receipt line cannot be deleted'); END;
CREATE TRIGGER transfer_ship_event_no_update
BEFORE UPDATE ON transfer_ship_event BEGIN SELECT RAISE(ABORT, 'ship transition is immutable'); END;
CREATE TRIGGER transfer_ship_event_no_delete
BEFORE DELETE ON transfer_ship_event BEGIN SELECT RAISE(ABORT, 'ship transition cannot be deleted'); END;
CREATE TRIGGER transfer_receive_event_no_update
BEFORE UPDATE ON transfer_receive_event BEGIN SELECT RAISE(ABORT, 'receive transition is immutable'); END;
CREATE TRIGGER transfer_receive_event_no_delete
BEFORE DELETE ON transfer_receive_event BEGIN SELECT RAISE(ABORT, 'receive transition cannot be deleted'); END;
CREATE TRIGGER transfer_cancel_event_no_update
BEFORE UPDATE ON transfer_cancel_event BEGIN SELECT RAISE(ABORT, 'cancel transition is immutable'); END;
CREATE TRIGGER transfer_cancel_event_no_delete
BEFORE DELETE ON transfer_cancel_event BEGIN SELECT RAISE(ABORT, 'cancel transition cannot be deleted'); END;

-- Price display is actively enforced in Jordan (J.3), so a price change
-- produces a labels-to-reprint worklist.
CREATE TABLE regulated_display_approval (
  id                     BLOB PRIMARY KEY,
  product_id             BLOB NOT NULL REFERENCES product(id),
  policy_version         TEXT NOT NULL,
  evidence_ref           TEXT NOT NULL,
  evidence_hash_algorithm TEXT NOT NULL CHECK (evidence_hash_algorithm IN ('blake3','sha256')),
  evidence_hash          BLOB NOT NULL CHECK (length(evidence_hash) = 32),
  approved_by            BLOB NOT NULL REFERENCES app_user(id),
  approved_at            TEXT NOT NULL,
  UNIQUE (product_id, policy_version)
) STRICT;

CREATE TABLE label_reprint_queue (
  id BLOB PRIMARY KEY,
  product_id BLOB NOT NULL REFERENCES product(id),
  store_id BLOB NOT NULL REFERENCES store(id),
  cause TEXT NOT NULL CHECK (cause IN ('price_change','new_product','displayed_price_override')),
  queued_at TEXT NOT NULL, printed_at TEXT
) STRICT;

CREATE TRIGGER tobacco_label_requires_display_approval
BEFORE INSERT ON label_reprint_queue
WHEN EXISTS (SELECT 1 FROM product p WHERE p.id = NEW.product_id
              AND p.regulated_kind = 'tobacco')
 AND NOT EXISTS (SELECT 1 FROM regulated_display_approval a
                  WHERE a.product_id = NEW.product_id)
BEGIN
  SELECT RAISE(ABORT, 'tobacco customer-facing labels remain blocked until display rules are approved');
END;
CREATE TRIGGER regulated_display_approval_no_update
BEFORE UPDATE ON regulated_display_approval
BEGIN SELECT RAISE(ABORT, 'regulated display approval evidence is immutable'); END;
CREATE TRIGGER regulated_display_approval_no_delete
BEFORE DELETE ON regulated_display_approval
BEGIN SELECT RAISE(ABORT, 'regulated display approval evidence cannot be deleted'); END;

CREATE TRIGGER promotion_version_no_update
BEFORE UPDATE ON promotion_version BEGIN
  SELECT RAISE(ABORT, 'published promotion terms are immutable — create a new version');
END;
CREATE TRIGGER promotion_version_no_delete
BEFORE DELETE ON promotion_version BEGIN
  SELECT RAISE(ABORT, 'promotion evidence cannot be deleted');
END;
CREATE TRIGGER promotion_regulated_exclusion_no_update
BEFORE UPDATE ON promotion_regulated_exclusion BEGIN
  SELECT RAISE(ABORT, 'regulated-product exclusion evidence is immutable');
END;
CREATE TRIGGER promotion_regulated_exclusion_no_delete
BEFORE DELETE ON promotion_regulated_exclusion BEGIN
  SELECT RAISE(ABORT, 'a published promotion cannot lose its regulated-product exclusion');
END;
CREATE TRIGGER promotion_publication_no_update
BEFORE UPDATE ON promotion_publication BEGIN
  SELECT RAISE(ABORT, 'published offer wording is immutable');
END;
CREATE TRIGGER promotion_publication_no_delete
BEFORE DELETE ON promotion_publication BEGIN
  SELECT RAISE(ABORT, 'published offer evidence cannot be deleted');
END;
CREATE TRIGGER promotion_attribution_no_update
BEFORE UPDATE ON promotion_attribution BEGIN
  SELECT RAISE(ABORT, 'charged-price attribution is immutable');
END;
CREATE TRIGGER promotion_attribution_no_delete
BEFORE DELETE ON promotion_attribution BEGIN
  SELECT RAISE(ABORT, 'charged-price attribution cannot be deleted');
END;
CREATE TRIGGER supplier_invoice_no_update
BEFORE UPDATE ON supplier_invoice BEGIN
  SELECT RAISE(ABORT, 'a supplier tax invoice is immutable — post a supplier credit');
END;
CREATE TRIGGER supplier_invoice_no_delete
BEFORE DELETE ON supplier_invoice BEGIN
  SELECT RAISE(ABORT, 'supplier tax evidence cannot be deleted');
END;
CREATE TRIGGER supplier_invoice_line_no_update
BEFORE UPDATE ON supplier_invoice_line BEGIN
  SELECT RAISE(ABORT, 'a supplier invoice line is immutable');
END;
CREATE TRIGGER supplier_invoice_line_no_delete
BEFORE DELETE ON supplier_invoice_line BEGIN
  SELECT RAISE(ABORT, 'supplier invoice lines cannot be deleted');
END;
CREATE TRIGGER supplier_invoice_line_tax_no_update
BEFORE UPDATE ON supplier_invoice_line_tax BEGIN
  SELECT RAISE(ABORT, 'supplier tax components are immutable');
END;
CREATE TRIGGER supplier_invoice_line_tax_no_delete
BEFORE DELETE ON supplier_invoice_line_tax BEGIN
  SELECT RAISE(ABORT, 'supplier tax components cannot be deleted');
END;
CREATE TRIGGER supplier_invoice_post_event_no_update
BEFORE UPDATE ON supplier_invoice_post_event BEGIN
  SELECT RAISE(ABORT, 'supplier-invoice posting evidence is immutable');
END;
CREATE TRIGGER supplier_invoice_post_event_no_delete
BEFORE DELETE ON supplier_invoice_post_event BEGIN
  SELECT RAISE(ABORT, 'a posted supplier invoice cannot be unsealed');
END;
CREATE TRIGGER tax_filing_period_no_update
BEFORE UPDATE ON tax_filing_period BEGIN
  SELECT RAISE(ABORT, 'filing-period schedules are versioned, not edited');
END;
CREATE TRIGGER tax_filing_period_no_delete
BEFORE DELETE ON tax_filing_period BEGIN
  SELECT RAISE(ABORT, 'a filing period referenced by evidence cannot be deleted');
END;
CREATE TRIGGER tax_filing_event_no_update
BEFORE UPDATE ON tax_filing_event BEGIN
  SELECT RAISE(ABORT, 'filing status is append-only');
END;
CREATE TRIGGER tax_filing_event_no_delete
BEFORE DELETE ON tax_filing_event BEGIN
  SELECT RAISE(ABORT, 'filing history cannot be deleted');
END;
CREATE TRIGGER tax_period_adjustment_no_update
BEFORE UPDATE ON tax_period_adjustment BEGIN
  SELECT RAISE(ABORT, 'tax adjustments are correction facts, not editable rows');
END;
CREATE TRIGGER tax_period_adjustment_no_delete
BEFORE DELETE ON tax_period_adjustment BEGIN
  SELECT RAISE(ABORT, 'tax-adjustment evidence cannot be deleted');
END;
CREATE TRIGGER common_input_allocation_no_update
BEFORE UPDATE ON common_input_allocation BEGIN
  SELECT RAISE(ABORT, 'common-input allocation evidence is immutable');
END;
CREATE TRIGGER common_input_allocation_no_delete
BEFORE DELETE ON common_input_allocation BEGIN
  SELECT RAISE(ABORT, 'common-input allocation evidence cannot be deleted');
END;
CREATE TRIGGER tax_credit_ledger_no_update
BEFORE UPDATE ON tax_credit_ledger BEGIN
  SELECT RAISE(ABORT, 'tax credit is a ledger — append a correction');
END;
CREATE TRIGGER tax_credit_ledger_no_delete
BEFORE DELETE ON tax_credit_ledger BEGIN
  SELECT RAISE(ABORT, 'tax-credit history cannot be deleted');
END;
CREATE TRIGGER tax_filing_election_no_update
BEFORE UPDATE ON tax_filing_election BEGIN
  SELECT RAISE(ABORT, 'a filing election is immutable — append superseding evidence');
END;
CREATE TRIGGER tax_filing_election_no_delete
BEFORE DELETE ON tax_filing_election BEGIN
  SELECT RAISE(ABORT, 'filing-election evidence cannot be deleted');
END;
CREATE TRIGGER credit_note_period_assignment_no_update
BEFORE UPDATE ON credit_note_period_assignment BEGIN
  SELECT RAISE(ABORT, 'filed-period lineage is immutable');
END;
CREATE TRIGGER credit_note_period_assignment_no_delete
BEFORE DELETE ON credit_note_period_assignment BEGIN
  SELECT RAISE(ABORT, 'filed-period lineage cannot be deleted');
END;
```

> ⚠️ **OPEN — blocks 4.7.2.** Which return period and box must receive a credit note issued after the original invoice's filed period for each supported return type and jurisdiction? Default until answered: show the credit as a negative in sales reconciliation on the credit-note date, preserve the original and credit periods, and leave statutory `box_disposition` unresolved rather than auto-populating a return.
> Owner: `4.7.2`. Source that settles it: the current official ISTD credit-note return instructions for General Tax, Special Tax, and each enabled zone profile or a written ISTD ruling; the merchant's accountant confirms how that authority applies to the merchant.

> ⚠️ **OPEN — blocks 4.6.1.** What current tobacco-display layout, marking and customer-facing label restrictions apply to each enabled tobacco product, and which other product classes carry equivalent restrictions? Default until answered: no customer-facing display feature is built, promotions exclude tobacco, the label worklist refuses tobacco labels, and only the sale-form and advertising blocks above ship.
> Owner: `4.6.1`. Source that settles it: the current official Tobacco Products Display Regulation and written implementation guidance from the responsible authority.

---

## Postgres mirror

`apps/server/migrations/` mirrors the SQLite chain through declared header mappings, not shared filenames: sqlx requires unique timestamp versions while SQLite uses `NNNN`. Semantics align even when names differ; representations differ:

| SQLite | Postgres | Note |
|---|---|---|
| `BLOB` PK (16 bytes) | `UUID` | sqlx maps `uuid::Uuid` natively |
| `INTEGER` money/qty | `BIGINT` | 64-bit either way |
| `TEXT` timestamp | `TIMESTAMPTZ` | server converts on ingest |
| `TEXT` JSON | `JSONB` | queryable server-side |
| `INTEGER` 0/1 | `BOOLEAN` | |
| — | `version BIGINT DEFAULT nextval('change_seq')` | the pull cursor; `change_seq` exists already |
| — | partial indexes on `deleted_at IS NULL` | same intent as SQLite |

Both wire envelopes carry `protocol_version` and `schema_version`. PostgreSQL persists the accepted
values from each `sync_commit`; pull pages echo both before any payload so an old register can refuse
an unknown future schema without consuming its cursor. The server accepts the documented compatibility
window, never guesses a missing version, and returns a typed version-mismatch result while the register
continues selling offline.

### Shared multi-tenant decision for sign-off

The server is one shared multi-tenant service. This is a decision recorded for owner sign-off,
not an external requirement: the same schema can run one tenant, while a schema that omits tenant
ownership cannot safely become shared later. The rejected alternative is one server instance and
database per merchant. It offers a smaller blast radius and simpler tenant reasoning, but multiplies
deployments, backups, migrations and monitoring for a solo operator. The owner may overrule this
before the first Phase-3 server migration; after merchant data lands, changing models is a data move.

SQLite remains single-tenant by construction: provisioning binds one database to one `org`, and no
register query accepts an arbitrary tenant. It does not add RLS or duplicate `org_id` onto every
child fact. PostgreSQL is different. Every merchant-owned server table carries `org_id UUID NOT
NULL`, including every fact, queue, projection, reference row, join table and artifact defined above.
The exhaustive exception inventory is the machine-readable comment below: migration metadata and
the immutable vendor capability catalogue are global. PostgreSQL `tender_type` is tenant-owned
because activation and sort order are merchant configuration, not vendor constants.

<!-- postgres-global-tables: _sqlx_migrations, capability -->

Tenant ownership is structural rather than a query convention:

- Every parent has `UNIQUE (org_id, id)`. Every child carries its parent's `org_id` and uses a
  composite `FOREIGN KEY (org_id, parent_id) REFERENCES parent(org_id, id)`. A UUID copied from
  another merchant therefore fails before application filtering matters.
- Merchant keys are tenant-scoped: `UNIQUE (org_id, sku)`, `UNIQUE (org_id, code)` for users,
  promotions and live barcodes, and live `UNIQUE (org_id, phone)` for customers. Global uniqueness
  is reserved for protocol identities that are generated as UUIDs, never merchant-entered codes.
- Store/register relationships are composite too: `(org_id, store_id)`, `(org_id, register_id)` and
  `(org_id, shift_id)` cannot cross organizations. Ownership-bearing scopes are never opaque:
  PostgreSQL `setting` has nullable `org_scope_id`, `store_scope_id` and `register_scope_id` columns,
  a `CHECK` requiring exactly the column named by `scope_kind`, and composite tenant foreign keys.
  PostgreSQL `doc_sequence` likewise uses typed `register_scope_id`/`store_scope_id`; its CHECK maps
  receipt/Z to register and ICV to store. Business-polymorphic `ref_id` values still carry `org_id`,
  canonical kind and payload hash, but they are not trusted as tenant ownership keys.
- From Phase 3, the server submission allocator owns the PostgreSQL `doc_sequence` row keyed by
  `(org_id, 'store', store_id, 'fiscal_icv')` and locks it while assigning a one-value lease bound
  to one `fiscal_uuid`. PostgreSQL also
  enforces `UNIQUE (org_id, store_id, icv) WHERE icv IS NOT NULL` on fiscal queue rows. Registers
  never advance independent store counters; when they cannot reach this allocator they retain NULL.
- Every fact INSERT, permanent `fact_commit_member` and delivery `sync_outbox` row share one
  PostgreSQL transaction. A UUID conflict compares
  canonical payload bytes: identical is `duplicate`; different is `rejected`, dead-lettered and
  alarmed. There is no blind upsert and no `UPDATE` fallback on a fact table.

The migration owner role owns tables. The application role is a non-owner with `NOSUPERUSER`,
`NOCREATEDB`, `NOCREATEROLE`, `NOREPLICATION`, `NOINHERIT` and `NOBYPASSRLS`; it receives only the
required DML grants. Every merchant-owned table enables and
forces RLS, and its only normal application policy is default-deny unless the transaction-scoped org
matches both reads and writes:

```postgresql
ALTER ROLE pos_app NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOINHERIT NOBYPASSRLS;

ALTER TABLE org ENABLE ROW LEVEL SECURITY;
ALTER TABLE org FORCE ROW LEVEL SECURITY;
CREATE POLICY org_tenant_isolation ON org
  USING (id = NULLIF(current_setting('pos.org_id', true), '')::uuid)
  WITH CHECK (id = NULLIF(current_setting('pos.org_id', true), '')::uuid);

ALTER TABLE product ENABLE ROW LEVEL SECURITY;
ALTER TABLE product FORCE ROW LEVEL SECURITY;
CREATE POLICY product_tenant_isolation ON product
  USING (org_id = NULLIF(current_setting('pos.org_id', true), '')::uuid)
  WITH CHECK (org_id = NULLIF(current_setting('pos.org_id', true), '')::uuid);

REVOKE ALL ON product FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE ON product TO pos_app;
```

Phase 3 microstep 3.1.2 extends `scripts/verify-pg-migrations.py` to derive `tenant-tables` as every
mirrored table minus the exact `postgres-global-tables` comment. For every derived table it must fail
unless the table has `org_id NOT NULL` (or `org.id` for the root), `UNIQUE (org_id, id)`, composite
tenant foreign keys, `ENABLE` and `FORCE ROW LEVEL SECURITY`, a policy with both `USING` and
`WITH CHECK`, and no grant broader than its required operations. The same step must prove
`pos_app.rolsuper = false`, `rolbypassrls = false`, the other role flags above, and that `pos_app`
owns no tenant table. The current mapping verifier does not perform those checks; this is planned
acceptance work, because a hand-maintained example is not evidence of exhaustive isolation.

The request transaction derives `org_id` from the authenticated principal/device token and executes
`SET LOCAL pos.org_id = '<uuid>'`; client JSON never supplies the trusted scope. Microstep 3.1.2 adds
an engine-backed lane that connects as the real `pos_app` role, proves a missing setting returns no
rows, proves cross-org reads/writes fail, and proves the role cannot disable RLS, change ownership or
set `row_security = off`. Current CI connects as the migration role and does not prove those attacks.
Running the eventual test as that owner would be a false pass because owners normally bypass policies.

> ⚠️ **OPEN — blocks 3.1.6.** In which country and legal entity will the shared service and each subprocessor host merchant and customer data, and what cross-border basis applies? Default until answered: no customer PII may sync or enter telemetry outside Jordan; only non-PII fixtures may use a development host.
> Owner: 3.1.6. Source that settles it: the signed hosting/subprocessor contract, Jordan PDPL transfer assessment and counsel's written conclusion.

**Two further rules keep the mirrors honest.** (1) Every server table carrying reference data has a
`BEFORE INSERT OR UPDATE` trigger assigning `version = nextval('change_seq')` — the cursor cannot drift
because nobody remembers to bump it. (2) Every machine-listed fact table at the head of this file
carries `REVOKE UPDATE, DELETE` on `pos_app`; `tender_status_event` and `shift_close_event` are INSERTs,
and their current-state tables are disposable projections. Immutability is enforced by the database,
not by discipline (I-4).

**Shipped:** `20260820120000_change_sequence.sql` implements rule (1) for `product` — `assign_change_version()` plus `idx_product_version`, because the pull is `WHERE version > $after ORDER BY version` and without the index that is a sequential scan on every poll from every register. Before it, `change_seq` existed and nothing called `nextval` on it: the column defaulted to 0 and stayed there, so the comment in `20260819200319_init.sql` described a mechanism that was never built. Add the same trigger to each new reference table as it lands.

Rule (2) has no server-side counterpart yet because the server has no fact tables yet. The register's half of it is live — the triggers in `0002_sale_integrity.sql`. When `sale` reaches Postgres, the `REVOKE` lands in the same migration that creates it, not afterwards.

The server also owns `reprint_bundle(org_id, sale_id, receipt_artifact_id, fiscal_result_id,
canonical_payload, payload_hash, created_at)`. It is immutable and permission-gated, but has no
foreign key to a receiving register's local `sale`: register A must be able to fetch register B's
document without pretending that B's sale is a local fact. The bundle is transport evidence, not a
second mutable rendering path.

### Canonical convergence projections

Literal database-byte equality is not a gate: registers own disjoint sale facts, SQLite and
PostgreSQL encode the same values differently, and local job/cache tables are intentionally
different. The three gates are
`prop_server_facts_equal_the_union_of_register_outboxes`,
`prop_reference_tables_converge_across_all_three_nodes`, and
`prop_apply_is_idempotent_under_any_replay_order`.

The canonical register-fact dump selects exactly the tables in the machine-readable
`sync-authority-register-up` inventory. Every business-fact row is keyed by
`(org_id, origin_register_id, entity, entity_id)` and joins its permanent
`fact_commit_member` for `change_id`, `commit_id`, `commit_index`, canonical payload bytes and
`payload_hash`; that member joins immutable `sync_commit` for `commit_size`, `commit_hash`,
`protocol_version`, `schema_version` and `producer_version`. `sync_commit` and
`fact_commit_member` themselves appear once as the envelope/manifest portion of the projection,
not recursively as their own members. The dump verifies that each member's canonical payload
decodes to the selected domain columns of its named local row. Pruning an acknowledged
`sync_outbox` row therefore cannot erase convergence evidence. Server-origin and server-down
immutable events use the two corresponding machine inventories and are checked in their own
authority projections; they are never invented as register outbox rows.
The fact dump excludes storage-only
`rowid`, PostgreSQL sequence values, transport timestamps and retry/lease state because those describe
delivery, not the money fact. UUIDs are lower-case hyphenated text, timestamps are UTC RFC 3339,
binary values are lower-case hex, JSON keys are recursively sorted, and integers remain decimal
integers. No float participates.

The reference dump includes the domain columns of every table in the machine-readable
`sync-reference-tables` inventory plus every server-down tombstone,
ordered by `(org_id, table, id)`. It excludes engine-assigned `version`, local `updated_at` receipt
time and SQLite FTS shadow rows; those differ by engine or delivery while the reference payload and
tombstone must match. The following register-local operational tables are excluded entirely because
they are rebuilt, device-owned or ephemeral: `sync_outbox`, `sync_cursor`, `user_session`,
`auth_attempt_state`, `parked_cart`, `checkout_operation`, `product_quick_add_request`,
`stock_adjustment_request`, `print_job`, `print_attempt`, `doc_sequence`, `stock_cache`,
`refunded_qty_cache`, `shift_state`, `tender_status_current`, `fiscal_queue`, `consent_current`,
`privacy_request_current`, `loyalty_balance_cache`, `stored_value_balance_cache`,
`trusted_time_state`, `product_fts`, `product_fts_map` and FTS shadow tables. The exclusion list is
part of the fixture: adding a table without classifying it fails the dump test.

---

## Index strategy

Added deliberately; each one exists to serve a named query.

| Index | Serves |
|---|---|
| `idx_barcode_code_live` | scan → product, the hot path (< 100 ms budget) |
| `product_fts` | search-as-you-type (< 50 ms over 50k SKUs) |
| `idx_sale_business_date` *(exists)* | X/Z reports, day reports |
| `idx_sale_shift` | shift close, Z totals |
| `idx_shift_one_open` | database-enforced one-open-shift-per-register invariant |
| `idx_stock_ledger_product_store` | on-hand rebuild, movement report |
| `idx_stock_cache_negative` | negative-stock report (C.7) |
| `idx_outbox_unpushed` *(exists)* | sync drain |
| `idx_outbox_expired_lease` | reclaim an in-flight sync claim after process death |
| `idx_sale_line_sale` *(exists, 0002)* | the lines of a sale — reprint, refund, every report |
| `idx_sale_tender_sale` *(exists, 0002)* | the payments of a sale — reprint, reconciliation |
| `idx_sale_receipt_number` *(exists, 0002)* | unique per register; also receipt lookup by number |
| `idx_product_version` *(exists, Postgres)* | the pull cursor, `version > $after` |
| `idx_fiscal_queue_pending` | fiscal retry loop, uncleared-count badge |
| `idx_fiscal_queue_expired_lease` | reclaim a worker claim after process death |
| `idx_fiscal_queue_store_icv` | collision-free store-scoped ICV allocation |
| `idx_fiscal_reconciliation_issue_queue` | operator reconciliation history for one fiscal document |
| `idx_drawer_event_sale_command` | one attributable drawer-open command per cash sale/refund |
| `idx_refund_link_original` | remaining-refundable check, the E.16 hot path |
| `idx_stored_value_instrument` | rebuild and authorize a stored-value balance |
| `idx_audit_action_at` / `idx_audit_actor_at` | fraud reports: overrides & refunds by user |
| `idx_consent_event_customer_kind` | chronological PDPL consent evidence lookup |
| `idx_promotion_attribution_version` | charged-price evidence for one immutable offer version |
| `idx_supplier_invoice_period` | purchase-side tax reconciliation by store and period |

Each owning query/migration microstep runs `EXPLAIN QUERY PLAN` for its hot-path
index and commits the result beside the query. There is no later blanket
`1.11.3` owner: that number belongs to formatting, so pointing schema evidence
there would leave shift, refund and fiscal indexes untested.
