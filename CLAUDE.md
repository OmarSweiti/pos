# POS — read this before you touch anything

Cross-platform, offline-first point of sale for the Jordanian market.
Tauri 2 + Rust core + React UI; SQLite/SQLCipher on the register, Axum/Postgres in the cloud.

## The plan

`docs/implementation/` is the plan of record. The three source documents under `docs/plan/` are
**immutable historical inputs**, frozen on purpose — read them for intent, never for a name.

| Document | Answers |
|---|---|
| [`docs/implementation/`](docs/implementation/) | **what to type**, in what order, and how you know it worked |
| [`docs/implementation/02-development-workflow.md`](docs/implementation/02-development-workflow.md) | **how to work** — every command, the feature lifecycle, manual testing, drills, release |
| [`docs/implementation/03-github-workflow.md`](docs/implementation/03-github-workflow.md) | **how work ships** — the four branches, issues, the board, PRs, release channels, Jira |
| [`docs/plan/business-functional-master-plan.md`](docs/plan/business-functional-master-plan.md) | *immutable source* — what to build, and why Jordanian law demands it |
| [`docs/plan/engineering-blueprint.md`](docs/plan/engineering-blueprint.md) | *immutable source* — stack, architecture, standards |

Start at [`docs/implementation/README.md`](docs/implementation/README.md).
The engineering law is [`docs/implementation/01-conventions.md`](docs/implementation/01-conventions.md) — read it once, keep it open.
The daily workflow, command by command, is [`docs/implementation/02-development-workflow.md`](docs/implementation/02-development-workflow.md).

**Before you act on a sentence from `docs/plan/`, read
[`00-master-plan.md`](docs/implementation/00-master-plan.md) §4a, "Errata and concordance."** It is
the single ledger of every place a source plan has been superseded, and the list is long enough to
cost a day: `rate_bp`, banker's rounding as the money default, `stock_movement`, `tax_group`,
`product_barcode`, `user`, `role_perm`, a mutable `loyalty_points` column, migrations with `down`
steps, and a Phase-3 fiscal production cutover all read as current truth in `docs/plan/` and are
all wrong now. §4a also carries the status of corrections C-1 to C-4 — **two of the four
corrections were themselves wrong** — and the open items none of them closed. Nothing edits a
source plan; the concordance is how a superseded table name fails to become a schema.

## The flow

```
feature branch  →  development  →  staging  →  main
                   default branch  candidate   what a merchant runs
```

Branch from `development`, never from `main`. A work PR into `development` is **squash-merged**
and its *title* becomes the commit, so the title obeys the commit convention. A promotion PR
(`development → staging`, `staging → main`) is merged with a **merge commit** — squashing one
forks the branches permanently. `just branch <name>`, `just pr`, `just merge`, `just flow`,
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
4. **Completed sales are immutable.** No `UPDATE` on a complete sale, ever. Corrections are new
   documents. Tender settlement and shift close are **not** exceptions: they append
   `tender_status_event` and `shift_close_event` facts, and current state is a rebuildable
   projection. The server revokes `UPDATE` on fact tables, so a register that mutates one leaves
   central reconciliation permanently stale.
5. **Price and name are copied onto the sale line** at capture time. Reports and refunds read the line, never today's catalog.
6. **Stock is a ledger.** On-hand is `SUM(qty_delta)`, cached in `stock_cache`, and the cache is
   **rebuildable by a command CI runs**. Ledger append, cache projection and watermark commit together.
7. **Ordering comes from owned sequences**, never a device clock. Pull order is the server's
   `version`; push order is `(register_id, sync_outbox.seq)`. UUIDv7 supplies identity and index
   locality — it embeds a device timestamp and is never the causal authority.
8. **`pos-domain` is pure.** No I/O, no SQLite, no Tauri, no network, no clock, no randomness — time and IDs are *arguments*.
9. **Every fact graph and its delivery envelope commit in one transaction.** The facts, one
   `sync_commit`, the complete `fact_commit_member` manifest, and the `sync_outbox` delivery rows.
   One `BEGIN`, one `COMMIT` — a partial manifest lets the server accept a header without its lines.

Two more rules are refused just as hard, and both are how a price control gets defeated. **No base
sale command accepts a price**: `cart_add_line` has no `unit_price_minor`, a price-embedded label
arrives as a typed `ScanLookup::PriceEmbedded`, and price-bearing IPC arguments exist only on
audited `cart_override_price`, capped audited `cart_add_department_sale`, and inert content-hashed
`product_quick_add_prepare`. **Every privileged command consumes a one-use `ApprovalHandle`** in
the same transaction as its financial effect and audit row. [`01-conventions.md`](docs/implementation/01-conventions.md) §12–§13 owns both.

