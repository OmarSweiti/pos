-- 0009 — sale capture  ·  Phase 1, microstep 1.8.2
--
-- The facts a completed sale captured that the schema had no column for. The
-- domain's `CompletedSale` (1.4.x, 1.5.2) keeps each line's unit, where its
-- price came from, its ceiling, its age gate and its label's implied weight,
-- each manual discount and override as it was asked for, the cart's opening
-- moment, the price mode it was priced under, and what the final cash tender
-- was rounded from. `0003` and `0005` had kept the money a line came to and
-- not these, so `SaleRepository` (1.8.2) could not read back the sale it
-- wrote, and a refund or reprint would have read today's catalogue for them
-- (I-5). The owner's decision on #359 was to add them now, before any real
-- sale exists to lose them.
--
-- ── Shape decisions, settled here because 0009 is never reopened ───────────
--
--   1. Every column arrives by ALTER TABLE ADD COLUMN, nullable, as 0005's
--      did. A sale completed before this migration has none of them, and no
--      default would be true for it: `unit` defaulted to 'each' would call a
--      kilogram of cheese one item. The completion gate at the end requires the
--      ones every new sale has, from this migration on.
--
--   2. A discount is stored as it was asked for, beside the attributions
--      `sale_line_discount` already holds. The attribution is the fiscal truth
--      (`ref/fiscal-jofotara.md`); the request is what the cashier typed, and
--      it is not derivable from the attributions: a basket share or a line
--      discount that rounds to nothing writes no attribution row at all.
--
--   3. Department lines are not given a price origin here. A department line
--      has no product, and `sale_line.product_id` is NOT NULL: #330 asks how
--      that changes, and its migration widens these vocabularies with it.
--      The origin's vocabulary is guarded by a trigger rather than a CHECK for
--      that reason, so widening it never means rebuilding `sale_line`.
--
--   4. The coherence rules span several columns, and ALTER TABLE cannot add a
--      table CHECK, so each is a pair of triggers, as 0005's buyer identifier
--      is. Single-column vocabularies and ranges are CHECKs on the column.
--
--   5. `sale_tender.tender_no` orders a sale's tenders as they were taken.
--      Nothing else did: tender ids are not ordered, and a rowid is not a fact
--      (VACUUM may renumber it on a table with no INTEGER PRIMARY KEY).
--
--   6. No PRAGMA appears in this file, for 0005's reason: the runner applies
--      each migration in one transaction.

-- ── The sale: the cart's opening, its price mode, its basket discount ───────

ALTER TABLE sale ADD COLUMN opened_at TEXT;
ALTER TABLE sale ADD COLUMN price_mode TEXT
  CHECK (price_mode IN ('inclusive','exclusive'));
-- What the final cash tender was asked to round: `CashRounding.original`.
-- NULL when no cash tender settled the sale. `rounding_adj_minor` (0005) is the
-- adjustment, and the two give the rounded amount.
ALTER TABLE sale ADD COLUMN cash_rounding_original_minor INTEGER;
ALTER TABLE sale ADD COLUMN basket_discount_kind TEXT
  CHECK (basket_discount_kind IN ('percent','amount'));
ALTER TABLE sale ADD COLUMN basket_discount_ppm INTEGER
  CHECK (basket_discount_ppm BETWEEN 1 AND 1000000);
ALTER TABLE sale ADD COLUMN basket_discount_amount_minor INTEGER
  CHECK (basket_discount_amount_minor > 0);
ALTER TABLE sale ADD COLUMN basket_discount_reason   TEXT;
ALTER TABLE sale ADD COLUMN basket_discount_by       BLOB REFERENCES app_user(id);
ALTER TABLE sale ADD COLUMN basket_discount_approver BLOB REFERENCES app_user(id);

