-- Mirrors SQLite 0002_sale_integrity.sql (conventions §9 rule 4).
--
-- The register's half of 0002 is trigger-enforced sale immutability; the server
-- has no sale tables yet, so that half lands with them (ref/schema.md §"Postgres
-- mirror"). What the server CAN fix now is the half that is already wrong here.
--
-- 20260819200319_init created change_seq under the comment "every mutated row
-- gets the next value in its `version`" — and then nothing assigned it. The
-- column defaulted to 0 and stayed there. A comment describing a mechanism that
-- was never built is worse than no comment at all: pull-sync (blueprint §4,
-- pos-sync::PullRequest.after) would have shipped against a cursor that never
-- advances, and every register would pull the catalog either endlessly or never.

CREATE OR REPLACE FUNCTION assign_change_version() RETURNS trigger AS $$
BEGIN
  -- One global sequence across every synced entity, so a register holds one
  -- cursor per entity and they never collide (I-7: ordering is server-assigned,
  -- never a device clock).
  NEW.version    := nextval('change_seq');
  NEW.updated_at := now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER product_assign_change_version
BEFORE INSERT OR UPDATE ON product
FOR EACH ROW EXECUTE FUNCTION assign_change_version();

-- The pull is `WHERE version > $after ORDER BY version LIMIT $limit`. Without
-- this index that is a sequential scan of the whole catalog on every poll from
-- every register.
CREATE INDEX idx_product_version ON product (version);
