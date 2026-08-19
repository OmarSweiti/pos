-- Phase 0 subset of blueprint §3. Full schema (tax, shifts, cash, users,
-- audit chain, customers) lands as migrations 0002+ during Phase 0/1.
-- Conventions: UUIDv7 as 16-byte BLOB PKs; money as INTEGER minor units;
-- ISO-8601 TEXT timestamps; soft-delete tombstones on synced reference data.

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
);

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
);

CREATE TABLE sale_line (
  id               BLOB PRIMARY KEY,
  sale_id          BLOB NOT NULL REFERENCES sale(id),
  product_id       BLOB NOT NULL REFERENCES product(id),
  qty              INTEGER NOT NULL,
  unit_price_minor INTEGER NOT NULL,
  discount_minor   INTEGER NOT NULL DEFAULT 0,
  tax_minor        INTEGER NOT NULL DEFAULT 0,
  total_minor      INTEGER NOT NULL
);

CREATE TABLE sale_tender (
  id           BLOB PRIMARY KEY,
  sale_id      BLOB NOT NULL REFERENCES sale(id),
  method       TEXT NOT NULL,
  amount_minor INTEGER NOT NULL,
  psp_ref      TEXT,
  change_minor INTEGER NOT NULL DEFAULT 0
);

-- Transactional outbox (blueprint §4): every local write appends here
-- IN THE SAME TRANSACTION as the write itself.
CREATE TABLE sync_outbox (
  seq        INTEGER PRIMARY KEY AUTOINCREMENT,
  entity     TEXT NOT NULL,
  entity_id  BLOB NOT NULL,
  op         TEXT NOT NULL,
  payload    TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  pushed_at  TEXT
);

CREATE TABLE sync_cursor (
  entity         TEXT PRIMARY KEY,
  server_version INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_sale_business_date ON sale(business_date);
CREATE INDEX idx_outbox_unpushed    ON sync_outbox(pushed_at) WHERE pushed_at IS NULL;
