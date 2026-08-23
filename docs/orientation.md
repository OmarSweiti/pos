# Orientation — this repository in plain English

This is the map for a new human or coding agent: what the project is, where its
authority lives, what each control layer does, and how a change moves from an
idea to a release candidate.

This file deliberately contains no copied phase status, test totals, migration
totals, action SHAs, or line counts. Those facts change whenever the repository
does. The commands and source files below are the authoritative way to measure
them.

## 1. Start with the authority

Read these in order before changing code:

1. [`../AGENTS.md`](../AGENTS.md) is Codex's repository entry point.
2. [`../CLAUDE.md`](../CLAUDE.md) is the maintained cross-agent operational
   overview.
3. [`implementation/README.md`](implementation/README.md) locates the current
   implementation work.
4. [`implementation/01-conventions.md`](implementation/01-conventions.md) is the
   engineering law.
5. [`implementation/02-development-workflow.md`](implementation/02-development-workflow.md)
   defines the daily loop and commands.
6. [`implementation/03-github-workflow.md`](implementation/03-github-workflow.md)
   defines branches, pull requests, promotions, and releases.

Files under [`plan/`](plan/) are read-only source material. Corrections and
implementation discoveries belong under [`implementation/`](implementation/),
never in the source plans.

To establish the repository's current state, run the commands rather than
copying a snapshot into documentation:

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

This is a cross-platform, offline-first point of sale for Jordan. The register
is a Tauri application with a React interface and Rust core. It writes to an
encrypted local SQLite database and synchronizes with an Axum/PostgreSQL service
when connectivity returns. Arabic and right-to-left layout are the default.

The architecture has four boundaries:

```text
React UI
   │ typed Tauri commands
   ▼
Rust shell / server orchestration
   ├── pos-domain: pure decisions and arithmetic
   └── pos-db: persistence, migrations, and repositories
```

The nine invariants are specified in
[`implementation/01-conventions.md`](implementation/01-conventions.md). The
short form is:

- Money is signed `i64` minor units; floating point never touches money.
- Currency exponent is data; JOD has three decimal places.
- Quantity is signed `i64` milli-units.
- Completed sales are immutable; corrections are new documents.
- Sale lines snapshot the name and price the customer saw.
- Stock is an append-only ledger with a rebuildable on-hand view.
- Ordering uses server versions and supplied UUIDv7 values, never device time.
- `pos-domain` performs no I/O, clock access, or random generation.
- A fact and its outbox row commit in one transaction.

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

Important root files:

| File | Purpose |
|---|---|
| [`../Cargo.toml`](../Cargo.toml) | Rust workspace, shared dependencies, and lint policy |
| [`../Cargo.lock`](../Cargo.lock) | committed Rust resolution |
| [`../package.json`](../package.json) | pnpm workspace root and package-manager pin |
| [`../pnpm-lock.yaml`](../pnpm-lock.yaml) | committed JavaScript resolution |
| [`../rust-toolchain.toml`](../rust-toolchain.toml) | Rust toolchain pin |
| [`../biome.json`](../biome.json) | TypeScript/React formatting and linting |
| [`../deny.toml`](../deny.toml) | Rust advisories, licences, bans, and source policy |
| [`../.gitleaks.toml`](../.gitleaks.toml) | repository secret-scanning policy |
| [`../.gitattributes`](../.gitattributes) | LF normalization for hooks/tooling and binary declarations |
| [`../SECURITY.md`](../SECURITY.md) | vulnerability reporting and honest current security posture |

## 4. The supported command surface

Use `just` recipes rather than assembling a parallel command sequence.
`just --list` is authoritative; these are the families to remember.

### Setup

`just setup` deliberately does the local safety work first:

1. set `core.hooksPath` to `.githooks`;
2. set the repository-local author email;
3. require working Gitleaks and Ruby/Psych policy tools;
4. install pnpm dependencies from the frozen lockfile;
5. fetch Cargo dependencies with `--locked`.

