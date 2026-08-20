# Schema — every migration, SQLite and Postgres

`0001_init.sql` exists (product, sale, sale_line, sale_tender, sync_outbox, sync_cursor). Everything below is new, appended in order, **never edited once committed** (conventions §9).

**Conventions applied throughout:** UUIDv7 as 16-byte `BLOB` primary keys · money as `INTEGER` minor units named `*_minor` · quantity as `INTEGER` milli-units named `*_milli` · rates as `INTEGER` parts-per-million named `*_ppm` · timestamps as ISO-8601 UTC `TEXT` named `*_at` · store-local trading days as `YYYY-MM-DD` `TEXT` named `*_date` · booleans as `INTEGER` 0/1 named `is_*` · soft-delete tombstones (`deleted_at`) on **reference** data only, never on facts.

| # | File | Adds | Phase |
|---|---|---|---|
| 0002 | `0002_catalog_depth.sql` | stores, registers, categories, tax categories & rates, barcodes, settings; fixes `sale_line.qty` | 1 |
| 0003 | `0003_people_and_audit.sql` | users, roles, capabilities, sessions, hash-chained audit log | 1 |
| 0004 | `0004_sale_columns_and_sequences.sql` | sale identity/shift/training/rounding columns, per-register counters | 1 |
| 0005 | `0005_stock_ledger.sql` | stock ledger + rebuildable on-hand/WAC cache | 1 |
| 0006 | `0006_search_and_seed.sql` | FTS5 index + triggers, price-embedded barcode rules | 1 |
| 0007 | `0007_shifts_and_cash.sql` | shifts, cash movements, Z reports | 2 |
| 0008 | `0008_refunds_and_returns.sql` | refund links, restock decisions, refund policy | 2 |
| 0009 | `0009_fiscal.sql` | fiscal queue, results, dead letters, reconciliation | 2 |
| 0010 | `0010_customers_loyalty.sql` | customers, consents, loyalty ledger | 3 |
| 0011 | `0011_pricing_promotions_supply.sql` | price lists, promotions, suppliers, receipts, counts, transfers | 4 |

---

## 0002 — catalog depth  ·  Phase 1, microsteps 1.2.1–1.2.3

Introduces the organisational spine the whole schema hangs from. `store` and `register` must exist in Phase 1 even though multi-store is Phase 4 — retrofitting a `store_id` onto a live stock ledger is a data migration nobody enjoys.

