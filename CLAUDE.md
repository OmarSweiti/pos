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

Commit and squash titles use `<type>(<scope>): <summary>  [<step>]`, where `<step>` is one
`N.N.N`, an ordered `N.N.N–N.N.N` range, or `—`. Coding assistants are tools, not co-authors:
never add AI attribution trailers. The exact Dependabot bot author/trailer combination is a
narrow compatibility exception and uses the same title grammar with `[—]`; Git author metadata
alone is not cryptographic proof of App identity.

Branch protection does **not** exist here: the repo is private on the GitHub Free plan, where
protection and rulesets both answer 403. The git hooks in `.githooks/` are the first local safety
net, so `just setup` is not optional — a clone that skipped it, or an explicit `--no-verify`, can
still bypass them. CI makes violations loud and reviewable but cannot block this repository's
administrator on the current plan.
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
just lint       # fmt · clippy · architecture/purity · schema/mapping · CSS · tests names · biome · links
just test       # cargo nextest --locked --workspace · pnpm -r test
just guards     # prove the write guards still refuse
just verify-pg  # the Postgres mirror, against a real server
just pre-push   # lint · test · build-web · guards · full-history secret scan
just audit      # cargo-deny advisories/licences · pnpm audit
just setup      # after pulling
```

`just pre-push` is the complete local gate. Time-varying advisory checks stay in CI's
`supply-chain` job because they reach the network and can change without a repository change.
CI also supplies real PostgreSQL and promotion-only macOS/Windows Tauri builds, so report those
separately instead of claiming the local gate reproduces every runner environment.

`just verify-pg` needs a Postgres — `$DATABASE_URL` or Docker. Without one it audits the
SQLite↔Postgres mapping, says it skipped the engine pass, and leaves that to CI.
`unwrap()` and `expect()` are **denied** outside tests and `main()`.

## Safety layers, and their limits

`.claude/rules/` holds the standards, split by the paths they govern — they load
when a matching file is read, so a Rust rule costs nothing while you edit React.
`.claude/hooks/` holds the agent-time guards:

`AGENTS.md` is the Codex entry point for the same repository law, and
`.agents/skills/` exposes the maintained migration/schema workflows to Codex.
Codex-specific execution policy and hook adapters live under `.codex/`.

| Guard | Refuses |
|---|---|
| `.claude/hooks/protect-immutable.py` | writing, deleting, or moving a **committed migration** or anything in `docs/plan/` through Claude write tools, Bash, PowerShell, or Monitor |
| `.claude/hooks/docs-links-on-write.py` | leaving a broken cross-reference after Claude changes `docs/**.md`; the `.sh` file is only an inactive POSIX compatibility wrapper |
| `.codex/hooks/` | immutable-path and forward-only SQLx checks for Codex shell, immutable-path checks for `apply_patch`, and documentation-link checks after a docs `apply_patch` |
| `.githooks/commit-msg` | a title outside `<type>(<scope>): <summary>  [N.N.N\|N.N.N–N.N.N\|—]`, or coding-assistant attribution |
| `.githooks/pre-commit` | protected/sensitive paths, oversized staged blobs, plan or committed-migration edits, and Gitleaks findings in staged content |
| `.githooks/pre-push` | direct/force/deletion pushes to the three flow branches, moving/deleting an existing tag, assistant attribution, or a secret anywhere in reachable history |
| `scripts/check-protected-paths.sh` | a pull request that edits a source plan or a migration already present in its base; `branch-flow.yml` runs policy from the exact trusted workflow revision |
| `scripts/check-branch-workflow-policy.rb` | weakening the read-only `pull_request_target` boundary, title/body attribution wiring, any workflow definition, or the trusted CI/agent/Git-hook/label/dependency/security/repository-setup policy and helper set without an explicit red/manual review; ordinary application/test code is not byte-pinned |
| `scripts/gh-actions-policy.sh` | mutable or unapproved external Action references before the post-merge full-SHA repository policy is enabled |

All are negative-tested — `just guards` runs every suite
(`.claude/hooks/test-protect-immutable.sh`, `.claude/hooks/test-docs-links.sh`,
`.codex/hooks/test-hooks.sh`, `.codex/test-policy.py`, `.agents/test-skills.py`,
`.githooks/test-hooks.sh`, `scripts/test-gh-setup.sh`, and the repository
checkers' `--self-test`s). Run it
after touching any of them.
A guard nobody has seen fail is a guard nobody should trust.

The shell arm of `protect-immutable.py` is defence in depth, not a proof: it follows `cd`,
covers redirects, output flags, copy destinations, PowerShell verbs, and literal protected
paths passed to interpreters. Arbitrary code can still construct a path dynamically. On
macOS/Linux/WSL2, `.claude/settings.json` adds an OS sandbox `denyWrite` for `docs/plan`, secret
read boundaries, credential scrubbing, fail-closed sandbox startup, no unsandboxed retry, and no
preapproved command-egress domain. Project reads are not broadly re-allowed over those denies;
the tracked `.env.example` remains usable policy input while arbitrary `.env.<suffix>` files are
denied by both the sandbox and Read hook. New domains remain prompt-mediated
unless a user/managed strict allowlist is configured.
The staged Git hook and trusted-workflow PR check remain separate backstops.

Claude invokes every active hook through the same shell-free Node launcher and includes actual
PowerShell/Monitor payload tests. Native Windows hook dispatch was not executed here, and the
official Claude OS sandbox still does not cover native Windows, so do not present that platform
as proven. Fail-open internal parser errors use a visible structured warning; the PreToolUse
launcher fails closed if Python cannot start, and ConfigChange validation fails closed when
project or local settings try to weaken the reviewed contract.

Codex loads this project's config, rules, and hooks only for a trusted repository,
and hook definitions require their own review. On first use, open `/hooks`, inspect
the exact commands in `.codex/hooks.json`, and trust them; do not make
`--dangerously-bypass-hook-trust` routine setup. Codex has Python-only Windows hook
commands, but native Windows dispatch is not verified here and is not treated as an
enforcement boundary. Execpolicy prompts are escalation policy, not a universal command parser;
the PreToolUse adapter separately catches common wrapped and nested `sqlx migrate revert`
spellings. The git hooks and CI remain the cross-platform backstops.

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

Migrations are **forward-only** and are **never edited once committed** — deleting or renaming
one counts as editing it. Derive the current chain from the migration directory and the Rust
`MIGRATIONS` array; do not freeze a count in agent guidance. `verify-schema.py` requires exact
ordered parity and applies the exact chain the application compiles with the runtime's per-file
transaction and `user_version` update. Migration entries must be repository-owned regular SQL
files; symlinks, gitlinks, devices, and other filesystem indirection are forbidden.

The Postgres mirror in `apps/server/migrations/` cannot share those numbers — sqlx names files
with a unique 14-digit UTC timestamp and lower-snake name — so each mirror **declares** the SQLite migration it corresponds to in a header
comment, and `./scripts/verify-pg-migrations.py` checks the declaration both ways. The names
may differ where the server's half of the work differs: `20260820120000_change_sequence.sql`
mirrors `0002_sale_integrity.sql`. SQLx runs each server migration in a transaction by default;
only a case-sensitive `-- no-transaction` prefix at byte zero opts out. The verifier mirrors that
boundary, and an opt-out migration must account explicitly for partial-failure recovery.