A dependency/network failure therefore does not leave the clone without hooks.
The hooks are still local and bypassable; setup does not turn them into branch
protection.

### Development and verification

| Recipe | Purpose |
|---|---|
| `just check` | compile and type-check every Rust target |
| `just lint` | formatting, Clippy, architecture, domain purity, schema, PG mapping, RTL, property names, Biome, and docs links |
| `just test` | workspace Rust and declared pnpm tests |
| `just build-web` | production web builds and TypeScript checking |
| `just guards` | every negative guard, hook, skill, and policy self-test |
| `just secrets` | Gitleaks scan of all reachable Git history |
| `just audit` | time-varying Rust and npm supply-chain audit |
| `just pre-push` | lint, test, web build, guards, and full-history secret scan |
| `just verify-schema` | exact runtime migration parity and real SQLite verification |
| `just verify-pg` | PostgreSQL mapping plus an engine pass when available |
| `just domain-purity` | prove the domain's normal dependency graph and source remain pure |

`just audit` is intentionally outside `just pre-push`: advisory databases
change independently of a code diff. CI owns that time-varying gate, and the
weekly security workflow reruns it when no pull request is active.

## 5. Agent setup: shared law, client-specific boundaries

The project keeps Claude and Codex aligned at the level that matters: both read
the same conventions, both protect immutable paths, both expose the same schema
skills, and Git/CI backstop both clients. Their native configuration formats are
different, so the adapter files are not expected to be byte-identical.

### Claude Code: `.claude/`

[`../.claude/settings.json`](../.claude/settings.json) is checked-in project
policy. On supported macOS/Linux/WSL2 hosts it:

- enables Claude's OS sandbox;
- keeps the default permission mode manual;
- explicitly keeps project hooks enabled;
- disables bypass-permissions mode, sandboxed Bash auto-approval, and automatic
  unsandboxed retry, and refuses closed if the OS sandbox cannot start;
- limits writes to the project and denies writes to `docs/plan`;
- denies explicit project secret/database/key patterns and sensitive home
  credential locations. Claude resolves overlapping entries by the more-specific
  path; this policy still avoids a broad project `allowRead` so the least-privilege
  boundary stays obvious. The tracked `.env.example` template remains readable
  while arbitrary `.env.<suffix>` live-secret names do not;
- removes common API, package-registry, database, cloud, GitHub, and Tauri
  signing variables from sandboxed subprocesses;
- pre-approves no network domain and denies metadata endpoints.

An ordinary new network host can still produce a user prompt. Project settings
cannot impose Claude's effective strict network allowlist; the documentation
therefore does not claim a hard network deny.

The hooks use the current exec-form configuration: Node invokes
`hooks/run-python-hook.mjs`, which resolves the repository and launches the
Python policy without POSIX environment interpolation. Matchers include the real
`PowerShell` tool as well as Bash.

- `protect-immutable.py` rejects writes, deletes, moves, output redirection,
  `git diff/show --output`, `touch`, copy target-directory forms, PowerShell
  write verbs, and literal interpreter writes targeting committed migrations or
  `docs/plan`.
- `docs-links-on-write.py` reports documentation links broken by an edit, shell,
  PowerShell, or monitored command.
- `validate-settings.py` rejects a session-time weakening of the checked-in or
  local Claude configuration and loss of required skill contracts.

No lexical command parser can prove the target of dynamically constructed
interpreter code. The staged-index guard and trusted-base CI check are the
backstops. The immutable/docs hooks fail open on malformed input or repository/Git
failure so they cannot brick a session, but emit a visible structured system
message rather than failing silently. The ConfigChange settings validator is the
exception: it fails closed. It can reject a weakening for the current session;
it does not erase the attempted disk edit or replace CLI/startup/managed policy.

Claude's OS sandbox is not available on native Windows. The portable launcher
and genuine PowerShell-shaped tool routing are contract-tested, but native
Windows process dispatch was not exercised here. Git hooks and CI provide
cross-platform backstops and signals; a red CI result cannot block the repository
administrator on this Free plan.