```sql
-- ── Organisation ───────────────────────────────────────────────────────────
CREATE TABLE org (
  id            BLOB PRIMARY KEY,
  legal_name    TEXT    NOT NULL,
  tin           TEXT,                     -- tax number, printed on every receipt (B.6)
  deleted_at    TEXT,
  updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE store (
  id                 BLOB PRIMARY KEY,
  org_id             BLOB NOT NULL REFERENCES org(id),
  code               TEXT NOT NULL UNIQUE,
  name_ar            TEXT NOT NULL,
  name_en            TEXT,
  address            TEXT,
  phone              TEXT,
  currency           TEXT NOT NULL DEFAULT 'JOD',
  -- Jurisdiction (master plan B.1). ASEZ / development areas are a store-level
  -- tax profile, not a hack.
  tax_profile        TEXT NOT NULL DEFAULT 'standard'
                       CHECK (tax_profile IN ('standard','asez','development_area','unregistered')),
  price_mode         TEXT NOT NULL DEFAULT 'inclusive'
                       CHECK (price_mode IN ('inclusive','exclusive')),
  -- Fiscal profile: 'disabled' keeps the product legal for unregistered
  -- micro-merchants and sellable outside Jordan (master plan E.29).
  fiscal_profile     TEXT NOT NULL DEFAULT 'disabled'
                       CHECK (fiscal_profile IN ('disabled','jordan_jofotara')),
  utc_offset_minutes INTEGER NOT NULL DEFAULT 180,   -- Asia/Amman
  day_cutover_minutes INTEGER NOT NULL DEFAULT 240,  -- 04:00 local (conventions §11)
  money_decimals     INTEGER NOT NULL DEFAULT 3 CHECK (money_decimals BETWEEN 0 AND 3),
  cash_round_step_minor INTEGER NOT NULL DEFAULT 10, -- 1 qirsh (B.5)
  cash_round_direction  TEXT NOT NULL DEFAULT 'nearest'
                       CHECK (cash_round_direction IN ('nearest','up','down')),
  rounding_rule      TEXT NOT NULL DEFAULT 'half_away_from_zero'
                       CHECK (rounding_rule IN ('half_away_from_zero','half_even','floor','ceil')),
  allow_negative_stock INTEGER NOT NULL DEFAULT 1,   -- allow-and-flag default (C.7)
  receipt_locale     TEXT NOT NULL DEFAULT 'ar',
  deleted_at         TEXT,
  updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version            INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE register (
  id           BLOB PRIMARY KEY,
  store_id     BLOB NOT NULL REFERENCES store(id),
  code         TEXT NOT NULL,             -- 'REG01' → receipt prefix
  name         TEXT NOT NULL,
  device_id    TEXT,                      -- hardware fingerprint; clone detection (E.13)
  is_active    INTEGER NOT NULL DEFAULT 1,
  deleted_at   TEXT,
  updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version      INTEGER NOT NULL DEFAULT 0,
  UNIQUE (store_id, code)
);

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
);

CREATE TABLE tax_category (
  id          BLOB PRIMARY KEY,
  code        TEXT NOT NULL UNIQUE,       -- 'STD16','EXEMPT','ZERO','RED04'
  name_ar     TEXT NOT NULL,
  name_en     TEXT,
  treatment   TEXT NOT NULL CHECK (treatment IN ('standard','reduced','zero','exempt')),
  deleted_at  TEXT,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version     INTEGER NOT NULL DEFAULT 0
);

-- Rates are DATA with effective dates. Jordan changes reduced rates by Cabinet
-- decree; a rate in code is a re-release (master plan B.1).
-- `component_code` allows >1 component per item so Special Sales Tax is a data
-- change, not an engine migration.
CREATE TABLE tax_rate (
  id               BLOB PRIMARY KEY,
  tax_category_id  BLOB NOT NULL REFERENCES tax_category(id),
  component_code   TEXT NOT NULL DEFAULT 'GST',
  treatment        TEXT NOT NULL CHECK (treatment IN ('standard','reduced','zero','exempt')),
  rate_ppm         INTEGER NOT NULL,      -- 16% = 160000
  valid_from       TEXT NOT NULL,         -- inclusive
  valid_to         TEXT,                  -- exclusive; NULL = open
  profile_scope    TEXT CHECK (profile_scope IN ('standard','asez','development_area','unregistered')),
  deleted_at       TEXT,
  updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_tax_rate_lookup ON tax_rate(tax_category_id, component_code, valid_from);

-- ── Product depth ──────────────────────────────────────────────────────────
ALTER TABLE product ADD COLUMN name_ar          TEXT;
ALTER TABLE product ADD COLUMN name_en          TEXT;
ALTER TABLE product ADD COLUMN category_id      BLOB REFERENCES category(id);
ALTER TABLE product ADD COLUMN tax_category_id  BLOB REFERENCES tax_category(id);
ALTER TABLE product ADD COLUMN unit             TEXT NOT NULL DEFAULT 'each';
ALTER TABLE product ADD COLUMN is_weighed       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE product ADD COLUMN is_service       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE product ADD COLUMN min_age          INTEGER;         -- E.69
ALTER TABLE product ADD COLUMN max_price_minor  INTEGER;         -- ministry ceiling (J.3, E.71)
ALTER TABLE product ADD COLUMN reorder_point_milli INTEGER;
UPDATE product SET name_ar = name WHERE name_ar IS NULL;

-- A product often carries several codes: multipacks, supplier relabels.
-- The barcode is a LOOKUP KEY; identity is the UUID (master plan C.1).
CREATE TABLE barcode (
  id          BLOB PRIMARY KEY,
  product_id  BLOB NOT NULL REFERENCES product(id),
  code        TEXT NOT NULL,
  kind        TEXT NOT NULL DEFAULT 'ean13'
                CHECK (kind IN ('ean13','ean8','upca','code128','internal','price_embedded','weight_embedded')),
  is_primary  INTEGER NOT NULL DEFAULT 0,
  deleted_at  TEXT,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version     INTEGER NOT NULL DEFAULT 0
);
-- Partial unique: a tombstoned code may be reissued, and collisions among LIVE
-- codes are caught. E.36 resolves scans to the newest active + a warning.
CREATE UNIQUE INDEX idx_barcode_code_live ON barcode(code) WHERE deleted_at IS NULL;
CREATE INDEX idx_barcode_product ON barcode(product_id);

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
);
```

### The `sale_line.qty` fix — gap G-12

`0001_init` declared `qty INTEGER`, contradicting the milli-unit decision (conventions I-3). SQLite cannot rename-and-retype in place, so the table is rebuilt. **This must land before a single sale row exists.**

