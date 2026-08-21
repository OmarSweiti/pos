# POS — read this before you touch anything

Cross-platform, offline-first point of sale for the Jordanian market.
Tauri 2 + Rust core + React UI; SQLite/SQLCipher on the register, Axum/Postgres in the cloud.

## The plan

| Document | Answers |
|---|---|
| [`docs/plan/business-functional-master-plan.md`](docs/plan/business-functional-master-plan.md) | **what** to build, and why Jordanian law demands it |
| [`docs/plan/engineering-blueprint.md`](docs/plan/engineering-blueprint.md) | **how** — stack, architecture, standards |
| [`docs/implementation/`](docs/implementation/) | **what to type**, in what order, and how you know it worked |
| [`docs/implementation/02-development-workflow.md`](docs/implementation/02-development-workflow.md) | **how to work** — every command, the feature lifecycle, manual testing, drills, release |
| [`docs/implementation/03-github-workflow.md`](docs/implementation/03-github-workflow.md) | **how work ships** — the four branches, issues, the board, PRs, release channels, Jira |

Start at [`docs/implementation/README.md`](docs/implementation/README.md).
The engineering law is [`docs/implementation/01-conventions.md`](docs/implementation/01-conventions.md) — read it once, keep it open.
The daily workflow, command by command, is [`docs/implementation/02-development-workflow.md`](docs/implementation/02-development-workflow.md).

## The flow

```
feature branch  →  development  →  staging  →  main
                   default branch  candidate   what a merchant runs
```

Branch from `development`, never from `main`. A work PR into `development` is **squash-merged**
and its *title* becomes the commit, so the title obeys the commit convention. A promotion PR
(`development → staging`, `staging → main`) is merged with a **merge commit** — squashing one
forks the branches permanently. `just branch <name>`, `just pr`, `just flow`,
`just promote-staging`, `just promote-main`.

Branch protection does **not** exist here: the repo is private on the GitHub Free plan, where
protection and rulesets both answer 403. The git hooks in `.githooks/` are the enforcement, so
`just setup` is not optional — a clone that skipped it can push straight to `main`.
[`03-github-workflow.md`](docs/implementation/03-github-workflow.md) §3 has the full honest table.

## The nine invariants

Not style preferences. Each one, violated, produces a class of bug that costs money.

1. **Money is `i64` minor units. Always.** No float touches money in Rust, TypeScript, SQL, or JSON.
   Intermediate math uses `rust_decimal`, rounds **once**, returns to `i64`.
   `clippy::float_arithmetic` is **denied** workspace-wide.
2. **The minor-unit exponent is per-currency data.** JOD = 3 (1 dinar = 1000 fils). Never `100`.
3. **Quantities are `i64` milli-units.** `1 unit = 1000`. Weighed and discrete share one representation.
4. **Completed sales are immutable.** No `UPDATE` on a complete sale, ever. Corrections are new documents.
5. **Price and name are copied onto the sale line** at capture time. Reports and refunds read the line, never today's catalog.
6. **Stock is a ledger.** On-hand is `SUM(qty_delta)`, cached and rebuildable.
7. **Ordering comes from server versions and UUIDv7**, never a device clock.
8. **`pos-domain` is pure.** No I/O, no SQLite, no Tauri, no network, no clock, no randomness — time and IDs are *arguments*.
9. **Every fact write and its outbox row commit in one transaction.**

## Quality gates

```bash
just lint     # fmt --check · clippy -D warnings · acyclic · schema · logical-css · biome ci · doc-links
just test     # cargo nextest --workspace · pnpm -r test
just audit    # cargo-deny advisories/licences · pnpm audit
just guards   # prove the write guards still refuse
just pre-push # all of the above
just setup    # after pulling
```

CI runs exactly these, so a green local `just pre-push` predicts a green build.
`unwrap()` and `expect()` are **denied** outside tests and `main()`.

## What is enforced, not merely written down

`.claude/rules/` holds the standards, split by the paths they govern — they load
when a matching file is read, so a Rust rule costs nothing while you edit React.
`.claude/hooks/` holds the guards, which are not advisory:

| Guard | Refuses |
|---|---|
| `.claude/hooks/protect-immutable.py` | writing a **committed migration**, or anything in `docs/plan/` |
| `.claude/hooks/docs-links-on-write.sh` | leaving a broken cross-reference in `docs/**.md` |
| `.githooks/commit-msg` | a commit subject outside `<type>(<scope>): <summary>  [<step>]` |
| `.githooks/pre-commit` | committing a key, an `.env`, a database file, or a committed migration |
| `.githooks/pre-push` | a direct push, force-push, or deletion of `main`/`staging`/`development` |

All are negative-tested — `just guards` runs both suites
(`.claude/hooks/test-protect-immutable.sh` and `.githooks/test-hooks.sh`). Run it after touching
any of them. A guard nobody has seen fail is a guard nobody should trust.

## Where things live

```
crates/pos-domain/     pure rules: Money, tax, cart machine   ← the crown jewel, keep it pure
crates/pos-db/         SQLite schema, migrations, repositories
crates/pos-sync/       outbox/cursor protocol (client + server)
crates/pos-hardware/   printer/scanner/terminal traits + simulator
apps/terminal/         the register (Tauri 2): src/ = React, src-tauri/ = Rust shell
apps/server/           Axum: sync, auth, reporting
apps/backoffice/       React admin
packages/money/        the minor-unit rule, shared by both front ends
```

Migrations are **forward-only** and are **never edited once committed**. Two are
committed: `0001_init.sql`, and `0002_sale_integrity.sql`, which fixed `qty` →
`qty_milli` (gap G-12) and put I-4 into triggers.