CREATE TRIGGER sale_basket_discount_coherent_insert
BEFORE INSERT ON sale
WHEN NOT (
  (NEW.basket_discount_kind IS NULL
    AND NEW.basket_discount_ppm IS NULL AND NEW.basket_discount_amount_minor IS NULL
    AND NEW.basket_discount_reason IS NULL AND NEW.basket_discount_by IS NULL
    AND NEW.basket_discount_approver IS NULL)
  OR (NEW.basket_discount_kind IS NOT NULL
    AND NEW.basket_discount_reason IS NOT NULL AND NEW.basket_discount_by IS NOT NULL
    AND ((NEW.basket_discount_kind = 'percent'
           AND NEW.basket_discount_ppm IS NOT NULL
           AND NEW.basket_discount_amount_minor IS NULL)
      OR (NEW.basket_discount_kind = 'amount'
           AND NEW.basket_discount_amount_minor IS NOT NULL
           AND NEW.basket_discount_ppm IS NULL))))
BEGIN
  SELECT RAISE(ABORT, 'a basket discount is a share or an amount, with its reason and who gave it, or nothing at all');
END;
CREATE TRIGGER sale_basket_discount_coherent_update
BEFORE UPDATE OF basket_discount_kind, basket_discount_ppm, basket_discount_amount_minor,
                 basket_discount_reason, basket_discount_by, basket_discount_approver ON sale
WHEN NOT (
  (NEW.basket_discount_kind IS NULL
    AND NEW.basket_discount_ppm IS NULL AND NEW.basket_discount_amount_minor IS NULL
    AND NEW.basket_discount_reason IS NULL AND NEW.basket_discount_by IS NULL
    AND NEW.basket_discount_approver IS NULL)
  OR (NEW.basket_discount_kind IS NOT NULL
    AND NEW.basket_discount_reason IS NOT NULL AND NEW.basket_discount_by IS NOT NULL
    AND ((NEW.basket_discount_kind = 'percent'
           AND NEW.basket_discount_ppm IS NOT NULL
           AND NEW.basket_discount_amount_minor IS NULL)
      OR (NEW.basket_discount_kind = 'amount'
           AND NEW.basket_discount_amount_minor IS NOT NULL
           AND NEW.basket_discount_ppm IS NULL))))
BEGIN
  SELECT RAISE(ABORT, 'a basket discount is a share or an amount, with its reason and who gave it, or nothing at all');
END;

-- ── The line: unit, price origin, ceiling, age gate, label, discount, override

-- `product.unit`'s spelling (0003), which names the dimension.
ALTER TABLE sale_line ADD COLUMN unit TEXT
  CHECK (unit IN ('each','package','weight','volume','length'));
ALTER TABLE sale_line ADD COLUMN price_origin      TEXT;
ALTER TABLE sale_line ADD COLUMN label_rule_prefix TEXT;
ALTER TABLE sale_line ADD COLUMN label_raw_code    TEXT;
-- The ministry ceiling on the line's unit price when it was captured (J.3).
ALTER TABLE sale_line ADD COLUMN max_price_minor INTEGER
  CHECK (max_price_minor >= 0);
ALTER TABLE sale_line ADD COLUMN min_age INTEGER
  CHECK (min_age BETWEEN 0 AND 255);
ALTER TABLE sale_line ADD COLUMN is_age_confirmed INTEGER
  CHECK (is_age_confirmed IN (0,1));
-- The weight a scale label's price implied at the catalogue's price per unit:
-- the stock basis, never money (E.80).
ALTER TABLE sale_line ADD COLUMN label_qty_milli INTEGER
  CHECK (label_qty_milli > 0);
ALTER TABLE sale_line ADD COLUMN label_unit_price_minor INTEGER
  CHECK (label_unit_price_minor >= 0);
ALTER TABLE sale_line ADD COLUMN line_discount_kind TEXT
  CHECK (line_discount_kind IN ('percent','amount'));
ALTER TABLE sale_line ADD COLUMN line_discount_ppm INTEGER
  CHECK (line_discount_ppm BETWEEN 1 AND 1000000);
ALTER TABLE sale_line ADD COLUMN line_discount_amount_minor INTEGER
  CHECK (line_discount_amount_minor > 0);
