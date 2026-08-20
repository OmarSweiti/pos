# POS implementation master plan

The spine. Everything else in this directory hangs off it.

Three documents govern this project:

| Document | Answers |
|---|---|
| [`business-functional-master-plan.md`](../plan/business-functional-master-plan.md) | **what** to build, and why Jordanian law and the shop floor demand it |
| [`engineering-blueprint.md`](../plan/engineering-blueprint.md) | **how** to build it — stack, architecture, standards |
| **this set** | **what to type tomorrow morning** |

---

## 1 · Verdict on the master plan

**Build on it.** It is materially better than the specifications most commercial POS products are built from, and it gets right the four things cheap POS software gets wrong — the four you cannot retrofit:

1. **Sales are immutable facts; corrections are new documents.** Eliminates the hardest class of sync conflict and the hardest class of audit dispute.
2. **Price and name are captured onto the sale line at sale time.** Half of all historical-data bugs in POS systems are this rule missing.
3. **Stock is a ledger, not a column.** Append-only rows merge across offline registers without a lock.
4. **Tax rates are data with effective dates.** Jordan changes reduced rates by decree; a hardcoded 16% is a re-release every time.

Add correct instincts on semi-integrated payments, on treating card timeouts as *unknown* rather than *failed*, on rasterising Arabic receipts instead of fighting codepages, and a rare willingness to enumerate 72 edge cases before writing code.

**Four claims are wrong.** One invalidates a phase gate. Full audit with sources: [`ref/plan-validation.md`](ref/plan-validation.md).

| | Master plan says | Verified, Aug 2026 | Effect |
|---|---|---|---|
| **C-1** | JoFotara has a sandbox; Phase 2 exits when four sandbox docs clear | **No sandbox exists** | The Phase-2 gate is unbuildable. Replaced with a conformance harness + mock ISTD; the real hop becomes a Phase-5 certification milestone |
| **C-2** | Order-level discounts must be prorated across lines | ISTD rejects global discounts entirely — **per-line, as a percentage** | Proration plus an absolute→percentage conversion with a round-trip self-check |
| **C-3** | Round once per line to fils | ISTD recomputes at **9 decimals**, tolerance **< 0.001 JOD** | A pre-submit totals self-check that dead-letters drift locally |
| **C-4** | Threshold ~50k goods / 30k services | **75k goods / 30k services / 10k special-tax goods** | Seeded defaults; a tobacco-selling minimarket crosses at 10k, not 75k |

**Twelve gaps** the master plan leaves to the blueprint, and the blueprint does not carry either — each gets a design and microsteps: local backup and tested restore · sequence integrity · device provisioning · the business-date algorithm · the i18n mechanism · permission-enforcement teeth · the audit hash-chain spec · proven PII scrubbing · budgets as CI benchmarks · a Jordanian seed fixture · `Money` without a currency · `sale_line.qty` in the wrong unit.

---

## 2 · The document set

| File | What it is |
|---|---|
| **[`01-conventions.md`](01-conventions.md)** | the engineering law: nine invariants, naming, errors, testing, definition of done. **Read once, keep open** |
| [`phase-0-closeout.md`](phase-0-closeout.md) | finish what is started — 1–2 days |
| [`phase-1-sellable-mvp.md`](phase-1-sellable-mvp.md) | cash, tax, receipts, Arabic — 8–12 weeks |
| [`phase-2-money-grade.md`](phase-2-money-grade.md) | cards, refunds, shifts, fiscal — 8–10 weeks |
| [`phase-3-connected.md`](phase-3-connected.md) | sync, back office, customers — 8–10 weeks |
| [`phase-4-depth.md`](phase-4-depth.md) | promotions, supply, reports, multi-store — 8–10 weeks |
| [`phase-5-harden-and-launch.md`](phase-5-harden-and-launch.md) | certification, compliance, packaging — 6–10 weeks |

**Reference** — consulted from the phase files, not read front to back:

| File | Owns |
|---|---|
| [`ref/plan-validation.md`](ref/plan-validation.md) | the audit, the four corrections, every source |
| [`ref/domain-api.md`](ref/domain-api.md) | every `pos-domain` type and signature |
| [`ref/schema.md`](ref/schema.md) | migrations 0002→0011, SQLite and Postgres |
| [`ref/tax-jordan.md`](ref/tax-jordan.md) | GST as an engine: categories, rates-as-data, rounding, the filing report |
| [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) | the highest-risk component in the product |
| [`ref/ipc-contract.md`](ref/ipc-contract.md) | every Tauri command, its capability, its audit |
| [`ref/sync-protocol.md`](ref/sync-protocol.md) | ownership classes, push/pull, chaos, accepted risks |
| [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) | traits, the Arabic raster pipeline, the lab checklist |
| [`ref/ui-spec.md`](ref/ui-spec.md) | screen by screen, RTL mechanics, keyboard map |
| [`ref/test-catalog.md`](ref/test-catalog.md) | E.1–E.72 → named tests. **The coverage matrix** |
| [`ref/security-compliance.md`](ref/security-compliance.md) | PDPL, PCI, the audit chain, permissions, secrets |
| [`ref/merchant-decisions.md`](ref/merchant-decisions.md) | the questionnaire to fill in with the merchant |

---

## 3 · The phase map

Blueprint Part 10 and master-plan Part G, reconciled with the corrections and the gaps.

| Phase | Adds | A real store could… | Exit gate |
|---|---|---|---|
| **0** | workspace hygiene, CI on a remote, product identity, panic lints | …nothing. It compiles and ships | 7 checks, all green |
| **1** | `Money`/`Qty`/ids/time · catalog + FTS · **tax engine** · cart machine · cash tenders · users/permissions/audit chain · **Arabic receipts** · atomic finalize · **backup** · sequences · stock ledger · RTL UI · seed fixture | **…sell for cash, all day, fully offline, in Arabic, with correct GST and a printed receipt** | 10 demonstrations, done with the cable unplugged |
| **2** | card terminal + the `Unknown` protocol · refunds/returns/exchanges · shifts, cash movements, X/Z · **the fiscal pipeline** · electronic journal · diagnostics | …take cards that reconcile, handle returns without being defrauded, close a shift that balances | 12 demonstrations + drills |
| **3** | sync push/pull · **chaos convergence** · customers & loyalty under PDPL · back office · device provisioning · licensing · Sentry | …run more than one register, administer centrally, keep customers | 11 demonstrations |
| **4** | price lists · receiving + WAC · counts · transfers · **promotions** · report suite · **shelf labels** · multi-store | …run three stores with promotions and full inventory | three-store pilot week |
| **5** | soak · **Fiscal Certification** · PDPL walkthrough · PCI SAQ · restore drills · signing · staged updater · onboarding | **…be sold to someone who is not you** | someone else installs, provisions, and sells from the docs alone |

**Total: roughly 40–52 weeks solo**, and that is a range, not a commitment. Phases 1–2 deliberately front-load the unforgiving parts — hardware, money, offline, fiscal — because they dictate the architecture. Dashboards never do.

---

## 4 · What changed from Part G, and why

| Change | Reason |
|---|---|
| **Phase 1 grows** — `Money`/`Currency`/`Qty`, backup, sequences, business date, i18n, permission guards, audit chain, benchmarks, seed fixture | Every one is cheap now and expensive later. Three (G-11, G-12, G-2) are **impossible** once real sale rows exist |
| **Phase 2's fiscal gate is replaced** — conformance harness + mock ISTD + five goldens, instead of "four sandbox documents clear" | Correction C-1: there is no sandbox. The replacement is stronger — it runs on every commit forever, where a sandbox run is a one-off |
| **Fiscal Certification moves to Phase 5** as a self-contained, eleven-item milestone | It needs a merchant, real credentials, and a tax advisor's written answer. None of that is available now |
| **Electronic journal promoted into Phase 2** (master-plan J.1 already flagged it) | It is a thin UI over facts already stored, and support teams live in it |
| **Shelf labels promoted to a Phase-4 compliance feature** (master-plan J.3) | Jordan's MoITS enforcement statistics are dominated by price-display failures. This is not a convenience feature |
| **Backup moves from Phase 5 to Phase 1** | Between a sale and its successful push, the only copy of that money is one SQLCipher file |

---

## 5 · Working rhythm

