-- 0007 — search and seed  ·  Phase 1, microsteps 1.2.5–1.2.7
--
-- Catalogue search for the register: an FTS5 index over the Arabic search
-- projections `0003` generates (`name_ar_exact` for precision, `name_ar_fold`
-- for recall), the English name, the SKU and every live barcode, kept in step
-- by triggers on `product` and `barcode` and backfilled here for the catalogue
-- that exists when this migration runs. Then the commissioning side of
-- scale-priced labels: immutable trade-scale identity, append-only
-- verification evidence, and embedded-barcode rules that cannot be switched on
-- without current verified evidence and are switched off the moment it is
-- lost. Last, PLU quick codes and the tile grid for goods with no barcode.
--
-- The body is `ref/schema.md` §0007, transcribed verbatim.
--
-- ── Shape decisions, settled here because 0007 is never reopened ───────────
--
--   1. Raw `name_ar` is not indexed. The tokenizer treats tashkeel as token
--      separators, so a vocalised name would shred into single letters and a
--      one-letter query would match unrelated products. Arabic matching comes
--      entirely from the two generated projections, and the query side folds
--      the same way (`pos_db::search`, held to these columns by
--      `prop_sql_and_rust_folding_agree`).
--
--   2. `prefix='2 3'` is declared now because it cannot be added later without
--      a compensating migration and a full reindex on every register: 1.2.7
--      benchmarks search at one to three characters.
--
--   3. Every embedded-barcode rule this migration can create starts inactive,
--      and none can be activated without a scale whose newest evidence is
--      `verified`. What evidence JSMO accepts is the open item in
--      `ref/schema.md` §0007; until it is answered no scale-derived price
--      reaches checkout, which is the default that item names.

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
