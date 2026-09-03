# Orientation — this repository in plain English

The map of this repository's **control surface**: what each directory is for, what each guard
refuses, which scripts hold which promise, and how a change moves from an idea to a release
candidate.

It deliberately does **not** restate the engineering law, the command list, or the delivery rules.
Each of those has exactly one owner, named below. This file used to carry a nine-line summary of the
invariants, and that summary drifted from the law it summarised — which is the whole argument for a
pointer instead of a copy.

It also contains no copied phase status, test totals, migration totals, action SHAs, or line counts.
Those change whenever the repository does; the commands below are how you measure them.

## 1. Start with the authority

Read these in order before changing code:

1. [`../CLAUDE.md`](../CLAUDE.md) — the cross-agent operational overview and the nine invariants
   on one screen.
2. [`../AGENTS.md`](../AGENTS.md) — the Codex entry point for the same law, plus Codex's own
   execution boundaries.
3. [`implementation/README.md`](implementation/README.md) — locates the current implementation work.
4. [`implementation/00-master-plan.md`](implementation/00-master-plan.md) — the spine: phase map,
   risk register, long-lead register, and **§4a, "Errata and concordance."**
5. [`implementation/01-conventions.md`](implementation/01-conventions.md) — the engineering law.
   The nine invariants are *specified* here; every other file quotes them, and where a quotation
   and this file disagree, this file wins.
6. [`implementation/02-development-workflow.md`](implementation/02-development-workflow.md) — the
   daily loop and every command.
7. [`implementation/03-github-workflow.md`](implementation/03-github-workflow.md) — branches, pull
   requests, promotions, releases, and §3's honest table of what is machine-enforced.

Files under [`plan/`](plan/) are **immutable historical source material**. They are frozen on
purpose, which means a superseded table name in them still reads as current truth. **§4a of the
master plan is the ledger of every such supersession** — `rate_bp`, banker's rounding,
`stock_movement`, `tax_group`, `product_barcode`, `role_perm`, a mutable `loyalty_points` column,
`down` migrations, and more. Read it before you act on a sentence from `plan/`. Corrections and
implementation discoveries go into [`implementation/`](implementation/), never into a source plan.

To establish the repository's current state, run the commands rather than copying a snapshot into
documentation:

```bash
git status --short
just --list
just lint
just test
just guards
just secrets
./scripts/verify-schema.py --verbose
./scripts/verify-pg-migrations.py --mapping-only --verbose
```

## 2. What this product is

This is a cross-platform, offline-first point of sale for Jordan. The register is a Tauri
application with a React interface and Rust core. It writes to an encrypted local SQLite database
and synchronizes with an Axum/PostgreSQL service when connectivity returns. Arabic and
right-to-left layout are the default, not a translation layer.

The architecture has four boundaries, and the seam between them is the point:

```text
React UI
   │ typed Tauri commands — the only channel from UI to core
   ▼
Rust shell / server orchestration — the only place that orchestrates read → domain → write
   ├── pos-domain: pure decisions and arithmetic; time and IDs are arguments
   └── pos-db: persistence, migrations, and repositories; it computes no totals
```

**The nine invariants live in [`implementation/01-conventions.md`](implementation/01-conventions.md)
§1**, with the one-screen version in [`../CLAUDE.md`](../CLAUDE.md). They are not summarised here.
Each one, violated, produces a class of bug that costs money — which is also why a second copy of
them is a liability rather than a convenience.

## 3. Repository map

```text
.agents/                 Codex-discoverable repository skills
.claude/                 Claude Code policy, hooks, rules, and skills
.codex/                  Codex sandbox, execution policy, rules, and hooks
.githooks/               local Git commit/push safeguards
.github/                 GitHub workflows, templates, labels, and ownership metadata
apps/                    terminal, server, and back-office applications
crates/                  Rust domain, database, sync, and hardware libraries
packages/                shared TypeScript packages
docs/plan/               immutable source plans
docs/implementation/     executable plan, workflow, standards, and references
infra/                   local development infrastructure
scripts/                 repository policy and verification programs
justfile                 the supported command surface
```

Three directories the plan names do not exist yet, so nothing here pretends they do:
`benchmarks/` for the reference-register record and committed baselines (microstep 1.4.9),
`docs/drills/` for dated drill and hardware-lab records, and `notes/` as local scratch for
promotion bodies. `notes/` is not covered by `.gitignore`, so do not `git add` it.

Important root files:

