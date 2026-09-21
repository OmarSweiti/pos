# Handoff — the single current one

**Reflects `development` @ `8f05458`, 21 September 2026.**

There is one handoff — keep updating this file rather than adding a dated one.

> ## ✅ `1.6.6b` LANDED — THE CHAIN NOW HAS A COMMAND THAT CAN PROVE IT, AND THE SAME REVIEW SHAPE PAID AGAIN
>
> `1.6.6b` (#208, issue #207) takes Phase 1 to **30 of 112 (~27%)** — `verify-audit`, the forensic
> command the Phase-1 exit gate has been promising since `1.6.5`. §2k is the record.
>
> **The lesson `1.6.6` taught repeated itself, exactly.** A 23-mutation sweep caught 22 and left
> one survivor already written into the code as untestable. It looked finished. An adversarial
> read then found a defect **no mutation could reach, because it was not a missing guard — it was
> two correct halves producing a wrong answer between them**: a read that stops at seq 3 hands
> `verify_chain` the rows below it, and an anchor at seq 5 makes it answer `Truncated` over rows
> that are *still on the disk*. The tool would have accused a merchant of deleting rows because it
> could not read one of them itself. A sweep asks "is this guard load-bearing"; it cannot ask "do
> these two right answers compose".
>
> **And CodeQL earned its keep for the second microstep running.** `rust/cleartext-logging` on the
> line that echoed the anchor's register led to the real defect beside it: `source_kind` and
> `anchored_at` were printed byte for byte out of a file supplied by whoever is being investigated.
> One newline inside `source_kind` forges a register block, heading and `INTACT` verdict included,
> into the middle of a document read as evidence. Nothing raw from the anchor reaches the report
> now, and **the alert cleared on the fix** — no dismissal.
>
> **`protected-paths` was red on #208, by design, and the merge is on the ledger.** Retiring a
> `PLANNED` entry means editing `scripts/check-test-catalog.py`, which is inside the frozen policy
> surface; rule suite **`4158403946`** records the bypass naming exactly one failing check. §2k and
> §9 have the detail, and §4 records that **every future microstep that lands a catalogued test
> takes this same red**.
>
> **Nothing is in flight.** The WIP=1 slot is free and §4 names what is left.
>
> ## ✅ `1.6.6` LANDED, AND AN ADVERSARIAL REVIEW CAUGHT THE DESIGN DEFECT THE MUTATION SWEEP COULD NOT
>
> `1.6.6` (#204, issue #202) takes Phase 1 to **29 of 112 (~26%)** — the audit repository, so the
> hash chain `1.6.5` shipped now has something that writes it and something that reads it back.
> §2j is the record.
>
> **The thing worth carrying is the order the two checks fired in.** An 18-mutation hand sweep
> caught 16 and left two survivors, both of which the code had already written down as untestable
> before the sweep ran. It looked finished. An adversarial review then found a defect **no
> mutation of the shipped code could have caught, because it was not a missing guard — it was the
> wrong shape**: `chain()` returned `Err` on any row it could not rebuild and discarded every
> honest row with it. `audit_log_no_update` and `audit_log_no_delete` guard mutation, and
> **nothing guards `INSERT`** — so one added row silenced verification for the whole register,
> permanently, and neither trigger would then let anyone remove it. Cheaper than the re-chaining
> attack the chain exists to detect, and more effective. A mutation sweep asks "is this guard
> load-bearing"; it cannot ask "is this the right guard".
>
> **`1.6.6` carries a correction to the previous four microsteps' habit, not a repeat of it.**
> §2j also records the two survivors, the golden that failed on its own fixture, and the one
> mutation that killed no test at all until a test was written for it.

**14 September moved three microsteps, and each unlocked the next.** #170 merged **`1.9.1`**: 1,179
lines of migration `0005`, the shared registered-chain fixture, seven tests, the Postgres mirror,
and four documentation sites where the ICV decision still read as an open question. That shipped
`trusted_time_state`, the only thing blocking **`1.1.9`**'s deferred database half — #173 merged it
the same afternoon and **closed group 1.1**. #177 then took **`1.2.6`**, the FTS5 assertion, and
#178 redacted a canonical payload out of two `Debug` impls.

**15 September audited all of it, found four defects, and then closed the largest thing the audit
could not fix where it stood.** That window left Phase 1 at **24 of 112 (~21%)** — #185 is
guard-hardening on shipped code, not a microstep. **19 September moved it to 25 (~22%)** with
`1.11.6`, the first UI microstep since 13 September; §2f is that record. §2b,
§2c and §2d are the 14–15 September windows; §2a keeps 13 September and §2 the 9–11 September
record, where twenty-six pull requests changed the governance layer and no microstep advanced.

**`development` is green, tip included.** `just pre-push` exits 0 at `8f05458`, all 37
`just guards` steps pass, and `ci` run **`35602921581` is a success on the tip**, queried by SHA
rather than taken as the newest green one — `ci.yml`'s ref-scoped concurrency group cancels runs
when merges land inside two minutes of each other, and the cancellation is invisible unless you ask
about the tip specifically.

**One red in this window was nobody's diff.** #180 failed `supply-chain` on
[GHSA-2mjx-qc3c-rqvc](https://github.com/rustls/rustls/security/advisories/GHSA-2mjx-qc3c-rqvc),
which RUSTSEC published after `development`'s previous green run — so the first pull request to run
CI afterwards inherited it and looked like its cause. **Verify before believing that:**
`cargo deny check advisories` failed on an untouched checkout of `development` too. #181 bumped
`rustls` 0.23.43 → 0.23.45; it reaches the graph only through `sqlx-core` → `pos-server`, so no
register code was involved. This is the class `CLAUDE.md` predicts and deliberately keeps out of the
local gate.

**The WIP=1 slot is EMPTY.** 0 open pull requests and nothing in flight. Board #4 reads **23
items — ten `Todo`, thirteen `Done`**, counted live; no item is `In Progress`. Pick one microstep from §4, file its
Microstep issue, put it on the board, set it `In Progress`, then build it — a loop now four
microsteps old and followed every time since #162.

**There are ELEVEN open issues, not ten, and #203 is the one every previous handoff missed.**
It was filed automatically at 09:08 UTC on 21 September by the weekly `security` workflow's own
escalation job, carries **no labels and is not on board #4**, and is a real red: §3 diagnoses it.

**#174 is the one open issue code alone can close.** It came out of reviewing this window's own
work and needs an answer from nobody. **#179 was the other, and it is closed** — by hand, on
15 September, after #185 landed its item 3. Its remaining three items were not fixed by that and
are not fixed now; they are untracked rather than done. §3 says which, and re-measures each against
the code.

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
mise exec -- just pre-push          # passes at 8f05458
```

Nothing is in flight, so there is no branch to resume. `phase-1/group-6-audit-verifier` merged as
#208 and `gh pr merge --delete-branch` already removed it on both sides.

**Two reds are open and neither is a diff of yours.** `protected-paths` on any PR that edits
`scripts/check-test-catalog.py` is by design (§2k), and **the weekly `security` workflow is red
with issue #203 tracking it** — `dtolnay/rust-toolchain` moved its mutable `v1` tag, so zizmor now
calls six of our correct SHA pins stale-commented. §3 has the diagnosis. Neither blocks a
microstep; both will be waiting for you.

**`just setup` without `mise exec --` fails.** The shell's Node is `v26.4.0`; `.nvmrc` pins
`24.19.0` exactly and the check is fail-closed. This is the first thing that goes wrong every time.

**One operational gotcha, and the last two handoffs both diagnosed it wrongly:** the failure is a
command built in a **shell variable**, nothing else. `CMD="node --version"; mise exec -- $CMD` fails
with `couldn't exec process: No such file or directory`, because the whole unsplit string becomes
`argv[0]`. Pass the words literally. **A `cd` earlier in the same Bash invocation is harmless** —
`cd <repo> && mise exec -- node --version` prints `v24.19.0` — and a relative script path works.
Re-measured 13 September; the `cd` clause that stood here was false.

### Verified gate baselines at `8f05458`

Use these as the "nothing is broken" reference. **One row moved when `1.6.6b` landed**: the test
count, 298 → **324**, all twenty-six of them `1.6.6b`'s and all Rust. The schema chain did not
move, because `1.6.6b` needed no migration — and neither did `Cargo.lock`, because the verifier
parses its two flags by hand rather than taking an argument-parser dependency.

**Every row below was re-measured on the merged tip `8f05458`, and for once none is carried
forward.** `just pre-push`'s five, plus `verify-schema`, `verify-pg` — **real engine pass** through
the Docker fallback — `just audit`, and `bench-gate`, which was run rather than assumed and
refused with exit 3 as it should. `just audit` still reports **135 package releases and 11
reviewed expressions**, which is the expected answer *and* the measured one: `1.6.6b` took no
dependency at all, so `Cargo.lock` is byte-identical and no licence or advisory surface moved.
**Re-run the one you are about to depend on anyway** — these are dated observations.

| Command | Reads |
|---|---|
| `just pre-push` | **exit 0** — `lint test build-web guards secrets`, `justfile:368`, ~1:30 warm |
| `just check` | exit 0 — all **seven** workspace members |
| `just lint` | exit 0 — 2 prerequisites + 14 body steps = **16 checkers** |
| `just test` | exit 0 — **324 tests run: 324 passed, 2 skipped**; JS **8 files / 71 tests** |
| `just build-web` | exit 0 — `tsc -b` + vite 8.2.2 across 5 packages |
| `just guards` | exit 0 — **37 steps** |
| `just verify-schema` | exit 0 — **5 migrations, 48 tables, 457 columns** |
| `just verify-pg` | exit 0 — **the engine pass RAN** via the Docker fallback |
| `just secrets` | exit 0 — gitleaks 8.30.1, `--history` |
| `just audit` | exit 0 — cargo-deny clean; **135 package releases, 11 reviewed expressions** |
| `just bench-gate` | **REFUSED, exit 3** — no reference register. Correct, not broken |
| `pnpm --filter terminal exec vitest run` | **6 files, 58 tests**, vitest **5.0.0** |
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

**The three per-package vitest rows must sum to the `just test` row.** They do, re-measured
package by package on 21 September: **6 + 1 + 1 = 8 files, 58 + 3 + 10 = 71 tests**.

**This paragraph existed to catch exactly the drift it had itself acquired, so the failure is
worth naming rather than quietly overwriting.** It stood at "4 + 1 + 1 = 6 files, 31 + 3 + 10 =
44 tests" while the table above it said 71, and it asserted "no JavaScript test has been added
since 13 September" and that "`1.11.6` moves them again" — after `1.11.6` and `1.11.11` had both
landed and taken the terminal from 4 files / 31 tests to 6 / 58. Three handoff generations copied
it forward. **No gate reads this file**, which is the whole reason the arithmetic is written out:
`scripts/check-test-catalog.py` reconciles the catalogue against the runner and has never looked
at `docs/handoff.md`.

`1.6.6` added no JavaScript test — it is a `pos-db` microstep — so 298 − 280 = 18 is entirely
Rust and the JS rows are unchanged *since 20 September*, which is a narrower claim than the one
that stood here. `1.6.6b` is a `pos-db` microstep too, so 324 − 298 = 26 is Rust again and the JS
rows have now been still for two days. They are still the rows that move most often, because every
UI microstep adds tests — and **the next microstep after `1.6.6b` is likely to be a UI one**, so
expect them to move.

**The 26 break down as 17 + 7 + 2**, and the split is worth keeping because it is unusual: 17
integration tests in `crates/pos-db/tests/audit_verifier.rs` that **drive the binary as a
subprocess**, 7 unit tests inside `crates/pos-db/src/bin/verify-audit.rs` for the two pure
parsers, and 2 in `crates/pos-db/tests/audit.rs` for the register reader. The subprocess suite is
the first in this repository, and it carries a guard worth knowing about — see §2k.

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
| `development` | **`8f05458`** — `just pre-push` exits 0 and `ci` run **`35602921581` is a success on the tip**, queried by SHA. Carries `1.9.1`, migration `0005`, `1.1.9`'s `ClockRepository`, `1.2.6`, the audit's fixes, #185's seven tests, `1.11.6`'s scan capture with #189's fix, `1.11.11`'s keyboard map, `1.9.2`'s document counters, `1.7.2`'s embedded typeface, `1.6.6`'s audit repository and `1.6.6b`'s `verify-audit` |
| `staging` | **`f2edbb6`** — **45 behind** `origin/development`, 5 ahead (its own five promotion merges). Re-measured 21 September with `git rev-list --count origin/staging..origin/development`; **this row has been wrong before and §9 states it independently** — if the two disagree, run the command rather than picking one. **Use the `origin/` refs**: a local `staging` left stale by 51 commits answers 92, which is how this row went wrong before |
| `main` | `24a0283` — **173 behind** `origin/development`, **133 behind** `origin/staging`, untouched since 20 August |
| Phase 1 | **30 of 112** executable microsteps (~27%) — `1.6.6b` (#208) landed 21 September, the same day as `1.6.6` (#204), after `1.11.6` (#188), `1.11.11` (#192), `1.9.2` (#195) and `1.7.2` (#200) on 19–20 September. The percentage moved on every one of the six. **Group 1.1 is closed** |
| Open PRs | **0**, and **nothing is in flight**. The WIP=1 slot is free |
| Open issues | **11**, not ten — #68, #69, #70, #71, #111, #112, **#113 (reopened)**, #114, #174, #197 and **#203**, listed live rather than carried forward. **#203 is the one every handoff since 21 September has missed**: the weekly `security` workflow filed it automatically at 09:08 UTC, it carries no labels, it is **not on board #4**, and it is a real red — §3 diagnoses it. #202 opened and closed with `1.6.6`; #207 opened and closed with `1.6.6b`. Both of the issues this session closed with their substance unresolved have been put right: **#113 is reopened** and **#197 carries #179's three surviving findings**, with `1.10.1` amended (#198) so `0006` is where they land. Nine of the ten are blocked on a human; **#174 is the exception**. #187, #191, #194 and #199 opened and closed with their microsteps |
| Board #4 | **23 items — 10 `Todo`, 13 `Done`**, counted live on 21 September rather than incremented. `Done` gains #207; `Todo` is **ten of the eleven open issues — #203 is not on the board at all**, which is the first time the "Todo is exactly the open issues" equation has been false. Archived items are excluded from the listing and their count is not readable through it |
| Rulesets | **four, all active**, all four checked in under `.github/rulesets/`. They **agreed with live when last compared by hand** (11 September) — no gate diffs them, so this is a dated observation, not an invariant. See §3 |
| Tags / releases | **zero of each.** The append-only tag ruleset has never been exercised |
| Repository | **PUBLIC**, GitHub Free, `OmarSweiti` the sole collaborator (admin) |

### Complete: 30 microsteps

Read live from the frontier region — the block between the `<!-- frontier:begin -->` and
`<!-- frontier:end -->` markers in `docs/implementation/README.md` — in its own order:

`1.1.0` `1.1.1` `1.1.2a` `1.1.6` `1.1.3` `1.1.4` `1.1.2b` `1.1.7` `1.1.5` `1.1.8` `1.2.1` `1.2.2`
`1.3.1` `1.8.9` `1.8.5` `1.11.2` `1.6.5` `1.6.1` `1.6.3` `1.11.0` `1.11.3` `1.9.1` `1.1.9` `1.2.6`
`1.11.6` `1.11.11` `1.9.2` `1.7.2` `1.6.6` `1.6.6b`

**Six predictions, six held.** `round(100*25/112)` through `round(100*30/112)` are 22, 23, 24,
25, 26 and 27, so the region moved on every one of the last six microsteps and now reads `30 of
112 executable microsteps fully complete (~27%)`. **The 31st moves it again** —
`round(100*31/112) == 28`.

**The heading above said "28 microsteps" while the list under it held 29 and the region said 29.**
It was a third hand-typed copy of a number two other surfaces already carry, and
`check-implementation-frontier.py` does not read this file. It is 30 now; if you change the list,
change the heading in the same edit or delete the heading's number.

**`1.2.6` was the exception and it happened as predicted.** `round(100*23/112)` and
`round(100*24/112)` are both 21, so #177 moved the count and the prose list and left `~21%` alone.
The counts that share a rounded percentage are **32/33, 42/43, 51/52, 60/61, 69/70, 79/80, 88/89,
98/99 and 107/108** — computed, not guessed. An earlier draft of this paragraph claimed 27/28 was
one; it is not, `round(100*27/112)` is 24 and `round(100*28/112)` is 25. Run the arithmetic. The same denominators appear on three
surfaces the checker reconciles — the region, `00-master-plan.md:96`, and
`status-page.html:464-504` — so a denominator change lands in all three or the gate reds.

### Partially delivered — two, down from three

A `**Full-step status:**` marker mechanically means "partial" whatever its prose, and
`scripts/check-implementation-frontier.py` refuses to let such a step be declared complete.

| Step | Marker | What is missing |
|---|---|---|
| `1.2.0` | `phase-1:221` | a reference register exists nowhere — **#68**. `bench-gate --check-profile` exits 3 |
| `1.6.4` | `phase-1:691` | only the 1.8.x terminal handler — gated on **#111** and on 1.8.x wiring `pos-db` into the terminal |

> **Every `file:line` reference in this document rots, and this one rotted twice in a day.** The
> 14 September handoff corrected `phase-1:691` to `:690` against a grep; `1.2.6` then added lines
> above it and it is `:691` again. Six references were stale when the 15 September audit checked
> them. They are re-measured at `1c1fd4f` — but **treat every one as a hint and grep for the quoted
> text**, which is why each is quoted. A line number into a file that every microstep edits is a
> fact with a half-life of about a day.

**`1.1.9` cleared its marker on 14 September — the first one ever cleared rather than added — and
it took three deletions, not two.** Each is caught from a different side, and the third is the one
nobody had met:

| Rule | What it holds | Verdict on 14 September |
|---|---|---|
| 3 | the `**Full-step status:**` marker itself | deleted |
| 8 | the `README.md` prose naming the same backticked id | rewritten in the same commit |
| **4** | a `**Done when:**` line must exist to have been satisfied | **this is the one that fired** |

Rule 4 fired because `1.1.9` carried a `**Current half done when:**` line, which is deliberately
**not** the literal string the checker matches. While the halves were split, rule 3 refused every
completion claim and rule 4 never got the chance; delete the marker alone and the checker reds from
the opposite direction with a message about a missing `Done when`. Promoting that line to a real
`Done when:` over all three commands is the third deletion. **`1.2.0` carries the same
`Current half done when:` shape** (`phase-1:214`), so whoever finishes it meets rule 4 too.

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

## 2b · LANDED — microstep `1.9.1`, migration `0005` (#170, 14 September)

**Merged into `development` as #170, closing #169. Nothing is in flight.** This section is the
record of what shipped and what it cost, kept because five of its findings are not in any plan.

### What shipped

| | |
|---|---|
| `crates/pos-db/migrations/0005_sale_columns_and_sequences.sql` | **1,179 lines**, transcribed verbatim from `ref/schema.md` §0005's **three** code fences in document order. **16** `CREATE TABLE` · **5** `CREATE INDEX` · **2** `CREATE UNIQUE INDEX` · **60** `CREATE TRIGGER` · **1** `CREATE VIEW` · **1** `INSERT` · **14** `ALTER TABLE` = **99 objects** |
| `crates/pos-db/tests/common/registered_chain.rs` | the shared fixture the repair is built on — org, an approved rate pack and policy, an evidenced store, registers, open shifts, and the auxiliary facts a completion must prove |
| `crates/pos-db/tests/migration_0005_sale_columns_and_sequences.rs` | the six tests `phase-1:1063` names, plus one more (below) |
| five existing `pos-db` suites | the seventeen deliberately-red tests, repaired |
| `apps/server/migrations/20260914090000_sale_columns_and_sequences.sql` | the mirror, which creates nothing and records why |
| `ref/schema.md` · `phase-1-sellable-mvp.md` · `00-master-plan.md` · `README.md` | `· SHIPPED`, four ICV sites, the frontier |

`verify-schema` moved **4 migrations / 32 tables / 300 columns → 5 / 48 / 457**. `just test` moved
**241 → 248**. `verify-pg` maps 5 against 5 and its engine pass applies all five to real PostgreSQL 18.

### The seventeen failures — what the last handoff got slightly wrong

It said every pre-`0005` test "completes a *minimal* sale, so none can satisfy them", implying seven
gates each. Measured: **sixteen of the seventeen failed on one thing — `a sale must name an existing
register`**, the trigger repairing the FK `ALTER TABLE` cannot retrofit. Only
`a_committed_sale_is_readable_on_a_fresh_connection` reached the durable-outputs gate, because it
alone inserted a sale already `completed`. The gates are sequential, so the first wall is the only
wall you see. **Budget a repair by the first failure, not by the number of gates.**

### Four things that outlive the repair

* **A sale can no longer be born `completed`.** Its original `receipt_artifact` carries
  `REFERENCES sale(id)` and the durable-outputs gate wants that artifact already present, so the row
  must exist before the thing that proves it may exist at all. Every sale is inserted `parked` and
  sealed. `outbox.rs` already wrote in that order for an I-4 reason; now there are two.
* **The smallest fact graph a checkout produces is seven facts, not three** — sale, line, tender,
  the line's tax component, the tender's initial status event, the original receipt artifact, and
  the audit row. That is what `sale_commit_base_complete` means by a whole sale. `outbox.rs`'s
  `COMMIT_SIZE` moved 3 → 7 and its counts are now **scoped to the sale's own commit**: the shift
  open beside it is a separate business transaction with an envelope of its own.
* **A line's tax category must equal its product's.** `0003`'s
  `sale_line_tax_category_evidenced_*` said so; nothing had yet had to obey it. A line that differs
  is a supply-specific override needing immutable `sale_supply_tax_context` evidence — which is a
  materially different thing and must not be faked in a fixture.
* **A shift open cannot happen inside a checkout transaction.** `shift_open_has_ready_commit` needs
  a ready envelope, writing one needs a transaction, and SQLite has no nested `BEGIN`. It is a prior
  business transaction, which is also the truth about the boundary.

### The fixture's contract, and the trap inside it

`registered_chain.rs` writes **what `0005` requires and nothing more**. It does *not* write
`sale_tax_summary` or `sale_supply_tax_context`: `0005` requires those rows to be in the manifest
**if they exist**, never that they exist, and a fixture that seeded optional facts would silently
widen what every suite using it asserts.

Its ids are `[0xFC, slot, 0 … 0, tag]`, outside the `[b; 16]` / `vec![b; 16]` / `Uuid::from_u128(n)`
shapes every `pos-db` test mints. **The trap:** derived ids must not overwrite byte 1, which carries
the slot — an earlier draft did, so two checkouts in one database mapped onto the same change id and
the second was refused by `fact_commit_member`'s primary key. Loud rather than silent, but it would
have surfaced in the *next* microstep's fixture. Derivation marks bytes 2 and 3 now, and
`two_sales_on_one_register_share_its_shift_and_not_their_commits` holds the property — it is the
seventh test, not one of the six `1.9.1` names, and it fails against the previous derivation.
**`1.9.2` and `1.9.5` both ring up more than one sale per register; that test is for them.**

### Five findings from this work, none of them in the plan

* **`0002:103-106` is a stale forward reference that reads as an instruction.** It says "0005 adds
  `tender_state`/`captured_at`" to `sale_tender`. **`0003:322-324` superseded it** with append-only
  `tender_status_event`, and §0005 contains **zero** `ALTER` on `sale_tender`. A reader following
  0002 adds columns that must not exist.
* **The live completed-sale guard set is the post-0003 one.** `0003:320` replaced 0002's
  `sale_tender_amount_frozen_once_completed` with `sale_tender_no_update_once_completed`, and 0003
  defines the `sale_line_no_*` triggers **twice** — the `913-933` versions are live.
* **`sale` had no foreign keys at all** — `pragma_foreign_key_list('sale')` was empty. `0005` repairs
  `register_id` and `ref_sale_id` with **triggers**, the documented trade under `ref/schema.md`'s
  `-- THE SALE FOREIGN-KEY REPAIR.` comment — *"ALTER TABLE can attach one to a NEW column and
  cannot retrofit one onto an existing column"*.
* **No `PRAGMA` may appear in `0005`.** The runner wraps each migration in one transaction, where
  `PRAGMA foreign_keys` is a no-op and `PRAGMA defer_foreign_keys` cannot clear a deferred violation.
* **`verify-schema.py:405` skips any heading containing `SHIPPED`**, and pass 1 has already applied
  the file from disk. The marker is therefore required in the **same** commit as the migration. A
  scoping agent advised the opposite; the script settles it.

### The Postgres mirror creates nothing, and `REGISTER_LOCAL` was the wrong instrument

Every one of the 99 objects is either tenant-owned and blocked on the multi-tenant sign-off, or
register-local and never synced. There is no third group, and unlike `0002` — which found a half
already wrong on the server and fixed it — there is nothing here for the server to correct.

The last handoff said to add "`REGISTER_LOCAL` entries for the tables that never sync". **That list
is keyed by migration *file*, not by table**, and `0005` is not a register-local migration: most of
what it ships does sync, once the server has somewhere to put it. The nine register-local tables are
named in the mirror's header instead. `tender_type` is the one that looks global and is not —
`ref/schema.md`'s machine-readable inventory names exactly two global tables, and the sentence
beside it says why activation and sort order make this one tenant-owned.

---

## 2c · LANDED — microstep `1.1.9`'s database half (#173, 14 September)

**Merged as #173, closing #172. Group 1.1 is closed.** `1.9.1` shipped `trusted_time_state` in the
morning and this consumed it in the afternoon, which is the only reason two microsteps landed in
one day.

`crates/pos-db/src/repo/clock.rs` maps the pure domain `ClockState` onto the table field for field,
across the restart that is exactly when a wrong clock arrives. Eight tests, where the microstep
names one.

### Three decisions inside it

* **`trusted_time_state` is register-local, not a fact.** `ref/schema.md` §"Convergence" names it in
  the exclusion list, so unlike every other repository in this crate it writes an **upsert** and
  takes no `sync_commit` envelope. It still takes an explicit `&Transaction`, because `repo/mod.rs`
  makes the caller own the boundary.
* **`boot_token` is stored here and interpreted elsewhere.** A bare monotonic counter cannot
  identify its own boot — a new boot eventually passes the old anchor. The shell compares the token
  on startup and calls `note_monotonic_reset` when it changes. The repository hands the bytes back
  unread; deciding what a change *means* belongs to a shell that does not exist yet, and was not
  smuggled in early.
* **An unknown `anomaly_kind` is a hard error, not `None`.** A later migration may widen that
  `CHECK`. Reading an unrecognised variant as "no anomaly" would silently downgrade a register from
  *something is wrong with the clock* to *nothing is wrong with it*, on the one value whose whole
  job is to be distrusted.

### The security finding, and how it was caught

**`StoredClock` derived `Debug`, and `boot_token` ends in `_token`.**
`.claude/rules/security.md` redacts every such field at every nesting depth and names "test fixtures
that print" among the surfaces it covers — a derived `Debug` is exactly that, and
`ref/security-compliance.md:511` already required the fix: *"secret-bearing types implement `Debug`
and `Display` as a redacted constant"*. It is now a hand-written impl showing presence and not
bytes, held by `the_boot_token_is_never_printed`, **verified live** by restoring the derive and
watching the test fail.

Two things about that worth carrying forward, because both are general:

* **The first version of that test was vacuous in an instructive way.** It asserted that no token
  byte appeared as a decimal digit anywhere in the `Debug` string. Every timestamp is full of
  digits, so it failed on the fixture's own `7`. It now builds the exact rendering a derived `Debug`
  would have emitted, *from the value itself*, so changing the fixture's bytes cannot make it
  silently pass. **A redaction test must name what leaking looks like, not what it does not.**
* **The review also found a doc comment that overclaimed.** It said the error path never quotes a
  stored value; it quotes one — an unrecognised `anomaly_kind`, because the discriminant is the
  whole diagnosis and it is a schema enum rather than merchant data. A comment that overstates a
  security property is worse than no comment, because the next reader trusts it instead of checking.

The same sweep found the same class of defect in **shipped** code and did **not** fix it in this
pull request: `outbox.rs`'s `ManifestEntry` derives `Debug` over `pub payload: String`, the
canonical fact JSON its own module doc says carries customer data. That is **#174**, filed rather
than folded in — `outbox.rs` is `1.8.9`'s code, and a late file in a microstep's diff is the scope
leak the issue template refuses by name.

---

## 2d · 15 September — an audit of the day before, and the four defects it found

**Every pull request in §2b, §2c and the `1.2.6`/redaction pair was green in CI. Four of them were
wrong anyway.** #180 fixed what was fixable; #179 carries what a committed migration cannot.

### How it was run, because the shape is reusable

Ten independent review lenses over `005962d..6231a8a`, each told *"an empty findings array is a
valid and respectable answer — do NOT invent findings to look thorough"*. Every finding was then
attacked by **three skeptics with different angles** — correctness, evidence, materiality — and kept
only on a majority non-refutation. **53 findings raised, 121 verdicts, 86 refuted.** 169 agents.

Two things to know before running it again:

* **It hit the session limit.** 38 agents died, and the `gates`, `invariants` and `security` lenses
  lost their whole verify stage. Their findings were dropped as *unverified*, which is **not** the
  same as refuted — the raw findings survive in the run's `journal.jsonl`, and the important ones
  were hand-checked instead. A re-run would cover them properly.
* **The return value did not survive the task notification.** Read `journal.jsonl` directly: the
  `confirmed` array came back empty from the envelope while 53 findings sat in the journal.

### The four defects

**1 · Three I-4 assertions had stopped proving anything.** `tests/sale_immutability.rs`. Once the
shared fixture attached the sale to a registered chain, the line `INSERT` was refused by 0003's
`sale_line_tax_category_evidenced_insert` — `attach()` gives the product a tax category and the
test's line omitted one — and the two `DELETE`s by foreign keys the fixture's own auxiliary rows
created. **All three stayed green with their I-4 trigger removed**, reproduced against a real chain.
The companion test only checks the trigger's *name* exists in `sqlite_master`, which this repo's own
`fact_table_guards.rs` header calls "a presence check masquerading as a proof". They assert the
exact refusal message now. `migration_0003.rs` had already hit this interaction and adapted; the
sibling file was missed.

**2 · A redaction test was vacuous, and its mutation check hid it.** #178's `ManifestEntry` half
searched the `Debug` output for the payload's raw text — and `Debug` escapes the quotes a canonical
JSON payload is full of, so the needle could never appear. The mutation check removed **both** impls
at once, so `FactMember`'s live half masked the dead one. **When a guard has two halves, mutate them
one at a time.**

**3 · A "workspace sweep" missed a crate.** `pos_sync::Change` derived `Debug` over the same
canonical payload at its destination on the wire, and `PushBatch` prints its changes, so the leak
nested. `grep` the workspace, not the file you are editing.

**4 · Two doc comments described behaviour the code did not have.** `clock.rs`'s `invalid_stored`
claimed the error "never carries a timestamp": all four timestamp columns went through
`Timestamp::parse_iso8601`, whose error is `#[error("cannot parse {0:?} …")]` and echoed the raw
text, and the message named none of the four columns. Both halves of the claim were false. **A
comment that overstates a security property is worse than none**, because the next reader trusts it
instead of checking.

### The pattern worth carrying

**Three of the four were in the verification, not the code** — a vacuous assertion, a mutation check
that masked a dead guard, an incomplete sweep. CI cannot see any of them; every one was green. The
only thing that finds this class is an adversarial reader who assumes the author was wrong.

### What #179 carries, and cannot be fixed where it was found

Migrations are forward-only, so these need a later one:

* **`sale.is_training` has no `CHECK (… IN (0,1))`** while the fiscal-decision gate keys on `= 0` —
  so `2` silently disables it. **The transcription was faithful**; `ref/schema.md`'s own line has the
  same gap.
* **`receipt_artifact` and `print_attempt` are declared fact tables with no delivery-envelope gate**,
  while their three siblings in the same migration have one.
* **Five of `0005`'s seven completion gates have no negative test** — including
  `sale_completed_requires_durable_outputs_*`, the gate that is the entire justification for
  `outbox.rs` growing 3 → 7 members. It could be dropped today and nothing would go red.
* **`doc_sequence`** — the one knowingly irreversible decision in `0005` — has no test and no
  `DELETE` guard.

## 2e · 15 September — the five gates that could have vanished

**#179's third finding is closed.** `0005` put seven gates in front of
`sale.status = 'completed'` and `1.9.1` shipped a negative test for two. The other five are now six
tests — the durable-outputs gate has two independent halves and gets one each — plus a seventh that
proves every one of them is load-bearing.

| Gate | Test | What is withheld |
|---|---|---|
| tax policy | `a_completed_sale_snapshots_its_store_current_policy` | the policy, then a *different* approved one |
| fiscal decision | `a_live_sale_requires_an_evidenced_fiscal_decision` | the store's evidenced obligation |
| tax components | `completed_lines_require_exactly_their_applicable_tax_components` | the component, then its rate |
| discount recap | `a_completed_sale_recaps_exactly_its_line_allowances` | a recap with no allowance behind it |
| durable outputs | `sale_completion_requires_a_queued_original_receipt` | the queued original print job |
| durable outputs | `sale_completion_requires_a_manifest_naming_every_fact` | one manifest member, five ways |
| all five | `every_completion_gate_is_load_bearing` | the trigger itself |

`just test` reads **266** where it read 259; `pos-db` alone goes 91 → 98. Nothing else moved: no
schema, no migration, no Postgres mirror, and `check-test-catalog.py` still reconciles at 92 cases.

### The harness is the part worth copying

A negative test says the statement was refused. It does not say *by what* — and §2d's first defect
was exactly three assertions refused by a foreign key and a sibling trigger rather than by the guard
they named. `every_completion_gate_is_load_bearing` closes that by running each gate twice against
the same withheld precondition: with the trigger in place the refusal must carry that trigger's own
message, and **with the trigger dropped the identical `UPDATE` must be accepted**. Only the
`_update` half is ever dropped, never its `_insert` sibling — that is #178's mistake, where removing
both halves at once let the live one mask the dead one.

Two properties make it worth the lines. `DROP TRIGGER` errors on a name that is not there, so the
test reds the day a later migration removes one of these gates — which is #179's actual complaint.
And the harness was itself falsified before it was trusted: pointed at
`sale_completed_requires_durable_outputs_insert` instead, it fails, because the `_update` half still
stands and the completion is still refused. Each of the six tests was also falsified individually —
its gate dropped at the top of its own test — and **all six went red**.

### Three limits the tests carry as comments rather than work around

* **The obvious fiscal withholding is unavailable.** `store_fiscal_evidence_consistent_update`
  (`0003`) already refuses to let a store leave either evidenced shape by edit, so a test written
  that way would assert `0003`'s guard and stay green with `0005`'s removed. The reachable shape is
  the third `fiscal_obligation` value — `pending_evidence` with a disabled profile, which is a
  consistent store and the one a merchant actually sits in before the ISTD paperwork arrives.
* **Two arms cannot be isolated at all.** An *extra* tax component, and a line allowance with no
  recap, are each also a fact the delivery manifest does not name, so the durable-outputs gate
  refuses the same statement and SQLite does not document which of two eligible triggers fires
  first. An ambiguous assertion is the failure mode the whole exercise exists to avoid.
* **One arm is covered but not isolated.** For a `doc_type = 'sale'` a NULL policy trips both the
  `IS NULL` arm of the tax-policy gate and its store-comparison arm, because SQLite's
  `NULL IS NOT <blob>` is true. Isolating the first needs a `doc_type = 'refund'` whose referenced
  sale is also policy-less.

### #179's other three items have no migration number to land in

`0006` is named by `1.10.1`, the stock ledger (`phase-1-sellable-mvp.md:1117`), and `0007` by
`1.2.5`, FTS5/PLU/tiles/scan rules (`:303`). So `sale.is_training`'s missing
`CHECK (… IN (0,1))`, the two declared fact tables with no delivery-envelope gate, and
`doc_sequence`'s missing delete guard must either ride inside one of those two migrations or claim
`0008`. **That is why item 3 was the right one to take alone:** it needed no migration number at
all, and the other three are a scheduling decision rather than a coding one.

## 2f · 19 September — `1.11.6`, and a guard that hid behind its own other half

**The 25th microstep.** `1.11.6` — global scan capture — is the first UI microstep since
13 September, the first JavaScript test added since then, and the first work to enter through the
full §4 loop from the start: issue **#187** filed with its eight fields, added to board #4, set
`In Progress`, then built.

`apps/terminal/src/lib/scanner.ts` splits a keystroke stream into scans and typing by inter-key
timing alone — under `SCAN_MAX_GAP_MS` (30) apart is one burst, and an Enter inside the same window
commits it — and routes the
result whatever holds focus. The heuristic takes the time as an argument, the discipline
`direction.ts` applies to the document root, so the boundary is testable without a scheduler.

### The hard part is not the timing, and it is not fixable in the heuristic

`ref/ui-spec.md:140` calls focus routing the detail where most implementations break. It breaks for
a reason no amount of care in the heuristic removes: **a burst cannot be recognised until a second
character arrives inside the threshold**, and by then the first has already been delivered to
whatever had focus. Measured against `1.11.0`'s harness rather than reasoned about — a capture-phase
listener that suppresses from the second key onward leaves exactly one character, `خبز6`, in the
search box.

So `ScanStep` carries the leak count rather than hiding it, and the commit path restores the value
the field held before that keystroke. `scan_routes_while_search_focused` asserts both halves,
because only the pair is "routing correctly".

### The finding worth carrying: a guard masked by its own sibling

Eight mutations, one per guard. **Seven were caught by exactly the test that claims to cover them.
One was not: deleting `preventDefault` from the absorbed branch left all thirteen tests green.**

`scan_routes_while_search_focused` asserts the field's *final* value, and retraction restores that
value from a snapshot taken before the burst began — so it passes whether the absorbed characters
were suppressed on the way in or merely undone on the way out. The two halves of the guard masked
each other. **This is #178's shape exactly**, found in new code by the same technique that found it
in old code, which is the argument for running the sweep every time rather than when something feels
wrong.

The fourteenth test, `never lets an absorbed character reach the field`, watches the `input` events
instead of the end state. Undoing is not equivalent to never inserting: a field that receives all
six characters fires six `input` events, so a controlled search box would run its query six times
and repaint the barcode before it vanished.

### Two facts established by throwaway probes, both easy to get wrong

* **`delay` is a `userEvent.setup()` option, not an option to `type` or `keyboard`.** Passed to the
  call it is silently ignored and every keystroke lands on the same timestamp — which presents as a
  scanner that never scans, with no error anywhere. In `setup`, consecutive `keydown` events are
  exactly `delay` ms apart on the fake clock.
* **`KeyboardEvent.timeStamp` is driven by the faked clock** under `vi.useFakeTimers()` in this
  configuration, so the module reads the event's own time rather than reaching for a global.

### `just build-web` is the only thing that typechecks a test

`tsc -b` refused `Array.prototype.at`: `tsconfig.app.json` targets **ES2020** and lists
`lib: ["ES2020", "DOM", "DOM.Iterable"]`, and `.at` is ES2022. Fourteen tests had already passed,
because **`vitest` transpiles without typechecking**. So a `.test.ts` can be green in its own runner
and red in the build, and `just test` will never say so — the gate that catches it is `build-web`,
which is why `just pre-push` runs both. The fix was to index; widening the application's target for
a test's convenience would have been the wrong direction.

### The defect the scope found after the microstep had merged

**`1.11.6` shipped a real bug, and the reason it shipped is the more useful half.** An adversarial
scope of the microstep finished after #188 had merged and found that `feedScanKey` accepted an Enter
arriving arbitrarily long after the characters it terminated. So an abandoned burst sat in the
candidate waiting to be committed by the next unrelated Enter the cashier pressed. Written as a
failing test before the fix: `["6", 0], ["2", 2], ["Enter", 5000]` produced a scan. At a till that is
a phantom line added minutes after a misread, against whatever is on screen by then. Fixed by
requiring the terminator inside the same window as the characters.

**`ref/hardware-and-receipts.md` §5 is a normative source on scanning, and `1.11.6` was written from
`ref/ui-spec.md` alone.** Both carry the same *"route correctly even when focus is in the search
box"* sentence, so reading one felt like reading the subject. It is not: `:290` is the definition the
fix rests on — *"a burst with < 30 ms between characters, **terminated by Enter**"*, one
transmission — and `:292` is the line that settles what "route correctly" means:

> A cashier types two letters, then scans; the scan must become a line, **not extra text in the
> search field**. Test `scan_routes_while_search_focused` exists for exactly this.

So the retraction `1.11.6` shipped was **required** rather than a refinement chosen on the way past,
and the test it shipped already reproduces that document's own scenario — three Arabic letters, then
a scan. The behaviour was right; its sourcing was not, and only one of those is visible in a green
run. **Before implementing from a `ref/` document, grep the whole `ref/` set for the subject** —
`grep -ril 'scan' docs/implementation/ref/` returns both files in under a second.

### Documentation nothing would have caught

`ref/test-catalog.md:314` and `02-development-workflow.md:1710` both listed these two tests as
"still owed". Neither claim is reconciled by any checker — `check-test-catalog.py` runs
catalogue → runner, never the reverse — so both were corrected deliberately. Lesson 10 again: the
document nobody's gate can read is the document that goes stale.

## 2g · 20 September — `1.11.11`, and the sweep that changed the code three times

**The 26th microstep.** `1.11.11` — the keyboard map — landed as #192. Phase 1 reads **26 of 112
(~23%)**, and `round(100*27/112)` is 24, so the next one moves it again.

`apps/terminal/src/lib/keymap.ts` carries all ten rows of `ref/ui-spec.md` §7 as data, with the
dispatcher taking its handlers as arguments. None of the actions exists yet — pay, park, resume and
returns are screens nobody has written — so a module that reached for them could not have been
tested until they were.

### The `ref/` sweep earned its place on the first outing

`1.11.6` shipped a defect because it was written from `ui-spec.md` while
`ref/hardware-and-receipts.md` §5 was equally normative on the same subject (§2f). So `1.11.11`
started with `grep -ril 'keyboard\|keymap\|shortcut' docs/implementation/ref/`. Six files, two that
mattered, and it **changed the implementation three times before a line was written**:

* **§7 has ten rows; the phase file's prose lists eight**, omitting `Esc` (back / cancel) and
  `Enter` (confirm / commit scan). The phase file summarises rather than narrows — "every action
  reachable" is not satisfied by a map with no cancel — so all ten are bound.
* **The spec's minus is `−`, U+2212**, a typographic sign no keyboard emits. A binding on it would
  never fire *and would pass review*, because it matches the document character for character. The
  binding is on U+002D, and a test asserts U+2212 maps to nothing so that "correcting" it reds.
* **`ref/hardware-and-receipts.md:335`** qualifies wedge scanners against "the Arabic keyboard
  layout", which is why the map keys off the character produced rather than a physical scancode.

That is the whole argument for the habit: one grep, thirty seconds, three changes.

### The keys a focused field keeps, and why this differs from the scanner

A key that means something inside a text box belongs to the text box. `+` in the search field is a
plus sign, not a quantity change; `Delete` is a character deletion, not a voided line. Function keys
and `Escape` have no such meaning and fire wherever focus is.

It is the same collision `scanner.ts` faces with the **opposite** answer. A scan is recognisable by
its timing and can be retracted from the field it leaked into; a single `+` is indistinguishable
from a cashier typing one. So the map yields where the scanner retracts.

### `Enter` is now wanted by two modules

`ref/ui-spec.md:245` gives it both jobs — "confirm / commit scan". `scanner.ts` listens in the
**capture** phase and calls `preventDefault` only when an Enter commits a burst; `keymap.ts` listens
in the **bubble** phase and skips an already-handled event. Neither imports the other, so both sides
of the seam are asserted. A regression there is a phantom `confirm` on every scan, which presents as
a double submit.

### The mutation sweep caught a vacuous test for the second microstep running

Seven mutations, six caught by the test that names them. The seventh survived: the unwired-handler
test used `+` **with a field focused**, so `actionFor` returned `null` before the handler lookup was
ever reached — it passed without exercising the branch it was named for. It now observes
`defaultPrevented` through a probe registered *after* the map, and that probe needed its own
ordering fix first: both listeners bubble on `document`, so a probe attached before the map reads
`false` whatever the map did.

**Two microsteps, two tests that were green for the wrong reason, neither visible to any gate.**
CI, `tsc`, Biome and vitest were all green over both. The sweep is not an occasional technique; it
is the step that finds what the gates structurally cannot.

### One more thing asserted, because a test can shrink with its subject

The named test's expectation is **written out, not derived from `KEY_BINDINGS`**. A test that reads
its expected set out of the table under test stays green when a row is deleted from that table,
because the expectation shrinks with it. The duplication is the point.

## 2h · 20 September — `1.9.2`, and the sweep that found the word the design turned on

**The 27th microstep.** `1.9.2` — `SequenceRepository` — landed as #195. Phase 1 reads **27 of 112
(~24%)**, and `round(100*28/112)` is 25, so the next moves it again. `pos-db` goes 98 → 108 tests
and the workspace 266 → 276.

`crates/pos-db/src/repo/sequence.rs` allocates receipt and Z numbers from `doc_sequence` inside the
caller's `&Transaction`, so a number is spent only by a transaction that commits. It needed **no
migration**, which is why it was takeable at all: `0006` belongs to `1.10.1` and `0007` to `1.2.5`.

### One word decided the design, and the first sweep missed it

`ref/plan-validation.md:338` defines G-2, and it does not say gapless:

> Per-register counters must be crash-safe and **gap-detectable**. A gap in a receipt sequence is
> what an auditor asks about first.

Detection, not prevention. So `gaps()` reports rather than guarantees, and the two claims split
across the tests: `rollback_does_not_consume_a_number` and the hundred-point crash schedule prove a
number is never spent on nothing; `gaps_finds_the_number_whose_document_never_arrived` proves that
when a gap does happen it is visible.

**That line came out of a wider sweep than the one run first, and the difference is the lesson.**
`1.11.6` shipped a defect for want of *any* `ref/` sweep (§2f), so this microstep began with one —
`doc_sequence|SequenceScope|SeqKind|gapless|gap-free`, which returns **four** files. Sweeping the
*subject* in plain words — `sequence|counter|receipt number` — returns **ten**, and
`plan-validation.md` is in the difference. No term in the first sweep matches the word
"gap-detectable".

> **Sweep for the subject, not for the identifiers you expect.** Searching for the names you already
> believe in returns the documents that agree with you. The habit from §2f is necessary and was not
> sufficient; this is its second-order form.

The wider sweep also placed the boundary: `ref/test-catalog.md:240` and `:90` put
`prop_z_number_is_gap_free` and `prop_icv_is_gap_free_and_strictly_increasing_within_its_scope` in
**Phase 2**, so the deeper gap-free properties are not this microstep's and the four named tests are
its whole scope.

### `next()` refuses `SeqKind::FiscalIcv`, and the reason is #113

Not because the scope rule forbids it — `0005`'s `CHECK` pairs `fiscal_icv` with `store` quite
happily. **The first such row that exists closes #113's reversal window.** `ref/schema.md:4255` is
explicit: the frozen `store` namespace stays cheap only while nothing has been allocated, and after
one row correcting it is a migration *plus* a data repair on a counter required to be gapless.
Allocation belongs to `2.7.4`, which must re-check merchant decision 6.9 first.

A `next()` that could create that row would let a stray test spend a decision reserved for a later
microstep. It refuses by name, a test asserts no `fiscal_icv` row exists, and `2.7.4` deletes the
refusal as part of doing the re-check — which is the forcing function `schema.md` asks for.

### `gaps()` errors rather than returning empty

The counter stores a high-water mark and nothing couples a bump to a document, so the evidence has
to come from the documents — and today it exists for one kind. `Receipt` reconciles against
`sale.receipt_number`; `ZReport` has no table (`z_report` is Phase 2) and `FiscalIcv` is unreachable
while `next()` refuses it, so both return a **named error**. The exception is where nothing was ever
allocated, the one case where "no gaps" is true rather than merely unknown.

An unconditional `Ok(vec![])` would read as "this counter is sound" to every caller and every test.
Likewise an unparseable receipt number is refused rather than skipped: skipping manufactures a gap
that is not there, and counting it as zero hides one that is.

### The mutation sweep found a warning with no test — the third microstep running

Seven mutations, six caught by the test that names them. **The seventh survived: `INSERT OR REPLACE`
passed all nine tests** — and it is the exact hazard the module doc-comment warns about at length.
`REPLACE` deletes the row and re-inserts it, so `doc_sequence_monotonic` (a `BEFORE UPDATE OF
next_value` trigger) never fires, and `doc_sequence` has **no delete guard** behind it — #179's
fourth finding, still open and now untracked (§3).

The numbers come out identical either way, which is why every other test passed. What `REPLACE`
cannot fake is the rest of the row: `prefix` is `TEXT NOT NULL DEFAULT ''`, so a re-insert that does
not name it silently resets it — and `gaps()` reads `prefix` to strip it off a receipt number, so
the register would lose its prefix and every gap report over it would start refusing rows it should
have parsed. `next_preserves_the_row_it_does_not_own` pins that.

**Three microsteps, three tests that were green for the wrong reason** (§2f, §2g, here). The
constant is not the code; it is believing a test covers what its name says. Every gate — CI, clippy,
`tsc`, nextest — stayed green over all three.

### Two things measured rather than assumed

* **`busy_timeout` is five seconds**, set in `pos_db::open` at `crates/pos-db/src/lib.rs:160`. A
  first reading of that file concluded it was unset and nearly designed the concurrency test around
  a `SQLITE_BUSY` that does not happen here — a `grep … | head -8` had truncated two lines short.
  With the timeout, the second writer waits, so `concurrent_next_never_duplicates` is deterministic;
  it was run ten times to confirm rather than once.
* **`next()` is a single statement**, an upsert with `RETURNING`, so there is no read-then-write
  window in which two transactions could observe the same value.

## 2i · 20 September — `1.7.2`, and the three decisions the operator took

**The 28th microstep, and the first one this project has taken because a human answered a question
rather than because the code was ready.** `1.7.2` landed as #200. Phase 1 reads **28 of 112 (~25%)**,
and `round(100*29/112)` is 26. `pos-hardware` goes 2 → 6 tests; the workspace 276 → 280.

**IBM Plex Sans Arabic**, Regular and Bold, committed under `assets/fonts/` with the SIL Open Font
License 1.1 beside them and read from one place, `pos_hardware::font`. The operator chose the family
on 20 September from the three `phase-1:740` names. It unblocks `1.11.1`.

`ref/hardware-and-receipts.md:110` wants two things at once — the font is embedded *and* it is the
same file the UI uses. A network font fails both: a register trades offline by design, so a fetched
font is absent exactly when it is needed, and a font the UI resolves separately is not the file the
rasteriser drew with. `1.7.3` and `1.11.1` build on the same constants.

### What the tests assert beyond their names

`embedded_font_bytes_are_not_empty` would be worthless read literally: a text placeholder, a Git LFS
pointer and a truncated download are all non-empty. It checks the TrueType `sfnt` version
(`0x00010000`) and that Regular and Bold are **not the same file twice** — a bold byte-identical to
the regular renders totals that do not stand out.

Four mutations, four caught, including **replacing a face with a text placeholder**. That is the
failure a diff reading `Binary files differ` physically cannot show a reviewer, which is the whole
argument for testing a committed asset at all.

### Two things corrected rather than worked around

* **The `Files:` line named `crates/pos-hardware/Cargo.toml` and no change was needed.**
  `include_bytes!` requires no dependency and the workspace is `publish = false`, so no `include`
  key is needed to package the asset. The line now says so rather than carrying an edit manufactured
  to match it.
* **"Verbatim" was the wrong word for the licence.** IBM ships `LICENSE.txt` with CRLF and
  `.gitattributes`' `* text=auto` normalises it on commit — 4,456 bytes on disk became 4,363
  committed, exactly one per line across 93 lines. The OFL text is unchanged so the licence is
  satisfied, but `assets/fonts/README.md` now distinguishes the text from the file. The two `.ttf`
  faces are binary to Git and committed unchanged, which is the half worth checking: a font silently
  normalised would not be a font.

### The OFL clause that is inert now and binds `1.7.3`

The header reserves the font name — *"Copyright © 2017 IBM Corp. with Reserved Font Name 'Plex'"* —
and OFL 1.1 §3 forbids redistributing a **modified** version under a reserved name. Nothing here
modifies it. But **subsetting the file to shrink the binary is a modification**, and that is a thing
a rasteriser step might reasonably want. It is in `assets/fonts/README.md`, where someone doing it
will be looking.

### The three decisions, and what each turned into

The operator took all three open decisions on 20 September. Two were bookkeeping with teeth; the
third was the font above.

**#113 is reopened** (§3). It had been closed as `COMPLETED` while eight documents still carried the
question. The comment on it records why it cannot be closed: the ICV namespace is a **protocol
fact**, and `ref/merchant-decisions.md:160` puts it with the issuance event and validator tolerance
as things only the official ISTD package or a written E-Invoicing Directorate ruling can answer.
**Reopening an issue does not move its board status** — #113 sat at `Done` until it was set back to
`Todo` by hand. Worth knowing before the next reopen.

**#197 carries #179's three surviving findings, and #198 put the obligation into `1.10.1`.** They
ride inside `0006` not by preference but by arithmetic: `0007` is named by `1.2.5`, and
`verify-schema.py` requires migration numbers contiguous from `0001`, so a dedicated `0008` could
not land until both exist. §3 has the three, each re-measured.

`1.10.1`'s entry now also flags a consequence that would otherwise surface as a mystery red test:
**if `receipt_artifact` gains an envelope gate,
`sale_completion_requires_a_manifest_naming_every_fact` must omit a different member** — it omits
that one precisely because the table has no gate today, and it already iterates four others.

### What the ICV question actually needs, so the next session does not re-derive it

Recorded because it took a four-dimension sweep to establish and half of it corrected a first
answer that was wrong:

* **Five candidates, not three** — register, store, income source, credential, or one TIN across
  stores (`ref/schema.md:2650`). The short-form `⚠️ OPEN` blocks at `fiscal-jofotara.md:102`,
  `plan-validation.md:274`, `phase-2-money-grade.md:471` and `test-catalog.md:100` collapse
  "store/income source" and drop "credential", which is how a reader comes to think there are three.
* **Three of the five are unrepresentable today**, and correcting to one of them is not the "one
  forward-only migration" the documents advertise. #113's own owner comment measured it: the paired
  `CHECK` binds scope to kind, so widening `scope_kind` alone is cosmetic; and there is **no
  `income_source`, `credential` or `tin` table anywhere in the schema** to point a `scope_id` at.
  "Three coupled changes and a weakened invariant, not a free hedge."
* **"One store and one register makes the answers identical" is true of three of the five and false
  of the other two.** Nothing in the repository defines "income source" or states its cardinality
  relative to a store, and `fiscal-jofotara.md:59` has it as a *seller field on the document* — a
  tax-registration concept, not a physical one. One store may hold two, in which case store-scoping
  is already wrong at today's scale.
* **The specification may be obtainable now, and the two statements about that disagree.**
  `phase-2-money-grade.md:391` says *"ISTD publicly lists its Technical Integration Guide"* and calls
  the wait-for-a-merchant premise stale; #69 says the package *"can be obtained. Obtaining it is
  `2.7.0`, and it needs the credentials above."* Probably the Guide is public and the full package
  is not. Contact settles it cheaply.
* **There is no sandbox.** #69 quotes the 24-August audit: *"your own TIN is the sandbox."* The first
  document this product submits goes to a live authority, so the real deadline is the first
  submission, not the abstract reversal window.

## 2j · 21 September — `1.6.6`, and the defect a mutation sweep structurally cannot find

**#204 merged `1.6.6`** — `crates/pos-db/src/repo/audit.rs` (764 lines) and
`crates/pos-db/tests/audit.rs` (18 tests), plus four `DbError` variants, `serde_json` in `pos-db`
and the frontier. Issue #202 is the microstep issue, filed before the branch and closed by the
PR. Phase 1 is **29 of 112 (~26%)**.

### The two decisions the phase entry did not make

**`seq` is allocated here, explicitly, before the hash.** `audit_log.seq` is `INTEGER PRIMARY KEY
AUTOINCREMENT`, but it is *inside* the hashed canonical bytes and `audit_log_no_update` refuses
every `UPDATE` — so the number cannot be learned after the insert, and a wrong one is unfixable
forever. The append reads `IFNULL(MAX(seq),0)` and the register's head `hash` in one statement
through the caller's `&Transaction`. **`sqlite_sequence` was rejected on measurement, not taste**:
it rolls back with its transaction *and* carries no trigger guard at all — `UPDATE sqlite_sequence
SET seq = 9999` is accepted, after which AUTOINCREMENT and the chain diverge.
`a_rewritten_sqlite_sequence_does_not_move_the_chain` is the only test that distinguishes the two
candidates, because in every ordinary sequence of events they return the same number.

**An unrecognised `action` or `entity` is never a read error.** Both are `&'static str` on the
domain type and SQLite returns `String`, so the read path interns — one leak per *distinct*
spelling, never one per row. Interning against `cap::ALL` and refusing the rest was wrong twice:
`action` is already a superset of the capability vocabulary (`registered_chain.rs` writes
`sale.complete`), and a read that failed on an unknown spelling would let one inserted row disable
the whole verifier.

### The chain cannot fork, and nothing in SQL is why

There is no `CHECK`, no unique index and no trigger on `prev_hash`; an `INSERT` duplicating a
parent is accepted. The guarantee is structural and the module says so, because a reader who
assumes the schema is holding the line will remove the thing that is: I-9 puts the delivery
envelope's writes ahead of the audit insert in the same transaction, so the write lock is already
held when the head is read; a stale snapshot is refused at its write *without* consulting
`busy_timeout`; and `seq`, derived from that same read, collides on the primary key.

### The sweep found sixteen of eighteen, and then a review found what it could not

**18 mutations, 16 caught, 2 survived** — `ORDER BY seq` (indistinguishable while `seq` is the
rowid and no index applies to `WHERE register_id = ?`) and `canonical_version` left to the column
default (indistinguishable while `VERSION` is 1 and so is the default). Both were written into the
code as untestable *before* the sweep ran, and both comments now record that the sweep agreed.

**Then an adversarial review found a defect no mutation could have caught**, and this is the
lesson worth keeping. `chain()` returned `Err` on any row it could not rebuild and discarded every
honest row already read. That is not a missing guard — it is the wrong shape, and a sweep only
asks whether an existing guard is load-bearing. The consequence was severe and asymmetric:
`audit_log_no_update` and `audit_log_no_delete` guard mutation while **nothing guards `INSERT`**,
so adding one poison row — with an envelope the same SQL console can write — made every honest row
below it unreachable through the only public read, permanently, since neither trigger would then
let anyone remove or repair it. Cheaper than the re-chaining attack the chain exists to detect,
and more effective, because a re-chained history still verifies somewhere while an `Err` locates
nothing.

`chain()` now returns an `AuditChain`: the rows it could rebuild, **plus** an optional `ChainStop`
naming the `seq` it stopped at and why. `DbError` means what it should always have meant here —
the database would not answer.

The same review found the canonical-version gate refused in **both** directions, so the first
`VERSION` bump would have made every row already on disk unreadable and unrecoverable. It is still
`!=` — a row from below this build's layout is equally unhashable — but as a stop the bump now
costs the rows above the boundary and reports where it is.

### Three smaller things, each worth one line

* **A golden caught its own fixture.** `the_stored_hash_matches_a_pinned_golden` writes the
  canonical bytes by hand and hashes them itself rather than routing through the read path under
  test. It failed on its first run — not on the encoder, on `AT_MS`, which was eleven days from
  the `AT` string it claimed to be.
* **One mutation killed no test at all.** Moving the head read from `tx` to `self.conn` changed
  nothing, because `Transaction` dereferences to its `Connection` and in the house call shape the
  two are one SQLite handle. The test that closes it constructs the misuse deliberately: the
  repository over one connection, the transaction from another.
* **`indexing_slicing` fired after `nextest` was green**, as this document has warned twice. 25
  errors in the test file, all of them `rows[0]`, none visible until `just lint`.

### One reference corrected on the way past

`ref/security-compliance.md:290` attributed `verify-audit.rs` to microstep 5.4.4. `phase-1:715`
builds it at `1.6.6b` and `phase-5:187` says in its own words that the CLI *"is **not** new here"*.
The stale citation sat in the section a 1.6.6 implementer is sent to read, so it was fixed in the
same PR rather than filed. Nothing reconciles an owner citation across those files — this is the
same class as §3's "an issue's state and the plan of record are two surfaces".

### CodeQL blocked the first push, and it was right

`rust/hard-coded-cryptographic-value`, critical, on a fixed 16-byte nonce in the approval fixture.
It is a test, and conventions §5 gives a test no randomness — but `tests/approval.rs` already
solved the same problem by deriving the nonce from the handle id, which is deterministic *and*
unique per handle. Copying that idiom was a real improvement, not an appeasement: two handles in
one file no longer share a nonce. **A dismissal would not have survived**, which this repository
learned once already (`docs/a-codeql-dismissal-does-not-survive-a-move`).

---

## 2k · 21 September — `1.6.6b`, and two right answers that compose into a wrong one

**#208 merged `1.6.6b`** — `crates/pos-db/src/bin/verify-audit.rs` (the repository's first binary
target), `crates/pos-db/tests/audit_verifier.rs` (17 tests that drive it as a subprocess), seven
parser unit tests inside the binary, `AuditRepository::registers` with two tests of its own, and a
`PLANNED` entry retired. Issue #207 is the microstep issue, filed before the branch and closed by
the PR. Phase 1 is **30 of 112 (~27%)**.

**Both halves of the `Done when` were run, not inferred**: the suite passes, and
`cargo run -p pos-db --bin verify-audit -- --help` exits zero.

### The defect no mutation could reach, again, and it is a different shape from `1.6.6`'s

`1.6.6`'s was *the wrong guard*. This one is **two correct guards composing into a wrong answer**,
which is a shape worth naming separately because the sweep that finds neither finds this one even
less.

`chain()` stops at a row this build cannot rebuild and hands back the prefix — correct, and
`1.6.6` argued for it at length. `verify_chain` compares a chain against an anchor and reports
`Truncated` when the anchor is above the last row present — correct, and it is the whole reason
anchors exist. Put them together: a read that stops at seq 3 hands `verify_chain` rows 1–2, an
anchor at seq 5 finds no anchored row, and the verdict is
`Truncated { anchored_seq: 5, found_seq: 2 }` — **the rows between were removed**. They were not.
Rows 3, 4 and 5 are still on the disk. The tool accuses a merchant of deleting audit rows because
it could not read one of them itself — and once this product has two versions in the field, the
commonest cause of an unrebuildable row is an upgrade.

An anchor **at or above** the stop is now withheld and the run stays inconclusive; an anchor
*below* it is untouched, because the row it names was read. Over-correcting was the other way to
get this wrong, so `an_anchor_below_a_stop_is_still_applied` holds the opposite direction and
mutation 20 proves it does.

### CodeQL found the line beside a real defect, for the second microstep running

`rust/cleartext-logging`, two high-severity alerts, on the lines that echoed the anchor's register.
Following the flow found something worse a few lines down: **`source_kind` and `anchored_at` were
printed byte for byte out of a file supplied by whoever is being investigated**, into a document
read as evidence. One newline inside `source_kind` forges a whole register block — heading, rows
read and an `INTACT` verdict — into the middle of the report.

Nothing raw from the anchor reaches the report now. `audit_checkpoint.source_kind` is already
`CHECK (source_kind IN ('z_report','verified_backup','server'))`, so the safe answer was the
correct one: match it and print the matched `&'static str`. `anchored_at` must parse as an instant
and is re-rendered through `Timestamp`. **The alert cleared on the fix and nothing was dismissed** —
which is the second time in two days that following a CodeQL alert rather than arguing with it
found a real defect. Treat that as evidence, not luck.

### `protected-paths` was red by design, and this will happen to you too

**Retiring a `PLANNED` entry means editing `scripts/check-test-catalog.py`, which is inside the
frozen policy surface `check-branch-workflow-policy.rb` guards.** So the PR was red on
`protected-paths` from its first push, naming exactly one path and nothing else — the other three
walls in that job (source-plan/migration immutability, attribution, full-SHA actions) all passed,
because they run under `if: ${{ !cancelled() }}` precisely so a policy red cannot mask them.

**The edit was not optional**, and that is the part to carry forward. Landing a test the catalogue
names while leaving its `PLANNED` entry in place produces two assertion-3 violations —
*"retired PLANNED entry was reactivated"* and *"the runner lists this test; remove the stale
PLANNED entry"*. So **every future microstep that lands a catalogued test takes this same red.**
It is not an exception, it is the mechanism: shrinking the ceiling is a reviewed act.

The merge followed §9's manual recipe — title, attribution and branch-flow validated by hand — and
is on the ledger as rule suite **`4158403946`**, `result: bypass`, with one failing rule evaluation
naming `protected-paths`. That is a materially better ledger entry than the three the handoff
criticised on 13 September: those read *"6 of 6 required status checks have not succeeded"* because
a rebase had restarted everything, and could not be distinguished from a real override. This one
names one check, and every other check had **completed** and passed.

### The sweep: 23 mutations, 22 caught, 1 survived

The survivor is `ORDER BY register_id` in `registers()` — indistinguishable today because
`GROUP BY` over an unindexed column already sorts through a temporary b-tree, which is the same
class `1.6.6` recorded for `chain()`'s `ORDER BY seq`. It is written into the code as such, and it
becomes load-bearing the day a migration indexes `register_id`.

Three mutations are worth keeping because of what they cost to write:

* **`--help` exits 1.** Caught, because the four exit codes are asserted from a test rather than
  left in a source comment. They are a published interface: a phase gate and a shell script both
  branch on the number.
* **A stop outranks tamper evidence.** Survived the first sweep, because no test produced both.
  Writing one — break seq 2, make seq 3 unrebuildable, in the same copy — is what made
  `Verdict::worse`'s ordering testable at all. Tamper evidence wins: *"this register was altered"*
  is a finding somebody acts on today.
* **`anchored_at` accepted unparsed.** Did not compile on the first attempt because `Timestamp` is
  not `Default`. A mutation that does not build is not a sweep datum; it was rewritten and re-run
  rather than counted.

### Four things the next session should not re-derive

* **A verifier must never create the file it was asked to verify.** `Connection::open` creates a
  missing database and `pos_db::open` then migrates the empty result, so `verify-audit --database
  typo.db` used to answer *"no audit rows"* and exit 0 — indistinguishable from a clean register.
  It refuses now, and the test asserts the file is still absent afterwards.
* **The set of chains walked is what the file holds ∪ what the anchor names.** Delete *every* row
  of a register and it leaves the enumeration, so a verifier reading only `audit_log` walks
  nothing and exits 0 holding an anchor that says there were four rows.
* **The subprocess suite refuses to run in a release build rather than skipping.** It drives the
  binary with `POS_DB_KEY`, which only a debug build honours (`1.8.5`); a release run would fall
  through to the machine's **real** OS credential store and, on a clean machine, write a key into
  it. Two `#[cfg]` bodies, not a `const` assertion — a `const` one would break
  `cargo build --release --all-targets` for everybody.
* **`verify-audit` does not write `audit_checkpoint`,** and `a_z_close_anchors_the_head` is still
  owned by nobody in Phase 1. `ref/security-compliance.md:275` calls it Phase 1;
  `phase-2-money-grade.md:347` owns it. That disagreement is noted, not resolved — it is the same
  class as §2j's stale owner citation, and nothing reconciles an owner across those two files.

---

## 3 · The eleven open issues

**Ten are on board #4, all `Todo`, all assigned** — #68, #69, #70, #71, #111, #112, #113, #114,
#174 and #197 — and **#203 is not on it**, which is why every handoff since it was filed has
reported ten. Re-read live at `8f05458` with `gh issue list --state open`, which is the command
that finds the eleventh. **Nine are blocked on a human** — one on `hardware`, five on a
`decision`, two on a `merchant answer`, and #197 on the sequencing of `0006`. **#174 and #203 are
the two code alone can close today.**

### #203 — the weekly security workflow is red, and it is not your diff

Filed automatically at **09:08 UTC on 21 September** by `security.yml`'s own
`scheduled-failure-escalation` job, against run `35581552459` on `40425a8`. No labels, no board
item, no assignee. It has been open through three merges.

**The failing job is `workflow-analysis`, and the two other jobs in that run passed** —
`scheduled-advisories` and the escalation job itself. zizmor 1.29.0 exits 13 with six
`stale-action-refs` warnings, at `ci.yml:72`, `:402`, `:537`, `cross-platform-canary.yml:106`,
`proptest-scheduled.yml:49` and `release.yml:271`. Every one is the same line:

```
action's hash pin has mismatched or missing version comment: points to commit 02cb101ec7c4
```

**Diagnosed rather than guessed, and the diagnosis is the opposite of alarming.**
`dtolnay/rust-toolchain` moved its **mutable `v1` tag** from `6c977a6ca407` (committed
2026-08-05, and still a live commit) to `02cb101ec7c4`. This repository pins the immutable SHA
`6c977a6ca407` and writes `# v1` beside it. zizmor now compares the comment against where the tag
points *today*, finds they disagree, and says so. **The pin is correct and did its job** — that is
exactly what full-SHA pinning is for, and the repository was not moved by whatever moved upstream.
What is stale is a human-readable comment.

So this is the class `CLAUDE.md` predicts and deliberately keeps out of the local gate: a
time-varying advisory check that went red **without any repository change**, like the `rustls`
advisory in §0's window. `just lint` and `just guards` are green on the same tree.

**It is not free to leave.** The weekly workflow stays red, so the *next* genuine security finding
arrives as "still failing" rather than as news — which is precisely the failure mode a scheduled
escalation exists to prevent. Two ways to close it, and both are `.github/workflows/**` edits and
therefore **deliberately-red `protected-paths` PRs** of their own:

* re-pin to `02cb101ec7c4` after reading what moved between the two commits, or
* keep the pin and make the comment say something that cannot go stale — the resolved version or
  the date — rather than a tag that upstream is free to move.

Nothing about this blocks a microstep. It should get a label, a board item and a decision.

**#174 is the only one code alone can close.** The sentence that stood here on 14 September —
*"there is no issue here that code can close"* — became false twice over when #174 and #179 were
filed, and is now false once: **#179 was closed by hand on 15 September** with three of its four
findings unaddressed. They are still true in the code and no longer tracked; the block below
re-measures each and says so.

| # | Title | Prio | Risk | Blocked | Blocks |
|---|---|---|---|---|---|
| 68 | `hardware: buy the reference register, scanner and both printers` | — | — | hardware | `1.2.0`'s deferred half, group 1.7, and four budgets |
| 69 | `decision: the legal entity, its TIN, and ISTD registration for JoFotara` | — | — | decision | **group 2.7, not a Phase-1 microstep** — see below |
| 70 | `decision: a tax adviser's written opinion on the four group-1.3 questions` | — | — | merchant answer | `1.3.4`, `1.3.7`, and the Phase-1 exit gate |
| 71 | `decision: JSMO on trade-scale verification evidence and reverification cadence` | — | — | decision | `1.2.4`'s DB half **and Phase-1 exit demonstration 2** |
| 111 | `decision: does deactivating an approver revoke an already-issued handle?` | P1 | security | decision | the 1.8.x approval handler, so `1.6.4`'s last file |
| 112 | `decision: the three manual discount caps (merchant decisions 3.1–3.3)` | P1 | money path | merchant answer | `1.4.5` |
| **113** | `decision: ICV scope, before migration 0005 freezes it (merchant decision 6.9)` | P1 | migration · compliance | decision | **nothing in Phase 1** — `0005` shipped the `CHECK` on 14 September as option 4, and `1.9.2` now refuses to allocate a `fiscal_icv` number so the reversal window stays open in code. **Reopened 20 September** after being closed while eight documents still carried the question. Binds **`2.7.4`**, which must re-check 6.9 before allocating the first value. §2i has what would settle it |
| 114 | `gap: the agent read-deny blocks the memory directory and workflow resume` | P2 | — | decision | agent memory, workflow resume |
| **174** | `gap: derived Debug prints canonical payloads and digests the never-list redacts` | P2 | security | **not blocked** | nothing — it is a guard, not a gate. **Half done**: `payload` is redacted in `pos-db` (#178) and `pos-sync` (#180). What is left is one decision, below |
| **197** | `gap: three guards 0005 did not ship, riding inside 0006` | P2 | migration | **not blocked**, but it has nowhere of its own to land | nothing. Inherits #179's three surviving findings, each re-measured at `1544c04`. Rides inside `0006` because `0007` is `1.2.5`'s and `verify-schema.py` requires contiguity from `0001`; `1.10.1`'s entry carries the obligation (#198) |

**#69 does not gate a Phase-1 microstep.** Its own body says it blocks *"all of group 2.7 and the
22 ⚠️ OPEN items microstep 2.7.0 owns"*, and `phase-1:1066` says the opposite of gating: *"Owner:
2.7.0 ratifies 6.9 via #69, on a timeline outside this project's control, **so this microstep cannot
wait for it**."* It is a long lead ordered in Phase 1, not a Phase-1 blocker.

**Two divergences nothing reconciles.** Issues **#68, #69, #70 and #71 carry no `priority:` and no
`risk:` label at all**, while the board shows all four as P1, Phase `1 sellable MVP`, and a Risk —
substantive for three of them (#69 `compliance`, #70 `money path`, #71 `compliance`) and the
explicit `none` option for #68. And **#113 carries
two risk labels** (`migration` *and* `compliance`) while the board's single-select Risk field holds
only `migration` — the field is structurally incapable of holding both, and the loss is silent.
Decide which surface is authoritative and make them agree, or stop reading one of them.

### The two issues closed with their substance unresolved — both now put right

**This happened twice in six days, to two different issues.** An issue closed as `COMPLETED` is
read by everyone afterwards as an answered question; neither of these was. **Both were corrected on
20 September** — #113 reopened, #197 filed — and the section is kept because the pattern is the
thing worth remembering, not the two incidents.

> **An issue's state and the plan of record are two surfaces and nothing reconciles them.** No gate
> compares an open `⚠️ OPEN` block against a closed issue, and none ever has. The only thing that
> caught either of these was reading them side by side. **And reopening an issue does not restore
> its board status** — #113 came back as `Done` and had to be set to `Todo` by hand.

**#113 — `decision: ICV scope` — closed 20 September at 09:36Z as `COMPLETED`.** The plan of record
does not agree, in two places:

* `ref/schema.md:4255` still opens with `⚠️ **OPEN — blocks 2.7.0, and carries a standing obligation
  on 2.7.4.**`
* `ref/merchant-decisions.md:152` row 6.9's **Answer** cell is still **empty**. The `**store**` in
  that row sits in the **Default** column — the header at `:130` reads
  `| # | Question | Default | Answer | Lives in | Step |`, and the two are one cell apart. Easy to
  misread; misread once while writing this section, and caught only by going back for the header.

So the question is unanswered in the plan of record whatever the tracker says, and **eight documents
still carry it** — `00-master-plan.md`, `phase-1`, `phase-2`, and `ref/`'s `fiscal-jofotara`,
`merchant-decisions`, `plan-validation`, `schema` and `test-catalog`. That is lesson 1's shape
exactly: one belief, many live sites.

**This makes `1.9.2`'s refusal more justified, not less.** `next()` declines to allocate a
`fiscal_icv` number precisely because the namespace decision is not made; a closed issue does not
make it made, and `2.7.4` still owns the re-check. Either re-open #113, or sweep the eight documents
so they stop asking a question the tracker considers answered — but not neither.

### #179's three surviving items, now tracked by #197

**Closed by hand on 15 September at 08:58Z**, eighteen minutes after #185 merged and seventeen
seconds after the comment recording what it had and had not closed. The pull request did not do it:
`gh pr view 185 --json closingIssuesReferences` returns `[]`, so the wording "Closes the third of
the four findings in #179" did not trip GitHub's parser. It was a deliberate act, and the three
findings below outlived it. **Each was re-measured against the tree at `46e957b`:**

| Item | Still true? | Evidence |
|---|---|---|
| `sale.is_training` has no `CHECK (… IN (0,1))`, so `2` silently disables the fiscal-decision gate | **yes** | `grep -n is_training crates/pos-db/migrations/*.sql` returns four lines, none of them a `CHECK` |
| `receipt_artifact` and `print_attempt` are declared fact tables with no delivery-envelope gate | **yes** | no `*_has_ready_commit` trigger exists for either; the five that have one are `approval_handle`, `approval_consumption`, `audit_log`, `shift` and `shift_close_event` |
| `doc_sequence` has no `DELETE` guard, so a G-2 gapless counter resets to 1 | **yes** | no `BEFORE DELETE ON doc_sequence` anywhere in `0001`–`0005` |

**One part of the fourth item is closed.** #179 also said `doc_sequence` had *no test* — that
`doc_sequence_monotonic` was unexercised and nothing asserted which `scope_kind` values the `CHECK`
admits. `1.9.2` closed the first half: `the_counter_advances_by_exactly_one_or_not_at_all`
(`crates/pos-db/tests/sequence.rs`) asserts the trigger's exact refusal. The second half is covered
at the **Rust** boundary by `invalid_scope_for_sequence_kind_is_refused` and not at the SQL one —
`SequenceRepository` refuses an illegal pairing before a statement reaches the table, so `0005`'s
composite `CHECK` is still never exercised. That is the right layering and it is not the same as
testing the `CHECK`; whoever adds the `DELETE` guard should exercise both from SQL while they are
there.

All three need a migration and **no number is free** — `0006` belongs to `1.10.1` and `0007` to
`1.2.5`, both by name in the phase file — so they ride inside one of those or claim `0008`. That is
unchanged from §2e; what changed is that nothing now carries the reminder. **Either re-file them or
record deliberately that they live only here**, which is the same choice the four ⚠️ OPEN items
below have been waiting on since 13 September.

`1.9.2` closes the *test* half of the third one without any migration at all — see §4.

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

## 4 · What is next — the WIP=1 slot is FREE

**Take one of these, and only one.** WIP = 1 means one microstep, not one open pull request. Every
candidate this file has named since 14 September is spent — `1.9.1` (§2b), `1.1.9`'s database half
(§2c), `1.2.6` (#177), #179's five missing negative tests (#185, §2e), `1.11.6` (#188, §2f),
`1.11.11` (#192, §2g), `1.9.2` (#195, §2h), `1.7.2` (#200, §2i), `1.6.6` (#204, §2j) and
**`1.6.6b`, which this section named as the closest successor and which landed the same day**
(#208, §2k).

**Read this before you pick anything:** a microstep that lands a test `ref/test-catalog.md` names
**will be red on `protected-paths`**, because retiring its `PLANNED` entry means editing
`scripts/check-test-catalog.py` inside the frozen policy surface. That red is the review, the edit
is not optional, and §2k has the recipe. Budget for it rather than being surprised by it —
`1.11.1`, `1.11.12`, `1.2.3`, `1.2.4` and `1.2.5` all carry `PLANNED` names today.

| Candidate | Verdict | Why |
|---|---|---|
| ~~`1.9.2` — `SequenceRepository`~~ | **DONE, #195** | §2h |
| ~~`1.7.2` — font decision and embedding~~ | **DONE, #200** | §2i |
| ~~`1.6.6` — `AuditRepository`~~ | **DONE, #204** | its two documentation prerequisites were authored in its own PR, as predicted; §2j |
| ~~`1.6.6b` — local audit verifier~~ | **DONE, #208** | §2k. It was the first binary target, it took no argument-parser dependency, and it retired the first `PLANNED` entry this repository has ever retired through a microstep |
| `1.2.4` pure half | **blocked** | `ref/schema.md:3410` is an `⚠️ **OPEN` item that names it, and #71 is the issue |
| `1.11.12` — empty and edge states | **blocked** | needs rendered screens that do not exist |

**`1.6.6b` leaves four things behind that the next session should read before choosing.**

* **`1.11.1` — i18n — is the closest thing to a successor, and it has been unblocked and unscoped
  for two days.** `1.7.2` cleared it on 20 September and nobody has looked at what else it needs.
  It has a `Done when` (`phase-1:1256`) and two named tests, one of which —
  `ui_and_rasterizer_resolve_the_same_embedded_font` — **is `1.7.2`'s inherited obligation**: the
  screen and the receipt must resolve the same font file. It is a front-end microstep, so it is
  also the one that moves the JavaScript rows §0 says have been still for two days. Scope it
  before starting it; nothing has.
* **`1.6.7` — capability exhaustiveness — is still behind the IPC wall.** It needs
  `apps/terminal/src-tauri/src/ipc/registry.rs`, and `src-tauri/src` still holds only `lib.rs`,
  `main.rs` and `time.rs`. Same wall as `1.9.3` and `1.11.4`.
* **`1.6.8` — PII scrubbing — looks closer than it is, and it is worth knowing why.** Its two
  files are both new (`telemetry.rs`, `ipc/error.rs`), so the IPC wall does not obviously stop it,
  and it would define the `SENSITIVE_FIELD_RULES` registry **#174 is really asking for**. But its
  own text says the tracing layer, the diagnostic bundle and the telemetry transport all iterate
  that registry, and none of those exists; `apps/terminal/src-tauri/Cargo.toml` has no `tracing`
  dependency either, so adding one is a decision rather than an edit. Scope it honestly or leave
  it.
* **#174 still has no fix and now has a fourth crate's worth of context.** `1.6.6b` printed no
  payload anywhere, so it added no exposure — but the open question it names (is a content digest
  in scope for the `_hash` suffix rule?) is the same one `1.6.6` left, and `AuditIntent`'s derived
  `Debug` in `pos-domain` still prints a payload.

**One thing neither `1.6.6` nor `1.6.6b` closed, and it is worth stating so nobody re-derives it.**
Nothing writes an `audit_checkpoint` row. Anchoring the head is Z close's work, and both
`verify_chain` and `verify-audit` take the anchor as an argument precisely so the storage of one
is a later decision — `verify-audit` reads it from a file, which is the only form that exists
today. `a_z_close_anchors_the_head` (`ref/security-compliance.md:275`) is still owned by nobody in
Phase 1; `phase-2-money-grade.md:347` owns it, and that file's own table calls it Phase 1.

**And #174 gained a third crate's worth of evidence rather than a fix.** `1.6.6` hand-wrote
`Debug` on its one payload-bearing public type, following `outbox.rs` — but it recorded in its
module doc that the argument does **not** transfer unchanged, because `audit_log.payload`'s own
DDL forbids the data `outbox.rs` was protecting. So the audit redaction is a latent guard on a
column whose contract already forbids the content, and `AuditIntent`'s own derived `Debug` in
`pos-domain` still prints a payload. That is #174's third crate, named rather than silently
skipped — which is exactly the omission #179's audit found the first time.

**`1.9.3` is not the successor it looks like.** It reads as the natural next step after `1.9.2` —
receipt numbering consumes the counter this microstep built — but its `Files:` line names
`apps/terminal/src-tauri/src/commands/sale.rs`, and `src-tauri/src/` holds only `lib.rs`, `main.rs`
and `time.rs`. There is no `commands/` module, so `1.9.3` is the step that *creates the IPC command
layer*, which is the same wall `1.11.4` sits behind. Worth knowing before it is picked up as a
small one.

**And #174 is not a microstep at all.** It needs no issue ceremony, no board dance and no frontier
arithmetic — it is a guard on shipped code, unblocked, with the fix shape already written down from
`1.1.9`'s precedent. If the WIP rule is what is stopping you starting something, this sits outside
it.

Per `03-github-workflow.md` §4 the loop is: pick **one** microstep, open **one** `Microstep` issue,
add it to board #4 by hand, set it `In Progress`, then build it.
`.github/ISSUE_TEMPLATE/01-microstep.yml` has **eight required fields**, including a proving command
(*"The command that proves it. Not a description of the command."*) and a *Test-catalog rows closed*
field — so a microstep with no `Done when` line cannot even be filed without authoring one first.

**That loop is one microstep old.** #162, for `1.11.3`, is the first Microstep issue this repository
has ever carried; the twenty before it were built without one. The law was right and unfollowed, so
treat §4's procedure as a new habit rather than an established one — and note that `1.9.1` has not
been filed yet either, which §2b lists as its first item of unfinished work.

### DONE: `1.9.1` — migration `0005`, merged as #170

**All five obligations this section listed were discharged**, and §2b is the record. Briefly, so the
list is not re-derived: `schema.md`'s heading carries `· SHIPPED` in the migration's own commit; the
Postgres mirror exists with its 14-digit name and declaration header; `MIGRATIONS` carries `0005`,
so `user_version` is 5; `reference_blocks_at_or_after(5)` became `6`; and the ICV blocks were
rewritten — at **four** sites, not the three this section predicted. The fourth is
`00-master-plan.md` §4a's `doc_sequence` concordance row, which said the ICV scope was
"store-scoped by default". It stopped being a default the moment `0005` committed.

**Two things the decision did NOT do**, and both survived contact:
`ref/merchant-decisions.md` row 6.9's Answer cell **stays empty** — option 4 is a hedge, and only
the official ISTD package can answer it — and **#113 stays open**. What is settled is what `0005`
does, not what the ICV namespace is.

**The reversal window is now a standing obligation on `2.7.4`.** ICV is never allocated at checkout;
allocation begins there, in Phase 2. Until the first ICV row exists, correcting a wrong `store`
guess is one forward-only migration. After it, it is a migration plus a data repair on a sequence
required to be gapless. All four rewritten blocks now carry that deadline.

The decision also recorded a blast-radius correction nobody had written down: **the ICV question
binds Phase 3 too.** `ref/test-catalog.md:96` row 87 names
`two_offline_registers_never_allocate_the_same_icv`, owned by `phase-3-connected.md:108`. And the
store-scoped default is written into **five** documents — `ref/fiscal-jofotara.md:102`,
`ref/plan-validation.md:274`, `phase-2-money-grade.md:471`, `ref/test-catalog.md:100` and
`ref/schema.md:4255`. A later correction sweeps all five.

### Still true, and now unblocked three migrations deep

`verify-schema.py:259` requires migration numbers contiguous from `0001`, so `0005` was the gate in
front of `0006`, `0007`, `1.2.3`, `1.9.2`–`1.9.5`, `1.10.2`–`1.10.5`, the `1.1.9` DB half and the
`1.2.4` DB half. **That gate is open.** `1.2.3` is still blocked — its FTS repository needs
`0007`'s tables — but it is now two migrations away rather than three.

### The three genuinely small ones — all three are now spent

| Candidate | State |
|---|---|
| ~~`1.2.6` — assert FTS5 at open~~ | **DONE, merged as #177 on 14 September.** Its `Done when` was an outcome and is now a command; one thing it could not prove is recorded there and is worth knowing — **no test can tell whether `open` still *calls* `assert_fts5`**, because on an FTS5-enabled build a checking open and a non-checking one are indistinguishable, and there is no feature flag to build without it. The call site is reviewed, not tested |
| ~~`1.11.6` — global scan capture~~ | **DONE, merged as #188 on 19 September**, with #189 fixing the stale-burst defect an adversarial scope found after it had landed. §2f |
| ~~`1.11.11` — the keyboard map~~ | **DONE, merged as #192 on 20 September.** Its heading is "Keyboard map"; "keyboard reachability" was this file's name for it, and is its test's subject. §2g |

### The other candidates, with what is actually true of each

| Candidate | State |
|---|---|
| `1.2.4` **pure half** | **BLOCKED, and this row said otherwise until 20 September.** `ref/schema.md:3410` is an `⚠️ **OPEN` item whose own text reads "blocks 1.2.4", and #71 is the issue behind it. What follows was true of its *size* and stays useful — the gateway to group 1.4, since `CartLine` needs `PriceOrigin` and `DerivedWeight` and neither name appears anywhere under `crates/`. Big: 14 named tests, a new module, a trybuild pair. Its full `Done when` (`phase-1:301`) chains commands unreachable before `0007`, so the issue's proving command must be rewritten for the half. **Two of the 14 tests are blocked by #71**, whose body says it blocks "`1.2.4`'s database commissioning half". And it defers half of itself with **no `Full-step status:` marker** — unlike `1.1.9` and `1.2.0` — so nothing mechanically stops a premature "complete" claim |
| ~~`1.6.6` — `AuditRepository`~~ | **DONE, #204 on 21 September.** Both documentation prerequisites were authored in its own PR, exactly as this row predicted; §2j is the record, and the defect worth reading is the one the mutation sweep could not reach |
| `1.11.1` — i18n | **UNBLOCKED as of #200**, and the closest thing to a successor after `1.6.6b`. `assets/fonts/` holds both faces and their licence, and `pos_hardware::font` exposes them. Its `Done when` is at `phase-1:1256`; its second test is `1.7.2`'s inherited obligation. Still nothing has re-assessed what else it needs, so scope it rather than start it — and note it carries `PLANNED` names, so it takes §2k's `protected-paths` red |
| `1.11.4` — Lock / PIN | **Soft-blocked.** All four IPC commands it drives are absent — `src-tauri/src` has no `commands/`, and `src/lib/ipc.ts` does not exist. It would be tested entirely against invented mocks |
| `1.11.5` — Sale screen | **Blocked.** `CartSnapshot` does not exist (`packages/api-types/src/index.ts` is `export {};`), and it would rewrite the green `1.11.0` canary |
| `1.3.3` — `compute_line_tax` exclusive | Technically buildable, but **no `Done when` line**; document order puts the externally-blocked `1.3.2` first; both edit the same file |
| `1.3.2`, `1.3.5` | **Blocked, and by nothing anyone filed** — `ref/domain-api.md:1299` and `:1290`. See §3 |
| `1.6.2` — Argon2id PINs | **Blocked twice**, neither time by code: `just bench-gate pin-verify` refuses until #68, **and** `ref/security-compliance.md:413` |
| `1.2.3` | Blocked three migrations deep — its FTS repository needs `0007`'s tables |

**Eighteen executable Phase-1 microsteps carry no `**Done when:**` line at all** — `1.2.0`
`1.3.2` `1.3.3` `1.4.1` `1.4.2` `1.4.3` `1.4.4` `1.4.5` `1.4.7` `1.4.8` `1.4.10` `1.5.1` `1.5.2`
`1.5.4` `1.7.1` `1.7.4` `1.7.6` `1.7.8`. **`1.6.6` left this list on 21 September** by authoring
one as part of its own delivery — the second step ever to clear one rather than add it, after
`1.1.9`. **`1.6.6b` did not change it**: it already had a `Done when` with two commands, which is
why §4 named it the closest successor in the first place. The number is still eighteen; re-run the
snippet below rather than trusting that sentence.

**The number did not fall, and the reason is a counting bug this document carried for a week.**
The list that stood here named eighteen steps *excluding* `1.2.0`, while claiming to be the result
of walking every `### 1.x` heading — and that walk includes `1.2.0`, because
`**Current half done when:**` is not the literal string. So the mechanical answer was nineteen on
the day this said eighteen. `1.6.6` removed one, and the honest count is now eighteen with `1.2.0`
named inside it. Reproduce it rather than trusting it:

```bash
python3 - <<'EOF'
import re, pathlib
s = pathlib.Path("docs/implementation/phase-1-sellable-mvp.md").read_text()
h = list(re.finditer(r"^### (1\.\d+\.\d+[a-z]?) ", s, re.M))
out = [m.group(1) for i, m in enumerate(h)
       for body in [s[m.end(): h[i+1].start() if i+1 < len(h) else len(s)]]
       if "**Concordance only:**" not in body and "**Done when:**" not in body]
print(len(out), out)
EOF
```

It prints `18`, and 113 `### 1.x` headings against 112 executable microsteps — the difference
being `1.1.2`:

* **`1.1.9` gained a real `Done when`** when it completed on 14 September — §1 records that as the
  third of the three deletions its completion required. It is no longer in the list.
* **`1.2.0` is the sole remaining `**Current half done when:**` case.** That is deliberately not the
  literal string the checker matches, and it also carries a `**Full-step status:**` marker, so rule
  3 refuses a completion claim and rule 4 never gets the chance.
* **`1.1.2` has no `Done when` and correctly never will.** The walk turns up 113 `### 1.x` headings
  against 112 executable microsteps, and `1.1.2` is the difference: its own body says
  *"**Concordance only:** this retained anchor is not an executable microstep"*. Anyone re-running
  this count mechanically will find it and must exclude it.

For the eighteen, checker rule 4 refuses a completion claim until one is written, and the issue form
will not accept the microstep without a proving command. **Each is a documentation prerequisite to
its own delivery.**

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

**68 evaluations in the trailing month: 28 bypass, 39 pass, 1 fail** — re-read live on
21 September, and the shape has improved: the pass count nearly tripled while bypasses rose by two.
The endpoint defaults to `time_period=day`, so an unqualified call returns only today's — pass
`time_period=month` or you will conclude the opposite.

**One of the two new bypasses is `1.6.6b`'s, and it is the good kind.** Rule suite
**`4158403946`**, 21 September, `result: bypass`, with **one** failing rule evaluation:
*"Required status check \"protected-paths\" is failing."* Every other required check had
**completed** and passed. That is the frozen-surface review mechanism working exactly as
documented — and it is distinguishable in the ledger from the 13 September entries below, which
read *"6 of 6 required status checks have not succeeded"* and cannot be told apart from a real
override. **When you take this red, make sure the ledger entry names one check.**

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
| `Target` | `PVTF_lAHOCn5KRs4BhoZ-zhgjqZw` | date — **unset on every item, deliberately** |

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
5. **#203 — decide what to do about the moved `dtolnay/rust-toolchain` tag.** The fix is not
   mechanical: either re-pin six sites to `02cb101ec7c4` *after reading what moved between the two
   commits*, or keep the pins and change the comments to something upstream cannot invalidate.
   Either way it is a `.github/workflows/**` edit and therefore a deliberately-red
   `protected-paths` PR, and either way somebody has to look at an upstream diff and decide whether
   to adopt it. Leaving the weekly workflow red is the option with a real cost: the next genuine
   finding arrives as "still failing". §3 has the diagnosis. **The issue also needs a label and a
   board item** — it has neither, which is why it went unnoticed for three merges.

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

`staging` is **45 behind** — `git rev-list --count origin/staging..origin/development`,
re-measured 21 September after #208, and the `origin/` spellings matter: a local `staging` left at
#91 answers 92. The gap now carries **ten** microsteps — `1.11.3`, `1.9.1`, `1.1.9`, `1.2.6`,
`1.11.6`, `1.11.11`, `1.9.2`, `1.7.2`, `1.6.6` and `1.6.6b` — migration `0005`, an embedded
typeface, the audit chain's writer, reader and verifier, a security bump and a code fix (#164)
rather than documentation alone. Open a promotion when you want the cross-platform matrix
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

**One thing to settle before the next promotion:** `workflow-analysis` is red on `development`
today and has been since 21 September (#203, §3). It is **not** one of the six required contexts,
so it cannot wall a promotion — which is exactly why #106 merged with it red. Decide whether to
close #203 first or promote past it deliberately; do not discover it at the merge button.

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
    the next migration would break them all again. **Built and merged as #170;** the helper is
    `crates/pos-db/tests/common/registered_chain.rs`.
48. **The fixture writes what `0005` REQUIRES and nothing more.** It does not seed
    `sale_tax_summary` or `sale_supply_tax_context`: `0005` requires those rows to be in the
    manifest *if they exist*, never that they exist. A fixture that seeded optional facts would
    silently widen what every suite leaning on it asserts, and five suites lean on it.
49. **`0005`'s Postgres mirror is a file that creates nothing, not a `REGISTER_LOCAL` entry.**
    `REGISTER_LOCAL` is keyed by migration **file**, and its contract is "never syncs" — false for
    `0005`, most of which does sync once the server has somewhere to put it. The nine register-local
    tables are named in the mirror's header instead. **Do not re-propose the `REGISTER_LOCAL`
    route**; the last handoff suggested it, and it is the wrong instrument.
50. **`1.1.9`'s database half did NOT ride along in `1.9.1`'s pull request.** `trusted_time_state`
    unblocked it, but it is a separate microstep half with its own `Files:`, its own `Tests:` and
    its own `Done when`, and #170 was already 2,700 lines. It is §4's recommendation instead.
51. **`opening_a_second_shift_for_the_register_is_refused` is named by BOTH `1.9.1` and `1.9.5`,
    and was deliberately left in both.** Unlike the `latin_runs_…` duplicate #165 resolved, these
    sit on different rungs and in different test binaries: `1.9.1` proves the storage guarantee
    (`idx_shift_one_open`, a partial unique index) in
    `tests/migration_0005_sale_columns_and_sequences.rs`; `1.9.5` will prove the repository refuses
    it, in `tests/shift_lifecycle.rs`, which does not exist yet. nextest addresses them by distinct
    binary ids and `check-test-catalog.py` reconciles. If `1.9.5` wants the name alone, that is
    `1.9.5`'s edit to make against a test that exists.
52. **`1.9.1` shipped SEVEN tests where its `Tests:` line names six**, and the line was **not**
    amended — the same call as `1.11.0`'s. The seventh,
    `two_sales_on_one_register_share_its_shift_and_not_their_commits`, holds the property the six
    lean on: one open shift serves both sales, and no id is reused between their commits.
53. **`trusted_time_state` is register-local, so `ClockRepository` upserts and takes no envelope.**
    `ref/schema.md` §"Convergence" names it in the exclusion list. It still takes an explicit
    `&Transaction`, because `repo/mod.rs` makes the caller own the boundary — the transaction
    argument is about *who decides*, not about whether the row is a fact.
54. **An unknown `anomaly_kind` is a hard error, never `None`.** A later migration may widen the
    `CHECK`. Reading an unrecognised variant as "no anomaly" downgrades a register from *something
    is wrong with the clock* to *nothing is wrong with it*, silently, on the one value whose job is
    to be distrusted. `DbError::ClockStateInvalid` names the discriminant, which is a schema enum
    rather than merchant data.
55. **A type holding a field the never-list covers implements `Debug` by hand.**
    `ref/security-compliance.md:511` already required it — *"secret-bearing types implement `Debug`
    and `Display` as a redacted constant"* — and `StoredClock.boot_token` was the first case anyone
    met. **A derived `Debug` is a printing surface**, which `.claude/rules/security.md` names
    explicitly, and `pos-db` having no `tracing` at all does not make it safe: these are public
    types handed to callers this crate does not control.
56. **A redaction test must name what leaking looks like, not what it does not.** The first version
    of `the_boot_token_is_never_printed` asserted that no token byte appeared as a decimal digit
    anywhere in the string; every timestamp is full of digits and it failed on the fixture's own
    `7`. It now builds the exact rendering a derived `Debug` would have emitted, from the value
    itself, so changing the fixture cannot make it vacuous — and the guard was **verified live** by
    restoring the derive and watching it fail.
57. **#174 was filed, not folded into #173.** The same review found the same defect class in
    `outbox.rs`'s shipped `ManifestEntry`. Fixing it there would have put a `1.8.9` file in a
    `1.1.9` diff, which is the scope leak `.github/ISSUE_TEMPLATE/01-microstep.yml` refuses by name,
    and changing a public `Debug` has API consequences worth their own review.
58. **When a guard has two halves, mutate them ONE AT A TIME.** #178's redaction test passed with
    its `ManifestEntry` impl removed; the mutation check had removed both, and `FactMember`'s live
    half masked the dead one. The check looked rigorous and proved half of what it claimed.
59. **A redaction test must search for what leaking looks like after `Debug` renders it.** Raw
    payload text never appears in a `Debug` string — quotes are escaped — so `contains(&payload)`
    cannot fail. Build the rendering a derived `Debug` would emit, from the value itself. This has
    now been got wrong twice and right twice; the working form is in `pos-sync`'s test.
60. **`.is_err()` is not an assertion once a shared fixture exists.** Three I-4 tests were satisfied
    by a tax-category guard and two foreign keys the fixture itself created. **Assert the exact
    refusal message** — the migration's own `RAISE(ABORT, …)` text — which no unrelated constraint
    can produce.
61. **A `file:line` reference into a file every microstep edits has a half-life of about a day.**
    `phase-1:691` was corrected to `:690` on 14 September and was `:691` again on 15 September. Six
    were stale when the audit checked. Quote the text you are pointing at so a reader can grep.
62. **No new commit scopes.** The ten in `scripts/validate-change-title.sh` are enough, and
    `.github/labeler.yml` is the evidence: it already maps `packages/money` to `domain`,
    `packages/ui` and `packages/api-types` to the consuming front end, and `.claude/`, `.codex/`,
    `.agents/`, `.github/` and `Cargo.lock` to `repo`. Scopes for those would contradict a decision
    already written down. The one real gap was `crates/pos-test-support`, which matched **no**
    labeler rule at all — a missing glob, not a missing scope, fixed in #182.


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
| **So does every microstep that lands a test `ref/test-catalog.md` names** | Retiring its `PLANNED` entry means editing `scripts/check-test-catalog.py`, which is in the frozen policy surface. The edit is **not optional** — leaving the entry in place produces two assertion-3 violations. Budget for §9's manual merge; §2k has the worked example. `1.11.1`, `1.11.12`, `1.2.3`, `1.2.4` and `1.2.5` all carry `PLANNED` names today |
| **A scheduled workflow can go red with no repository change, and its issue lands nowhere** | #203 was filed automatically by `security.yml`, carries **no labels and no board item**, and sat open through three merges while every handoff reported "ten open issues". `gh issue list --state open` is the command that finds it; the board is not. Check it when you count |
| **An upstream mutable tag moving makes a correct SHA pin look stale** | zizmor's `stale-action-refs` compares the `# v1` comment against where the tag points *today*. When upstream moves `v1`, six green pins become six warnings and the job exits 13 — with nothing in this repository having changed. Diagnose before re-pinning: the pin is what protected you |
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
| **`just pr` exits 1 on a PR that auto-merges, and the work succeeded** | `watch-pr-checks.sh` refuses with *"PR snapshot changed while watching checks; discard the old check evidence and run this command again"* — it noticed the PR merge out from under it and correctly declined to vouch for evidence that had gone stale. With auto-merge as the standing policy this happens on **every** PR whose checks finish before the watcher does. A red exit meaning "merged successfully" is worth recognising rather than re-deriving: check `gh pr view <n> --json state` before believing the exit code |
| **`clippy::indexing_slicing` is denied workspace-wide, tests included, and provable in-bounds-ness is no defence** | `SCHEDULE[point % SCHEDULE.len()]` and `line_taxes[0]` were both refused, in `1.9.2` and in #185. Neither can panic and clippy does not care. Iterate instead — `.iter().copied().cycle().take(n)` — or take the `Option` from `.first()`/`.get()`. It surfaces in `just lint`'s clippy step, **after** `cargo nextest` has already gone green, so a passing test run says nothing about it |
| **A `.test.ts` can be green in vitest and red in the build** | `vitest` transpiles without typechecking, so `tsc -b` under `just build-web` is the *only* thing that types a test. `tsconfig.app.json` targets **ES2020** with `lib: ["ES2020", "DOM", "DOM.Iterable"]`, so `Array.prototype.at` and anything else ES2022 runs fine and fails the build. Fourteen green tests preceded this discovery. Index instead; do not widen the app's target for a test |
| **`userEvent`'s `delay` is a `setup()` option, not a `type`/`keyboard` option** | Passed to the call it is **silently ignored** and every keystroke lands on the same timestamp, which presents as a scanner that never scans and raises nothing. `userEvent.setup({ delay, advanceTimers: vi.advanceTimersByTime })` puts consecutive `keydown` events exactly `delay` ms apart on the fake clock — measured |
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

An eleventh, on 15 September, scoped #179's five negative tests — twelve agents, 452 tool calls,
zero deaths. It taught one new thing and confirmed the oldest: **a scoping workflow launched before
you start editing will read your edits mid-run.** Three of its refuters reported "nothing in the
repository asserts this message today" against `HEAD` and then refuted *themselves* against the
working tree, because by then the tests existed. Name the commit that is the subject, or scope
before you write. Its genuinely new finding was one line of SQL semantics — `NULL IS NOT <blob>` is
true, so a NULL policy trips two arms of the tax-policy gate at once — which is exactly the class a
wide read finds and a single command does not.

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

- **#184 is an empty commit on `development`, and the cause is worth more than the commit.** The
  handoff branch had already merged as **#183**; a second session compared `HEAD` against a
  **stale local `origin/development`**, saw one commit ahead, and opened and merged #184 for content
  that was already in. `git diff 70934b3 b4776cb` is empty, so nothing regressed — but `development`
  is append-only, so the redundant commit and its duplicate title are permanent. §0's
  `git fetch --all --prune` is the first line of this document for exactly this reason, and skipping
  it costs a commit that cannot be taken back. **`git rev-list --left-right --count` against an
  unfetched remote ref answers a question about yesterday.**
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
- **Your local `staging` is 51 commits behind `origin/staging` and now 92 behind `development`** —
  it still sits at #91's promotion merge (`f2edbb6`), four promotions behind (#106, #108, #130,
  #148). `just promote-staging` without fetching first works from the wrong base. **The number that
  matters is `origin/staging..origin/development`, which is 45**; the 92 is the local ref's answer
  and it is the reason §1's row says to use the `origin/` spellings.
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
- **`pos-db` now has a binary target, and no CI job builds it in release.** `verify-audit` is
  compiled by `cargo check --all-targets` and by `cargo nextest`, both in debug; nothing anywhere
  builds it with `--release`, and its integration suite deliberately **refuses** to run in that
  profile (§2k). That is the right trade today — a release run would reach the machine's real
  credential store — but it means the binary a forensic investigator would actually be handed has
  never been built the way it would ship. Whoever packages it owes that a thought.
- **The `verify-audit` anchor file format is specified in exactly one place that a person will
  read: `--help`.** `ref/security-compliance.md` §4 names the tool but not the file's shape, and
  `phase-1:717`'s entry describes the decision rather than the schema. Microstep `5.4.4` extends
  `--anchor` to a server checkpoint and will need the shape written down somewhere a server author
  can find it. The field names deliberately mirror `audit_checkpoint`'s columns so that extension
  is additive.
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
9. **A control that fires on every merge is not a control; it is a habit.** 28 of 68 ruleset
   evaluations in the trailing month were administrator bypasses. Most were the byte-frozen-surface
   review working as designed — but "most" is the word that makes a ledger necessary, because
   nothing distinguishes the designed reds from the impatient ones except reading them. **`1.6.6b`
   shows what a readable entry looks like**: rule suite `4158403946` names one failing check, with
   every other check completed and passed, where the 13 September entries say "6 of 6 have not
   succeeded" and say nothing at all.
10. **Two right answers can compose into a wrong one, and no mutation sweep asks that question.**
    A sweep asks whether each guard is load-bearing. It cannot ask whether two guards that are each
    correct produce a correct result together. `1.6.6` found the *wrong guard*; `1.6.6b` found a
    *correct stop* and a *correct anchor* answering `Truncated` over rows that were never deleted.
    Both were found by reading the design adversarially after the sweep was green, which is now
    three microsteps in a row where that second pass paid and the sweep alone would have shipped
    the defect. **Run both, in that order, and do not treat a green sweep as the end.**
11. **Follow the static analyser to the line it points at, then look around it.** CodeQL flagged a
    non-sensitive register id in `1.6.6b` — a false positive under this repository's own
    never-list — and three lines below it sat raw, unvalidated file content being printed into a
    document read as evidence. Arguing with the tool would have shipped that. This is the second
    microstep running where a CodeQL alert repaid being followed rather than dismissed.
12. **The document nobody's gate can read is the document that goes stale.** Four documents were
    corrected on 11 September; the one sentence that survived lived two more days in the HTML file
    no checker parses, until a deliberate prose sweep caught it at #163 — along with a second error
    in the same page that no gate had ever read. The sweep, not a gate, is still the only thing that
    reads that file.
