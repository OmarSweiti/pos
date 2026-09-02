-- Mirrors SQLite 0004_people_and_audit.sql (conventions §9 rule 4).
--
-- The server's half of 0004 is the immutable vendor capability catalogue and
-- nothing else. `ref/schema.md`'s machine-readable exception inventory —
-- `<!-- postgres-global-tables: _sqlx_migrations, capability -->` — names
-- `capability` as one of the two globally owned server tables, so it can be
-- created and seeded now without answering any tenant question.
--
-- Everything else in 0004 is deferred, and the reasons are not the same reason.
--
-- Tenant-owned, and blocked on the shared multi-tenant decision that
-- ref/schema.md §"Shared multi-tenant decision for sign-off" records as still
-- awaiting owner sign-off "before the first Phase-3 server migration": role,
-- role_capability, app_user, user_role, user_session, auth_attempt_state.
-- Creating them now would omit the org_id UUID NOT NULL column, the
-- UNIQUE (org_id, id) parent keys, the composite tenant foreign keys, the
-- tenant-scoped UNIQUE (org_id, code) that section reserves for user and role
-- codes, forced row-level security and the owner/application role split — and a
-- committed migration cannot be reopened to add them. The 0003 mirror deferred
-- every tenant-bearing structure for exactly this reason and nothing has
-- changed since. Three of them also have no parent to point at yet: app_user
-- references org, user_role references store, and auth_attempt_state references
-- register, none of which the server has.
--
-- Fact tables, deferred with the rest of the server fact spine: approval_handle,
-- approval_consumption, audit_log, audit_checkpoint. Their SQLite guards —
-- approval_handle_no_update, approval_handle_no_delete,
-- approval_consumption_no_update, approval_consumption_no_delete,
-- approval_consumption_matches_handle_and_audit, approval_handle_has_ready_commit,
-- approval_consumption_has_ready_commit, audit_log_has_ready_commit,
-- audit_log_no_update, audit_log_no_delete, audit_checkpoint_no_update and
-- audit_checkpoint_no_delete — plus idx_user_role_scoped,
-- idx_user_role_org_wide, idx_audit_action_at, idx_audit_actor_at and
-- idx_audit_approval_once come with them. The three has_ready_commit triggers
-- have nothing to interrogate on this server: sync_commit, fact_commit_member
-- and sync_outbox do not exist here either. On PostgreSQL the append-only half
-- is REVOKE UPDATE on the fact tables rather than a BEFORE UPDATE trigger, and
-- that revocation is part of the same unsigned role split. The 0002 mirror made
-- the same call in the same words: sale immutability "lands with the server
-- fact tables".
--
-- The register's 128 role_capability rows are therefore not mirrored yet. They
-- are the vendor default a merchant may edit under `user.admin`, which is what
-- makes the table tenant-owned rather than a second global catalogue.
--
-- `capability` deliberately carries no `version`/`updated_at` pair and no
-- assign_change_version trigger. The SQLite table has neither column, so adding
-- them only on the server would be the undeclared divergence rule 4 exists to
-- refuse. Reference-table pull for this catalogue lands with the Phase-3
-- reference-sync work, on both engines at once.

CREATE TABLE capability (
  code         TEXT PRIMARY KEY,
  description  TEXT NOT NULL
);

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