| File | Purpose |
|---|---|
| [`../Cargo.toml`](../Cargo.toml) | Rust workspace, shared dependencies, and lint policy |
| [`../Cargo.lock`](../Cargo.lock) | committed Rust resolution |
| [`../package.json`](../package.json) | pnpm workspace root and package-manager pin |
| [`../pnpm-lock.yaml`](../pnpm-lock.yaml) | committed JavaScript resolution |
| [`../rust-toolchain.toml`](../rust-toolchain.toml) | Rust toolchain pin |
| [`../.nvmrc`](../.nvmrc) | the Node pin, and the only place it is written down |
| [`../biome.json`](../biome.json) | TypeScript/React formatting and linting |
| [`../deny.toml`](../deny.toml) | Rust advisories, licences, bans, and source policy |
| [`../.gitleaks.toml`](../.gitleaks.toml) | repository secret-scanning policy |
| [`../.gitattributes`](../.gitattributes) | LF normalization for hooks/tooling and binary declarations |
| [`../SECURITY.md`](../SECURITY.md) | vulnerability reporting and honest current security posture |

## 4. The supported command surface

**[`implementation/02-development-workflow.md`](implementation/02-development-workflow.md) §3 is the
command reference**, and the [`../justfile`](../justfile) is the source of truth behind it. Use
`just` recipes rather than assembling a parallel command sequence; `just --list` is what actually
exists today, and the workflow document names several recipes an upcoming microstep still has to
create.

Two ordering decisions are worth knowing before your first run:

- **`just setup` does the local safety work first** — `core.hooksPath`, the repository-local author
  email, and a fail-closed check for Gitleaks, Ruff, ShellCheck and Ruby/Psych — and only then
  installs dependencies. A dependency or network failure therefore cannot leave a clone without
  hooks. Those hooks remain local and bypassable; setup does not turn them into branch protection.
- **`just audit` is deliberately outside `just pre-push`.** Both halves reach the network and depend
  on advisory-database state, so it can fail on a push that changed nothing. CI's `supply-chain`
  job owns that time-varying gate, and the weekly security workflow reruns it when no pull request
  is active.

## 5. Agent setup: shared law, client-specific boundaries

Claude and Codex are aligned where it matters: both read the same conventions, both refuse writes to
immutable paths, both expose the same schema skills, and Git plus CI backstop them equally. Their
native configuration formats differ, so the adapter files are not expected to be byte-identical —
and neither client's configuration is an operating-system containment boundary.

| Owner | Holds |
|---|---|
| [`../CLAUDE.md`](../CLAUDE.md) | the guard table: every Claude/Codex/Git hook and repository checker, what each refuses, and where each stops |
| [`../AGENTS.md`](../AGENTS.md) | Codex's hook-trust requirement, execpolicy limits, and sandbox boundary |
| [`implementation/03-github-workflow.md`](implementation/03-github-workflow.md) §3 | which of these rules a machine enforces today, and which are only written down |

### Claude Code: `.claude/`

[`../.claude/settings.json`](../.claude/settings.json) is checked-in project policy. It keeps the
default permission mode manual, keeps project hooks enabled, disables bypass-permissions mode, and
retains the exact Read/Edit denies for `docs/plan`, project secret/database/key patterns, and
sensitive home credential locations. It also **intentionally disables Claude's OS sandbox**, so
permitted package-manager, Git/SSH and GitHub commands can use the host normally.

That trade is stated plainly because it decides what the rest of the layer can claim: the denies are
tool-level policy, so a permitted shell subprocess has ambient host filesystem, network,
environment, and credential access — including whatever SSH and GitHub credential helpers reach.
This project therefore claims no subprocess credential scrubbing, no metadata-endpoint denial, and
no command-egress boundary.

Hooks run through the shell-free Node launcher `.claude/hooks/run-python-hook.mjs`, and the matchers include
the real `PowerShell` tool as well as Bash. The immutable-path and docs-link hooks **fail open** on
malformed input or a Git failure — with a visible structured warning, so a broken parser cannot
brick a session silently — while the ConfigChange settings validator **fails closed**. It can refuse
a weakening for the current session; it cannot erase the attempted disk edit or replace
CLI/startup/managed policy. No lexical command parser can prove the target of dynamically
constructed interpreter code; the staged-index guard and the trusted-base CI check are the backstops.

The path-scoped standards in [`../.claude/rules/`](../.claude/rules/) cover pure Rust domain code,
forward-only migrations, and Arabic-first frontend work. The fourth — security: authentication,
secrets, payments, PII, logging, and compliance language — carries no path scope and always applies,
because a never-log rule that arrives once you are editing the logger arrives too late.

