---
name: verify-schema
description: Audit and diagnose the POS schema, migration registration, and verifier coverage without implementing a schema change. Use for standalone schema audits, verifier failures, or questions about whether the schema reference matches runtime; implementation belongs in add-migration.
---

# Verify the schema

Use the repository verifier rather than ad hoc `sqlite3` commands. It requires
exact ordered parity between Rust's `MIGRATIONS` array and the SQL files on disk,
applies that runtime chain with the application's per-file transaction and
`user_version` update, applies the plan-of-record SQL in a separate in-memory
database, then audits naming, types, and foreign keys.

## Workflow

1. Read `.claude/rules/sql-migrations.md`, then run the normal verifier before
   loading large schema sections:

   ```bash
   ./scripts/verify-schema.py --verbose
   just verify-schema
   ```

2. Read the verifier's runtime-registration result. It rejects omissions,
   duplicates, nonexistent entries, numbering gaps, and array/directory order
   drift before applying the exact compiled chain. Manually inspect the array
   only when diagnosing one of those failures or reviewing verifier coverage.
3. Diagnose failures at their source, then read the referenced failing headings
   in `docs/implementation/ref/schema.md`. Distinguish invalid SQL from convention
   failures, dangling foreign keys, and non-blocking naming notes. Do not edit a
   committed migration to repair a failure; add a forward migration when a fix
   is authorized.
4. If the request asks whether the schema reference is still true, manually
   compare the relevant documented objects and constraints with the registered
   runtime migrations. A clean verifier exit does not prove documentation/runtime
   parity.
5. Audit PostgreSQL in layers:

   ```bash
   ./scripts/verify-pg-migrations.py --mapping-only
   ```

   Mapping proves declaration coverage, not semantic equivalence. Review relevant
   type conversions, constraints, triggers, indexes, grants, reference data, and
   server-only/register-local exceptions before making a parity claim. Run
   `just verify-pg` only when an engine pass is in scope and its target is safe.
   Repository policy removes inherited `$DATABASE_URL` from Codex commands, so
   Codex uses the throwaway Docker path or reports mapping-only coverage. A human
   may run an explicit environment-backed pass after confirming it is a disposable
   development server—the verifier creates a uniquely named scratch database and
   removes it in a `finally` cleanup. Confirm ordinary server migrations use
   SQLx's default transaction boundary; only a case-sensitive, byte-zero
   `-- no-transaction` marker may opt out, with explicit partial-failure recovery.
6. If either verifier or its policy tables changed, prove negative cases and run
   the repository guard suite:

   ```bash
   ./scripts/verify-schema.py --self-test
   ./scripts/verify-pg-migrations.py --self-test
   just guards
   ```

Report the commands, exit status, material notes, and any coverage that was
skipped. Separate these conclusions explicitly: SQL execution, convention/FK
audit, runtime registration/order, documentation parity, PostgreSQL mapping,
PostgreSQL execution, and cross-engine semantic parity. A clean exit with zero
relevant inputs is not meaningful evidence; confirm the expected migration and
schema blocks were actually evaluated.
