# Handoff — the single current one

**Reflects `development` @ `1ccdd63`, 24 September 2026 — re-measured, not incremented.**

There is one handoff — keep updating this file rather than adding a dated one.

> ## 🔎 THIS EDITION WAS AUDITED, NOT APPENDED TO
>
> Every live claim was re-derived on 21 September rather than carried forward, and **§1 gains the
> view this document has never had**: *"What is LEFT"* — every remaining microstep by group, the
> four missing files that gate seventeen of them, which of them are startable, and a snippet that
> regenerates the whole block. It is derived from the phase file and the filesystem, so it cannot
> drift silently the way the prose around it can. (It read 82 and 40 the day it was written;
> `1.11.1` has since taken it to 81 and 39, which is what a regenerating block is for.)
>
> **The audit found four errors in this file and one in the plan.** All 76 `file:line` references
> were checked mechanically: three had drifted onto the wrong line and two onto blank lines — all
> five repaired. **One was worse than drift: a quotation that does not exist.** §3 argued #69 gates
> nothing by quoting `phase-1` as saying *"…so this microstep cannot wait for it"*; that clause is
> in no file in this repository. The conclusion survives on other evidence and §3 now says so, and
> §15 records why a sentence in quotation marks is the claim nobody re-checks. The plan defect:
> `apps/terminal/src/lib/ipc.ts` is named as a prerequisite by two microsteps and **created by
> none**.
>
> Also re-measured rather than assumed: 30 `[gone]` local branches that day, 31 now (the row said fifteen), the
> ruleset ledger at 28 bypasses of 68, and all four rulesets re-diffed against live by hand — they
> still match.