### Codex: `AGENTS.md`, `.codex/`, and `.agents/`

[`../AGENTS.md`](../AGENTS.md) is Codex's repository instruction file and routes to `CLAUDE.md`, the
conventions, the applicable Claude rules, and the project skills, so there is no second copy of the
engineering law.

[`../.codex/config.toml`](../.codex/config.toml) keeps work in a `workspace-write` sandbox with
network access deliberately on inside that write boundary — reading the world is free, writing is
visible — requires approval for escalation and reviewed mutating operations, filters
credential-shaped environment variables including `DATABASE_URL`, and explicitly keeps hooks
enabled. [`../.codex/rules/safety.rules`](../.codex/rules/safety.rules) contains no `allow` rules on
purpose, prompts for history-changing Git, pushes, mutating GitHub commands, publishing, and
destructive database operations, and forbids `sqlx migrate revert` because this repository is
forward-only. [`../.codex/hooks.json`](../.codex/hooks.json) adds the immutable-path check before
shell or patch operations and the docs-link check after patches; a trusted Codex repository and
reviewed hook definitions are prerequisites, and these files are not an excuse to bypass hook trust.

[`../.agents/skills/`](../.agents/skills/) contains Codex's `add-migration` and `verify-schema`
procedures, with synchronized native copies under `.claude/skills/`. The repository tests both
contracts, so a handoff between agents cannot lose runtime-array parity, PostgreSQL scratch safety,
or the required verification sequence.

## 6. Git safeguards: `.githooks/`

Git executes these only after `just setup` configures `core.hooksPath`, and the LF rules in
`.gitattributes` keep their shebangs executable after a Windows clone. Each hook's exact contract
and its honest limit are in
[`implementation/03-github-workflow.md`](implementation/03-github-workflow.md) §3; the guard table in
[`../CLAUDE.md`](../CLAUDE.md) lists what each refuses.

### `commit-msg`

One shared parser, [`../scripts/validate-change-title.sh`](../scripts/validate-change-title.sh),
validates both commit subjects and squash-merge pull-request titles — the same grammar, because a
squash merge commits the title:

```text
<type>(<scope>): <summary>   [<step>]
```

The type and scope lists are closed. A step is `N.N.N`, an ordered inclusive `N.N.N–N.N.N` range, or
`—`. [`../scripts/check-automation-attribution.py`](../scripts/check-automation-attribution.py)
rejects coding-assistant co-author and generated-by lines; the exact Dependabot author/trailer
combination is a narrow compatibility exception, not cryptographic proof of App identity.

### `pre-commit`

[`../scripts/check-staged-policy.py`](../scripts/check-staged-policy.py) inspects the index — the
exact content Git will commit, not the working copy — and fails closed on a Git error. It rejects a
staged write under `docs/plan`, a change/delete/rename of a migration already in `HEAD`, sensitive
and generated paths, and an oversized staged blob.
[`../scripts/scan-secrets.sh`](../scripts/scan-secrets.sh) then runs Gitleaks over staged content,
which catches a token in an ordinary filename that no path list could. Findings are redacted and
scanner failure refuses closed.

### `pre-push`

Uses the destination remote Git supplies rather than hardcoding `origin`. It scans every commit
absent from that remote for assistant attribution, runs Gitleaks over reachable history, and refuses
direct, force, and deletion pushes to `development`, `staging`, and `main`. Tags are append-only:
a new tag is allowed, moving or deleting an existing one is not.

It remains a local safety belt. `--no-verify` or a clone that skipped setup bypasses it, and the
repository deliberately offers no environment-variable override. Server workflows are the evidence
layer, and without branch protection red evidence still cannot stop the repository administrator.

## 7. Repository verification: `scripts/`

Small policy programs, each with negative self-tests that `just guards` runs. About eleven thousand
lines of them decide whether a migration may be edited or a secret may be committed, so
`lint-scripts.sh` lints them too.

One honest exception before the table: **`check-staged-policy.py` has no step of its own in
`ci.yml`.** It inspects the *staged index*, which a CI checkout has no equivalent of, so CI reaches
it the only way that means anything — through `.githooks/pre-commit`, which
[`.githooks/test-hooks.sh`](../.githooks/test-hooks.sh) drives against a real index. Every other
script below runs in CI as well as locally.

