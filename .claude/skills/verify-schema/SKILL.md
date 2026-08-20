---
name: verify-schema
description: Execute every SQL block in docs/implementation/ref/schema.md against real SQLite and audit the result against conventions §2 — float types, unit suffixes, timestamp types, dangling foreign keys. Use before writing a migration, after editing schema.md, or when asked whether the schema reference is still true.
---

# Verify the schema reference

`ref/schema.md` is ~888 lines of DDL that nothing compiles. Prose SQL rots silently:
a column referenced in one migration and never created in another reads perfectly
and fails at runtime.

## Run it

```bash
./scripts/verify-schema.py            # the check
./scripts/verify-schema.py --verbose  # name each block as it applies
./scripts/verify-schema.py --self-test  # prove the checks still fire
```

It applies `crates/pos-db/migrations/0001_init.sql` first, then every ` ```sql `
block in `schema.md` in document order, against an in-memory database — so
migration 0004's `ALTER TABLE` is checked against the table 0002 actually created.

## Reading the output

**`FAIL block '<heading>' does not execute`** — the DDL in that section is not
valid SQLite. Read the section, fix the doc. If the block deliberately elides a
body (`BEGIN … END`), that is not a formatting choice — it means nobody has
written that SQL yet, and whoever implements the microstep will have to invent it.
Say so rather than quietly ignoring it.

**`FAIL <table>.<column>` is a provable convention violation** — a float type, a
money column with no `_minor`, a quantity with no `_milli`, a rate with no `_ppm`,
a `*_at` that is not TEXT, a flag that is not INTEGER, or a foreign key naming a
table or column that no migration creates.

**`note <table>.<column>`** is a judgment call, never a failure — flag columns
spelled without `is_`/`has_`. Report them; do not "fix" `schema.md` on your own
initiative, because renaming a column in the plan of record is a design decision.

## Before you trust a change to the script

`--self-test` runs the audit against a deliberately bad fixture and asserts every
check fires, plus that the clean table produces no findings. Run it after any edit
to `scripts/verify-schema.py`. A guard nobody has seen fail is a guard nobody
should trust.

## What it does not do

It does not check the Postgres mirror, and it does not compare `schema.md` against
the migrations actually committed in `crates/pos-db/migrations/`. Drift between the
doc and the shipped migrations is still a human read.