```sql
PRAGMA foreign_keys = OFF;

CREATE TABLE sale_line_new (
  id               BLOB PRIMARY KEY,
  sale_id          BLOB NOT NULL REFERENCES sale(id),
  line_no          INTEGER NOT NULL,
  product_id       BLOB NOT NULL REFERENCES product(id),
  name_snapshot    TEXT NOT NULL,        -- I-5: copied at capture, never re-read
  qty_milli        INTEGER NOT NULL,     -- G-12
  unit_price_minor INTEGER NOT NULL,     -- I-5
  discount_minor   INTEGER NOT NULL DEFAULT 0,
  net_minor        INTEGER NOT NULL DEFAULT 0,
  tax_minor        INTEGER NOT NULL DEFAULT 0,
  tax_category_id  BLOB REFERENCES tax_category(id),
  tax_rate_ppm     INTEGER NOT NULL DEFAULT 0,
  total_minor      INTEGER NOT NULL,
  is_weighed       INTEGER NOT NULL DEFAULT 0,
  UNIQUE (sale_id, line_no)
);

INSERT INTO sale_line_new
  (id, sale_id, line_no, product_id, name_snapshot, qty_milli,
   unit_price_minor, discount_minor, tax_minor, total_minor)
SELECT l.id, l.sale_id,
       ROW_NUMBER() OVER (PARTITION BY l.sale_id ORDER BY l.rowid),
       l.product_id, COALESCE(p.name, ''), l.qty * 1000,
       l.unit_price_minor, l.discount_minor, l.tax_minor, l.total_minor
FROM sale_line l LEFT JOIN product p ON p.id = l.product_id;

DROP TABLE sale_line;
ALTER TABLE sale_line_new RENAME TO sale_line;
CREATE INDEX idx_sale_line_sale ON sale_line(sale_id);
PRAGMA foreign_keys = ON;

-- Per-component tax on a line: v1 writes one GST row, and Special Sales Tax
-- becomes a second row with no schema change (master plan B.1).
CREATE TABLE sale_line_tax (
  id              BLOB PRIMARY KEY,
  sale_line_id    BLOB NOT NULL REFERENCES sale_line(id),
  component_code  TEXT NOT NULL,
  treatment       TEXT NOT NULL,
  rate_ppm        INTEGER NOT NULL,
  net_minor       INTEGER NOT NULL,
  tax_minor       INTEGER NOT NULL
);
CREATE INDEX idx_sale_line_tax_line ON sale_line_tax(sale_line_id);

-- Discount attributions. Campaign-cost reporting (C.9) AND JoFotara's per-line
-- discount requirement (correction C-2) both read this table. A basket discount
-- that has not been attributed to lines cannot become a fiscal document.
CREATE TABLE sale_line_discount (
  id             BLOB PRIMARY KEY,
  sale_line_id   BLOB NOT NULL REFERENCES sale_line(id),
  source         TEXT NOT NULL CHECK (source IN ('manual_line','manual_basket','promotion','loyalty','price_override')),
  promotion_id   BLOB,
  authorized_by  BLOB,
  reason         TEXT,
  amount_minor   INTEGER NOT NULL,
  percent_ppm    INTEGER            -- the percentage the fiscal builder emits (C-2)
);
CREATE INDEX idx_sale_line_discount_line ON sale_line_discount(sale_line_id);
```

---

## 0003 — people and audit  ·  Phase 1, microsteps 1.6.1–1.6.4

```sql
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
);

CREATE TABLE role (
  id          BLOB PRIMARY KEY,
  code        TEXT NOT NULL UNIQUE,        -- cashier|shift_lead|manager|owner
  name_ar     TEXT NOT NULL,
  name_en     TEXT,
  deleted_at  TEXT,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE role_capability (
  role_id     BLOB NOT NULL REFERENCES role(id),
  capability  TEXT NOT NULL,               -- 'sale.void', flat strings (C.10)
  limit_json  TEXT,                        -- e.g. {"max_percent_ppm":50000}
  PRIMARY KEY (role_id, capability)
);

CREATE TABLE user_role (
  user_id   BLOB NOT NULL REFERENCES app_user(id),
  role_id   BLOB NOT NULL REFERENCES role(id),
  store_id  BLOB REFERENCES store(id),     -- NULL = org-wide
  PRIMARY KEY (user_id, role_id, store_id)
);

CREATE TABLE user_session (
  id           BLOB PRIMARY KEY,
  user_id      BLOB NOT NULL REFERENCES app_user(id),
  register_id  BLOB NOT NULL REFERENCES register(id),
  started_at   TEXT NOT NULL,
  ended_at     TEXT,
  end_reason   TEXT CHECK (end_reason IN ('logout','idle_lock','switch_user','shift_close','crash'))
);

-- Hash-chained (G-7). hash = BLAKE3(prev_hash ‖ canonical_json(entry)).
-- Append-only: no UPDATE, no DELETE, ever.
CREATE TABLE audit_log (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  id          BLOB NOT NULL UNIQUE,
  register_id BLOB NOT NULL,
  actor_id    BLOB NOT NULL,
  approver_id BLOB,                        -- distinct from actor on escalation (E.52)
  action      TEXT NOT NULL,
  entity      TEXT NOT NULL,
  entity_id   BLOB,
  reason      TEXT,
  payload     TEXT NOT NULL,               -- canonical JSON. NEVER PII or card data.
  prev_hash   BLOB NOT NULL,
  hash        BLOB NOT NULL,
  at          TEXT NOT NULL
);
CREATE INDEX idx_audit_action_at ON audit_log(action, at);
CREATE INDEX idx_audit_actor_at  ON audit_log(actor_id, at);
```

---

## 0004 — sale columns and sequences  ·  Phase 1, microsteps 1.4.11, 1.9.1