The path-scoped standards in [`../.claude/rules/`](../.claude/rules/) cover:

- pure Rust domain code;
- forward-only SQLite/PostgreSQL migrations;
- Arabic-first frontend work, including shared money code and each app's HTML;
- authentication, secrets, payments, PII, logging, and compliance language.

### Codex: `AGENTS.md`, `.codex/`, and `.agents/`

[`../AGENTS.md`](../AGENTS.md) is Codex's repository instruction file. It routes
Codex to `CLAUDE.md`, conventions, the applicable Claude rules, and the project
skills; this prevents a second copy of the engineering law.

[`../.codex/config.toml`](../.codex/config.toml) keeps repository work in a
workspace-write sandbox, disables network access inside that sandbox, requires
approval for escalation, filters credential-shaped environment variables, and
explicitly keeps hooks enabled. User preferences such as model or UI remain in
the user's Codex configuration rather than the repository.

[`../.codex/rules/safety.rules`](../.codex/rules/safety.rules) prompts for
history-changing Git, pushes, GitHub mutations, publishing, and destructive
database operations. `sqlx migrate revert` is forbidden because this repository
is forward-only.

[`../.codex/hooks.json`](../.codex/hooks.json) adds immutable-path checks before
shell or patch operations and a docs-link check after patches. A trusted Codex
repository and reviewed hook definitions are prerequisites; these files are not
an excuse to bypass hook trust.

[`../.agents/skills/`](../.agents/skills/) contains Codex's `add-migration` and
`verify-schema` procedures. Claude has synchronized native copies under
`.claude/skills/`. The repository tests both contracts so a handoff between
agents does not lose runtime-array parity, PostgreSQL scratch safety, or the
required verification sequence.

## 6. Git safeguards: `.githooks/`

Git executes these only after `just setup` configures `core.hooksPath`. The LF
rules in `.gitattributes` keep their shebangs executable after a Windows clone.

### `commit-msg`

One shared parser, [`../scripts/validate-change-title.sh`](../scripts/validate-change-title.sh),
validates both commit subjects and squash-merge PR titles:

```text
<type>(<scope>): <summary>   [<step>]
```

The type/scope lists are closed. A step is `N.N.N`, an ordered inclusive
`N.N.N–N.N.N` range, or `—`. The subject before the tag is at most 72 characters
and does not end in a period.

[`../scripts/check-automation-attribution.py`](../scripts/check-automation-attribution.py)
rejects coding-assistant co-author and generated-by lines. Repository automation
is different: a commit whose author name and email exactly match Dependabot's
GitHub identity uses the same title grammar and `[—]`, and may retain that exact
Dependabot trailer. Git author metadata is locally configurable, so this is a
narrow compatibility exception rather than cryptographic proof of App identity.

### `pre-commit`

[`../scripts/check-staged-policy.py`](../scripts/check-staged-policy.py) inspects
the index—the exact content Git will commit. It is NUL-safe, escapes control
characters in diagnostics, fails closed on Git errors, measures the staged blob
rather than the working copy, and avoids the `git | grep -q` SIGPIPE race.

It rejects:

- any staged write under `docs/plan`;
- changes, deletes, or renames of a migration that already exists in `HEAD`;
- environment, database/sidecar, private-key, keystore, package credential,
  generated, or build-artifact paths;
- staged blobs over the repository size limit.

[`../scripts/scan-secrets.sh`](../scripts/scan-secrets.sh) then runs Gitleaks
against staged content. This catches a token in an ordinary filename, which a
path list cannot. Findings are fully redacted and scanner failure refuses closed.

### `pre-push`

The hook uses the destination remote Git supplies rather than hardcoding
`origin`. It scans every commit absent from that remote for assistant attribution,
runs Gitleaks over reachable history, and refuses direct, force, or deletion
pushes to `development`, `staging`, and `main`. Tags are append-only: creating a
new tag is allowed, while moving or deleting any existing tag is refused.

