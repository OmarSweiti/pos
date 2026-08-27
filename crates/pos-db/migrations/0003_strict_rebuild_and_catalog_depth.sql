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