ALTER TABLE sale_line ADD COLUMN line_discount_reason   TEXT;
ALTER TABLE sale_line ADD COLUMN line_discount_by       BLOB REFERENCES app_user(id);
ALTER TABLE sale_line ADD COLUMN line_discount_approver BLOB REFERENCES app_user(id);
-- What an overridden line charges a unit. `unit_price_minor` keeps the price
-- it was captured at (1.4.7).
ALTER TABLE sale_line ADD COLUMN override_price_minor INTEGER
  CHECK (override_price_minor >= 0);
ALTER TABLE sale_line ADD COLUMN override_reason   TEXT;
ALTER TABLE sale_line ADD COLUMN override_by       BLOB REFERENCES app_user(id);
ALTER TABLE sale_line ADD COLUMN override_approver BLOB REFERENCES app_user(id);

CREATE TRIGGER sale_line_price_origin_coherent_insert
BEFORE INSERT ON sale_line
WHEN (NEW.price_origin IS NOT NULL AND NEW.price_origin NOT IN ('catalog','label'))
  OR (NEW.price_origin IS 'label')
       <> (NEW.label_rule_prefix IS NOT NULL AND NEW.label_raw_code IS NOT NULL)
  OR (NEW.label_rule_prefix IS NULL) <> (NEW.label_raw_code IS NULL)
  OR (NEW.label_qty_milli IS NULL) <> (NEW.label_unit_price_minor IS NULL)
BEGIN
  SELECT RAISE(ABORT, 'a line priced from the catalogue carries no label; a label line carries its rule and code, and a weight with its price');
END;
CREATE TRIGGER sale_line_price_origin_coherent_update
BEFORE UPDATE OF price_origin, label_rule_prefix, label_raw_code,
                 label_qty_milli, label_unit_price_minor ON sale_line
WHEN (NEW.price_origin IS NOT NULL AND NEW.price_origin NOT IN ('catalog','label'))
  OR (NEW.price_origin IS 'label')
       <> (NEW.label_rule_prefix IS NOT NULL AND NEW.label_raw_code IS NOT NULL)
  OR (NEW.label_rule_prefix IS NULL) <> (NEW.label_raw_code IS NULL)
  OR (NEW.label_qty_milli IS NULL) <> (NEW.label_unit_price_minor IS NULL)
BEGIN
  SELECT RAISE(ABORT, 'a line priced from the catalogue carries no label; a label line carries its rule and code, and a weight with its price');
END;

CREATE TRIGGER sale_line_discount_request_coherent_insert
BEFORE INSERT ON sale_line
WHEN NOT (
  (NEW.line_discount_kind IS NULL
    AND NEW.line_discount_ppm IS NULL AND NEW.line_discount_amount_minor IS NULL
    AND NEW.line_discount_reason IS NULL AND NEW.line_discount_by IS NULL
    AND NEW.line_discount_approver IS NULL)
  OR (NEW.line_discount_kind IS NOT NULL
    AND NEW.line_discount_reason IS NOT NULL AND NEW.line_discount_by IS NOT NULL
    AND ((NEW.line_discount_kind = 'percent'
           AND NEW.line_discount_ppm IS NOT NULL
           AND NEW.line_discount_amount_minor IS NULL)
      OR (NEW.line_discount_kind = 'amount'
           AND NEW.line_discount_amount_minor IS NOT NULL
           AND NEW.line_discount_ppm IS NULL))))
BEGIN
  SELECT RAISE(ABORT, 'a line discount is a share or an amount, with its reason and who gave it, or nothing at all');
END;
CREATE TRIGGER sale_line_discount_request_coherent_update
BEFORE UPDATE OF line_discount_kind, line_discount_ppm, line_discount_amount_minor,
                 line_discount_reason, line_discount_by, line_discount_approver ON sale_line
