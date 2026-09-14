-- 0005 — sale columns and sequences  ·  Phase 1, microsteps 1.4.11, 1.9.1
--
-- Shifts as an immutable open plus an append-only close, with `shift_state` as
-- the rebuildable projection that carries the one-open-per-register rule; the
-- sale columns and the guards that make a completed sale prove its shift, its
-- tax policy, its fiscal decision and its tender events; `sale_tax_summary`;
-- append-only `tender_status_event` and its projection; the six-row
-- `tender_type` catalogue; durable `parked_cart` and `checkout_operation`;
-- content-hashed `product_quick_add_request`; immutable receipt artifacts with
-- their print jobs and attempts; persisted `trusted_time_state`; and the scoped
-- `doc_sequence` both counters share.
--
-- ── Shape decisions, settled here because 0005 is never reopened ───────────
--
-- Conventions §9: a committed migration is never edited. Anything left for
-- later is left forever, so these are answered now.
--
--   1. `doc_sequence.scope_kind` is `CHECK (scope_kind IN ('register','store'))`
--      — merchant decision 6.9, the authoritative ICV namespace, is NOT ratified
--      and this CHECK is NOT an answer to it. It is issue #113 option 4, taken
--      deliberately on 13 September 2026: freeze `store`, knowingly.
--
--      Why a choice had to be made rather than deferred: this one table serves
--      two counters. `('register', register_id, 'receipt')` numbers receipts for
--      microstep 1.9.3, a Phase-1 selling capability; `('store', store_id,
--      'fiscal_icv')` carries the ICV, which is Phase 2. Deferring the table to
--      wait on a fiscal answer would have taken receipt numbering with it.
--
--      Why the mistake stays cheap: no ICV is ever allocated at checkout, and
--      none is allocated at all before microstep 2.7.4. Until the first
--      `fiscal_icv` row exists, a wrong scope is repaired by one further
--      forward-only migration and no data repair. After it, it is a migration
--      plus a repair on a sequence required to be gapless — so 2.7.4 MUST
--      re-check 6.9 before it allocates the first value.
--
--      `ref/merchant-decisions.md` row 6.9's Answer cell stays EMPTY. Only the
--      official ISTD package can fill it, and a hedge recorded as an answer is
--      exactly the failure that ledger exists to catch.
--
--   2. The `sale` columns arrive by ALTER TABLE ADD COLUMN, not a rebuild.
--      0003 rebuilt these six tables precisely so 0005 would not have to, and
--      ALTER cannot retrofit a REFERENCES clause onto an existing column — so
--      `sale.register_id` and `sale.ref_sale_id` are repaired by triggers
--      instead of foreign keys. That is the documented trade, not an oversight.
--
--   3. No PRAGMA appears in this file. The runner wraps each migration in one
--      transaction, where `PRAGMA foreign_keys` is a no-op and
--      `PRAGMA defer_foreign_keys` cannot clear a deferred violation.
--
--   4. `tender_type` seeds six rows and carries no immutability trigger, so the
--      one value still under an open question — `store_credit.is_internal`,
--      seeded 0 per `domain-api.md` §7.1 — can be corrected by a later
--      migration before `store_credit` is ever activated. It is seeded
--      `is_active = 0`, so it has no Phase-1 effect.
--
-- Transcribed from `ref/schema.md` §0005, all three code blocks, in document
-- order: the fence boundaries there are a documentation artifact and not a
-- statement boundary.

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

-- THE SALE FOREIGN-KEY REPAIR.
-- `register_id` and `ref_sale_id` are the two columns 0001 declared without a
-- foreign key, and 0003's STRICT rebuild carried the omission forward unchanged.
-- Every other column added above gets REFERENCES inline, because ALTER TABLE can
-- attach one to a NEW column and cannot retrofit one onto an existing column.
--
-- Rebuilding `sale` is not available here. 0003's own header records why: the
-- documented twelve-step procedure begins by turning foreign keys off, and
-- `PRAGMA foreign_keys` is a no-op inside a transaction — the migration runner
-- wraps every file in one — while `PRAGMA defer_foreign_keys` is not a
-- substitute, because DROP TABLE records a deferred violation that re-creating
-- the parent does not clear and the COMMIT then fails. The only route would be
-- 0003's staging-table dance, dragging all three tables that reference `sale`
-- through it, and that same header warns that "rebuilding a table with inbound
-- references is far worse later". 0003 rebuilt these six tables precisely so
-- 0005 would not have to.
--
-- Triggers are also the stronger guarantee, not merely the cheaper one: SQLite
-- enforces REFERENCES only while `PRAGMA foreign_keys = ON`, which is per
-- connection, and a trigger fires whatever the connection has set.
CREATE TRIGGER sale_register_exists_insert
BEFORE INSERT ON sale
WHEN NOT EXISTS (SELECT 1 FROM register r WHERE r.id = NEW.register_id)
BEGIN
  SELECT RAISE(ABORT, 'a sale must name an existing register');
END;
CREATE TRIGGER sale_register_exists_update
BEFORE UPDATE OF register_id ON sale
WHEN NOT EXISTS (SELECT 1 FROM register r WHERE r.id = NEW.register_id)
BEGIN
  SELECT RAISE(ABORT, 'a sale must name an existing register');
END;

-- `ref_sale_id` is the nullable self-reference a correction document uses to name
-- the sale it corrects. NULL is the ordinary case and stays legal; a non-NULL
-- value must resolve.
CREATE TRIGGER sale_ref_sale_exists_insert
BEFORE INSERT ON sale
WHEN NEW.ref_sale_id IS NOT NULL
 AND NOT EXISTS (SELECT 1 FROM sale s WHERE s.id = NEW.ref_sale_id)
BEGIN
  SELECT RAISE(ABORT, 'a correction must name an existing sale');