The hook remains a local safety belt. Git's `--no-verify` flag or a clone that
skipped setup can bypass it; the repository deliberately provides no additional
environment-variable override. The server workflows are the evidence layer;
without branch protection, red evidence still cannot stop the repository
administrator.

## 7. Repository verification: `scripts/`

The scripts are small policy programs with negative self-tests. The important
ones are:

| Script | Contract |
|---|---|
| `verify-schema.py` | the Rust `MIGRATIONS` array exactly and uniquely matches every SQLite migration on disk in order; each file applies with the runtime transaction and `user_version` update; the executable schema reference also applies to real SQLite and obeys naming/reference rules |
| `verify-pg-migrations.py` | every PostgreSQL migration declares its SQLite mirror or server-only status, both sides are accounted for, and the mirror applies to a real engine when available |
| `check-domain-purity.py` | `pos-domain`'s resolved normal graph has no RNG/UUID generation feature and its source has no direct clock/random calls; the broader no-I/O rule remains an architectural review requirement |
| `check-protected-paths.sh` | a PR does not edit a base-committed migration or source plan |
| `scan-secrets.sh` | staged, range, or reachable-history content has no known secret |
| `gh-actions-policy.sh` | workflow Actions use full SHAs from the local repository allowlist before enabling GitHub's SHA-only setting |
| `check-branch-workflow-policy.rb` | the read-only trusted PR boundary and its future workflow/policy executables cannot silently weaken themselves |
| `watch-pr-checks.sh` | every expected workflow/job for one immutable PR head has registered and passed before a documented merge proceeds |
| `test-gh-setup.sh` | mocked GitHub API/list/parse failures make bootstrap and project setup stop instead of reporting false success or creating duplicates |
| `check-logical-css.sh` | frontend paths use logical rather than physical layout properties |
| `check-prop-test-names.py` | property tests retain the `prop_` names used by verification filters |
| `check-doc-links.sh` | relative Markdown references under `docs/` resolve |

The PostgreSQL verifier never applies migrations directly to the database named
by `DATABASE_URL`. It creates a collision-resistant scratch database for the run
and drops it in `finally`. Without a suitable server it creates a uniquely named
throwaway container. The Compose service, CI service, and verifier share one
full Postgres image digest. If neither path is available, the script states that
the engine pass was skipped; mapping-only success is not described as engine
success.

The protected-path CI job checks out the policy script from the exact trusted
base revision into a temporary directory and passes the PR revisions as data. A
PR cannot weaken its own verifier and then use that version to approve a plan or
migration edit.

`gh-actions-policy.sh` is a post-merge activation tool. It first audits every
checked-in `uses:` reference against a repository allowlist and full-SHA grammar,
then enables GitHub's SHA-only Actions setting. Run it only after the hardened
workflow files are on the default branch; its dry-run and self-test are safe
before then.

## 8. GitHub automation: `.github/`

Every external Action reference is pinned to a complete commit SHA and annotated
with a human-readable release comment. Workflows set explicit token permissions,
realistic timeouts, and `persist-credentials: false` for checkouts that do not
perform authenticated Git operations. Cargo commands that resolve dependencies
use the committed lockfile.

### CI

`ci.yml` runs on work and long-lived branches:

- `rust` checks formatting, Clippy, locked tests, domain structure/purity,
  property naming, exact SQLite runtime parity, and the PostgreSQL mirror on a
  unique scratch database;
- `guards` runs every Claude, Codex, Git, schema, title, attribution, secret, and
  workflow-policy negative test;
- `web` runs Biome, logical CSS, tests, builds/type-checking, test-script
  reporting, and documentation links;
- `supply-chain` scans the trusted commit range for secrets and checks Rust/npm
  advisories and supply-chain policy;
- `cross-platform` tests the core crates and builds the real Tauri application
  on Linux, macOS, and Windows for promotions and release branches, before tag time.