```sql
ALTER TABLE sale ADD COLUMN store_id            BLOB REFERENCES store(id);
ALTER TABLE sale ADD COLUMN shift_id            BLOB;
ALTER TABLE sale ADD COLUMN cashier_id          BLOB REFERENCES app_user(id);
ALTER TABLE sale ADD COLUMN customer_id         BLOB;
ALTER TABLE sale ADD COLUMN doc_type            TEXT NOT NULL DEFAULT 'sale'
                                                  CHECK (doc_type IN ('sale','refund'));
ALTER TABLE sale ADD COLUMN buyer_tin           TEXT;      -- B2B fiscal (B.2)
ALTER TABLE sale ADD COLUMN buyer_name          TEXT;
ALTER TABLE sale ADD COLUMN is_training         INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sale ADD COLUMN discount_minor      INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sale ADD COLUMN rounding_adj_minor  INTEGER NOT NULL DEFAULT 0;  -- B.5
ALTER TABLE sale ADD COLUMN receipt_printed_at  TEXT;      -- NULL ⇒ reprint worklist (C.15)
ALTER TABLE sale ADD COLUMN origin_device       TEXT;

-- Receipt tax summary, stored not derived. A refund six months later, a reprint,
-- and the fiscal document all read the SAME numbers the customer saw.
CREATE TABLE sale_tax_summary (
  id              BLOB PRIMARY KEY,
  sale_id         BLOB NOT NULL REFERENCES sale(id),
  component_code  TEXT NOT NULL,
  treatment       TEXT NOT NULL,
  rate_ppm        INTEGER NOT NULL,
  net_minor       INTEGER NOT NULL,
  tax_minor       INTEGER NOT NULL,
  gross_minor     INTEGER NOT NULL
);
CREATE INDEX idx_sale_tax_summary_sale ON sale_tax_summary(sale_id);

ALTER TABLE sale_tender ADD COLUMN tender_state TEXT NOT NULL DEFAULT 'collected'
                                                  CHECK (tender_state IN ('collected','pending','reversed'));
ALTER TABLE sale_tender ADD COLUMN masked_pan   TEXT;   -- receipt only
ALTER TABLE sale_tender ADD COLUMN scheme       TEXT;
ALTER TABLE sale_tender ADD COLUMN captured_at  TEXT;

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
);

-- Parked carts are register-local and NEVER sync (master plan C.14).
CREATE TABLE parked_cart (
  id           BLOB PRIMARY KEY,
  register_id  BLOB NOT NULL REFERENCES register(id),
  cashier_id   BLOB NOT NULL REFERENCES app_user(id),
  label        TEXT,
  snapshot     TEXT NOT NULL,           -- serialized Cart
  parked_at    TEXT NOT NULL,
  expires_on   TEXT NOT NULL            -- end of business day (C.2)
);

-- Sequence integrity (G-2). Counters, never derived from time (E.6).
-- Bumped in the SAME transaction as the document it numbers, so a crash
-- cannot consume a number without producing a document.
CREATE TABLE doc_sequence (
  register_id  BLOB NOT NULL REFERENCES register(id),
  kind         TEXT NOT NULL CHECK (kind IN ('receipt','zreport','fiscal_icv')),
  next_value   INTEGER NOT NULL DEFAULT 1,
  prefix       TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (register_id, kind)
);
```

---

## 0005 — stock ledger  ·  Phase 1, microstep 1.10.1

```sql
-- Stock is a LEDGER, not a column (I-6). On-hand = Σ qty_delta_milli.
CREATE TABLE stock_ledger (
  id              BLOB PRIMARY KEY,
  product_id      BLOB NOT NULL REFERENCES product(id),
  store_id        BLOB NOT NULL REFERENCES store(id),
  qty_delta_milli INTEGER NOT NULL,       -- negative on sale
  kind            TEXT NOT NULL CHECK (kind IN (
                    'sale','refund_restock','refund_damage','receive','adjust',
                    'count_correction','transfer_out','transfer_in','waste','rtv','kit_explode')),
  reason_code     TEXT,                    -- damage|theft|expiry|correction
  ref_kind        TEXT,                    -- 'sale','goods_receipt','stock_count'
  ref_id          BLOB,
  unit_cost_minor INTEGER,                 -- on receipts; feeds WAC
  actor_id        BLOB,
  occurred_at     TEXT NOT NULL,
  business_date   TEXT NOT NULL
);
CREATE INDEX idx_stock_ledger_product_store ON stock_ledger(product_id, store_id, occurred_at);
CREATE INDEX idx_stock_ledger_ref           ON stock_ledger(ref_kind, ref_id);

-- A CACHE, not a truth. `stock_cache_rebuild` regenerates it from the ledger
-- and CI asserts the rebuild is a no-op on the seeded fixture (I-6).
CREATE TABLE stock_cache (
  product_id    BLOB NOT NULL REFERENCES product(id),
  store_id      BLOB NOT NULL REFERENCES store(id),
  on_hand_milli INTEGER NOT NULL DEFAULT 0,
  wac_minor     INTEGER NOT NULL DEFAULT 0,
  last_event_at TEXT,
  PRIMARY KEY (product_id, store_id)
);
CREATE INDEX idx_stock_cache_negative ON stock_cache(store_id) WHERE on_hand_milli < 0;  -- C.7
```

---

## 0006 — search and scan rules  ·  Phase 1, microsteps 1.2.5–1.2.7

