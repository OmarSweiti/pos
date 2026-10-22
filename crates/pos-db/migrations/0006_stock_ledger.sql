-- 0006 — stock ledger  ·  Phase 1, microstep 1.10.1
--
-- Stock is a ledger, not a column (I-6): append-only `stock_ledger` events
-- that each carry the cost basis in force and the post-event projection, the
-- rebuildable `stock_cache` they project into in the same statement, and the
-- register-local `stock_adjustment_request` a manager approves by content
-- hash. From this migration on, a sale whose lines move stock completes only
-- with each line's exact stock fact in its own commit.
--
-- The body below is `ref/schema.md` §0006 transcribed verbatim except for the
-- one correction shape decision 4 records; the section after it carries the
-- three guards #197 tracked since #179, which ride here because migration
-- numbers are contiguous and `0007` is 1.2.5's.
--
-- ── Shape decisions, settled here because 0006 is never reopened ───────────
--
--   1. `sale.is_training` is guarded by two triggers, not a CHECK. SQLite's
--      ALTER TABLE cannot add a CHECK to an existing column, and rebuilding
--      `sale` — which carries twenty-two triggers from three earlier
--      migrations — to add one is the larger risk. No writer has ever set the column (0005 gave
--      it DEFAULT 0 and nothing else writes it), so no existing row needs a
--      data migration; the triggers refuse every future write of a value
--      other than 0 or 1.
--
--   2. `receipt_artifact` gains the delivery-envelope gate its five synced
--      siblings carry. `print_attempt` deliberately does not: `ref/schema.md`
--      lists it among the fact tables because it is append-only evidence, and
--      among the register-local tables because it never syncs. A row that
--      never travels has no envelope to wait for, so a `_has_ready_commit`
--      trigger on it would refuse every print outcome a register records.
--
--   3. `doc_sequence` gains the delete guard every append-only sibling in
--      0005 has. A deleted counter row restarts at 1, which is the one thing a
--      G-2 gapless sequence must never do.
--
--   4. A sale stock event may differ from its line's quantity in exactly one
--      case: a price-embedded label. `ref/schema.md` §0006 as first written
--      required every sale event to equal its line, while
--      `ref/domain-api.md` §4.3 makes a label line one package at the label's
--      price and sends the weight that price implies to the stock ledger with
--      `is_weight_derived` set. Taken together, the first draft would have
--      taken a kilogram of stock for a 49-gram package. The projection and
--      completion triggers now accept a derived weight — flagged, on a weighed
--      line, and still outgoing — and require every measured event to equal
--      its line exactly, as before. Corrected in the reference in the same
--      change.

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
          AND (NEW.qty_delta_milli = -l.qty_milli
            -- A price-embedded label is one package at the label's price, so
            -- its line quantity is not its weight. The weight it implies at
            -- today's price per unit is the stock basis instead, flagged as
            -- derived rather than measured (ref/domain-api.md §4.3, E.80).
            OR (NEW.is_weight_derived = 1 AND l.is_weighed = 1
                AND NEW.qty_delta_milli < 0))))
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
            AND (e.qty_delta_milli = -l.qty_milli
              OR (e.is_weight_derived = 1 AND l.is_weighed = 1
                  AND e.qty_delta_milli < 0))))
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
            AND (e.qty_delta_milli = -l.qty_milli
              OR (e.is_weight_derived = 1 AND l.is_weighed = 1
                  AND e.qty_delta_milli < 0))))
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

-- ── #197: three guards 0005 did not ship ───────────────────────────────────

-- 1. The fiscal-decision gate keys on `is_training = 0`, so any other non-zero
--    value would be a completed, non-training sale with no fiscal decision
--    behind it.
CREATE TRIGGER sale_is_training_is_a_flag_insert
BEFORE INSERT ON sale
WHEN NEW.is_training NOT IN (0, 1)
BEGIN
  SELECT RAISE(ABORT, 'sale.is_training is a flag: 0 or 1');
END;

CREATE TRIGGER sale_is_training_is_a_flag_update
BEFORE UPDATE OF is_training ON sale
WHEN NEW.is_training NOT IN (0, 1)
BEGIN
  SELECT RAISE(ABORT, 'sale.is_training is a flag: 0 or 1');
END;

-- 2. A receipt artifact is a synced fact: no row without its envelope. See
--    shape decision 2 for why `print_attempt` gets no such gate.
CREATE TRIGGER receipt_artifact_has_ready_commit
BEFORE INSERT ON receipt_artifact
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'receipt_artifact' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'receipt artifact requires its complete delivery envelope'); END;

-- 3. A gapless counter is advanced, never removed.
CREATE TRIGGER doc_sequence_no_delete
BEFORE DELETE ON doc_sequence BEGIN
  SELECT RAISE(ABORT, 'a document sequence is never deleted: its next value would restart at 1');
END;
