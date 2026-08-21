-- 0002 — sale integrity  ·  Phase 1, microstep 1.1.7
--
-- 0001 shipped the sale tables with the invariants written down and nothing
-- enforcing them, and with one column that contradicts them outright. This
-- migration closes both, in the order the rest of the file depends on:
--
--   1. rebuild sale_line so `qty` becomes `qty_milli`   (gap G-12, I-3)
--   2. index the foreign keys the register actually reads
--   3. make a receipt number identify exactly one sale
--   4. refuse any write that would alter a completed sale (I-4)
--
-- Step 1 must come first: a rebuild drops the old table and every trigger and
-- index attached to it, so anything created before it would silently vanish.

-- ── 1 · G-12 · sale_line.qty is a quantity and must say so ────────────────
--
-- ref/schema.md scheduled this for the catalog-depth migration because the
-- rebuilt table wants a tax_category_id, and tax_category does not exist yet.
-- It is done here instead: SQLite accepts `ALTER TABLE ... ADD COLUMN ...
-- REFERENCES`, so catalog depth can add the remaining columns without a second
-- rebuild — and waiting would have meant shipping a schema that contradicts I-3
-- for as long as it took to get there. Rebuild once, early, while it is free.
--
-- The data step is the whole point: existing rows hold unit COUNTS, and
-- 1 unit = 1000 milli-units. A rename without the multiplication understates
-- every historical quantity by a factor of a thousand.
--
-- No PRAGMA foreign_keys here on purpose: the runner wraps each migration in a
-- transaction, and SQLite ignores that pragma inside one. It is unnecessary
-- anyway — in this schema nothing references sale_line yet.

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

-- ── I-4 · a completed sale is immutable ───────────────────────────────────
-- Conventions §1 claims this is enforced "by review, by a #[test] that greps the
-- repositories, and by the absence of a repository method that could do it".
-- Review is the weakest of those three and was the only one present. A storage
-- engine that refuses is stronger than all three: it holds against a repository
-- that does not exist yet, and against a hand-typed sqlite3 session at 23:00.
--
-- Every guard keys on the OLD row, so the ordinary lifecycle is untouched: a
-- 'parked' sale becomes 'completed' exactly once, and is frozen from then on.
-- Corrections are new documents pointing back via sale.ref_sale_id.

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

-- The lines ARE the fiscal record (I-5: price and name are captured onto them).
-- Adding, altering or removing one after completion rewrites what the customer
-- was charged, so all three are refused.

CREATE TRIGGER sale_line_no_insert_once_completed
BEFORE INSERT ON sale_line
WHEN (SELECT status FROM sale WHERE id = NEW.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: cannot add a line to a completed sale');
END;

CREATE TRIGGER sale_line_no_update_once_completed
BEFORE UPDATE ON sale_line
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a line of a completed sale is immutable');
END;

CREATE TRIGGER sale_line_no_delete_once_completed
BEFORE DELETE ON sale_line
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
BEGIN
  SELECT RAISE(ABORT, 'I-4: a line of a completed sale cannot be deleted');
END;

-- Tenders are the one place a completed sale legitimately still moves: a
-- semi-integrated card capture settles asynchronously, and 0005 adds
-- tender_state/captured_at for exactly that. So UPDATE stays open — but only
-- for the settlement columns. The MONEY is frozen with everything else.

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

CREATE TRIGGER sale_tender_amount_frozen_once_completed
BEFORE UPDATE ON sale_tender
WHEN (SELECT status FROM sale WHERE id = OLD.sale_id) = 'completed'
 AND (NEW.amount_minor <> OLD.amount_minor
   OR NEW.change_minor <> OLD.change_minor
   OR NEW.method       <> OLD.method
   OR NEW.sale_id      <> OLD.sale_id)
BEGIN
  SELECT RAISE(ABORT, 'I-4: the amount of a tender on a completed sale is immutable');
END;

-- ── A receipt number identifies exactly one sale ──────────────────────────
-- Receipt numbers come from a per-register counter (0005), so uniqueness is
-- per register, not global: two registers legitimately both print 000123.
-- Without this, a duplicated counter is discovered by a tax inspector.
CREATE UNIQUE INDEX idx_sale_receipt_number ON sale(register_id, receipt_number);

-- ── The foreign keys the register actually reads ──────────────────────────
-- SQLite does not index a foreign key for you. "Fetch the lines for this sale"
-- is what a receipt reprint, a refund and every report do; without these it is
-- a full scan of every line ever sold. 0003 adds its columns with ALTER TABLE
-- rather than a second rebuild, so these indexes survive it — but the three
-- sale_line triggers above do not: 0003 has to drop them to run its backfill
-- and recreate them verbatim in the same file.
CREATE INDEX idx_sale_line_sale   ON sale_line(sale_id);
CREATE INDEX idx_sale_tender_sale ON sale_tender(sale_id);