```sql
-- FTS5 over Arabic AND English names plus SKU. Budget: <50 ms over 50k SKUs.
-- remove_diacritics=2 folds Arabic tashkeel so "قهوة" matches "قَهْوَة".
CREATE VIRTUAL TABLE product_fts USING fts5(
  name_ar, name_en, sku, barcodes,
  content='',                                  -- external content, manually synced
  tokenize="unicode61 remove_diacritics 2"
);

CREATE TABLE product_fts_map (rowid INTEGER PRIMARY KEY, product_id BLOB NOT NULL UNIQUE);

-- Triggers keep FTS in step with product and barcode writes.
CREATE TRIGGER product_ai AFTER INSERT ON product BEGIN … END;
CREATE TRIGGER product_au AFTER UPDATE ON product BEGIN … END;
CREATE TRIGGER product_ad AFTER DELETE ON product BEGIN … END;
CREATE TRIGGER barcode_ai AFTER INSERT ON barcode BEGIN … END;
CREATE TRIGGER barcode_ad AFTER DELETE ON barcode BEGIN … END;

-- Deli-scale barcodes: prefix means "the digits that follow are a weight/price".
-- Store-configured because every scale vendor picks a different layout (C.1).
CREATE TABLE embedded_barcode_rule (
  id               BLOB PRIMARY KEY,
  store_id         BLOB REFERENCES store(id),   -- NULL = org-wide
  prefix           TEXT NOT NULL,
  item_code_start  INTEGER NOT NULL,
  item_code_len    INTEGER NOT NULL,
  value_start      INTEGER NOT NULL,
  value_len        INTEGER NOT NULL,
  value_kind       TEXT NOT NULL CHECK (value_kind IN ('weight_milli','price_minor')),
  value_scale      INTEGER NOT NULL DEFAULT 1,
  verify_checksum  INTEGER NOT NULL DEFAULT 1,   -- E.40: reject, never guess
  is_active        INTEGER NOT NULL DEFAULT 1,
  updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version          INTEGER NOT NULL DEFAULT 0
);

-- PLU quick codes + the tile grid for unbarcoded goods (C.1).
CREATE TABLE plu_code (
  code        TEXT PRIMARY KEY,
  product_id  BLOB NOT NULL REFERENCES product(id),
  deleted_at  TEXT
);

CREATE TABLE tile_grid (
  id          BLOB PRIMARY KEY,
  store_id    BLOB REFERENCES store(id),
  name_ar     TEXT NOT NULL,
  name_en     TEXT,
  sort_order  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE tile (
  id          BLOB PRIMARY KEY,
  grid_id     BLOB NOT NULL REFERENCES tile_grid(id),
  product_id  BLOB REFERENCES product(id),
  category_id BLOB REFERENCES category(id),
  label_ar    TEXT,
  color       TEXT,
  position    INTEGER NOT NULL
);
```

> **FTS5 must be verified, not assumed.** `rusqlite` has no `fts5` feature flag; FTS5 arrives through the bundled SQLite build, and this project uses `bundled-sqlcipher-vendored-openssl`. Microstep 1.2.6 adds a startup assertion (`SELECT * FROM pragma_compile_options WHERE compile_options = 'ENABLE_FTS5'`) that fails loudly at open rather than letting search silently return nothing.

---

## 0007 — shifts and cash  ·  Phase 2, microsteps 2.4.1–2.4.2

```sql
CREATE TABLE shift (
  id                 BLOB PRIMARY KEY,
  register_id        BLOB NOT NULL REFERENCES register(id),
  store_id           BLOB NOT NULL REFERENCES store(id),
  business_date      TEXT NOT NULL,              -- conventions §11
  opened_by          BLOB NOT NULL REFERENCES app_user(id),
  opened_at          TEXT NOT NULL,
  float_minor        INTEGER NOT NULL DEFAULT 0,
  closed_by          BLOB REFERENCES app_user(id),
  closed_at          TEXT,
  counted_minor      INTEGER,                    -- BLIND: entered before expected is shown
  expected_minor     INTEGER,
  over_short_minor   INTEGER,
  z_number           INTEGER,
  close_kind         TEXT CHECK (close_kind IN ('normal','forced_stale')),  -- E.53
  ack_by             BLOB REFERENCES app_user(id),   -- over/short past threshold
  UNIQUE (register_id, z_number)
);
-- One open shift per register (C.6).
CREATE UNIQUE INDEX idx_shift_one_open ON shift(register_id) WHERE closed_at IS NULL;

CREATE TABLE cash_movement (
  id            BLOB PRIMARY KEY,
  shift_id      BLOB NOT NULL REFERENCES shift(id),
  kind          TEXT NOT NULL CHECK (kind IN ('paid_in','paid_out','drop','bank_deposit','float_add')),
  amount_minor  INTEGER NOT NULL,
  reason_code   TEXT NOT NULL,
  note          TEXT,
  actor_id      BLOB NOT NULL REFERENCES app_user(id),
  approver_id   BLOB REFERENCES app_user(id),
  occurred_at   TEXT NOT NULL
);
CREATE INDEX idx_cash_movement_shift ON cash_movement(shift_id);

CREATE TABLE shift_count_line (          -- the denomination grid (D screen 8/9)
  id                 BLOB PRIMARY KEY,
  shift_id           BLOB NOT NULL REFERENCES shift(id),
  phase              TEXT NOT NULL CHECK (phase IN ('open','close')),
  denomination_minor INTEGER NOT NULL,
  count              INTEGER NOT NULL
);

-- The Z report is an immutable stored DOCUMENT: reprintable, synced,
-- sequentially numbered per register (C.6).
CREATE TABLE z_report (
  id           BLOB PRIMARY KEY,
  shift_id     BLOB NOT NULL UNIQUE REFERENCES shift(id),
  register_id  BLOB NOT NULL REFERENCES register(id),
  z_number     INTEGER NOT NULL,
  payload      TEXT NOT NULL,            -- the full ZReport model, frozen
  generated_at TEXT NOT NULL,
  generated_by BLOB NOT NULL REFERENCES app_user(id)
);

CREATE TABLE drawer_event (              -- no-sale opens are the classic theft tell (E.35)
  id           BLOB PRIMARY KEY,
  register_id  BLOB NOT NULL REFERENCES register(id),
  shift_id     BLOB REFERENCES shift(id),
  actor_id     BLOB NOT NULL REFERENCES app_user(id),
  approver_id  BLOB REFERENCES app_user(id),
  cause        TEXT NOT NULL CHECK (cause IN ('sale','refund','no_sale','cash_movement','shift_open','shift_close')),
  sale_id      BLOB,
  reason       TEXT,
  occurred_at  TEXT NOT NULL
);
```