> ## ✅ 24 SEPTEMBER — `1.5.4` LANDED, #203 IS CLOSED, AND NOTHING THIS FILE CALLED UNTRACKED STILL IS
>
> **Eight pull requests before this edition's own, one microstep, ten issues filed and five closed
> (#203, #230, #234, #236, #241), one spike answered, and the longest-running red closed.** §2r is the
> record.
>
> - **`1.5.4` (#238, issue #236)** takes Phase 1 to **37 of 112 (~33%)**: the dinar's denominations
>   for the numpad and the float and close grids. **Its own first test run refuted a rule its
>   docs-first commit had authored.** *"Every denomination is a whole number of qirsh"* is false
>   for the plan's table, because 25 fils is not, and a 25-fil piece tendered against a remainder
>   rounded to `0.020` is owed five fils that no piece pays. The test and the `Done when` were
>   corrected openly, the values stay the plan's, and **#237** asks the merchant which coins are
>   real. A measured prohibition rides on the function: counting out largest-first fails on 80 of
>   200 qirsh-multiples up to 2.000 JOD, and on none once the 25-fil piece is gone.
> - **#203 is closed (#240)** after being red since 21 September. The upstream diff was read before
>   adopting it: six commits, and the only change our jobs run is `--force-non-host` on two `rustup`
>   calls, which installs nothing different for a host toolchain. `workflow-analysis` passes on the
>   PR **and on `development`** (runs `35972895273` and `35973289230`).
> - **The money-path label reaches the money (#239, closing #230).** `tender*` and `pricing*` are
>   covered, and the dead `discount*` glob is gone.
> - **`PROJECT-GUIDE.md` is tracked (#242),** at the operator's word, as a dated snapshot with a
>   status banner and an errata table of eight claims measured false. It is no longer a loose end.
> - **#234, the WAL-reset spike, is answered (#245), and the answer is that the fix is absent.**
>   The compiled build reads `sqlite_version()` `3.50.4` and `PRAGMA cipher_version`
>   `4.10.0 community`, and SQLite's own §11 puts the fix in `3.51.3`. The fixed SQLCipher (`4.14.0`)
>   is one `rusqlite` bump away (0.40), and **`sqlx-sqlite`'s `libsqlite3-sys <0.38.0` bound blocks
>   that bump**, which is also why Dependabot never proposed it. **#244** tracks the three routes
>   past it. So the one-source-connection rule is a standing constraint, and `00-master-plan.md`
>   lists **six** Phase-1 blockers, not seven.
> - **#241 is closed (#247).** `docs/drills/` exists with its index and record format, ahead of
>   the first drill, and a fifth issue form, **Drill result**, files a result from the lab. `1.7.5`'s
>   native-reader record now has somewhere agreed to land.
> - **Every item this file carried as *"no issue tracks it"* now has one.**
>   - The four ⚠️ OPEN items: **#232** (tax adviser: `1.3.5`, `1.3.2`), **#233** (second factor:
>     `1.6.2`) and **#234** (WAL-reset spike: `1.8.1`).
>   - **#115's remainder** is **#235**.
>   - The two findings from `1.5.3`: **#230** (done) and **#231** (cash-rounding step: policy or
>     store?).
>   - The drill-record gap the old guide found is **#241**.
>   - #203 got its labels and board card before it closed, and #197's card got its fields.
>
> **Both frozen-surface merges took a question first.** The operator read the diffs and approved,
> and each ledger entry names exactly one failing check: **`4206181799`** (#240) and **`4206239586`**
> (#239). #239 was merged second and brought up to date on the server first, so that its entry
> stayed a single check.
>
> **The board reconciles for the first time since 21 September**: 39 items, 22 `Done` and 17
> `Todo`, where 17 is exactly the open issue count. #227's missing item reappeared, so the listing
> had **lagged**, not dropped it.
>
> **Nothing is in flight.** Group 1.5 is 3 of 4, and its last step, `1.5.2`, waits on `Tendering`.
> §4 says what is honestly next, and it is less obvious than it has been.

> ## ✅ `1.5.3` LANDED — CASH ROUNDING, AND THE CHANGE IT REFUSES TO ROUND
>
> `1.5.3` (#228, issue #227) takes Phase 1 to **36 of 112 (~32%)**: cash rounding, the second of
> group 1.5's four steps. §2q is the record.
>
> **The entry was written for a type that does not exist, and that is the thing to carry.** Four of
> its six tests describe a *settlement*: a card leaving a remainder, a final cash tender, change.
> §7's `Tendering` holds a `Cart` and a `PricedCart`, and neither exists. §4 called the inputs "all
> present", which was true of `compute_cash_rounding` and not of the rule above it. So the E.14 rule
> is stated over what *does* exist: `final_tender_rounding(kind, remaining, step, dir)`, whose
> `Option<CashRounding>` is exactly what `Tendering.cash_rounding` holds. *Which* tender is final
> belongs to `1.4.8`, and that obligation is written onto its entry. **Read a step's `Tests:` line, not
> only its signatures, before calling it startable.**
>
> **For one commit, a right conclusion rested on a superseded formula.** The docs-first commit
> justified keying the rule on `is_cash_counted` with master plan C.6's *"− cash rounding given
> away"*. `00-master-plan.md` §4a row 176 supersedes that formula because it *"double-counted cash
> rounding"*, and `ref/domain-api.md` §11 says the opposite: cash rounding carries **no** term,
> because the counted tender's amount already is the rounded amount. It was caught by reading §11
> before the code commit, restated there, and recorded on the issue. The conclusion survived and its
> reason did not. `CLAUDE.md`'s §4a warning describes exactly this case.
>
> **24 mutations, 24 caught.** The first pass showed that forcing `Up` in the cash branch was caught
> by one example only, so the E.14 property now asserts that the rule *delegates* to the arithmetic.
> The read then found three things no mutation could reach:
>
> - the documentation implied the step is a legal-tender claim, which `ref/tax-jordan.md` §5 says
>   it is not;
> - a remainder under half a step is asked as `0.000`, so a final cash tender can be empty, which
>   `1.4.8` must now accept;
> - the C.6 formula.
>
> **`protected-paths` was red by design, and the merge took the bypass after the operator read the
> six-line frozen diff and said yes.** Rule suite **`4196274812`** names exactly one failing check,
> and `ci` run **`35892425713` is a success on the tip**.
>
> **Nothing is in flight.** The WIP=1 slot is free, and `1.5.4` now heads §4's shortlist.

> ## ✅ `1.5.1` LANDED — A TABLE WRITTEN TO MATCH A SEED IT CANNOT CHANGE
>
> `1.5.1` (#224, issue #223) takes Phase 1 to **35 of 112 (~31%)** — the tender vocabulary, and
> the first thing in group 1.5. §2p is the record.
>
> **It is `1.6.3` in reverse, and the direction is the point.** `1.6.3`'s capability grid is *"what
> 1.6.1 writes its seed from"* — grid first, migration second. Here the **seed came first**: the
> complete `tender_type` insert shipped inside `0005` at `1.9.1`, and a committed migration is
> never reopened. So the domain table is written to match `0005:657-662` column for column, the
> test pins all six rows against literals transcribed from the `.sql`, and a disagreement is fixed
> in the **domain type**. Worth knowing before the next microstep whose types a migration already
> describes.
>
> **The distinction a shift closes short without.** `0005`'s own comment: *"`is_internal` is NOT
> implied by `is_cash_counted = 0`: `card` and `cliq` count no drawer cash but do move real value
> through a PSP."* `exchange` must carry both, *"or the offset would appear in expected drawer cash
> on both documents and the shift would close short by twice the exchanged value."* Two flags that
> look redundant and are not, now held apart by a test with `card` and `cliq` as the witnesses.
>
> **20 mutations, 20 caught** — and the read then found a gap this module **inherited**: `as_str`
> and `#[serde(rename_all = "snake_case")]` are the same mapping written twice, coinciding, which
> is exactly when a change to one goes unnoticed. Closed here; **`catalog.rs` and `permissions.rs`
> carry the same unchecked pair**, noted rather than fixed.
>
> **No property test, and that is conventions §5.1 rather than an omission.** Six rows and six
> columns is a bounded claim, and §5.1 says a bounded universal claim uses an exhaustive loop when
> that loop is feasible. `.claude/rules/rust-domain.md` asks for a `prop_` per business rule; the
> unbounded ones here are `1.5.2`'s and `1.5.3`'s and already carry the names.
>
> **`1.7.5` is still the head of §4's shortlist and still needs an answer from a human** — §7 item
> 6. `1.5.1` was taken because it needed one from nobody.
>
> **Nothing is in flight.** The WIP=1 slot is free.
>
> ## ✅ `1.7.4` LANDED — A DOCUMENT THAT CUTS AND NEVER OPENS THE DRAWER
>
> `1.7.4` (#221, issue #220) takes Phase 1 to **34 of 112 (~30%)** — the ESC/POS emitter, so
> `1.7.3`'s page of dots is now a stream a printer would take. §2o is the record. Group 1.7 is
> **4 of 10**, and it has supplied its own successor three times running.
>
> **The test everybody would write first is wrong, and that is the thing to carry.** The drawer
> pulse must never be in a receipt's bytes (`ref/hardware-and-receipts.md` §4: *"the stream is
> persisted and retried; the pulse is a physical, non-idempotent, cash-access effect"*). The
> obvious assertion is to scan the stream for `1B 70` — and it would be **intermittently red**,
> because the raster payload is an arbitrary bitmap and those two bytes occur inside it by
> coincidence about once in every 65 536 pairs, which a 576-dot receipt reaches within roughly 900
> rows. The assertion is structural instead, and one test makes the coincidence deliberate.
>
> **The finding this microstep could not fix: the cut has no feed, and nothing checks where it
> lands.** On many thermal printers the cutter sits ten to fifteen millimetres above the print
> head, so a bare `GS V` severs the paper at a point the document has not reached and the last
> lines stay inside the machine. The distance is a device property — §6a makes `cut_command` a
> profile field for exactly this — and **the matrix ships empty**, so choosing one would be the
> defect §6a.1 refuses. What is missing is not the feed but the **check**: §9's hardware-lab
> checklist covers truncation *across* the paper and nothing covers it *along* the paper. A
> receipt that loses its footer inside the printer passes every check in that table.
>
> **22 mutations, 22 caught** — and the read still found two things the sweep could not, because
> neither was code: an empty page was accepted and emitted a blank tab, and the cut gap above.
>
> **`1.7.4` is the fourth microstep ever to clear a missing `Done when` rather than add one**,
> after `1.1.9`, `1.6.6` and `1.7.1`, and the second to author it in its own commit before any
> code. The list is 16.
>
> **Nothing is in flight.** The WIP=1 slot is free and §4 names what is left — including a
> question for the operator, which §7 now carries.
>
> ## ✅ `1.7.3` LANDED — THE ARABIC PROBLEM, AND THE FONT STACK TWO ADVISORIES CHOSE
>
> `1.7.3` (#218, issue #217) takes Phase 1 to **33 of 112** — the raster pipeline, so a receipt
> that shapes Arabic, keeps a Latin SKU and a Western-digit price the right way round inside a
> right-to-left line, and comes out as dots on both papers. §2n is the record. **The percentage
> does not move**: 32 and 33 share a rounded value, which the last edition predicted. Do not read
> a still `~29%` as a still frontier.
>
> **The plan's dependency is unshippable, and not for the reason the swap started over.**
> `cosmic-text` was rejected first because it brings `fontdb` (enumerates the machine's fonts) and
> `sys-locale` (reads its language) — wrong for a register that must draw with the face `1.7.2`
> embedded. Then `just audit` refused the lighter substitute too: **RUSTSEC-2026-0206** retires
> `rustybuzz` and **RUSTSEC-2026-0192** retires `ttf-parser`, each naming its fontations
> successor. `cosmic-text` 0.19 depends on `rustybuzz`, so **the advisory reaches the plan's own
> choice** — no reading of `1.7.3` could take the phase file literally and pass `supply-chain`.
> `deny.toml` allows a dated exception and none was taken. The stack is `harfrust` + `skrifa` +
> `unicode-bidi` + `tiny-skia`, verified before the swap to produce **identical glyph ids,
> advances and mark offsets**, and it made the suite five times faster as a side effect.
>
> **The adversarial read found four things no mutation could reach, because none of them was
> code.** The line printed only its name — §2.3 is *"name · qty × unit · line total"*; the totals
> block was a column of bare numbers; neither the date nor the business date was printed; and the
> tax summary printed only the tax, where a bi-monthly return needs the base it was charged on.
> All four came from reading `ref/hardware-and-receipts.md` §2.3 **against** the layout. *No
> mutation of code that was never written can fail.*
>
> **The sweep still paid: 43 mutations, 42 caught**, after nine survivors on the first pass — most
> of them guards that were testing the adjacent thing. Two structural vacuities are worth
> carrying: **both paper widths are exact multiples of eight**, so every padding assertion about
> the last byte of a row is untestable through a rendered page; and drawing **no glyphs at all**
> passed, because the separators kept the page from being blank.
>
> **`protected-paths` was red by design and the merge took the ruleset bypass.** Retiring
> `narrow_profile_reflows_rather_than_truncates` edits `scripts/check-test-catalog.py`. §9's
> recipe is missing a flag — see §9 — and the ledger entry names exactly one path.
>
> **Nothing is in flight.** The WIP=1 slot is free and §4 names what is left.
>
> ## ✅ `1.7.1` LANDED — THE RECEIPT MODEL, AND A SETTING IT DELIBERATELY CANNOT HOLD
>
> `1.7.1` (#215, issue #214) takes Phase 1 to **32 of 112 (~29%)** — `ReceiptModel`, the gate to
> the largest unblocked chain left in Phase 1. Group 1.7 is 2 of 10 and `1.7.3` → `1.7.4` →
> `1.7.5` all consume it. §2m is the record.
>
> **The design decision worth carrying is a field that is not there.** `ReceiptLocale` carries no
> `money_decimals`, although §13's inline comment lists it — because two paragraphs below, the same
> section says `money_decimals` *"does not govern a document the customer is handed"*, and
> `ref/hardware-and-receipts.md` §2.3 makes that the first of four rules the renderer may not
> negotiate. The paragraph wins over the comment because it states its reason, and the enforcement
> is structural: **a comment cannot be violated, a field can.** A three-fil rounding line rendered
> at two decimals reads `0.00`, which is money the document hides from the person paying it.
>
> **The sweep paid twice and then the read paid three times more.** 35 mutations, 35 caught — but
> the first pass found a validator that treated a **zero** amount as currency-free and survived,
> because the property's own doc comment claimed `any::<i64>()` covered zero and it reaches zero
> only by chance. A coverage claim written in the same session and never earned. It also found a
> **serialization test whose fixture was half empty**: `#[serde(skip)]` on `masked_pan` round-trips
> unchanged when the fixture leaves it `None`.
>
> **Then the adversarial read found three things no mutation could reach**, all by going back to
> §2.3's four non-negotiable rules rather than to the code: a line with **no name** — §2.3's
> *"never 'unknown item'"* rule in the form a model can break it — which became a fifth validation
> rule; `DocKind::ALL` needing a **compile error** rather than a count, since a sixth variant added
> to the enum and forgotten in `ALL` leaves the coverage test quietly looping over five; and a
> comment claiming the currency error *"names the field that is wrong"* when it named none.
>
> **`1.7.1` is the third microstep ever to clear a missing `Done when` rather than add one**, after
> `1.1.9` and `1.6.6` — and the first to author it in **its own commit, before any code**. The list
> is 17 now. §4 has the snippet; re-run it rather than trusting the number.
>
> **Two plan findings, neither a code problem.** `build_receipt_model` is specified in §13, needs
> `CompletedSale`, and is **owned by no microstep** — the same class as the `lib/ipc.ts` the
> 21 September audit found. And `1.7.3` now inherits the obligation to call `validate`, written
> onto its entry rather than hoped for.
>
> **Nothing is in flight.** The WIP=1 slot is free and §4 names what is left.
>
> ## ✅ `1.11.1` LANDED — THE REGISTER SPEAKS ARABIC BECAUSE IT DECIDES TO, NOT BECAUSE A FILE SAID SO
>
> `1.11.1` (#212, issue #211) takes Phase 1 to **31 of 112 (~28%)** — the i18n infrastructure, gap
> G-5, and the fourth front-end microstep to move the JavaScript rows since `1.11.0` built the
> harness — after `1.11.3`, `1.11.6` and `1.11.11`. §2l is the record.
>
> **The finding worth carrying is what "by default" turned out to mean.** `index.html` has shipped
> `<html lang="ar" dir="rtl">` since the scaffold and `1.11.0`'s harness feeds *that file* to
> jsdom, so an assertion on the document root passes today with **no product code involved at
> all** — which `Sale.test.tsx` had already written down as the thing it could not rule out.
> Meanwhile `useLocale` called `applyLocale` only on a **toggle**. Nothing established direction at
> boot, so an `index.html` regressed to `ltr`, a window served from elsewhere, or a second entry
> point would each have rendered Arabic left-to-right with every test green. §5 ruling 4 reserved
> this for `1.11.1` and it is **discharged**.
>
> **The sweep found an argument, not a bug, and the fix was deletion.** 23 mutations, 22 caught.
> `installLocale` was written with an optional `locale = DEFAULT_LOCALE` that nothing passes, so no
> test could tell it from a constant — removed rather than given a test of its own. The other
> survivor is real and is recorded in the test: point **both** the CSS and `font.rs` at
> `assets/fonts/LICENSE.txt` and this suite correctly reports that the UI and the rasteriser
> resolve the same asset, which is not a font. What refuses it is `1.7.2`, in another crate and
> another runner — **so the guarantee composes across two runners and the `Done when`'s single
> command does not carry it alone.** Measured by applying the mutation, not reasoned.
>
> **An adversarial read then found two gaps no mutation in the first list reached**, and both are
> mutations now: deleting `font-family: var(--font-ui)` while keeping the variable left every
> element on the fallback stack with the suite green, and the numeral guard was `[0-9]` when
> conventions §10 wants Western digits *because* Eastern Arabic-Indic ones confuse a Jordanian
> reader — so `٢` in a catalogue phrase was the worse version of the defect, passing.
>
> **`protected-paths` was GREEN, and §4 said it would be red.** That sentence was wrong: `1.11.1`
> is absent from `check-test-catalog.py`'s `PLANNED` ceiling, while the other four names in its
> list are present. Corrected in §4. All six required checks and all five CodeQL analyses passed
> with no alert.
>
> **Nothing is in flight.** The WIP=1 slot is free and §4 names what is left.
>
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

**`development` is green, tip included.** `just pre-push` exits 0 at `d573662`, the last code-bearing
tip, where all 37 `just guards` steps pass. Four documentation merges followed, #243, #245, #246 and #247, each with
`just pre-push` run by `just pr`. `ci` run **`35981557707` is a success on the tip `1ccdd63`**,
queried by SHA
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

**The WIP=1 slot is EMPTY.** 0 open pull requests and nothing in flight, and no board item is
`In Progress`. The board count is §1's row, and this paragraph no longer carries a copy of it: its copy
read *"24 items — ten `Todo`, fourteen `Done`"* while §1 said 28, which is the shape §1's own
warning about hand-typed numbers predicts. Pick one microstep from §4, file its Microstep issue,
put it on the board, set it `In Progress`, then build it. That loop has been followed every time
since #162, and #236 is its latest turn.

**There are SEVENTEEN open issues, up from eleven, and that is the point rather than a
regression.** On 24 September every item this file had carried as *"no issue tracks it"* was filed:
#231–#235, #237 and #241, with #230 filed and closed the same morning. #203, the one every handoff
from 21 September had missed, was labelled, put on the board and then **closed** by #240. Every open
issue is on board #4. §3 is the list.

**Two open issues can be closed by work rather than by waiting**: #174 (a guard on shipped code) and
#235 (the ruleset drift check). #234 and #241 were the other two, and work closed both the same day
(#245, #247). **Thirteen wait on a person**: a merchant, the adviser, ISTD, hardware, or an operator
decision, now including #244's. **#197 waits on `1.10.1`'s migration `0006`**, where #179's three
surviving items ride.

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
mise exec -- just pre-push          # passes at d573662
```

Nothing is in flight, so there is no branch to resume. All four of 24 September's merged branches
are gone on both sides: #238 and #242 through `just merge`, and #239 and #240 through §9's recipe
with `--delete-branch`. **`phase-1/group-7-raster-pipeline` is still here** as a `[gone]` local branch;
`git branch -D phase-1/group-7-raster-pipeline` removes it when it bothers you.

**One red is left, and it is by design.** `protected-paths` reds on any PR that edits a frozen file,
such as `scripts/check-test-catalog.py` or `.github/**` (§2k). **The weekly `security` workflow is
no longer red.** #240 re-pinned `dtolnay/rust-toolchain` to where its moved `v1` tag points, and
`workflow-analysis` passed on `development` itself after both of that day's `.github` merges. The
first *scheduled* run since then is still to come, and it will also run `scheduled-advisories`,
which reaches the network and can go red with no repository change.

**`just setup` without `mise exec --` fails.** The shell's Node is `v26.4.0`; `.nvmrc` pins
`24.19.0` exactly and the check is fail-closed. This is the first thing that goes wrong every time.

**One operational gotcha, and the last two handoffs both diagnosed it wrongly:** the failure is a
command built in a **shell variable**, nothing else. `CMD="node --version"; mise exec -- $CMD` fails
with `couldn't exec process: No such file or directory`, because the whole unsplit string becomes
`argv[0]`. Pass the words literally. **A `cd` earlier in the same Bash invocation is harmless** —
`cd <repo> && mise exec -- node --version` prints `v24.19.0` — and a relative script path works.
Re-measured 13 September; the `cd` clause that stood here was false.

### Verified gate baselines at `d573662`

Use these as the "nothing is broken" reference. **`1.5.4` moved the Rust row and only that**:
413 → **416**, all three in `crates/pos-domain/src/tender.rs`. `Cargo.lock` is byte-identical, the
JavaScript rows are unchanged at 9 files / 79 tests, and the schema chain did not move. The day's
other four PRs changed `.github/labeler.yml`, one action SHA in four workflows, and added a root
document. None of those moves a row, but they are why the doc-link count is **50**, not 49.

**Every row below was re-measured on the merged tip `d573662`**, with `just pre-push` run on the
tip itself (2:18, exit 0). `just pre-push`'s five, plus
`verify-schema`, `verify-pg` — **real engine pass** through the Docker fallback — `just audit`,
and `bench-gate`, which refuses with exit 3 as it should. `just audit` still reports **135 package
releases and 11 reviewed expressions**: no *third-party* dependency entered either graph, so no
licence or advisory surface moved. **Re-run the one you are about to depend on anyway** — these
are dated observations.

| Command | Reads |
|---|---|
| `just pre-push` | **exit 0** — `lint test build-web guards secrets`, `justfile:368`, ~2:10–2:20 warm on this machine |
| `just check` | exit 0 — all **seven** workspace members |
| `just lint` | exit 0 — 2 prerequisites + 14 body steps = **16 checkers** |
| `just test` | exit 0 — **416 tests run: 416 passed, 2 skipped**; JS **9 files / 79 tests** |
| `just build-web` | exit 0 — `tsc -b` + vite 8.2.2 across 5 packages |
| `just guards` | exit 0 — **37 steps** |
| `just verify-schema` | exit 0 — **5 migrations, 48 tables, 457 columns** |
| `just verify-pg` | exit 0 — **the engine pass RAN** via the Docker fallback (Colima, server 29.5.2) |
| `just secrets` | exit 0 — gitleaks 8.30.1, `--history` |
| `just audit` | exit 0 — cargo-deny clean; **135 package releases, 11 reviewed expressions** |
| `just bench-gate` | **REFUSED, exit 3** — no reference register. Correct, not broken |
| `pnpm --filter terminal exec vitest run` | **7 files, 66 tests**, vitest **5.0.0** |
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

**The three per-package vitest rows must sum to the `just test` row.** They do, and `1.5.4` did
not touch them: **7 + 1 + 1 = 9 files, 66 + 3 + 10 = 79 tests**.

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
| `development` | **`1ccdd63`** — `ci` run **`35981557707` is a success on the tip**, queried by SHA; `just pre-push` last ran on a code-bearing tip at `d573662` (exit 0), and the four documentation merges since each ran it through `just pr`. Carries `1.9.1`, migration `0005`, `1.1.9`'s `ClockRepository`, `1.2.6`, the audit's fixes, #185's seven tests, `1.11.6`'s scan capture with #189's fix, `1.11.11`'s keyboard map, `1.9.2`'s document counters, `1.7.2`'s embedded typeface, `1.6.6`'s audit repository, `1.6.6b`'s `verify-audit`, `1.11.1`'s i18n infrastructure, `1.7.1`'s receipt model, `1.7.3`'s raster pipeline, `1.7.4`'s ESC/POS emitter, `1.5.1`'s tender vocabulary, `1.5.3`'s cash rounding and `1.5.4`'s denominations |
| `staging` | **`531ea04`**, #148's merge — **68 behind** `origin/development` at `1ccdd63`, 5 ahead (its own five promotion merges). Re-measured 24 September with `git rev-list --count origin/staging..origin/development`; **this row has been wrong before and §9 states it independently**, so if the two disagree, run the command rather than picking one. **Use the `origin/` refs**: the local `staging` is still #91's `f2edbb6`, which is how this row once named the wrong commit |
| `main` | `24a0283` — **196 behind** `origin/development` at `1ccdd63`, **133 behind** `origin/staging`, untouched since 20 August |
| Phase 1 | **37 of 112** executable microsteps (~33%) — `1.5.4` (#238) landed 24 September and `1.5.3` (#228) on 23 September, after `1.5.1`, `1.7.4`, `1.7.3` and `1.7.1` on 22 September. **Group 1.1 is closed**; group 1.7 is **4 of 10** and group 1.5 is **3 of 4**, its last step (`1.5.2`) waiting on `Tendering`. The next pair of counts sharing a rounded percentage is 42/43, so every microstep until then moves it |
| Open PRs | **0**, and **nothing is in flight**. The WIP=1 slot is free |
| Open issues | **16**, listed live: #68, #69, #70, #71, #111, #112, **#113 (reopened)**, #114, #174, #197, **#231**, **#232**, **#233**, **#235**, **#237** and **#244**. 24 September filed every item this file had carried as untracked, plus #237, which `1.5.4`'s first test run produced, and #244, which #234's answer produced. **Closed the same day:** #203 (#240), #230 (#239), #234 (#245), #236 (with `1.5.4`) and #241 (#247). **Every open issue is on board #4.** Two can be closed by work alone (#174, #235), thirteen wait on a person, and #197 waits on migration `0006`. §3 is the table |
| Board #4 | **40 items — 16 `Todo`, 24 `Done`**, counted live on 24 September after #247, and **16 `Todo` is exactly the 16 open issues**, the first time that equation has held since #203 was filed on 21 September. **#227's item is listed again**: on 23 September the listing and `items { totalCount }` both omitted it for at least 2h44m while its node read `Done`. So the listing **lagged** rather than dropped it. Count from the listing, but re-count before trusting a figure taken right after an edit. Archived items are excluded |
| Rulesets | **four, all active**, all four checked in under `.github/rulesets/`. **Re-diffed by hand on 21 September: all four still match live** on enforcement, target, conditions, rules and bypass actors. **Not re-diffed since**, so the observation is three days old. No gate does this; **#235** now tracks building one. See §3 |
| Tags / releases | **zero of each.** The append-only tag ruleset has never been exercised |
| Repository | **PUBLIC**, GitHub Free, `OmarSweiti` the sole collaborator (admin) |

### Complete: 37 microsteps

Read live from the frontier region — the block between the `<!-- frontier:begin -->` and
`<!-- frontier:end -->` markers in `docs/implementation/README.md` — in its own order:

`1.1.0` `1.1.1` `1.1.2a` `1.1.6` `1.1.3` `1.1.4` `1.1.2b` `1.1.7` `1.1.5` `1.1.8` `1.2.1` `1.2.2`
`1.3.1` `1.8.9` `1.8.5` `1.11.2` `1.6.5` `1.6.1` `1.6.3` `1.11.0` `1.11.3` `1.9.1` `1.1.9` `1.2.6`
`1.11.6` `1.11.11` `1.9.2` `1.7.2` `1.6.6` `1.6.6b` `1.11.1` `1.7.1` `1.7.3` `1.7.4` `1.5.1`
`1.5.3` `1.5.4`

**Thirteen predictions, thirteen held.** `round(100*37/112)` is 33 and the region says so. **The next
pair that share a rounded value is 42/43**, so every microstep from here to the 42nd moves the
percentage, and the 43rd will not. Computed, not guessed — the full list of colliding counts is
32/33, 42/43, 51/52, 60/61, 69/70, 79/80, 88/89, 98/99 and 107/108.

**The heading above said "28 microsteps" while the list under it held 29 and the region said 29.**
It was a third hand-typed copy of a number two other surfaces already carry, and
`check-implementation-frontier.py` does not read this file. It is 35 now; if you change the list,
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

### What is LEFT: 75 microsteps, and four files that gate a third of them

**Derived, not typed.** Every number below comes from walking `phase-1-sellable-mvp.md`'s `### 1.x`
headings against the frontier region's declared-complete list, and from asking the filesystem
whether each remaining step's named prerequisite files exist. Re-run it rather than trusting it —
the snippet is at the end of this block, and it is the only view in this document that answers
*"what can I actually start"* without reading 1,500 lines of phase file.

| Group | Done | Left | What remains |
|---|---|---|---|
| `1.1` foundations | **11 / 11** | — | **COMPLETE** |
| `1.2` catalogue & scanning | 3 / 9 | 6 | `1.2.0` `1.2.3` `1.2.4` `1.2.5` `1.2.7` `1.2.8` |
| `1.3` tax engine | 1 / 8 | 7 | `1.3.2`–`1.3.8` — **four of the seven are externally blocked**, by two different authorities |
| `1.4` cart | **0 / 13** | 13 | the largest untouched group, and eight of the thirteen wait on one file |
| `1.5` tender | **3 / 4** | 1 | `1.5.1`, `1.5.3` and `1.5.4` landed 22–24 September; `1.5.2` is all that is left and waits on `Tendering` (a `Cart` and a `PricedCart`) — see §4 |
| `1.6` auth & audit | 5 / 9 | 4 | `1.6.2` `1.6.4`(partial) `1.6.7` `1.6.8` |
| `1.7` receipts & printing | **4 / 10** | 6 | three landed on 22 September; `1.7.5` is next and is **partly human-gated** — see §4 — and the group still wants #68's hardware |
| `1.8` storage & lifecycle | 2 / 14 | 12 | `1.8.0`/`1.8.1` first, and `1.8.1` is externally blocked |
| `1.9` documents | 2 / 5 | 3 | `1.9.3` `1.9.4` `1.9.5` — all three behind the IPC wall |
| `1.10` stock | **0 / 5** | 5 | `1.10.1` is migration `0006`, and it carries #197 |
| `1.11` UI | **6 / 19** | 13 | `1.11.1` landed 21 September; most of the rest wait on screens |
| `1.12` seed & sweeps | **0 / 5** | 5 | `1.12.1` is the seeded catalogue four other steps are scheduled behind |

**Three groups have not started at all**: `1.4`, `1.10` and `1.12`. For a day after `1.5.1` started
group 1.5, this sentence still said four and named it. `1.4` is the one to notice: thirteen
microsteps, zero done, and it is the cart.

#### The gates — one missing file, and how many remaining steps name it

| Missing file | Named by | Created by |
|---|---|---|
| `apps/terminal/src-tauri/src/ipc/registry.rs` | **9** remaining steps | `1.6.7` |
| `crates/pos-domain/src/cart.rs` | **8** | `1.4.1` |
| `apps/terminal/src-tauri/src/commands/sale.rs` | 3 | `1.8.3` |
| `apps/terminal/src-tauri/src/commands/cart.rs` | 3 | `1.4.11` |
| `apps/terminal/src/screens/Sale.tsx` | 3 | `1.11.5` |
| `crates/pos-domain/src/pricing.rs` | 2 | `1.4.5` |
| `crates/pos-db/src/repo/stock.rs` | 2 | `1.10.2` |
| `apps/terminal/src/lib/ipc.ts` | 2 | **no step** — the plan defect below |

`crates/pos-domain/src/tender.rs` held a row here, *"3 · created by `1.5.1`"*, for a day after `1.5.1`
created it. The snippet at the end of this block never listed it after that; the table was carried forward
rather than regenerated. It is regenerated now.

**`1.6.7` and `1.4.1` between them unblock seventeen microsteps**, and both are startable today.
That is the strongest argument this document can make about sequencing, and it is the first time it
has been able to make it — nothing here is a judgement, it is a file-existence check.

#### 36 of the 75 are startable by file dependency alone

Startable means *every file its `Files:` line names that is not marked `(new)` already exists*. It
does **not** mean unblocked — seven carry a `**Scheduled in:**` line deferring them behind other
work, and several are blocked by a `⚠️ OPEN` item or an issue that no file can show:

```
1.2.0  1.2.3  1.2.4  1.2.7  1.2.8  1.3.2  1.3.3  1.3.5  1.3.6  1.3.7  1.3.8
1.4.1  1.4.5  1.5.2  1.6.4  1.6.7  1.6.8  1.7.5  1.7.6
1.7.7  1.7.8  1.7.8b 1.8.0  1.8.1  1.8.1b 1.8.2  1.8.3  1.8.4  1.8.6  1.8.8
1.10.2 1.11.13 1.11.14 1.11.15 1.12.3 1.12.4
```

Subtract what the rest of this document already knows: `1.2.0` and `1.2.7` and `1.12.3` wait on #68's
hardware, `1.2.4` on #71, `1.3.2`/`1.3.4`/`1.3.5`/`1.3.7` on two OPEN items and #70, `1.6.2` on #68
*and* an OPEN item, `1.8.1` on an OPEN item nobody filed, and seven carry `Scheduled in:`. **What is
left after that subtraction is small, and `1.4.1`, `1.6.7`, `1.6.8` and `1.7.5` are the names on
it**. None of them is clean: `1.7.5` needs a human reader, `1.4.1` needs types two blocked steps
own, and `1.6.7`/`1.6.8` each need a scoping decision. §4 spells out all four.

**`1.5.1` raised this count rather than lowering it, and the reason is the trap this block already
warns about.** Creating `crates/pos-domain/src/tender.rs` made `1.5.2`, `1.5.3` and `1.5.4`
file-startable at once, so 36 became 38. Only two of the three really are: `1.5.3`'s
`compute_cash_rounding` takes a `Money`, a step and a direction, and `1.5.4`'s denominations take
nothing — but **`1.5.2` needs `Tendering`, which holds a `Cart` and a `PricedCart`**, and neither
name exists under `crates/`. Same shape as `1.4.1`: the file exists, the types do not.

**`1.5.3` then lowered the count by exactly one, to 37, and showed a third shape of the same
limit.** Its *signatures* were startable, as the paragraph above said. But four of its six *tests*
describe a settlement over the `Tendering` that does not exist. It shipped by stating the rule over
the types that do exist, and §2q records how. So before calling a step startable, check which types
its `Tests:` line needs, as well as which files its `Files:` line names.

**`1.5.4` lowered it by one more, to 36, and was the cleanest start in weeks**: a constant table,
three tests, no type it needed was missing. It still produced a merchant question (#237), because its
first test run showed that the plan's data disagreed with the plan's own cash step.

**`1.4.1` is on that list by file existence and is not as startable as it looks**, which is worth
one line here because §4 has been recommending it. Its `Files:` line names one new file, so the
check passes — but `CartLine` is typed in terms of `PriceOrigin` and `DerivedWeight` (`1.2.4`,
blocked by #71) and `LineDiscount`, `BasketDiscount` and `PriceOverride` (`1.4.5`). Measured:
`grep -rn --include='*.rs' PriceOrigin crates/` returns **0**, as does `DerivedWeight`,
`LineDiscount`, `BasketDiscount`, `PricedCart`, `Tendering`, `CartContext` and `PromoGroupRef`.
That is the file-existence check's known limit doing exactly what this block warns about, and it
is why `1.11.1` was taken ahead of it.

#### Two plan defects this view found, and neither is a code problem

* **`apps/terminal/src/lib/ipc.ts` is a prerequisite nobody creates.** `1.11.4b` and `1.11.9b` both
  name it, neither marks it `(new)`, it does not exist, and `grep -rn 'lib/ipc.ts'` across the whole
  plan returns exactly those two lines. Whoever picks up either step writes it, or the plan gains a
  step that does.
* **The `(new)` marker is applied inconsistently, so creation ownership is not mechanically
  derivable.** `1.11.5`'s `Files:` line reads *"`…/Sale.tsx` and components, `…/Sale.test.tsx`"* —
  the marker lands on the test file and not on the screen, and `1.2.5`, `1.11.4`, `1.11.7` and
  `1.11.8` do the same. So a tool cannot distinguish *creates* from *edits*, and the table above was
  finished by hand for five files. Worth fixing the day anybody wants this view generated rather
  than written.

#### Reproduce the whole block

```bash
python3 - <<'EOF'
import re, pathlib
root = pathlib.Path(".")
phase = (root/"docs/implementation/phase-1-sellable-mvp.md").read_text()
readme = (root/"docs/implementation/README.md").read_text()
region = readme.split("<!-- frontier:begin phase=1 -->")[1].split("<!-- frontier:end -->")[0]
done = set(re.findall(r"`(\d+\.\d+\.\d+[a-z]?)`", region))
h = list(re.finditer(r"^### (1\.\d+\.\d+[a-z]?) — ", phase, re.M))
rem = []
for i, m in enumerate(h):
    body = phase[m.end(): h[i+1].start() if i+1 < len(h) else len(phase)]
    if "**Concordance only:**" in body or m.group(1) in done: continue
    fl = re.search(r"^\*\*Files:\*\* (.*)$", body, re.M)
    need = []
    if fl:
        line = fl.group(1)
        spans = [(x.start(), x.end()) for x in re.finditer(r"\(new[^)]*\)", line)]
        for mm in re.finditer(r"`([^`]+)`", line):
            p = mm.group(1)
            if "/" not in p or p.endswith("/"): continue
            if any(a <= mm.start() <= b for a, b in spans): continue
            if line[mm.end():mm.end()+6].lstrip().startswith("(new"): continue
            if not (root/p).exists(): need.append(p)
    rem.append((m.group(1), need))
print(f"complete={len(done)} remaining={len(rem)} startable={sum(1 for _, n in rem if not n)}")
from collections import Counter
for p, n in Counter(x for _, ns in rem for x in set(ns)).most_common(8):
    print(f"  {n:>2}x {p}")
EOF
```

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
| `crates/pos-db/tests/migration_0005_sale_columns_and_sequences.rs` | the six tests `phase-1:1088` names, plus one more (below) |
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

`0006` is named by `1.10.1`, the stock ledger (`phase-1-sellable-mvp.md:1142`), and `0007` by
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
on 20 September from the three `phase-1:769` names. It unblocks `1.11.1`.

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
  stores (`ref/schema.md:2652`). The short-form `⚠️ OPEN` blocks at `fiscal-jofotara.md:102`,
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

`ref/security-compliance.md:290` attributed `verify-audit.rs` to microstep 5.4.4. `phase-1:717`
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

## 2l · 21 September — `1.11.1`, and the difference between a default and an attribute

**#212 merged `1.11.1`** — `packages/ui/src/i18n.ts` (the shared locale contract, and that
package's first real content), `apps/terminal/src/i18n/` (two catalogues, `t`, `installLocale` and
eight tests), `apps/terminal/src/styles/font.css`, and the `main.tsx` line that applies the locale
before the first render. Issue #211 is the microstep issue, filed before the branch and closed by
the PR. Phase 1 is **31 of 112 (~28%)** and the JavaScript rows move **8 files / 71 tests → 9
files / 79 tests** — the fourth front-end microstep to move them, after `1.11.3`, `1.11.6` and
`1.11.11`.

**All three conditions of the `Done when` were run, not inferred.**

### The defect was in what "by default" had been allowed to mean

`1.11.1`'s `Done when` asks for *"Arabic as the rendered default"*, and §5 ruling 4 had already
written down the trap: *"supplying `dir="rtl"` to jsdom is a test-fixture fact, not that
deliverable."* The trap is deeper than it reads. `index.html` has shipped
`<html lang="ar" dir="rtl">` since the scaffold, `1.11.0`'s harness feeds **that file** to jsdom
on purpose, and `Sale.test.tsx` already asserts on the root and passes. So the obvious test was
green before a line was written.

What was actually missing: `useLocale` seeds `DEFAULT_LOCALE` into React state and calls
`applyLocale` only when somebody **toggles**. Nothing in the product established direction at
boot. An `index.html` regressed to `ltr`, a Tauri window served from somewhere else, or a second
entry point would each have rendered Arabic left-to-right with the whole suite green.

`installLocale` is that decision and `main.tsx` makes it before `createRoot`. Two tests, kept
separate on purpose: one drives the function against a plain root saying `en`/`ltr`, and one
**imports `../main` itself** against a document flipped the same way. Deleting the line from
`main.tsx` while keeping the function reds exactly one of them, which is the only arrangement that
tells you *which* half broke.

**Unlike `1.2.6`, this call site is provable, and the contrast is the reusable part.** That
microstep could not test its own: on an FTS5-enabled build a checking `open` and a non-checking
one are indistinguishable, so the call is reviewed. Here the effect is observable on the document
root, so it is tested. *"The call site cannot be tested"* is a claim to check rather than inherit.

### The sweep: 23 mutations, 22 caught, and the survivor is a composition

The first pass left two survivors and both had been predicted by reading the design rather than by
running anything.

* **`installLocale` ignored its argument.** It was written with an optional
  `locale = DEFAULT_LOCALE`; nothing passes one, because the toggle goes through `useLocale` and
  calls `applyLocale` directly. **Fixed by deleting the parameter**, not by writing a test for an
  argument with no caller. Worth stating as a rule: a sweep that finds an untested parameter is
  usually telling you the parameter is speculative, and the cheaper of the two repairs is the one
  that removes surface.
* **Both sides embedding the licence instead of a face.** Point the CSS *and* `font.rs` at
  `assets/fonts/LICENSE.txt`: the two sets agree, the file exists, and this suite correctly
  reports that the UI and the rasteriser resolve the same asset. It is not a font. What refuses it
  is `1.7.2` in another crate and another runner — `font_asset.rs` pins both face *names* against
  the compiled bytes and asserts the TrueType `sfnt` magic. **Measured, not reasoned:** the
  mutation was applied and `cargo nextest run -p pos-hardware` failed on
  `the_committed_faces_match_the_compiled_ones`.

  So the guarantee **composes** — paths equal here, bytes proven a real face there — but it
  composes across two runners, and the `Done when`'s single vitest command does not carry it
  alone. Re-deriving the byte check in TypeScript would be a second hand-maintained copy of a
  fact, which is the worse of the two failures. It is recorded in the test instead.

### The adversarial read found two gaps the first mutation list did not reach

Both are now mutations 19 and 21, and both are the same shape: a guard that was checking the
adjacent thing.

* **Declaring a family is not asking for it.** Deleting `font-family: var(--font-ui)` from `:root`
  while leaving the variable in place puts every element on the fallback stack, and the test that
  checked the variable stayed green. Both halves of the binding are asserted now.
* **`[0-9]` was the wrong numeral guard.** Conventions §10 mandates Western Arabic digits
  *because* Eastern Arabic-Indic ones confuse a Jordanian reader more than they serve — so `٢`
  inside a catalogue phrase is the **worse** version of the defect the rule refuses, and it was
  passing. The guard covers `\u0660-\u0669` and `\u06F0-\u06F9` now.

### Two facts established by throwaway probes, both easy to get wrong

* **A test under `apps/terminal/src/**` cannot read a file.** `tsconfig.app.json` carries no
  `"types": ["node"]`, so `import { readFileSync } from "node:fs"` in `src/` fails `tsc -b` with
  **`TS2591`** — and `just build-web` is the only thing that typechecks a test. §5's trap list
  already said where the reading goes (*"read repository files in `vite.config.ts`, which
  `tsconfig.node.json` already types"*); what it did not say is how the result reaches the suite.
  The answer is **vitest's `provide`/`inject`** with a `declare module "vitest"` augmentation,
  verified end to end against vitest 5.0.0 before the branch existed: `test.provide` typechecks as
  `Partial<ProvidedContext>` and both `tsc -b` and the runner are green.
* **Font URLs may reach out of the Vite root, and Vite still bundles them.**
  `../../../../assets/fonts/…` from `src/styles/font.css`, rather than a copy under `public/`,
  because a copy would be a *second* file a later `1.7.3` change could diverge from silently.
  `just build-web` emits `IBMPlexSansArabic-Regular-CjFS_rN4.ttf` at 236.70 kB and
  `IBMPlexSansArabic-Bold-Bid7JPYR.ttf` at 247.89 kB — the two on-disk sizes exactly. No network
  font; conventions §10 holds.

### Three smaller things, each worth one line

* **`packages/ui` has content for the first time**, and needed `"types"` in its manifest for a
  `moduleResolution: bundler` consumer to find the declarations — `@pos/money` already does this.
  It has no test runner of its own and deliberately gains none: every symbol is exercised from
  `apps/terminal`, where the consumer and the DOM both are.
* **`src/lib/direction.test.ts` was not edited and stays green.** After the primitives moved to
  `@pos/ui` it proves the terminal's *binding* to the shared law rather than a local copy of it,
  which is the stronger thing for it to prove. That is why re-export was chosen over a rewrite.
* **The catalogue's ten keys come from `ref/ui-spec.md` §3's Sale-screen diagram**, not from the
  Phase-0 smoke panel. Conventions §2's own worked example, `sale.action.park`, is one of them.
  `App.tsx` is deliberately **not** converted: its strings are scaffolding `1.11.5` deletes, and
  conventions §6 rule 6 keeps a microstep inside its `Files:` list.

### The `Files:` line was three entries and the work touched twelve

Amended in the PR, in the phase file's voice, with a reason against each addition. The one that
matters most is **`README.md` (implementation frontier)**: `check-implementation-frontier.py`
refuses a completion claim whose region was not rewritten, `1.11.3`'s line names it, and
`1.11.1`'s omitted it. That is a plan oversight rather than a difference in the work, and the same
omission is worth checking on the next microstep's line before starting it.

### What `1.11.1` did not do

No lint for string literals in components. Conventions §10 calls one a lint failure and none
exists — `check-logical-css.sh` polices CSS sides and knows nothing about strings. That is a real
gap with no owning microstep, and it belongs to a checker rather than to the step that made
compliance possible for the first time. Nothing here reads a stored locale preference either;
there is no storage yet, which is why `installLocale` takes no argument.

---

## 2m · 22 September — `1.7.1`, and three guards that were checking the adjacent thing

**#215 merged `1.7.1`** — `crates/pos-domain/src/receipt.rs` (1,101 lines: `ReceiptModel`, its
twelve blocks, `ReceiptError` and sixteen tests) and six `lib.rs` re-exports. Issue #214 is the
microstep issue, filed before the branch and closed by the PR. Phase 1 is **32 of 112 (~29%)** and
the Rust suite is **324 → 340**.

**The `Done when` was run**: `cargo nextest run -p pos-domain receipt::` is 16 tests in 0.17 s.

### The field that is not there

`ReceiptLocale` carries language and direction and **no `money_decimals`**, although §13's inline
comment reads `// language, direction, money decimals`. Two paragraphs below that comment, §13
says `money_decimals` *"does not govern a document the customer is handed, because a receipt whose
visible rows do not add to its own total is not proof of anything"*, and
`ref/hardware-and-receipts.md` §2.3 makes it the first of four rules the renderer may not
negotiate — every money field through `Money::format_exact`, *"with the store's `money_decimals`
nowhere in the path."*

The paragraph wins over the comment because it states its reason. What matters more is **how** it
wins: the field does not exist, so no renderer can read it. That is the same move `ref/ui-spec.md`
§1 makes for `MoneyDisplay` — *"has no precision argument at all; there is no code path in which a
settings value can reach it"* — and it is the only version of the rule a future `1.7.3` cannot
break by accident. **A comment cannot be violated; a field can.**

### The order the two reviews fired in, for the fourth microstep running

**The sweep found two things, and both were claims this session had written down and not earned.**

* A validator reading `a.minor() != 0 && a.currency() != expected` — treating a zero amount as
  currency-free — **survived** `prop_a_model_mixing_two_currencies_is_refused`, whose own doc
  comment claimed the generator covered *"the full `i64` minor range including zero"*.
  `any::<i64>()` reaches zero only by chance. Conventions §5.1 already answers this: a bounded
  universal claim gets an exhaustive `#[test]` loop, not a generator that might. Not theoretical
  either — a zero-amount tender is how a fully discounted basket settles, and a zero tax row is
  every exempt supply.
* `#[serde(skip)]` on `masked_pan` **survived a round-trip test**, because the fixture left
  `masked_pan`, `scheme`, `watermark`, `fiscal`, `change` and the rounding line at `None`, and
  `None` round-trips through a skipped field unchanged. **A serialization test whose fixture is
  half empty tests half the struct.**

**Then the adversarial read found three more, and none of them is a shape a mutation can produce.**
All three came from re-reading §2.3's four non-negotiable rules *against* the validator rather than
reading the validator:

* **A line with no name.** §2.3's second rule is *"a department line prints the department's name,
  never 'unknown item' — a customer's proof of purchase has to describe what they bought."* The
  builder chooses which name; what this type can refuse is none at all, which is the purest form of
  the same failure. It became the fifth validation rule.
* **`DocKind::ALL` needed a compile error, not a count.** `every_doc_kind_is_covered_by_the_validator`
  loops over `ALL` and guards itself with `filter(may_carry_fiscal).count() == 2`. Add a sixth
  variant to the enum and forget `ALL`: the loop quietly covers five and the guard still holds. An
  exhaustive `match` closes it — 1.4.10's move for audit intents — and `may_carry_fiscal` became a
  full `match` rather than `matches!` for the same reason, so a new variant cannot silently default
  to "not an invoice". That default is the *safe* answer and not a *decided* one, and the variant
  the method exists for — `Acknowledgement` — arrived exactly as a decision somebody had to make.
* **A comment claimed the error named a field, and it named none.** `check_one_currency`'s doc said
  a mismatch *"names the field that is wrong"*; `MixedCurrency` carried two currencies and nothing
  else. Fixed in the direction of the comment rather than by deleting it, because an operator
  handed *"this receipt mixes JOD with USD"* has to read every amount on the document to find out
  which one.

**Final sweep: 35 mutations, 35 caught.**

### #174's fourth crate, and the first one that pays

`BuyerBlock.name` is `buyer_name`; `MerchantBlock.phone` is `phone`. Both are on
`ref/security-compliance.md` §6's registry, which redacts **at any nesting depth**, and §6's own
table names *"Panic payloads and `Debug` output"* as a channel the `tracing` layer cannot reach.
`Debug` is hand-written on both — a deviation from §13's derive list, with the reason in the module
doc, following `outbox.rs` and `StoredClock.boot_token` (#173).

Three judgements, stated rather than assumed, because each could be reversed by a later sweep that
pattern-matched instead of reading:

* **`masked_pan` stays visible, and a test holds that direction.** The registry names `pan` and
  `card_number`; a masked value is neither and matches no suffix rule, and
  `.claude/rules/security.md` explicitly permits storing *"the masked PAN the terminal returns for
  the receipt"*. Redacting it is over-correction past a reviewed rule, and it removes the one field
  a tender mismatch is diagnosed by.
* **`LoyaltyBlock` carries a balance and no name**, because §2.3 asks for a balance and
  `customer_name` is on the registry — the type simply has nowhere to put one.
* **Neither block implements `Display` at all.** §6's control names `Debug` *and* `Display`. The
  absence of the second is the safe state rather than half a control, and the module says so where
  whoever adds a `Display` will read it.

`a_receipt_round_trips_through_canonical_json` asserts the buyer's name and merchant's phone
**survive** JSON. Serialization is the data path, not a log, and that assertion is what makes the
two `Debug` tests a redaction rather than a deletion.

### Two plan findings, neither a code problem

* **`build_receipt_model` is owned by nobody.** §13 specifies it, it needs `CompletedSale` from
  `1.4.1`, and `grep -rn build_receipt_model docs/implementation/` returns exactly one line — the
  reference. No `Files:` or `Tests:` line claims it. Same class as the `apps/terminal/src/lib/ipc.ts`
  the 21 September audit found named by two microsteps and created by none. It is explicitly **not**
  a half-delivered `1.7.1` and takes no `**Full-step status:**` marker: the heading is
  `ReceiptModel`, the body describes the model, the `Files:` line named one file, and §13 splits
  across microsteps exactly as §7 does — `1.5.1` owns `TenderType` and `Tender` while `Tendering`
  and `remaining_due` are `1.5.2`'s, and nobody calls `1.5.1` partial for it. Written into the
  phase file where `1.4.1` and `1.8.3` will find it.
* **`1.7.3` inherits a call.** `validate()` is a method, not a private constructor, because §13
  specifies a plain struct with public fields and a render model is transparent to its three
  consumers by design; redesigning a specified public shape to enforce something the reference did
  not ask for is the larger error. So nothing forces the call, and the obligation is written onto
  `1.7.3`'s entry — as `1.7.2` wrote `1.11.1`'s. **Unlike `1.2.6`'s `assert_fts5`, this call site
  is testable** once its caller exists; it is a deferred test rather than an untestable one.

### Three smaller things, each worth one line

* **`TaxSummaryRow` is reused, not redefined.** Its own doc comment already calls it *"One
  receipt-summary row"*, so the per-rate summary is 1.3.1's type; a second way to group the same
  numbers is a second answer to "what tax was charged". It is also why `ReceiptTotals` and
  `ReceiptModel` derive `PartialEq` **without** `Eq` — that is §13's derive list and `tax.rs`'s
  house rule, and reaching into another microstep's type to add a bound for this one's convenience
  is the wrong direction of change.
* **`1.7.1` cleared a missing `Done when` rather than adding one** — the third step ever, after
  `1.1.9` and `1.6.6`, and the first to author it in **its own commit before any code**, so the
  order conventions §6 requires is on the record rather than asserted. The list is 17 now.
* **The `Tests:` line authored up front named ten; sixteen shipped.** The six additions are all
  traceable to the two reviews above, and the third commit says which came from where. That gap is
  the honest shape of authoring a completion condition first: it is a floor, not a forecast.

### A harness bug that would have reported a phantom survivor

The sweep restored each mutated file with `shutil.copy2`, **which preserves mtime**, so cargo's
freshness check reused the *mutated* binary after a restore. Every per-mutation result was still
valid — writing a mutation bumps mtime forward — but the final "the suite is green again"
assertion ran a stale build and reported two failures that did not exist. Diagnosed by `touch`ing
the file and re-running.

Worth carrying because the failure mode is worse in a different order: a harness that restores with
`copy2` and then runs the *next* mutation against a stale binary would report **caught** for a
mutation that never compiled. Touch the file you restore.

---

## 2n · 22 September — `1.7.3`, and the dependency two advisories chose

**#218 merged `1.7.3`** — `crates/pos-hardware/src/render/` (`mod.rs`, `layout.rs`, `raster.rs`;
38 tests) and four new dependencies. Issue #217 is the microstep issue, filed before the branch
and closed by the PR. Phase 1 is **33 of 112**, and the percentage holds at ~29% by arithmetic.
The Rust suite is **340 → 378**.

**The `Done when` was run**: `cargo nextest run -p pos-hardware render::tests::` exits zero for the
embedded font, contextual Arabic glyphs, RTL run order and both raster widths.

### The plan's dependency cannot ship, and the second reason is the one that matters

The entry names `cosmic-text`. It was rejected first on what it brings: **`fontdb`**, which
enumerates the machine's fonts, and **`sys-locale`**, which reads its language — 55 crates against
20, measured in a throwaway crate before the branch existed. A register draws with the face
`1.7.2` embedded, which `1.11.1` spent a cross-language test proving the screen resolves too, and
it takes its language from the `ReceiptModel`. A stack that can silently answer either question
from the host is the wrong one for this device.

So the first draft used `rustybuzz` + `ttf-parser`, the crates `cosmic-text` uses underneath.
**`just audit` refused it, twice:**

* **RUSTSEC-2026-0206** — `rustybuzz` unmaintained; the advisory names `harfrust`, from the
  HarfBuzz project itself.
* **RUSTSEC-2026-0192** — `ttf-parser` unmaintained; the advisory names `skrifa`, from Google's
  fontations project.

One story rather than two: the RazrFalcon font stack has been retired in favour of fontations. And
**`cosmic-text` 0.19 depends on `rustybuzz`**, so the advisory reaches the plan's own choice as
well — there was no reading of this microstep that could take the phase file literally and pass
`supply-chain`. That is worth knowing before the next microstep quotes a crate name from a
reference written months ago.

`deny.toml` permits a dated exception and none was taken. Adopting a dependency already flagged on
the day it arrives, on the path that draws every document a merchant hands a customer, is
permanent debt — and `1.7.5` is about to pin goldens to whatever this produces. **Measured before
the swap, not after:** `harfrust` returns the identical glyph ids, advances and mark offsets for
the pinned Arabic word, so the migration was verifiable rather than hopeful.

**Two side effects worth carrying.** `harfrust` is roughly twenty times faster than `rustybuzz` in
a debug build — the render suite went from 45 s to 5.6 s and a budget problem evaporated. And
`harfrust` requires `guess_segment_properties()` where `rustybuzz` guessed on its own: without it
the shaper plans for no script and returns the **isolated** form of every letter, so `بيع` comes
back as three unjoined glyphs. The old crate was hiding a requirement.

### The read found four things the sweep structurally could not

All four were found by reading `ref/hardware-and-receipts.md` §2.3 **against** the layout rather
than reading the layout, and none of them is a shape a mutation can produce — *no mutation of code
that was never written can fail.*

* **The line printed only its name.** §2.3 is *"name · qty × unit · line total"*. A receipt that
  says what was bought and not how many or for how much is not a proof of purchase.
* **The totals block was a column of bare numbers**, every amount against an empty label. The
  words are this crate's, not `apps/terminal/src/i18n`'s: paper is read by a customer and a tax
  authority rather than by a user interface, and wiring it to the front end would make the printed
  document depend on a package that cannot load without a webview.
* **Neither the date nor the business date was printed**, and §2.3 asks for *"date & time"*. They
  differ for a sale after midnight (conventions §11), and a customer returning goods and an
  auditor placing a document in a period need different ones.
* **The tax summary printed only the tax**, where §2.3 wants net / tax / gross per rate — the row a
  bi-monthly return is reconstructed from needs the base an amount was charged on.

### The sweep: 43 mutations, 42 caught, and two structural vacuities

Nine survived the first pass. Most were guards testing the adjacent thing; two are worth keeping
because they are properties of this problem rather than of this code:

* **Both paper widths are exact multiples of eight** (576 and 384), so every assertion about the
  padding dots in the last byte of a row is untestable through a rendered page. A stride computed
  as `/ 8` where `div_ceil(8)` belonged survived everything. The repair was one definition of the
  stride rather than a better test.
* **Drawing no glyphs at all passed**, because the horizontal separators are drawn separately and
  kept the page from being blank. `ink() > 0` is not an assertion that anything was *set*. A long
  receipt now has to carry more ink than a short one.

Three more worth a line each: dropping the y-flip survived twice, because rules keep ink on the
page and because Arabic is full of descending strokes — the assertion is the **balance**, 1 234
dots above the baseline against 327 below when it is right and 148 against 1 340 when it is not.
Dividing by a literal `1000` instead of `units_per_em` survived because this font's em *is* 1 000.
And the mark-offset mutation survived because unvocalised Arabic has no marks; `بَيْع` has two.

**The one survivor is recorded beside the code**: removing the one-em gutter between a label and
its amount loses no data and breaks no test, and legibility on paper is judged by `1.7.5`'s
goldens under a native reader's eye.

### Two defects the properties found, and two measurements taken wrongly first

* **A glyph at x = −48.** The fiscal block's 36-character UUID has no space in it, does not fit a
  58 mm line, and was centred — so it hung off *both* edges, and dots past the paper are not
  clipped, they are never drawn. Over-long words break at a character boundary now.
* **An over-wide *amount* was never wrapped at all**, found by the collision test after the label
  half was fixed. A row whose amount alone exceeds the content box becomes two paragraphs.
* **The narrow profile does not set smaller type.** A per-profile size was invented so the receipt
  would keep its shape; the effect was that a long name took the *same* number of rows on both
  papers, because the narrow measure and the smaller type cancelled exactly. Neither reference
  specifies a size per paper, so the invention went.
* **The reflow test then measured the wrong thing twice** — whole-receipt heights (811 against 986,
  the *narrow* one shorter) and then whole-receipt rows (31 against 31, the totals block's own
  wrapping cancelling the other way). It measures the rows the long name **added**, per paper, now.

### The integer layout, and the lint that proved it

`float_arithmetic` is `forbid` workspace-wide, so nothing in this pipeline may apply an arithmetic
operator to a float. The layout works in font units instead and every pen position, line break and
page dimension is an `i32`. **That is better than the float version**: `1.7.5` needs
`golden_receipts_are_byte_stable`, and an accumulated `f32` pen position is how a golden starts
differing between a laptop and CI.

One float survives, `tiny-skia`'s transform, and it comes from a `Decimal` division. **The lint
caught the first draft of that line** — `-scale` for the y-flip is unary float arithmetic, and
`forbid` makes it `E0453` rather than a warning. The flip is a negated *integer* em size now,
which is the clearer statement anyway.

### What `1.7.3` did not do, and one gap it found

No ESC/POS bytes (`1.7.4` owns `escpos.rs`), no goldens (`1.7.5`), no printer status (`1.7.6`), no
QR raster — `ref/fiscal-jofotara.md:450` puts that at `2.7.9` and `FiscalBlock`'s payload is
opaque here by design.

**And no logo, which §2.3 lists first.** `ReceiptModel` has no field for one, so `1.7.1` could not
carry it; `ref/hardware-and-receipts.md` §2.4 says a logo is back-office-editable alongside the
header and footer text, so the product expects one. **No microstep assigns it** — the same class
as `build_receipt_model` and `lib/ipc.ts`. Reported rather than invented.

---

## 2o · 22 September — `1.7.4`, and the test everybody would write first

**#221 merged `1.7.4`** — `crates/pos-hardware/src/escpos.rs` (509 lines, 14 tests) and one
`lib.rs` line. Issue #220 is the microstep issue, filed before the branch and closed by the PR.
Phase 1 is **34 of 112 (~30%)** and the Rust suite is **378 → 392**. `Cargo.lock` does not move:
the emitter takes no dependency.

`1.7.3` stopped at a bitmap that is 1 bit per pixel, MSB first and row-major — exactly the layout
`GS v 0` takes — so this is a header and a copy rather than a re-encode:

```text
ESC @            1B 40                      initialise
GS  v 0          1D 76 30 00 xL xH yL yH    raster, normal size
                 <the bitmap's own bytes>
GS  V            1D 56 01                   partial cut
```

### The obvious test is wrong, and it would have been intermittently red

`ref/hardware-and-receipts.md` §4 is unusually direct about why the drawer pulse is not in a
document: *"the stream is persisted and retried; the pulse is a physical, non-idempotent,
cash-access effect. Retrying a stream that contains one opens the drawer again, with no cashier
action, no audit entry and nobody watching."* The **cut** stays, because cutting twice wastes a
few millimetres of paper and cutting is what makes the document a document.

The assertion everybody reaches for is `assert!(!stream.contains(&[0x1B, 0x70]))`. **It is
wrong.** The raster payload is an arbitrary bitmap, so `1B 70` occurs inside it by coincidence
about once in every 65 536 byte pairs — which a 576-dot receipt reaches within roughly 900 rows.
The test would pass on the fixtures and fail on a real receipt, some of the time, for a reason
nobody would find quickly.

The assertion is structural instead: a stream is exactly `INIT ++ header ++ payload ++ CUT`, and
the pulse is absent from everything that is not the payload.
`a_bitmap_containing_the_pulse_bytes_is_still_a_document` makes the coincidence deliberate — it
builds a page that really does contain those bytes, asserts the naive test *would* have failed,
and asserts the correct one passes.

**Worth generalising:** a payload of arbitrary bytes makes every "this sequence never appears"
assertion a probabilistic one. The question to ask is always *where* it may not appear.

### The finding this microstep could not fix

On many thermal printers the cutter sits ten to fifteen millimetres above the print head, so a
bare `GS V` severs the paper at a point the document has not reached and **the last lines stay
inside the machine**. Feeding first — `ESC d n`, or `GS V 66 n` in one command — is the usual
answer and the distance is a device property, which is why §6a makes `cut_command` a profile field
beside `raster_command`. **The matrix ships empty except the simulator**, so choosing a distance
would be the same defect §6a.1 refuses for the reference register.

**What is missing is not the feed but the check.** §9's hardware-lab checklist covers truncation
*across* the paper — check 2, *"58 mm profile prints without truncation"* — and nothing at all
covers it *along* the paper. A receipt that loses its footer inside the printer passes every check
in that table. Recorded in `1.7.4`'s phase entry and in the module, where whoever qualifies the
first printer behind #68 will be looking.

The same reasoning settles banding, which is why one `GS v 0` ships rather than a banded sequence:
field libraries split a raster because real printers have finite buffers, the spec does not
require it, and there is no device whose buffer could be measured.

### The sweep and the read, for the sixth microstep running

**22 mutations, 22 caught** on the first pass — a big-endian header, swapped width and height, the
dot count where the byte count belonged, a payload emitted twice, a document carrying a pulse.

**The read then found two things no mutation could**, because neither was code: the cut gap above,
and **an empty page was accepted**. A zero-line bitmap emitted an initialise and a cut with
nothing between them — paper and a cutter cycle spent to hand a customer a blank tab. It is
refused now. `render_receipt` cannot produce one, so reaching it is a caller's bug, and a loud one
is cheaper than a quiet one.

### Three smaller things

* **`encode` takes a bitmap and not a bitmap and a profile.** A `Bitmap` already carries its
  width, height and row stride; a second argument naming the paper would be a second source of
  truth for the same three numbers and a way to hand the emitter a mismatched pair.
* **`payload_range` is public** for the same reason the structural assertion exists: a caller
  checking what is *outside* the payload should not have to re-derive the offset arithmetic and
  get it wrong.
* **`1.7.4` cleared a missing `Done when`** — the fourth step ever, after `1.1.9`, `1.6.6` and
  `1.7.1`, and the second to author it in its own commit before any code. The list is 16.

### Two gaps named rather than absorbed

`ref/hardware-and-receipts.md` §1 specifies a richer `ReceiptPrinter` than `lib.rs` ships — four
`PrintOutcome` variants and a `profile()` — and closing that gap is on no microstep's `Files:`
line that this session could find. And §2.1's plain-codepage fallback for *"exotic printers that
reject raster"* has **no owning microstep at all**. Neither was in `1.7.4`'s scope; both are on
the pile with the receipt logo and `build_receipt_model`.

---

## 2p · 22 September — `1.5.1`, and a table written to match a seed it cannot change

**#224 merged `1.5.1`** — `crates/pos-domain/src/tender.rs` (601 lines, ten tests) and six
`lib.rs` re-exports. Issue #223 is the microstep issue, filed before the branch and closed by the
PR. Phase 1 is **35 of 112 (~31%)** and the Rust suite is **392 → 402**. `Cargo.lock` does not
move; no migration is touched.

**Not `1.7.5`, which §4 still names first.** §7 item 6 records why: its `Done when` needs a dated
record naming a native reader, and nobody has been asked who that is. `1.5.1` was the next name on
the list and needed an answer from nobody.

### `1.6.3` in reverse, and the direction is the part to carry

`1.6.3`'s entry says its capability grid is *"what 1.6.1 writes its seed from, so the two
microsteps stop pointing at each other"* — grid first, migration second. Here it is the other way
round: `tender_type`'s **complete seed shipped inside migration `0005` at `1.9.1`**, months before
the type that describes it, and a committed migration is never reopened.

So `standard_tender_types()` is written to match `0005:657-662` column for column, and
`the_standard_grid_matches_the_committed_seed` pins all six rows against literals **transcribed
from the `.sql`** rather than built from the function they check — a fixture derived from its
subject asserts nothing. If the two ever disagree, the domain type is what changes.

**Worth knowing before the next microstep whose types a migration already describes**, and there
are several: `1.2.4`'s barcode rules, `1.10.x`'s stock ledger columns, `1.3.x`'s tax rules. The
question to ask first is which came first.

### The distinction that keeps a shift from closing short

`0005` carries its own comment about this, which is unusual and is the reason it was noticed:
*"`is_internal` is NOT implied by `is_cash_counted = 0`: `card` and `cliq` count no drawer cash
but do move real value through a PSP."*

Two predicates that look redundant on five of six rows. §7.1 states what confusing them costs:
`exchange` must carry `is_cash_counted = 0` **and** `is_internal = 1`, *"or the offset would appear
in expected drawer cash on both documents and the shift would close short by twice the exchanged
value."* `an_internal_tender_is_not_merely_uncounted_cash` holds them apart with `card` and `cliq`
as the witnesses, so collapsing the two flags into one fails a test rather than a shift.

### Four columns the domain type does not carry

`tender_type` has ten and `ref/domain-api.md` §7's `TenderType` has six. `name_ar`, `name_en` and
`sort_order` are presentation. **`is_active` is the one worth naming**: the seed ships `cash`
active and the other five inactive, because *"later payment/refund microsteps enable behavior;
they never reopen 0005 to append codes"*. Activation is therefore a property of one store's row,
not of a kind of tender, and putting it on a pure type would invite a pure function to answer a
question only the database can.

### The gap this module inherited rather than created

**20 mutations, 20 caught.** The read then found something the sweep had no way to ask about:
`as_str` and `#[serde(rename_all = "snake_case")]` are **the same mapping written twice** — one
for a SQL bind against `0005`'s `CHECK` constraints, one for the JSON wire form. They coincide,
which is exactly the condition under which a change to one goes unnoticed. Both were already
checked separately against the migration's constraint lists;
`the_wire_form_and_the_storage_form_are_the_same_word` asserts the third edge of the triangle, and
two added mutations prove it.

**`catalog.rs` and `permissions.rs` carry the same unchecked pair** — an `as_str` beside a
`rename_all` on four enums between them. Noted rather than fixed: conventions §6 rule 6 keeps a
microstep inside its own `Files:` list, and this is a two-line test whenever somebody is in those
files anyway.

### One test lost an assertion it should not have carried

`only_cash_opens_the_drawer_and_gives_change` was additionally pinning `is_cash_counted`, which is
neither what its name promises nor a rule — it is one column of a grid
`the_standard_grid_matches_the_committed_seed` already pins in full. Removed. A test whose name
under-describes it is a test somebody will later "fix" by deleting the wrong half.

### No property test, and that is §5.1 rather than an omission

`.claude/rules/rust-domain.md` asks for a `prop_<invariant>` per business rule, and this microstep
ships none. The rules are bounded — six rows, six columns — and conventions §5.1 is explicit that
*"a bounded universal claim uses an exhaustive `#[test]` loop when that loop is feasible"*. A
generator over six fixed rows samples what a loop enumerates and tests strictly less.

The unbounded rules in group 1.5 are `1.5.2`'s and `1.5.3`'s — split tender summing to the total,
cash rounding only on the final cash tender — and both already carry `prop_` names the catalogue
lists. **Stated here because a future sweep of "does every module have a property" would flag this
file**, and the answer is written down rather than re-argued.

### And a number that went the wrong way, correctly

§1's startable count rose from 36 to **38** on a microstep that completed one. Creating
`tender.rs` made `1.5.2`, `1.5.3` and `1.5.4` file-startable at once. Only two of the three really
are: `1.5.2` needs `Tendering`, which holds a `Cart` and a `PricedCart`, and neither name exists
under `crates/` — the same trap `1.4.1` sits in. The file-existence check is doing what it says;
what it says is narrower than "startable".

## 2q · 23 September — `1.5.3`, and a rule stated before the type that will hold it

**#228 merged `1.5.3`.** It adds `CashRounding`, `compute_cash_rounding` and `final_tender_rounding`
in `crates/pos-domain/src/tender.rs` with eleven tests, two of them properties, plus three `lib.rs`
re-exports and `ref/domain-api.md` §7 in the code's own commit. It also retires two `PLANNED`
catalogue names. Issue #227 was filed before the branch and closed by the PR. Phase 1 is **36 of 112
(~32%)** and the Rust suite is **402 → 413**. `Cargo.lock` does not move and no migration is touched.
The PR body follows `.github/pull_request_template.md` section by section. The PRs before it used
free-form bodies, and the template's **Closes** section is what moves the board card.

### The inputs were present; the type the rule lives on was not

§4 said `compute_cash_rounding`'s inputs were "all present", and they were: a `Money`, a step and a
direction. But four of the entry's six tests describe a *settlement*: a card that leaves a
remainder, a final cash tender, change handed back. E.14 is a rule about *which* tender is
rounded, and §7's `Tendering`, where that rule naturally lives, holds a `Cart` and a `PricedCart`
that do not exist.

So the rule is stated over the vocabulary `1.5.1` left:
`final_tender_rounding(kind, remaining, step, dir) -> Result<Option<CashRounding>, MoneyError>`
answers *"if this tender settles the sale, what is it asked for?"* A cash kind gets
`compute_cash_rounding(remaining, …)`. Every other kind gets `None` and is asked for the exact
remainder. The `Option<CashRounding>` is exactly `Tendering.cash_rounding`'s type, so `add_tender`
can store it rather than re-derive it. **Which tender is final belongs to `1.4.8`, and its entry now
says so**: a cash tender covering `rounded` is final; one that does not is a partial cash tender,
applied exactly. "Cash rounds once" is structural, because one tender reaches the rule.

### The formula that was superseded, caught one commit late

The docs-first commit justified keying on `is_cash_counted` with master plan C.6's expected-cash
formula, *"− cash rounding given away"*. `00-master-plan.md` §4a row 176 supersedes that formula
because it *"double-counted cash rounding"*, and `ref/domain-api.md` §11 is its normative
replacement. §11 says the opposite of what the commit leaned on: **cash rounding carries no term**,
because the counted tender's `amount` already is the rounded amount.

The conclusion survives on the right reason. `is_cash_counted` is the flag that makes a tender's
amount *coin*: §11 counts `amount − change` into the drawer over exactly those tenders. Every other
kind can carry any fil, so rounding it would charge fils nothing required. `ref/tax-jordan.md` §5
rule 5 pairs the two in one sentence.

**It was found by opening §11 to cite it, before the code commit**, and it is recorded in three
places: the code commit's message, the PR's Design section, and a *"Corrected after filing"* note on
#227. `CLAUDE.md` tells every session to read §4a before acting on a sentence from `docs/plan/`, and
the examples it lists are names (`rate_bp`, `stock_movement`, `tax_group`) or rules someone would
implement (banker's rounding, migrations with `down` steps). This was a *formula used only as a
reason*, which is harder to notice because nothing in it was going to be implemented.

### Why `is_cash_counted`, and the test that makes it load-bearing

On today's six-row grid, `is_cash_counted`, `opens_drawer`, `allows_change` and `code == "cash"` all
select the same row, so any of the four passes every test built from seeded kinds.
`rounding_follows_the_drawer_count_not_the_code` constructs two kinds that pull them apart:
`"coins"`, counted with no drawer and no change, is rounded; `"cash"`, uncounted with drawer and
change, is not. **Three of the sweep's 24 mutations are caught by that test and by nothing
else**: the predicate replaced by `allows_change`, by `opens_drawer`, and by `code == "cash"`.
Without it, the choice this microstep argued for would be a coincidence of the seed.

### The sweep: 24 mutations, 24 caught, and one that only one example caught

The first pass caught **23 of 23**. Reading the list of catching tests turned up a weaker spot:
**forcing `Up` in the cash branch was caught by one example only**, the Done-when's
`1.247`/`0.624` case. The E.14 property checked where rounding applied but never in which direction.
It now also asserts `Ok(r) == compute_cash_rounding(remaining, step, dir)`, meaning the rule
delegates to the arithmetic. That is the same shape `money.rs`'s proration property uses. The
re-run added a 24th mutation, the adjustment's currency hard-coded to JOD, which both properties
catch because they draw USD and EUR as well. **24 caught, 0 invalid**. Each mutation rebuilt the
crate, was restored with a fresh mtime (lesson 17), and counted as invalid, never as caught, if it
did not compile.

### The read found three things the sweep structurally could not

This read was done against the reference rather than the diff: `ref/tax-jordan.md` §5,
`ref/domain-api.md` §7 and §11, master plan B.5 and E.14, and merchant decisions 2.1, 2.2 and 6.12.

1. **The docs implied a legal-tender claim.** They said the ten-fil step followed from the coins
   in circulation. `tax-jordan.md` §5: *"The configured step is an operational merchant choice, not
   a claim about legal tender or tax law."* The wording now says what the reference says.
2. **A final cash tender can be empty.** A remainder under half a step is asked as `0.000` under
   `Nearest`, and one under a whole step under `Down`. The adjustment alone then settles the sale.
   That belongs to `add_tender`, so it is on `1.4.8`'s obligation beside the finality rule.
3. **The C.6 formula** above.

### Two plan findings, neither a code problem

* **The cash-rounding policy does not live where four documents say it does.**
  `cash_round_step_minor` and `cash_round_direction` are columns of `tax_computation_policy`
  (`0003`), which a store references. `ref/tax-jordan.md` §5 rule 1 and merchant decisions 2.1–2.2
  say `store.cash_round_*`, and so does `0003`'s own comment two lines above the columns: *"cash
  rounding remains a separate settlement rule on `store`"*. The comment is in a committed migration
  and cannot change. The pure function takes step and direction as arguments, so nothing here
  depends on it. But whoever wires `add_tender` needs the right table, so `1.4.8`'s entry names it.
  The underlying tension is real and unresolved: an operational merchant choice is stored in a
  source-hashed, versioned jurisdiction row.
* **`1.1.6` said this step builds `round_to_step`.** `1.1.2b` built it (#45). That was a false
  statement about this microstep's own scope, so it was corrected in its docs-first commit.

### And one repository finding: the money-path label misses the money

`.github/labeler.yml`'s `risk: money path` globs are `money*`, `tax*`, `cart*`, `discount*`,
`packages/money/**` and `ref/tax-jordan.md`. **No planned module is named `discount*`**, since
discounts are `pricing.rs` (`1.4.5`–`1.4.7`), and **neither `tender*` nor `pricing*` is covered**. So
the label that §4 of `03-github-workflow.md` says *"means the PR needs a property test"* was absent
from #228, which carried two. It was added by hand. Fixing the globs means editing
`.github/labeler.yml`, which is in the frozen policy set, so the fix is its own deliberately red PR.
§4 carries it.

### What `1.5.3` did not do

No `Tendering`, `remaining_due`, `change_due` or `is_settled`: those are `1.5.2`'s. No `add_tender`:
that is `1.4.8`'s. No `compute_refund_rounding`, which belongs to `2.3.3` and has its own direction
default and OPEN item. No denominations, which are `1.5.4`'s. And no tax or fiscal treatment of the
adjustment: `tax-jordan.md` §5's OPEN item keeps it a signed tender-level amount.

**One limit is named rather than absorbed.** `CashRounding`'s fields are public and it derives
`Deserialize`, because §7 specifies it so. That means an inconsistent one — an `adjustment` that
is not `rounded − original` — can be constructed or deserialized. `compute_cash_rounding` is the only
constructor that guarantees the relation. Whoever persists a `Tendering` for crash recovery (`1.8.4`'s
checkout journal) should re-derive the rounding rather than trust a stored one.

## 2r · 24 September — `1.5.4`, the tracking sweep, and the red that lasted three days

**Four pull requests before this edition's own, and nine issues** (#230–#237 and #241). In order:

1. the tracking sweep, #230–#235, with #203 and #197 brought up to standard;
2. `1.5.4` (#236, #238), which produced #237;
3. #240 (closing #203), then #239 (closing #230);
4. #242, with #241 filed on the way.

### The tracking sweep, because "everything gets documented" was the instruction

This file had carried seven items under some form of *"no issue tracks it"*, some since
13 September:

- the four ⚠️ OPEN items that gate Phase-1 microsteps;
- #115's enforcement remainder;
- the two findings `1.5.3` left.

Each one is now an issue. Each has the repository's format (verbatim `file:line` quotation, what it
blocks, the default until answered, and who settles it), the matching labels, and a filled board
card:

| # | What | Waits on |
|---|---|---|
| #230 | the money-path label globs | — **closed by #239** |
| #231 | is the cash-rounding step a jurisdiction policy or the merchant's store setting? | an operator decision |
| #232 | the SST base order and fixed part (`1.3.5`), and zero-rating reasons (`1.3.2`) | the tax adviser, **in #70's engagement** |
| #233 | what second factor exists on the counter (`1.6.2`) | the merchant |
| #234 | does the bundled SQLCipher carry SQLite's WAL-reset fix (`1.8.1`)? | **work**, a spike with a stated method |
| #235 | nothing diffs the checked-in rulesets against live; `gh-protect.sh` is obsolete | **work** |

**#70 got a cross-link** to #231 and #232, because the same adviser sitting can answer both, and
#231 changes what #70's item 3 must contain. **#203** gained its labels and board card before it
closed, and **#197's card** gained the fields it never had.

One thing the sweep deliberately did **not** do is turn §8's other repository-queue items into
issues. A recurring bypass-ledger check, Actions allowlisting, the matrix on every PR and the
`staging → main` promotion are recorded there by design. This file never called them untracked.

### `1.5.4`, and a `Done when` the plan's own data refuted

**#238 merged `1.5.4`.** `denominations(currency) -> Option<&'static [Money]>` returns the plan's
eleven pieces, largest first. Phase 1 is **37 of 112 (~33%)** and the Rust suite **413 → 416**.

**Keyed by `Currency`, and `None` is not an error.** Master plan E.17: *"system doesn't care about
denominations for correctness, but count helper does."* A currency with no table gets no helper,
and it never gets the dinar's, which would be off by a factor of ten in USD (I-2).

**The docs-first commit authored a rule the plan's data breaks, and the first test run said so.**
The rule was *"every denomination is a whole number of qirsh"*, and 25 fils is not. The failure
was not a code bug. It was the plan disagreeing with itself: a 25-fil piece tendered against a
remainder rounded to `0.020` is owed five fils, and the table has no piece that pays it. So:

- the test became `the_smallest_denomination_is_one_qirsh`, which is true and still ties the table
  to merchant decision 2.1;
- the `Tests:` and `Done when` lines were corrected **in the code commit, with the reason in its
  message**, and #236 carries a *"Corrected during the build"* note;
- the values stay the plan's, because lesson 7 says transcribe and nobody here can verify the real
  coin set;
- **#237** asks the merchant which coins cross the counter;
- the inconsistency is written on the function rather than asserted by a test of the hole.

This is the sixth microstep to clear a missing `Done when` and the fourth to author it in its own
commit before any code, after `1.7.1`, `1.7.4` and `1.5.1`. It is the **first whose authored
condition was wrong about the plan's data** rather than merely incomplete.

**A hazard measured before it was written.** With 25 fils and no 5, counting out largest-first
fails on **80 of the 200** qirsh-multiples up to 2.000 JOD, starting at 30 fils. Without the
25-fil piece it fails on **none**, measured over every qirsh-multiple to 200.000 JOD. Nothing makes
change from the table today, and its documentation forbids anything from doing so greedily.

**The two reviews.** *The sweep: 10 mutations, 10 caught.* The currency-tag mutation is caught by
one assertion only, the one that checks each entry's currency, which is why that assertion stays
although the literal-equality check looks like it covers everything. *The read* found that
`ref/ui-spec.md` uses the `DenominationGrid` for float entry at shift **open** as well as the blind
count at close, and the docs had named only the close. Both are fixed.

### #203: the upstream diff read, then adopted

`dtolnay/rust-toolchain`'s `v1` moved by **six commits**. Upstream's
`compare/6c977a6ca407...02cb101ec7c4` shows one change that runs in our jobs: `--force-non-host`
added to `rustup toolchain install` and to `rustup default`. It permits a non-host toolchain, and we
install the host one, so nothing we install changes.

**Re-pinned rather than re-commented.** Decision 12 requires a pin's comment to name a tag that
resolves to that exact SHA, and the old SHA no longer has one. A zizmor suppression would silence
the check that caught the drift. Evidence: `workflow-analysis` passed on #240 with **0** warnings,
`rust`, `guards` and `supply-chain` passed while running the new pin, and then `workflow-analysis`
passed **on `development`** after both merges. The canary, `proptest-scheduled` and `release`
sites run on schedules, promotions and tags, **not on a PR**, so their first evidence is their next
run.

### Two frozen-surface PRs, merged one at a time

#239 and #240 were both red by design, and both merged through §9's recipe after the operator read
each diff and approved it. **Order mattered.** `development` is `strict: true`, so the second PR
would have been behind at merge time, and a bypass of a behind branch can put a second failure into
the ledger entry. So #240 merged first, #239 was brought up to date **on the server**
(`gh pr update-branch 239`, a merge commit that squash flattens), its checks re-ran, and only then
was it bypass-merged. Both ledger entries name one check: **`4206181799`** and **`4206239586`**. As
a side effect, #239's re-run showed `workflow-analysis` green from a second PR.

### #234, answered the same day: the fix is absent, and `sqlx` is why

#234 was the one board item that work alone could close with a stated method, and it closed by
that method (#245):

- **The runtime, read from the compiled build.** A throwaway crate in the session scratchpad, not
  the repository, built the exact `rusqlite` 0.39.0 / `libsqlite3-sys` 0.37.0 that `Cargo.lock`
  pins. It printed `sqlite_version()` **`3.50.4`**, source id `2025-07-30 19:33:53 4d8adfb3…`,
  and `PRAGMA cipher_version` **`4.10.0 community`**. The amalgamation's `#define`s in the crate
  source agree.
- **The advisory.** [SQLite §11](https://sqlite.org/wal.html) says the bug is present from 3.7.0
  through 3.51.2, fixed in **3.51.3**, with backports in 3.44.6 and 3.50.7. It needs two
  connections on the **same** file writing or checkpointing at once.
  [SQLCipher 4.14.0](https://www.zetetic.net/blog/2026/03/17/sqlcipher-4.14.0-release/) is the first
  release found to carry the fix.
- **The blocker.** `rusqlite`'s `upgrade_sqlcipher.sh` at each release tag puts SQLCipher 4.14.0
  first in **`rusqlite` 0.40.0**. But `sqlx-sqlite` 0.9.0 requires
  `libsqlite3-sys >=0.30.1, <0.38.0`, and Cargo's `links` rule covers that optional driver, so 0.40
  does not resolve. **That is why Dependabot, which proposes cargo majors here, never proposed it.**
  #244 carries the three routes: wait for `sqlx`, patch `sqlx-sqlite`, or split the workspace.

**What changed in the plan of record:**

- `ref/plan-validation.md`'s OPEN block is now an *ANSWERED* block with every source.
- `1.8.0`'s entry names the minimums, SQLite `3.51.3` and SQLCipher `4.14.0`, and **the design
  question they raise**: those minimums, read as *"refuse an unsupported build"*, would refuse
  today's build at every open, so the policy must gate a second connection rather than the open,
  or carry two tiers.
- `00-master-plan.md` counts six Phase-1 blockers, not seven, and settles its architectural note:
  `1.8.6`'s backup runs through the one source connection, which its `snapshot(conn: &Connection, …)`
  already takes.

**One citation was wrong and is corrected.** #234's first body and #244's cited `1.8.5b` for the
backup; the backup is `1.8.6`, and `1.8.5b` is key custody. The sweep across the docs that lesson 1
asks for found it, along with two more sites that still called the question open.

### #241: the drill home, built before the first drill

`02-development-workflow.md` §5.10 specifies `docs/drills/` and a `05-drill-result.yml` issue form
*"so it can be filed from a phone in the lab"*, and neither existed. §5.10 had said the first drill
would create the directory, and nothing at all owned the form. **#247** built both:

- the index, with an empty runs table that says so, and §5.10's record format (drill, ran against,
  hardware, operator, times, outcome, surprises), extended to reviews done by a person, because
  `1.7.5` and `1.11.14` each need one;
- the fifth issue form;
- `03-github-workflow.md` §4 now lists five forms, and §5.10 now says why the directory predates
  its first drill.

Issue templates are not frozen (#166 edited one with `protected-paths` green), so #247 was green
throughout, `workflow-analysis` included.

### `PROJECT-GUIDE.md` is tracked, as a snapshot

The operator's answer to the question put on 8 and 9 September was *"start track it"*. It is
tracked with a **status banner and an errata table**: eight claims measured false on 24 September,
including two that contradict `CLAUDE.md` (`float_arithmetic` is `forbid`, and the repository is
public with four rulesets). The body is untouched except one line that published a local absolute
home path. This follows `00-master-plan.md` §4a's pattern for the frozen source plans: keep the
document, and put the errata where a reader meets them first. The one gap it recorded that is still
true (no `docs/drills/`, no drill-result form) is **#241**.

---

## 3 · The seventeen open issues

**All seventeen are on board #4, all `Todo`, all assigned**, re-read live at `d573662` with
`gh issue list --state open`. That is still the command to count with, because the board is only as
complete as the last person who added to it. **Two can be closed by work alone**: #174 and #235.
#234 and #241 were the others until #245 and #247 closed them the same day. **The other fourteen
wait**, thirteen of them on a person:

- one on `hardware` (#68);
- seven on a `decision` (#69, #71, #111, #113, #114, #231, #244);
- five on a `merchant answer` (#70, #112, #232, #233, #237);
- #197 on the sequencing of `0006`.

### #203 — CLOSED by #240 on 24 September

**The diagnosis below held, and the fix is in §2r.** Re-pinning to where upstream's moved `v1` tag
now points turned `workflow-analysis` green on the PR and then on `development`. The subsection is
kept as the record of a scheduled check going red with no repository change, because that class
will recur.

### #203 — the weekly security workflow is red, and it is not your diff (the 21 September record)

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

Nothing about this blocks a microstep. It should get a label, a board item and a decision. *(It got
all three on 24 September, and then the fix.)*

**#174 is one of two that code alone can close**, with #235. **#179 was closed by
hand on 15 September** with three of its four findings unaddressed; they ride in #197, and the block
below re-measures each.

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
| **231** | `decision: is a store's cash-rounding step a jurisdiction policy or the merchant's setting?` | P2 | money path | decision | `1.4.8`'s wiring of `add_tender`, and what #70's item 3 must contain. `0003` put the columns on `tax_computation_policy`, while `ref/tax-jordan.md` §5 and merchant decisions 2.1–2.2 say `store` |
| **232** | `decision: two more tax-adviser questions — the SST base (1.3.5) and zero-rating reasons (1.3.2)` | P1 | money path · compliance | merchant answer | `1.3.5` and `1.3.2`. Belongs in **#70's engagement**, and #70 is cross-linked |
| **233** | `decision: what second factor exists on a Jordanian minimarket counter (1.6.2)` | P1 | security | merchant answer | `1.6.2`, which is also blocked by #68's bench gate. Answering this alone does not unblock it |
| ~~234~~ | ~~`spike: does the bundled SQLCipher carry SQLite's WAL-reset fix?`~~ | P1 | — | — | **CLOSED by #245 on 24 September**: the fix is absent (SQLite `3.50.4`, SQLCipher `4.10.0`). Its successor is #244 |
| **244** | `gap: the WAL-reset fix is one rusqlite bump away, and sqlx's libsqlite3-sys bound blocks it` | P1 | — | decision | every second connection on the source file. The routes are wait for `sqlx`, patch `sqlx-sqlite`, or split the workspace. `1.8.0` pins the minimums either way |
| **235** | `gap: nothing checks the checked-in rulesets against live, and gh-protect.sh is obsolete` | P2 | security | **not blocked, closed by work** | nothing; it is a control. Each fix is a frozen-surface, deliberately red PR |
| **237** | `decision: confirm the dinar coin set — the plan lists 25 fils and omits 5 fils` | P2 | money path | merchant answer | nothing in code (E.17). A coin missing from the grid reads **short at every close** |
| ~~241~~ | ~~`gap: docs/drills/ and the drill-result issue form are promised and do not exist`~~ | P2 | — | — | **CLOSED by #247 on 24 September**: the index, the record format and the fifth form exist, ahead of the first drill |

**#69 does not gate a Phase-1 microstep**, but the sentence this document used to prove it **does
not exist**. Until 21 September this block quoted `phase-1:1066` as saying *"Owner: 2.7.0 ratifies
6.9 via #69, on a timeline outside this project's control, **so this microstep cannot wait for
it**."* The clause after the comma is in no file in this repository: `grep` for it across `docs/`
returns only this handoff. The real sentence is at **`phase-1:1091`** and stops earlier —
*"Owner: `2.7.0` ratifies 6.9 via #69, on a timeline outside this project's control. Source that
settles it: the official ISTD business rules or a written ISTD E-Invoicing Directorate ruling."*

**The conclusion survives the correction and the evidence for it is elsewhere**, which is why this
is a repair rather than a retraction: #69's own body scopes it to *"all of group 2.7 and the 22
⚠️ OPEN items microstep 2.7.0 owns"*, and the mechanical check is that **no Phase-1 microstep names
#69 or merchant decision 6.9 as a prerequisite**. It is a long lead ordered in Phase 1, not a
Phase-1 blocker. But a fabricated quotation in a document whose opening line promises that *"every
number here was read from `git`, `gh` or a command"* is the most expensive kind of error this file
can carry, so it is recorded rather than quietly fixed — see §15.

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

### Four ⚠️ OPEN items gated Phase-1 microsteps — three still do (#232, #233), and #234 answered one

Two more than this document carried until 13 September. The sweep that found them is
`grep -rn '⚠️ \*\*OPEN' docs/implementation/ref/*.md docs/implementation/phase-1-sellable-mvp.md`,
filtered to those naming a `1.x` microstep — run that, not a keyword search, because the two new
ones share no vocabulary with the two old ones.

* ~~`ref/plan-validation.md:324` — **blocks `1.8.1`**~~ — **ANSWERED on 24 September (#234,
  #245): the compiled build lacks the fix.** The one-connection rule stands as a constraint, and
  `1.8.0`'s minimums are SQLite `3.51.3` and SQLCipher `4.14.0`. The bump is #244.
* `ref/security-compliance.md:415` — **blocks `1.6.2`**: what second factor exists on a Jordanian
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

**Filed on 24 September.** The two tax questions are **#232**, in #70's engagement because it is the
same adviser. The second factor is **#233**, and the WAL-reset question was **#234**. That one was a
*spike*, not a decision, and work closed it the same day, with #244 as its successor. The sentence
that stood here, *"File them, or record deliberately
that they live only in the reference documents"*, had waited eleven days for a decision, and the
operator's instruction that *"everything gets documented"* made it.

### #115's remainder is still partly open — tracked by #235 since 24 September

The four ruleset payloads exist under `.github/rulesets/` and agreed with live when last compared
by hand. What is still open is **enforcement**: no gate runs the diff — so every statement that they
"byte-match live" is a dated observation and decays the moment someone edits a ruleset in the UI — the payloads have never been
round-tripped (no restore has been executed), and `scripts/gh-protect.sh` still refuses at **exit
3**. It refuses correctly — its legacy-API `PUT` would now apply three verified defects (a required
list omitting `guards`, `supply-chain` and `protected-paths`; `require_code_owner_reviews: true`
against a sole-developer CODEOWNERS with 0 required approvals plus `enforce_admins` on main; and the
legacy API cannot express `allowed_merge_methods` at all). **#235 now tracks this remainder.** It
also records that `gh-protect.sh`'s refusal text, *"The replacement is a ruleset … Until it lands"*,
describes a world that ended when the rulesets landed.

---

## 4 · What is next — the WIP=1 slot is FREE

**Take one of these, and only one.** WIP = 1 means one microstep, not one open pull request. Every
candidate this file has named since 14 September is spent — `1.9.1` (§2b), `1.1.9`'s database half
(§2c), `1.2.6` (#177), #179's five missing negative tests (#185, §2e), `1.11.6` (#188, §2f),
`1.11.11` (#192, §2g), `1.9.2` (#195, §2h), `1.7.2` (#200, §2i), `1.6.6` (#204, §2j),
**`1.6.6b`, which this section named as the closest successor and which landed the same day**
(#208, §2k), **`1.11.1`, named here as `1.6.6b`'s closest successor and landed the same day
again** (#212, §2l), **`1.7.1`** (#215, §2m) and **`1.7.3`** (#218, §2n) and **`1.7.4`** (#221, §2o) and **`1.5.1`** (#224, §2p) and **`1.5.3`** (#228, §2q) and **`1.5.4`** (#238, §2r).

**§1's "What is LEFT" block is the view to open first.** It is derived from the phase file and the
filesystem rather than from this list, it says which of the 75 remaining steps are startable, and it
names the two files — `apps/terminal/src-tauri/src/ipc/registry.rs` (`1.6.7`) and
`crates/pos-domain/src/cart.rs` (`1.4.1`) — that between them unblock **seventeen** microsteps. Both
are startable today. This section is the human judgement on top of that; the block is the evidence
under it.

**There is no clean next microstep, and saying so is this section's job.** Every name left on the
shortlist needs one thing settled before it is honest to start:

- **`1.7.5`**, the most valuable, needs a **native reader** (§7 item 6). Its record now has
  somewhere to land: #247 built `docs/drills/` and the Drill result form.
- **`1.6.7`**, capability exhaustiveness, needs a **scoping decision**. It *creates* the registry,
  but its `Done when` requires *"the department threshold boundary"*, and there is no department
  command and no IPC command of any kind: `apps/terminal/src-tauri/src/` holds only `lib.rs`,
  `main.rs` and `time.rs`. The registry mechanism and its two negative fixtures
  (`a_registry_fixture_without_a_command_spec_is_refused`,
  `a_privileged_fixture_without_a_binding_is_refused`) could land now, either as a partial step
  carrying a `**Full-step status:**` marker, as `1.1.9` and `1.2.0` did, or as a lettered split
  like `1.6.6`/`1.6.6b`. The command-walk tests could not.
- **`1.6.8`**, PII scrubbing, needs **a dependency decision** (`apps/terminal/src-tauri` has no
  `tracing`) and a sale trace for `no_pii_in_a_full_sale_trace` that no flow produces yet. The
  `SENSITIVE_FIELD_RULES` registry itself is the part #174 is really asking for.
- **`1.4.1`** needs types owned by `1.2.4` (blocked, #71) and `1.4.5` (blocked, #112).

**Work that is not a microstep, and is fully unblocked.** Take one of these if the WIP rule is what
is stopping you:

- ~~**#234**, the WAL-reset spike~~, **done the same day (#245)**. It found the fix absent and filed
  #244. `1.8.1` may now proceed under the one-connection rule, and `1.8.0` has its minimums plus one
  design question to settle.
- ~~**#241**, the drill directory and form~~, **done the same day (#247)**.
- **#235**, the ruleset drift check, which is frozen-surface and so deliberately red.
- **#174**, the `Debug` redaction decision.

**`1.5.3` was this section's head, and it is DONE — #228, §2q.** It took the path this section
predicted: `protected-paths` red on its two `PLANNED` names, §9's recipe with `--admin`, and a
ledger entry naming one check. But "its inputs are all present" turned out to be true of its
signatures and not of its tests: four of the six describe a settlement over the `Tendering` that
does not exist. §2q records how the E.14 rule was stated anyway, and the obligation it left on
`1.4.8`.

**`1.5.4` was the head after that, and it is DONE — #238, §2r.** Both questions this section set
for its first commit were settled there: the table is keyed by `Currency` with `None` meaning *no
helper*, and the plan's eleven values are transcribed with no 5-fil row. What the section did not
foresee is that the plan's data would contradict the plan's step: 25 fils is not a whole number of
qirsh. That became #237, not a guess.

**`1.5.2` is not startable**, whatever §1's file check says: `remaining_due`, `change_due` and
`is_settled` all take a `Tendering`, which holds a `Cart` and a `PricedCart`, and neither exists.

**`1.7.5` is the successor `1.7.4` created, and it is the first on this list whose `Done when` a
human has to satisfy.** Seven golden fixtures, a `.png` projection beside every `.bin`, and
`scripts/check-golden-review.py`. Its completion condition ends *"every changed Arabic/bilingual
pair has a dated `docs/drills/` record naming the commit and native reader. A hexdump cannot show
a lost medial form."*

So the code half is startable today — `1.7.3` and `1.7.4` between them produce both the raster and
the bytes, and `tiny-skia`'s `png-format` feature comes back on as a **dev** dependency for the
projection, which is what `1.7.4` said it was for. The other half is a person reading Arabic on a
screen and signing a dated record. **§7 now carries that as a question for the operator**, because
it is the same shape as `1.7.2`'s font choice: a microstep that waits on an answer nobody has been
asked for.

Whoever takes it should also expect **`protected-paths` red**: `golden_receipt_ar_80mm` and
`golden_receipt_ar_58mm` are both in the `PLANNED` ceiling.

**Read §2n before picking anything that adds a dependency.** `1.7.3` found that a crate named in a
reference can be unshippable by the time the microstep arrives: two RUSTSEC unmaintained
advisories retired the stack the plan named, `cosmic-text` inherits one of them transitively, and
`just audit` — a required check — is where you find out. Budget a measurement, not an edit.

**Read this before you pick anything:** a microstep that lands a test `ref/test-catalog.md` names
**will be red on `protected-paths`**, because retiring its `PLANNED` entry means editing
`scripts/check-test-catalog.py` inside the frozen policy surface. That red is the review, the edit
is not optional, and §2k has the recipe. Budget for it rather than being surprised by it —
`1.11.12`, `1.2.3`, `1.2.4`, `1.2.5` and **`1.7.5`** carry `PLANNED` names today, among **29** Phase-1 steps
in all (`1.5.3` left the ceiling at #228). **`1.7.3` took
this red on 22 September**: the CI refusal names exactly one path, and §9's recipe turned out to
be missing a flag. **`1.7.4` did not**, because neither of its tests was in the ceiling — checked
with `grep` rather than assumed, which is what §15 lesson 12 asks for.

> **This list said `1.11.1` carried them too, and it did not.** #212 was **green on
> `protected-paths`** and touched `scripts/check-test-catalog.py` not at all. Neither of
> `1.11.1`'s two named tests is in `ref/test-catalog.md` or in the `PLANNED` ceiling — the other
> four names in the sentence are, this one was never there. The failure shape is worth keeping:
> a list of five where four are right reads as verified, and nobody re-checks the fifth. **Check
> the ceiling for the step you are about to take** — `grep -n '"<step>"' scripts/check-test-catalog.py`
> — rather than trusting its membership in a list here.

| Candidate | Verdict | Why |
|---|---|---|
| ~~`1.9.2` — `SequenceRepository`~~ | **DONE, #195** | §2h |
| ~~`1.7.2` — font decision and embedding~~ | **DONE, #200** | §2i |
| ~~`1.6.6` — `AuditRepository`~~ | **DONE, #204** | its two documentation prerequisites were authored in its own PR, as predicted; §2j |
| ~~`1.6.6b` — local audit verifier~~ | **DONE, #208** | §2k. It was the first binary target, it took no argument-parser dependency, and it retired the first `PLANNED` entry this repository has ever retired through a microstep |
| ~~`1.11.1` — i18n infrastructure~~ | **DONE, #212** | §2l. It discharged §5 ruling 4 and was **green on `protected-paths`** against this section's own prediction |
| ~~`1.7.1` — `ReceiptModel`~~ | **DONE, #215** | §2m. It cleared its own missing `Done when` in a commit before any code, and unblocked `1.7.3` |
| ~~`1.7.3` — the raster pipeline~~ | **DONE, #218** | §2n. It shipped a stack the plan does not name, because two advisories retired the one it does — and it took the `protected-paths` red this section predicts for every catalogued test |
| ~~`1.7.4` — ESC/POS emitter~~ | **DONE, #221** | §2o. Green on `protected-paths`, as predicted, and the test worth reading is the one that would have been intermittently red |
| ~~`1.5.1` — `TenderType` and `Tender`~~ | **DONE, #224** | §2p. It matches a seed that shipped before it, ships no property test on purpose, and raised the startable count while completing |
| ~~`1.5.3` — cash rounding~~ | **DONE, #228** | §2q. It stated E.14 over the tender vocabulary because `Tendering` does not exist, wrote the finality obligation onto `1.4.8`, and caught a superseded formula under its own justification |
| ~~`1.5.4` — denominations~~ | **DONE, #238** | §2r. Its first test run refuted its own authored `Done when`, because the plan's 25-fil piece is not a whole number of qirsh. That became #237, and the table carries a measured prohibition against greedy change-making |
| `1.2.4` pure half | **blocked** | `ref/schema.md:3410` is an `⚠️ **OPEN` item that names it, and #71 is the issue |
| `1.11.12` — empty and edge states | **blocked** | needs rendered screens that do not exist; `1.11.1` created none |

**`1.5.4` leaves three things behind, `1.5.3` four, `1.5.1` two, `1.7.4` three, `1.7.3` two and `1.7.1` two.**

* **The coin set is unconfirmed: #237.** The plan lists 25 fils and omits 5, which contradicts the
  ten-fil step. The values stay the plan's until the merchant answers, and the change is one data
  edit plus its tests.
* **Nothing may make change greedily from `denominations()`.** With the plan's set, largest-first
  fails on 80 of 200 qirsh-multiples up to 2.000 JOD. The prohibition is on the function; a future
  "suggest change" feature must read it.
* **The count grid must decide what happens to a coin the table does not list.** Otherwise a drawer
  holding one reads short at every close. That decision belongs to the steps that build the grid
  (`1.11.x`, `2.4.x`), and #237 records the cost.

**#240 leaves one thing to watch.** Three of its six re-pinned sites (`cross-platform-canary.yml`,
`proptest-scheduled.yml` and `release.yml`) run on schedules, promotions and tags, not on a PR.
Their first run after 24 September is the first evidence that `--force-non-host` is harmless there
too. Read the next canary run's result rather than assuming it.

* **The `risk: money path` label misses `tender.rs` and `pricing.rs`.** `.github/labeler.yml`
  globs `money*`, `tax*`, `cart*` and `discount*`, and no planned module is named `discount*`. It
  was added by hand on #228. The fix is a `.github/labeler.yml` edit, which is in the frozen policy
  set, so it is its own deliberately red PR. Replacing `discount*` with `pricing*` and adding
  `tender*` is two lines, but the red is the review, so read §9 first.
* **The cash-rounding policy lives on `tax_computation_policy`, not `store`**, whatever
  `ref/tax-jordan.md` §5 rule 1, merchant decisions 2.1–2.2 and `0003`'s comment say. The pure
  function does not care, but whoever wires `add_tender` does. §2q states the unresolved tension
  under it: a merchant's operational choice stored in a source-hashed jurisdiction row.
* **A final cash tender can be `0.000`.** A remainder under half a step is asked as nothing under
  `Nearest`. `add_tender` must accept that rather than refuse an empty tender, and `1.4.8`'s entry
  now says so.
* **`CashRounding` can be built inconsistent.** It has public fields and derives `Deserialize`, as
  §7 specifies. Whoever persists a `Tendering` (`1.8.4`) should re-derive the rounding rather than
  trust a stored one.

* **`catalog.rs` and `permissions.rs` carry an `as_str` beside a `rename_all` and nothing compares
  them.** Four enums between them. `1.5.1` closed its own instance with a two-line test and left
  theirs, because conventions §6 rule 6 keeps a microstep inside its `Files:` list — so this is
  free for whoever is next in either file.
* **A module with no property test now needs its reason written down.** `1.5.1`'s is in §2p and in
  its phase entry: the rules are bounded and §5.1 prefers an exhaustive loop. A future sweep for
  *"does every domain module have a `prop_`"* will find `tender.rs` and should find the answer
  beside it rather than re-arguing it.

* **The cut has no feed and §9's checklist has no check for it.** §2o has the detail. It is not a
  code defect — the distance is a device property behind #68 — but the *missing check* is a gap
  anybody can close, and a receipt that loses its footer inside the printer passes every check in
  that table today.
* **`ReceiptPrinter` is poorer than its specification.** §1 of `ref/hardware-and-receipts.md`
  gives it four `PrintOutcome` variants and a `profile()`; `lib.rs` has neither. No microstep's
  `Files:` line claims the gap.
* **§2.1's plain-codepage fallback has no owning microstep**, for *"exotic printers that reject
  raster"*. Fourth of its kind, after the receipt logo, `build_receipt_model` and `lib/ipc.ts`.

* **The receipt has no logo, and no microstep gives it one.** §2.3 lists it first and §2.4 makes it
  back-office-editable beside the header and footer text, so the product expects one —
  `ReceiptModel` has no field for it, so `1.7.1` could not carry it and `1.7.3` could not draw it.
  Third of its kind, after `build_receipt_model` and `lib/ipc.ts`.
* **`1.7.3`'s one mutation survivor is a typographic gutter**, recorded beside the code and
  waiting on `1.7.5`'s native-reader review rather than on an assertion.

* **`build_receipt_model` is an ownerless function.** §13 specifies it, it needs `CompletedSale`,
  and no `Files:` or `Tests:` line anywhere claims it. Whoever takes `1.4.1` or `1.8.3` assigns it;
  §2m has the reasoning and `1.7.1`'s phase entry carries the note.
* **`1.7.3` discharged `1.7.1`'s `validate` obligation.**
  `an_invalid_model_is_refused_before_anything_is_laid_out` is the test, and it is the only thing
  forcing a call the public struct shape deliberately cannot.

* **The string-literal lint conventions §10 promises still does not exist, and now nothing
  excuses it.** *"The catalog is the single source for UI strings; a string literal in a component
  is a lint failure."* `check-logical-css.sh` polices CSS sides and knows nothing about strings,
  so the rule is written and unenforced — the same state `1.11.2` found the RTL rule in. Until
  `1.11.1` there was no catalogue to point a component at and the gap was inert; there is one now.
  It has **no owning microstep**, which makes it the same class as §3's four ⚠️ OPEN items.
* **`1.4.1` is less startable than §1's file check says**, and §4 recommended it for two
  editions. `CartLine` is typed in `PriceOrigin` and `DerivedWeight` (`1.2.4`, blocked by #71) and
  `LineDiscount`/`BasketDiscount`/`PriceOverride` (`1.4.5`), and **none of those seven names
  exists under `crates/`** — measured with `grep -rn --include='*.rs'`, all zero. Taking it means
  either bringing `1.2.4`'s pure half with it or inventing placeholder types, and neither is what
  its entry describes. Scope that honestly before picking it.
* **`1.11.12` is no closer.** It still needs rendered screens that do not exist, and `1.11.1`
  created none.
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
| ~~`1.11.1` — i18n~~ | **DONE, #212 on 21 September.** §2l. Two of this row's three claims were wrong and are worth keeping as a shape: it did **not** carry `PLANNED` names and took no `protected-paths` red, and *"scope it rather than start it"* undersold it — the scoping question that mattered was not what it needed but what *"by default"* already appeared to be true of |
| `1.11.4` — Lock / PIN | **Soft-blocked.** All four IPC commands it drives are absent — `src-tauri/src` has no `commands/`, and `src/lib/ipc.ts` does not exist. It would be tested entirely against invented mocks |
| `1.11.5` — Sale screen | **Blocked.** `CartSnapshot` does not exist (`packages/api-types/src/index.ts` is `export {};`), and it would rewrite the green `1.11.0` canary |
| `1.3.3` — `compute_line_tax` exclusive | Technically buildable, but **no `Done when` line**; document order puts the externally-blocked `1.3.2` first; both edit the same file |
| `1.3.2`, `1.3.5` | **Blocked, and by nothing anyone filed** — `ref/domain-api.md:1299` and `:1290`. See §3 |
| `1.6.2` — Argon2id PINs | **Blocked twice**, neither time by code: `just bench-gate pin-verify` refuses until #68, **and** `ref/security-compliance.md:415` |
| `1.2.3` | Blocked three migrations deep — its FTS repository needs `0007`'s tables |

**Fifteen executable Phase-1 microsteps carry no `**Done when:**` line at all** — `1.2.0`
`1.3.2` `1.3.3` `1.4.1` `1.4.2` `1.4.3` `1.4.4` `1.4.5` `1.4.7` `1.4.8` `1.4.10` `1.5.2` `1.5.4`
`1.7.6` `1.7.8`. **Three left it on 22 September** — `1.7.1`, `1.7.4` and `1.5.1` — taking the
running total to five ever, after `1.1.9` and `1.6.6`, and all three authored the line in **its
own commit before any code**. That is the order conventions §6 asks for, and it is now visible in
the history three times rather than asserted in a PR. **Eleven of the fifteen remaining are group
1.4**, which is the cart, so whoever opens that group writes most of what is left. Re-run the
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

It prints `15`, and 113 `### 1.x` headings against 112 executable microsteps — the difference
being `1.1.2`:

* **`1.7.1` gained one on 22 September**, authored before the code and then *widened* — the line
  as written named ten tests and sixteen shipped, because two reviews found six more rules worth
  holding. A completion condition authored first is a floor, not a forecast.
* **`1.1.9` gained a real `Done when`** when it completed on 14 September — §1 records that as the
  third of the three deletions its completion required. It is no longer in the list.
* **`1.2.0` is the sole remaining `**Current half done when:**` case.** That is deliberately not the
  literal string the checker matches, and it also carries a `**Full-step status:**` marker, so rule
  3 refuses a completion claim and rule 4 never gets the chance.
* **`1.1.2` has no `Done when` and correctly never will.** The walk turns up 113 `### 1.x` headings
  against 112 executable microsteps, and `1.1.2` is the difference: its own body says
  *"**Concordance only:** this retained anchor is not an executable microstep"*. Anyone re-running
  this count mechanically will find it and must exclude it.

For the fifteen, checker rule 4 refuses a completion claim until one is written, and the issue form
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
4. **DISCHARGED by #212 on 21 September, and it paid for itself.** The ruling was that `1.11.1`
   owns `<html dir="rtl" lang="ar">` **by default** and *"Arabic as the rendered default"*, and
   that supplying `dir="rtl"` to jsdom is a test-fixture fact rather than that deliverable. It is
   what stopped `1.11.1` from asserting on a root the harness had already set and calling it
   done — the assertion was green before a line was written. What discharges it:
   `installLocale`, called from `main.tsx` before `createRoot`, and two tests that separate the
   function from its call site. §2l. Kept here because the *shape* recurs: whenever a fixture and
   a deliverable would produce the same observation, the test has to start from the opposite one.
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

**91 evaluations in the trailing month: 32 bypass, 58 pass, 1 fail**, re-read live on
24 September after #247. It read 82 (30 bypass) on 23 September and 68 (28 bypass) on 21 September. The four
newest bypasses are all frozen-surface reds, and **each names exactly one failing check**:

| Suite | PR | What was frozen |
|---|---|---|
| `4173862367` | #218 (`1.7.3`) | the catalogue ceiling |
| `4196274812` | #228 (`1.5.3`) | the catalogue ceiling |
| `4206181799` | #240 | four workflow files |
| `4206239586` | #239 | `.github/labeler.yml` |

The last three were each taken **after the operator read the diff and said yes**. #239 was brought up
to date on the server before its bypass, so that `strict: true` would not add a second failure to
its entry.
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
5. **DONE on 24 September — #203 closed by #240.** The operator read the upstream diff and approved
   the re-pin (§2r). What stood here:

   **#203 — decide what to do about the moved `dtolnay/rust-toolchain` tag.** The fix is not
   mechanical: either re-pin six sites to `02cb101ec7c4` *after reading what moved between the two
   commits*, or keep the pins and change the comments to something upstream cannot invalidate.
   Either way it is a `.github/workflows/**` edit and therefore a deliberately-red
   `protected-paths` PR, and either way somebody has to look at an upstream diff and decide whether
   to adopt it. Leaving the weekly workflow red is the option with a real cost: the next genuine
   finding arrives as "still failing". §3 has the diagnosis. **The issue also needs a label and a
   board item** — it has neither, which is why it went unnoticed for three merges.
6. **`1.7.5` needs a native reader, and nobody has been asked.** Its `Done when` ends *"every
   changed Arabic/bilingual pair has a dated `docs/drills/` record naming the commit and native
   reader. A hexdump cannot show a lost medial form."* The code half is startable today — `1.7.3`
   renders and `1.7.4` emits — and the other half is a person looking at seven rendered receipts
   and signing a record. It is the same shape as `1.7.2`'s font choice, which sat unblocked and
   unasked until somebody answered it, and it is the first item on §4's shortlist.

   **The question is who, and it may well be you.** This is a product for the Jordanian market;
   if the operator reads Arabic, the review is an hour with seven PNGs rather than a hiring
   problem. Nobody has written down which it is, so `1.7.5` looks startable and is not quite.
   §9's hardware-lab checklist wants the same reader again on paper at check 1, so the answer is
   needed twice regardless.
7. **Four questions filed on 24 September that only people can answer**, each with the full
   form on its issue:
   - **#231**, an operator decision: is the cash-rounding step a jurisdiction policy, as `0003` put
     it, or the merchant's store setting, as three documents say?
   - **#232**, for the tax adviser in #70's sitting: the SST base order and fixed part, and the
     zero-rating reasons.
   - **#233**, for the merchant: what second factor the counter can have.
   - **#237**, for the merchant: which coins actually cross the counter. The plan lists 25 fils and
     omits 5, which contradicts its ten-fil step.

   **Two of them can go in one meeting**: #232 with #70's adviser, and #237 with the merchant
   conversation merchant decision 2.1 already needs.

   **A fifth, filed the same afternoon: #244, an operator decision.** The SQLite WAL-reset fix is
   one `rusqlite` bump away, and `sqlx-sqlite`'s `libsqlite3-sys <0.38.0` bound blocks it. Wait for
   `sqlx`, patch `sqlx-sqlite`, or split the workspace. Nothing breaks while it waits, because the
   one-connection rule holds, but every storage design taken until then works around it.

---

## 8 · The repository queue

| Item | State |
|---|---|
| Board sync, four views, repo description and topics | **done** |
| The `0005` documentation preconditions | **done** — #116 |
| The twelve queued documentation corrections | **done** — #118, #121, #122, #123 |
| Ruleset control | **done** — four rulesets, all active, all checked in and matching live, re-diffed by hand 21 September |
| Promotion tooling | **done** — `just promote-merge <pr>` plus a push-only `promotion-shape` job |
| Hook installation proof | **done** — #134, `scripts/check-hooks-installed.py` |
| Actions SHA pinning | **done** — `sha_pinning_required: true`, repository-wide |
| Tag signing | **done** — key registered 9 September |
| **`#115`'s enforcement remainder** | **not done, and tracked by #235 since 24 September.** No gate diffs `.github/rulesets/` against live; no restore has been round-tripped; `gh-protect.sh` still exits 3, and its refusal text describes a world that ended when the rulesets landed |
| **A recurring bypass-ledger check** | **not done.** 32 of 87 evaluations in the trailing month were bypasses (re-read 24 September), and nothing in `scripts/`, `.github/workflows/` or the justfile reads the rule-suites endpoint. `1.6.6b`'s entry shows what a *readable* bypass looks like (§2k), which is the argument for the check rather than against it |
| **Selected-Action allowlisting** | **not done** — `allowed_actions: "all"` |
| **Issues exception-only** | **not done — and it is not a wording fix.** #166 corrected `01-microstep.yml:2` and `CONTRIBUTING.md:81`, which both claimed issues were "the normal way work enters this repo". But `03-github-workflow.md` §4 still *requires* an issue for "the microstep you are starting now (one at a time — WIP = 1)", so making issues exception-only means changing §4 — a policy decision, not a template edit |
| **Claude read-only permissions** | **not done** — #114. `.claude/settings.local.json` is `{}` |
| **Matrix on every PR** | **not done.** Removing `cross-platform`'s promotion-only `if:` — do it **last** |
| **The `staging → main` promotion** | **not done, and it needs a person.** See §9 — it is now the single highest-value follow-up in the repository |

---

## 9 · Promotion and the release path

### `development → staging`

`staging` is **68 behind at `1ccdd63`** — `git rev-list --count origin/staging..origin/development`,
re-measured 24 September, and the `origin/` spellings matter: a local `staging` left at #91 answers
far more. It was **60 at `b9fcb28`**, the #229 tip the previous edition predicted it could not
measure, which confirms the rule a second time: 56, 57, 58, 59, 60, then eight more for #238, #240,
#239, #242, #243, #245, #246 and #247, one per merge and no promotion between.

> **This row is stale by one the moment a handoff merges, and that is structural rather than
> careless.** An edition measures the distance while writing, which is necessarily *before* its own
> documentation commit lands — so the number it records is true at the microstep tip and one short
> at the tip a reader actually has. It read 56 at `c289466` and 57 at `c285761`, which is #225,
> this document. Every edition has done this.
>
> **So the row now names the commit it was measured at**, and the rule for the next one is: write
> the distance at the microstep tip, then re-measure and correct it in the *following* edition, or
> quote the reference point as these two rows now do. Do not "fix" it by adding one and hoping —
> a promotion PR that lands between the two moves it again. The gap now carries **seventeen** microsteps — `1.11.3`, `1.9.1`, `1.1.9`, `1.2.6`,
`1.11.6`, `1.11.11`, `1.9.2`, `1.7.2`, `1.6.6`, `1.6.6b`, `1.11.1`, `1.7.1`, `1.7.3`, `1.7.4`,
`1.5.1`, `1.5.3` and `1.5.4` — migration `0005`, an embedded typeface, the audit chain's writer,
reader and verifier, the i18n infrastructure, a receipt that is now a model, a raster and a
printable document, the tender vocabulary, cash rounding and the denomination table — **and #240's
re-pin, which a promotion would exercise on the cross-platform matrix for the first time.** **A promotion
also carries four new runtime dependencies**, so the cross-platform matrix is doing more than it
was. Open a promotion when you want the cross-platform matrix
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

**Settled on 24 September, the thing this paragraph said to settle before the next promotion.**
`workflow-analysis` had been red on `development` since 21 September (#203), and #240 closed it:
the check passed on `development` after both of that day's `.github` merges. A promotion now carries
the re-pin to the cross-platform matrix, which is the first time the canary's pinned site runs it.
`workflow-analysis` is still **not** one of the six required contexts, so it cannot wall a
promotion, which is why #106 once merged with it red. Read it anyway.

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

gh pr merge <N> --admin --match-head-commit "$OID" --squash --delete-branch \
  --subject "$LIVE (#<N>)" --body-file /tmp/body.txt
```

> **`--admin` was missing from this recipe until 22 September, and the omission costs a round
> trip.** Without it `gh` answers *"the base branch policy prohibits the merge"* and offers
> `--auto`, which would wait forever on a check that is red on purpose. `--admin` is what takes
> the ruleset's `bypass_mode: "pull_request"` — the mechanism `CLAUDE.md` describes, recorded by
> GitHub as a bypass event, and the whole reason the bypass is kept rather than the ruleset
> disabled. Found by running the recipe as written on #218.
>
> **This path deleted only the remote branch on #218, and both refs on #228.** `just merge` always
> cleans up both sides. On #218 the recipe left the local branch behind as `[gone]`, which is why
> that count moved 30 → 31 on a day a single branch merged. On #228, `gh pr merge --delete-branch`
> ran with the branch checked out, switched to `development`, fast-forwarded it and deleted the
> local ref too. The difference is not measured beyond that, so check `git branch -vv` afterwards
> rather than assuming either outcome.
>
> **On #228 the bypass took a question first.** The agent stopped before `--admin` and put the
> six-line frozen diff to the operator, who approved it.
> `CLAUDE.md` says the frozen surface is *"deliberately red until a human reads the diff"*, and the
> author of the diff is not that human. Keep asking.

**Before merging, confirm the red names only the pinned file and nothing else**, and put a
"Manual review note" section in the PR body stating what changed, that no gate is weakened, and
which suites were run. #218 is the worked example: the refusal reads *"candidate changed trusted
policy blob or mode on 1 path(s) — changed by this branch: scripts/check-test-catalog.py"*, which
is a materially better ledger entry than one naming six checks that never ran.

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
| **So does every microstep that lands a test `ref/test-catalog.md` names** | Retiring its `PLANNED` entry means editing `scripts/check-test-catalog.py`, which is in the frozen policy surface. The edit is **not optional** — leaving the entry in place produces two assertion-3 violations. Budget for §9's manual merge; §2k has the worked example. **29 Phase-1 steps** carry `PLANNED` names, re-counted 23 September. `1.11.1` stood in this row long after §4 recorded that it never carried any. So check the step you are taking with `grep -n '"<step>"' scripts/check-test-catalog.py`, not by its presence in a list |
| **A scheduled workflow can go red with no repository change, and its issue lands nowhere** | #203 was filed automatically by `security.yml`, carries **no labels and no board item**, and sat open through three merges while every handoff reported "ten open issues". `gh issue list --state open` is the command that finds it; the board is not. Check it when you count |
| **An upstream mutable tag moving makes a correct SHA pin look stale** | zizmor's `stale-action-refs` compares the `# v1` comment against where the tag points *today*. When upstream moves `v1`, six green pins become six warnings and the job exits 13 — with nothing in this repository having changed. Diagnose before re-pinning: the pin is what protected you |
| **`just fmt` does not fix `organizeImports`** | Biome *assists* are applied only by `biome check --write`, which no recipe wraps. Run `pnpm biome check --write <files>` by hand |
| `git add -A` sweeps in whatever is lying around | `git add <only the files for this concern>`. `PROJECT-GUIDE.md` was the classic case and is **tracked since #242**, so the tree is clean today, but the habit is what protects it tomorrow |
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
| **A mutation sweep over a property test leaves `proptest-regressions/` behind** | proptest persists the shrunk counterexample it finds against the *mutated* code: `crates/pos-domain/proptest-regressions/tender.txt` appeared untracked after `1.5.3`'s sweep. Those seeds record sabotage, not a failure, and the repository commits no regression file today. Delete the directory, and read `git status --short` after every sweep before staging anything |
| **The `risk: money path` label misses `tender.rs` and `pricing.rs`** | `.github/labeler.yml` globs `money*`, `tax*`, `cart*` and `discount*`, and no planned module is named `discount*`. Add the label by hand on a tender or pricing PR. Fixing the globs is a frozen-surface edit and its own deliberately red PR (§4) |
| **The board's item listing can omit a live item** | #227's item read `Done` and `isArchived: false` when queried by node id, while `gh project item-list` and GraphQL's `items { totalCount }` both said 28 without it, for at least 2h44m after creation. Count from the node when a figure matters, and write the count as a floor |
| **Two frozen-surface PRs open at once** | `development` is `strict: true`, so the second becomes *behind* the moment the first merges, and a bypass of a behind branch can put a second failure into its ledger entry. Merge one, run `gh pr update-branch <n>` on the other (a server-side merge commit that squash flattens, and one the local `commit-msg` hook never sees), wait for its checks, then take the second bypass. Done this way on 24 September with #240 then #239, and both entries name one check |
| **`gh pr merge` fast-forwards your local `development` only when run from the merged branch** | #240's merge, run from its own branch, switched to `development` and pulled. #238's `just merge`, run from `development`, left the local ref one merge behind. Run `git pull --ff-only` after every merge regardless: a stale local `development` is how #184's empty commit happened |
| **No Dependabot PR does not mean no update** | Dependabot drops an update that Cargo cannot resolve, and it leaves no PR and no issue behind. `rusqlite` 0.40, which carries the SQLite WAL-reset fix, was never proposed, because `sqlx-sqlite`'s `libsqlite3-sys <0.38.0` bound and Cargo's `links` rule make it unresolvable (#244). For a dependency that matters, compare `Cargo.lock` against crates.io yourself, as `ref/plan-validation.md` §5's currency table does |
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
- **RESOLVED on 24 September — `PROJECT-GUIDE.md` is tracked (#242)**, at the operator's word, with a
  status banner and an errata table of eight claims measured false. Its still-true gap is #241.
  What stood here:
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
  **thirty-one feature branches with `[gone]` upstreams** — re-counted 22 September with
  `git branch -vv | grep -c ': gone]'`, and the row said fifteen until then, having been written
  when it was true and never re-measured. Also a `refs/original/refs/heads/main` filter-branch
  backup at `a7c2379` and seven `refs/codex/turn-diffs/checkpoints/*` refs. `git branch -d` refuses
  them because squash merges break ancestry; `git branch -D` is safe for all thirty — each is
  merged content on `development`.
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

   **A quotation is a claim too, and this document got one wrong.** §3 argued that #69 gates no
   Phase-1 microstep by quoting `phase-1` as saying *"…so this microstep cannot wait for it"*. That
   clause is in no file in this repository. It survived several generations because a quotation
   *looks* like evidence — nobody re-greps a sentence in quotation marks. The 21 September sweep
   found it by checking all 76 `file:line` references mechanically: 74 resolved, three landed on the
   wrong line, and one landed on text that did not contain what was quoted. **Grep the quotation,
   not just the line number.**
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
    **four** microsteps in a row where that second pass paid and the sweep alone would have
    shipped the defect. **Run both, in that order, and do not treat a green sweep as the end.**

    `1.11.1` is the fourth and its variant is the cheapest to miss: the sweep was green and the
    read found two guards that were each checking *the adjacent thing* — a CSS variable that
    carried the right family while nothing resolved it, and a numeral rule that refused `2` and
    let `٢` through. Neither is a missing guard or a wrong composition. Both are a guard aimed one
    step short of the property it was written for, which a mutation of the code under it can
    never reveal.

    **`1.7.1` is the fifth, and it names where to look.** Its three read-only findings all came
    from re-reading the reference's four non-negotiable rules *against* the validator rather than
    reading the validator — which turned up the rule that had no code at all (a line with no name),
    the coverage table that needed a compile error rather than a count, and a comment describing an
    error field that did not exist. **The sweep tests what you wrote; the read tests what you were
    asked for.** Open the spec, not the diff.
11. **Follow the static analyser to the line it points at, then look around it.** CodeQL flagged a
    non-sensitive register id in `1.6.6b` — a false positive under this repository's own
    never-list — and three lines below it sat raw, unvalidated file content being printed into a
    document read as evidence. Arguing with the tool would have shipped that. This is the second
    microstep running where a CodeQL alert repaid being followed rather than dismissed.
12. **A list where most entries are right reads as verified, and nobody re-checks the rest.**
    §4 named five microsteps as carrying `PLANNED` catalogue entries and therefore taking §2k's
    `protected-paths` red. Four did. `1.11.1` was never in the ceiling at all, and #212 merged
    green — a budgeted red that never came, which is the harmless direction. The harmful one is
    the same sentence pointing the other way. **Check the membership for the item you are acting
    on**, not the list's reputation: one `grep` against `scripts/check-test-catalog.py` answers it.
13. **A guard that has been true for free is not a guard.** `<html dir="rtl" lang="ar">` has been
    in `index.html` since the scaffold and `Sale.test.tsx` asserted on it and passed — so "Arabic
    is the default" *looked* proven while nothing in the product decided it. A fixture and a
    deliverable that produce the same observation are indistinguishable until the test starts from
    the opposite state. §5 ruling 4 caught this one a week before it was due, which is the only
    reason `1.11.1` did not ship the tautology. **Ask what would still be green if the code were
    deleted.**
14. **When a sweep finds an untested parameter, the usual repair is deleting it.**
    `installLocale` was written with an optional locale nobody passes, and a test for it would
    have been a test of an API with no caller. The step that needs the argument adds it back with
    its caller and its test. Coverage bought by writing tests for speculative surface is the
    cheapest kind and the least worth having.
15. **A test fixture with empty optional fields tests the fields it populates.** `#[serde(skip)]`
    on `masked_pan` survived `1.7.1`'s round-trip test, because the fixture left that field and
    five others at `None`, and `None` round-trips through a skipped field unchanged. A
    serialization test earns its name only against a fully populated value; the sparse fixture
    belongs beside it, not instead of it.
16. **A generator is not a coverage claim.** `prop_a_model_mixing_two_currencies_is_refused` said
    in its own comment that `any::<i64>()` covered *"the full range including zero"*. It reaches
    zero only by chance, and a validator that skipped zero amounts survived the sweep because of
    it. Conventions §5.1 already says the rule — a bounded universal claim uses an exhaustive
    `#[test]` loop when that loop is feasible — and the failure mode is that the comment sounds
    like the loop. **Write the number of cases you are actually asserting, or write the loop.**
17. **A mutation harness must touch the file it restores.** `shutil.copy2` preserves mtime, so
    cargo's freshness check reused a mutated binary after a restore and reported two failures
    that did not exist. The harmless direction; the other one is a harness that reports **caught**
    for a mutation that never compiled. A sweep is evidence only if its build is.
18. **The document nobody's gate can read is the document that goes stale.** Four documents were
    corrected on 11 September; the one sentence that survived lived two more days in the HTML file
    no checker parses, until a deliberate prose sweep caught it at #163 — along with a second error
    in the same page that no gate had ever read. The sweep, not a gate, is still the only thing that
    reads that file.
19. **A crate named in a reference can be unshippable by the time the microstep arrives.**
    `1.7.3`'s entry names `cosmic-text`; RUSTSEC-2026-0206 and RUSTSEC-2026-0192 had since retired
    both crates the obvious lighter substitute is made of, and `cosmic-text` depends on one of
    them — so no reading of that entry could pass `supply-chain`. A reference records what was
    true when it was written. **Run `just audit` before the branch, not after the code**, whenever
    a microstep's `Files:` line touches a manifest.
20. **The `Done when` is a floor, not a forecast, and the gap is where the reviews paid.**
    `1.7.1`'s authored `Tests:` line named ten and sixteen shipped; `1.7.3`'s plan line named
    seven and eighteen shipped. In both cases the additions came out of the two reviews rather
    than out of scope creep — which is the argument for authoring the line first and letting it
    grow, instead of writing it to match what was built.
21. **Ask what a test would still pass if the feature were deleted.** `1.7.3`'s
    `raster_width_matches_profile` asserted `ink() > 0` and passed with **every glyph missing**,
    because the horizontal rules are drawn separately. Ink on the page is not evidence that
    anything in particular was set. The repair was comparative — more text must mean more ink —
    and the same shape answers the y-flip, which needed a *balance* rather than a presence:
    1 234 dots above the baseline against 327 below when it is right, 148 against 1 340 when the
    page is mirrored.
22. **A payload of arbitrary bytes makes "this sequence never appears" a probabilistic claim.**
    `1.7.4`'s drawer pulse must not be in a receipt's bytes, and the assertion everybody reaches
    for — scan the stream for `1B 70` — would have been **intermittently red**: the raster payload
    is a bitmap, so those two bytes occur by coincidence about once in every 65 536 pairs, which a
    576-dot receipt reaches within roughly 900 rows. It would have passed on the fixtures and
    failed in the field, for a reason nobody would find quickly. **Ask where a sequence may not
    appear, not whether it appears**, and write the test that makes the coincidence deliberate.
23. **When a migration already describes a type, the migration wins — check which came first.**
    `1.6.3` built its grid and `1.6.1` seeded from it; `1.5.1` found the opposite, because
    `tender_type`'s complete seed shipped inside `0005` months before the domain type existed and
    a committed migration is never reopened. The test therefore pins the type against literals
    **transcribed from the `.sql`**, not built from the function it checks — a fixture derived
    from its subject asserts nothing. Several microsteps ahead are in the same position:
    `1.2.4`'s barcode rules, `1.3.x`'s tax rules, `1.10.x`'s stock columns.
24. **A second definition of one mapping is a second chance to get it wrong, even when both are
    already checked.** `as_str` and `#[serde(rename_all = "snake_case")]` produce the same words,
    and both were separately checked against a `CHECK` constraint — so each was right and nothing
    said they agreed with *each other*. They coincide, which is exactly when a change to one goes
    unnoticed. The third edge of a triangle is cheap and is the one nobody draws.
25. **A document that measures the repository cannot measure itself.** Every edition of this file
    has recorded `staging`'s promotion distance one short, because the number is taken while
    writing and the edition's own merge commit lands afterwards. It is not carelessness and it
    cannot be fixed by adding one — a promotion or a microstep landing in between moves it again.
    **Quote the commit a drifting number was measured at**, and let the next edition correct it.
    The same shape reaches any count that includes the act of recording it.
26. **A superseded source can hand you the right conclusion for the wrong reason.** `1.5.3`'s
    docs-first commit keyed cash rounding on `is_cash_counted`, which is right, and justified it
    with master plan C.6's *"− cash rounding given away"*, which `00-master-plan.md` §4a row 176
    supersedes because it *double-counted* the rounding. `ref/domain-api.md` §11, the normative
    replacement, says the opposite: the rounding carries no term at all. `CLAUDE.md`'s warning lists
    superseded names and rules, which a reader would type or implement. A superseded *formula used
    only as a reason* is harder to see, because nothing in it is going to be implemented. **Check §4a
    before citing anything from `docs/plan/`, including a reason you never intended to build.**
27. **When a rule delegates to an arithmetic, assert the delegation.** `1.5.3`'s first sweep caught
    every mutation, but forcing the direction to `Up` inside the rule was caught by one example
    only. The property checked *where* rounding applied and never *how*. Asserting
    `Ok(r) == compute_cash_rounding(remaining, step, dir)` made the rule's one job — deciding
    whether, never how — a property rather than a hope, and `money.rs`'s proration property had
    already shown the shape. **After a green sweep, count how many tests catch each mutation**; a
    count of one is where to look next.
28. **A step can be startable by its signatures and not by its tests.** This file called
    `1.5.3`'s inputs "all present", and they were. But four of its six named tests described a
    settlement over a `Tendering` that no crate defines. The file-existence check reads `Files:`
    lines and the previous edition read signatures; neither reads `Tests:` lines. **Before calling
    a step startable, ask what type each named test would construct.**
29. **A completion condition authored first can be refuted by the plan's own data, and that is the
    condition working.** `1.5.4`'s docs-first commit wrote *"every denomination is a whole number
    of qirsh"*, and the plan's own table failed it on the first run, because 25 fils is not. The
    code was right, since it transcribed the table, and the *plan* disagreed with itself. The
    professional move was neither to delete the check nor to edit the data to pass it. It was to
    correct the claim openly (in the code commit's message and on the issue), keep the plan's values,
    and file the contradiction for the person who can resolve it (#237). **A failing test on the first
    run is sometimes a finding about the specification, and the fix belongs to whoever owns that.**
30. **"No issue tracks it" has a half-life too.** Seven items rode in this file under that phrase,
    some for eleven days. Each edition re-measured them faithfully and nothing moved, because a
    sentence in a handoff asks nobody in particular. Filing all seven took one morning, and it put
    each on the board where the operator plans. **When a document keeps saying something is
    untracked, the cheap half of the decision is to track it**, and the handoff should record the
    issue number rather than the absence.
31. **The absence of an update is not evidence that none exists.** `rusqlite` 0.40, released with
    `libsqlite3-sys` 0.38 in May, carries the fix for a corruption bug in this register's storage
    engine. This repository pinned 0.39 on purpose, because of the `links` collision recorded in
    `docs/phase-0-remaining-setup.md` (`7dda4f4`). Nothing revisited the pin, because the tool
    that re-proposes upgrades drops one that Cargo cannot resolve and says nothing. #234 found the
    consequence only by running the method its OPEN item had written down: ask the runtime for its
    version, then read the upstream advisory. **When a question names its own method, run the
    method.** It is usually cheaper than the reasoning that stood in for it, and it is the only
    thing that turns a pin nobody questions into a reason somebody wrote down.
