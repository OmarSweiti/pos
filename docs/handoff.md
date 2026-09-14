# Handoff — the single current one

**Reflects `development` @ `60ef37f`, 14 September 2026.**

There is one handoff — keep updating this file rather than adding a dated one.

> ## ⚠ READ §2b FIRST — microstep `1.9.1` is IN FLIGHT and the tree on its branch is RED
>
> Migration `0005` is written, registered and schema-verified on the pushed branch
> **`phase-1/group-9-migration-0005`** at **`e6d774f`**, and `just test` fails there on **17
> pre-existing tests, by design**. `development` itself is untouched and green. §2b is the complete
> resume instruction: what is done, what is left, and the one command to run before editing `0005`.

**13 September moved the product, for the first time since 8 September.** Six pull requests merged
into `development` — #161, #163, #164, #165, #166, #167 — and one of them, **#165, landed microstep
`1.11.3`**: `apps/terminal/src/lib/format.ts` and thirteen tests. Phase 1 is **21 of 112 (~19%)**.
Two P2 gap issues closed with it (#119, #120), and #113 took a decision. §2a is that window; §2
keeps the 9–11 September record, where twenty-six pull requests changed the governance layer and no
microstep advanced.

**`development` is green, tip included.** `just pre-push` exits 0 at `60ef37f`, all 37 `just guards`
steps pass, and `ci` run **`34757633967` is a success on the tip**. The cancelled-CI caveat that
stood here on 13 September is resolved: `34753605560` was cancelled on `3014a88` when five merges
inside two minutes tripped `ci.yml`'s concurrency group, and #167's own merge then produced a clean
run. **The lesson survives the fix** — after a burst of merges, query the tip specifically rather
than assuming the newest green run covers it.

**The WIP=1 slot is FULL.** 0 open pull requests, but `1.9.1` is in flight on the branch above. Do
not start a second microstep. Board #4 reads **eleven items — eight `Todo`, three `Done`**; no item
is `In Progress`, because **no Microstep issue has been filed for `1.9.1` yet** — that is the first
item of unfinished work in §2b.

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
mise exec -- just pre-push          # passes at 60ef37f

# To resume the in-flight microstep (§2b) instead:
git checkout phase-1/group-9-migration-0005    # at e6d774f, pushed
mise exec -- ./scripts/verify-schema.py        # exits 0
mise exec -- cargo nextest run -p pos-db --no-fail-fast   # 17 fail, by design
```

**`just setup` without `mise exec --` fails.** The shell's Node is `v26.4.0`; `.nvmrc` pins
`24.19.0` exactly and the check is fail-closed. This is the first thing that goes wrong every time.

**One operational gotcha, and the last two handoffs both diagnosed it wrongly:** the failure is a
command built in a **shell variable**, nothing else. `CMD="node --version"; mise exec -- $CMD` fails
with `couldn't exec process: No such file or directory`, because the whole unsplit string becomes
`argv[0]`. Pass the words literally. **A `cd` earlier in the same Bash invocation is harmless** —
`cd <repo> && mise exec -- node --version` prints `v24.19.0` — and a relative script path works.
Re-measured 13 September; the `cd` clause that stood here was false.

### Verified gate baselines at `60ef37f`

Every row re-measured on 13 September. Use these as the "nothing is broken" reference.

| Command | Reads |
|---|---|
| `just pre-push` | **exit 0** — `lint test build-web guards secrets`, `justfile:368`, 1:29.90 warm |
| `just check` | exit 0 — all **seven** workspace members |
| `just lint` | exit 0 — 2 prerequisites + 14 body steps = **16 checkers** |
| `just test` | exit 0 — **241 tests run: 241 passed, 2 skipped**; JS **6 files / 44 tests** |
| `just build-web` | exit 0 — `tsc -b` + vite 8.2.2 across 5 packages |
| `just guards` | exit 0 — **37 steps** |
| `just verify-schema` | exit 0 — **4 migrations, 32 tables, 300 columns** |
| `just verify-pg` | exit 0 — **the engine pass RAN** via the Docker fallback |
| `just secrets` | exit 0 — gitleaks 8.30.1, `--history` |
| `just audit` | exit 0 — cargo-deny clean; **135 package releases, 11 reviewed expressions** |
| `just bench-gate` | **REFUSED, exit 3** — no reference register. Correct, not broken |
| `pnpm --filter terminal exec vitest run` | **4 files, 31 tests**, vitest **5.0.0** |
| `pnpm --filter backoffice exec vitest run` | 1 file, **3 tests** |
| `pnpm --filter money exec vitest run` | 1 file, 10 tests |
| `check-implementation-frontier.py` | phase 1: **112**, 2: 61, 3: 45, 4: 42, 5: 36 |
| `check-test-catalog.py` | reconciles, **92 cases** |
| `check-js-licenses.py` | **135** package releases, 11 reviewed expressions |
| `test-gh-setup.sh` | **69 passed, 0 failed** |
| `check-branch-workflow-policy.rb --self-test` | **219 passed, 0 failed** |
| `.githooks/test-hooks.sh` | **136 passed** |
| `check-protected-paths.sh --self-test` | 18 passed · `watch-pr-checks.sh --self-test` **49** |
| `test-settings.py` | **30 passed**; `.claude/settings.json` is **4,426 bytes** |

**The three per-package vitest rows must sum to the `just test` row.** They do now — 4 + 1 + 1 = 6
files, 31 + 3 + 10 = 44 tests. The `money` row is new here; without it the sums did not reconcile
and a reader could not tell which number was wrong. The JS counts are the only rows that move
often, because every UI microstep adds tests: #164 took the back office 2 → 3 and #165 took the
terminal 18 → 31 on the same day.

The self-test counts are stable anchors rather than moving targets, and have held across eleven
commits and two handoff generations: `check-branch-workflow-policy.rb` **219**,
`.githooks/test-hooks.sh` **136**, `test-gh-setup.sh` **69**.

**Toolchain actually in use:** rustc 1.97.1, cargo-nextest 0.9.143, node v24.19.0, pnpm 11.22.0,
vitest 5.0.0, gitleaks 8.30.1, Docker Engine 29.5.2.

### Two things the gate table does not say

* **`just verify-pg` ran the real engine pass here**, because `$DATABASE_URL` is unset *and* Docker
  is running, so the Docker fallback fired (`postgres:18-alpine`, pinned by digest at
  `scripts/verify-pg-migrations.py:66-70`). With Docker stopped it prints that it skipped the engine
  pass and still exits 0. **Read the output line, not the exit code.**
* **There are no executable doctests at all**, which is stronger than the "doctests run nowhere"
  this file used to say. `just test` is `cargo nextest`, which does not run them, and no CI job
  does — but nothing would run if one did. The 10 fence lines are **5 blocks in 4 files**
  (`pos-db/src/repo/outbox.rs:390`, `pos-domain/src/catalog.rs:165`, `ids.rs:216` and `:249`,
  `audit.rs:291`), and **every one opens ```text**. They are prose diagrams, not compiled examples.
  So they can drift from the code silently and no gate will notice — and adding a `--doc` step to
  `just test` would be a no-op, not a fix. The fix, if one is wanted, is turning a diagram into a
  compiled example, one block at a time.

---

## 1 · Where the project stands

| | |
|---|---|
| `development` | **`60ef37f`** — `just pre-push` exits 0 and `ci` run **`34757633967` is green on the tip**. Untouched by the in-flight `1.9.1` work, which lives only on its own branch |
| `staging` | **`531ea04`** — **17 behind** `development`, 5 ahead (its own five promotion merges) |
| `main` | `24a0283` — **145 behind** `development`, **133 behind** `staging`, untouched since 20 August |
| Phase 1 | **21 of 112** executable microsteps (~19%) — `1.11.3` landed 13 September (#165), the first advance since 8 September |
| Open PRs | **0** — but see §2b: `1.9.1` is in flight on a pushed branch with no PR open |
| Open issues | **8** — see §3. None is `In Progress`, but **the WIP=1 slot is FULL**: `1.9.1` is in flight (§2b) with no Microstep issue filed yet |
| Board #4 | the API's default listing returns **11 items — 8 `Todo`, 3 `Done`** (#119, #120 and #162, all closed 13 September). Archived items are excluded from that listing and their count is not readable through it |
| Rulesets | **four, all active**, all four checked in under `.github/rulesets/`. They **agreed with live when last compared by hand** (11 September) — no gate diffs them, so this is a dated observation, not an invariant. See §3 |
| Tags / releases | **zero of each.** The append-only tag ruleset has never been exercised |
| Repository | **PUBLIC**, GitHub Free, `OmarSweiti` the sole collaborator (admin) |

### Complete: 21 microsteps

Read live from the frontier region, `docs/implementation/README.md:22-41`, in its own order:

`1.1.0` `1.1.1` `1.1.2a` `1.1.6` `1.1.3` `1.1.4` `1.1.2b` `1.1.7` `1.1.5` `1.1.8` `1.2.1` `1.2.2`
`1.3.1` `1.8.9` `1.8.5` `1.11.2` `1.6.5` `1.6.1` `1.6.3` `1.11.0` `1.11.3`

**When the 22nd lands, the region must read `22 of 112 executable microsteps fully complete
(~20%)`.** The checker computes `round(100 * done / total)` and `round(100*22/112) == 20`; leaving
`~19%` is a hard `just lint` failure, not a rounding quibble. This bit on 13 September and cost
nothing only because it was expected: `1.11.3` had to move the count, the percentage **and** the
prose list in one commit. The same denominators appear on three
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

## 2a · What landed 13 September — six pull requests, one microstep

Six commits. **The first product code since 8 September**, and the first time a microstep and its
issue moved together.

| PR | Commit | What |
|---|---|---|
| **#161** | `021cd25` | conventions §5 gains a **DOM component** row and "Nine layers" becomes "Ten"; §4.3's Vitest row splits in two, so every `Layer` cell names a real §5 layer. **Closes #120** |
| **#163** | `6ccf253` | `status-page.html`'s Protection row stops saying `main` is unprotected — the twelfth site of the belief #136 corrected in eleven. Also "two source plans" → **three** |
| **#164** | `afc7395` | `apps/backoffice` gets `setupFiles` and a cleanup-only `src/test/setup.ts`. **Closes #119** |
| **#165** | `5220f46` | **microstep `1.11.3`** — `format.ts`, thirteen tests, the frontier to 21 of 112. **Closes #162** |
| **#166** | `3014a88` | the Microstep issue form stopped contradicting its own body text |
| **#167** | `60ef37f` | this handoff, refreshed — 74 stale claims corrected, four sections rebuilt |

### The four rulings that matter

**1 · `latin_runs_inside_arabic_text_are_bidi_isolated` belongs to `1.11.12`, and the duplicate is
gone.** It was named by both `1.11.3` and `1.11.12`. #165 struck it from `1.11.3` and trimmed the
matching clause from that step's `Done when`. Three primary sources decided it —
`ref/test-catalog.md:314`, `02-development-workflow.md:1709`, and `ref/ui-spec.md:292`, which files
bidi isolation on the DOM-component rung `1.11.12`'s `EdgeStates.test.tsx` occupies and a pure
`src/lib/format.ts` does not. The arithmetic sealed the direction: `1.11.12`'s `Done when` says "all
**five** named rendered-state tests" and its `Tests:` line names exactly five, so striking it there
would break a counted condition. **The name now appears exactly once in the phase file.**

**2 · `ar-JO` renders Arabic-Indic digits, and bare `ar` does not.** `format.ts` pins the Intl
numbering system to `latn`, and a test that only ever passes `"ar"` passes **without** the pin —
current ICU resolves bare `ar` to `latn`. `ar-JO` resolves to `arab` and renders ٢٠٢٦ and ٠٠:٣٠, and
so does `ar-EG`; measured on node v24.19.0, full ICU. Regionalising `Locale` to `ar-JO` is the
obvious next edit for a Jordanian product and is exactly the edit that would switch every receipt
date to digits §10 forbids. The rationale is recorded at the constant, not here.

**3 · `formatMoney` wraps `formatMinor`; it does not re-implement it.** `packages/money`'s own
docstring marks the seam — "no grouping separators and no currency symbol, because both are locale
decisions … The locale-aware wrapper belongs in the UI layer, over this exact string." So the
exponent is never re-derived in the UI and nothing divides by 100.

**4 · #113 has a decision, and it is a hedge rather than an answer.** Option 4: `0005` ships
`doc_sequence` as `ref/schema.md` specifies it, `scope_kind IN ('register','store')`, chosen
deliberately. See §3.

### Two traps this window created, and one it disproved

**A · A rebase to clear `BEHIND` throws away the green checks.** `development` carries
`strict_required_status_checks_policy: true`, so every sibling PR goes `BEHIND` the moment one of
them merges. Updating the branch mints a **new SHA with no check results**, discarding the run that
had just passed. Merging straight after therefore merges with six checks *in progress*, which the
ruleset records as a bypass. **Three of the five merges on 13 September did exactly this**, all
reading `6 of 6 required status checks are in progress`. Nothing unsafe merged — every branch was
green before its rebase — but the ledger cannot tell "green, then rebased" from "never green". Cut
each branch from the previous one, or wait for the post-rebase run.

**B · Five merges inside two minutes cancel CI on the tip.** `ci.yml`'s ref-scoped concurrency group
cancelled the runs on `5220f46` and `3014a88`. That specific gap is closed — #167's merge produced
a green `ci` on `60ef37f` — but the mechanism is not, and it is invisible unless you query the tip
specifically.

**C · `mise exec` after `cd` is fine.** Two handoffs asserted it fails. It does not — see §0. The
real failure is a command built in a shell variable.

---

## 2b · IN FLIGHT — microstep `1.9.1`, migration `0005`

**Branch `phase-1/group-9-migration-0005`, pushed, at `e6d774f`. `just test` is RED there on 17
pre-existing tests, by design. `development` is untouched and green. Do not merge this branch.**

Started 13 September after #113 recorded its decision. This section is the complete resume
instruction; nothing about the work lives only in a session transcript.

### Done, with the evidence

| | |
|---|---|
| `crates/pos-db/migrations/0005_sale_columns_and_sequences.sql` | **1,179 lines.** Transcribed verbatim from `ref/schema.md` §0005's **three** code fences (`1673-2242`, `2261-2649`, `2670-2835`) in document order |
| Object counts, checked against the specification | **16** `CREATE TABLE` · **5** `CREATE INDEX` · **2** `CREATE UNIQUE INDEX` · **60** `CREATE TRIGGER` · **1** `CREATE VIEW` · **1** `INSERT` · **14** `ALTER TABLE` = **99 objects** |
| `crates/pos-db/src/lib.rs` | appended to `MIGRATIONS`. `SCHEMA_VERSION` derives from array length — no separate edit |
| `crates/pos-db/tests/common/mod.rs` | `reference_blocks()` **5 → 6**, with its doc comment |
| `ref/schema.md` §0005 heading | gained **`· SHIPPED`** |
| `./scripts/verify-schema.py` | **exits 0.** Chain moved 4 migrations / 32 tables / 300 columns → **5 / 48 / 457** |

### Left to do, in order

1. **File the Microstep issue for `1.9.1`**, put it on board #4, set `In Progress`. Not done
   deliberately: the form calls a late-appearing file "a scope leak", and the true `Files:` list was
   unknown until the test blast radius was measured. It is known now — item 6 below.
2. **Repair the 17 failing tests** (named below).
3. **Write the six tests `1.9.1` names**, in
   `crates/pos-db/tests/migration_0005_sale_columns_and_sequences.rs` (new, name fixed by
   `phase-1:1050`): `opening_a_second_shift_for_the_register_is_refused` ·
   `a_completed_sale_requires_an_open_matching_shift` ·
   `migration_0005_preserves_completed_sale_guards` ·
   `every_completed_tender_has_an_initial_status_event` ·
   `exchange_tender_seed_matches_internal_contract` ·
   `an_exchange_tender_never_opens_or_counts_the_drawer`.
4. **The Postgres mirror** — `apps/server/migrations/<14-digit UTC>_<lower_snake>.sql` with a
   `-- Mirrors SQLite 0005_sale_columns_and_sequences.sql` header, plus `REGISTER_LOCAL` entries in
   `scripts/verify-pg-migrations.py` for the tables that never sync. Candidates the scope identified:
   `parked_cart`, `checkout_operation`, `product_quick_add_request`, `trusted_time_state` — **verify
   each against `ref/schema.md` rather than trusting that list.**
5. **Rewrite the three ⚠️ OPEN blocks that still say the ICV choice is unmade** —
   `phase-1:1055`, `ref/schema.md:2651`, `ref/schema.md:4250`. Each enumerates #113's four options or
   says `1.9.1` "must choose deliberately first". Landing `0005` without rewriting all three leaves
   the plan of record contradicting a decision that has been taken.
6. **Advance the frontier to `22 of 112 executable microsteps fully complete (~20%)`** in
   `docs/implementation/README.md` — `round(100*22/112) == 20`, and the prose list must gain
   `1.9.1`. **Extend `1.9.1`'s own `Files:` line** to name what actually changed, the way `1.11.0`
   and `1.11.3` do; it currently names two files and the change touches ten.
7. Consider whether the **deferred `1.1.9` database half** (`phase-1:1057-1060`,
   `crates/pos-db/src/repo/clock.rs`, test `clock_state_survives_restart`) lands in the same PR.
   `trusted_time_state` now exists, which was its only blocker. It is a *separate* microstep half
   with its own `Done when`; deciding is part of the work, not a given.

### The 17 failures, and why they are correct

`0005` adds **seven gates** to sale completion, each with an `_insert` and an `_update` trigger:
shift · tax policy · fiscal decision · tax components · discount recap · tender events · durable
outputs. Every pre-`0005` test completes a *minimal* sale, so none can satisfy them. The
representative refusal is
`sale completion atomically requires its original receipt job and complete sync commit`.

```
approval.rs           a_consumed_handle_is_still_consumed_after_restart
                      a_handle_used_twice_is_refused
                      the_effect_and_the_consumption_commit_together_or_not_at_all
durability.rs         a_committed_sale_is_readable_on_a_fresh_connection
migration_0003.rs     after_the_rebuild_the_six_tables_enforce_their_types
                      the_rebuild_restores_the_immutability_triggers
outbox.rs             a_completed_sale_has_one_ready_sync_commit
                      a_second_commit_cannot_claim_the_same_fact
                      delivery_rows_can_be_pruned_without_losing_the_manifest
                      every_fact_member_is_in_the_commit_manifest
                      outbox_commit_rolls_back_with_the_fact_graph
sale_immutability.rs  a_completed_sale_refuses_update_and_delete
                      a_completed_tender_refuses_settlement_updates_and_reparenting
                      a_parked_sale_is_still_editable
                      a_receipt_number_is_unique_per_register_but_not_across_them
                      every_immutability_trigger_survives_the_migration_runner
                      the_lines_of_a_completed_sale_are_frozen
```

**`fact_table_guards.rs` and `authorization_scope.rs` PASS**, which is the evidence the
`reference_blocks()` bump was right: they were already getting `0005`'s shape from the reference
document and now get it from the shipped migration instead.

**The intended repair** is one shared registered-chain fixture — a `complete_sale` helper that seeds
a valid world and passes all seven gates — rather than seventeen local patches. No pos-db test today
inserts a `register` or `store` row on the registered chain; that recipe exists only inside
`fact_fixture.rs`, reachable from `full_schema` suites. Whoever writes it is writing the crate's
first registered-chain store/register seed.

### Before you edit `0005`, run this

```bash
git reset HEAD~1        # ONLY if 0005 itself must change
```

Once a migration is in `HEAD`, `.claude/hooks/protect-immutable.py` and `.githooks/pre-commit`
refuse to edit it — `is_committed` tests membership of `git ls-tree HEAD`, not whether anything
shipped. Nothing on this branch has shipped, merged, or touched a protected branch, so moving it
back out of `HEAD` is the intended escape. **A second migration to fix a typo in an unmerged one
would be permanent for no reason.** Re-commit when it is right.

### If the file is ever lost, it is reproducible

Concatenate `ref/schema.md` lines `1674-2241`, `2262-2648` and `2671-2834` (the fence *contents*,
excluding the fence markers) in that order, and prepend the header. **The fence split is the primary
transcription hazard**: fence 1 holds only 55 of the 99 objects, so copying "the 0005 code block"
silently drops `tender_type` and its seed, `parked_cart`, `checkout_operation`, the receipt and
print tables, `product_quick_add_request`, `doc_sequence`, `trusted_time_state` and the view.

### Five findings from this work, none of them in the plan

* **`0002:103-106` is a stale forward reference that reads as an instruction.** It says "0005 adds
  `tender_state`/`captured_at`" to `sale_tender`. **`0003:322-324` superseded it** with append-only
  `tender_status_event`, and §0005 contains **zero** `ALTER` on `sale_tender`. A reader following
  0002 adds columns that must not exist.
* **The live completed-sale guard set is the post-0003 one.** `0003:320` replaced 0002's
  `sale_tender_amount_frozen_once_completed` with `sale_tender_no_update_once_completed`, and 0003
  defines the `sale_line_no_*` triggers **twice** — the `913-933` versions are live. A test naming
  0002's set fails; a migration restoring 0002's wording reopens a hole 0003 closed, which
  `0003:907-912` records as having happened once already.
* **`sale` has no foreign keys at all** — `pragma_foreign_key_list('sale')` is empty. `0005` repairs
  `register_id` and `ref_sale_id` with **triggers**, because `ALTER TABLE` cannot retrofit a
  `REFERENCES` clause. That is the documented trade at `ref/schema.md:1798-1817`, not an oversight.
* **No `PRAGMA` may appear in `0005`.** The runner wraps each migration in one transaction, where
  `PRAGMA foreign_keys` is a no-op and `PRAGMA defer_foreign_keys` cannot clear a deferred violation.
* **`verify-schema.py:405` skips any heading containing `SHIPPED`**, and pass 1 has already applied
  the file from disk. The `· SHIPPED` marker is therefore required in the **same** commit as the
  migration — without it pass 2 re-executes the DDL against the schema it just built. A scoping
  agent advised the opposite; the script settles it.

---

## 3 · The eight open issues

All eight are on board #4, all `Todo`, all assigned — and **every one is blocked on a human**:
one on `hardware`, five on a `decision`, two on a `merchant answer`. Nothing on the board reads
`not blocked` any more. The two that did, #119 and #120, both shipped on 13 September, which is why
the remaining list is entirely external: **there is no issue here that code can close.**

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

**#69 does not gate a Phase-1 microstep.** Its own body says it blocks *"all of group 2.7 and the
22 ⚠️ OPEN items microstep 2.7.0 owns"*, and `phase-1:1055` says the opposite of gating: *"Owner:
2.7.0 ratifies 6.9 via #69, on a timeline outside this project's control, **so this microstep cannot
wait for it**."* It is a long lead ordered in Phase 1, not a Phase-1 blocker.

**Two divergences nothing reconciles.** Issues **#68, #69, #70 and #71 carry no `priority:` and no
`risk:` label at all**, while the board shows all four as P1, Phase `1 sellable MVP`, and a Risk —
substantive for three of them (#69 `compliance`, #70 `money path`, #71 `compliance`) and the
explicit `none` option for #68. And **#113 carries
two risk labels** (`migration` *and* `compliance`) while the board's single-select Risk field holds
only `migration` — the field is structurally incapable of holding both, and the loss is silent.
Decide which surface is authoritative and make them agree, or stop reading one of them.

### Four ⚠️ OPEN items gate Phase-1 microsteps and **no issue tracks them**

Two more than this document carried until 13 September. The sweep that found them is
`grep -rn '⚠️ \*\*OPEN' docs/implementation/ref/*.md docs/implementation/phase-1-sellable-mvp.md`,
filtered to those naming a `1.x` microstep — run that, not a keyword search, because the two new
ones share no vocabulary with the two old ones.

* `ref/plan-validation.md:324` — **blocks `1.8.1`**: the SQLCipher/SQLite WAL-reset corruption
  question and the two minimum version constants `1.8.0` must pin and hash.
* `ref/security-compliance.md:413` — **blocks `1.6.2`**: what second factor exists on a Jordanian
  minimarket counter. **Buying #68's hardware does not unblock `1.6.2`** — it is blocked twice.
* `ref/domain-api.md:1290` — **blocks `1.3.5`**: for a line carrying both General Sales Tax and
  Special Sales Tax, each component's base, the order they apply in, and whether the fixed part is
  per unit or per line.
* `ref/domain-api.md:1299` — **blocks `1.3.2`**: which `ZeroRatingReason` values the filing return
  distinguishes.

**Neither new one is covered by #70.** That issue batches four named questions — the tie rule, cash
rounding, the approved `tax_computation_policy` row, and SST category evidence — and blocks `1.3.4`
and `1.3.7`. `1.3.2` and `1.3.5` are not among them, so group 1.3 is blocked in four places by two
different authorities.

A search of every issue, open and closed, for WAL / SQLCipher / storage / second factor / zero-rating
/ SST base returns nothing. File them, or record deliberately that they live only in the reference
documents.

### #115's remainder is still partly open

The four ruleset payloads exist under `.github/rulesets/` and agreed with live when last compared
by hand. What is still open is **enforcement**: no gate runs the diff — so every statement that they
"byte-match live" is a dated observation and decays the moment someone edits a ruleset in the UI — the payloads have never been
round-tripped (no restore has been executed), and `scripts/gh-protect.sh` still refuses at **exit
3**. It refuses correctly — its legacy-API `PUT` would now apply three verified defects (a required
list omitting `guards`, `supply-chain` and `protected-paths`; `require_code_owner_reviews: true`
against a sole-developer CODEOWNERS with 0 required approvals plus `enforce_admins` on main; and the
legacy API cannot express `allowed_merge_methods` at all). **No issue tracks this remainder.**

---

## 4 · What is next — the WIP=1 slot is TAKEN by `1.9.1`

**Finish §2b before starting anything here.** `1.9.1` is in flight, and WIP = 1 means one microstep,
not one open pull request. The candidates below are what comes *after* it, and they are recorded now
so the choice is not re-derived later.

Per `03-github-workflow.md` §4 the loop is: pick **one** microstep, open **one** `Microstep` issue,
add it to board #4 by hand, set it `In Progress`, then build it.
`.github/ISSUE_TEMPLATE/01-microstep.yml` has **eight required fields**, including a proving command
(*"The command that proves it. Not a description of the command."*) and a *Test-catalog rows closed*
field — so a microstep with no `Done when` line cannot even be filed without authoring one first.

**That loop is one microstep old.** #162, for `1.11.3`, is the first Microstep issue this repository
has ever carried; the twenty before it were built without one. The law was right and unfollowed, so
treat §4's procedure as a new habit rather than an established one — and note that `1.9.1` has not
been filed yet either, which §2b lists as its first item of unfinished work.

### IN FLIGHT, not recommended: `1.9.1` — migration `0005`

**This work has started — see §2b for its state and how to resume it.** The rest of this subsection
is the case for it, kept because the obligations it lists are still owed.

**#113 is decided.** On 13 September it took **option 4 — freeze `store`, knowingly** — and its own
words are *"`1.9.1` is unblocked and may write `0005` transcribing the specified DDL unchanged."*
That makes `0005` the highest-leverage buildable microstep in the repository: `verify-schema.py:259`
requires migration numbers contiguous from `0001`, so `0005` is the gate standing in front of
`0006`, `0007`, `1.2.3`, `1.9.2`–`1.9.5`, `1.10.2`–`1.10.5`, the `1.1.9` DB half and the `1.2.4` DB
half.

**Unblocked is not small.** `ref/schema.md` §0005 runs `:1671` to `:2838` — **1,168 lines** of
specification, the largest single migration in Phase 1. Budget accordingly.

Five things that PR owes, and the fifth is new:

1. **`schema.md`'s `## 0005` heading must gain `· SHIPPED`** in the same commit.
   `verify-schema.py:405` skips re-executing a section whose heading contains `SHIPPED`; without it
   the second pass re-runs the DDL against the schema it just built. `0002`, `0003` and `0004` carry
   the marker. `0005` does not.
2. **The Postgres mirror**, with its 14-digit UTC name and its declaration header.
3. **The `lib.rs` `MIGRATIONS` registration** — `SCHEMA_VERSION` is `MIGRATIONS.len()`, so shipping
   `0005` moves `user_version` to 5.
4. **`tests/common/mod.rs`'s `reference_blocks_at_or_after(5)` → `6`**, and the test file.
5. **Three ⚠️ OPEN blocks still say the ICV choice is unmade.** `phase-1:1055`,
   `ref/schema.md:2651` and `ref/schema.md:4250` each enumerate #113's four options or say `1.9.1`
   "must choose deliberately first". Landing `0005` without rewriting all three leaves the plan of
   record contradicting a decision that has been taken — the same defect class as §2a's twelfth site.

**Two things the decision did NOT do**, and both must survive contact with whoever writes `0005`:
`ref/merchant-decisions.md` row 6.9's Answer cell **stays empty** — option 4 is a hedge, and only
the official ISTD package can answer it — and **#113 stays open**. What is settled is what `0005`
does, not what the ICV namespace is.

**The reversal window is now a standing obligation on `2.7.4`.** ICV is never allocated at checkout;
allocation begins there, in Phase 2. Until the first ICV row exists, correcting a wrong `store`
guess is one forward-only migration. After it, it is a migration plus a data repair on a sequence
required to be gapless.

The decision also recorded a blast-radius correction nobody had written down: **the ICV question
binds Phase 3 too.** `ref/test-catalog.md:96` row 87 names
`two_offline_registers_never_allocate_the_same_icv`, owned by `phase-3-connected.md:108`. And the
store-scoped default is written into **five** documents — `ref/fiscal-jofotara.md:102`,
`ref/plan-validation.md:274`, `phase-2-money-grade.md:471`, `ref/test-catalog.md:100` and
`ref/schema.md:4250`. A later correction sweeps all five.

### If `0005` is too big for the sitting — three genuinely small ones

| Candidate | State |
|---|---|
| `1.2.6` — assert FTS5 at open | **The smallest DB step available, and structurally unblocked.** One file (`crates/pos-db/src/lib.rs`), one test (`open_asserts_fts5_available`, `phase-1:330`), and it asserts over `pragma_compile_options` — no table, so no migration. Group 1.2's build-order note (`phase-1:196`) puts it after `0007`, but neither reason that note gives touches this step. Its `Done when` (`:331`) states an **outcome, not a command**, so the issue's `Verify` field must be authored |
| `1.11.6` — global scan capture | **The closest repeat of `1.11.3`.** Two new files under `apps/terminal/src/lib/`, two named tests (`scan_routes_while_search_focused`, `scan_burst_detected_over_typing`), and `1.11.0`'s fake-timer bridge exists precisely for the burst timing. Named as still owed by `ref/test-catalog.md:314` and `02-development-workflow.md:1710` |
| `1.11.11` — keyboard reachability | Unblocked by the same harness; one named test, `every_action_reachable_without_a_mouse`. Its `Done when` (`:1302`) is already a command |

### The other candidates, with what is actually true of each

| Candidate | State |
|---|---|
| `1.2.4` **pure half** | **The largest unblocked domain step** — the gateway to group 1.4, since `CartLine` needs `PriceOrigin` and `DerivedWeight` and neither name appears anywhere under `crates/`. Big: 14 named tests, a new module, a trybuild pair. Its full `Done when` (`phase-1:302`) chains commands unreachable before `0007`, so the issue's proving command must be rewritten for the half. **Two of the 14 tests are blocked by #71**, whose body says it blocks "`1.2.4`'s database commissioning half". And it defers half of itself with **no `Full-step status:` marker** — unlike `1.1.9` and `1.2.0` — so nothing mechanically stops a premature "complete" claim |
| `1.6.6` — `AuditRepository` | **A dark horse nobody named.** Fully unblocked: `0004` shipped `audit_log`, and `1.6.5` and `1.8.9` landed. **No `Done when` line** — one must be authored first |
| `1.11.1` — i18n | **Blocked by `1.7.2`.** `assets/fonts/` does not exist, no font file is tracked anywhere, `pos-hardware/src` is only `lib.rs` |
| `1.11.4` — Lock / PIN | **Soft-blocked.** All four IPC commands it drives are absent — `src-tauri/src` has no `commands/`, and `src/lib/ipc.ts` does not exist. It would be tested entirely against invented mocks |
| `1.11.5` — Sale screen | **Blocked.** `CartSnapshot` does not exist (`packages/api-types/src/index.ts` is `export {};`), and it would rewrite the green `1.11.0` canary |
| `1.3.3` — `compute_line_tax` exclusive | Technically buildable, but **no `Done when` line**; document order puts the externally-blocked `1.3.2` first; both edit the same file |
| `1.3.2`, `1.3.5` | **Blocked, and by nothing anyone filed** — `ref/domain-api.md:1299` and `:1290`. See §3 |
| `1.6.2` — Argon2id PINs | **Blocked twice**, neither time by code: `just bench-gate pin-verify` refuses until #68, **and** `ref/security-compliance.md:413` |
| `1.2.3` | Blocked three migrations deep — its FTS repository needs `0007`'s tables |

**Twenty executable Phase-1 microsteps carry no `**Done when:**` line at all** — `1.1.9` `1.2.0`
`1.3.2` `1.3.3` `1.4.1` `1.4.2` `1.4.3` `1.4.4` `1.4.5` `1.4.7` `1.4.8` `1.4.10` `1.5.1` `1.5.2`
`1.5.4` `1.6.6` `1.7.1` `1.7.4` `1.7.6` `1.7.8`. The first two are a different case: `1.1.9` and
`1.2.0` carry a `**Current half done when:**` line, which is not the literal string the checker
matches, plus a `**Full-step status:**` marker — so rule 3 already refuses them and rule 4 never
gets the chance. For the other eighteen, checker rule 4 refuses a completion claim until one is
written, and the issue form will not accept the microstep without a proving command. **Each is a
documentation prerequisite to its own delivery.**

### One question this window answered, so nobody re-opens it

**Adding unnamed vitest tests does not red `check-test-catalog.py`.** Measured, not reasoned: #165
added thirteen vitest tests while `ref/test-catalog.md` names only three of them, touched no
catalogue row, and the gate still reconciles at 92 cases with `just pre-push` exiting 0. The
reconciliation runs catalogue → runner, never the reverse.

---

## 5 · Rulings carried forward — read before touching group 1.11

### DISCHARGED: `1.11.12` owns `latin_runs_inside_arabic_text_are_bidi_isolated`

**Done on 13 September, in `1.11.3`'s own PR (#165), as this ruling required.** The name was struck
from `1.11.3`'s Tests line and the matching clause from its `Done when`;
`grep -c latin_runs_inside_arabic_text_are_bidi_isolated docs/implementation/phase-1-sellable-mvp.md`
now returns **1**, at `1.11.12`'s Tests line. Kept here because the *reasoning* generalises and the
duplicate class can recur:

Three primary sources decided it — `ref/test-catalog.md:314` files the name under the DOM harness,
`02-development-workflow.md:1709` attributes it to 1.11.12, and `ref/ui-spec.md:292` places bidi
isolation on the DOM-component rung (as a *description*, not the identifier — it is invisible to a
name-grep, which is why two sweeps missed it). The **arithmetic decided the direction**: `1.11.12`'s
`Done when` says "all **five** named rendered-state tests" and its Tests line names exactly five, so
striking it there breaks a counted condition, whereas `1.11.3`'s carried it as prose that trimmed
cleanly.

**Do not read a gate's silence as permission.** `check-test-catalog.py` exited 0 the whole time the
duplicate stood, because the harness table's header is `| Harness | Unblocks |` while the collector
requires `header[0] in {"Test","Property"}`. Nothing would ever have caught it.

### The safe-rewrite rule for `ref/test-catalog.md`'s harness row

`check-test-catalog.py` scans prose in **table cells** for two patterns, either of which makes the
names in the `Unblocks` cell compete with their owning `Tests:` lines and trips assertion 7:
`REFERENCE_TEST_MENTION` (`:1079-1081`) — a capital `Tests?` **or `Properties`** immediately before
a backticked identifier, whitespace the only thing allowed to intervene — and
`REFERENCE_TEST_COVER` (`:1082-1084`). Both still compile at exactly those lines.
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

* **`scripts/check-logical-css.sh` scans `.ts`, `.tsx`, `.css` and `.html`** under `apps/` and
  `packages/` (`:82`, roots at `:153`) with no test-file exemption, and its bare-side regex
  (**`:43`** — `(^|[^-[:alnum:]])(left|right)[[:space:]]*:`) refuses a bare `right:` or `left:`. **The canonical DOM-test rect
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
* **`biome.json` cannot be narrowed to exclude tests.** `check-branch-workflow-policy.rb:830-833`
  refuses any `files.includes` negation outside the four exclusions frozen at `:106-111`
  (`!**/dist`, `!**/src-tauri`, `!**/node_modules`, `!**/public/**/*.svg`), and `:825-827`
  requires `apps/**` and `packages/**` to stay covered.
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
  `check-branch-workflow-policy.rb:793-799` — the reviewed literal at `:793-796`, asserted by exact
  equality at `:797-799`. A dependency with an
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

Measured live 13 September. Keep this current; it saves an hour every session.

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

**39 evaluations since the rulesets were created: 26 bypass, 12 pass, 1 fail.** 25 bypasses on
`development`, 1 on `staging`. The endpoint defaults to `time_period=day`, so an unqualified call
returns only today's — pass `time_period=month` or you will conclude the opposite.

**13 September alone contributed three of those 26 bypasses**, and all three for the same
mechanical reason rather than a judgement: a rebase to clear `BEHIND` mints a new SHA with no check
results, so merging straight after reads as *"6 of 6 required status checks are in progress"*. Every
one of those branches had been green before its rebase. See §2a trap A — this is the bypass shape
most likely to recur, and the one the ledger cannot distinguish from a real override.

* **The current `development` HEAD was a bypass.** Rule suite `4053292420`: PR #166,
  `result: bypass`, `required_status_checks` fail — *"6 of 6 required status checks have not
  succeeded."* It merged **36 seconds** after #165, and under `strict: true` a rebased branch
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
waits; the fast back-to-back merges did not. Recording the ledger as a recurring check is still an
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
stray). **Six milestones**; Phase 1 reads **open 7 / closed 30**, and `closed_issues` counts PRs — 26 of the 30 are pull requests.
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

Project id `PVT_kwHOCn5KRs4BhoZ-`, user `OmarSweiti`. Private board, public repo. It is the only
**open** project — a **closed** project #3, `pos`, also exists. `gh project list --owner OmarSweiti`
hides it and `--closed` shows it, which is how the earlier "only project that exists" claim survived.

| Field | Field id | Options (id) |
|---|---|---|
| `Status` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjb_I` | Todo `f75ad846` · In Progress `47fc9ee4` · Done `98236657` |
| `Phase` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjqPw` | 0 `e28be675` · **1 `ecb2fe9c`** · 2 `a09a7d4b` · 3 `5d19eeed` · 4 `3f2f8542` · 5 `1d54d952` |
| `Group` | `PVTF_lAHOCn5KRs4BhoZ-zhgjqSA` | text |
| `Microstep` | `PVTF_lAHOCn5KRs4BhoZ-zhgjqWA` | text |
| `Priority` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjqX0` | P0 `952302e9` · P1 `00584313` · P2 `98f4223d` |
| `Risk` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjqX4` | money path `11636174` · migration `53da4eea` · security `3b34bad4` · compliance `df4f887e` · immutable `4800cc22` · none `ace6a93f` |
| `Blocked` | `PVTSSF_lAHOCn5KRs4BhoZ-zhgjqX8` | merchant answer `6420eac0` · decision `617fa9da` · hardware `c7becb76` · not blocked `1110b92f` |
| `Target` | `PVTF_lAHOCn5KRs4BhoZ-zhgjqZw` | date — **unset on all eleven, deliberately** |

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
| **A recurring bypass-ledger check** | **not done.** 26 of 39 evaluations were bypasses, and nothing in `scripts/`, `.github/workflows/` or the justfile reads the rule-suites endpoint |
| **Selected-Action allowlisting** | **not done** — `allowed_actions: "all"` |
| **Issues exception-only** | **not done — and it is not a wording fix.** #166 corrected `01-microstep.yml:2` and `CONTRIBUTING.md:81`, which both claimed issues were "the normal way work enters this repo". But `03-github-workflow.md` §4 still *requires* an issue for "the microstep you are starting now (one at a time — WIP = 1)", so making issues exception-only means changing §4 — a policy decision, not a template edit |
| **Claude read-only permissions** | **not done** — #114. `.claude/settings.local.json` is `{}` |
| **Matrix on every PR** | **not done.** Removing `cross-platform`'s promotion-only `if:` — do it **last** |
| **The `staging → main` promotion** | **not done, and it needs a person.** See §9 — it is now the single highest-value follow-up in the repository |

---

## 9 · Promotion and the release path

### `development → staging`

`staging` is **16 behind**, and the gap now carries a real microstep (`1.11.3`, #165) and a code
fix (#164) rather than documentation alone. Open a promotion when you want the cross-platform matrix
over the current tip:

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
`main` is a strict ancestor of `staging`, **133 commits behind**, its tip still `24a0283` of
20 August 2026. The gap is **232 files,
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

### Taken 13 September

41. **#113 takes option 4: `0005` freezes `store`, knowingly.** A **hedge, not an answer** —
    `ref/merchant-decisions.md` row 6.9's Answer cell stays **empty** and #113 stays **open**, because
    only the official ISTD package can answer what the ICV namespace is. Two of the four options were
    rejected on evidence rather than taste: option 2's *"costs nothing structurally"* is false against
    the DDL (`ref/schema.md:2686-2689`'s paired `CHECK` makes a widened `scope_kind` unreachable, the
    existence triggers hard-code `register` and `store`, no `income_source`/`credential`/`tin` table
    exists to join to, and the Postgres mirror uses typed columns), and option 3 would have deferred
    **register-scoped receipt numbering** (`1.9.3`) along with the ICV, because `doc_sequence` is one
    table serving both. The reversal window closes at `2.7.4`.
42. **`latin_runs_inside_arabic_text_are_bidi_isolated` is `1.11.12`'s, and the duplicate is gone.**
    Settled by three primary sources and by arithmetic — see §5, kept there because the reasoning
    generalises.
43. **The Microstep-form fix was deliberately narrowed, and "issues exception-only" was NOT done.**
    #166 corrected two documents that called an issue "the normal way work enters this repo".
    Making issues genuinely exception-only is a different change, because `03-github-workflow.md`
    §4 *requires* an issue for the microstep in flight — so it means editing §4, which is a policy
    decision about how work is tracked rather than a wording fix. Do not conflate them again.
44. **A rebase to clear `BEHIND` discards the green checks, and merging straight after is a bypass.**
    Not a judgement call to re-argue — it is mechanical, it happened three times on 13 September, and
    the fix is to wait for the post-rebase run or to cut each branch from the previous one.

### Taken 14 September

45. **`ref/schema.md`'s `## NNNN` heading gains `· SHIPPED` in the SAME commit as the migration.**
    Settled against the script, not opinion: `verify-schema.py:405` skips any heading containing
    `SHIPPED`, and pass 1 has already applied the file from disk. Without the marker, pass 2
    re-executes the section's DDL against the schema it just built and the verifier reds. A scoping
    agent advised deferring the marker to a separate documentation edit; it was wrong.
46. **An unmerged migration is moved back out of `HEAD` to correct it, not patched by a successor.**
    `is_committed` in `protect-immutable.py` tests membership of `git ls-tree HEAD`, so committing a
    migration on a feature branch makes the agent guard and the Git hooks refuse to edit it. On a
    branch that has not shipped, not merged, and not touched a protected branch, `git reset HEAD~1`
    is the intended escape. The forward-only law protects released history; it is not a reason to
    make a typo in an unmerged file permanent.
47. **The 17 tests `0005` breaks are repaired by ONE shared registered-chain fixture, not 17 local
    patches.** They fail because `0005` adds seven gates to sale completion and every pre-`0005`
    test completes a minimal sale — correct behaviour, not a defect. One `complete_sale` helper that
    passes all seven gates is one recipe in one place; seventeen local worlds would drift apart and
    the next migration would break them all again.

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
corrupted data, compliance breach"*. #114, #119 and #120 are P2 — #119 and #120 closed on
13 September and still carry the label. **#68–#71 carry no priority or risk
label on the issue at all** — see §3.

---

## 12 · Traps that will bite you

| Trap | What to do |
|---|---|
| The shell's Node fails the fail-closed `.nvmrc` pin | Prefix every recipe with `mise exec --`. Never edit `.nvmrc`. Fix a moved pin with `nvm use` first — `just setup` refuses before installing |
| **`mise exec --` silently returns the HOST Node when the cwd is outside the repository** | It resolves the version from the current directory, and there is no `.mise.toml` here — only `.nvmrc`. From `/private/tmp` it prints `v26.4.0` and **exits 0**, which reads as a green run on the wrong toolchain. Always run it from the repository root. A `cd` *into* the root is harmless; the old claim that `cd` breaks `mise exec` was false. The one genuine failure is a command built in a shell variable, which becomes `argv[0]` unsplit |
| **Commit subjects are capped at 72 characters before the step tag** | `bash scripts/validate-change-title.sh --validate '<title>  [—]'`. Note the **`--validate`**: mode selection is explicit, so a bare invocation prints its usage to stderr and exits **2** rather than validating anything |
| The step grammar **admits a letter** | `[1.1.2a]` is valid and 13 Phase-1 microsteps require it |
| **`just branch` refuses `feat/…`** | Microstep work uses `phase-<0-5>/group-<m>-<kebab-slug>`; also allowed are `fix/`, `docs/`, `refactor/`, `perf/`, `test/`, `chore/`, `hotfix/`. `just pr` derives the milestone from the `phase-<0-5>/` prefix |
| `just pr`'s `$body` is a **file path**, not text | `just pr 'title' path/to/body.md` |
| `just branch` runs `git pull --ff-only` internally (`justfile:408`) | Under `set -euo pipefail` a network stall aborts the recipe before `git switch -c` (`:409`), leaving you on `development` — and if you piped the invocation through `\| tail -1` you will not see why. Check `git rev-parse --abbrev-ref HEAD` after |
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

## 13 · Workflows — what ten runs have taught

Ten audit workflows have now been launched across six sessions. A tenth, on 14 September, scoped
migration `0005` before a line of it was written — six dimensions, and its adversarial half refuted
**32** of the scopes' own claims, including a blast-radius estimate of four failing tests where the
measured number is **17**. That is the pattern to keep: the scope is a hypothesis, the command is
the evidence. The two of 13 September scoped
the five-item work queue before any of it was written, and then audited this document against live
state — 47 refutations in the first, 22 in the second, of the *auditors* rather than the tree. **The 11 September run finished**:
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

- **CLOSED by #163 — `status-page.html` is still the document no gate reads.** The stale
  *"`main` is deliberately unprotected"* sentence is fixed, and the sweep that fixed it turned up a
  **second** error on the same page that nobody had ever looked for: *"The two source plans under
  `docs/plan/`"*, where `ls docs/plan/` returns three. The structural cause is unchanged and will
  produce the next one: `check-doc-links.py` filters on `.md`, and
  `check-implementation-frontier.py` reconciles only this page's per-phase **step counts**, never its
  prose. **Nothing reads it but a deliberate sweep**, so schedule one rather than expecting a gate.
- **`PROJECT-GUIDE.md`** — untracked, 123,758 bytes, 2,265 lines, self-dated to `6d997f2`
  (28 August). **84 commits separate that from HEAD.** Of 17 factual claims spot-checked, **11 are
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
  **fifteen feature branches with `[gone]` upstreams** from the 9–13 September work, a
  `refs/original/refs/heads/main` filter-branch backup at `a7c2379`, and seven
  `refs/codex/turn-diffs/checkpoints/*` refs. `git branch -d` refuses them because squash merges
  break ancestry; `git branch -D` is safe for all fifteen — each is merged content on
  `development`.
- **Your local `staging` is 51 commits behind `origin/staging` and 63 behind `development`** — it
  still sits at #91's promotion merge (`f2edbb6`), four promotions behind (#106, #108, #130, #148).
  `just promote-staging` without fetching first works from the wrong base.
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
- **The Dependabot squash-body half is verified in history**, not still open. Three real Dependabot
  squashes landed on 10 September — `c7580cc` (#147), `371485d` (#151) and `7fc2deb` (#152) — each
  carrying exactly `Co-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>`
  with matching author metadata. The gate that sees a squash body is `ci.yml:361-380`, the
  *"Checked-out reachable history contains no assistant-attribution trailers"* step in the
  `supply-chain` job. The human path was verified at #109.
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
   at `pre-push:187` fired first and five refusal branches never executed — a miscitation in this
   document from the start, not drift; `:115` was never that refusal. That is the least
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
9. **A control that fires on every merge is not a control; it is a habit.** 26 of 39 ruleset
   evaluations were administrator bypasses. Most were the byte-frozen-surface review working as
   designed — but "most" is the word that makes a ledger necessary, because nothing distinguishes
   the designed reds from the impatient ones except reading them.
10. **The document nobody's gate can read is the document that goes stale.** Four documents were
    corrected on 11 September; the one sentence that survived lived two more days in the HTML file
    no checker parses, until a deliberate prose sweep caught it at #163 — along with a second error
    in the same page that no gate had ever read. The sweep, not a gate, is still the only thing that
    reads that file.