## Quality gates

```bash
just check          # cargo check --locked --workspace --all-targets — the fastest "would build"
just fmt            # rewrite formatting, Rust and TS, before lint refuses it
just lint           # node pin · fmt · clippy · workspace lints · acyclic/purity · schema + PG
                    # mapping · logical CSS · prop names · test catalog · frontier reconciliation
                    # · biome · doc links · policy-script lint
just test           # cargo nextest --locked --workspace · pnpm -r test
just guards         # the write guards, the git hooks and every policy checker still refuse
just build-web      # web build coverage · pnpm -r build — the only place `tsc` runs
just verify-schema  # exact MIGRATIONS/disk parity, then the runtime chain on real SQLite
just verify-pg      # the Postgres mirror, against a real server
just secrets        # Gitleaks over every commit reachable from this clone
just pre-push       # lint · test · build-web · guards · secrets
just audit          # Rust advisories/licences · JS licences · npm advisories
just setup          # after pulling
```

`just lint` also reconciles [`ref/test-catalog.md`](docs/implementation/ref/test-catalog.md)
against the suite, the phase files and its own arithmetic
(`scripts/check-test-catalog.py`). That gate exists because the dangerous failure of a coverage
matrix is not a red test — it is an absent test behind a green row, and a hand-maintained table
counted 73 cases against a stated total of 72 before anything checked it. **It now runs in CI**, in
the `rust` job, because it reconciles against nextest listings and needs cargo.

Until then this file claimed it was "the one checker in the local gate with no `ci.yml` step". That
was wrong twice over: there were **two** — `scripts/tests/bench_gate_test.py` had drifted the same way
unnoticed — and naming one of them made the other invisible. Both are wired now, and
`scripts/check-ci-gate-parity.py` compares the gate recipes against the workflows so the gap cannot
reopen silently. Its structural cause is still there and is deliberate: `ci.yml` hand-enumerates the
guard steps rather than calling `just guards`, so a failure names the check that failed instead of
one opaque step. The parity checker is what makes that enumeration safe.

`just pre-push` is the complete local gate. Time-varying advisory checks stay in CI's
`supply-chain` job because they reach the network and can change without a repository change.
CI also supplies real PostgreSQL and promotion-only macOS/Windows Tauri builds, so report those
separately instead of claiming the local gate reproduces every runner environment.

`just verify-pg` needs a Postgres — `$DATABASE_URL` or Docker. Without one it audits the
SQLite↔Postgres mapping, says it skipped the engine pass, and leaves that to CI.
`unwrap()` and `expect()` are **denied** outside tests and `main()`.

Three recipes the workflow document names do not exist yet, and each has an owning microstep:
`just seed` (1.12.1), `just fuzz` (1.2.8) and `just test-soak` (2.9.6). `just bench-gate` landed with
1.2.0 and **refuses every run**: no reference register has been bought, so `ref/hardware-and-receipts.md`
§6a.1's matrix and `benchmarks/reference-register.toml` are both deliberately blank and conventions
§7.1 accepts no baseline against a blank record. Its budgets arrive at 1.2.7, 1.4.9, 1.6.2 and
1.11.13, its measurement job at 1.12.3. `just --list` is the only authority on what is runnable today.

## Safety layers, and their limits

`.claude/rules/` holds the standards. Three are path-scoped — they load when a matching file is
read, so a Rust rule costs nothing while you edit React. `security.md` carries **no** path scope
and always applies, because a never-log rule that arrives only once you are already editing the
logger is a rule that arrives too late.
`.claude/hooks/` holds the agent-time guards:

`AGENTS.md` is the Codex entry point for the same repository law, and
`.agents/skills/` exposes the maintained migration/schema workflows to Codex.
Codex-specific execution policy and hook adapters live under `.codex/`.