WHEN NOT (
  (NEW.line_discount_kind IS NULL
    AND NEW.line_discount_ppm IS NULL AND NEW.line_discount_amount_minor IS NULL
    AND NEW.line_discount_reason IS NULL AND NEW.line_discount_by IS NULL
    AND NEW.line_discount_approver IS NULL)
  OR (NEW.line_discount_kind IS NOT NULL
    AND NEW.line_discount_reason IS NOT NULL AND NEW.line_discount_by IS NOT NULL
    AND ((NEW.line_discount_kind = 'percent'
           AND NEW.line_discount_ppm IS NOT NULL
           AND NEW.line_discount_amount_minor IS NULL)
      OR (NEW.line_discount_kind = 'amount'
           AND NEW.line_discount_amount_minor IS NOT NULL
           AND NEW.line_discount_ppm IS NULL))))
BEGIN
  SELECT RAISE(ABORT, 'a line discount is a share or an amount, with its reason and who gave it, or nothing at all');
END;

CREATE TRIGGER sale_line_override_coherent_insert
BEFORE INSERT ON sale_line
WHEN (NEW.override_price_minor IS NULL) <> (NEW.override_reason IS NULL)
  OR (NEW.override_price_minor IS NULL) <> (NEW.override_by IS NULL)
  OR (NEW.override_price_minor IS NULL AND NEW.override_approver IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'an override carries its price, its reason and who set it, or nothing at all');
END;
CREATE TRIGGER sale_line_override_coherent_update
BEFORE UPDATE OF override_price_minor, override_reason, override_by, override_approver
  ON sale_line
WHEN (NEW.override_price_minor IS NULL) <> (NEW.override_reason IS NULL)
  OR (NEW.override_price_minor IS NULL) <> (NEW.override_by IS NULL)
  OR (NEW.override_price_minor IS NULL AND NEW.override_approver IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'an override carries its price, its reason and who set it, or nothing at all');
END;

-- ── The tender: the order it was taken in ─────────────────────────────────

ALTER TABLE sale_tender ADD COLUMN tender_no INTEGER
  CHECK (tender_no > 0);
CREATE UNIQUE INDEX idx_sale_tender_no ON sale_tender(sale_id, tender_no);

-- ── The completion gate ──────────────────────────────────────────────────
--
-- From this migration on, a sale completes only with the facts every sale
-- captures: its opening moment and price mode, each line's unit, price origin
-- and age check, and each tender's place. The optional ones (a ceiling, an age
-- limit, a label, a discount, an override, a cash rounding) are absent when the
-- sale had none, which is what NULL says for them.

CREATE TRIGGER sale_completed_requires_capture_insert
BEFORE INSERT ON sale
WHEN NEW.status = 'completed' AND (
  NEW.opened_at IS NULL OR NEW.price_mode IS NULL
  OR EXISTS (
    SELECT 1 FROM sale_line l
     WHERE l.sale_id = NEW.id
       AND (l.unit IS NULL OR l.price_origin IS NULL OR l.is_age_confirmed IS NULL))
  OR EXISTS (
    SELECT 1 FROM sale_tender t WHERE t.sale_id = NEW.id AND t.tender_no IS NULL))
BEGIN
  SELECT RAISE(ABORT, 'a completed sale carries what it captured: its opening, its price mode, each line''s unit, price origin and age check, and each tender''s order');
END;
CREATE TRIGGER sale_completed_requires_capture_update
BEFORE UPDATE OF status, opened_at, price_mode ON sale
WHEN NEW.status = 'completed' AND (
  NEW.opened_at IS NULL OR NEW.price_mode IS NULL
  OR EXISTS (
    SELECT 1 FROM sale_line l
     WHERE l.sale_id = NEW.id
       AND (l.unit IS NULL OR l.price_origin IS NULL OR l.is_age_confirmed IS NULL))
  OR EXISTS (
    SELECT 1 FROM sale_tender t WHERE t.sale_id = NEW.id AND t.tender_no IS NULL))
BEGIN
  SELECT RAISE(ABORT, 'a completed sale carries what it captured: its opening, its price mode, each line''s unit, price origin and age check, and each tender''s order');
END;
