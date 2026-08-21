---
name: add-migration
description: Add a database migration correctly — next number, conventions §2 naming, validated against real SQLite, registered in the MIGRATIONS array, mirrored on Postgres, with a data-migration test when the shape changes. Use whenever a schema change is needed on SQLite or Postgres.
---

# Add a migration

Law: `docs/implementation/01-conventions.md` §9.
Target shapes: `docs/implementation/ref/schema.md`.
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
`docs/implementation/ref/schema.md` first — migrations 0002–0012 are already
specified there, and deviating from the doc without updating it turns the
reference into a liability.

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

## 3 · Run the whole chain through real SQLite before committing

```bash
./scripts/verify-schema.py --verbose
```

This applies **every** file in `crates/pos-db/migrations/` — including the one you
just wrote, because the pass reads the directory rather than git — in the order the
`PRAGMA user_version` runner applies them. That matters when yours `ALTER`s an
earlier migration's table: SQLite will not tell you a column is missing until it
runs, and a hand-written `sqlite3 :memory: ".read …"` applies only the files you
remembered to name. It also catches what plain `.read` cannot — a
`REFERENCES ghost(id)` to a table nothing creates, which raw SQLite accepts in
silence — and audits every new column against the naming table above.

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

`apps/server/migrations/`, same **semantics** (§9 rule 4). sqlx names files with a
timestamp, so the numbers cannot match and the mapping is *declared* rather than
inferred. Open the new file with one of these, and `verify-pg-migrations.py` will
check it:

```sql
-- Mirrors SQLite NNNN_short_name.sql (conventions §9 rule 4).
-- Server-only: <why nothing on the register corresponds>.
```

The name may differ too when the server's half of the work is different — see
`20260820120000_change_sequence.sql`, which mirrors `0002_sale_integrity.sql`.

If the entity is register-local and never syncs, do not write an empty mirror: add
it to `REGISTER_LOCAL` in `scripts/verify-pg-migrations.py` with the reason.

Then check it against a real server:

```bash
./scripts/verify-pg-migrations.py --verbose
```

It uses `$DATABASE_URL` when one is set and a throwaway Docker container otherwise.
With neither it audits the mapping, says it skipped the engine pass, and leaves the
real check to CI — which is not the same as passing.

## 6 · Test it, if the shape changed

A migration that changes existing data ships with the data migration **in the same
file** plus a test that seeds the old shape, migrates, and asserts the new one
(§9 rule 3). `crates/pos-db/tests/`. The `sale_line.qty` → `qty_milli` fix (G-12,
`docs/implementation/ref/schema.md`) is the worked example: existing rows are unit counts and must be
multiplied by 1000, so a migration without a data step corrupts every historical
sale by a factor of a thousand.

## 7 · Close it out

```bash
just lint && just test    # lint runs verify-schema.py and the mapping audit
just verify-pg            # the mirror against a real PostgreSQL server
```

Commit with the microstep number: `feat(db): stock ledger tables   [1.10.1]`.
