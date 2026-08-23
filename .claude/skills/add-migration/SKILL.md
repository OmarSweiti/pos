---
name: add-migration
description: Add a forward-only POS database migration with correct runtime registration, engine-specific scope, behavioral coverage, and verification. Use when implementing a schema change; do not use merely to inspect or explain the current schema.
---

# Add a migration

Preserve the repository's append-only migration history and keep the register and
server schemas semantically aligned.

## Establish scope before editing

1. Read `docs/implementation/01-conventions.md` section 9 and
   `.claude/rules/sql-migrations.md` in full. Locate the active microstep and read
   its `Files`, `Tests`, and `Done when` requirements plus the relevant sections
   of `docs/implementation/ref/schema.md`; do not load the entire schema reference
   when a focused section is sufficient.
2. Use `git ls-tree -r --name-only HEAD` to establish which migration files are
   committed. A file listed in `HEAD` is immutable, including its path and name.
3. Inspect both `crates/pos-db/migrations/` and `apps/server/migrations/`; derive
   the next names from the current tree rather than from agent documentation.
4. Classify the change before creating files:
   - **SQLite-led and synchronized:** add the register migration and a PostgreSQL
     mirror with equivalent behavior.
   - **Register-local:** add SQLite only and record the reason in `REGISTER_LOCAL`
     in `scripts/verify-pg-migrations.py`.
   - **Server-only:** add PostgreSQL only with a `-- Server-only: <reason>` header.
     Do not invent a register migration merely to make the mapping symmetrical.
5. If the tree, active microstep, and schema reference disagree about numbering,
   scope, tests, or shape, preserve committed history and report the conflict.
   Do not silently choose whichever document is easiest to satisfy.

## Required result

- For a SQLite-led or register-local change, add a new
  `crates/pos-db/migrations/NNNN_short_name.sql`; never edit, delete, rename, or
  replace a committed migration. A server-only change creates no SQLite file.
- Create migration entries as repository-owned regular SQL files. Do not use a
  symlink, gitlink, device, or other filesystem indirection in either migration
  tree.
- Follow the repository's unit/type naming rules. Money is `*_minor`, quantities
  are `*_milli`, rates are `*_ppm`, and IDs use the documented representation.
  SQLite timestamps are UTC text `*_at`; PostgreSQL uses the mapped native types
  documented for the server. Never introduce floating-point money.
- For a SQLite migration, append the file to `MIGRATIONS` in
  `crates/pos-db/src/lib.rs` in the same change. Array position is the runtime
  version; do not insert or reorder entries. The schema verifier requires exact,
  ordered parity between that array and every migration file on disk.
- For a SQLite-led change, add the PostgreSQL migration with the same semantics
  and its mapping header. For a register-local change, update `REGISTER_LOCAL`
  with the reason and add no PostgreSQL file. For a server-only change, add only
  the PostgreSQL migration with its `-- Server-only: <reason>` header.
- Name every PostgreSQL migration `<14-digit UTC timestamp>_<lower_snake>.sql`.
  The sqlx version prefix must be a valid calendar timestamp, unique in the
  directory, and later than every existing version; never reuse a timestamp.
- Keep each PostgreSQL migration transactional. Use SQLx's case-sensitive,
  byte-zero `-- no-transaction` marker only when PostgreSQL forbids the required
  statement inside a transaction, and add an explicit partial-failure recovery
  test or procedure.
- Add every behavioral test required by the active microstep. When existing rows
  change shape or meaning, put the data transition in the migration and add a
  regression test that seeds the old shape, migrates, and asserts the result.
- Before rebuilding a table, inventory its indexes, triggers, views, foreign
  keys, and constraints, then recreate and test every dependent object that must
  survive the rebuild.
- Keep `docs/implementation/ref/schema.md` aligned with the implemented shape.
  If the requested design differs from that reference, update the reference in
  the same authorized change or report the unresolved design conflict.
- Treat a declared mapping and successful SQL execution as necessary but not
  sufficient evidence of cross-engine equivalence. Review representation and
  behavior differences explicitly (`BLOB`/UUID, integer widths and booleans,
  text timestamps/`TIMESTAMPTZ`, JSON, triggers, indexes, grants, and reference
  data/version behavior where applicable).
- Preserve completed-sale immutability and fact/outbox transaction atomicity.

## Verify

Run the focused checks first, followed by the canonical repository gates:

```bash
./scripts/verify-schema.py --verbose
just verify-schema
./scripts/verify-pg-migrations.py --mapping-only
env -u DATABASE_URL just verify-pg
just lint
just test
```

Do not trust an inherited `$DATABASE_URL` in a Claude shell: the OS sandbox is
disabled, so the variable may be ambient. Default to mapping-only coverage and
run the engine check with `DATABASE_URL` explicitly absent so the verifier uses
its throwaway Docker path when available. A human may authorize an explicit
environment-backed pass only after confirming it points to a disposable
development server: the verifier creates a uniquely named scratch database and
removes it in a `finally` cleanup. If neither safe path is available, report the
PostgreSQL pass as skipped rather than claiming validation.

Run `just guards` when the change touches either verifier, `REGISTER_LOCAL`, a
hook, or another guard. In the handoff, summarize runtime registration order,
engine scope, the declared mapping or exception, behavioral/data-transition
coverage, cross-engine review, and every skipped check.
