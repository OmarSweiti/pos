-- 0004 — people and audit  ·  Phase 1, microsteps 1.6.1–1.6.4
--
-- Users, the four standard roles, the capability catalogue and the complete
-- (role, capability) decision matrix; sessions; one-use approval handles and
-- the consumption facts that spend them; brute-force throttle state; and the
-- hash-chained audit log with its external checkpoint.
--
-- `app_user`, not `user`: `user` is reserved in PostgreSQL, and the mirror has
-- to carry the same name.
--
-- ── Three shape decisions, settled here because 0004 is never reopened ─────
--
-- Conventions §9: a committed migration is never edited. Anything left for
-- later is left forever, so the three below are answered now.
--
--   1. `role_capability` holds ONE ROW PER CELL — all 128 of them, not only the
--      75 cells a role actually holds. `decision` is NOT NULL with NO DEFAULT,
--      so a cell nobody decided cannot arrive as a silent denial that no query
--      can tell apart from a deliberate one: it is an absent row instead, and
--      the 1.6.3 test comparing this seed with `cap::DEFAULT_MATRIX` fails on
--      it. Three values rather than a boolean, because `Grant::SetsTheLimit` in
--      `pos-domain` is a different answer from `Grant::Withheld`: the owner
--      runs no till, so they cannot apply a manual discount or override a
--      price — what they do is configure the ceiling that bounds the roles
--      which can. A boolean collapses those two into the same blank.
--   2. `role.code` carries a real CHECK. `pos_domain::permissions::Role` is a
--      closed enum of four; the column that mirrors it was open text.
--   3. The four `role.id` values are fixed literals, not `randomblob(16)`. The
--      same logical role must carry the same id on every register: `role` and
--      `role_capability` are server-wins reference tables
--      (ref/sync-protocol.md §1), and two registers that invented different
--      ids for "manager" cannot be reconciled centrally.
--
-- ── The role ids ──────────────────────────────────────────────────────────
--
-- UUIDv7-shaped (RFC 9562), hand-built so they are reproducible:
--
--   bytes 0–5   unix_ts_ms = 0x01A05F6A5800 = 1788307200000 ms
--                          = 2026-09-02T00:00:00.000Z, this migration's own
--                            fixed instant. A UUIDv7 timestamp exists for index
--                            locality and is never the causal authority (I-7),
--                            so pinning it to a constant costs nothing and buys
--                            a value that is the same on every register.
--   byte  6     0x70       = version nibble 7, then rand_a's high 4 bits (zero)
--   byte  7     0x00       = rand_a's low 8 bits (zero)
--   byte  8     0x80       = variant bits 0b10, then rand_b's high 6 bits (zero)
--   bytes 9–14  0x00       = rand_b
--   byte  15    0x01–0x04  = rand_b's last byte, the only thing that differs
--
--   01a05f6a-5800-7000-8000-000000000001  cashier
--   01a05f6a-5800-7000-8000-000000000002  shift_lead
--   01a05f6a-5800-7000-8000-000000000003  manager
--   01a05f6a-5800-7000-8000-000000000004  owner
--
-- ── What `limit_json` holds, and what it deliberately does not ────────────
--
-- The KIND of limit and nothing else: {"kind":"own_shift"}, {"kind":"role_cap"},
-- {"kind":"own_store"}, {"kind":"refund_threshold"}, {"kind":"exact_match_only"}
-- — spelled exactly as `pos_domain::permissions::Limit::as_str` spells it, which
-- is the stable token that documentation already promises this column writes.
-- An object rather than a bare string, so the configured value can join it later
-- without changing the shape of a row nobody may rewrite.
--
-- The numeric ceiling on `discount.manual` is NOT here. It is merchant decision
-- 3.1–3.3 (ref/merchant-decisions.md §C), whose Answer column is still blank and
-- whose owner is microstep 1.4.5. Seeding a guessed 5% would make an unanswered
-- merchant question permanent. When 1.4.5 answers it, the value joins the same
-- object as a sibling key — {"kind":"role_cap","max_percent_ppm":50000} — which
-- is where `role_capability.limit_json`'s documented example lands.

CREATE TABLE capability (
  code          TEXT PRIMARY KEY,
  description   TEXT NOT NULL
) STRICT;

