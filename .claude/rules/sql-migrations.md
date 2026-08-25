---
paths: ["**/migrations/**/*.sql", "crates/pos-db/src/lib.rs", "scripts/verify-schema.py", "scripts/verify-pg-migrations.py", "scripts/rust_lexer.py", "docs/implementation/ref/schema.md"]
---

# Migrations — forward-only, append-only

Law: `docs/implementation/01-conventions.md` §9. Target shapes: `docs/implementation/ref/schema.md`.

- **Never edit a committed migration.** Not for a typo. Not "it hasn't shipped yet." A new
  migration fixes it. A `PreToolUse` hook denies the write — if you hit it, the hook is working;
  write the next migration instead of arguing with it. **Deleting or renaming one is the same
  edit**, and `.githooks/pre-commit` refuses that too.
- **No down migrations.** The runner is a `PRAGMA user_version` counter with no down path, by
  design. The rollback story is restore-from-encrypted-backup.
- **Register it in the same commit**: `crates/pos-db/migrations/NNNN_short_name.sql`, appended to
  the `MIGRATIONS` array in `crates/pos-db/src/lib.rs`, in order.
- **Migration entries are regular SQL files.** Symlinks, gitlinks, devices, and other filesystem
  indirection are forbidden in both migration trees; the staged-file policy and both verifiers
  reject them.
- **Naming is not optional.** `*_minor` money · `*_milli` quantities · `*_ppm` rates (16% =
  `160_000`) · `*_at` UTC ISO-8601 TEXT · `*_date` store-local `YYYY-MM-DD` · `is_*`/`has_*`
  INTEGER 0/1 · `<table>_id` BLOB(16) · enums TEXT + `CHECK (x IN (…))`.
- **Run the exact runtime chain through real SQLite before committing** —
  `./scripts/verify-schema.py`. It parses the Rust `MIGRATIONS` array, requires exact ordered
  parity with the SQL files on disk, and then applies that compiled chain using the runtime's
  per-file transaction and `user_version` update. Omissions,
  duplicates, nonexistent entries, numbering gaps, and order drift are failures before SQL
  execution begins. Do **not** hand-roll `sqlite3 :memory: ".read …"`: it applies only the files
  you remember to name and accepts a `REFERENCES ghost(id)` in silence.
- **A shape change ships with its data migration in the same file**, plus a test that seeds the
  old shape, migrates, and asserts the new one.
- **No path that `UPDATE`s a completed sale (I-4).** Not in DDL, not in a trigger, not in a
  repository method — not even a private one.

## The Postgres mirror, and how the two are mapped

`apps/server/migrations/` mirrors the register's schema with the **same semantics**, and
`./scripts/verify-pg-migrations.py` checks it — the mapping on every run, and the SQL itself
against a real PostgreSQL server whenever one is reachable.

Mapping coverage and successful SQL execution are necessary, not proof of semantic parity.
Review UUID/BLOB representation, integer widths and booleans, text timestamps/`TIMESTAMPTZ`,
JSON, constraints, triggers, indexes, grants, and reference-data behavior explicitly. An
environment-backed pass must target a disposable development server: the verifier creates a
uniquely named scratch database and removes it in a `finally` cleanup.

SQLx runs each PostgreSQL migration in one transaction by default. Its only opt-out is the
case-sensitive `-- no-transaction` prefix starting at byte zero. Use that marker only for a
statement PostgreSQL forbids inside a transaction, and design/test recovery from partial
application; the verifier deliberately mirrors this exact boundary.

The numbers cannot match, so do not pretend they do: sqlx files are named exactly
`<14-digit UTC timestamp>_<lower_snake>.sql`; versions are unique and strictly increasing, and
a timestamp is not `NNNN`. **The mapping is declared, not inferred.** Every Postgres migration
carries one of these lines in its header comment:

```sql
-- Mirrors SQLite 0002_sale_integrity.sql (conventions §9 rule 4).
-- Server-only: <why nothing on the register corresponds>.
```

The name may differ too, and often should: `0002_sale_integrity` is mirrored by
`20260820120000_change_sequence`, because the register's half of that migration was
trigger-enforced sale immutability and the server's half was the change sequence those triggers
imply. A mirror is the same *semantics*, not the same file with a different extension.

A migration for an entity that never syncs has no mirror at all. Record it in `REGISTER_LOCAL`
in `scripts/verify-pg-migrations.py` with the reason, rather than committing an empty file that
claims a mirror exists.