| Script | Contract |
|---|---|
| `verify-schema.py` | the Rust `MIGRATIONS` array exactly and uniquely matches every SQLite migration on disk in order; each file applies with the runtime transaction and `user_version` update; the executable schema reference also applies to real SQLite and obeys naming/reference rules |
| `verify-pg-migrations.py` | every PostgreSQL migration declares its SQLite mirror or server-only status, contains SQL rather than `psql` commands, both sides are accounted for, PG18 image/storage policy agrees, and the mirror applies to an exact-major real engine when available |
| `check-domain-purity.py` | `pos-domain`'s resolved normal graph has no RNG/UUID generation feature and its source has no direct clock/random calls; the broader no-I/O rule remains an architectural review requirement |
| `check-domain-acyclic.py` | `pos-domain`'s module graph has no cycles, at module rather than item granularity |
| `check-workspace-lints.py` | every Cargo member inherits the exact workspace lint levels, so the lint table is the whole lint scope of every gate |
| `check-test-catalog.py` | every catalogued test name resolves to its runner or sits in the shrinking `PLANNED` allowlist with a tombstone for anything retired; every normative reference name has one phase-microstep owner; every `E.n` a phase file claims has a row for that phase; the coverage arithmetic is recomputed from the rows |
| `check-staged-policy.py` | the staged index carries no plan edit, committed-migration change, sensitive path, or oversized blob |
| `check-protected-paths.sh` | a pull request does not edit a base-committed migration or source plan |
| `scan-secrets.sh` | staged, range, or reachable-history content has no known secret |
| `validate-change-title.sh` / `check-automation-attribution.py` | one grammar for commit subjects and pull-request titles, and no coding-assistant attribution anywhere |
| `gh-actions-policy.sh` | workflow Actions use full SHAs from the local repository allowlist before enabling GitHub's SHA-only setting |
| `check-branch-workflow-policy.rb` | the read-only trusted pull-request boundary and the frozen policy surface — workflows, Git hooks, both agent entry points, and the `.claude/`/`.codex/` trees — cannot silently weaken themselves |
| `watch-pr-checks.sh` | every expected workflow/job for one immutable PR head has registered and passed before a documented merge proceeds |
| `test-gh-setup.sh` | mocked GitHub API/list/parse failures make bootstrap and project setup stop instead of reporting false success or creating duplicates |
| `check-logical-css.sh` | frontend paths use logical rather than physical layout properties |
| `check-prop-test-names.py` | property tests retain the `prop_` names the verification filters depend on |
| `check-doc-links.sh` / `check-doc-links.py` | local inline and reference targets in every tracked Markdown document resolve with exact filename case; code examples are ignored, and heading anchors are not validated |
| `check-node-version.py` | exact Node runtime, root engine, Node typings, pnpm resolver and setup-node workflows agree with `.nvmrc` |
| `check-web-build-coverage.py` | every non-root pnpm workspace package declares a build before recursive type/build execution |
| `check-js-licenses.py` | installed JavaScript licence expressions form a non-empty inventory accepted by the reviewed repository policy |
| `check-justfile-policy.py` | a user-supplied `just` argument never becomes shell source |
| `lint-scripts.sh` | Ruff, ShellCheck and `ruby -c` over the policy code itself, fail-closed when a linter is missing |

Two safety properties worth meeting early. **The PostgreSQL verifier never applies migrations to the
database named by `DATABASE_URL`**: it creates a collision-resistant scratch database, drops it in
`finally`, ignores user `psql` startup files, and refuses a server whose major version differs from
the repository pin. Without a suitable server it uses a uniquely named throwaway container, and if
neither path is available it says the engine pass was skipped rather than calling mapping-only
success an engine success.

And **a pull request cannot weaken its own verifier**: the protected-path CI job checks the policy
script out from the exact trusted base revision into a temporary directory and passes the PR
revisions in as data.

`gh-actions-policy.sh` is a post-merge activation tool. It audits every checked-in `uses:` reference
against the repository allowlist and full-SHA grammar, then enables GitHub's SHA-only Actions
setting. Run it only after the hardened workflow files are on the default branch; its dry-run and
self-test are safe before then.

## 8. GitHub automation: `.github/`

Every external Action reference is pinned to a complete commit SHA with a human-readable release
comment. Workflows set explicit token permissions, realistic timeouts, and
`persist-credentials: false` for checkouts that perform no authenticated Git operation. Cargo
commands that resolve dependencies use the committed lockfile.
[`implementation/03-github-workflow.md`](implementation/03-github-workflow.md) §3 is the honest
rule-by-rule table; this is the inventory.

### CI