INSERT INTO capability (code, description) VALUES
  ('sale.create', 'Create and finalize an ordinary sale'),
  ('sale.park', 'Park a cart'),
  ('sale.resume', 'Resume a parked cart'),
  ('sale.void', 'Void a non-completed sale'),
  ('sale.reprint', 'Create a linked duplicate receipt'),
  ('sale.department', 'Create an audited open-price department line'),
  ('line.void', 'Remove a priced line before completion'),
  ('discount.manual', 'Apply a deliberate manual discount'),
  ('price.override', 'Override a catalog price with bound approval'),
  ('drawer.open', 'Open the drawer outside an automatic cash-sale effect'),
  ('cash.movement', 'Post a double-entry cash movement'),
  ('shift.open', 'Open a register shift'),
  ('shift.close', 'Close a register shift'),
  ('xreport.run', 'Run a non-closing X report'),
  ('zreport.run', 'Create the immutable closing Z report'),
  ('refund.receipted', 'Refund against an original document'),
  ('refund.receiptless', 'Create a policy-limited receiptless refund'),
  ('refund.above_threshold', 'Approve a refund above the configured threshold'),
  ('refund.cash_for_card', 'Approve cash settlement of an original card tender'),
  ('refund.outside_window', 'Approve a defective claim outside the store goodwill window'),
  ('stock.adjust', 'Post a stock correction event'),
  ('product.edit', 'Edit mutable catalog reference data'),
  ('tax.rate.edit', 'Create a new source-backed tax-rate version'),
  ('fiscal.remediate', 'Rebuild a locally failed fiscal payload after correction'),
  ('customer.lookup', 'Look up minimized customer data'),
  ('journal.view', 'Read the sales journal'),
  ('reports.own', 'Read the actor own reports'),
  ('reports.all', 'Read store-wide reports'),
  ('training_mode.toggle', 'Enter or leave visibly marked training mode'),
  ('settings.edit', 'Edit merchant settings'),
  ('user.admin', 'Administer merchant users and roles'),
  ('backup.restore', 'Enter the out-of-band recovery flow');

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
) STRICT;

CREATE TABLE role (
  id          BLOB PRIMARY KEY,
  code        TEXT NOT NULL UNIQUE         -- cashier|shift_lead|manager|owner
                CHECK (code IN ('cashier','shift_lead','manager','owner')),
  name_ar     TEXT NOT NULL,
  name_en     TEXT,
  deleted_at  TEXT,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  version     INTEGER NOT NULL DEFAULT 0
) STRICT;

-- Arabic first, because Arabic is the product and English is the toggle
-- (conventions §10). The ids are the fixed UUIDv7-shaped literals the header
-- lays out byte by byte.
INSERT INTO role (id, code, name_ar, name_en) VALUES
  (X'01A05F6A580070008000000000000001', 'cashier', 'أمين صندوق', 'Cashier'),
  (X'01A05F6A580070008000000000000002', 'shift_lead', 'مسؤول وردية', 'Shift lead'),
  (X'01A05F6A580070008000000000000003', 'manager', 'مدير', 'Manager'),
  (X'01A05F6A580070008000000000000004', 'owner', 'مالك', 'Owner');

CREATE TABLE role_capability (
  role_id     BLOB NOT NULL REFERENCES role(id),
  capability  TEXT NOT NULL REFERENCES capability(code),
  -- One row per cell. NOT NULL and no DEFAULT, so an undecided cell is an
  -- absent row and not a denial nothing can distinguish from a decision.
  decision    TEXT NOT NULL
                CHECK (decision IN ('granted','withheld','sets_the_limit')),
  -- The limit's kind, as pos-domain spells it: {"kind":"own_shift"}. The
  -- merchant-configured value joins it later as a sibling key, which is where
  -- e.g. {"max_percent_ppm":50000} lands. Both are set out in the header.
  limit_json  TEXT,
  -- A denial carrying a limit is nonsense, and would read as a bounded grant to
  -- anything that reached for `limit_json` before `decision`.
  CHECK (decision = 'granted' OR limit_json IS NULL),
  PRIMARY KEY (role_id, capability)
) STRICT;