The PostgreSQL service is digest-pinned. Superseded work-branch runs may cancel;
promotion/release-branch evidence does not.

### Branch flow and protected paths

`branch-flow.yml` validates legal head/base pairs, the originating repository for
official promotions and hotfix back-merges, branch naming, the shared change-title
grammar, and automation attribution. Promotion PRs receive an explicit warning
to use a merge commit.

Its protected-path job fetches the PR objects but executes the trusted base
revision's verifier. This matters because `pull_request` code is untrusted even
when the workflow token is read-only.

### Labels

`labeler.yml` runs under `pull_request_target` with the narrow write permission
needed to label a PR. It never executes PR code: path labels are applied by the
pinned labeler Action, then the exact base SHA is checked out to derive `type:`
from the trusted title-mapping script. The no-existing-type-label case is handled
without a `grep`/`pipefail` first-run failure.

The path map covers the Rust crates, applications, shared money package,
implementation docs, migrations, fiscal/compliance material, immutable sales,
authentication, CSP/capabilities, agent controls, hooks, and workflow security.

### Security maintenance

`security.yml` runs actionlint and zizmor when automation changes. A weekly and
manual job scans full Git history and reruns Rust/npm advisory checks even when
the repository has had no recent pull request. It uses check annotations rather
than unavailable private-repository code-scanning storage.

### Releases

`release.yml` creates a draft release only after a guarded, multi-platform path:

- exact final or RC/beta SemVer grammar;
- a signed, annotated tag whose signature GitHub reports as verified;
- the current `main` tip for a final or `staging` tip for a candidate—not an old
  ancestor;
- matching versions in every maintained application/workspace source;
- a successful `ci.yml` push run for the exact tag SHA and branch;
- both updater-signing repository secrets and the committed Tauri updater public
  configuration present before compilation;
- independent platform build/sign jobs with read-only repository tokens;
- a minimal publisher job with `contents: write` but no signing secrets;
- an SPDX JSON SBOM and SHA-256 manifest covering release assets.

GitHub release immutability is enabled on the live repository, so a published
release's tag and assets cannot be silently replaced. Releases are nevertheless
intentionally blocked until a human configures verified tag signing, the Tauri
updater key/public configuration, and platform signing/notarization. A draft is
not evidence that an installer is safe to distribute.

### Dependabot, templates, and ownership

Dependabot groups monthly Cargo, npm, Actions, and Docker Compose updates against
`development`. Action updates preserve full-SHA pins. The issue forms and PR
templates ask for the project-specific evidence reviewers need.

`CODEOWNERS` records intended ownership but is not active for automatic review
assignment or enforcement on this private Free-plan repository. Branch
protection/rulesets are likewise unavailable. GitHub-native secret scanning and
push protection are unavailable; the independent Gitleaks implementation is the
repository's content scanner. These limitations are stated, not presented as
completed controls.

Exact selected-Action patterns are not available for this private,
non-enterprise repository. The local action allowlist and GitHub SHA-only setting
cover immutability once the post-merge activation succeeds; the live SHA-only
setting is still pending that merge. The current `allowed_actions` mode is not
misrepresented as a selected-action allowlist.

## 9. Code and application directories

### `crates/`

- `pos-domain` contains pure money and business decisions. It accepts time and
  identifiers as arguments. UUID serialization is available; UUID generation
  and runtime RNG features are not.
- `pos-db` owns SQLCipher/SQLite, the runtime migration array, schema-version
  checks, repositories, and migration/immutability/encryption tests.
- `pos-sync` is the shared outbox/cursor protocol boundary.
- `pos-hardware` contains hardware traits and simulators so application code can
  test failure modes without real devices.

### `apps/`

- `terminal` is the Tauri register: React under `src/`, Rust orchestration under
  `src-tauri/`, and a narrow capability definition.
- `server` is the Axum/PostgreSQL cloud service and owns timestamped sqlx
  migrations.
- `backoffice` is the React administration application.