---

## 0008 — refunds and returns  ·  Phase 2, microsteps 2.3.1–2.3.2

```sql
-- Post-completion "void" DOES NOT EXIST. It is a same-day full refund document
-- referencing the original (master plan C.5). `ref_sale_id` already exists on
-- `sale`; these tables carry the return-specific facts.
CREATE TABLE refund_line_link (
  id                    BLOB PRIMARY KEY,
  refund_line_id        BLOB NOT NULL REFERENCES sale_line(id),
  original_line_id      BLOB NOT NULL REFERENCES sale_line(id),
  qty_milli             INTEGER NOT NULL,
  restock               TEXT NOT NULL CHECK (restock IN ('to_stock','damaged','none')),
  reason_code           TEXT NOT NULL,   -- change_of_mind|defective|wrong_item|expired
  is_defective_claim    INTEGER NOT NULL DEFAULT 0   -- rights-based, may bypass the window (J.3)
);
CREATE INDEX idx_refund_link_original ON refund_line_link(original_line_id);

-- Denormalised guard for the invariant that must never break:
-- cumulative refunds per line never exceed sold qty (E.16).
-- Maintained in the same transaction as the refund; rebuildable from the links.
CREATE TABLE refunded_qty_cache (
  original_line_id BLOB PRIMARY KEY REFERENCES sale_line(id),
  refunded_milli   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE refund_policy (
  store_id                     BLOB PRIMARY KEY REFERENCES store(id),
  window_days                  INTEGER NOT NULL DEFAULT 14,
  allow_receiptless            INTEGER NOT NULL DEFAULT 0,
  receiptless_max_minor        INTEGER,
  receiptless_store_credit_only INTEGER NOT NULL DEFAULT 1,
  allow_cash_for_card          INTEGER NOT NULL DEFAULT 0,   -- laundering vector (C.5)
  cash_for_card_max_minor      INTEGER,
  escalate_above_minor         INTEGER NOT NULL DEFAULT 20000,
  ban_self_approval            INTEGER NOT NULL DEFAULT 1,   -- E.52
  updated_at                   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version                      INTEGER NOT NULL DEFAULT 0
);

-- Exchange = return + new sale, settling only the difference. Under the hood it
-- is exactly those two documents, linked (C.5). The chain matters because
-- refundable qty follows it (E.30).
CREATE TABLE document_link (
  id         BLOB PRIMARY KEY,
  from_sale  BLOB NOT NULL REFERENCES sale(id),
  to_sale    BLOB NOT NULL REFERENCES sale(id),
  link_kind  TEXT NOT NULL CHECK (link_kind IN ('exchange','correction','reprint_of')),
  created_at TEXT NOT NULL
);
```

---

## 0009 — fiscal  ·  Phase 2, microsteps 2.7.1–2.7.3

Full pipeline design in [`fiscal-jofotara.md`](fiscal-jofotara.md).

```sql
-- Durable queue. The sale ALWAYS completes locally; clearance is asynchronous
-- and retried (master plan B.2). Written in the same transaction as the sale.
CREATE TABLE fiscal_queue (
  id             BLOB PRIMARY KEY,
  sale_id        BLOB NOT NULL REFERENCES sale(id),
  store_id       BLOB NOT NULL REFERENCES store(id),
  doc_kind       TEXT NOT NULL CHECK (doc_kind IN ('invoice','credit_note')),
  icv            INTEGER NOT NULL,          -- monotonic per-taxpayer counter
  fiscal_uuid    TEXT NOT NULL,             -- v4 SHAPE, generated separately (see C-3 note)
  payload_xml    TEXT NOT NULL,             -- built ONCE from the finalized sale, never rebuilt
  payload_hash   TEXT NOT NULL,
  state          TEXT NOT NULL DEFAULT 'queued'
                   CHECK (state IN ('queued','sending','cleared','rejected','dead','skipped')),
  attempts       INTEGER NOT NULL DEFAULT 0,
  next_attempt_at TEXT,
  last_error     TEXT,
  -- A credit note may not clear before its invoice does (E.26).
  depends_on     BLOB REFERENCES fiscal_queue(id),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);
CREATE INDEX idx_fiscal_queue_pending ON fiscal_queue(state, next_attempt_at)
  WHERE state IN ('queued','sending');
CREATE UNIQUE INDEX idx_fiscal_queue_sale_kind ON fiscal_queue(sale_id, doc_kind);

CREATE TABLE fiscal_result (
  sale_id       BLOB PRIMARY KEY REFERENCES sale(id),
  fiscal_uuid   TEXT NOT NULL,
  invoice_number TEXT,
  qr_payload    TEXT NOT NULL,              -- printed on the receipt; reprints identically (E.46)
  raw_response  TEXT NOT NULL,
  cleared_at    TEXT NOT NULL,
  environment   TEXT NOT NULL CHECK (environment IN ('production','mock'))  -- E.28
);

-- Rejections surface the ISTD error VERBATIM. The local sale is never mutated;
-- amount corrections are a credit note plus a new invoice (E.25).
CREATE TABLE fiscal_dead_letter (
  id            BLOB PRIMARY KEY,
  queue_id      BLOB NOT NULL REFERENCES fiscal_queue(id),
  error_code    TEXT,
  error_body    TEXT NOT NULL,
  failed_check  TEXT,                       -- set when OUR pre-flight caught it (C-2, C-3)
  occurred_at   TEXT NOT NULL,
  resolved_at   TEXT,
  resolved_by   BLOB REFERENCES app_user(id),
  resolution    TEXT CHECK (resolution IN ('requeued','superseded','written_off'))
);

CREATE TABLE fiscal_credentials_ref (       -- POINTER only. Secrets live in the keyring.
  store_id      BLOB PRIMARY KEY REFERENCES store(id),
  keyring_entry TEXT NOT NULL,
  client_id_hint TEXT,                      -- last 4 chars, for the diagnostics screen
  tin           TEXT NOT NULL,
  income_source_sequence TEXT NOT NULL,
  environment   TEXT NOT NULL CHECK (environment IN ('production','mock')),
  updated_at    TEXT NOT NULL
);
```