-- ref/domain-api.md §8.2, row for row, in `pos_domain::permissions::cap::ALL`'s
-- declaration order. Four rows per capability — cashier, shift_lead, manager,
-- owner, ids …0001 through …0004 — so the grid reads straight down against that
-- table. All 128 cells are here.
-- `crates/pos-db/tests/migration_0004_people_and_audit.rs` proves every role
-- carries an explicit decision for every capability; microstep 1.6.3's deferred
-- half (`crates/pos-db/tests/role_matrix.rs`, not written yet) is what will
-- compare each seeded cell with `cap::DEFAULT_MATRIX`.
INSERT INTO role_capability (role_id, capability, decision, limit_json) VALUES
  (X'01A05F6A580070008000000000000001', 'sale.create',            'granted',        NULL),
  (X'01A05F6A580070008000000000000002', 'sale.create',            'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'sale.create',            'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'sale.create',            'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'sale.park',              'granted',        NULL),
  (X'01A05F6A580070008000000000000002', 'sale.park',              'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'sale.park',              'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'sale.park',              'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'sale.resume',            'granted',        NULL),
  (X'01A05F6A580070008000000000000002', 'sale.resume',            'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'sale.resume',            'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'sale.resume',            'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'sale.void',              'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'sale.void',              'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'sale.void',              'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'sale.void',              'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'sale.reprint',           'granted',        NULL),
  (X'01A05F6A580070008000000000000002', 'sale.reprint',           'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'sale.reprint',           'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'sale.reprint',           'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'sale.department',        'granted',        NULL),
  (X'01A05F6A580070008000000000000002', 'sale.department',        'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'sale.department',        'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'sale.department',        'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'line.void',              'granted',        NULL),
  (X'01A05F6A580070008000000000000002', 'line.void',              'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'line.void',              'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'line.void',              'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'discount.manual',        'granted',        '{"kind":"role_cap"}'),
  (X'01A05F6A580070008000000000000002', 'discount.manual',        'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'discount.manual',        'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'discount.manual',        'sets_the_limit', NULL),

  (X'01A05F6A580070008000000000000001', 'price.override',         'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'price.override',         'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'price.override',         'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'price.override',         'sets_the_limit', NULL),

  (X'01A05F6A580070008000000000000001', 'refund.receipted',       'granted',        '{"kind":"refund_threshold"}'),
  (X'01A05F6A580070008000000000000002', 'refund.receipted',       'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'refund.receipted',       'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'refund.receipted',       'sets_the_limit', NULL),

  (X'01A05F6A580070008000000000000001', 'refund.above_threshold', 'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'refund.above_threshold', 'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'refund.above_threshold', 'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'refund.above_threshold', 'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'refund.receiptless',     'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'refund.receiptless',     'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'refund.receiptless',     'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'refund.receiptless',     'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'refund.cash_for_card',   'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'refund.cash_for_card',   'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'refund.cash_for_card',   'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'refund.cash_for_card',   'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'refund.outside_window',  'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'refund.outside_window',  'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'refund.outside_window',  'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'refund.outside_window',  'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'drawer.open',            'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'drawer.open',            'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'drawer.open',            'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'drawer.open',            'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'cash.movement',          'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'cash.movement',          'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'cash.movement',          'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'cash.movement',          'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'shift.open',             'granted',        '{"kind":"own_shift"}'),
  (X'01A05F6A580070008000000000000002', 'shift.open',             'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'shift.open',             'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'shift.open',             'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'shift.close',            'granted',        '{"kind":"own_shift"}'),
  (X'01A05F6A580070008000000000000002', 'shift.close',            'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'shift.close',            'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'shift.close',            'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'xreport.run',            'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'xreport.run',            'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'xreport.run',            'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'xreport.run',            'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'zreport.run',            'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'zreport.run',            'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'zreport.run',            'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'zreport.run',            'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'journal.view',           'granted',        '{"kind":"own_shift"}'),
  (X'01A05F6A580070008000000000000002', 'journal.view',           'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'journal.view',           'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'journal.view',           'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'stock.adjust',           'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'stock.adjust',           'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'stock.adjust',           'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'stock.adjust',           'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'product.edit',           'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'product.edit',           'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'product.edit',           'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'product.edit',           'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'tax.rate.edit',          'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'tax.rate.edit',          'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'tax.rate.edit',          'withheld',       NULL),
  (X'01A05F6A580070008000000000000004', 'tax.rate.edit',          'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'fiscal.remediate',       'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'fiscal.remediate',       'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'fiscal.remediate',       'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'fiscal.remediate',       'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'customer.lookup',        'granted',        '{"kind":"exact_match_only"}'),
  (X'01A05F6A580070008000000000000002', 'customer.lookup',        'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'customer.lookup',        'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'customer.lookup',        'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'training_mode.toggle',   'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'training_mode.toggle',   'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'training_mode.toggle',   'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'training_mode.toggle',   'withheld',       NULL),

  (X'01A05F6A580070008000000000000001', 'settings.edit',          'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'settings.edit',          'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'settings.edit',          'granted',        '{"kind":"own_store"}'),
  (X'01A05F6A580070008000000000000004', 'settings.edit',          'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'user.admin',             'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'user.admin',             'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'user.admin',             'granted',        '{"kind":"own_store"}'),
  (X'01A05F6A580070008000000000000004', 'user.admin',             'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'backup.restore',         'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'backup.restore',         'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'backup.restore',         'withheld',       NULL),
  (X'01A05F6A580070008000000000000004', 'backup.restore',         'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'reports.own',            'granted',        NULL),
  (X'01A05F6A580070008000000000000002', 'reports.own',            'granted',        NULL),
  (X'01A05F6A580070008000000000000003', 'reports.own',            'granted',        NULL),
  (X'01A05F6A580070008000000000000004', 'reports.own',            'granted',        NULL),

  (X'01A05F6A580070008000000000000001', 'reports.all',            'withheld',       NULL),
  (X'01A05F6A580070008000000000000002', 'reports.all',            'withheld',       NULL),
  (X'01A05F6A580070008000000000000003', 'reports.all',            'granted',        '{"kind":"own_store"}'),
  (X'01A05F6A580070008000000000000004', 'reports.all',            'granted',        NULL);

-- `store_id` is NULL for an org-wide grant, and that NULL is load-bearing: it is
-- how an owner or an area manager holds a role across every store.
--
-- It therefore cannot be part of the PRIMARY KEY of a STRICT table. SQLite makes
-- every primary-key component of a STRICT table implicitly NOT NULL — composite
-- keys included — so `PRIMARY KEY (user_id, role_id, store_id)` under STRICT
-- makes an org-wide grant impossible to insert. That is exactly what happened
-- when STRICT was applied here, and it is why the key is gone.
--
-- Two partial unique indexes give back what the key was for, without forbidding
-- the NULL: one grant per (user, role, store), and one org-wide grant per
-- (user, role). A plain unique index over the triple would not do, because SQLite
-- treats NULLs as distinct and would allow the same org-wide grant twice. Same
-- idiom as `idx_barcode_code_live` and `idx_shift_one_open` above.
CREATE TABLE user_role (
  user_id   BLOB NOT NULL REFERENCES app_user(id),
  role_id   BLOB NOT NULL REFERENCES role(id),
  store_id  BLOB REFERENCES store(id)      -- NULL = org-wide
) STRICT;
CREATE UNIQUE INDEX idx_user_role_scoped ON user_role(user_id, role_id, store_id)
  WHERE store_id IS NOT NULL;
CREATE UNIQUE INDEX idx_user_role_org_wide ON user_role(user_id, role_id)
  WHERE store_id IS NULL;

CREATE TABLE user_session (
  id           BLOB PRIMARY KEY,
  user_id      BLOB NOT NULL REFERENCES app_user(id),
  register_id  BLOB NOT NULL REFERENCES register(id),
  started_at   TEXT NOT NULL,
  ended_at     TEXT,
  end_reason   TEXT CHECK (end_reason IN ('logout','idle_lock','switch_user','shift_close','crash'))
) STRICT;

-- A runtime approval is one-use evidence bound to the exact effect. Keeping the
-- handle and its separate consumption fact makes restart safe: replay is refused
-- by the primary key, and consumption commits in the same transaction as the
-- financial effect and its audit row.
CREATE TABLE approval_handle (
  id            BLOB PRIMARY KEY,
  capability    TEXT NOT NULL REFERENCES capability(code),
  actor_id      BLOB NOT NULL REFERENCES app_user(id),
  approver_id   BLOB NOT NULL REFERENCES app_user(id),
  entity_id     BLOB NOT NULL,
  amount_minor  INTEGER NOT NULL,
  -- Present for a prepared-intent command and absent otherwise. This is the
  -- BLAKE3 digest of the versioned, domain-separated canonical intent bytes;
  -- the webview never supplies it.
  content_hash  BLOB,
  reason        TEXT NOT NULL,
  issued_at     TEXT NOT NULL,
  expires_at    TEXT NOT NULL,
  nonce         BLOB NOT NULL UNIQUE,
  CHECK (actor_id <> approver_id),
  CHECK (content_hash IS NULL OR length(content_hash) = 32),
  CHECK (expires_at > issued_at)
) STRICT;

CREATE TABLE approval_consumption (
  handle_id     BLOB PRIMARY KEY REFERENCES approval_handle(id),
  effect_id     BLOB NOT NULL,
  audit_log_id  BLOB NOT NULL UNIQUE REFERENCES audit_log(id),
  consumed_at   TEXT NOT NULL
) STRICT;

CREATE TRIGGER approval_handle_no_update
BEFORE UPDATE ON approval_handle BEGIN
  SELECT RAISE(ABORT, 'ApprovalHandle is immutable — consume it once with the effect');
END;
CREATE TRIGGER approval_handle_no_delete
BEFORE DELETE ON approval_handle BEGIN
  SELECT RAISE(ABORT, 'ApprovalHandle is audit evidence and cannot be deleted');
END;
CREATE TRIGGER approval_consumption_no_update
BEFORE UPDATE ON approval_consumption BEGIN
  SELECT RAISE(ABORT, 'an approval consumption fact is immutable');
END;
CREATE TRIGGER approval_consumption_no_delete
BEFORE DELETE ON approval_consumption BEGIN
  SELECT RAISE(ABORT, 'an approval consumption fact cannot be deleted');
END;

-- Brute-force throttling survives process restart. This is mutable security
-- state, not a fact; successful recovery resets it through a named repository
-- operation that also writes the audit event.
CREATE TABLE auth_attempt_state (
  user_id             BLOB NOT NULL REFERENCES app_user(id),
  register_id         BLOB NOT NULL REFERENCES register(id),
  failed_attempts     INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
  delay_until_at      TEXT,
  locked_until_at     TEXT,
  last_attempt_at     TEXT NOT NULL,
  PRIMARY KEY (user_id, register_id)
) STRICT;

-- Hash-chained (G-7). hash = BLAKE3(prev_hash ‖ canonical_bytes(entry)).
-- Canonical bytes cover every immutable persisted field, including `id`,
-- `register_id` and `canonical_version`; changing row identity is therefore a
-- chain break rather than an unhashed reattribution. Append-only forever.
CREATE TABLE audit_log (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  id          BLOB NOT NULL UNIQUE,
  canonical_version INTEGER NOT NULL DEFAULT 1 CHECK (canonical_version > 0),
  register_id BLOB NOT NULL,
  actor_id    BLOB NOT NULL,
  approver_id BLOB,                        -- distinct from actor on escalation (E.52)
  approval_handle_id BLOB REFERENCES approval_handle(id),
  action      TEXT NOT NULL,
  entity      TEXT NOT NULL,
  entity_id   BLOB,
  reason      TEXT,
  payload     TEXT NOT NULL,               -- canonical JSON. NEVER PII or card data.
  prev_hash   BLOB NOT NULL,
  hash        BLOB NOT NULL,
  at          TEXT NOT NULL
) STRICT;
CREATE INDEX idx_audit_action_at ON audit_log(action, at);
CREATE INDEX idx_audit_actor_at  ON audit_log(actor_id, at);
CREATE UNIQUE INDEX idx_audit_approval_once ON audit_log(approval_handle_id)
  WHERE approval_handle_id IS NOT NULL;

-- Consumption is not a free-standing "used" bit. It names the one audit row
-- that proves the same actor, approver, capability, entity, amount and reason
-- were consumed with the financial effect. The command transaction inserts the
-- effect, audit row and this row together; a rollback removes all three.
CREATE TRIGGER approval_consumption_matches_handle_and_audit
BEFORE INSERT ON approval_consumption
WHEN NOT EXISTS (
  SELECT 1
    FROM approval_handle h JOIN audit_log a ON a.id = NEW.audit_log_id
   WHERE h.id = NEW.handle_id
     AND NEW.effect_id = h.entity_id
     AND NEW.consumed_at >= h.issued_at
     AND NEW.consumed_at < h.expires_at
     AND a.approval_handle_id = h.id
     AND a.actor_id = h.actor_id
     AND a.approver_id = h.approver_id
     AND a.entity_id = h.entity_id
     AND a.action = h.capability
     AND a.reason = h.reason
     AND json_type(a.payload, '$.amount_minor') = 'integer'
     AND json_extract(a.payload, '$.amount_minor') = h.amount_minor)
BEGIN
  SELECT RAISE(ABORT, 'approval consumption must match one bound financial effect and audit row');
END;

-- A chain alone cannot detect deletion of its current tail or a full re-chain.
-- Z close and each verified backup export this checkpoint; Phase 3 also stores
-- it on the server. Verification compares the database head with an anchor that
-- was already outside the database before claiming the chain is intact.
CREATE TABLE audit_checkpoint (
  id            BLOB PRIMARY KEY,
  register_id   BLOB NOT NULL REFERENCES register(id),
  last_seq      INTEGER NOT NULL CHECK (last_seq >= 0),
  last_hash     BLOB NOT NULL,
  source_kind   TEXT NOT NULL
                  CHECK (source_kind IN ('z_report','verified_backup','server')),
  anchor_ref    TEXT NOT NULL,
  anchored_at   TEXT NOT NULL,
  UNIQUE (register_id, last_seq, last_hash, source_kind)
) STRICT;

CREATE TRIGGER approval_handle_has_ready_commit
BEFORE INSERT ON approval_handle
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'approval_handle' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'approval issuance requires its complete delivery envelope'); END;
CREATE TRIGGER approval_consumption_has_ready_commit
BEFORE INSERT ON approval_consumption
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'approval_consumption' AND m.entity_id = NEW.handle_id)
BEGIN SELECT RAISE(ABORT, 'approval consumption requires its complete delivery envelope'); END;
CREATE TRIGGER audit_log_has_ready_commit
BEFORE INSERT ON audit_log
WHEN NOT EXISTS (
  SELECT 1 FROM fact_commit_member m JOIN sync_commit_ready ready ON ready.id = m.commit_id
   WHERE m.entity = 'audit_log' AND m.entity_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'audit fact requires its complete delivery envelope'); END;

-- ── audit_log is append-only, and now says so in something other than prose ──
--
-- The DDL above already asserts "Append-only: no UPDATE, no DELETE, ever".
-- Nothing enforced it, which made the only forensic control in the design the
-- one control an insider could edit. The hash chain detects a modified row but
-- cannot detect a deleted tail, so DELETE is the attack that matters most.

CREATE TRIGGER audit_log_no_update
BEFORE UPDATE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'I-4: audit_log is append-only — no UPDATE, ever');
END;

CREATE TRIGGER audit_log_no_delete
BEFORE DELETE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'I-4: audit_log is append-only — no DELETE, ever');
END;

CREATE TRIGGER audit_checkpoint_no_update
BEFORE UPDATE ON audit_checkpoint BEGIN
  SELECT RAISE(ABORT, 'an audit checkpoint is append-only');
END;
CREATE TRIGGER audit_checkpoint_no_delete
BEFORE DELETE ON audit_checkpoint BEGIN
  SELECT RAISE(ABORT, 'an audit checkpoint cannot be deleted');
END;

-- This migration owns the capability catalogue, the four standard roles and
-- every one of their 128 (role, capability) decisions. Provisioning creates the
-- merchant's users and their `user_role` grants against these rows; it does not
-- invent a role, because a role invented per install cannot be reconciled across
-- registers. A later authorization microstep verifies the typed registry against
-- what is seeded here; it never edits 0004 to append a capability the plan
-- already knew about.

