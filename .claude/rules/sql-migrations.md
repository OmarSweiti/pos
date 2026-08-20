---
paths: ["**/migrations/**/*.sql", "crates/pos-db/src/lib.rs"]
---

# Migrations — forward-only, append-only

Law: `docs/implementation/01-conventions.md` §9. Target shapes: `docs/implementation/ref/schema.md`.

- **Never edit a committed migration.** Not for a typo. Not "it hasn't shipped yet." A new
  migration fixes it. A `PreToolUse` hook denies the write — if you hit it, the hook is working;
  write the next migration instead of arguing with it.
- **No down migrations.** The runner is a `PRAGMA user_version` counter with no down path, by
  design. The rollback story is restore-from-encrypted-backup.
- **Register it in the same commit**: `crates/pos-db/migrations/NNNN_short_name.sql`, appended to
  the `MIGRATIONS` array in `crates/pos-db/src/lib.rs`, in order.
- **Naming is not optional.** `*_minor` money · `*_milli` quantities · `*_ppm` rates (16% =
  `160_000`) · `*_at` UTC ISO-8601 TEXT · `*_date` store-local `YYYY-MM-DD` · `is_*`/`has_*`
  INTEGER 0/1 · `<table>_id` BLOB(16) · enums TEXT + `CHECK (x IN (…))`.
- **Run the DDL through real SQLite before committing** — `sqlite3 :memory: ".read <file>"`.
  This is not theoretical: it is how a missing `product_id` was caught.
- **A shape change ships with its data migration in the same file**, plus a test that seeds the
  old shape, migrates, and asserts the new one.
- **Postgres mirrors SQLite** in `apps/server/migrations/` — same number, same name, same
  semantics. Divergence is a sync bug waiting.
- **No path that `UPDATE`s a completed sale (I-4).** Not in DDL, not in a trigger, not in a
  repository method — not even a private one.
