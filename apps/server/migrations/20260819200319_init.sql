-- Server-side mirror of the catalog + the global change sequence that powers
-- pull-sync (blueprint §4: server-assigned monotonically increasing version).

CREATE TABLE product (
  id           UUID PRIMARY KEY,
  sku          TEXT NOT NULL UNIQUE,
  name         TEXT NOT NULL,
  price_minor  BIGINT NOT NULL,
  currency     TEXT NOT NULL,
  is_active    BOOLEAN NOT NULL DEFAULT TRUE,
  deleted_at   TIMESTAMPTZ,
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  version      BIGINT NOT NULL DEFAULT 0
);

-- One global sequence; every mutated row gets the next value in its `version`.
CREATE SEQUENCE change_seq;