`ci.yml` runs on work and long-lived branches: `rust` (formatting, Clippy, locked tests, domain
structure/purity, property naming, exact SQLite runtime parity, and the PostgreSQL mirror on a
unique scratch database), `guards` (every Claude, Codex, Git, schema, title, attribution, secret,
and workflow-policy negative test), `web` (Biome, logical CSS, tests, builds/type-checking,
test-script reporting, and documentation links), `supply-chain` (trusted-range secret scanning plus
Rust/npm advisories), and `cross-platform` (core crates plus a real Tauri build on Linux, macOS and
Windows for promotions and release branches, before tag time). The PostgreSQL service is
digest-pinned. Superseded work-branch runs may cancel; promotion and release-branch evidence does not.

### Branch flow and protected paths

`branch-flow.yml` validates legal head/base pairs, the originating repository for official
promotions and hotfix back-merges, branch naming, the shared change-title grammar, and automation
attribution. Its protected-path job fetches the pull request's objects but executes the **trusted
base revision's** verifier, because `pull_request` code is untrusted even when the workflow token is
read-only.

### Labels

`labeler.yml` runs under `pull_request_target` with the narrow write permission needed to label a
pull request, and never executes pull-request code: path labels come from the pinned labeler Action,
then the exact base SHA is checked out to derive `type:` from the trusted title-mapping script. The
path map in [`../.github/labeler.yml`](../.github/labeler.yml) covers the Rust crates, applications,
shared money package, implementation docs, migrations, fiscal/compliance material, immutable sales,
authentication, CSP/capabilities, agent controls, hooks, and workflow security.

### Security maintenance

`security.yml` runs actionlint and zizmor when automation changes. A weekly and manual job scans full
Git history and reruns Rust/npm advisory checks even when the repository has had no recent pull
request. It uses check annotations rather than the code-scanning storage a private repository on this
plan does not have.

### Releases

`release.yml` creates a **draft** release only after a guarded, multi-platform path: exact final or
RC/beta SemVer grammar; a signed annotated tag whose signature GitHub reports as verified; the
current `main` tip for a final or `staging` tip for a candidate; matching versions in every
maintained application and workspace source; a successful `ci.yml` push run for the exact tag SHA
and branch; both updater-signing secrets and the committed updater public configuration present
before compilation; independent platform build/sign jobs with read-only repository tokens; a minimal
publisher job with `contents: write` and no signing secrets; and an SPDX JSON SBOM plus a SHA-256
manifest over the release assets.

Release immutability is enabled live, so a published release's tag and assets cannot be silently
replaced. Releases are nevertheless intentionally blocked until a human configures verified tag
signing, the updater key and public configuration, and platform signing/notarization — and until the
signing step is split off the step that compiles third-party code
([`implementation/ref/security-compliance.md`](implementation/ref/security-compliance.md) §6b). A
draft is not evidence that an installer is safe to distribute.

### Dependabot, templates, and ownership

Dependabot groups monthly Cargo, npm, Actions, and Docker Compose updates against `development`, and
Action updates preserve full-SHA pins. Monthly and grouped is deliberate: an unread dependency bump
is how a supply-chain problem arrives politely. The issue forms and pull-request templates demand
the project-specific evidence a reviewer needs.

`CODEOWNERS` records intended ownership but is **not active** for automatic review assignment or
enforcement on this private Free-plan repository. Branch protection and rulesets are likewise
unavailable, as are GitHub-native secret scanning and push protection — the independent Gitleaks
implementation is this repository's content scanner. Exact selected-Action patterns are also
unavailable here, so the local allowlist plus the SHA-only setting cover immutability once the
post-merge activation succeeds. These limitations are stated, never presented as completed controls.

## 9. Code and application directories

### `crates/`

| Crate | Holds |
|---|---|
| `pos-domain` | pure money and business decisions; time and identifiers are arguments. UUID serialization is available, generation and runtime RNG are not |
| `pos-db` | SQLCipher/SQLite, the runtime migration array, schema-version checks, repositories, and the migration/immutability/encryption tests |
| `pos-sync` | the shared outbox/cursor protocol boundary, client and server |
| `pos-hardware` | hardware traits and simulators, so application code can test failure modes without devices |
| `pos-test-support` | the shared proptest configuration and named strategies — **created at microstep 1.1.0** |
| `pos-fiscal` | UBL builder, the pinned code tables, the clearance queue, and the conformance harness — **created in group 2.7** |