---

## 0010 — customers and loyalty  ·  Phase 3, microsteps 3.4.1–3.4.2

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
  version       INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_customer_phone_live ON customer(phone)
  WHERE phone IS NOT NULL AND deleted_at IS NULL AND is_anonymized = 0;

-- Consent is a RECORD, not a boolean: timestamp + the wording version consented
-- to. "We had consent" must be provable years later (PDPL).
CREATE TABLE consent (
  id            BLOB PRIMARY KEY,
  customer_id   BLOB NOT NULL REFERENCES customer(id),
  kind          TEXT NOT NULL CHECK (kind IN ('loyalty_terms','marketing','data_processing')),
  text_version  TEXT NOT NULL,
  granted       INTEGER NOT NULL,
  captured_by   BLOB REFERENCES app_user(id),
  captured_at   TEXT NOT NULL,
  channel       TEXT NOT NULL CHECK (channel IN ('register','backoffice','web'))
);
CREATE INDEX idx_consent_customer_kind ON consent(customer_id, kind, captured_at);

-- Append-only ledger. Balance = Σ points_delta. Conflict-free across offline
-- registers, exactly like stock and cash (master plan C.8).
CREATE TABLE loyalty_ledger (
  id            BLOB PRIMARY KEY,
  customer_id   BLOB NOT NULL REFERENCES customer(id),
  points_delta  INTEGER NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('earn','redeem','adjust','expire')),
  ref_kind      TEXT, ref_id BLOB,
  actor_id      BLOB REFERENCES app_user(id),
  reason        TEXT,
  occurred_at   TEXT NOT NULL
);
CREATE INDEX idx_loyalty_customer ON loyalty_ledger(customer_id, occurred_at);

CREATE TABLE loyalty_balance_cache (        -- rebuildable, like stock_cache
  customer_id BLOB PRIMARY KEY REFERENCES customer(id),
  points      INTEGER NOT NULL DEFAULT 0,
  updated_at  TEXT NOT NULL
);

