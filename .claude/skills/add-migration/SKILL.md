---
name: add-migration
description: Add a database migration correctly — next number, conventions §2 naming, validated against real SQLite, registered in the MIGRATIONS array, mirrored on Postgres, with a data-migration test when the shape changes. Use whenever a schema change is needed on SQLite or Postgres.
---

# Add a migration

Law: `docs/implementation/01-conventions.md` §9. Target shapes: `ref/schema.md`.
**Forward-only, append-only.** A committed migration is never edited — a
`PreToolUse` hook enforces it, and if you hit that denial the answer is a new file,
never an argument with the hook.

## 1 · Confirm you are adding, not editing

```bash
ls crates/pos-db/migrations/            # the next number is max + 1, zero-padded to 4
git ls-tree -r --name-only HEAD | grep migrations/
```

Anything in that second list is frozen. If the fix belongs to a committed
migration, it becomes the next one instead — including a typo, including "it
hasn't shipped yet."

## 2 · Write the SQL

`crates/pos-db/migrations/NNNN_short_name.sql`. Check the intended shape in
`ref/schema.md` first — migrations 0002–0011 are already specified there, and
deviating from the doc without updating it turns the reference into a liability.

Naming is not optional (§2):

| Kind | Suffix | Example |
|---|---|---|
| money | `*_minor` | `total_minor` |
| quantity | `*_milli` | `qty_milli` (1 unit = 1000) |
| rate | `*_ppm` | `rate_ppm` — 16% = `160_000` |
| timestamp | `*_at`, ISO-8601 UTC TEXT | `completed_at` |
| calendar day | `*_date`, store-local | `business_date` |
| flag | `is_*` / `has_*`, INTEGER 0/1 | `is_active` |
| foreign key | `<table>_id`, BLOB(16) | `sale_id` |
| enum | TEXT + `CHECK (x IN (…))` | `status TEXT CHECK (…)` |

No `REAL`/`FLOAT`/`NUMERIC` column, ever (I-1). No path that `UPDATE`s a completed
sale (I-4) — not in DDL, not in a trigger.

## 3 · Run it through real SQLite before committing

```bash
sqlite3 :memory: ".read crates/pos-db/migrations/0001_init.sql" \
                 ".read crates/pos-db/migrations/NNNN_short_name.sql"
```

Apply the earlier migrations too when yours `ALTER`s their tables — SQLite will not
tell you a column is missing until it runs. Note what this does **not** catch: a
`REFERENCES ghost(id)` to a table nothing creates is accepted silently. For that,
and for the naming audit, run `./scripts/verify-schema.py`.

## 4 · Register it

Append to `MIGRATIONS` in `crates/pos-db/src/lib.rs`, **in order, same commit**:

```rust
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/NNNN_short_name.sql"),
];
```

The runner is a `PRAGMA user_version` counter — position in this array *is* the
version. Inserting rather than appending silently re-numbers every later migration.

## 5 · Mirror it on Postgres

`apps/server/migrations/`, same snake_case name, same semantics (§9 rule 4).
Note the friction: sqlx names files with a timestamp, so the SQLite number and the
Postgres number cannot literally match. Use the same name and record the SQLite
number in a header comment. If the entity is register-local and never syncs, say so
in the commit message instead of writing an empty mirror.

## 6 · Test it, if the shape changed

A migration that changes existing data ships with the data migration **in the same
file** plus a test that seeds the old shape, migrates, and asserts the new one
(§9 rule 3). `crates/pos-db/tests/`. The `sale_line.qty` → `qty_milli` fix (G-12,
`ref/schema.md`) is the worked example: existing rows are unit counts and must be
multiplied by 1000, so a migration without a data step corrupts every historical
sale by a factor of a thousand.

## 7 · Close it out

```bash
just lint && just test
./scripts/verify-schema.py
```

Commit with the microstep number: `feat(db): stock ledger tables   [1.10.1]`.