`pos-fiscal` does not exist on disk yet. It is a separate crate on purpose: everything
reconstructed from the ISTD specification lives in one module, so an official change is one diff plus
its goldens.

### `apps/`

- `terminal` — the Tauri register: React under `src/`, Rust orchestration under `src-tauri/`, and a
  narrow capability definition. It is the only place that orchestrates read → domain → write.
- `server` — the Axum/PostgreSQL cloud service, and the owner of the timestamped sqlx migrations.
- `backoffice` — the React administration application.

### `packages/`

- `money` — keeps frontend minor-unit formatting and arithmetic consistent with Rust; covered by
  both the frontend and security path rules.
- `api-types` — the home for Rust-generated IPC types as that surface grows.
- `ui` — the shared component package.

The implementation plan, not this orientation, says what is complete in each directory and what
comes next.

## 10. Schema and migration workflow

Migrations are forward-only, and one already present in `HEAD` is never edited, deleted, renamed, or
replaced — deleting or renaming *is* editing. A correction is the next migration; `sqlx migrate
revert` is forbidden. Both trees accept only repository-owned regular SQL files: symlinks, gitlinks,
devices, and every other form of filesystem indirection are rejected.

The law is [`implementation/01-conventions.md`](implementation/01-conventions.md) §9, the target
shapes are [`implementation/ref/schema.md`](implementation/ref/schema.md), and the step-by-step
sequence is [`implementation/02-development-workflow.md`](implementation/02-development-workflow.md)
§4.5 — or the `add-migration` skill, which performs it correctly. Use the skill.

The one thing to internalise before you start: **`just verify-schema` applies exactly what the
application runs**, one transaction and one `user_version` update per file, so a runtime entry that
is omitted, duplicated, reordered, malformed, or missing from disk fails before any schema claim is
made. A directory that looks right is not a verified chain.

## 11. Daily change and delivery flow

```text
work branch → development → staging → main
               squash       merge      merge
```

Normal work branches from `development` and returns through a squash pull request whose **title** is
what gets committed. Promotions use merge commits so the branches keep shared ancestry; squashing one
forks them permanently. A production hotfix branches from `main`, returns to `main`, and back-merges
through `main → staging → development`. Direct, force, and deletion pushes to the long-lived branches
are refused locally. Coding assistants never receive commit or pull-request attribution.

Before a normal push:

```bash
just pre-push
git status --short
```

The release sequence adds exact-commit CI evidence, a signed verified annotated tag at the correct
branch tip, signing material, checksums, an SBOM, and human review of the draft. Every command, and
the hotfix path, are in
[`implementation/03-github-workflow.md`](implementation/03-github-workflow.md).

## 12. What is enforced, and where it stops

| Layer | What it contributes | Limit |
|---|---|---|
| compiler/lints/tests | domain, money, schema, frontend, and behavior checks | only the behavior actually encoded is proved |
| agent permissions, the Codex sandbox, and Claude/Codex hooks | safer agent execution and immediate immutable/docs feedback | Claude shell subprocesses have ambient host access; client support and lexical parsing limits apply |
| Git hooks | staged policy, content scanning, message/history policy, branch-push safety | local and intentionally bypassable |
| GitHub workflows | trusted-base policy, CI, security analysis, releases, logged evidence | red checks cannot block this repository's administrator without protection |
| GitHub live settings | read-only default token posture, full-SHA policy after post-merge activation, immutable published releases | private-Free plan omits branch protection, active CODEOWNERS, and native secret scanning |

No compliance validation is complete. No text in this repository should claim PCI DSS, SAQ, JoFotara
certification, or PDPL registration without the evidence required by
[`implementation/ref/security-compliance.md`](implementation/ref/security-compliance.md). Where an
answer depends on an official source nobody has read yet, the reference documents carry a greppable
`⚠️ OPEN` block with a stated default and an owning microstep; the architecture-changing ones are
collected in [`implementation/00-master-plan.md`](implementation/00-master-plan.md) §4a.3.

## 13. Keeping this map accurate

Update this file when a directory's responsibility, supported command, control boundary, or known
platform limitation changes. Do not add counts, do not copy a moving phase status, and **if you find
yourself copying a rule into this file, write a pointer instead** — that is the failure mode this
file has already had once.

When a code change proves the implementation plan wrong, update the relevant implementation document
in the same pull request. When a new edge case appears, give it a row in
[`implementation/ref/test-catalog.md`](implementation/ref/test-catalog.md) — a test, an accepted
risk, an open question with a stated default, or an explicit out-of-scope. A surprise that becomes
none of those will happen again.