- **One microstep, one commit**, with the step number in the message. A bisect that lands on a microstep tells you exactly what broke.
- **One group, one branch**, squash-merged. `main` is always green and always sellable-or-earlier.
- **Property tests are written with the code, not after.** They are the layer that finds what you did not imagine.
- **A microstep is done when its `Done when` line is objectively true**, checked by running its command — not when the code looks finished.
- **Stuck on the cart machine?** Groups 1.6 (users/audit), 1.9 (sequences), and 1.10 (stock) are independent. Go sideways rather than stopping.
- **Phase running long?** Cut scope *toward* the next row of the "a real store could…" table, never away from it.

---

## 6 · Risk register

The things most likely to hurt, and what is already in place.

| Risk | Likelihood | Impact | Mitigation | Owner |
|---|---|---|---|---|
| **ISTD spec differs from the reconstruction** | **high** | medium | Everything reconstructed lives in one module (`pos-fiscal/src/codes.rs`). Microstep 5.2.2 diffs the official spec and corrects it in one place | 5.2 |
| **No sandbox ⇒ first real submission is on a merchant's tax record** | certain | high | Conformance harness + mock ISTD + five goldens; certification uses low-value invoices immediately credit-noted; **merchant consent in writing** | 5.2 |
| **Jordan changes a rate by decree mid-build** | medium | low | Rates are time-effective data. A decree is a row, not a release | 1.3 |
| **Learning Rust slows Phase 1 badly** | medium | medium | Groups 1.1–1.3 are the steep part. Budget generously; the curve is real and it flattens hard | 1.x |
| **Acquirer terminal lacks a status query** | medium | **high** | Ask before choosing (11.3 in the questionnaire). Without it there is no safe timeout recovery — **choose a different acquirer** | 2.1.1 |
| **Arabic receipt looks wrong on real paper** | medium | medium | Raster pipeline + six goldens + the hardware lab, where a native reader confirms on paper | 1.7, 2.9.4 |
| **Performance degrades at year-one volume** | medium | medium | Soak test in Phase 5; budgets are CI benchmarks from Phase 1 | 5.1.1 |
| **PDPL enforcement arrives suddenly** | low | medium | Built to the law, not to the lag. Consent records, export, anonymisation, scrubbed logs — all tested | 3.4 |
| **Scope creep from J.1's deferred list** | **high** | medium | Every deferred item has a named hook and a merchant-decisions row. A "yes" is a phase, not a rewrite | ongoing |
| **Solo-developer bus factor** | certain | high | This document set *is* the mitigation. Phase 5 gate #10 tests it: someone else must install, provision, and sell from the docs alone | 5.6 |

---

## 7 · What "done" means

Nine claims. Each is a passing test, a signed checklist, or a named advisor — **never an intention**.

1. **Sells offline.** A cut cable changes nothing.
2. **Money is exact.** No float touches it; every total is a property test.
3. **Tax is correct.** The filing report reconciles to a hand-check on a scripted day.
4. **Arabic is right.** Confirmed on paper, by a native reader, and defended by golden files.
5. **Cards never double-charge.** `prop_no_input_sequence_yields_two_tenders_for_one_auth`.
6. **Refunds cannot exceed what was sold.** `prop_cumulative_refunds_never_exceed_sold_qty`, in any order.
7. **Registers converge.** `prop_both_databases_converge_byte_identical`, through a chaos week.
8. **Fiscal clears.** Eleven certification items, dated and signed — the *only* thing that makes "JoFotara compliant" true.
9. **Data is safe.** Restored on real hardware by someone who did not write the code, twice, and timed.

---

## 8 · Standing work after launch

- **Re-run [`ref/plan-validation.md`](ref/plan-validation.md) quarterly.** Rates move by decree, JoFotara adds waves and changes validation, the PDPL authority is still standing up. **A compliance claim has a shelf life.**
- **Watch the fiscal rejection rate.** A rise means ISTD changed something, and you will see it before the announcement does.
- **Keep [`ref/test-catalog.md`](ref/test-catalog.md) honest.** Every new surprise becomes E.73, E.74 — with a test, an accepted risk, or an out-of-scope. **A surprise that becomes none of the three will happen again.**
- **The hardware lab runs before every release.** A golden file proves bytes; only paper proves a receipt.

---

*The blueprint says how to build. The master plan says what to build and why the law demands it. This set says what to type, in what order, and how you will know it worked.*