END;
CREATE TRIGGER sale_ref_sale_exists_update
BEFORE UPDATE OF ref_sale_id ON sale
WHEN NEW.ref_sale_id IS NOT NULL
 AND NOT EXISTS (SELECT 1 FROM sale s WHERE s.id = NEW.ref_sale_id)
BEGIN
  SELECT RAISE(ABORT, 'a correction must name an existing sale');
END;

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


CREATE TABLE tender_type (
  code            TEXT PRIMARY KEY,
  name_ar         TEXT NOT NULL,
  name_en         TEXT,
  opens_drawer    INTEGER NOT NULL DEFAULT 0,
  allows_change   INTEGER NOT NULL DEFAULT 0,
  is_cash_counted INTEGER NOT NULL DEFAULT 0,
  is_internal     INTEGER NOT NULL DEFAULT 0
                    CHECK (is_internal IN (0,1)),
  refundable_to   TEXT NOT NULL DEFAULT 'same'
                    CHECK (refundable_to IN ('same','cash','store_credit','none')),
  sort_order      INTEGER NOT NULL DEFAULT 0,
  is_active       INTEGER NOT NULL DEFAULT 1
) STRICT;

-- Complete initial seed. Later payment/refund microsteps enable behavior; they
-- never reopen 0005 to append codes. `exchange` only offsets two linked
-- documents and therefore never opens or counts a drawer.
--
-- `is_internal` marks a tender that moves no value outside the two documents it
-- settles. It is NOT implied by `is_cash_counted = 0`: `card` and `cliq` count
-- no drawer cash but do move real value through a PSP. `exchange` must carry
-- both, or the offset appears in expected drawer cash on both documents and the
-- shift closes short by twice the exchanged value (domain-api.md 7.1).
INSERT INTO tender_type
  (code, name_ar, name_en, opens_drawer, allows_change, is_cash_counted,
   is_internal, refundable_to, sort_order, is_active)
VALUES
  ('cash',         'نقدي',          'Cash',         1, 1, 1, 0, 'cash',         10, 1),
  ('card',         'بطاقة',         'Card',         0, 0, 0, 0, 'same',         20, 0),
  ('cliq',         'كليك',          'CliQ',         0, 0, 0, 0, 'same',         30, 0),
  ('voucher',      'قسيمة',         'Voucher',      0, 0, 0, 0, 'none',         40, 0),
  ('store_credit', 'رصيد المتجر',   'Store credit', 0, 0, 0, 0, 'store_credit', 50, 0),
  ('exchange',     'تسوية استبدال', 'Exchange',     0, 0, 0, 1, 'none',         60, 0);

-- Parked carts are register-local and NEVER sync (master plan C.14).
--
-- `state` and `session_nonce` are the durable working state and session claim
-- 1.8.2b's four repository methods require. `resume` atomically claims the row as
-- `active` under a fresh nonce and never deletes the only durable copy before the
-- IPC response; `save_active` replaces the snapshot under the same nonce; re-park
-- returns the row to `parked`; `consume_on_finalize` removes it only inside the
-- complete-sale transaction. A claim IS an `active` row, which is what stops a
-- second live session from claiming one already held, and what lets startup
-- restore an `active` claim after a process death.
CREATE TABLE parked_cart (
  id            BLOB PRIMARY KEY,
  register_id   BLOB NOT NULL REFERENCES register(id),
  cashier_id    BLOB NOT NULL REFERENCES app_user(id),
  label         TEXT,
  snapshot      TEXT NOT NULL,          -- serialized Cart
  parked_at     TEXT NOT NULL,
  expires_on    TEXT NOT NULL,          -- end of business day (C.2)
  state         TEXT NOT NULL DEFAULT 'parked'
                  CHECK (state IN ('parked','active')),
  session_nonce BLOB,
  -- An active row always carries its claim; a parked row never does.
  CHECK ((state = 'active') = (session_nonce IS NOT NULL))
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
-- One row per register, mapping `ClockState` (domain-api.md 3.2) field for field.
--
-- `confidence` and an observed skew are deliberately NOT stored. Both are outputs
-- of `clock_confidence(state, device_now, monotonic_now_ms, policy)`, and a stored
-- verdict can disagree with the inputs it was derived from — the register would
-- then trust a snapshot instead of its own clock.
--
-- The three trust-anchor readings are captured together, but `device_at_trust` may
-- be absent on its own: that is precisely what makes a partial anchor detectable,
-- so no constraint forces it to accompany the other two.
--
-- `boot_token` is the opaque shell-owned boot-continuity token. The shell compares
-- it on startup and calls `ClockState::note_monotonic_reset` before use when it
-- changes, because a numeric counter alone cannot identify its own boot.
CREATE TABLE trusted_time_state (
  register_id              BLOB PRIMARY KEY REFERENCES register(id),
  last_trusted_at          TEXT,
  device_at_trust          TEXT,
  monotonic_since_trust_ms INTEGER CHECK (monotonic_since_trust_ms >= 0),
  high_water               TEXT NOT NULL,   -- E.6: largest timestamp ever issued
  anomaly_kind             TEXT
                             CHECK (anomaly_kind IN
                               ('jumped_back','jumped_forward','monotonic_reset')),
  anomaly_by_ms            INTEGER,
  anomaly_at               TEXT,
  boot_token               BLOB,
  updated_at               TEXT NOT NULL,
  -- An anomaly always carries the instant it was observed.
  CHECK ((anomaly_kind IS NULL) = (anomaly_at IS NULL)),
  -- Only the two jump variants carry a magnitude; MonotonicReset has none.
  CHECK ((anomaly_by_ms IS NOT NULL)
         = (COALESCE(anomaly_kind,'') IN ('jumped_back','jumped_forward')))
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