### `packages/`

- `money` keeps frontend minor-unit formatting/arithmetic consistent with Rust
  and is included in frontend/security path rules.
- `api-types` is the home for Rust-generated IPC types as that surface grows.
- `ui` is the shared component package.

The implementation plan, not this orientation, says what is complete in each
directory and what comes next.

## 10. Schema and migration workflow

Migrations are forward-only. Never edit, delete, rename, or replace one already
present in `HEAD`. A correction is the next migration; `sqlx migrate revert` is
forbidden. Both migration trees contain only repository-owned regular SQL files;
symlinks, gitlinks, devices, and other filesystem indirection are rejected.

For a schema change:

1. use the `add-migration` skill;
2. add the next SQLite file;
3. append its `include_str!` entry to the Rust `MIGRATIONS` array in the same
   change;
4. run `just verify-schema`—directory-only success is insufficient;
5. add the PostgreSQL mirror or a documented engine-specific exception;
6. run `just verify-pg` and state whether the real engine pass ran;
7. add behavior/data migration coverage and run the relevant Rust tests.

PostgreSQL migrations run transactionally unless their bytes begin exactly and
case-sensitively with SQLx's `-- no-transaction` marker. That escape is only for
statements PostgreSQL forbids inside a transaction and requires an explicit
partial-failure recovery test or procedure. The verifier uses the same boundary.

`verify-schema.py` applies exactly what the application runs, including one
transaction and one `user_version` update per file. An omitted, duplicated,
reordered, malformed, or nonexistent runtime entry fails before any schema claim
is made.

## 11. Daily change and delivery flow

```text
work branch → development → staging → main
               squash       merge      merge
```

- Normal work branches from `development` and returns through a squash PR.
- A squash title uses the same exact grammar as a local commit.
- Promotions use merge commits so the branches preserve shared ancestry.
- A production hotfix branches from `main`, returns to `main`, then back-merges
  through `main → staging → development`.
- Direct/force/deletion pushes to long-lived branches are refused locally.
- Coding assistants never receive commit/PR attribution. The exact Dependabot
  author/trailer combination remains visible under the normal `[—]` title, as a
  compatibility exception rather than proof of authenticated App provenance.

Before a normal push:

```bash
just pre-push
git status --short
```

The release sequence adds exact-commit CI evidence, a signed verified annotated
tag at the correct branch tip, signing material, checksums, an SBOM, and human
review of the draft. See
[`implementation/03-github-workflow.md`](implementation/03-github-workflow.md)
for the complete branch and hotfix commands.

## 12. What is enforced, and where it stops

| Layer | What it contributes | Limit |
|---|---|---|
| compiler/lints/tests | domain, money, schema, frontend, and behavior checks | only the behavior actually encoded is proved |
| Claude/Codex sandbox and hooks | safer agent execution and immediate immutable/docs feedback | client support and lexical parsing limits apply |
| Git hooks | staged policy, content scanning, message/history policy, branch-push safety | local and intentionally bypassable |
| GitHub workflows | trusted-base policy, CI, security analysis, releases, logged evidence | red checks cannot block this repository's administrator without protection |
| GitHub live settings | read-only default token posture, full-SHA policy after post-merge activation, immutable published releases | private-Free plan omits branch protection, active CODEOWNERS, and native secret scanning |

No compliance validation is complete. No text in this repository should claim
PCI DSS, SAQ, JoFotara certification, or PDPL registration without the evidence
required by [`implementation/ref/security-compliance.md`](implementation/ref/security-compliance.md).

## 13. Keeping this map accurate

Update this file when a directory's responsibility, supported command, control
boundary, or known platform limitation changes. Do not add counts or copy a
moving phase status here. Link to the maintained source or give the command that
measures it.

When a code change proves the implementation plan wrong, update the relevant
implementation document in the same pull request. When a new edge case appears,
give it a test, an accepted risk, or an explicit out-of-scope decision. A surprise
that becomes none of those will happen again.