-- Stored value & store credit (J.1). Online-authorize-only by default (E.61).
CREATE TABLE stored_value_ledger (
  id            BLOB PRIMARY KEY,
  instrument_id BLOB NOT NULL,
  customer_id   BLOB REFERENCES customer(id),
  amount_minor  INTEGER NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('issue','topup','redeem','expire','adjust')),
  ref_kind TEXT, ref_id BLOB,
  occurred_at   TEXT NOT NULL
);
```

---

## 0011 — pricing, promotions, supply  ·  Phase 4, microsteps 4.1.1, 4.2.1, 4.4.1

```sql
CREATE TABLE price_list (
  id         BLOB PRIMARY KEY,
  store_id   BLOB REFERENCES store(id),     -- NULL = org base
  name       TEXT NOT NULL,
  valid_from TEXT, valid_to TEXT,
  priority   INTEGER NOT NULL DEFAULT 0,
  deleted_at TEXT, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE price (
  id            BLOB PRIMARY KEY,
  price_list_id BLOB NOT NULL REFERENCES price_list(id),
  product_id    BLOB NOT NULL REFERENCES product(id),
  unit_minor    INTEGER NOT NULL,
  deleted_at TEXT, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 0,
  UNIQUE (price_list_id, product_id)
);
-- Resolution order: promotion > store price list > base price (C.1).

CREATE TABLE promotion (
  id          BLOB PRIMARY KEY,
  code        TEXT UNIQUE,
  name_ar     TEXT NOT NULL, name_en TEXT,
  kind        TEXT NOT NULL CHECK (kind IN (
                'percent_off','amount_off','multibuy','mix_match','basket_threshold')),
  config_json TEXT NOT NULL,
  priority    INTEGER NOT NULL DEFAULT 0,    -- best single promotion per line wins
  valid_from TEXT, valid_to TEXT,
  time_of_day_json TEXT,                     -- happy hour
  store_scope BLOB REFERENCES store(id),
  customer_group TEXT,
  is_active   INTEGER NOT NULL DEFAULT 1,
  deleted_at TEXT, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE supplier (
  id BLOB PRIMARY KEY, name TEXT NOT NULL, phone TEXT, email TEXT, tin TEXT,
  deleted_at TEXT, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE goods_receipt (
  id BLOB PRIMARY KEY, store_id BLOB NOT NULL REFERENCES store(id),
  supplier_id BLOB REFERENCES supplier(id),
  reference TEXT, received_by BLOB NOT NULL REFERENCES app_user(id),
  received_at TEXT NOT NULL, business_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'posted' CHECK (status IN ('draft','posted'))
);
CREATE TABLE goods_receipt_line (
  id BLOB PRIMARY KEY, receipt_id BLOB NOT NULL REFERENCES goods_receipt(id),
  product_id BLOB NOT NULL REFERENCES product(id),
  qty_milli INTEGER NOT NULL, unit_cost_minor INTEGER NOT NULL,
  cost_confirmed INTEGER NOT NULL DEFAULT 0   -- deviation guard (E.43)
);

CREATE TABLE stock_count (
  id BLOB PRIMARY KEY, store_id BLOB NOT NULL REFERENCES store(id),
  started_at TEXT NOT NULL, started_by BLOB NOT NULL REFERENCES app_user(id),
  posted_at TEXT, posted_by BLOB REFERENCES app_user(id),
  scope TEXT NOT NULL DEFAULT 'full' CHECK (scope IN ('full','category','partial'))
);
CREATE TABLE stock_count_line (
  id BLOB PRIMARY KEY, count_id BLOB NOT NULL REFERENCES stock_count(id),
  product_id BLOB NOT NULL REFERENCES product(id),
  expected_milli INTEGER NOT NULL,      -- snapshot at count START; sales mid-count are fine (E.42)
  counted_milli INTEGER,
  variance_milli INTEGER
);

CREATE TABLE transfer (
  id BLOB PRIMARY KEY,
  from_store BLOB NOT NULL REFERENCES store(id),
  to_store   BLOB NOT NULL REFERENCES store(id),
  status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','in_transit','received','cancelled')),
  sent_at TEXT, received_at TEXT
);
CREATE TABLE transfer_line (
  id BLOB PRIMARY KEY, transfer_id BLOB NOT NULL REFERENCES transfer(id),
  product_id BLOB NOT NULL REFERENCES product(id),
  qty_sent_milli INTEGER NOT NULL,
  qty_received_milli INTEGER            -- short/damaged → adjustment at destination (E.44)
);

-- Price display is actively enforced in Jordan (J.3), so a price change
-- produces a labels-to-reprint worklist.
CREATE TABLE label_reprint_queue (
  id BLOB PRIMARY KEY,
  product_id BLOB NOT NULL REFERENCES product(id),
  store_id BLOB NOT NULL REFERENCES store(id),
  cause TEXT NOT NULL CHECK (cause IN ('price_change','new_product','displayed_price_override')),
  queued_at TEXT NOT NULL, printed_at TEXT
);
```

---

## Postgres mirror

`apps/server/migrations/` mirrors each SQLite migration with the same number and name, applied through sqlx. Semantics are identical; representations differ:

| SQLite | Postgres | Note |
|---|---|---|
| `BLOB` PK (16 bytes) | `UUID` | sqlx maps `uuid::Uuid` natively |
| `INTEGER` money/qty | `BIGINT` | 64-bit either way |
| `TEXT` timestamp | `TIMESTAMPTZ` | server converts on ingest |
| `TEXT` JSON | `JSONB` | queryable server-side |
| `INTEGER` 0/1 | `BOOLEAN` | |
| — | `version BIGINT DEFAULT nextval('change_seq')` | the pull cursor; `change_seq` exists already |
| — | partial indexes on `deleted_at IS NULL` | same intent as SQLite |

**Two rules that keep them honest.** (1) Every server table carrying reference data has a `BEFORE UPDATE` trigger assigning `version = nextval('change_seq')` — the cursor cannot drift because nobody remembers to bump it. (2) Fact tables (`sale`, `sale_line`, `stock_ledger`, `cash_movement`, `audit_log`, `z_report`, `loyalty_ledger`) carry a `REVOKE UPDATE, DELETE` grant on the application role. Immutability enforced by the database, not by discipline (I-4).

---

## Index strategy

Added deliberately; each one exists to serve a named query.

| Index | Serves |
|---|---|
| `idx_barcode_code_live` | scan → product, the hot path (< 100 ms budget) |
| `product_fts` | search-as-you-type (< 50 ms over 50k SKUs) |
| `idx_sale_business_date` *(exists)* | X/Z reports, day reports |
| `idx_sale_shift` | shift close, Z totals |
| `idx_stock_ledger_product_store` | on-hand rebuild, movement report |
| `idx_stock_cache_negative` | negative-stock report (C.7) |
| `idx_outbox_unpushed` *(exists)* | sync drain |
| `idx_fiscal_queue_pending` | fiscal retry loop, uncleared-count badge |
| `idx_refund_link_original` | remaining-refundable check, the E.16 hot path |
| `idx_audit_action_at` / `idx_audit_actor_at` | fraud reports: overrides & refunds by user |
| `idx_consent_customer_kind` | PDPL proof-of-consent lookup |

`EXPLAIN QUERY PLAN` on every hot query is microstep 1.11.3, and the result is committed alongside the query so a regression is visible in a diff.