| Guard | Refuses |
|---|---|
| `.claude/hooks/protect-immutable.py` | writing, deleting, or moving a **committed migration** or anything in `docs/plan/` through Claude write tools, Bash, PowerShell, or Monitor |
| `.claude/hooks/docs-links-on-write.py` | leaving a broken cross-reference after Claude changes **any** tracked `.md` — the five root documents included — whatever the link target's extension; the `.sh` file is only an inactive POSIX compatibility wrapper |
| `.claude/hooks/validate-settings.py` | a session-time weakening of the reviewed project or local Claude settings, or the loss of a required skill contract. The one hook here that **fails closed** |
| `.codex/hooks/` | immutable-path and forward-only SQLx checks for Codex shell, immutable-path checks for `apply_patch`, and complete documentation-link checks after any Markdown `apply_patch` |
| `.githooks/commit-msg` | a title outside `<type>(<scope>): <summary>  [N.N.N\|N.N.N–N.N.N\|—]`, or coding-assistant attribution |
| `.githooks/pre-commit` | protected/sensitive paths, oversized staged blobs, plan or committed-migration edits, and Gitleaks findings in staged content |
| `.githooks/pre-push` | direct/force/deletion pushes to the three flow branches, moving/deleting an existing tag, assistant attribution, or a secret anywhere in reachable history |
| `scripts/check-protected-paths.sh` | a pull request that edits a source plan or a migration already present in its base; `branch-flow.yml` runs policy from the exact trusted workflow revision |
| `scripts/check-branch-workflow-policy.rb` | weakening the read-only `pull_request_target` boundary, title/body attribution wiring, any workflow definition, or the trusted CI/agent/Git-hook/label/dependency/security/repository-setup policy and helper set without an explicit red/manual review. **This file and `AGENTS.md` are inside that frozen set**, so editing either is deliberately red until a human reads the diff; ordinary application/test code is not byte-pinned |
| `scripts/gh-actions-policy.sh` | mutable or unapproved external Action references before the post-merge full-SHA repository policy is enabled |

All are negative-tested — `just guards` runs every suite
(`.claude/hooks/test-settings.py`, `.claude/hooks/test-protect-immutable.sh`,
`.claude/hooks/test-docs-links.sh`,
`.codex/hooks/test-hooks.sh`, `.codex/test-policy.py`, `.agents/test-skills.py`,
`.githooks/test-hooks.sh`, `scripts/test-gh-setup.sh`, and every repository
checker's `--self-test`). Two entries are not self-tests but live proofs:
`scripts/check-justfile-policy.py` proves a `just` argument never becomes shell source, and
`scripts/check-branch-workflow-policy.rb --candidate-root .` runs the frozen-surface policy
against this working tree. Run `just guards` after touching any of them.
A guard nobody has seen fail is a guard nobody should trust.

The shell arm of `protect-immutable.py` is defence in depth, not a proof: it follows `cd`,
covers redirects, output flags, copy destinations, PowerShell verbs, and literal protected
paths passed to interpreters. Arbitrary code can still construct a path dynamically.
`.claude/settings.json` intentionally disables Claude's OS sandbox so permitted package-manager,
Git/SSH, GitHub, and other networked shell commands can use the host normally. The normal manual
permission flow, exact project `permissions.deny` list, and repository hooks remain. Those
Read/Edit denies govern Claude tools, not subprocesses: a permitted shell command has ambient host
filesystem, network, environment, and credential access. Do not describe this posture as OS
containment or credential scrubbing. The staged Git hook and trusted-workflow PR check remain
separate backstops.

Claude invokes every active hook through the same shell-free Node launcher and includes actual
PowerShell/Monitor payload tests. Native Windows hook dispatch was not executed here; the OS
sandbox is deliberately disabled on every platform, so do not present the hook tests as an OS
enforcement boundary. Fail-open internal parser errors use a visible structured warning; the
PreToolUse launcher fails closed if Python cannot start, and ConfigChange validation fails closed
when project or local settings try to weaken the reviewed contract.

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
crates/pos-domain/        pure rules: Money, tax, cart machine   ← the crown jewel, keep it pure
crates/pos-db/            SQLite schema, migrations, repositories
crates/pos-sync/          outbox/cursor protocol (client + server)
crates/pos-hardware/      printer/scanner/terminal traits + simulator
crates/pos-test-support/  the shared proptest configuration and strategies (dev-only)
crates/pos-fiscal/        UBL builder, pinned code tables, queue, conformance    → group 2.7
apps/terminal/            the register (Tauri 2): src/ = React, src-tauri/ = Rust shell
apps/server/              Axum: sync, auth, reporting
apps/backoffice/          React admin
packages/money/           the minor-unit rule, shared by both front ends
packages/ui/              shared React components, and packages/api-types/ shared DTOs — both scaffolds
```

The one arrowed row **does not exist on disk yet**; the arrow is the microstep that creates it.
`pos-test-support` landed at microstep 1.1.0 and is a `[dev-dependencies]` entry in every crate that
uses it — it reads the environment, which no crate that ships to a register may do, and the
dependency table is the boundary that keeps that harmless.
`pos-fiscal` is named before it exists because the reference documents already specify its contents
and because it must stay its own crate: everything reconstructed from the ISTD specification lives in
one module, `pos-fiscal/src/codes.rs`, so an official code change is one diff in one file and its
goldens rather than conditionals leaking through the builder. Its layout is
[`ref/fiscal-jofotara.md`](docs/implementation/ref/fiscal-jofotara.md) §9.

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
