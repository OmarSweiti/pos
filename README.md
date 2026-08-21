# POS

Cross-platform, offline-first point of sale for the Jordanian market.

A register keeps trading when the internet does not. Sales are captured locally
against an encrypted SQLite database, and sync to the cloud when there is a
connection — never the other way round. Money is integer minor units end to end,
Arabic is the default language rather than a translation layer, and completed
sales are immutable facts that corrections reference rather than overwrite.

**Status: Phase 0 complete.** The skeleton, the toolchain, the quality gates and
the schema are in place; the selling starts in Phase 1. See
[`docs/implementation/00-master-plan.md`](docs/implementation/00-master-plan.md)
for what lands when.

## Stack

| Layer | Choice |
|---|---|
| Register | Tauri 2 — React + TypeScript UI over a Rust core |
| Local store | SQLite with SQLCipher; key in the OS credential store |
| Cloud | Axum + PostgreSQL |
| Back office | React |
| Shared logic | `pos-domain`, a pure Rust crate with no I/O |

```
crates/pos-domain/     pure rules: Money, tax, cart machine   ← keep it pure
crates/pos-db/         SQLite schema, migrations, repositories
crates/pos-sync/       outbox/cursor protocol (client + server)
crates/pos-hardware/   printer/scanner/terminal traits + simulator
apps/terminal/         the register (Tauri 2)
apps/server/           Axum: sync, auth, reporting
apps/backoffice/       React admin
packages/money/        the minor-unit rule, shared by both front ends
```

## Getting started

Requires [Rust](https://rustup.rs) (the pinned toolchain installs itself from
`rust-toolchain.toml`), [pnpm](https://pnpm.io), [just](https://just.systems),
and Docker for the development database. Tauri also needs
[its platform prerequisites](https://tauri.app/start/prerequisites/).

```bash
just setup          # install dependencies AND enable the git hooks — not optional
just db-up          # development Postgres
just dev-terminal   # run the register
```

`just setup` is not optional. Branch protection is unavailable on this plan, so
the hooks in `.githooks/` are the enforcement; a clone that skipped it can push
straight to `main`. `just --list` shows everything else.

## Quality gates

```bash
just lint     # fmt · clippy -D warnings · acyclic · schema · biome · doc-links
just test     # cargo nextest --workspace · pnpm -r test
just audit    # cargo-deny advisories/licences · pnpm audit
just guards   # prove the write guards still refuse what they must
just pre-push # all of the above, in the order CI runs them
```

CI runs exactly these, so a green `just pre-push` predicts a green build.

## The invariants

Nine rules that are not style preferences — each one, violated, produces a class
of bug that costs money. They are listed in
[`CLAUDE.md`](CLAUDE.md) and specified in
[`docs/implementation/01-conventions.md`](docs/implementation/01-conventions.md),
which is the engineering law for this repository.

The short version: money is `i64` minor units and the exponent is per-currency
data (JOD has three decimal places, not two); quantities are `i64` milli-units;
completed sales are immutable; the price and name a customer saw are copied onto
the sale line; stock is a ledger; ordering comes from server versions, never a
device clock; and `pos-domain` performs no I/O of any kind.

## Documentation

| Document | Answers |
|---|---|
| [`docs/plan/business-functional-master-plan.md`](docs/plan/business-functional-master-plan.md) | **what** to build, and why Jordanian law demands it |
| [`docs/plan/engineering-blueprint.md`](docs/plan/engineering-blueprint.md) | **how** — stack, architecture, standards |
| [`docs/implementation/`](docs/implementation/) | **what to type**, in what order, and how you know it worked |
| [`docs/implementation/02-development-workflow.md`](docs/implementation/02-development-workflow.md) | the daily loop, command by command |
| [`docs/implementation/03-github-workflow.md`](docs/implementation/03-github-workflow.md) | branches, issues, PRs, release channels |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | how to propose a change |
| [`SECURITY.md`](SECURITY.md) | reporting a vulnerability, and the security posture |

## Licence

Proprietary — all rights reserved. See [`LICENSE`](LICENSE).
