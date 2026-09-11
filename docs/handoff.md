# Handoff — the single current one

**Reflects `development` @ `9acc1f3`, 11 September 2026.**

This file **replaces** the 9 September handoff. Everything from it that is still true has been
carried forward; the rest was superseded by the twenty-six pull requests below and is deliberately
gone. There is one handoff — keep updating this file rather than adding a dated one.

**9–11 September changed the governance layer again, and the product not at all.** Twenty-six pull
requests merged into `development` (#131–#159, less the two closed unmerged), two promotions reached
`staging` (#130, #148), and **no microstep advanced**. The only three commits that touch `crates/`
source touch nothing outside `#[cfg(test)] mod tests`. Phase 1 is exactly where it was: **20 of 112**.

**The tree is green.** `just pre-push` exits 0 in 1:29.90 on a warm cache, every one of the 37
`just guards` steps passes, and CI run `34613826217` is green on `9acc1f3`. There is no fire to put
out. **The WIP=1 slot is empty**: 0 open pull requests, and all ten board items read `Todo`.

**Read `CLAUDE.md` first.** This document assumes it. Where this file and the repository disagree,
**the repository is right** — every number here was read from `git`, `gh` or a command, and where
something is unverified it says so.

---

## 0 · Do this first

```bash
cd ~/My_Projects/pos
git checkout development && git pull --ff-only
git fetch --all --prune            # your local staging is 51 commits stale — see §14
mise exec -- just setup
mise exec -- just pre-push          # passes at 9acc1f3, 1:29.90 warm
```

**`just setup` without `mise exec --` fails.** The shell's Node is `v26.4.0`; `.nvmrc` pins
`24.19.0` exactly and the check is fail-closed. This is the first thing that goes wrong every time.

**One operational gotcha, measured:** `mise exec -- <cmd>` fails with `couldn't exec process: No
such file or directory` if the same Bash invocation does `cd` first. Run it from the working
directory with an absolute script path, and never build the command in a shell variable — the whole
string is passed as `argv[0]`.

### Verified gate baselines at `9acc1f3`

Every row re-measured on 11 September. Use these as the "nothing is broken" reference.

| Command | Reads |
|---|---|
| `just pre-push` | **exit 0** — `lint test build-web guards secrets`, `justfile:368`, 1:29.90 warm |
| `just check` | exit 0 — all **seven** workspace members |
| `just lint` | exit 0 — 2 prerequisites + 14 body steps = **16 checkers** |
| `just test` | exit 0 — **241 tests run: 241 passed, 2 skipped**; JS **5 files / 30 tests** |
| `just build-web` | exit 0 — `tsc -b` + vite 8.2.2 across 5 packages |
| `just guards` | exit 0 — **37 steps** |
| `just verify-schema` | exit 0 — **4 migrations, 32 tables, 300 columns** |
| `just verify-pg` | exit 0 — **the engine pass RAN** via the Docker fallback |
| `just secrets` | exit 0 — gitleaks 8.30.1, `--history` |
| `just audit` | exit 0 — cargo-deny clean; **135 package releases, 11 reviewed expressions** |
| `just bench-gate` | **REFUSED, exit 3** — no reference register. Correct, not broken |
| `pnpm --filter terminal exec vitest run` | **3 files, 18 tests**, vitest **5.0.0** |
| `pnpm --filter backoffice exec vitest run` | 1 file, 2 tests |
| `check-implementation-frontier.py` | phase 1: **112**, 2: 61, 3: 45, 4: 42, 5: 36 |
| `check-test-catalog.py` | reconciles, **92 cases** |
| `check-js-licenses.py` | **135** package releases, 11 reviewed expressions |
| `test-gh-setup.sh` | **69 passed, 0 failed** |
| `check-branch-workflow-policy.rb --self-test` | **219 passed, 0 failed** |
| `.githooks/test-hooks.sh` | **136 passed** |
| `check-protected-paths.sh --self-test` | 18 passed · `watch-pr-checks.sh --self-test` **49** |
| `test-settings.py` | **30 passed**; `.claude/settings.json` is **4,426 bytes** |

Three counts moved since 9 September and are the ones a stale document will get wrong:
`check-branch-workflow-policy.rb` 216 → **219**, `.githooks/test-hooks.sh` 108 → **136** (#134,
#137, #142), and the JavaScript licence count 142 → **135** (the Dependabot bumps and vitest 5).

**Toolchain actually in use:** rustc 1.97.1, cargo-nextest 0.9.143, node v24.19.0, pnpm 11.22.0,
vitest 5.0.0, gitleaks 8.30.1, Docker Engine 29.5.2.

### Two things the gate table does not say

* **`just verify-pg` ran the real engine pass here**, because `$DATABASE_URL` is unset *and* Docker
  is running, so the Docker fallback fired (`postgres:18-alpine`, pinned by digest at
  `scripts/verify-pg-migrations.py:66-70`). With Docker stopped it prints that it skipped the engine
  pass and still exits 0. **Read the output line, not the exit code.**
* **Doctests run nowhere.** `just test` is `cargo nextest`, which does not execute them, and no CI
  job does either. There are 10 doc-example fences across `pos-db/src/repo/outbox.rs` and
  `pos-domain/src/{catalog,ids,audit}.rs`. They can rot silently.

---

## 1 · Where the project stands

| | |
|---|---|
| `development` | **`9acc1f3`** — green; CI run `34613826217` all-success |
| `staging` | **`531ea04`** — **10 behind** `development`, 5 ahead (its own five promotion merges) |
| `main` | `24a0283` — **138 behind** `development`, **133 behind** `staging`, untouched since 20 August |
| Phase 1 | **20 of 112** executable microsteps (~18%) — unchanged since 8 September |
| Open PRs | **0** |
| Open issues | **10** — see §3. **None is `In Progress`: the WIP=1 slot is empty** |
| Board #4 | the API's default listing returns **10 items, every one `Todo`**. Archived items are excluded from that listing and their count is not readable through it |
| Rulesets | **four, all active**, and all four now checked in under `.github/rulesets/`, byte-matching live |
| Tags / releases | **zero of each.** The append-only tag ruleset has never been exercised |
| Repository | **PUBLIC**, GitHub Free, `OmarSweiti` the sole collaborator (admin) |

### Complete: 20 microsteps

Read live from the frontier region, `docs/implementation/README.md:22-39`, in its own order:

`1.1.0` `1.1.1` `1.1.2a` `1.1.6` `1.1.3` `1.1.4` `1.1.2b` `1.1.7` `1.1.5` `1.1.8` `1.2.1` `1.2.2`
`1.3.1` `1.8.9` `1.8.5` `1.11.2` `1.6.5` `1.6.1` `1.6.3` `1.11.0`

**When the 21st lands, the region must read `21 of 112 executable microsteps fully complete
(~19%)`.** The checker computes `round(100 * done / total)` and `round(100*21/112) == 19`; leaving
`~18%` is a hard `just lint` failure, not a rounding quibble. The same denominators appear on three
surfaces the checker reconciles — the region, `00-master-plan.md:96`, and
`status-page.html:464-504` — so a denominator change lands in all three or the gate reds.

### Partially delivered — still three

A `**Full-step status:**` marker mechanically means "partial" whatever its prose, and
`scripts/check-implementation-frontier.py` refuses to let such a step be declared complete.

| Step | Marker | What is missing |
|---|---|---|
| `1.1.9` | `phase-1:190` | the database half, gated on `1.9.1` creating `trusted_time_state` |
| `1.2.0` | `phase-1:222` | a reference register exists nowhere — **#68**. `bench-gate --check-profile` exits 3 |
| `1.6.4` | `phase-1:691` | only the 1.8.x terminal handler — gated on **#111** and on 1.8.x wiring `pos-db` into the terminal |

Rule 8 of the checker is the non-obvious half: every `**Full-step status:**` id must also still
appear as a backticked id in `README.md`. Completing one means deleting **both** the marker and the
README prose in the same change; delete one and the checker errors from the other direction.

---

## 2 · What landed 9–11 September — twenty-six pull requests, zero microsteps

Thirty commits, 47 files, +3195/−630. Nine files added, none deleted. Classified: **19**
governance/docs/CI, **5** Dependabot, **2** domain PRs that touch only `#[cfg(test)] mod tests`.
**No shipping product code changed anywhere.**

### The five rulings that matter

**1 · `main` has a ruleset (#136).** `main-append-only`, id `22744344`, active since 10 September,
`deletion` + `non_fast_forward` only. Eleven places said it had none. The old reasoning survives but
applies **only to `required_status_checks`**: main's `ci.yml` predates four of the six contexts, so
a `hotfix/*` cut from main would wait forever on checks that never report — while `deletion` and
`non_fast_forward` need no checks at all, which is why they could be applied first. The correct
sentence is *"force-push and deletion are refused server-side on `main` today, while a pull request
is still not required and no check is a merge wall"* — **not** "main is protected". And
`branches/main/protection` answering `404` is the **legacy** API, which cannot see a ruleset;
`repos/.../branches` reports `main protected=true`.

**2 · `strict_required_status_checks_policy` is `true` on `development` and must stay `false` on
`staging` (#146 set both, #153 reverted staging).** This is structural, not bad luck: a promotion's
merge commit lives only on `staging` and is never merged back, so `staging` runs permanently ahead
by one commit per prior promotion. Strict compares branch **tips**, so on `staging` it can never be
satisfied — PR #148 opened as `BEHIND` and could not be merged. Strict also buys nothing there: it
guards concurrent PRs racing a moving base, which is a `development` phenomenon.

**3 · A CodeQL dismissal binds to an alert number, not to code (#157, #159).** Alerts #4/#5/#6 were
`rust/hard-coded-cryptographic-value`, critical, on three nonce literals in
`crates/pos-domain/src/permissions.rs`. #157 consolidated them into one constant
`TEST_NONCE: [u8; 16] = [0xA5; 16]` at `:1043`, inside `#[cfg(test)] mod tests`. **That produced a
brand-new alert #7** at the new location — created 15:12Z on 10 September, hand-dismissed 14:26Z on
11 September — while the comment #157 added in the same commit already asserted it was dismissed.
So **any future move or rename of `TEST_NONCE` costs one more manual dismissal**, and leaving it
undone leaves an open critical alert on the default branch of the money crate. The check is
`gh api 'repos/:owner/:repo/code-scanning/alerts?state=open'`.

Why the fixed literal is mandatory, so nobody "fixes" it again: `ApprovalHandle::issue`
(`permissions.rs:747`) takes `nonce` as a **parameter** because invariant I-8 forbids `pos-domain`
from generating randomness. It is enforced twice — `check-domain-purity.py`'s self-test rejects
`Uuid::new_v4()` in this crate, and `Cargo.toml:67` pins `uuid` without the `v4` feature, so the
suggested fix **cannot compile here**. The value is `0xA5` and not `0` or `1` because
`approval_debug_output_redacts_content_hash_and_nonce` asserts the raw bytes are *absent* from
`Debug` output, and a run of zeroes could appear there for unrelated reasons — passing the assertion
without proving redaction.

**Copilot Autofix PRs #154 and #156 were closed unmerged**, for six independently disqualifying
reasons: they do not compile (`E0599`, no `new_v4`); they violate I-8; they carry
`Co-authored-by: Copilot Autofix powered by AI`, which reds `supply-chain`; branches
`alert-autofix-5/6` are illegal routes, which reds `topology`; the titles are outside the §8
grammar; and they randomise a value the tests never read. **Do not reopen them.**

**4 · `.githooks/pre-push` now scans only what a push publishes (#140, #143, #144).** It no longer
runs `scan-secrets.sh --history` — gitleaks with no `--log-opts` walks `--all`, so a token on an
unrelated local branch or in a stash refused a clean push. It now runs
`scan-secrets.sh --pushed LOCAL_SHA REMOTE`, deliberately the same revision expression the
attribution and sensitive-path checks already use, so all three content gates cover one commit set
by construction. Measured: median 0.516s → 0.044s; attribution on a 132-commit push 11.26s → 1.55s;
CI's `supply-chain` scan 7.72s → 0.07s over 142 commits.

`--pushed` is a **named mode, not a `--log-opts` passthrough** — gitleaks word-splits that string
and an arbitrary caller value would be git-log argument injection. A push straight to a URL falls
back to `--history`, widening the scan, never narrowing it. **The one accepted loss:** a secret that
reached the remote by another route no longer alarms on your next unrelated push. Four gates keep
that alarm — `just secrets` (all-ref, inside `just pre-push`), CI's `supply-chain`, `security.yml`'s
weekly full-history run, and GitHub push protection. **Remove any of them and this trade needs
revisiting.**

**5 · Both agents now refuse `sqlx migrate revert` (#138).** `.claude/hooks/protect-immutable.py`
contained zero occurrences of `sqlx` or `revert` — the same command was blocked for Codex and waved
through for Claude, measured at `exit=0` for the bare form, `bash -c '…'` and `sudo …`. It is now
refused across eleven spellings, asserted behaviourally by driving **both** hooks and requiring the
same verdict. `RELEVANT` at `:112` had to gain `"sqlx"`, without which the matcher was unreachable.
**No Git hook and no CI job can backstop this**: it mutates a database and produces no commit and no
diff, so nothing downstream can observe it.

### The rest, in one table

| PR | What |
|---|---|
| **#131, #132** | the board describes itself; the record counts right |
| **#133** | the release page gets categories |
| **#134** | `scripts/check-hooks-installed.py` — `core.hooksPath` proven installed, wired into `just lint`, `just test` and `just setup`. Also: `git commit -v` used to refuse clean messages (the hook saw message **plus diff** because Git strips the scissors section *after* the hook runs); fixed by cutting on Git's own `cut_line` at `.githooks/commit-msg:32`/`:39` |
| **#135** | the release-tag chain is exercised — it was **entirely untested** before; rulesets become checked-in data |
| **#136** | `main-append-only`, and eleven documents corrected |
| **#137, #142** | the suite proves the wiring, not only the scripts; every push assertion owns its fixture |
| **#138** | both agents refuse the revert that leaves no diff |
| **#139, #141** | one paragraph per claim; the two gates that share the name `pre-push` are disambiguated |
| **#140, #143, #144** | the push and CI scans narrowed to the published range |
| **#145** | `.githooks/post-merge` — an advisory line after a pull that moved your pins |
| **#146** | Actions hardening: `cargo-deny` via `taiki-e/install-action` pinned to a full SHA instead of a Dockerfile action that did `curl \| tar` with no integrity check; `rust-cache` `save-if` only from a restorable branch |
| **#147, #149, #150, #151, #152** | Dependabot. **vitest 4.1.11 → 5.0.0 landed cleanly and broke nothing** |
| **#153** | strict status checks belong on `development` alone |
| **#155** | the two secret-scanning settings are **not offered on this account** — see §7 |
| **#157, #159** | `TEST_NONCE`, and the dismissal-per-move finding |
| **#158** | sixteen live-state corrections across four documents |

### Two new traps this window created

**A · Every Dependabot Action-SHA bump reds `protected-paths` by construction.**
`check-branch-workflow-policy.rb` byte-freezes every workflow definition, and a `uses:` bump edits
`.github/workflows/*.yml`. Confirmed on **#149** and **#151**: the failing step is *"The next
workflow retains this trusted-workflow boundary"*, every other step green. Both merged through the
admin bypass. **This recurs monthly** — `.github/dependabot.yml` runs a `github-actions` ecosystem
at `interval: monthly`, `open-pull-requests-limit: 5`.

**B · Two commits on `development` carry titles the repo's own `commit-msg` hook refuses.**
`5c53f60 chore(repo): bump the js-patch group with 5 updates` and
`941d820 chore(repo): bump the actions-patch group with 2 updates` — both missing the mandatory
`[<step>]` token. `./scripts/validate-change-title.sh --validate` refuses each. They landed because
**#149 and #150 were merged with merge commits, not squashes**, and a merge-commit merge drags the
raw bot commits across. Nothing catches it: `development-flow` permits
`allowed_merge_methods: ["squash","merge"]`, `.githooks/commit-msg` never sees a server-side merge,
and CI's grammar gate (`branch-flow.yml:157`) validates only the **PR title**, which becomes the
*squash* commit. **#131 was also merged with a merge commit** — 3 of 26. Squash a work PR.

---

## 3 · The ten open issues

All ten are on board #4, all `Todo`, all assigned. **Eight of the ten are blocked on a human**;
only **#119** and **#120** are `not blocked`.

| # | Title | Prio | Risk | Blocked | Blocks |
|---|---|---|---|---|---|
| **113** | `decision: ICV scope, before migration 0005 freezes it (merchant decision 6.9)` | P1 | migration · compliance | decision | **the `doc_sequence` half of `1.9.1` — read before any SQL** |
| 68 | `hardware: buy the reference register, scanner and both printers` | — | — | hardware | `1.2.0`'s deferred half, group 1.7, and four budgets |
| 69 | `decision: the legal entity, its TIN, and ISTD registration for JoFotara` | — | — | decision | **group 2.7, not a Phase-1 microstep** — see below |
| 70 | `decision: a tax adviser's written opinion on the four group-1.3 questions` | — | — | merchant answer | `1.3.4`, `1.3.7`, and the Phase-1 exit gate |
| 71 | `decision: JSMO on trade-scale verification evidence and reverification cadence` | — | — | decision | `1.2.4`'s DB half **and Phase-1 exit demonstration 2** |
| 111 | `decision: does deactivating an approver revoke an already-issued handle?` | P1 | security | decision | the 1.8.x approval handler, so `1.6.4`'s last file |
| 112 | `decision: the three manual discount caps (merchant decisions 3.1–3.3)` | P1 | money path | merchant answer | `1.4.5` |
| 114 | `gap: the agent read-deny blocks the memory directory and workflow resume` | P2 | — | decision | agent memory, workflow resume |
| **119** | `gap: the back office's Testing Library cleanup never registers` | P2 | — | **not blocked** | nothing today; the next back-office screen test |
| **120** | `gap: conventions §5 has no DOM-component layer` | P2 | — | **not blocked** | nothing; a two-document inconsistency |

**#69 does not gate a Phase-1 microstep.** Its own body says it blocks *"all of group 2.7 and the
22 ⚠️ OPEN items microstep 2.7.0 owns"*, and `phase-1:1055` says the opposite of gating: *"Owner:
2.7.0 ratifies 6.9 via #69, on a timeline outside this project's control, **so this microstep cannot
wait for it**."* It is a long lead ordered in Phase 1, not a Phase-1 blocker.

**Two divergences nothing reconciles.** Issues **#68, #69, #70 and #71 carry no `priority:` and no
`risk:` label at all**, while the board shows all four as P1 with a Risk value. And **#113 carries
two risk labels** (`migration` *and* `compliance`) while the board's single-select Risk field holds
only `migration` — the field is structurally incapable of holding both, and the loss is silent.
Decide which surface is authoritative and make them agree, or stop reading one of them.

### Two ⚠️ OPEN items gate Phase-1 microsteps and **no issue tracks them**

* `ref/plan-validation.md:324` — **blocks `1.8.1`**: the SQLCipher/SQLite WAL-reset corruption
  question and the two minimum version constants `1.8.0` must pin and hash.
* `ref/security-compliance.md:413` — **blocks `1.6.2`**: what second factor exists on a Jordanian
  minimarket counter. **Buying #68's hardware does not unblock `1.6.2`** — it is blocked twice.

A search of every issue, open and closed, for WAL / SQLCipher / storage / second factor returns
nothing. File them, or record deliberately that they live only in the reference documents.

### #115's remainder is still partly open

The four ruleset payloads exist under `.github/rulesets/` and byte-match live as of 11 September.
What is still open is **enforcement**: no gate runs the diff, the payloads have never been
round-tripped (no restore has been executed), and `scripts/gh-protect.sh` still refuses at **exit
3**. It refuses correctly — its legacy-API `PUT` would now apply three verified defects (a required
list omitting `guards`, `supply-chain` and `protected-paths`; `require_code_owner_reviews: true`
against a sole-developer CODEOWNERS with 0 required approvals plus `enforce_admins` on main; and the
legacy API cannot express `allowed_merge_methods` at all). **No issue tracks this remainder.**

---

## 4 · What is next — the WIP=1 slot is EMPTY

Nothing is `In Progress`. Per `03-github-workflow.md` §4: pick **one** microstep, open **one**
`Microstep` issue, add it to board #4 by hand, set it `In Progress`, then build it. Note that
`.github/ISSUE_TEMPLATE/01-microstep.yml` has **eight required fields**, including a proving command
(*"The command that proves it. Not a description of the command."*) and a *Test-catalog rows closed*
field — so a microstep with no `Done when` line cannot even be filed without authoring one first.

### Recommended first: `1.11.3` — formatting helpers

It is the **only** genuinely unblocked UI candidate, and it is small: two new TypeScript files
(`apps/terminal/src/lib/format.ts` and `format.test.ts`, neither exists), four named tests, a real
`Done when` at `phase-1:1208`, no Rust, no migration, no IPC, no hardware. Every dependency is on
disk — `1.11.0`'s harness is green, `Locale` is at `lib/direction.ts:13` and `DEFAULT_LOCALE` at
`:17`, and `packages/money` exports `Currency` (`:23`), `JOD` with exponent 3 (`:31`), `toMinor`
(`:51`) and `formatMinor` (`:75`). **No `⚠️ OPEN` item anywhere names `1.11.3`**, and group 1.11
carries no build-order note.

Three things its PR owes:

1. **The `1.11.12` ownership edit** — see §5. Strike
   `latin_runs_inside_arabic_text_are_bidi_isolated` from `1.11.3`'s Tests line
   (**`phase-1:1207`**) and trim the third clause of its `Done when` (**`:1208`**).
2. **Decide `formatMoney` vs `packages/money`'s `formatMinor`.** The phase file says only *"Never
   `toLocaleString` inline"*; `ui-spec` §9 puts `MoneyDisplay`/`ShelfPrice` in `packages/ui`, which
   `1.11.3`'s `Files:` line does not include. Wrap or re-implement — choose and write it down.
3. **Check `check-test-catalog.py` before pushing.** It enumerates **vitest** tests too
   (`:1280`, `:1309`, `:1315-1325`), not only nextest, so four new test names enter its input. Its
   assertions read catalog → runner, so an unnamed extra test is *probably* fine — this is the one
   gate that could red an otherwise-clean PR, and it was **not** executed against a hypothetical
   `1.11.3` diff.

### Recommended second, and higher-leverage: answer **#113**

One unmade decision gates the entire database frontier — `0005`, `0006`, `0007`, `1.2.3`,
`1.9.2`–`1.9.5`, `1.10.2`–`1.10.5`, the `1.1.9` DB half and the `1.2.4` DB half — because
`scripts/verify-schema.py:259` requires migration numbers contiguous from `0001`.

**But read it precisely: #113 blocks the `doc_sequence` half of `0005`, not all of `0005`.** The
⚠️ OPEN at `phase-1:1055` is headed *"blocks the `doc_sequence` half of this migration"*, and **two
of its four options need no external party at all**:

* **Option 2** — widen the `CHECK` to all five candidate scopes and constrain the choice in code.
  #113's own body: *"Costs nothing structurally and keeps `0005` honest."*
* **Option 3** — defer `doc_sequence` out of `0005` entirely.

Either is takeable today, by one person, in one sitting. That is the single highest-value thing in
this repository right now, and it is not code.

Two further traps for that work when it comes:

* `schema.md`'s `## 0005` heading must gain **`· SHIPPED`** in the same commit that lands the
  migration. `scripts/verify-schema.py:405` skips re-executing a section whose heading contains
  `SHIPPED`; without it the second pass re-runs the DDL against the schema it just built. `0002`,
  `0003` and `0004` all carry the marker; `0005` does not yet.
* The microstep also needs the Postgres mirror, the `lib.rs` `MIGRATIONS` registration, the
  `tests/common/mod.rs` `reference_blocks_at_or_after(5)` → `6` change, and the test file.

### Other candidates, with what is actually true of each

| Candidate | State |
|---|---|
| `1.2.4` **pure half** | **Unblocked and the largest unblocked domain step** — the gateway to group 1.4, since `CartLine` needs `PriceOrigin` and `DerivedWeight`. But it is big (14 tests, a new module, a trybuild pair) and its full `Done when` at `phase-1:302` chains commands unreachable before `0007`, so the issue's required proving command must be rewritten for the half |
| `1.6.6` — `AuditRepository` | **A dark horse nobody named.** Fully unblocked: `0004` shipped `audit_log`, and `1.6.5` and `1.8.9` landed. **No `Done when` line** — one must be authored first |
| `1.11.1` — i18n | **Blocked by `1.7.2`.** `assets/fonts/` does not exist, no font file is tracked anywhere, `pos-hardware/src` is only `lib.rs`. One of its two tests and half its `Done when` cannot pass |
| `1.11.4` — Lock / PIN | **Soft-blocked.** All four IPC commands it drives (`auth_login_pin`, `auth_switch_user`, `session_state`, `health_status`, `ui-spec.md:53`) are absent — `src-tauri/src` has no `commands/`, and `src/lib/ipc.ts` does not exist. It would be tested entirely against invented mocks |
| `1.11.5` — Sale screen | **Blocked.** `CartSnapshot` does not exist (`packages/api-types/src/index.ts` is `export {};`), and it would rewrite the green `1.11.0` canary |
| `1.9.1` — migration `0005` | **Blocked by #113** on its `doc_sequence` half. See above |
| `1.3.3` — `compute_line_tax` exclusive | Technically buildable, but **no `Done when` line**; document order puts the externally-blocked `1.3.2` first; both edit the same file; and it is one test of value alone |
| `1.6.2` — Argon2id PINs | **Blocked twice**, neither time by code: `just bench-gate pin-verify` refuses until #68, **and** `ref/security-compliance.md:413` |
| `1.2.3`, `1.2.6` | Blocked three migrations deep |

**Eighteen executable Phase-1 microsteps carry no `Done when` line at all** — `1.3.2` `1.3.3`
`1.4.1` `1.4.2` `1.4.3` `1.4.4` `1.4.5` `1.4.7` `1.4.8` `1.4.10` `1.5.1` `1.5.2` `1.5.4` `1.6.6`
`1.7.1` `1.7.4` `1.7.6` `1.7.8`. Checker rule 4 refuses to let any of them be declared complete
until one is written, and the issue form will not accept the microstep without a proving command.
Each is a documentation prerequisite to its own delivery.

### Or close a P2 — both verify exactly as written, and both are small

**#119** — `apps/backoffice/vite.config.ts` still sets **neither `globals` nor `setupFiles`**
(grep exits 1; line 13 is `test: { environment: "jsdom" },`). Re-verified at `9acc1f3`. Either fix
works and both are gate-clean: a mirrored `src/test/setup.ts` plus `setupFiles` (preferred,
symmetric with the register) or `globals: true` (smaller, diverges from the explicit-import
convention). **Add a second rendering test in the same change** — a one-render file cannot prove the
fix.

**#120** — a 2-file docs edit: add a DOM-component row to `01-conventions.md` §5 and change
**"Nine layers"** (`:114`) to **"Ten"** in the same edit. Three of the issue's own citations have
since drifted and must be corrected while closing it: *"Pick the layer from conventions §5"* is now
`02-development-workflow.md:396` (not `:387`), the Vitest row is `:405` (not `:396`), and the
ui-spec DOM-component rung is `ref/ui-spec.md:292` (not `:291` — `:291` is the Logical-CSS row).
**#120's row arithmetic is also wrong**: the §4.3 table has **six** data rows, not ten; the real gap
is a layer-count mismatch, not a row-count one. And its evidence line *"`git grep "Nine layers"`
returns exactly one occurrence"* is now false — it returns three, because this handoff copied the
numeral twice.

---

## 5 · Rulings carried forward — read before touching group 1.11

### Ruling: `1.11.12` owns `latin_runs_inside_arabic_text_are_bidi_isolated`

Strike it from `1.11.3`'s Tests line (**`phase-1:1207`**) and trim the third clause of its
`Done when` (**`:1208`**), leaving `1.11.3` with three tests. Evidence: `ref/test-catalog.md:314`
files the name under the DOM harness; `ref/ui-spec.md:292` places it on the DOM-component rung; and
the **arithmetic decides it** — `1.11.12`'s `Done when` says "all **five** named rendered-state
tests" and its Tests line (`:1307`) names exactly five, so striking it *there* breaks a counted
condition, whereas `1.11.3`'s carries it as prose that trims cleanly.

**Do it in `1.11.3`'s own PR**, not elsewhere — it touches another microstep's contract, which
conventions §6's amendment licence does not cover. **Do not read the gate's silence as permission**:
`check-test-catalog.py` exits 0 with the duplicate present because the harness table's header is
`| Harness | Unblocks |` while the collector requires `header[0] in {"Test","Property"}`.

### The safe-rewrite rule for `ref/test-catalog.md`'s harness row

`check-test-catalog.py` scans prose in **table cells** for two patterns, either of which makes the
names in the `Unblocks` cell compete with their owning `Tests:` lines and trips assertion 7:
`REFERENCE_TEST_MENTION` (`:1079-1080`) — a capital `Tests?` immediately before a backticked
identifier — and `REFERENCE_TEST_COVER` (`:1082-1084`). Both still compile at exactly those lines.
Measured safe: lowercase `` tests `id` ``, and a form where a word intervenes before the backtick.
**Neither `ui-spec.md` nor `test-catalog.md` may claim the tests now exist** — only the harness does.
`check_reference_contracts` never checks existence, so an over-claim would pass.

### Six things in the 1.11.x block a future PR must NOT touch

1. `1.11.5`'s `Files:` names `Sale.test.tsx` **without** `(new)`, while `1.11.0`'s names it **with**
   it. That asymmetry is the plan's only record that `1.11.0` creates the file and `1.11.5` extends
   it. Do not "fix" either.
2. `1.11.5`'s three tests and its `Done when` must not absorb the canary.
3. *"both use fake timers from 1.11.0"* is now true, and truer than before.
4. `1.11.1` owns `<html dir="rtl" lang="ar">` **by default** and *"Arabic as the rendered default"*.
   Supplying `dir="rtl"` to jsdom is a test-fixture fact, not that deliverable.
5. The three `Scheduled in:` lines are **not** identically worded. Two read *"run its screen
   assertions after 1.11.0"*; `1.9.5`'s reads *"run its provisioning-screen assertions after 1.11.0
   creates the DOM harness"*. There is a fourth record in the group header.
6. `.claude/rules/frontend.md` is byte-pinned and is **not** falsified by `1.11.0` — leave it.

### Traps in the terminal test area, measured and still unhit

* **`scripts/check-logical-css.sh` scans `.tsx` files** (`:82`) with no test-file exemption, and its
  bare-side regex (now **`:44`**) refuses a bare `right:` or `left:`. **The canonical DOM-test rect
  stub trips it** — `{ top: 0, left: 0, right: 320, bottom: 48 }`. `getBoundingClientRect` is still
  absent from `apps/` and `packages/`, so the collision arrives with `1.11.4` onward. The only
  escape is `physical-ok: <reason>` on the same line; a bare marker is refused.
* **Biome's a11y rules police a test fixture as production JSX.** A `<div onClick>` plus an untyped
  button trips `noStaticElementInteractions`, `useKeyWithClickEvents` and `useButtonType`, and
  `pnpm biome explain` reports **`No fix available.`** for all three.
* **`just fmt` does NOT fix `organizeImports`.** The recipe is now **`justfile:315-317`** — `cargo
  fmt --all` plus `pnpm biome format --write .` — and assists are applied only by
  `biome check --write`, which **no `just` recipe wraps**, while `biome ci --error-on-warnings`
  enforces them. Fix by hand: `pnpm biome check --write <files>`. Biome sorts on the specifier
  **name**, with `type` travelling with it.
* **`biome.json` cannot be narrowed to exclude tests.** `check-branch-workflow-policy.rb:823`
  refuses any `files.includes` negation outside four approved exclusions, and `:825-827` requires
  `apps/**` and `packages/**` to stay covered.
* **`environment` and the jsdom options are effectively untyped.** `environment: "jsdom-typo"`
  typechecks and dies at run time. `html?: string | ArrayBufferLike` **is** really enforced.
* **`src/**` in `apps/terminal` has no Node types under `tsc -b`** — read repository files in
  `vite.config.ts`, which `tsconfig.node.json` already types.
* **Vite rewrites `new URL("<literal>", import.meta.url)`** into a client asset URL under jsdom, so
  `node:fs` throws `TypeError: The URL must be of scheme file`. Bare `import.meta.url` is never
  rewritten.
* **Nothing enforces a `.tsx` test's name.** `check-test-catalog.py` reconciles catalogue → runner,
  never the reverse.

### Supply-chain facts worth not rediscovering

* **`allowBuilds` is pinned to exactly `esbuild` and `@tailwindcss/oxide`** by
  `check-branch-workflow-policy.rb:797-799`, asserted by exact equality. A dependency with an
  install lifecycle script forces an edit to `pnpm-workspace.yaml` **and** to that pinned literal —
  itself inside the frozen script surface.
* **pnpm's `minimumReleaseAge` is 1440** — its own 24-hour default; nothing in the repository sets
  it. A version published in the last day is refused.
* **There is no pnpm catalog here**, and a `catalog:` reference would break the frozen
  `@types/node` equality check.
* **`apps/terminal/package.json`, `biome.json`, `package.json` and `pnpm-workspace.yaml` are
  STRUCTURAL policy paths, not byte-pinned** — adding devDependencies is permitted.
* `pnpm-lock.yaml` and `apps/terminal/vite.config.ts` are in **no** policy set.

---

## 6 · GitHub — the verified inventory

Measured live 11 September. Keep this current; it saves an hour every session.

### Rulesets — four, all active

| Ruleset | id | Target | Rules |
|---|---|---|---|
| `development-flow` | 22650197 | `refs/heads/development` | `deletion`, `non_fast_forward`, `pull_request` (0 approvals, `allowed_merge_methods: ["squash","merge"]`), `required_status_checks` — **`strict: true`** |
| `staging-promotion` | 22650570 | `refs/heads/staging` | same four, but `allowed_merge_methods: ["merge"]` only and **`strict: false`** |
| `main-append-only` | 22744344 | `refs/heads/main` | **`deletion` and `non_fast_forward` only.** No PR rule, no checks |
| `tags-v-append-only` | 22650129 | `refs/tags/v*` | `deletion` and `update`. **`bypass_actors: []`** — `current_user_can_bypass: "never"` |

The six required contexts on both branch rulesets: **`rust · guards · web · supply-chain ·
protected-paths · topology`**. All three branch rulesets keep one bypass actor —
`{actor_id: 5, actor_type: "RepositoryRole", bypass_mode: "pull_request"}`.

Both PR rules also set **`require_extra_approval_for_unattributed_changes: true`**, which no
document mentioned. With 0 required approvals and a single admin collaborator, an unattributed
change would demand an approval no second account can supply.

### ⚠️ The bypass is the routine merge path, not an exception

```bash
gh api 'repos/:owner/:repo/rulesets/rule-suites?per_page=100&time_period=month'
```

**33 evaluations since the rulesets were created: 23 bypass, 9 pass, 1 fail.** 22 bypasses on
`development`, 1 on `staging`. The endpoint defaults to `time_period=day`, which is why an
unqualified call shows two — pass `time_period=month` or you will conclude the opposite.

* **The current `development` HEAD was a bypass.** Rule suite `4038065660`: PR #159,
  `result: bypass`, `required_status_checks` fail — *"5 of 6 required status checks are in
  progress."* It merged **35 seconds** after #158, and under `strict: true` a rebased branch
  restarts all six checks.
* **The last promotion was a bypass.** Rule suite `4021711258`: PR #148 into `staging`,
  *"2 of 6 required status checks are in progress."*
* **The ruleset has one recorded live refusal**, which is the guard-nobody-has-seen-fail evidence
  the repository's law asks for. Rule suite `4021702008`: a direct force-push to `development`
  refused server-side on 10 September — `non_fast_forward` *"Cannot force-push to this branch"*,
  `pull_request` *"Changes must be made through a pull request."*

**Most of those bypasses are the byte-frozen-surface review mechanism working as documented** —
#146's own body says *"`protected-paths` will be RED, by design … That red is the review."* But
merging while checks are still *in progress* is a different thing, and it is not that. `just merge`
waits; the fast back-to-back merges did not. Recording the ledger in §16's weekly ritual is still an
open change, and the merge habit it points at is not a documentation fix.

### Repository configuration

```
visibility              public      has_issues              true     allow_squash_merge   true
default_branch          development has_projects            true     allow_merge_commit   true
delete_branch_on_merge  true        has_wiki                false    allow_rebase_merge   FALSE
allow_auto_merge        true        has_discussions         false    allow_update_branch  true
squash/merge title      PR_TITLE    license  NOASSERTION (deliberate)
squash/merge message    PR_BODY     collaborators  OmarSweiti (admin), sole
```

One **environment**, `release`, with a single `tag: v*` deployment policy, **no reviewers and no
wait timer** — the tag-pattern scoping is its entire protection. **0 secrets in it.** `autolinks` =
`[]`. **40 labels** (one, `accessibility`, is declared by no checked-in file — #158's body calls it
stray). **Six milestones**; Phase 1 reads **open 9 / closed 27**, and `closed_issues` counts PRs.
All four issue forms carry `projects: ["OmarSweiti/4"]`, so a new issue reaches the board at
creation. Nine workflows, all active.

### Security posture

| Feature | State |
|---|---|
| Secret scanning · push protection | **enabled** |
| Dependabot security updates | **enabled** |
| Private vulnerability reporting | **enabled** — it is what makes `SECURITY.md`'s advisory link resolve |
| Actions: `sha_pinning_required` | **TRUE** — the repository-wide full-SHA policy is **applied**, not pending |
| Actions: `allowed_actions` | `"all"` — selected-Action allowlisting remains unconfigured |
| Default workflow permissions | `read`, `can_approve_pull_request_reviews: false` |
| CodeQL | default setup, `state: configured`, **`query_suite: extended`**, weekly, seven languages including **`rust`** |
| `secret_scanning_non_provider_patterns` | **disabled** — and cannot be enabled. §7 |
| `secret_scanning_validity_checks` | **disabled** — and cannot be enabled. §7 |

**0 open code-scanning alerts. 0 open Dependabot alerts. 0 secret-scanning alerts.** Four CodeQL
alerts exist (#4–#7), all dismissed as *used in tests*, all `rust/hard-coded-cryptographic-value` in
`pos-domain/src/permissions.rs`. **The single dismissed Dependabot alert is gone** — the alerts
endpoint now returns `[]` in every state, so the old `GHSA-wrw7-89jp-8q8g` glib record no longer
exists to cite.

A PR carries **five `Analyze` runs plus a `CodeQL` summary**. The summary reports `neutral`, not
`success`; it is not a required context and does not gate a merge.

**Trap:** the CodeQL default-setup `PATCH` is accepted **asynchronously**. `query_suite` keeps
reading `default` until the `CodeQL Setup` run it triggers finishes — which looks exactly like the
silent no-op the two secret-scanning fields genuinely are. Do not conclude a CodeQL setting was
ignored from an immediate re-read.

### Board #4 `POS delivery`

Project id `PVT_kwHOCn5KRs4BhoZ-`, user `OmarSweiti`. Private board, public repo. It is the **only**
project that exists, open or closed.

| Field | Field id | Options (id) |
|---|---|---|
| `Status` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjb_I` | Todo `f75ad846` · In Progress `47fc9ee4` · Done `98236657` |
| `Phase` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjqPw` | 0 `e28be675` · **1 `ecb2fe9c`** · 2 `a09a7d4b` · 3 `5d19eeed` · 4 `3f2f8542` · 5 `1d54d952` |
| `Group` | `PVTF_lAHOCn5KRs4BhoZ-zhgjqSA` | text |
| `Microstep` | `PVTF_lAHOCn5KRs4BhoZ-zhgjqWA` | text |
| `Priority` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjqX0` | P0 `952302e9` · P1 `00584313` · P2 `98f4223d` |
| `Risk` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjqX4` | money path `11636174` · migration `53da4eea` · security `3b34bad4` · compliance `df4f887e` · immutable `4800cc22` · none `ace6a93f` |
| `Blocked` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjqX8` | merchant answer `6420eac0` · decision `617fa9da` · hardware `c7becb76` · not blocked `1110b92f` |
| `Target` | `PVTF_lAHOCn5KRs4BhoZ-zhgjqZw` | date — **unset on all ten, deliberately** |

```bash
PROJ=PVT_kwHOCn5KRs4BhoZ-
gh project item-add 4 --owner OmarSweiti --url https://github.com/OmarSweiti/pos/issues/<N>
gh project item-edit --id <ITEM_ID> --project-id "$PROJ" \
  --field-id <FIELD_ID> --single-select-option-id <OPTION_ID>     # single-selects
gh project item-edit --id <ITEM_ID> --project-id "$PROJ" \
  --field-id <FIELD_ID> --text "1.11"                              # text fields
```

**Grouping and sorting are unreachable by any API** and never will be — there is no such input
anywhere in the schema. `visibleFieldIds` is the only `ProjectV2ViewConfigurationInput` field.
Established by introspection and corrected in #122; do not re-litigate.

---

## 7 · What needs a human

1. **Board view grouping and sorting.** Project #4 → each view: `Board — now` group by **Status**;
   `Phase plan` group by **Phase**, sort by **Microstep** ascending. Confirmed unreachable by API.
2. **Tag signing is DONE — this row is closed.** `tag.gpgsign=true`, `gpg.format=ssh`, and the key
   was registered on GitHub 9 September. **The release blocker is elsewhere** — see §9.
3. **`secret_scanning_non_provider_patterns` and `secret_scanning_validity_checks` cannot be enabled
   from this account** (#155). A `PATCH` returns 200 with the payload unchanged (tried twice, both
   forms), and the toggles are absent from the repository's Settings → Advanced Security **and** from
   `github.com/settings/security_analysis`. This is **not** a chore anyone can do. The earlier claim
   that they are "free on a public repository, which makes leaving them off a pure loss" was wrong.

   The residual exposure was also overstated. Provider patterns **are** enabled and **are**
   push-protected, and the generic class non-provider patterns would add is already scanned locally
   because `.gitleaks.toml` sets `[extend] useDefault = true` with **zero** custom rules — so the
   full upstream default rule set applies in pre-commit, in pre-push over the pushed commits, in
   CI's range scan and in the weekly full-history run. What is genuinely lost is a second,
   independent, server-side detector that a local `--no-verify` cannot skip. **Validity checks are
   the real loss** (no local equivalent); flagged for re-testing at `5.5.0`/`5.5.1`.
4. **#114**, because an agent editing its own permission grants would defeat the control, and the
   permission-mode classifier refuses it. A **mode** change does not lift a deny rule. Settings are
   snapshotted at session start, so a restart is required after any change.

---

## 8 · The repository queue

| Item | State |
|---|---|
| Board sync, four views, repo description and topics | **done** |
| The `0005` documentation preconditions | **done** — #116 |
| The twelve queued documentation corrections | **done** — #118, #121, #122, #123 |
| Ruleset control | **done** — four rulesets, all active, all checked in and byte-matching live |
| Promotion tooling | **done** — `just promote-merge <pr>` plus a push-only `promotion-shape` job |
| Hook installation proof | **done** — #134, `scripts/check-hooks-installed.py` |
| Actions SHA pinning | **done** — `sha_pinning_required: true`, repository-wide |
| Tag signing | **done** — key registered 9 September |
| **`#115`'s enforcement remainder** | **not done, and untracked.** No gate diffs `.github/rulesets/` against live; no restore has been round-tripped; `gh-protect.sh` still exits 3 |
| **The bypass ledger in §16's weekly ritual** | **not done.** 23 of 33 evaluations were bypasses |
| **Selected-Action allowlisting** | **not done** — `allowed_actions: "all"` |
| **Issues exception-only** | **not done.** `01-microstep.yml:2` still says "the normal way work enters this repo"; `ISSUE_TEMPLATE` is in no policy set, so it is a one-line green PR |
| **Claude read-only permissions** | **not done** — #114. `.claude/settings.local.json` is `{}` |
| **Matrix on every PR** | **not done.** Removing `cross-platform`'s promotion-only `if:` — do it **last** |
| **The `staging → main` promotion** | **not done, and it needs a person.** See §9 — it is now the single highest-value follow-up in the repository |

---

## 9 · Promotion and the release path

### `development → staging`

`staging` is **10 behind**. Open a promotion when you want the cross-platform matrix over the
current tip:

```bash
mise exec -- just promote-staging     # opens the PR; it does NOT merge
mise exec -- just promote-merge <pr>  # route, checks, re-snapshot, then --merge
```

**Merge with a MERGE COMMIT, never squash.** `just promote-merge` does, and the `staging` ruleset
now refuses anything else. Squashing a promotion forks the branches permanently; that happened to
**#74** on 29 August and cost a force-reset — its merge commit `4de6415` has **one** parent and is
unreachable from every ref today. All five real promotions (**#91 `f2edbb6`, #106 `8dc85cd`, #108
`6ac5d49`, #130 `ad59023`, #148 `531ea04`**) have two parents. Verify:

```bash
git rev-list --parents -n 1 <the promotion commit> | wc -w      # 3 = merge commit
```

**"Synchronised" means the upstream tip is an ancestor and the promoted trees agree — not that
divergence counts are zero.** Each promotion creates its own merge commit, so `staging` reads
"N ahead" where N is the number of promotions. Today N is 5.

**The justfile's own prose is wrong here and should not be trusted as-is.** `justfile:776-778`
asserts *"two of four promotions landed with a required check red"*. Re-measured: #91, #108 and #130
were fully green; #106's only failure was `workflow-analysis`, which is not one of the six; and
#148's `supply-chain` failure post-dates its own merge by ten minutes. **On current API data, zero
promotions merged with a required context red at merge time.**

### `staging → main` — the expensive one, and now the most valuable

**No promotion has ever reached `main`.** `gh pr list --state merged --base main` returns `[]`.
`main` is a strict ancestor of `staging`, **133 commits and 22 days behind**. The gap is **232 files,
+70,260/−3,777**, carrying SQLite migrations `0002`, `0003`, `0004` (main has only `0001_init.sql`),
their three Postgres mirrors, and **24 distinct microstep tokens** (`1.1.0`–`1.11.2`).

It also carries the entire modern CI surface. **`branch-flow.yml`, `security.yml`, `labeler.yml`,
`proptest-scheduled.yml` and `cross-platform-canary.yml` do not exist on `main` at all**, and
`main`'s `ci.yml` declares just two jobs, `rust` and `web`.

**The sharpest reason to do it, which no earlier handoff stated:** `main`'s `release.yml` is a
**different, 53-line workflow with no `guard` job** — against 635 lines on `development`. It has one
job, **mutable action refs** (`actions/checkout@v4`, `dtolnay/rust-toolchain@stable`,
`tauri-apps/tauri-action@v0`), `permissions: contents: write`, **no `environment: release`**, and
`releaseDraft: true`. A tag push runs the workflow **at the tagged commit**. So a correctly shaped,
signed `v0.1.0` on `main`'s head — whose `Cargo.toml` and `tauri.conf.json` both read `0.1.0` — would
run *that* unguarded file: no signature check, no version check, no updater-config check, no
exact-commit CI wait, no environment gate.

`tags-v-append-only` does **not** close this. It carries `deletion` and `update` only; **`creation`
is absent**, so such a tag is permitted, and what the ruleset guarantees is that nobody — the
maintainer included — can move or delete it afterwards. It makes the mistake **irreversible, not
impossible.** The promotion is the repair; no ruleset rule substitutes for it.

**Nothing server-side stops the promotion itself being squash-merged**, repeating #74's fork on
`main` this time: ruleset `22744344` has no `pull_request` rule, hence no `allowed_merge_methods`.
The only guards are local — `justfile:960` hard-codes `--merge` — plus `branch-flow.yml`'s advisory
`promotion-notice` job and `ci.yml`'s after-the-fact `promotion-shape`. **Use `just promote-merge`.**

The runbook still says to open `staging → main` only **after the candidate has actually been used**,
and that remains right. The counterweight is now explicit and should be weighed deliberately: the
longer it waits, the longer `main` holds a release workflow that would publish unguarded.

### A release cannot be cut today, and signing is not the reason

Signing is configured. The two real blockers:

1. `apps/terminal/src-tauri/tauri.conf.json` has **no `bundle.createUpdaterArtifacts` and no
   `plugins.updater.pubkey`**, which fails `release.yml`'s `guard` step at `:126`.
2. The `release` environment holds **0 secrets**, so `TAURI_SIGNING_PRIVATE_KEY` is absent for the
   `build` job.

### Required contexts — exactly six

```
rust · guards · web · supply-chain · protected-paths · topology
```

Derived from `scripts/watch-pr-checks.sh`; do not hand-maintain a second list. They come from **two**
workflows — `ci.yml` supplies `rust`, `guards`, `web`, `supply-chain`; `branch-flow.yml`
(`pull_request_target`) supplies `protected-paths` and `topology`. **A push-only run therefore
reports 4 of 6**; the other two exist only on a pull request. That is not a gap.

Because `branch-flow.yml` triggers on `pull_request_target`, GitHub reads it from the **default
branch**. So `protected-paths` and `topology` will report on a `staging → main` PR from
`development`'s copy even though `main` has no `branch-flow.yml` — but `main`'s ruleset requires
none of them, so they are visibility only.

**Never require `workflow-analysis`** — `security.yml` is path-filtered, so a PR touching nothing
under `.github/**` produces no check run at all, and a required-but-absent context blocks forever.
Conditionally-skipped jobs *do* produce a run with `conclusion: skipped`, which GitHub treats as
satisfied.

`main` must not get **required checks** until a promotion carries the current `ci.yml` through. It
already has a ruleset; what it lacks is required checks and a required pull request.

### Merging a deliberately-red policy PR — the exact recipe

`just merge` fails closed on `protected-paths`, correctly. Mirror its validations by hand, then
merge with `--match-head-commit`:

```bash
LIVE=$(gh pr view <N> --json title -q .title)
OID=$(gh pr view <N> --json headRefOid -q .headRefOid)
gh pr view <N> --json body -q .body > /tmp/body.txt

bash scripts/validate-change-title.sh --validate "$LIVE"                    # 1
printf '%s\n\n' "$LIVE" | cat - /tmp/body.txt > /tmp/message.txt
mise exec -- python3 scripts/check-automation-attribution.py \
  --message-file /tmp/message.txt                                           # 2
bash scripts/validate-branch-flow.sh <branch> development remote remote     # 3

gh pr merge <N> --match-head-commit "$OID" --squash --delete-branch \
  --subject "$LIVE (#<N>)" --body-file /tmp/body.txt
```

**Before merging, confirm the red names only the pinned file and nothing else**, and put a
"Manual review note" section in the PR body stating what changed, that no gate is weakened, and
which suites were run.

---

## 10 · Decisions taken — do not re-litigate

1. **`bypass_actors` uses `bypass_mode: "pull_request"`**, never `"always"`.
2. **`0004` seeds 128 explicit decision rows.** An absent row would mean both "denied" and "nobody
   decided", which the forward-only law would make permanent.
3. **`decision` is three-valued**, no default.
4. **`role.id` values are deterministic UUIDv7-shaped literals.**
5. **Positioning is operational and minimal-footprint.** `NOASSERTION` is the correct result.
6. **The title check's ambiguous positional form was removed, not reordered.**
7. **`just merge` is the enforceable guarantee, not CI.**
8. **The Dependabot fix adds no `canonical-title` job.**
9. **`branch-flow` reads the PR title live** and tolerates a normalizable Dependabot title.
10. **Every script the justfile invokes must be in the frozen policy surface.**
11. **`staging → main` is not a synchronisation exercise.** §9.
12. **A SHA pin's version comment must name a tag that resolves to that exact SHA.** Never a branch.
13. **No local checker was added for that** — the audit needs the network.
14. **The `# master` staleness was NOT a Dependabot problem.** dependabot-core resolves the `uses:`
    ref through repository tags and never consults the comment. **Do not revive this.**
15. **`pos-db` MAY name `pos-domain` types, and does.** Conventions §3 mandates it.
16. **`DbError` gains no `From<PermissionError>`.**
17. **`deactivated_user_denied` belongs to 1.8.3, not 1.6.4.**
18. **"One migration per session" is not repository law.** The law is WIP = 1 microstep plus
    forward-only migrations.
19. **`1.6.4` shipped as ONE pull request, not two.**
20. **The `sale` foreign keys are repaired by triggers, not a rebuild.** `PRAGMA foreign_keys` is a
    no-op inside a transaction and the runner wraps every file in one.
21. **`store_credit.is_internal` is seeded `0` and recorded as `⚠️ OPEN`, not guessed.** Settle the
    semantics for all six codes, not just `exchange`.
22. **The board's `Phase` field tracks §6a's *order by*, not the microstep's own phase.** §11.
23. **`1.11.0`'s `src/test/setup.ts` stays a `.ts`.** Vite 8 runs on Oxc, which refuses JSX in a
    `.ts` file with no per-file override.
24. **The harness declares `setupFiles`** rather than relying on a test file's import.
25. **The provider tree is passed as `wrapper`**, with a fresh `QueryClient` per call and
    `retry: false`. react-query v5 has no `logger`.
26. **`1.11.0` includes `React.StrictMode`.** A harness that renders differently from production
    lies.
27. **`1.11.0` ships three tests where its `Tests:` line names one.** The other two are the harness
    proving itself. The `Tests:` line was **not** amended.
28. **The fake-timer `jest` bridge lives in the harness, not at each call site.**
    `vi.useFakeTimers({ shouldAdvanceTime: true })` was rejected — it reintroduces the scheduler
    dependence fake timers exist to remove.
29. **`phase-0-closeout.md` keeps `= "deny"`.** It is designated historical evidence; editing it
    would falsify a dated record.
30. **`store_credit.is_internal` was NOT folded into `00-master-plan.md` §4a.3**, whose scope is
    corrections that change an architecture rather than a value.

### Taken 9–11 September

31. **`core.hooksPath` stays RELATIVE.** Making it absolute looks like the fix and is worse — the
    setting is shared across linked worktrees, so `just setup` inside one repoints the whole
    repository. Measured end to end: after `git worktree remove`, the main tree committed a
    grammar-free subject with **no refusal**. Detection (#134's checker) is the fix.
32. **`post-checkout` is PERMANENTLY DECLINED.** The premise that both hooks' exit statuses are
    ignored is **false** for it, measured: exit 9 leaves `git merge` at 0 but gives `git switch`
    exit 1, `git clone` exit 9 and `git worktree add` exit 9. An advisory that can break a fresh
    clone inside any `set -e` script is a bad trade.
33. **`just pre-push` will NOT be renamed to `just verify`.** 124 `pre-push` hits across 26 files
    (46 recipe, 78 hook, no grep separates them); `scripts/check-justfile-policy.py:190` hardcodes
    `pr_body.find("just pre-push")` and fails closed, so a rename reds `just guards` and CI's
    `guards` job — and that checker is itself byte-pinned. The collision is answered in prose.
34. **`branch-flow.yml` keeps its per-commit attribution loop.** Its step bodies are byte-pinned
    inside `check-branch-workflow-policy.rb`, and there is nothing to win — the last ten merged PRs
    carried 1–3 commits each, ≈0.15s.
35. **A new flag on a policy script CI runs from the base needs a two-PR bootstrap.** `ci.yml:370`
    deliberately runs the **base** revision's verifier so a PR cannot supply the policy that judges
    it. That is why #144 could not be folded into #143.
36. **The scheduled per-flow-branch advisory job was REMOVED, not reshaped (#146).** CodeQL raised
    three high-severity `actions/cache-poisoning/poisonable-step` alerts and was right about the
    shape: a `schedule` run holds the default branch's privileged cache scope, and that job checked
    out another ref and executed it. `schedule` has no unprivileged variant, so no placement kept
    the feature and cleared the finding. **The real repair is the `staging → main` promotion.**
37. **No CodeQL badge in the README.** CodeQL runs as default setup, so
    `.github/workflows/codeql.yml` is a 404 and the badge would render permanently broken.
38. **`check-branch-workflow-policy.rb`'s adversarial fixtures must never be named after real
    hooks.** They used `.githooks/post-merge` and `.githooks/post-checkout`; creating the real
    `post-merge` collided with `File.symlink: File exists (Errno::EEXIST)` and took the self-test
    from 217 passed to a crash at 96. They are now `fixture-added-hook` and `fixture-symlinked-hook`.
39. **A moved `.nvmrc` cannot be fixed by `just setup` alone** — `setup` runs the fail-closed
    `check-node-version.py` and refuses **before** installing anything. The advice is `nvm use` (or
    `fnm use`) **then** `just setup`.
40. **SQLite migrations are NOT applied by `just migrate`** — that runs `sqlx migrate run` against
    the **Postgres mirror only**. The register's SQLite chain is applied by the application at
    runtime; the useful local command is `just verify-schema`.

---

## 11 · The board `Phase` field — why #69 reads Phase 1 with Microstep 2.7.0

Not a contradiction. `00-master-plan.md` §6a is *The long-lead register*, whose columns are
**`Item | Order by | Needed for | If it is late`**, with the note *"**Order by** is when the request
goes out, not when the item is used."* So `Phase` = *order by*, `Microstep` = *needed for*.

**Five** rows are ordered in Phase 1, not four: the legal entity, the official ISTD package, written
ISTD Directorate answers, **the acquirer conversation and a physical test terminal**, and Jordanian
counsel plus the merchant's tax advisor on retainer. Two rows are ordered *before group 1.7*: the
thermal printers plus one scanner, and the reference register itself.

`Risk` follows `03-github-workflow.md`, which defines the family as *"how it must be reviewed"*.
`Priority` is P1 on the external blockers because only P0 is defined — *"wrong money, lost sale,
corrupted data, compliance breach"*. #114, #119 and #120 are P2. **#68–#71 carry no priority or risk
label on the issue at all** — see §3.

---

## 12 · Traps that will bite you

| Trap | What to do |
|---|---|
| The shell's Node fails the fail-closed `.nvmrc` pin | Prefix every recipe with `mise exec --`. Never edit `.nvmrc`. Fix a moved pin with `nvm use` first — `just setup` refuses before installing |
| `mise exec --` fails after a `cd` in the same invocation | Run it from the working directory with an absolute script path; never build the command in a shell variable |
| **Commit subjects are capped at 72 characters before the step tag** | `bash scripts/validate-change-title.sh --validate '<title>  [—]'`. Note the **`--validate`** — without it the script echoes the title and exits 0, which reads as a pass |
| The step grammar **admits a letter** | `[1.1.2a]` is valid and 13 Phase-1 microsteps require it |
| **`just branch` refuses `feat/…`** | Microstep work uses `phase-<0-5>/group-<m>-<kebab-slug>`; also allowed are `fix/`, `docs/`, `refactor/`, `perf/`, `test/`, `chore/`, `hotfix/`. `just pr` derives the milestone from the `phase-<0-5>/` prefix |
| `just pr`'s `$body` is a **file path**, not text | `just pr 'title' path/to/body.md` |
| `just branch` runs `git pull` internally | On a network stall it fails *silently* under `\| tail -1` and leaves you on `development`. Check `git rev-parse --abbrev-ref HEAD` after |
| **A deliberately-red policy PR cannot go through `just merge`** | It fails closed on `protected-paths`, correctly. §9 has the exact recipe |
| **Merging a work PR with a merge commit drags raw bot commits across** | Two malformed titles are already on `development` this way. **Squash a work PR**; the server permits both |
| **Every Dependabot Action-SHA bump reds `protected-paths`** | By construction. Expect it monthly; read the diff, then merge through the bypass |
| **`just fmt` does not fix `organizeImports`** | Biome *assists* are applied only by `biome check --write`, which no recipe wraps. Run `pnpm biome check --write <files>` by hand |
| `git add -A` sweeps in **`PROJECT-GUIDE.md`** | `git add <only the files for this concern>`. This file is tracked; `PROJECT-GUIDE.md` is not |
| **`tests/common/mod.rs`'s `full_schema` weakens the connection** | It disables foreign keys and layers on unshipped reference blocks. Open the **registered chain** through `pos_db::open` in any test meant as evidence. Triggers still fire there |
| **A successful API response is not evidence of a change** | The secret-scanning fields return `200` and stay `disabled`. Read the value back. But note the inverse for CodeQL: its `PATCH` is accepted **asynchronously** and reads stale until the setup run finishes |
| **Emptying `.claude/settings.json` to relax permissions** | It relaxes nothing and reds the gate — `test-settings.py` runs in `just guards` and in the `guards` CI job. Repair with `git checkout --`, then confirm 30 passed |
| **A settings edit does not take effect mid-session** | Permission settings are snapshotted at session start. Restart |
| **The swiftly `cc` shim** can break every Rust build | `cc` resolves through `~/.swiftly/bin`. Fix with `swiftly use --global-default xcode`. Never prepend `/usr/bin` — it shadows Homebrew's Python with macOS 3.9 |
| **`pnpm install` aborts with no TTY** after lockfile merges | `CI=true mise exec -- pnpm install --frozen-lockfile` |
| **A dependency change makes `--frozen-lockfile` fail everywhere** | Regenerate the lockfile **in the same commit**: `mise exec -- pnpm install` at the repository root |
| A fresh worktree has **no `node_modules`** | `pnpm install --frozen-lockfile` before believing any JS gate |
| **Reading a repository file from inside a jsdom test** | No Node types in `src/**` under `tsc -b`, and Vite rewrites `new URL("<literal>", import.meta.url)`. Read it in `vite.config.ts` |
| **Prose naming the forward-only SQLx revert inside backticks within a *shell* command is refused** | Known false positive; the hook's segmenter splits before `shlex` sees the quoting. Ordinary quoting is safe, and editing a file that discusses it through Edit/Write is unaffected. Deliberately not softened |

**One trap from the last handoff is now half obsolete.** *"A stale branch reds `protected-paths` on
files it never touched"* no longer applies to the source-plan/migration wall —
`check-protected-paths.sh` resolves the merge base at `:197` and diffs `$merge_base..$head`. The blob
comparison survives only in `check-branch-workflow-policy.rb`, whose trusted root is
`github.workflow_sha`. **The trap is now specific to the frozen-policy surface, not to migrations.**

---

## 13 · Workflows — what seven runs have taught

Seven audit workflows have now been launched across five sessions. **The 11 September run finished**:
nine dimensions, twenty agents, 1,107 tool calls, each dimension followed by an adversarial verifier
whose default verdict was REFUTED. It produced this document, and **69 of its load-bearing claims
were refuted or corrected on re-measurement.**

What is established:

- **A workflow earns its keep on a wide, read-heavy, parallel question with no single command as
  oracle** — state surveys, pre-implementation scoping, stale-documentation sweeps.
- **A workflow adds little when a gate is the oracle.** SQL that runs is stronger evidence than
  agent consensus.
- **Make the refuter's default REFUTED and demand a command.** This is the highest-value half of the
  harness by a distance. In this run the refuters caught: a wrong count of workspace members (4 vs
  **7**), a wrong checker count (17 vs **16**), an endpoint queried with the wrong default window
  (2 bypasses vs **23**), a blocker overstated from "the `doc_sequence` half of `0005`" to "all of
  `0005`", an issue (**#70**) credited with blocking two microsteps it does not name, another
  (**#69**) credited with gating Phase 1 when its own body says group 2.7, and **roughly thirty
  off-by-one line citations**. Without that pass this handoff would have carried every one.
- **Tell the agents which files are not evidence.** `docs/handoff.md`, `PROJECT-GUIDE.md` and
  `docs/orientation.md` are secondary. One verifier still surfaced a `handoff.md` line as a "second
  occurrence" of a phrase — which is exactly the circularity the instruction prevents, and exactly
  how §5's "returns exactly one occurrence" claim became false.
- **Tell them not to write in the repository.** Check `git status --porcelain` after every run.
  This run left the tree byte-identical.
- **Give the heavy builds to exactly one agent.** Nine agents running `cargo` concurrently contend
  on the target directory lock.
- **Workflow resume is still impossible while the `~/.claude` read-deny stands** (#114): every
  persisted script lives under that denied prefix, and copying one out is refused separately.
  **The fix that works:** keep the script in the working tree.

Dimensions worth reviving narrowly: the deliberate-rejections inventory, and a sweep of the
documentation surface **no gate reads** (`status-page.html`'s prose — see §14).

---

## 14 · Loose ends

- **`docs/implementation/status-page.html:525` is stale and no gate can see it.** It still reads
  *"`main` is deliberately unprotected until a promotion carries the current `ci.yml`"*. `main` has
  carried `main-append-only` since 10 September. `check-doc-links.py` filters on `.md`, and
  `check-implementation-frontier.py` reconciles only the page's per-phase **step counts**, never its
  prose — which is the structural reason this one sentence survived while #136 corrected eleven
  places and #158 corrected four documents. **This is the smallest real bug in the tree.**
- **`PROJECT-GUIDE.md`** — untracked, 123,758 bytes, 2,265 lines, self-dated to `6d997f2`
  (28 August). **78 commits separate that from HEAD.** Of 17 factual claims spot-checked, **11 are
  now false**, three of which contradict `CLAUDE.md` outright — including the
  `float_arithmetic = "deny"` error at `:189` that #121 fixed everywhere tracked. **Do not commit it
  as-is.** It *is* already link-clean (`check-doc-links.py --working-tree` → 49 files, exit 0, and
  that mode is what both agent write-guards invoke), so committing costs nothing in link findings —
  the cost is maintaining a drifting parallel copy of a plan of record that already exists. Before
  deleting it, **extract the gaps in it that are still accurate**: `:2216`'s claim that
  `.github/ISSUE_TEMPLATE/05-drill-result.yml` does not exist is still true. **Put to the operator
  on 8 September and again on 9 September; still unanswered.**
- **Local branch hygiene.** The warning in the last handoff protected three branches
  (`backup-before-rewrite`, `pr77`, `pr78`) that **no longer exist in this clone**. What does exist:
  **nine feature branches with `[gone]` upstreams** from the 9–11 September work, a
  `refs/original/refs/heads/main` filter-branch backup at `a7c2379`, and seven
  `refs/codex/turn-diffs/checkpoints/*` refs. `git branch -d` refuses the nine because squash merges
  break ancestry; `git branch -D` is safe for all nine — each is merged content on `development`.
- **Your local `staging` is 51 commits stale** — it still sits at #91's promotion merge, four
  promotions behind. `just promote-staging` without fetching first works from the wrong base.
- **One stash remains:** `stash@{0}: On fix/float-arithmetic-forbid: codex scanner approach,
  superseded by forbid`. PR #121 landed, so it is safe to drop whenever you like.
- **`scripts/check-domain-acyclic.py` has no `--self-test`**, and it **silently ignores any
  argument** — passing `--self-test` exits 0 while printing the live graph. It is a real merge wall:
  `just lint` runs it and so does `ci.yml:108`, inside the `rust` job. So `CLAUDE.md`'s blanket
  *"All are negative-tested"* is **false for the checker that enforces invariant 8's acyclicity**,
  and `just guards` never touches it. `scripts/check-staged-policy.py` has no `--self-test` either;
  it is exercised only indirectly through `.githooks/test-hooks.sh`.
- **`just lint`'s clippy carries no `--all-features`**, so feature-gated code is unlinted locally.
  Not a failure today; just do not overstate clippy's scope.
- **`@pos/ui` and `@pos/api-types` declare no `test` script**, so `pnpm -r --if-present test` runs
  zero tests for them. They are covered only by `tsc --noEmit`. `pos-sync` is still a 27-line stub
  with **zero tests** — the only workspace crate with none.
- **`1.6.4`'s five stated limits are in the phase file, not here.** The load-bearing two: `0004`'s
  matching trigger does **not** compare `content_hash`; and `approval_consumption.effect_id` has no
  foreign key, so the rollback test proves shared-transaction plumbing, **not** that a consumption
  requires a financial effect — that is `1.8.3`'s.
- **`ApprovalHandle` no longer derives `Deserialize`**, and a `trybuild` case keeps it that way.
- **The `0004` Postgres mirror covers `capability` only** — not `role` or `role_capability`. Both are
  named in the mirror's deferral block. Worth a second look at Phase 3.
- **The Dependabot squash-body half is still unverified** and needs an actual Dependabot squash to
  compare. The human path was verified at #109.
- **`autoMode.environment` in the user-level Claude settings describes the wrong project.**
  Unverifiable from a session, because reading anything under `~/.claude` is refused (#114). Carried
  forward unconfirmed.

---

## 15 · The lessons this project keeps re-learning

1. **Sweep the defect class across the repository, not the file where it surfaced — and search by
   subject, not by phrase.** #136 is the clearest case yet: one wrong belief about `main` had
   **eleven** live sites.
2. **Verify a finding against the source before recording it.** In the survey behind this document,
   69 load-bearing claims were refuted or corrected on re-measurement, and roughly thirty of those
   were line numbers that had drifted by one. A handoff is a secondary source; `git`, `gh` and the
   files are primary and one command away.
3. **Prove the guard fails.** #135's five new tag cases exist because the release-tag chain past the
   lightweight check was **entirely untested** — every assertion passed a commit SHA, so the refusal
   at `pre-push:115` fired first and five refusal branches never executed. That is the least
   affordable chain to leave untested: `tags-v-append-only` has **no bypass actor**, so a rejected
   tag cannot be moved or deleted by anyone and the version number is permanently spent.
4. **Check `git diff` before believing a file's contents are the repository's intent.**
5. **A successful API response is not evidence of a change. Read the value back** — and know that
   the inverse also happens: CodeQL's `PATCH` is accepted asynchronously and reads stale.
6. **Prefer the check that answers in one command over the harness that answers better eventually** —
   but do not mistake an interrupted tool for a broken one.
7. **When a document specifies a design, transcribe it; do not redesign it.**
8. **When a document's own prose makes a promise the tree does not keep, the step that makes the
   promise makes it true.**
9. **A control that fires on every merge is not a control; it is a habit.** 23 of 33 ruleset
   evaluations were administrator bypasses. Most were the byte-frozen-surface review working as
   designed — but "most" is the word that makes a ledger necessary, because nothing distinguishes
   the designed reds from the impatient ones except reading them.
10. **The document nobody's gate can read is the document that goes stale.** Four documents were
    corrected on 11 September; the one sentence that survived is in the HTML file no checker parses.
