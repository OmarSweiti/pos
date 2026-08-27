-- Mirrors SQLite 0003_strict_rebuild_and_catalog_depth.sql (conventions §9 rule 4).
--
-- This server migration extends only the existing product table with 0003's
-- parent-free catalog fields, backfills name_ar, and enforces its row-local
-- unit, quantity-kind, and tobacco sealed-pack rules with CHECK constraints.
-- Phase 3 defers org, store, register, category, tax_category, tax_rule_pack,
-- tax_computation_policy, tax_rate, barcode, setting, sync_commit,
-- fact_commit_member, sale, sale_line, sale_tender, sale_line_tax,
-- sale_line_discount, sale_supply_tax_context, sync_outbox, and sync_cursor;
-- product.category_id, product.tax_category_id and their active-product guard;
-- and the Arabic search projections. The server has none of those parent,
-- fact, delivery, or search structures yet, and creating them before Phase 3's
-- shared-service decision is signed off would omit its required org_id keys,
-- composite tenant foreign keys, tenant-scoped uniqueness, forced RLS, and
-- owner/application role split. SQLite's STRICT staging rebuild is
-- engine-specific: PostgreSQL product is already natively typed and the other
-- five rebuilt register tables do not exist here; sale immutability lands with
-- the server fact tables, as the 0002 mirror records.

ALTER TABLE product
  ADD COLUMN name_ar TEXT,
  ADD COLUMN name_en TEXT,
  ADD COLUMN unit TEXT NOT NULL DEFAULT 'each',
  ADD COLUMN qty_step_milli BIGINT NOT NULL DEFAULT 1000,
  ADD COLUMN is_weighed BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN is_service BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN regulated_kind TEXT,
  ADD COLUMN sale_form TEXT NOT NULL DEFAULT 'sealed_pack',
  ADD COLUMN min_age INTEGER,
  ADD COLUMN max_price_minor BIGINT,
  ADD COLUMN reorder_point_milli BIGINT,
  ADD CONSTRAINT product_unit_valid
    CHECK (unit IN ('each', 'package', 'weight', 'volume', 'length')),
  ADD CONSTRAINT product_qty_step_positive
    CHECK (qty_step_milli > 0),
  ADD CONSTRAINT product_regulated_kind_valid
    CHECK (regulated_kind IN ('tobacco')),
  ADD CONSTRAINT product_sale_form_valid
    CHECK (sale_form IN ('sealed_pack', 'bulk', 'service')),
  ADD CONSTRAINT product_regulated_sale_form
    CHECK (regulated_kind IS DISTINCT FROM 'tobacco' OR sale_form = 'sealed_pack'),
  ADD CONSTRAINT product_quantity_kind
    CHECK (
      (unit IN ('each', 'package') AND qty_step_milli = 1000 AND NOT is_weighed)
      OR (unit IN ('weight', 'volume', 'length') AND is_weighed)
    );

UPDATE product SET name_ar = name WHERE name_ar IS NULL;
