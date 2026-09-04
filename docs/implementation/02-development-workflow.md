# Development workflow — how the work actually gets done

[`01-conventions.md`](01-conventions.md) says what must be **true** of the code.
This file says what you **do**: in what order, with which command, and how you know you are
finished. It covers the parts of professional practice that are not code standards — branching,
review, manual verification, drills, cadence, release — because those are where a solo project
quietly loses its discipline first.

Where this file and [`01-conventions.md`](01-conventions.md) disagree, conventions wins and this
file is a bug.

Branches, issues, the project board, pull requests and the release channels have their own
companion: [`03-github-workflow.md`](03-github-workflow.md). This file stops at "push and open the
PR"; that one takes it from there.

| | |
|---|---|
| **[0 · The development-phase licence](#0--the-development-phase-licence)** | what is free to break right now, what never is, and when that stops |
| **[1 · Bring-up](#1--bring-up)** | tools, versions, first run, and the five commands that prove the machine works |
| **[2 · The three loops](#2--the-three-loops)** | inner / gate / full — which command answers which question |
| **[3 · Every command in this repo](#3--every-command-in-this-repo)** | every recipe, in one table |
| **[4 · The feature lifecycle — thirteen stations](#4--the-feature-lifecycle--thirteen-stations)** | one microstep from picking it to merged, including the diff-reading checklist |
| **[5 · Manual testing — the playbook](#5--manual-testing--the-playbook)** | test bed, console IPC, the Arabic pass, the keyboard pass, the ten-minute smoke, the drills |
| **[6 · Definition of done — the checklists](#6--definition-of-done--the-checklists)** | microstep · group · phase, as copy-paste lists |
| **[7 · Review, when you are the only reviewer](#7--review-when-you-are-the-only-reviewer)** | what replaces a second pair of eyes, and the reviewer checklist for this codebase |
| **[8 · Git discipline](#8--git-discipline)** | the four branches, commits, squash vs merge commit, and what is never committed |
| **[9 · CI, and reproducing it locally](#9--ci-and-reproducing-it-locally)** | the workflow jobs, and why a green machine can still fail CI |
| **[10 · Debugging and observability](#10--debugging-and-observability)** | backtraces, log filters, proptest regressions, and where a wrong number actually comes from |
| **[11 · Performance — measured, not asserted](#11--performance--measured-not-asserted)** | the four budgets and the rules for measuring them |
| **[12 · Security in the daily loop](#12--security-in-the-daily-loop)** | the never-list applied to a diff |
| **[13 · Dependencies and toolchain](#13--dependencies-and-toolchain)** | adding a crate or a package without breaking purity or the lockfile |
| **[14 · Documentation upkeep](#14--documentation-upkeep)** | which docs must change with the code, and the recurring audits |
| **[15 · Release](#15--release)** | tag-driven builds on two channels, and what is not signed yet |
| **[16 · Cadence — the rituals that keep this repeatable](#16--cadence--the-rituals-that-keep-this-repeatable)** | the rituals a solo project loses first |
| **[17 · Known gaps in the toolchain](#17--known-gaps-in-the-toolchain)** | commands that cannot work yet, and what closes each |
| **[18 · The forget-list](#18--the-forget-list)** | one screen of what gets forgotten |

---

## 0 · The development-phase licence

**Status today:** no pilot merchant, no installed register, no real sale row anywhere, no released
installer, one developer. This is the cheapest the project will ever be to change. Spend that.

Free right now — do it without ceremony, without a migration ritual, without asking:

| Change | Why it costs nothing today |
|---|---|
| Wipe the local register database | Every row in it is fixture data you can regenerate |
| `just db-reset` — drop the dev Postgres volume and re-migrate | Same |
| Rename a table, a column, a Rust type, a Tauri command | No installed build depends on the old name |
| Delete a crate, split a crate, move a module | The only consumer is in this repo |
| Change an IPC signature | The only client is `apps/terminal/src` |
| Redesign a screen | No cashier has muscle memory yet |
| Drop and rebuild a table **in a new migration** | Forward-only still holds — a rebuild is a legitimate migration |
| Throw away a day's work and do it differently | Cheaper now than the design being wrong in Phase 3 |

Still forbidden today, and permanently. These are not caution — they are the five things that stay
expensive no matter how early it is:

1. **Editing a committed migration.** Another database has already applied it. The fix is the next
   migration. A `PreToolUse` hook refuses the write; if you hit it, the guard is working.
2. **Editing `docs/plan/**`.** Those are source documents — inputs, not working files. Corrections
   land in `docs/implementation/`, which is why this set exists.
3. **Committing a secret.** If one is already in the tree, say so and stop. Do not rewrite history
   unasked.
4. **Claiming a compliance validation you have not completed** — "PCI compliant", "JoFotara
   certified", "SAQ done" — in code, comments, docs, UI copy, or a commit message. See
   [`ref/security-compliance.md`](ref/security-compliance.md) §3.
5. **A float in a money path.** `clippy::float_arithmetic` is denied workspace-wide; do not
   `#[allow]` it to get past a compile error. Fix the arithmetic.

**On squashing or renumbering migrations.** Do not. Once a migration is present in `HEAD`, it is
append-only even before a pilot: the runner, repository guards, and other clones all depend on that
history. If a numbering or schema mistake reaches `HEAD`, correct it with the next forward-only
migration and register that file in `MIGRATIONS`; never renumber the existing files or reset the
runtime array to rewrite history.

**When this licence expires:** the first day a register you do not own holds a sale you did not
create. Record that date here when it happens. After it, "we can just reset the database" stops
being a sentence anyone in this project says.

> Licence expired on: _(not yet — no external register exists)_

---

## 1 · Bring-up

### 1.1 What must be on the machine

The version column is the repository's tested target, not a minimum unless marked. CI installs
the entries marked as pins; `just setup` refuses when a required local tool is absent.

| Tool | Here | Check | Needed for |
|---|---|---|---|
| `rustc` / `cargo` | 1.97.1 (pinned) | `cargo --version` | everything; the pin lives in [`rust-toolchain.toml`](../../rust-toolchain.toml) |
| `cargo-nextest` | 0.9.143 | `cargo nextest --version` | `just test`, test filtering |
| `just` | 1.58.0 | `just --version` | every recipe in this file |
| `pnpm` | 11.22.0 | `pnpm --version` | the workspace; version is pinned by `packageManager` |
| `node` | 24.19.0 (pinned) | `node --version` | hooks, workspace, and CI; `.nvmrc` is the single runtime pin |
| `docker` | 29.7.2 | `docker compose version` | the dev Postgres |
| `sqlx-cli` | 0.9.0 | `sqlx --version` | server migrations |
| `sqlite3` | 3.51.0 | `sqlite3 --version` | migration dry-runs, `just verify-schema` |
| `python3` | 3.14.7 | `python3 --version` | `scripts/*.py`, the write guards |
| `ruby` + Psych | 4.0.5 / 5.3.1 | `ruby -rpsych -e 'puts Psych::VERSION'` | semantic GitHub Actions policy parsing |
| `gh` | 2.97.0 | `gh --version` | PRs from the terminal |
| `gitleaks` | 8.30.1 | `gitleaks version` | fail-closed staged and CI secret scanning |
| `ruff` | 0.16.4 (CI pin) | `ruff --version` | Python policy and guard linting |
| `shellcheck` | 0.11.0 (CI pin) | `shellcheck --version` | shell policy, hooks, and recipe linting |
| `cargo-deny` | 0.20.2 | `cargo deny --version` | advisories, licences, registries |
| C toolchain | Xcode CLT | `cc --version` | `rusqlite`, `openssl-src` |

Not installed yet, and the microstep that needs each — install when you get there, not now:

| Tool | Install | First needed |
|---|---|---|
| `sqlcipher` CLI | `brew install sqlcipher` | §5.7 — inspecting the encrypted register DB by hand |
| `criterion` (dep) | workspace `[dev-dependencies]` | 1.4.9 / 1.2.7 benchmarks |
| Playwright | `pnpm add -D @playwright/test` | 1.11.14 RTL screenshot baselines |
| WebdriverIO + `tauri-driver` | per 2.9.5 | 1.11.13 scan-latency trace — Playwright drives browser engines and cannot attach to a Tauri webview |
| `ts-rs` (dep) | workspace dependency | conventions §13 — generated TS types |

Gitleaks is a setup prerequisite, not an optional later tool: pre-commit refuses
closed when it is missing. Install a current v8 release using the platform instructions in
[the upstream Gitleaks README](https://github.com/gitleaks/gitleaks#installing), then run
`just setup` again.

### 1.2 First run, and after every pull

```bash
just setup                 # hooks + identity + policy-tool checks, then locked installs
just db-up                 # dev Postgres, detached
just migrate               # apply apps/server/migrations
```

`apps/server/.env` is git-ignored; `apps/server/.env.example` is committed. Copy it once:

```bash
cp apps/server/.env.example apps/server/.env
```

For the register's encrypted database, dev and CI use `POS_DB_KEY`; production uses the OS
credential store and **the release build refuses to honour the env var** (microstep 1.8.5). Put it
in your shell profile, not in a file in the repo:

```bash
export POS_DB_KEY=dev-only-not-a-secret
```

### 1.3 Prove the bring-up worked

Run the repository gates rather than relying on remembered test counts. If they pass, you have a
working machine and a recorded baseline for diagnosing a later failure.

```bash
just check                                        # workspace compiles
just lint                                         # code, architecture, purity, schema, RTL, web and docs checks
just test                                         # every Rust and declared workspace web test
just audit                                        # Rust advisories/licences + JS licences + npm advisories
just guards                                       # every negative guard and policy self-test
just secrets                                      # content-scan all reachable Git history
curl -s localhost:8080/health/db                  # after `just dev-server` in another shell
```

---

## 2 · The three loops

Development is three nested loops. Knowing which one you are in tells you which command to type.

| Loop | Cadence | Command | Answers |
|---|---|---|---|
| **Inner** | every few seconds | `just check`, `cargo nextest run -p <crate> -E 'test(<filter>)'` | "does this compile, does my one test pass" |
| **Gate** | before every commit | `just lint && just test` | "is the tree healthy" |
| **Full** | before every push | `just pre-push` | "will CI be green" |

The rule that keeps this cheap: **never run the outer loop to answer an inner-loop question.**
A 4-second `cargo check -p pos-domain` beats a 90-second `just lint` forty times a day.

### 2.1 Inner loop

```bash
cargo check -p pos-domain                          # one crate, fastest possible signal
cargo nextest run -p pos-domain                     # one crate's tests
cargo nextest run -p pos-domain -E 'test(tax::)'    # one module
cargo nextest run -p pos-domain -E 'test(prop_)'    # every property test
cargo nextest run -E 'test(inclusive_16pct)'        # one test, by name, anywhere
cargo nextest list --workspace                      # what tests even exist
bacon                                               # continuous check/clippy/test, installed already
```

Frontend:

```bash
pnpm --filter terminal exec vitest                  # watch mode
pnpm --filter terminal exec vitest run src/lib/money.test.ts
pnpm --filter terminal exec tsc --noEmit            # types only, no bundle
```

### 2.2 Gate loop

```bash
just fmt                # rewrite: cargo fmt + biome format
just lint               # formatting, Clippy, architecture, schema, RTL, web and docs checks
just test               # cargo nextest --locked --workspace + pnpm -r test
just audit              # Rust/JS licences and advisories — needs cargo-deny
just acyclic            # pos-domain module graph
just docs-links         # every local link in every tracked Markdown document resolves
just verify-schema      # exact runtime-array/disk parity, then runtime + reference SQLite
just secrets            # Gitleaks over all reachable Git history
```

### 2.3 Full loop

```bash
just pre-push           # lint + test + build-web + guards + secret history scan
pnpm --filter terminal tauri build     # a real packaged app; slow, do it per group not per commit
```

---

`build-web` is in there for one reason: **`tsc` runs nowhere else.** `just lint` is Biome, which is
style and correctness lints, not the type checker, and `just test` is Vitest. So a TypeScript type
error used to pass every local gate and fail CI's `web` job — which is exactly the failure mode
§2's contract exists to prevent.

---

## 3 · Every command in this repo

One table, so you never have to grep the [`justfile`](../../justfile).

| Command | What it does |
|---|---|
| `just` | list every recipe |
| `just setup` | install hooks and identity first, require Gitleaks, Ruff, ShellCheck and Ruby/Psych, then frozen pnpm install + locked Cargo fetch |
| `just hooks` | point Git at `.githooks` — a local, bypassable safety net, not branch protection |
| `just gitleaks-check` | fail clearly unless the content scanner required by pre-commit is installed |
| `just check` | `cargo check --locked --workspace --all-targets` |
| `just dev-terminal` | the register, Tauri dev, HMR on the React side |
| `just dev-backoffice` | the admin app, Vite dev server |
| `just dev-server` | Axum on `127.0.0.1:8080` |
| `just db-up` / `just db-down` | dev Postgres up / down (volume preserved) |
| `just db-reset` | Postgres **including its volume**, then re-migrate |
| `just db-local-reset` | delete this machine's register database |
| `just migrate` | `sqlx migrate run` against `DATABASE_URL` |
| `just fmt` | rewrite formatting, Rust and TS |
| `just lint` | fmt-check · Clippy/workspace lint contract · acyclic/domain-pure · SQLite/PG mapping · policy-script lint · logical CSS/property names · **test-catalog reconciliation** · **frontier reconciliation** · Biome · doc-links |
| `just test` | `cargo nextest run --locked --workspace` · `pnpm -r --if-present test` — until 2.9.6 adds the `soak` profile, the default nextest run still includes soak and long-chaos tests |
| `just test-catalog` | locally reconcile catalog and normative-reference test names with runner listings, the shrinking `PLANNED` ceiling and exact phase-microstep owners; `just lint` and `ci.yml`'s `rust` job run the full check, `just guards` and `ci.yml`'s `guards` job run its `--self-test` |
| `just frontier` | reconcile `README.md`'s declared completion frontier and every published phase total with the phase files that own them |
| `just audit` | Rust advisories/licences/bans/sources · reviewed JavaScript licence expressions · npm advisories |
| `just node-version-check` | exact runtime, root engine, Node typings, pnpm resolver and every CI setup-node step agree with `.nvmrc` |
| `just workspace-lints` | every Cargo member inherits the exact workspace lint levels |
| `just lint-scripts` | fail-closed Ruff, ShellCheck and Ruby syntax checks over policy code |
| `just acyclic` | `pos-domain`'s module graph has no cycles |
| `just domain-purity` | `pos-domain` has no runtime RNG/UUID-generation capability or direct clock/random calls |
| `just docs-links` | no missing local inline/reference target in any tracked Markdown document; examples in code fences are ignored |
| `just verify-schema` | requires exact Rust `MIGRATIONS`/disk parity, then executes the runtime chain and every SQL block in `ref/schema.md` against real SQLite |
| `just verify-pg` | exact PG18 image/storage policy, SQL-only mirror mapping, and migration execution against an exact-major scratch PostgreSQL server |
| `just prop-names` | property tests are `prop_<invariant>` — the prefix microstep 1.1.5's verify filter depends on |
| `just logical-css` | no physical CSS side in `apps/**` — §10 is RTL by default, so a physical side is a layout bug in Arabic |
| `just secrets` | Gitleaks over every commit reachable from the local repository, with findings redacted |
| `just guards` | the write guards **and** the git hooks still refuse what they must |
| `just build-web` | require a build script in all five workspace packages, then `pnpm -r build` — **the only place `tsc` runs** |
| `just pre-push` | `lint` + `test` + `build-web` + `guards` + full-history secret scan |
| `just bench-gate [budget]` | conventions §7's absolute limits and §7.1's regression rule. **Refuses today** — no reference register exists, so both hardware records are blank; deliberately not part of `pre-push` |
| `just branch <name>` | fresh `development`, then a branch off it — **needs a clean tree** (§4.2) |
| `just pr [title] [body-file] [milestone]` | gates → push → PR into `development` → watch CI. Pass the title on a branch with more than one commit (§4.12); the milestone is derived from a `phase-<0-5>/` branch name |
| `just merge [pr]` | work PR only: validate route/state, watch exact checks, re-read both tips, atomically match the reviewed head, squash and delete the branch |
| `just flow` | what is on `development` but not `staging`, and on `staging` but not `main` |
| `just promote-staging` | PR: `development` → `staging` (a release candidate) |
| `just promote-main` | PR: `staging` → `main` (production) |
| `just gh-bootstrap` | labels, milestones, merge behaviour, default branch — idempotent |
| `just gh-protect` | **refuses, deliberately.** Written against a 403 that a public repository no longer returns, so its `PUT` would now actually apply — with an incomplete required-check list. A reviewed ruleset is the intended replacement; none is configured yet |
| `just gh-project` | the delivery board and its fields |

Not yet recipes, and the microstep that creates each: `just seed` (1.12.1) · `just fuzz` and its
first target (1.2.8) · `just test-soak` and the `soak` nextest profile (2.9.6) · the TS type
generation gate (conventions §13). `just bench-gate` arrived with 1.2.0 and enforces nothing yet:
its budgets land at 1.2.7, 1.4.9, 1.6.2 and 1.11.13, its measurement job at 1.12.3, and it refuses
every run until 1.2.0's deferred half fills the reference-register records.

After 2.9.6 lands, **`just test` has a runtime budget: under three minutes on the reference
machine**, and suites that exceed it belong in the `soak` profile. Until then the default command
includes soak and long-chaos tests and no selection-policy budget is enforced, as §17 records. A
gate loop that takes twenty minutes is a gate loop that stops being run — which is the failure §2
exists to prevent.

`just verify-pg` never applies migrations to the database named in a connection URL. When a
development server is supplied, it creates a unique scratch database for that run and drops it in
`finally`; otherwise it uses a uniquely named throwaway Docker container. The Compose service, CI
service, and verifier all use the same full Postgres image digest. Every `psql` call ignores user
startup files, migration SQL cannot contain executable `psql` meta-commands, and the target must
report PostgreSQL major 18 before any scratch work begins. Without either engine path it reports a
mapping-only skip explicitly rather than calling the engine pass successful.

PostgreSQL 18 stores the Compose cluster at `/var/lib/postgresql/18/docker` while the named volume
mounts its parent, `/var/lib/postgresql`. A volume created with the former
`/var/lib/postgresql/data` mount is not automatically upgraded or relocated. During the current
pre-pilot reset licence, remove it deliberately with `just db-reset` if it is disposable; otherwise
stop and make a real backup/upgrade plan rather than treating a mount change as data migration.

---

## 4 · The feature lifecycle — thirteen stations

One microstep, start to finish. Stations 5–7 are conditional; the rest always apply.

```
1 pick  →  2 branch  →  3 failing test  →  4 implement  →  [5 schema]  →  [6 IPC]  →  [7 UI]
        →  8 read your own diff  →  9 gates  →  10 manual demo  →  11 commit
        →  12 push · CI · review · merge  →  13 close the loop on the docs
```

### 4.1 Pick exactly one microstep

**WIP = 1.** Two half-finished microsteps is the most expensive state this project can be in,
because neither is verifiable and `main` can absorb neither.

Before typing anything, read three things:

1. the microstep in its phase file — its `Files:`, `Tests:`, `Verify:`, `Done when:`
2. every `ref/` section it points at — those are **normative**, the phase file is a summary
3. the rows it closes in [`ref/test-catalog.md`](ref/test-catalog.md) (`E.n`), so you write the
   edge-case tests with the feature instead of in a sweep three weeks later

```bash
grep -n '^### 1\.3\.' docs/implementation/phase-1-sellable-mvp.md   # the group's microsteps
grep -n 'E\.1[89]' docs/implementation/ref/test-catalog.md          # the cases it must cover
```

If the microstep turns out to be wrong at the keyboard, **fix the microstep** — station 13. These
files are the plan of record, not a historical artefact.

Stuck for more than an hour? The phase file's dependency graph names the groups that are
independent of the one you are in (in Phase 1: 1.6 users/audit, 1.9 sequences, 1.10 stock ledger).
Switching to one of those is discipline, not avoidance. Switching to a *new* microstep in the
*same* group while the current one is half-done is not.

### 4.2 Branch

One branch per **group**, not per microstep — a group is the reviewable unit; a microstep is the
commit unit.

```bash
just branch phase-1/group-3-tax     # git switch development && pull --ff-only && switch -c
```

**`just branch` needs a clean tree.** It switches to `development` first, and git refuses that
switch when your uncommitted edits touch a file that differs between the two branches. When you
have already started work — the common case, because you rarely know it is a separate branch until
you are in it — carry it across by hand:

```bash
git stash push -u -m "wip"          # -u: without it, NEW untracked files stay behind
git switch development && git pull --ff-only
git switch -c chore/<slug>
git stash pop                       # conflicts surface here, not later
```

**Check what you are branching off, not just what you are branching to.** If the branch you are
standing on has commits that are not yet on `development`, branching from where you stand drags
them into your PR:

```bash
git merge-base --is-ancestor HEAD origin/development && echo "already integrated" || \
  git log --oneline origin/development..HEAD          # these would come along
```

Naming: `phase-<0-5>/group-<m>-<slug>`; the six maintained gates are Phase 0 through Phase 5.
Fixes that are not part of a group:
`fix/<slug>`, `chore/<slug>`, `docs/<slug>`; a version-preparation PR uses
`chore/release-vX.Y.Z`.
The `branch-flow` check refuses a name outside the scheme, and `.githooks/pre-push` refuses a
direct push to any of the three long-lived branches.

**Branch from `development`, never from `main`.** The flow is
`feature → development → staging → main`: `development` is the integration surface and the default
branch, `staging` is the tagged release candidate, `main` is what a merchant is running. The full
model, and why an installer product needs the middle branch a web service does not, is
[`03-github-workflow.md`](03-github-workflow.md) §1.

### 4.3 Write the failing test first

Not dogma — the test is how you discover you misread the spec, and it costs nothing while the code
does not exist yet. Pick the layer from conventions §5:

| The thing you are building | Layer | Lives in | Run it with |
|---|---|---|---|
| A rule a human would argue about | example `#[test]` | inline `mod tests` | `cargo nextest run -p <crate> -E 'test(<name>)'` |
| An invariant (§1, or an `E.n` row) | `proptest` | inline `mod tests` | `cargo nextest run -p <crate> -E 'test(prop_)'` |
| Receipt bytes, fiscal XML | golden file | `crates/*/tests/golden/` | `cargo nextest run -p <crate>` |
| A repository, a migration, a transaction | integration, real SQLite | `crates/*/tests/` | `cargo nextest run -p pos-db` |
| Sync convergence under replay/drop/reorder | chaos | `crates/pos-sync/tests/` | `cargo nextest run -p pos-sync` |
| A React helper, a formatter, a store | Vitest | `apps/*/src/**/*.test.ts` | `pnpm --filter terminal exec vitest run` |

Name them exactly as the microstep says — `<subject>_<behaviour>` for examples, `prop_<invariant>`
for properties. The names are referenced from [`ref/test-catalog.md`](ref/test-catalog.md); a
renamed test is a broken cross-reference nobody notices.

Confirm the test fails **for the right reason** before you make it pass. A test that passes against
an unimplemented function is testing nothing.

### 4.4 Implement

Smallest change that turns the test green, then a second pass for clarity. Stay inside the
microstep's `Files:` list — conventions §6.6 allows imports and module declarations, nothing else.
That constraint is what makes a bisect land on a microstep and tell you the truth.

The three architectural questions to ask at every keystroke:

- **Am I in `pos-domain`?** Then no I/O, no clock, no randomness — time and IDs are arguments (I-8).
  A new dependency in that crate's `Cargo.toml` is a design review, not an edit.
- **Am I in `pos-db`?** Then return owned domain types, never a `rusqlite::Row`; never compute a
  total; take an explicit `&Transaction` for every write that produces a fact (conventions §3).
- **Am I in the shell?** Then this is the only layer that orchestrates: read → domain → write, one
  `BEGIN`, one `COMMIT`, fact and outbox row together (I-9).

### 4.5 If it touches the schema

Use the `add-migration` skill — it does the whole sequence correctly. By hand it is:

```bash
ls crates/pos-db/migrations/                                   # next number, no gaps
# author crates/pos-db/migrations/000N_short_name.sql from ref/schema.md
# append the include_str! entry to MIGRATIONS in crates/pos-db/src/lib.rs
just verify-schema                                             # proves exact parity, then applies the RUNTIME chain
# mirror it in apps/server/migrations/ — same semantics, with a header comment
# saying "Mirrors SQLite 000N_short_name.sql"; the numbers cannot match
just verify-pg                                                 # the mirror, on a real server
cargo nextest run -p pos-db                                    # migrations + round-trip
```

`just verify-schema` replaces the `sqlite3 :memory: ".read …"` dry-run that used to be here.
That command applied only the files you named — so an `ALTER` against an earlier migration's
table went unchecked — and it accepts a `REFERENCES ghost(id)` in silence.

Non-negotiables, from conventions §9 and `.claude/rules/sql-migrations.md`:

- Forward-only. No down migrations. The rollback story is restore-from-encrypted-backup.
- The Rust `MIGRATIONS` array and the directory have exact, duplicate-free, ordered parity. A file
  on disk that runtime omits is a failed verification, not a shipped migration.
- Naming carries the units: `*_minor` · `*_milli` · `*_ppm` · `*_at` · `*_date` · `is_*` ·
  `<table>_id BLOB(16)` · enums as `TEXT` + `CHECK (x IN (…))`.
- A shape change ships its data migration **in the same file**, plus a test that seeds the old
  shape, migrates, and asserts the new one.
- No path that `UPDATE`s a completed sale. Not in DDL, not in a trigger, not in a private method.
- Never edit a committed one. The guard will stop you; the guard is right.

### 4.6 If it crosses the IPC boundary

Commands are the only channel from UI to core ([`ref/ipc-contract.md`](ref/ipc-contract.md)).

- `snake_case`, verb-first, returns `Result<T, IpcError>`.
- Declare its required capability in the registry. A command with no declaration fails the
  exhaustiveness test.
- **Check the permission in Rust, in the handler.** Hiding the button is UX; the check is security.
- TS types are generated from Rust, never hand-written. Two hand-maintained copies of a money type
  is how a rounding bug ships.
- Anything slow — card collection, printing, fiscal submission — returns a handle immediately and
  emits progress events. A cashier watching a dead spinner presses the button again.

### 4.7 If it touches the UI

From conventions §10 and [`ref/ui-spec.md`](ref/ui-spec.md):

- RTL is the default, not a mode. Logical properties only — `ps-*`/`pe-*`/`ms-*`/`me-*`/
  `start-*`/`end-*`. `pl-4` is refused by `just logical-css` unless the line carries the narrow,
  reviewed physical-layout exception documented in conventions §10.
- No string literals in components. Keys go in the typed catalog, `ar` and `en` in lockstep — a
  test fails when a key exists in one and not the other.
- Money and dates render through `formatMoney` / `formatDate`. Never an inline `toLocaleString`:
  display precision is a store setting.
- Western Arabic digits (0–9) everywhere, in both languages.
- Touch targets ≥ 48 px. Every action reachable from the keyboard alone.

### 4.8 Read your own diff before anyone else does

The highest-yield ten minutes in the loop. Read it as a reviewer, not as the author:

```bash
git add -A && git diff --cached          # or: git diff --stat main...HEAD
```

| Look for | Because |
|---|---|
| An `f64`, an `as f64`, a `/` on money | I-1. Intermediate math in `rust_decimal`, round **once** |
| A literal `100` near a currency | I-2. The exponent is per-currency data; JOD is 3 |
| `unwrap()`, `expect()`, `panic!`, `dbg!` outside tests | A panic in a register is a lost sale |
| A new `#[allow(...)]` | Either justified in a comment on the same line, or removed |
| An `UPDATE` touching a completed sale | I-4, with no exceptions |
| A repository that computes a total | Conventions §3 — that belongs in `pos-domain` |
| A fact write without its outbox row in the same transaction | I-9 |
| A logged customer name, phone, PAN, PIN, key, or token | `.claude/rules/security.md`. Not even in a test fixture that prints |
| A device clock used for ordering | I-7. Record it for humans; never branch on it |
| A file outside the microstep's `Files:` list | Scope creep, or a missing microstep |
| A `TODO` with no owner and no microstep number | Either do it or write it down where the plan lives |

Then the reviews you can run:

```bash
/review             # correctness, reuse, simplification over the diff
/security-review    # the never-list, over the branch
```

### 4.9 Gates

```bash
just lint && just test
```

Both clean, no ignored tests, no new warnings. If clippy fails on a lint you disagree with, the
conversation is about the code, not the lint level — `-D warnings` is the contract with CI.

### 4.10 Manual demo — the microstep's `Done when`

Run the exact command in the microstep's `Verify:` line, and make its `Done when:` sentence
objectively true. Then do the human check from §5. **A microstep is not done because the code looks
finished.** It is done because a command said so and you watched it happen.

### 4.11 Commit

One microstep, one commit. Conventions §8:

```
<type>(<scope>): <summary>            [<step>]
```

```bash
git commit -m "feat(domain): tax engine, inclusive + exclusive extraction   [1.3.4]"
git commit -m "fix(db): sale_line qty to milli-units                        [1.1.7]"
git commit -m "test(fiscal): discount percentage round-trip property        [2.7.6]"
git commit -m "docs(impl): phase 2 fiscal conformance harness              [—]"
```

`type` ∈ `feat` `fix` `test` `docs` `chore` `refactor` `perf`.
`scope` ∈ `domain` `db` `sync` `hardware` `fiscal` `terminal` `server` `backoffice` `repo` `impl`.
Both lists are **closed**, and enforced twice: `.githooks/commit-msg` on the way in, and the
`branch-flow` check on the PR title — which is what a squash-merge actually commits.
Summary in the imperative, no trailing period, ≤ 72 characters before the step tag.

The body — when the change deserves one — answers **why**, not what. The diff already says what.
Mention the invariant, the `E.n` case, or the correction (`C-1`…`C-4`) that forced the design.

**Check the subjects before you write any of them.** `commit-msg` is a plain script, so you can
call it on a file. Finding out mid-commit that a subject is two characters too long is avoidable:

```bash
f=$(mktemp); printf '%s\n' 'feat(domain): tax engine   [1.3.4]' > "$f"
.githooks/commit-msg "$f" && echo legal; rm -f "$f"
```

**Use `-F <file>` when the message describes a destructive command.** The write guard
(`.claude/hooks/protect-immutable.py`) scans the whole shell command, and a heredoc puts the
message text inside it — so a body explaining that you fixed the handling of, say, removing a plan
directory gets refused, having named a protected path next to a write verb. Writing the message to
a file first sidesteps it entirely, and is better practice for a long body anyway:

```bash
git add <only the files for this one concern>     # never `git add -A`
git commit -F .git/COMMIT_DRAFT
```

**Before the push, check whose commits these are.** `just setup` sets the identity per clone, but a
fresh clone that skipped it inherits whatever the machine's global config says. Coding assistants
remain tools and receive no co-author/generated-by trailer. The history validator has one narrow
compatibility exception: an exact Dependabot author name/email may retain the exact Dependabot
trailer. Git author metadata is locally configurable, so this string match preserves existing bot
history but is not cryptographic proof of GitHub App identity:

```bash
git log development..HEAD --format='%an <%ae>' | sort -u              # expect one line
git log development..HEAD --format='%B' | grep -iE '^co-authored-by|generated with'   # expect nothing
```

### 4.12 Push, CI, review, merge

```bash
just pr 'feat(domain): tax engine, inclusive + exclusive extraction   [1.3.4]'
work_pr=$(gh pr view --json url --jq .url)
IFS=$'\t' read -r work_base work_head < <(
  gh pr view "$work_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
bash ./scripts/watch-pr-checks.sh "$work_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$work_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$work_base" ] || [ "$current_head" != "$work_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$work_pr" --match-head-commit "$work_head" --squash --delete-branch
```

Longhand, when you want to see it:

```bash
just pre-push
git push -u origin phase-1/group-3-tax
work_pr=$(gh pr create --base development --title '<conventions §8 subject>' --body-file notes/pr.md)
IFS=$'\t' read -r work_base work_head < <(
  gh pr view "$work_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
bash ./scripts/watch-pr-checks.sh "$work_pr"       # exact route/path-derived workflow set
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$work_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$work_base" ] || [ "$current_head" != "$work_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$work_pr" --match-head-commit "$work_head" --squash --delete-branch
```

The before/after base and head readings are load-bearing. A mismatch invalidates the check
evidence and requires another watcher run. `--match-head-commit` locks only the head atomically;
this repository's maintained merge sequence has no equivalent atomic target-base lock. Serialize
maintainer merges—or temporarily freeze the target branch—during that final window. The immediate
base recheck narrows but cannot eliminate the residual race; §3 in
[`03-github-workflow.md`](03-github-workflow.md) is the authoritative runbook.

**The PR title becomes the commit.** A squash-merge discards your commit subjects and commits the
PR *title*, so the title obeys conventions §8 — and the `branch-flow` check enforces exactly that.
Your microstep messages survive in the squash body, which is where they belong.

**Which is why `just pr` takes the title.** Bare `just pr` fills it from the *first commit* on the
branch (`gh pr create --fill-first`). On a one-commit branch that is exactly right. On a branch
carrying a group of microsteps it is wrong in a way that is easy to miss: `development` gets a
commit describing microstep one and standing for all seven. Pass the title whenever the branch has
more than one commit, and `just pr` runs it through `commit-msg` before pushing anything, because
that is what it becomes. A second argument is a body file when the change deserves prose; without
one, the body is the list of microstep subjects.

A PR description that a reviewer (or you, in three months) can act on:

```markdown
## What
Group 1.3 — the tax engine. Microsteps 1.3.1 → 1.3.7.

## Why now
Nothing in the cart machine can be priced until inclusive extraction is exact.

## Invariants touched
I-1 (all intermediate math in rust_decimal, one rounding), I-2 (exponent from Currency).

## Verification
- `cargo nextest run -p pos-domain tax::` — 14 tests
- `prop_line_tax_sum_equals_receipt_tax` — 4096 cases
- Manual: hand-checked a 5-line mixed-rate basket against ref/tax-jordan.md §3, to the fil.

## Test catalog
Closes E.18, E.19. E.14 still open — needs cash rounding (1.5.x).

## Not in this PR
Reduced-rate seeding — that is a merchant decision (#10), tracked in ref/merchant-decisions.md.
```

Squash-merge, so `development` gets one commit per group with the microsteps in the body. Delete
the branch. Never merge red: a red `development` means the next person cannot tell whether they
broke it.

**Read the log of any check that can skip.** A green tick means the job exited 0, which is not the
same as the check having run. `verify-pg-migrations.py` exits 0 and prints `SKIPPED` when no
Postgres is reachable — by design, because a check that cannot run must not look like one that
passed. Once per gate, confirm from the log that it did the work:

```bash
gh run view --job <job-id> --log | grep -E 'engine:|SKIPPED'
```

The same applies to `pnpm -r --if-present test`, which is why CI has `report-test-coverage.sh`:
`--if-present` makes a package with no test script indistinguishable from one that passed.

Promotion — `development → staging → main` — is a different act with a different merge button, and
it is [`03-github-workflow.md`](03-github-workflow.md) §2 and §6. The short version: **a promotion
PR is merged with a merge commit, never squashed**, because squashing one forks the branches
permanently.

### 4.13 Close the loop on the documentation

The station that gets skipped, and the reason a doc set rots:

| If the work… | Then, in the same PR |
|---|---|
| deviated from `ref/domain-api.md` | fix the reference — silent divergence makes it a liability |
| changed the schema | update [`ref/schema.md`](ref/schema.md) and re-run `just verify-schema` |
| added or changed a command | update [`ref/ipc-contract.md`](ref/ipc-contract.md) |
| proved a microstep wrong | fix the microstep in the phase file |
| **completed a numbered product microstep** | move the **current implementation frontier** line in [`README.md`](README.md) to the next one. Nothing else maintains it, and a dated pointer nobody moves is worse than no pointer |
| surfaced a new edge case | it becomes **E.93** in [`ref/test-catalog.md`](ref/test-catalog.md) — with a test, a written accepted risk, an open question carrying a stated default and an owning microstep, or an explicit out-of-scope. A surprise that becomes none of those will happen again |
| superseded something a source plan states | record it in [`00-master-plan.md`](00-master-plan.md) §4a. `docs/plan/` is immutable and a reader is routed there first, so an uncorrected name is a name someone will build |
| hit an external unknown — legal, tax, regulatory, protocol | add a `⚠️ OPEN` block to the reference document that owns it: the question, the default the code runs on meanwhile, the owning microstep, and the source that settles it. **Never a guessed fact** |
| needed a merchant's answer | add it to [`ref/merchant-decisions.md`](ref/merchant-decisions.md) rather than guessing in code |
| ran a drill | a dated record in `docs/drills/` — §5.10 |
| changed a command or a gate | update this file |

```bash
just docs-links            # the doc set is only worth its cross-references
just test-catalog          # the catalog and normative test tables still match runners and owners
just frontier              # the declared frontier and published phase totals still match their phase files
```

---

## 5 · Manual testing — the playbook

Automated tests prove the **code** does what you meant. Manual testing proves the **product** does
what a cashier needs at 19:40 with six people in the queue. They fail differently, so neither
replaces the other, and in this project the manual pass has caught the things that cost money:
Arabic that shapes wrong on paper, a receipt that is correct and unreadable, a total that is right
and in the wrong place on the screen.

Two rules that make manual testing worth the time:

1. **Every manual finding becomes an automated test before the fix is committed.** Otherwise you
   will find it again, by hand, forever.
2. **Test on the seed fixture, never on ad-hoc data.** One shared world means a screenshot, a bug
   report, and a test all describe the same store. (`just seed` arrives at 1.12.1; until then, seed
   by hand and keep the SQL in `crates/pos-db/tests/`.)

### 5.1 Build a clean test bed

Never debug on a database you have been poking at for two days — half of what you see is your own
earlier experiments.

```bash
just db-local-reset        # delete this machine's register DB
just db-reset              # dev Postgres, volume and all, re-migrated
export POS_DB_KEY=dev-only-not-a-secret
just dev-terminal          # rebuilds the DB empty on launch
# just seed                # (1.12.1) the Jordanian minimarket fixture
```

### 5.2 Drive the register by hand

```bash
just dev-terminal          # Tauri window + Vite HMR on the React side
```

- **React changes** hot-reload. **Rust changes** rebuild and restart the app — expect the window to
  disappear and come back; that is not a crash.
- **Devtools:** right-click → Inspect Element (enabled in dev builds). Console, network, and the
  React tree all work as in a browser.
- **The webview is not a browser.** `fetch` to the outside is blocked by the CSP in
  `tauri.conf.json`, and no `fs`/`shell`/`http` plugin is exposed. If something cannot reach the
  core through a command, that is the design working.

**Calling a command straight from the console** — faster than clicking to the screen that calls it.
`withGlobalTauri` is deliberately off, so add a dev-only bridge rather than turning it on:

```ts
// apps/terminal/src/main.tsx — dev only, never in a release bundle
if (import.meta.env.DEV) {
  const { invoke } = await import("@tauri-apps/api/core");
  (window as unknown as { ipc: typeof invoke }).ipc = invoke;
}
```

```js
await ipc("split_tender", { totalMinor: 1250, parts: 3 })   // → [417, 417, 416]
```

Argument names are camelCase in TypeScript and `snake_case` in Rust; Tauri maps between them, which
is why `App.tsx` sends `totalMinor` to a command declaring `total_minor`.

### 5.3 The RTL and Arabic pass

Do this on **every** UI change, not at the end of the phase. Arabic layout bugs are cheap to fix the
day they appear and a redesign three weeks later.

| Check | How | Passes when |
|---|---|---|
| Default direction | open the app, read the debug line | `lang=ar dir=rtl` before you touch anything |
| Toggle | press the language button | `dir` flips, layout mirrors, nothing else changes |
| Mirroring | compare `ar` and `en` side by side | icons, chevrons, progress and totals all move to the correct side |
| Physical properties | `grep -rnE '\b(p|m)[lr]-[0-9]|\b(left|right)-[0-9]|text-(left|right)' apps/*/src` | no hits — logical utilities only |
| Numerals | any screen in Arabic | Western Arabic digits, 0–9 |
| Truncation | a long Arabic product name | ellipsis on the correct side, no overflow |
| Mixed content | an Arabic name with a Latin SKU | no bidi scrambling of the code |
| Zoom / small screen | resize to 1024×640, the declared minimum | nothing overlaps; the min-size guard fires below it (E.60) |

### 5.4 The keyboard-only pass

A register is used by someone whose hands are on goods and a scanner, not a mouse.

```
F2 search · F4 pay · F6 park · F7 resume · F9 returns · Del void line · +/− qty · F12 lock
```

Unplug the mouse — literally — and complete a sale. Every action must be reachable, focus must
always be visible, and tab order must follow the reading direction. Scans must land with no focus
at all, and must still route correctly **while the search box has focus**: that is where most
implementations break.

### 5.5 Hardware, without hardware

`pos-hardware` ships a simulator behind the real traits, so printing and drawer kicks are testable
with nothing plugged in.

```bash
cargo nextest run -p pos-hardware                 # simulator captures prints and drawer kicks
cargo nextest run -p pos-hardware -- --nocapture  # see the captured byte stream
```

When receipts land (group 1.7), inspect the bytes rather than trusting the eye:

```bash
xxd crates/pos-hardware/tests/golden/receipt_ar_80mm.bin | head -40
```

A golden-file diff is regenerated **deliberately**, reviewed as a diff, and committed. A golden
file updated to make a test pass is a test deleted.

#### Reviewing a binary golden

"Look at the diff" is earned for the five fiscal goldens: they are XML, a diff is readable, and a
changed byte is visible. It is **not** earned for the receipt and label goldens, which are 1-bit
rasters — a hexdump cannot show that an Arabic letter lost its medial form, and a `cosmic-text`,
`rustybuzz`, `tiny-skia` or font bump produces a byte diff indistinguishable from a shaping
regression. Under that rule the only available response to a red golden test is `UPDATE_GOLDEN=1`,
which is a change-detector, not a correctness check.

So **every raster golden ships a committed `.png` beside its `.bin`**, generated by the same
rasteriser and diffed in the same pull request. GitHub renders the image diff; that is the review.

```bash
UPDATE_GOLDEN=1 cargo nextest run -p pos-hardware golden
git diff --stat crates/pos-hardware/tests/golden/     # the .bin AND the .png must both move
```

Three rules follow, and they are the ones that get skipped:

1. **A regenerated `receipt_ar_*` or `receipt_bilingual_*` golden carries the native-reader
   confirmation in that same pull request**, recorded in the drill log (§5.10) — not deferred to the
   next release, which is where it used to live.
2. A change under the golden directory earns a `risk: arabic-rendering` label, so the PR is
   reviewed as a rendering change rather than as whatever else it was about.
3. **A shaping-stack or font bump is its own pull request**, never inside a grouped dependency
   update, and it carries the same confirmation.

Real hardware has its own checklist — [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md)
§2.1 for Arabic rasterisation, and the lab checklist for paper, cutters, and drawer wiring. Arabic on
a thermal printer is never proven by a simulator; print it on paper before believing it.

### 5.6 Drive the server by hand

```bash
just db-up && just migrate
just dev-server                                    # 127.0.0.1:8080
RUST_LOG=pos_server=debug,sqlx=debug just dev-server

curl -s localhost:8080/health   | python3 -m json.tool
curl -s localhost:8080/health/db | python3 -m json.tool
curl -si localhost:8080/nope | head -1              # expect 404, not a panic
```

For anything with a body, assert the **status and shape**, not just that it returned:

```bash
curl -si -X POST localhost:8080/sync/push \
  -H 'content-type: application/json' \
  -d '{"device_id":"…","batch":[]}' | head -20
```

Postgres directly:

```bash
docker exec -it pos-postgres psql -U postgres -d pos
# \dt              list tables
# \d+ sale         one table's shape
# select * from _sqlx_migrations order by version;    what has actually been applied
```

### 5.7 Inspecting the register's database

The register database is **SQLCipher-encrypted**, so the system `sqlite3` cannot open it — it
reports `file is not a database`, which looks like corruption and is not. Two honest options:

```bash
brew install sqlcipher
sqlcipher "$HOME/Library/Application Support/com.perfectcoders.pos/pos.db"
# then, as the FIRST statement on the connection:
#   PRAGMA key = 'dev-only-not-a-secret';
#   .tables
#   PRAGMA user_version;      -- how many migrations have been applied
```

Or read it through the code that owns the key — a `#[test]` or a small dev binary using
`pos_db::open`, which is also the only way that works on a machine using the OS credential store.

Never copy a register database anywhere for inspection once it contains real data: it holds customer
records covered by the PDPL.

### 5.8 The backoffice

```bash
just dev-backoffice        # Vite on its own port, plain browser
pnpm --filter backoffice build && pnpm --filter backoffice preview   # the production bundle
```

Check it against the same RTL and keyboard rules. It is a browser app, so also check it at a
laptop width and with the browser zoomed to 150%.

### 5.9 The ten-minute smoke

Run this before any push that touches the app, and after any dependency or toolchain change. It is
deliberately short enough that you will actually do it.

```
 1. just db-local-reset && just dev-terminal   → window opens on a fresh DB, no error toast
 2. Sale screen renders in Arabic, RTL, on the seed fixture
 3. Add a line by scan (or scanner echo), one by search, one by tile
 4. Change a qty; void a line; park the sale; resume it
 5. Pay cash with overtender → change is correct to the fil
 6. Receipt prints to the simulator; Arabic shapes correctly; tax summary rows are separated
 7. Lock (F12), unlock with a PIN
 8. Diagnostics screen: printer status, DB health, backup status all report
 9. just dev-server + curl /health/db → 200
10. Devtools console clean — no React warning, no uncaught rejection
```

Anything red stops the push. Anything odd-but-not-red gets written down (§5.11).

### 5.10 The drills

These are the tests that only exist if you do them on purpose. They are in
[`ref/test-catalog.md`](ref/test-catalog.md) as `drill` and they are the ones a merchant experiences
on their worst day.

| Drill | How to cause it | Must happen |
|---|---|---|
| **Power cut mid-finalize** (E.1) | kill the register during finalize: `pkill -9 -f target/debug/terminal` — match the build path, not the word "terminal", or you kill your shell | on restart: exactly one sale, one stock event, one outbox row |
| **Real power loss after the receipt printed** (E.1b) | **cut the actual power** — pull the plug, or hard-reset the machine; on real register hardware, not your laptop. Do it once the receipt is in your hand, so the customer has paid and gone | the sale is **there** on restart. This is the *only* drill that tests the storage durability setting at all: `pkill -9` above ends the process but leaves the OS and its page cache alive, so the writes reach disk whether `synchronous` is NORMAL or FULL, and E.1 therefore passes under both. Losing power is what forces the question. `synchronous = FULL` in WAL is what makes this pass; `pos_db::open` refuses to run without both, and asserts each |
| **App killed with parked carts** (E.3) | `pkill -9` with two carts parked | both carts resume intact |
| **Keychain wiped** (E.4) | macOS Keychain Access → delete `pos-terminal`, then launch | a named recovery screen, not a panic and not a silent new database |
| **Clock moved backwards** (E.6) | `sudo date -v-2H` (macOS), then sell | an audit anomaly is recorded; timestamps never decrease; sequences unaffected |
| **Day boundary** (E.7) | open a shift at 00:30 local, cutover 04:00 | the shift, its sales and its Z all carry **yesterday's** business date |
| **Disk full** (E.5) | fill a small disk image and point the DB at it | new sales are blocked with an alarm, and nothing is half-written |
| **Half-migrated database** (E.58) | set `PRAGMA user_version` above `MIGRATIONS.len()` | the app refuses to start and says why — it does not guess |
| **Backup and restore** (G-1) | take a backup, `just db-local-reset`, restore | every sale is still there, byte-identical |
| **Two registers, last unit** (E.12) | two instances offline, both sell the last unit | both sales stand; stock goes negative and is flagged |

Restore your clock afterwards (`sudo sntp -sS time.apple.com`).

**A drill produces a record or it did not happen**, and "signed off and dated" needs somewhere to be
signed — a normative reference document is not a log. Every run is a dated file under `docs/drills/`,
which the first drill creates:

```
docs/drills/
├── README.md                      the index
└── YYYY-MM-DD-<drill>.md          one per run
```

Each carries: the drill, the commit SHA or tag it ran against, the hardware, **the operator's name**
(Phase 5 requires someone who did not write the code), start and end time, elapsed, the outcome, and
any surprise plus the case number it became. `.github/ISSUE_TEMPLATE/05-drill-result.yml` is the same
form, so it can be filed from a phone in the lab.

The drills that gate a phase — the hardware lab, the card reconciliation, the blind-Z, the three
restore drills, the key rotation, the breach tabletop, fiscal certification — all read their evidence
from here. So does the release checklist in §15: *"did the hardware lab run for `v0.4.0`?"* is a
question about a file, not about a memory.

### 5.11 When something is wrong, write this down

Three lines, in the PR or an issue, before you start fixing:

```
Repro:     fresh DB + seed → scan 6281234567890 → set qty 0.347 → F4 → cash 5.000
Expected:  change 1.234 JOD, one 16% tax row, one exempt row
Actual:    change 1.235 JOD; the exempt line is inside the 16% row
Invariant: I-1 (rounded twice — line then receipt)
Layer:     no property test asserted Σ line tax == receipt tax over weighed lines
```

The last line is the important one. **The missing test is the real bug**; the wrong number is a
symptom. Fix the layer, and the class of bug closes instead of the instance.

---

## 6 · Definition of done — the checklists

Copy these. "Mostly done" is a status that hides work; each list is all-or-nothing.

**A microstep** (conventions §6, plus the two stations people skip):

```
[ ] the named files exist, with the named items
[ ] the named tests exist, named exactly as specified, and pass
[ ] `just lint` clean
[ ] `just test` clean
[ ] the `Done when:` line is objectively true — checked by running its command
[ ] nothing outside the `Files:` list changed (imports and mod declarations excepted)
[ ] the manual check from §5 for this kind of change was actually done
[ ] the docs the change contradicted were fixed in the same commit
[ ] committed with the step number in the message
```

**A group / a PR:**

```
[ ] every microstep in the group is individually done
[ ] the group's edge cases in ref/test-catalog.md have a test, a written accepted risk,
    or an explicit out-of-scope — no blank rows
[ ] `just pre-push` clean
[ ] the §5.9 smoke passed on a fresh database
[ ] the PR description says what, why now, invariants touched, verification, catalog rows
[ ] every applicable CI, policy, supply-chain, and security check is green
[ ] squash-merged; branch deleted
```

**A phase:** its own exit gate, at the bottom of the phase file — runnable commands **and** the
numbered demonstrations. Phase 1's demonstrations 7 (power cut mid-finalize) and 10 (backup,
keychain wipe, restore) are the two most likely to be skipped and the two most likely to matter.

---

## 7 · Review, when you are the only reviewer

Peer review is the industry standard because a second pair of eyes catches what the author cannot
see. With one developer, the substitutes are not optional — they are the whole control:

1. **Time.** Read your own diff after a break, or the next morning. Same eyes, different brain.
2. **A checklist,** not intuition — §4.8. Intuition is what wrote the bug.
3. **The tools.** `/review` for correctness and simplification, `/security-review` for the
   never-list. They do not get bored on the four hundredth diff.
4. **CI.** The reviewer that never has a bad day. If a rule matters, make CI enforce it — a rule
   that lives only in this file is a rule that will be forgotten at 23:00.
5. **The gates in `.claude/`.** `.claude/rules/` loads standards for the paths you are editing;
   `.claude/hooks/` refuses what must not happen. After touching either, run `just guards` — a
   guard nobody has seen fail is a guard nobody should trust.

The checked-in Claude configuration intentionally disables its OS sandbox so permitted
package-manager, Git/SSH, GitHub, and other networked shell commands can use the host normally. It
keeps the default permission mode manual, disables bypass-permissions mode, explicitly keeps hooks
enabled, and retains the exact project Read/Edit denies. Disabling the sandbox does not approve
every command: the normal Claude permission flow still applies. It does mean a permitted shell
subprocess has ambient host filesystem, network, environment, and credential access; the Read
denies constrain Claude tools and are not OS containment. Pre-tool launcher and
settings-validation failures fail closed; post-tool documentation diagnostics remain visible but
cannot undo a completed write. The portable launcher and real `PowerShell`/`Monitor` routing are
contract-tested, but native Windows process dispatch was not exercised. Git hooks and CI provide
cross-platform backstops and signals; with `main` unprotected and zero rulesets configured, a red CI
result still cannot block the repository administrator from merging.

When a second developer arrives, the reviewer's job in this codebase, in priority order:

| Priority | The reviewer asks |
|---|---|
| 1 | Does any money value touch a float, or round more than once? |
| 2 | Can a completed sale be mutated by any path this diff adds? |
| 3 | Does every fact write share a transaction with its outbox row? |
| 4 | Is `pos-domain` still pure — no clock, no I/O, no randomness? |
| 5 | Is the permission checked in Rust, not merely reflected in the UI? |
| 6 | Can any log line, error detail, or fixture print PII or a card value? |
| 7 | Are the tests the ones the microstep named, and do they test the invariant rather than the implementation? |
| 8 | Does the diff match the microstep's scope, or has it grown a second feature? |

Review the tests first. If the tests are right, the implementation has somewhere to be wrong out
loud.

Two habits that make the GitHub surface do some of this work for you: open the PR and then **walk
away** — read the diff in the GitHub UI the next morning, where it reads differently than in your
editor — and **leave review comments on your own PR**, then resolve them. A comment you wrote and
answered is a decision with a record; a thought you had and forgot is a bug in three weeks.

---

## 8 · Git discipline

The flow, in one line: **`feature → development → staging → main`**. The full model is
[`03-github-workflow.md`](03-github-workflow.md) §1; this is the discipline it needs.

| Thing | Rule |
|---|---|
| Branch | `phase-<0-5>/group-<m>-<slug>`; `fix/`, `chore/`, `docs/`, `refactor/`, `perf/`, or `test/` otherwise. `hotfix/` is the only one that branches from `main` |
| Base | **`development`**, always — `just branch <name>` gets it right for you |
| Lifetime | days, not weeks — a group is a branch, and a long branch is a merge conflict accruing interest |
| Commit | one microstep, conventional prefix, `[<step>]` tag, imperative summary. `.githooks/commit-msg` refuses anything else |
| History on a branch | rebase freely — `git rebase -i development`, fixup, reword — it is yours until it is pushed |
| History after pushing | rebase only if nobody else has it; otherwise a new commit |
| Merge to `development` | **squash**. One commit per group, microsteps in the body. The PR *title* is the commit |
| Merge to `staging` or `main` | **merge commit**. Squashing a promotion forks the branches permanently |
| Rebase-merge | disabled at the repository level. It is the one button that silently produces a history nobody chose |
| `development` | always green. Never merge red — a red `development` means the next person cannot tell whether they broke it |
| `staging` | a candidate that will actually be installed. Tagged `v<x>.<y>.<z>-rc.<n>` |
| `main` | what a merchant is running. Tagged `v<x>.<y>.<z>`. Nothing lands here except a promotion or a hotfix |
| Tags | only a tag triggers a release build. Signed tags are append-only: `.githooks/pre-push` refuses moving/deleting any existing tag, and a bad build gets a new patch tag. GitHub locks the associated tag and assets only after the draft is published with immutable releases enabled |
| Direct pushes | refused by `.githooks/pre-push` on all three long-lived branches. Run `just setup` on every machine or you have no protection at all |

Never committed: `.env` files · database files and SQLite sidecars · private keys and credential
stores such as nested `id_rsa`, `.netrc`, registry credentials, or Docker config · `target/`,
`dist/`, `node_modules/` · `apps/terminal/src-tauri/gen/schemas` ·
`.claude/settings.local.json` · anything that contains a credential. The staged-path policy is
NUL-safe, measures the staged blob rather than the working copy, and fails closed if Git cannot
answer. Gitleaks separately inspects the content, so an ordinary filename is not a bypass.

```bash
git status --short              # before every commit; look for what you did not expect
git diff --cached --stat        # size sanity: a 40-file diff for one microstep is a story
git log --oneline -10           # is the history still readable as a plan?
```

## 9 · CI, and reproducing it locally

CI runs on every push to `development`, `staging` and `main`, and on every PR into them:

| Workflow | Job | Steps | Local equivalent |
|---|---|---|---|
| [`ci.yml`](../../.github/workflows/ci.yml) | `rust` | locked fmt/Clippy/tests · domain structure/purity · property names · exact runtime SQLite · real scratch PostgreSQL · the test-catalog reconciler, which lives on this job because it reads nextest listings | `just lint && just test && just verify-pg` |
| [`ci.yml`](../../.github/workflows/ci.yml) | `guards` | Claude/Codex/Git/protected-path/schema/title/attribution/secret/workflow policy negative suites | `just guards` |
| [`ci.yml`](../../.github/workflows/ci.yml) | `web` | exact Node contract · Biome · logical CSS · tests · fail-closed build/types coverage · test coverage notice · docs links | `just lint && just test && just build-web` |
| [`ci.yml`](../../.github/workflows/ci.yml) | `supply-chain` | trusted-range Gitleaks · Rust advisories/licences/bans/sources · reviewed JavaScript licences · npm advisories | `just secrets && just audit` |
| [`ci.yml`](../../.github/workflows/ci.yml) | `cross-platform` | core tests and a real Tauri package build on Linux/macOS/Windows; the packaged-app WebDriver smoke suite is future work owned by 2.9.5 | run the platform tests and Tauri build on each supported OS |
| [`branch-flow.yml`](../../.github/workflows/branch-flow.yml) | `protected-paths`, `topology` | exact-workflow-revision policy · verified data-only PR head · legal head/base/repository · title and attribution | relevant `just guards` self-tests |
| [`labeler.yml`](../../.github/workflows/labeler.yml) | `label` | path-derived area/risk plus title-derived type, executing only trusted base code | `bash scripts/validate-change-title.sh --self-test` for normalization and `bash scripts/pr-type-label.sh --self-test` for type selection; GitHub event, path labeling and mutations remain server-only |
| [`security.yml`](../../.github/workflows/security.yml) | workflow analysis, scheduled advisories | actionlint · zizmor · weekly full-history secret and dependency scan | policy self-tests plus `just secrets && just audit` |
| [`release.yml`](../../.github/workflows/release.yml) | guard, platform signing, publisher, metadata | verified signed exact-tip tag · exact-SHA CI · least-privilege publishing · SBOM/checksums | the release checklist in §15 |

`ci` cancels a superseded run on a work branch, but never on `staging` or `main`: a half-cancelled
promotion build tells you nothing about whether the candidate was green. Standard hosted-runner
minutes are not metered for this public repository; cancellation still retires stale evidence and
returns runner capacity to the latest head — [`03-github-workflow.md`](03-github-workflow.md) §8.

`just pre-push` predicts the deterministic code and policy jobs. It deliberately cannot reproduce
GitHub event topology, workflow static analysis, the time-varying advisory databases, or a build on
an operating system other than the one you are using. When CI fails and your machine did not:

| Symptom | Almost always |
|---|---|
| clippy fails only in CI | a warning gated behind a feature or `--all-targets`; run `cargo clippy --locked --workspace --all-targets -- -D warnings` |
| a test fails only in CI | order or time dependence — CI runs tests in parallel in a different order. Conventions §5: no wall clock, no ambient randomness, no filesystem ordering |
| `pnpm install` fails only in CI | the lockfile was not committed, or a package needs a build script allowed in `pnpm-workspace.yaml` (`allowBuilds`) |
| biome passes locally, fails in CI | `biome ci --error-on-warnings` is stricter than `biome check`; `just lint` uses the CI form |
| doc-links fails | a renamed/deleted local target, undefined reference label, or case mismatch; `just docs-links` names the document and target |
| a build fails on Linux only | a Tauri system dependency; the workflow's `apt-get` list is the reference |
| `supply-chain` fails and nothing local did | expected: `just pre-push` deliberately omits `just audit`, because both halves reach the network and read advisory databases that change hourly. Run `just audit` to see it |
| secret range scanning fails | the proposed commit range contains a detector match even if the final worktree is clean; do not print it, rotate a real credential first, then handle history separately |
| the Postgres mirror fails only in CI | `just verify-pg` skips the engine pass with no `$DATABASE_URL` and no Docker. `just db-up` first, or read the skip line — it is not a pass |

**Flake policy: there isn't one.** A flaky test in a money system is worse than no test, because it
teaches you to ignore red. Quarantine it in the same hour — either make it deterministic or delete
it and open the microstep that replaces it. Never re-run CI to get green.

```bash
gh run list --limit 5
gh run watch <run-id> --exit-status          # choose the run whose head SHA/ref you inspected
gh run view --log-failed
```

---

## 10 · Debugging and observability

**Rust:**

```bash
RUST_BACKTRACE=1 cargo nextest run -p pos-db -E 'test(schema_version)'
cargo nextest run -p pos-db --no-capture          # see println!/dbg! output (dbg! is denied in commits)
cargo nextest run --workspace --status-level all  # every test, including skipped, with timing
cargo nextest run --workspace --retries 0         # never mask a flake
RUST_LOG=pos_server=trace,sqlx=debug just dev-server
```

`tracing`, never `println!`, in anything that ships. Spans over messages: a span carries the sale id
through every log line under it. And nothing on the never-list ever goes into a field —
`.claude/rules/security.md`, enforced by a test that feeds known PII through the logger and asserts
absence.

**Property-test failures** are the best bug reports you will get. `proptest` writes the failing seed
to `crates/<crate>/proptest-regressions/<module>.txt` — **commit that file**. It is a permanent
regression test for a case you did not imagine.

```bash
PROPTEST_CASES=100000 cargo nextest run -p pos-domain -E 'test(prop_)'    # hunt harder
```

Shrink first, then debug: proptest hands you the minimal failing input, and the minimal input
usually names the cause.

**How a property is configured is engineering law, not a per-test choice** — conventions §5.1. The
part that matters at the keyboard:

- The case count comes from a shared `ProptestConfig` (4096 for `pos-domain`, 256 for the I/O-bound
  crates), set once in a test helper rather than sprinkled per test. Proptest's default is 256
  *randomly generated* cases, and 256 random sequences from an unstated generator is a green tick
  rather than evidence — especially for a property over input sequences to a state machine, which is
  what `prop_no_input_sequence_yields_two_tenders_for_one_auth` is.
- **Every property names its `Strategy`, beside the property, with a comment saying what input space
  it covers and what it deliberately excludes.** An unstated generator is the difference between a
  property that bites and one that has never looked at the case that matters.
- `failure_persistence` is on, `proptest-regressions/` is committed, and the seed is printed on
  failure. A green `cargo nextest run -E 'test(prop_)'` is reproducible evidence only if the seed
  that failed can be replayed — otherwise a genuinely failing property presents as a flake, under a
  flake policy that says there isn't one.
- A **wall-clock assertion never lives inside `proptest!`** and never carries the `prop_` prefix. A
  performance budget is a criterion bench under §11, not a property, and `prop_` is a filter the
  phase gates depend on.
- The stateful multi-process harness — the sync chaos generator — is the exception: a seeded RNG
  with committed seeds and **no shrinking**, because shrinking a fault sequence across processes
  produces a different execution. Replaying a recorded seed does not.

**Frontend:** devtools in the dev build; React Query's cache is inspectable from the console; Zustand
stores are plain objects. When a value is wrong on screen, find out whether it is wrong in the store,
wrong at the IPC boundary, or wrong in Rust — `await ipc(...)` from the console (§5.2) settles it in
one line.

**The database:** `PRAGMA user_version` first, always. Most "impossible" register bugs are a schema
version you did not expect.

---

## 11 · Performance — measured, not asserted

Four budgets, from conventions §7. Microstep 1.2.0 created the recipe that **fails the build on
regression**, and 1.12.3 adds the live reference-register CI job, which does not exist yet. The
recipe refuses every run today: no register has been bought, so §6a.1's matrix and
`benchmarks/reference-register.toml` are both blank and §7.1 accepts no baseline against a blank
record. A budget without that failing command is a wish, because `cargo bench` exits 0 whatever it
measures — and a budget whose machine does not exist is not measured at all.

| Budget | Limit | Tool | Measured on | Lands at |
|---|---|---|---|---|
| Scan → line on screen | < 100 ms | packaged-app WebDriver trace + hardware simulator | the reference register | 1.11.13 |
| Cart total recompute, 200 lines | < 16 ms | `criterion` | the reference register | 1.4.9 |
| Search-as-you-type, 50k SKUs | < 50 ms | `criterion` over the seed fixture | the reference register | 1.2.7 |
| Cold start → sellable | < 3 s | packaged-app smoke timer (2.9.5) | the reference register | 2.9.3 |

**"The slowest supported hardware" is not a machine.** Conventions §7 names one reference register —
the lowest-spec device on the merchant's supported hardware list (section G of the questionnaire) —
and every number above is measured there. A hosted CI runner varies well beyond 20% on a 16 ms
workload, so a budget gated on a runner fails randomly, and §17's own prediction comes true: an
unstable benchmark gets disabled within a month.

**The baseline is a file, and moving it is a commit.** `benchmarks/baselines/<budget>.json` is
committed; it changes only in a `perf(...)` commit whose body pastes the measurement. `just
bench-gate` compares medians over at least 50 samples and fails on a regression beyond 20% **and**
more than three baseline median absolute deviations outside the baseline's own noise — both
conditions, because either alone is a flake generator.

```bash
just bench-gate                               # exits non-zero on regression, and on a blank profile
cargo bench -p pos-domain -- price_cart       # one benchmark, while iterating
```

Rules that keep this from becoming folklore: measure on a **release** build over the **seed
fixture**; change one thing per measurement; and never optimise before a budget is red. A profile
beats an opinion — and an integer money path is usually already fast enough that the slow thing is a
query plan, not arithmetic.

---

## 12 · Security in the daily loop

The full treatment is [`ref/security-compliance.md`](ref/security-compliance.md); the one-page
version is conventions §12; the never-list is `.claude/rules/security.md`. What that means at the
keyboard, every day:

```bash
./scripts/scan-secrets.sh --staged  # the exact content gate used by pre-commit
just secrets                        # all reachable history; also part of just pre-push
/security-review                    # before any PR that touches money, auth, logging, or IPC
```

- **Never log** a value matched by `SENSITIVE_FIELD_RULES` — exact names, suffixes and contains rules
  are defined once in [`ref/security-compliance.md`](ref/security-compliance.md) §6 and mirrored in
  the reviewed `.claude/rules/security.md`. Do not maintain another field list here. The registry
  covers `tracing`, `IpcError.detail`, crash reporting, diagnostic bundles and fixtures that print.
- **Never store** anything from a card beyond the PSP reference, the masked PAN the terminal returns
  for the receipt, and the scheme.
- **The DB key lives in the OS credential store.** `POS_DB_KEY` is dev and CI only; the release build
  ignores it and continues to credential-store lookup, so a stray environment value cannot supply
  the key or stop the till opening.
- **Permissions are checked in Rust, in the handler.** Every time. Hiding a button is UX.
- **Escalation is recorded distinctly from operation** — the approving manager's id is a different
  column from the operating cashier's (E.52).
- **No compliance claim without the completed validation.** Read §3 of the security reference before
  writing the word "compliant" anywhere, including a commit message.

If a secret is already committed: say so, stop, and do not rewrite history unasked. Rotating the
secret comes first; the history is a second, separate decision.

**Not security, but enforced in the same policy layer.** Coding assistants are tools, not
co-authors: `commit-msg`, `pre-push`, and CI reject their `Co-Authored-By` and generated-by lines.
A genuine human co-author passes. An exact Dependabot author name/email may retain its exact
GitHub-generated trailer as a narrow compatibility exception. Because Git author metadata is
locally configurable, the policy does not describe that match as authenticated provenance.

---

## 13 · Dependencies and toolchain

**Adding a Rust dependency:**

```bash
# 1. declare the version ONCE, in the root Cargo.toml [workspace.dependencies]
# 2. in the crate:  <name>.workspace = true
cargo tree -i <crate>          # who else pulls it in, and at which version
cargo tree -d                  # duplicate versions of the same crate — a link-error waiting
just check && just test
```

Three questions before any new dependency, and the third is the one that matters here:

1. Does the standard library or an existing dependency already do this?
2. Is it maintained, and what does it pull in transitively?
3. **Which crate am I adding it to?** A dependency in `pos-domain` that can perform I/O breaks I-8
   and is a design review, not an edit. Say so out loud before adding it.

**Adding a JS dependency:** `pnpm --filter terminal add <pkg>` — always workspace-filtered, never a
bare `npm install`. Commit `pnpm-lock.yaml` in the same commit. If the package needs a post-install
script, pnpm 11 skips it silently and a fresh `--frozen-lockfile` install then fails with
`ERR_PNPM_IGNORED_BUILDS`; allow it explicitly in `pnpm-workspace.yaml` under `allowBuilds`.

**Toolchain bumps** are their own commit, never bundled with a feature:

```bash
# edit rust-toolchain.toml and Cargo.toml workspace rust-version together, then prove it
just lint && just test
git commit -m "chore(repo): rust 1.98.0                                     [—]"
```

This unpublished application promises the compiler it tests; it does not advertise a separate,
untested MSRV. The structural policy requires `rust-version` to equal the pinned toolchain. Apply
the same discipline to `packageManager` and to the exact Node/Node-types contract anchored in
`.nvmrc`.

**Dependency review** is wired locally and in CI. It is deliberately not in `just pre-push`
because advisory databases change independently of the proposed code:

```bash
just audit   # cargo-deny + reviewed JS licence metadata + pnpm audit
```

The JavaScript gate consumes `pnpm licenses list --json` and refuses any expression absent from
`js-license-policy.json`. Metadata acceptance is not a notice bundle: before external distribution,
inventory each platform artefact and ship every required third-party licence/source notice.

---

## 14 · Documentation upkeep

This documentation set is a working instrument, not a deliverable that was signed off. It has three
layers and they have different rules:

| Layer | Rule |
|---|---|
| [`../plan/`](../plan/) | **Read-only.** Source documents. A hook refuses writes. Corrections go in `docs/implementation/` |
| `docs/implementation/*.md` | The plan of record. When a microstep is wrong at the keyboard, **fix the microstep** |
| `docs/implementation/ref/*.md` | **Normative.** If the code must deviate, fix the reference in the same commit |

```bash
just docs-links       # every local inline/reference target in tracked Markdown resolves
just verify-schema    # ref/schema.md is executable SQLite and obeys conventions §2
```

The recurring jobs, from [`README.md`](README.md):

- **Quarterly:** re-run the validation audit in [`ref/plan-validation.md`](ref/plan-validation.md).
  Jordanian rates move by Cabinet decree, JoFotara keeps adding waves and changing validation, and
  the PDPL authority is still standing up. A compliance claim has a shelf life. *(Last verified
  20 August 2026 — next due November 2026.)*
- **Per surprise:** it becomes `E.93` in [`ref/test-catalog.md`](ref/test-catalog.md), with a test, a
  written accepted risk, an open question carrying a stated default and an owning microstep, or an
  explicit out-of-scope. `just test-catalog` is what keeps that promise mechanical.
- **Per merchant question:** into [`ref/merchant-decisions.md`](ref/merchant-decisions.md) rather
  than guessed at in code.
- **Per correction to a source plan:** into [`00-master-plan.md`](00-master-plan.md) §4a. `docs/plan/`
  cannot be edited and a reader is routed there first, so a superseded name that is not in the
  concordance is a name someone will build.
- **Per external unknown:** a `⚠️ OPEN` block in the reference document that owns it, in one shape —
  the question, the default until it is answered, the owning microstep, and the source that settles
  it. Never a guessed legal, tax or regulatory fact. An invented compliance fact is worse than a
  visible gap, and this repository has a standing rule against claiming an unearned validation.

A decision that shaped the architecture and is not written down will be re-litigated. If it does not
fit an existing reference file, it belongs in the phase file at the microstep it changed, in the
words you would use to explain it to yourself in a year.

---

## 15 · Release

Releases are tag-driven ([`.github/workflows/release.yml`](../../.github/workflows/release.yml)):
a `v*` tag builds macOS (universal), Linux (on the oldest supported glibc), and Windows, and opens a
**draft** GitHub release. Two channels, decided by the tag:

| Tag | Tagged on | Result |
|---|---|---|
| `v0.2.0-rc.1` | `staging` | a **pre-release** draft — the pilot channel |
| `v0.2.0` | `main` | a **production** draft release |

The `guard` job refuses before the three-platform matrix unless all of these are true: the tag has
the exact `vX.Y.Z`, `vX.Y.Z-rc.N`, or `vX.Y.Z-beta.N` grammar; it is a signed annotated tag whose
signature GitHub reports as verified; it resolves to the current `main`/`staging` tip for its
channel; all maintained version files agree; and `ci.yml` completed successfully for that exact
SHA on that branch. Platform build/sign jobs have a read-only repository token and the signing
material they need; a separate minimal publisher has the write token and no signing secrets.
After every platform succeeds, it attaches an SPDX JSON SBOM and SHA-256 manifest covering the
application assets and SBOM to the still-draft release.

```bash
# Define `exact_push_run` from 03-github-workflow.md §2 in this Bash session first.
set -euo pipefail
release_repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)

verify_release_tag() { # verify_release_tag <tag> <expected-tag-object> <expected-commit>
  local tag=$1 expected_object=$2 expected_commit=$3 ref_json tag_json
  ref_json=$(gh api "repos/$release_repo/git/ref/tags/$tag")
  [ "$(printf '%s' "$ref_json" | jq -r '.object.type')" = "tag" ]
  [ "$(printf '%s' "$ref_json" | jq -r '.object.sha')" = "$expected_object" ]
  tag_json=$(gh api "repos/$release_repo/git/tags/$expected_object")
  [ "$(printf '%s' "$tag_json" | jq -r '.object.type')" = "commit" ]
  [ "$(printf '%s' "$tag_json" | jq -r '.object.sha')" = "$expected_commit" ]
  [ "$(gh release view "$tag" --json isDraft --jq .isDraft)" = "true" ]
}

# ── version, through the normal development PR path ──
just branch chore/release-v0.2.0
# update Cargo.toml, apps/terminal/src-tauri/tauri.conf.json, apps/terminal/package.json
git commit -m "chore(repo): set version 0.2.0                              [—]"
just pre-push
git push -u origin HEAD
version_pr=$(gh pr create --base development \
  --title "chore(repo): set version 0.2.0   [—]" \
  --body "Synchronize every release-version source before promotion.")
IFS=$'\t' read -r version_base version_head < <(
  gh pr view "$version_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
bash ./scripts/watch-pr-checks.sh "$version_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$version_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$version_base" ] || [ "$current_head" != "$version_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$version_pr" --match-head-commit "$version_head" --squash --delete-branch

git switch development && git pull --ff-only
development_sha=$(git rev-parse HEAD)
development_ci=$(exact_push_run ci.yml development "$development_sha")
gh run watch "$development_ci" --exit-status

# ── candidate, from staging ──
staging_pr=$(gh pr create --base staging --head development \
  --title "promote development to staging" \
  --body-file .github/PULL_REQUEST_TEMPLATE/promotion.md)
IFS=$'\t' read -r staging_base staging_head < <(
  gh pr view "$staging_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
# Fill notes/promotion-staging.md from the template with both SHAs and exact evidence.
gh pr edit "$staging_pr" --body-file notes/promotion-staging.md
bash ./scripts/watch-pr-checks.sh "$staging_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$staging_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$staging_base" ] || [ "$current_head" != "$staging_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$staging_pr" --match-head-commit "$staging_head" --merge  # MERGE COMMIT, never squash
git switch staging && git pull --ff-only
staging_sha=$(git rev-parse HEAD)
staging_ci=$(exact_push_run ci.yml staging "$staging_sha")
gh run watch "$staging_ci" --exit-status
rc_tag=v0.2.0-rc.1
git tag -s "$rc_tag" -m "Phase 1 groups 1-4"    # signed annotated tag is required
rc_tag_object=$(git rev-parse "${rc_tag}^{tag}") # retain the exact object the build must use
git push origin "refs/tags/$rc_tag"
rc_release=$(exact_push_run release.yml "$rc_tag" "$staging_sha")
gh run watch "$rc_release" --exit-status
gh release view "$rc_tag"                       # inspect every asset and the checksum manifest
# Only after all external-release prerequisites below are satisfied:
verify_release_tag "$rc_tag" "$rc_tag_object" "$staging_sha"
gh release edit "$rc_tag" --draft=false --prerelease

# ── production, from main, after the candidate has actually been used ──
main_pr=$(gh pr create --base main --head staging \
  --title "promote staging to main" \
  --body-file .github/PULL_REQUEST_TEMPLATE/promotion.md)
IFS=$'\t' read -r main_base main_head < <(
  gh pr view "$main_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
# Fill notes/promotion-main.md from the template with both SHAs and exact evidence.
gh pr edit "$main_pr" --body-file notes/promotion-main.md
bash ./scripts/watch-pr-checks.sh "$main_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$main_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$main_base" ] || [ "$current_head" != "$main_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$main_pr" --match-head-commit "$main_head" --merge
git switch main && git pull --ff-only
main_sha=$(git rev-parse HEAD)
main_ci=$(exact_push_run ci.yml main "$main_sha")
gh run watch "$main_ci" --exit-status
final_tag=v0.2.0
git tag -s "$final_tag" -m "Phase 1"           # lightweight tags are refused
final_tag_object=$(git rev-parse "${final_tag}^{tag}")
git push origin "refs/tags/$final_tag"
final_release=$(exact_push_run release.yml "$final_tag" "$main_sha")
gh run watch "$final_release" --exit-status
gh release view "$final_tag"                    # draft: inspect every artifact before publishing
# This exact-object/commit check is the final command before publication.
verify_release_tag "$final_tag" "$final_tag_object" "$main_sha"
gh release edit "$final_tag" --draft=false --prerelease=false
```

The explicit PR and run identifiers compensate for the absence of configured merge enforcement.
Immediately before each merge, confirm the checked head SHA is unchanged and pass it to
`--match-head-commit` so the merge is atomic with that expectation. After each merge, watch the
`ci.yml` push run selected by both branch and exact SHA. Never use a bare `gh run watch` in a
promotion or release sequence.

A draft release and its tag are not atomically bound by GitHub. The workflow checks the exact
annotated tag object and target commit before and after mutating draft assets, but neither check
locks the ref. Retain the locally created tag-object SHA and run `verify_release_tag` immediately
before the human publication command. A final instruction-sized race is unavoidable; publishing
the verified draft closes that window because the repository's immutable-release setting then
locks the tag and assets. A failed release workflow or failed publication recheck means **do not
publish that draft**.

If any release job fails, choose GitHub's **Re-run all jobs**, or run
`gh run rerun "$release_run_id"` without `--failed` or `--job`. Every workflow artifact name
includes `github.run_attempt`, and the publisher deliberately accepts only the three platform
sets and SBOM from one attempt. A partial rerun therefore fails closed instead of mixing new
publisher output with older build artifacts.

Promotion CI builds the real Tauri application on Linux, macOS, and Windows before tag time, so the
release workflow is not the first platform-specific packaging run.

What is **not** ready, and must be before anything reaches a machine you do not own:

- **Verified tag signing and updater keys** — configure a signing identity, generate the updater
  keypair **on the offline signing host**, and commit the updater public configuration. This was
  Phase-0 item 0.3.2, left open when Phase 0 closed and then owned by no phase and no gate, while
  `release.yml` hard-fails without the public key — so no release could be built at all. It is
  re-homed as **microstep 5.5.0**, first in that milestone. The private key does not become a
  repository Actions secret on a step that compiles third-party code (5.5.1).
- **OS code signing** — Windows Authenticode certificate, Apple Developer ID plus notarisation
  (milestone 5.5.1). Until then an installer will warn, loudly, on every machine.
- **A tested restore path.** Do not ship a version whose backup you have not restored from (§5.10).
- **An update service.** There is no manifest endpoint, no cohort assignment and no `plugins.updater`
  block, so "staged rollout with rollback" describes nothing (5.5.0, 5.5.2).

Before publishing a draft, three things that are files rather than memories:

- **The hardware-lab record for this tag exists in `docs/drills/`** — a golden file proves bytes;
  only paper proves a receipt, and only a dated record proves the paper was looked at.
- **The migration duration against the soak dataset is recorded** for this release, so the trend is
  visible before it crosses the 60-second budget (5.5.3).
- **`just bench-gate` passed on the reference register**, not on a hosted runner.

Version discipline: `0.x` while pre-pilot; the minor number moves with a phase gate, the patch with
a fix. A bad build is a new patch, never a moved tag. The workflow refuses unsigned annotated and
lightweight tags. GitHub release immutability is enabled for published releases.

---

## 16 · Cadence — the rituals that keep this repeatable

Working alone removes the meetings and keeps every one of the failure modes they existed to prevent.
These are the smallest set that actually works.

| When | Ritual |
|---|---|
| **Start of a session** | `git switch development && git pull --ff-only`, `just setup` if the lockfiles moved, then read the microstep you are on out loud. Two minutes; it prevents an hour of building the wrong thing |
| **Before each microstep** | its `ref/` sections and its `E.n` rows — §4.1 |
| **After each microstep** | gates, manual check, commit. Never leave a microstep half-done overnight; finish it or revert it |
| **End of a session** | `git status` clean or `git stash` with a message. Write the next action as a single sentence in the branch's PR description — future-you starts from a sentence, not from a diff |
| **Per group** | PR into `development`, CI, squash-merge, delete the branch, close the doc loop, run the §5.9 smoke |
| **Per candidate** | `just flow`, then promote `development → staging` with the promotion template's evidence filled in honestly, tag `-rc.<n>`, and install it somewhere real |
| **Weekly** | re-read the phase's group graph. Is the order still right? Then the board's **Blocked** view — `gh issue list --label "needs: merchant answer"`. Anything blocked for a week is a risk, not a task |
| **Per phase** | the exit gate, in full, including the demonstrations. Then re-read the risk register in [`00-master-plan.md`](00-master-plan.md) §6 — every row has a **review date** for exactly this moment — the long-lead register in §6a, the open items in §4a.3, and the accepted risks in [`ref/sync-protocol.md`](ref/sync-protocol.md). Run `just bench-gate` after 1.2.0; from Phase 2 onward, run `just test-soak` only after 2.9.6 creates it |
| **Quarterly** | the validation re-audit (§14), and re-diff the pinned ISTD manifest against the current official package in the same pass |

Two habits that pay for themselves:

- **Time-box the unknown.** A microstep that has taken triple its estimate is a microstep that is
  wrong, not a day that is going badly. Stop, write down what you learned, fix the microstep, and
  either do the corrected version or move to an independent group.
- **Keep one page of "what I do not yet know" — and it is already written.** The `⚠️ OPEN` blocks in
  the reference documents *are* that page, and [`00-master-plan.md`](00-master-plan.md) §4a.3 is its
  index. Each carries the question, the default the code runs on meanwhile, the owning microstep, and
  the source that settles it. Grep for `⚠️ OPEN` at every phase gate. Unknowns that stay written down
  get scheduled; unknowns in your head become the reason a phase runs long — and an unknown that
  quietly becomes an assumption becomes a compliance claim nobody earned.

---

## 17 · Known gaps in the toolchain

Honest list, so nobody wastes an afternoon on a command that cannot work yet. Each names what closes
it.

| Gap | Closed by |
|---|---|
| `just seed` does not exist; there is no fixture, so every manual test is ad-hoc | 1.12.1 |
| `just bench-gate` exists and refuses, but there is still no benchmark and no committed baseline, so **no budget is enforced anywhere** and `cargo bench` alone cannot enforce one | budgets 1.2.7, 1.4.9, 1.6.2, 1.11.13; measurement job 1.12.3 |
| **No reference register has been bought**, so "the slowest supported hardware" has no machine behind it, `ref/hardware-and-receipts.md` §6a.1's table has no row, `benchmarks/reference-register.toml` is blank, and the gate refuses every baseline | the deferred half of 1.2.0, on hardware ordered before group 1.7; the remaining device classes at 2.9.4 |
| `pos-db` is not wired into the terminal yet, so `just db-local-reset` currently has nothing to delete | 1.8.x persistence |
| TS types are hand-written in `packages/api-types`; no `ts-rs` generation into `src/ipc/`, no CI drift gate | conventions §13, with the first real IPC surface |
| No i18n catalog lockstep test, and no message catalog to test | 1.11.1 |
| No PII-scrubber test on the logger (G-8) | 1.6.x |
| **No DOM component harness in `apps/terminal`** — it has `vitest` and no `jsdom` environment, while `apps/backoffice` already has the pattern. Three named 1.11.x tests, including `scan_routes_while_search_focused`, cannot be written until it exists | 1.11.0 |
| **Nothing automated launches the packaged application, on any OS.** CI builds the Tauri bundle and never starts it; `.spec.ts` naming implies Playwright, which drives browser engines and cannot attach to a Tauri webview | 2.9.5 (WebdriverIO + `tauri-driver`). Any platform `tauri-driver` does not support is named there, not left implicit |
| No fuzzing, for four parsers that consume input this product does not control — the scan parser, the receipt raster path, the UBL builder, and the sync decoder — under a `unwrap`/`expect` ban that makes panic-freedom on hostile input an invariant | 1.2.8, then per parser |
| The soak and long-chaos suites live in the default `cargo nextest --workspace` run, with no selection policy and no runtime budget | 2.9.6, and the `soak` nextest profile |
| Test coverage is not measured. Deliberate — property tests over invariants are the coverage story here — but `cargo llvm-cov` is worth running once per phase to find modules with **no** test at all. **Mutation testing on `pos-domain` is the better instrument** for the same reason: line coverage says a line ran, and `cargo-mutants` says whether the properties bite when `>=` becomes `>` or `HalfAwayFromZero` becomes `HalfEven` | per-phase, by hand |
| No installer signing of any kind | 0.3.2 (updater), 5.5.1 (OS) |
| **No branch protection or ruleset is configured.** The repository is public: `main` reports `404 Branch not protected`, and the rulesets API reports zero. `.githooks/pre-push` and server-side checks provide safety and evidence, but `--no-verify` and the administrator merge button remain possible | a reviewed server-side policy change outside this setup; no merge-blocking control is claimed until it is configured |
| A clone that has not run `just setup` has **no** protection, because the hooks live in `core.hooksPath` | nothing — it is inherent to hook-based enforcement. It is why §12 of `03-github-workflow.md` leads with `just setup` |
| Claude's OS sandbox is intentionally disabled on every host, so permitted shell subprocesses have ambient filesystem, network, environment, and credential access; native PowerShell process dispatch was not exercised here | accepted development-policy tradeoff; manual permissions, exact tool-level denies, hooks, Git hooks, and CI remain controls rather than OS containment. Re-enable an audited sandbox policy if host isolation becomes required |
| `staging` means "a tagged candidate", not "a running system" — there is no hosted environment for `apps/server`, no server backup, no tested restore, no monitoring and no on-call | group 3.10. Running one small instance from Phase 3 is also the cheapest way to buy operational experience before a merchant supplies it |
| There is no update service — no manifest endpoint, no cohort assignment, no `plugins.updater` block — behind a gate that requires a staged rollout proven end to end | 5.5.0, 5.5.2 |
| Ordinary commits are not signed | release tags already require verified signing; decide whether to require ordinary commit signing before external contributors arrive |

---

## 18 · The forget-list

One screen. These are the things that get forgotten, in the order they get forgotten.

```
[ ] the property test — not just the example test
[ ] the Strategy beside it, saying what it covers and what it excludes
[ ] the E.n rows in ref/test-catalog.md that this change was supposed to close
[ ] the PLANNED allowlist entry you were supposed to REMOVE, not add
[ ] the Postgres mirror of the SQLite migration
[ ] the data migration, and the test that seeds the old shape and asserts the new one
[ ] the fact table's no-UPDATE / no-DELETE triggers, and its row in FACT_TABLES
[ ] the outbox row, in the same transaction as the fact — and the whole commit group
[ ] the permission check in Rust — not only the hidden button
[ ] the ApprovalHandle on the privileged command, consumed in the same transaction
[ ] the Arabic pass, and the RTL mirror, on this exact screen
[ ] the .png beside the regenerated .bin golden, and the native reader in THIS pull request
[ ] both language catalogs, in lockstep
[ ] the keyboard path for the thing you just built with a mouse
[ ] the reference doc you contradicted
[ ] the erratum in 00-master-plan.md §4a, if you superseded something a source plan says
[ ] the ⚠️ OPEN block, if the honest answer is "we do not know yet"
[ ] the microstep you proved wrong
[ ] the drill record in docs/drills/ — a drill nobody wrote down did not happen
[ ] `just docs-links` after renaming any document or linked local target
[ ] `just guards` after touching .claude/hooks or .claude/rules
[ ] the manual smoke on a FRESH database, not the one you have been poking at
[ ] the step number in the commit message
[ ] the PR *title* — it is what a squash-merge actually commits
[ ] the branch, deleted after the squash-merge
[ ] the back-merge to development after a hotfix — skip it and the next promotion reverts the fix
[ ] a promotion PR merged with a MERGE COMMIT, never squashed
```

---

*Companion to [`01-conventions.md`](01-conventions.md). Maintained with the repository: when a
command, gate, platform limit, or gap in §17 changes, this file changes with it.*
