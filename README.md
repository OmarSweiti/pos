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
packages/ui/           shared React components        (scaffold)
packages/api-types/    shared request/response types  (scaffold)
```

## Getting started

Requires [Rust](https://rustup.rs) (the pinned toolchain installs itself from
`rust-toolchain.toml`),
[`cargo-nextest`](https://nexte.st/docs/installation/pre-built-binaries/),
[Python 3.11+](https://www.python.org/downloads/),
[Node.js 24 LTS](https://nodejs.org/) (pinned to an exact release by `.nvmrc`,
which CI reads too; `pnpm` refuses another line because `engineStrict` is on),
[pnpm](https://pnpm.io),
[just](https://just.systems),
[`gitleaks`](https://github.com/gitleaks/gitleaks) for content-based secret
scanning, Ruby with its bundled Psych YAML parser for workflow-policy checks,
and Docker for the development database.
[`gh`](https://cli.github.com) is needed by `just pr` and `just merge`, and
[`sqlx-cli`](https://crates.io/crates/sqlx-cli) by `just migrate` and
`just db-reset`; each recipe says so before it does any work. Tauri also needs
[its platform prerequisites](https://tauri.app/start/prerequisites/).
The optional time-varying `just audit` gate additionally requires
[`cargo-deny`](https://embarkstudios.github.io/cargo-deny/).
On Windows, install Python's standard `py` launcher and Git Bash; the committed
hooks use the portable launcher in `scripts/run-python.sh`.

```bash
just setup          # install hooks, check tools, then install locked dependencies
just db-up          # development Postgres
just dev-terminal   # run the register
```

`just setup` is not optional. It installs the local hooks before any networked
dependency step, so a failed install does not leave the clone silently
unprotected. Branch protection is unavailable on this plan; the hooks are a
bypassable local safety net, while CI provides the server-side evidence. A clone
that skipped setup can still push straight to `main`. `just --list` shows
everything else.

## Quality gates

```bash
just lint     # fmt · clippy · workspace lints · domain purity/acyclicity ·
              # schema/mapping · RTL · prop names · biome · doc links
just test     # cargo nextest --locked --workspace · pnpm -r test
just audit    # cargo-deny advisories/licences · pnpm audit
just guards   # prove the write guards still refuse what they must
just secrets  # content-scan all reachable Git history with Gitleaks
just pre-push # lint · test · web build · guards · secret history scan
```

CI repeats the deterministic gates, scans the proposed commit range for secrets,
and runs the time-varying supply-chain audit separately. A green `just pre-push`
therefore predicts the build gates; run `just audit` before a release as well.

Release automation requires a verified signed tag at the exact validated branch
tip, builds every platform with separated signing/publishing permissions, and
adds checksums plus an SBOM to a draft. Published releases are immutable. The
first external release is intentionally blocked until the updater-signing
repository secrets, committed updater public configuration, and OS signing
material are configured; see [`SECURITY.md`](SECURITY.md).

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
| [`docs/orientation.md`](docs/orientation.md) | what each repository directory and guard does |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | how to propose a change |
| [`SECURITY.md`](SECURITY.md) | reporting a vulnerability, and the security posture |

## Licence

Proprietary — all rights reserved. See [`LICENSE`](LICENSE).
