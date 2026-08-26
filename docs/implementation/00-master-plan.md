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

**Four claims were wrong, and two of the corrections were wrong in turn.** The audit of record is
[`ref/plan-validation.md`](ref/plan-validation.md); the controlling ledger — what is corrected, what
is superseded, and what is still open — is **§4a** below. Read §4a before acting on any of the four.

| | Master plan says | Current disposition (`2026-08-25`) |
|---|---|---|
| **C-1** | JoFotara has a sandbox; Phase 2 exits when four sandbox docs clear | **Conclusion retained, premise narrowed.** No public sandbox exists, so the Phase-2 gate is unbuildable and is replaced by a conformance harness + mock ISTD + five goldens. But the *specification* is obtainable now: obtaining and pinning it moves to microstep **2.7.0**, a precondition of everything fiscal |
| **C-2** | Order-level discounts must be prorated across lines | **Superseded.** The correction — convert to a percentage and gate on an exact round-trip — rejects valid documents. Proration stays; the wire format is exact line allowance **amounts** plus a document recap equal to their sum |
| **C-3** | Round once per line to fils | **Superseded.** "ISTD tolerates < 0.001 JOD" is not sourced. Round-once-per-line stays; the pre-submit check becomes a half-fil per-line comparison plus exact identities over the document's own carried values |
| **C-4** | Threshold ~50k goods / 30k services | **Numbers retained, categories corrected.** 75k goods / 30k services / 10k **producer of special-tax goods**. A minimarket that *resells* tobacco does not enter the 10k class |

**Twelve gaps** the master plan leaves to the blueprint, and the blueprint does not carry either — each gets a design and microsteps: local backup and tested restore · sequence integrity · device provisioning · the business-date algorithm · the i18n mechanism · permission-enforcement teeth · the audit hash-chain spec · proven PII scrubbing · budgets as CI benchmarks · a Jordanian seed fixture · `Money` without a currency · `sale_line.qty` in the wrong unit.

---

## 2 · The document set

| File | What it is |
|---|---|
| **[`01-conventions.md`](01-conventions.md)** | the engineering law: nine invariants, naming, errors, testing, definition of done. **Read once, keep open** |
| [`phase-0-closeout.md`](phase-0-closeout.md) | historical Phase-0 close-out record; not a current runbook |
| [`phase-1-sellable-mvp.md`](phase-1-sellable-mvp.md) | cash, tax, receipts, Arabic — 14–20 weeks |
| [`phase-2-money-grade.md`](phase-2-money-grade.md) | cards, refunds, shifts, fiscal — 10–13 weeks |
| [`phase-3-connected.md`](phase-3-connected.md) | sync, back office, customers, **operating the server** — 11–14 weeks |
| [`phase-4-depth.md`](phase-4-depth.md) | promotions, supply, reports, multi-store, the pilot — 9–12 weeks |
| [`phase-5-harden-and-launch.md`](phase-5-harden-and-launch.md) | certification, compliance, packaging, **commercial readiness** — 9–13 weeks |

**Reference** — consulted from the phase files, not read front to back:

| File | Owns |
|---|---|
| [`ref/plan-validation.md`](ref/plan-validation.md) | the audit of record and every source. §4a below, not this file, is the current correction ledger |
| [`ref/domain-api.md`](ref/domain-api.md) | every `pos-domain` type and signature |
| [`ref/schema.md`](ref/schema.md) | migrations 0002→0012, SQLite and Postgres. **Authoritative for every migration number** |
| [`ref/tax-jordan.md`](ref/tax-jordan.md) | GST as an engine: categories, rates-as-data, rounding, the sales-side reconciliation |
| [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) | the highest-risk component in the product |
| [`ref/ipc-contract.md`](ref/ipc-contract.md) | every Tauri command, its capability, its audit |
| [`ref/sync-protocol.md`](ref/sync-protocol.md) | ownership classes, push/pull, chaos, accepted risks |
| [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) | traits, the Arabic raster pipeline, the lab checklist |
| [`ref/ui-spec.md`](ref/ui-spec.md) | screen by screen, RTL mechanics, keyboard map |
| [`ref/test-catalog.md`](ref/test-catalog.md) | 92 edge cases → named tests, checked by a script. **The coverage matrix** |
| [`ref/security-compliance.md`](ref/security-compliance.md) | PDPL, PCI, the audit chain, permissions, tenancy, key custody, secrets |
| [`ref/merchant-decisions.md`](ref/merchant-decisions.md) | the questionnaire to fill in with the merchant |

---

## 3 · The phase map

Blueprint Part 10 and master-plan Part G, reconciled with the corrections, the gaps, and the
`2026-08-25` audit remediation.

| Phase | Adds | A real store could… | Exit gate |
|---|---|---|---|
| **0** | workspace hygiene, CI on a remote, product identity, panic lints | …nothing. It compiles and ships | 7 checks, all green |
| **1** | `Money`/`Qty`/ids/time · catalog + FTS · **tax engine** · cart machine · cash tenders · users/permissions/audit chain + `ApprovalHandle` · **Arabic receipts** · atomic finalize · **backup with a recovery code** · shift skeleton · sequences · stock ledger + `stock_adjust` · the sales-side tax report · RTL UI · seed fixture | **…sell for cash, all day, fully offline, in Arabic, with correct GST and a printed receipt** | 10 demonstrations, done with the cable unplugged |
| **2** | **2.7.0: obtain and pin the official ISTD specification** · card terminal + the `Unknown` protocol · refunds/returns/exchanges + minimal store credit · shifts, cash locations, X/Z · **the fiscal pipeline** · electronic journal · diagnostics · packaged-app smoke suite | …take cards that reconcile, handle returns without being defrauded, close a shift that balances | 14 demonstrations + drills |
| **3** | sync push/pull · **chaos convergence** · customers & loyalty under PDPL · back office with real human authentication · **multi-tenant isolation** · device provisioning · licensing · Sentry · **operating the server** | …run more than one register, administer centrally, keep customers | 15 demonstrations |
| **4** | price lists · receiving + WAC · counts · transfers · **promotions** · supplier tax invoices and filing periods · report suite · **shelf labels** · multi-store · **the pre-pilot gate** | …run three stores with promotions and full inventory | the pre-pilot gate, then 10 demonstrations and a three-store pilot week |
| **5** | soak · **Fiscal Certification** · PDPL registration walkthrough · PCI SAQ · restore drills · signing · the update service · onboarding · **commercial and legal readiness** | **…be sold to someone who is not you** | 13 items, of which the last is: someone else installs, provisions, and sells from the docs alone |

**Effort, and why it is falsifiable.** The old estimate — 40–52 weeks — priced Phase 1 at 8–12 weeks
for 87 microsteps, in the only phase whose author is learning the language, and then asserted that
groups 1.8 onward "accelerate sharply" about the hardest integration work in the product. That is
not a range, it is a wish. The counts below are reproducible from the phase files: count every
executable `### N.N.N` or suffixed microstep heading, including `.0` preconditions; count the nine
table-defined certification steps in group 5.2; and exclude the explicitly non-executable `1.1.2`
concordance heading while counting its executable `1.1.2a` and `1.1.2b` children.

| Phase | Microsteps | Weeks | Implied rate |
|---|---:|---|---|
| 1 | 112 | **14–20**, split 1A 8–11 / 1B 6–9 | 5.6–8.0 / week |
| 2 | 61 | **10–13** | 4.7–6.1 / week |
| 3 | 45 | **11–14** | 3.2–4.1 / week |
| 4 | 42 | **9–12** | 3.5–4.7 / week, plus the pilot week |
| 5 | 36 | **9–13** | 2.8–4.0 / week, and gated on other people |

Phase 1's rate is the highest and that is deliberate, not optimism: its microsteps are the finest in
the set — many are a single type, a single trigger or a single test — and its 1A/1B split makes a
slip visible at week 9 rather than at the final gate. Phases 3 and 5 carry the lowest rates because
their work waits on a host, a QSA, counsel, an acquirer and a merchant.

**Total: roughly 53–72 weeks solo**, a range and not a commitment. Record the actual per-microstep
cycle time over the first two groups and correct this table from evidence; an estimate nobody
revises against measurement is a decoration. Nothing in it prices the long-lead items in §6a — those
are queue time, not effort, and they are why several of them are ordered phases before they are used.

Phases 1–2 deliberately front-load the unforgiving parts — hardware, money, offline, fiscal —
because they dictate the architecture. Dashboards never do.

---

## 4 · What changed from Part G, and why

| Change | Reason |
|---|---|
| **Phase 1 grows** — `Money`/`Currency`/`Qty`, backup, sequences, business date, i18n, permission guards, audit chain, benchmarks, seed fixture | Every one is cheap now and expensive later. Three (G-11, G-12, G-2) are **impossible** once real sale rows exist |
| **Phase 1 grows again** (`2026-08-25`) — a shift skeleton, `stock_adjust`, the sales-side tax report, the approval modal, the department sale, key custody and a second backup destination | Each was a Phase-1 dependency the plan named and no Phase-1 microstep built. `Cart.shift_id` is not optional; the exit gate demands a tax report no phase created; the keychain-loss demonstration could not pass in a release build |
| **Phase 2's fiscal gate is replaced** — conformance harness + mock ISTD + five goldens, instead of "four sandbox documents clear" | Correction C-1: there is no sandbox. The replacement is stronger — it runs on every commit forever, where a sandbox run is a one-off |
| **Obtaining the official ISTD package moves *into* Phase 2 as microstep 2.7.0** (`2026-08-25`) | It was 5.2.1/5.2.2, on the premise that the specification could not be had before a merchant existed. ISTD publicly lists its Technical Integration Guide, so the premise is stale — and building `codes.rs`, the builder and five goldens from a reconstruction first is the expensive way round |
| **Fiscal Certification stays in Phase 5** as a nine-item credentialed milestone, and moves **in front of the pilot** | It still needs a merchant, real credentials, and a written outage ruling. What changed is that it no longer *discovers* the contract, and that three real stores may not trade fiscally before it |
| **Electronic journal promoted into Phase 2** (master-plan J.1 already flagged it) | It is a thin UI over facts already stored, and support teams live in it |
| **Shelf labels promoted to a Phase-4 compliance feature** (master-plan J.3) | Jordan's MoITS enforcement statistics are dominated by price-display failures. This is not a convenience feature |
| **Backup moves from Phase 5 to Phase 1** | Between a sale and its successful push, the only copy of that money is one SQLCipher file |
| **Operating the server becomes Phase-3 work** (`2026-08-25`) | Phase 4's gate is "three stores trade for a week with no intervention from you", which needs a host, a tested `pg_dump` restore, a migration procedure with live registers attached, and an alert that reaches a person |
| **A pre-pilot gate is inserted before Phase 4's pilot** (`2026-08-25`) | The pilot put real customers and real cards into three shops ahead of fiscal certification, the PDPL determination, the breach runbook, the SAQ determination and any independent security assessment |

---

## 4a · Errata and concordance

[`../plan/`](../plan/) is immutable and deliberately frozen as the historical source, and
`CLAUDE.md` routes a new reader there **first**. So a superseded table name in the blueprint reads
as current truth until something says otherwise. This section is that something. It has three
parts, and every other document in this set cites it rather than restating a correction.

Nothing here edits a source plan. Where a row says *superseded*, the source sentence stays as it
was written and the **Current authority** column is what the code implements.

### 4a.1 · Status of corrections C-1 to C-4

[`ref/plan-validation.md`](ref/plan-validation.md) is the audit of record and carries a dated
revision note against each of these in place. This is the short form.

| | Original correction | Status (`2026-08-25`) | Current authority |
|---|---|---|---|
| **C-1** | No sandbox; the Phase-2 gate is unbuildable | **retained**, premise narrowed | Conformance harness + mock ISTD + five goldens ([`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §6), all frozen only after microstep `2.7.0` pins the official package |
| **C-2** | Convert a line discount to a percentage and gate on an exact round-trip | **superseded — do not implement** | Largest-remainder proration to exact line allowance **amounts**, a document recap equal to their sum, and `prop_document_allowance_recap_equals_sum_of_line_allowances`. A percentage is provenance, never an eligibility gate. Precision, if a percentage must be emitted at all, is one constant `DISCOUNT_PERCENT_DECIMALS`, open at `2.7.0` |
| **C-3** | ISTD recomputes at 9 decimals and tolerates < 0.001 JOD | **superseded — the tolerance is not sourced** | Half-fil per-line comparison plus exact identities over the document's own carried values; a failure is `QueueState::BuildFailed`, not `Rejected` and not `Dead`. The regulator's real tolerance is an open item owned by `2.7.0` |
| **C-4** | 75k goods / 30k services / 10k special-tax goods | **numbers retained, categories corrected** | The 10k class is the **producer/manufacturer** of SST goods, not a reseller. Registration follows the merchant's registered activity and dated evidence, never their assortment. GST registration and JoFotara obligation are independent axes |

### 4a.2 · Concordance — superseded names, numbers and rules

A reader arriving from a source plan will meet every left-hand column as though it were current.

| Superseded | Where it appears | Current authority |
|---|---|---|
| `rate_bp` (basis points) | blueprint §3 | `rate_ppm` — parts per million. Every current Jordanian rate is exactly representable in either, and ppm is what the fiscal profile needs |
| Banker's rounding as the money default | blueprint §3 | `HalfAwayFromZero`, as **one versioned Jordan jurisdiction policy** rather than a per-store preference. Cash rounding is a separate settlement policy. The official scale and tie rule are an open item owned by `2.7.0` — see [`ref/tax-jordan.md`](ref/tax-jordan.md) §4 |
| `stock_movement` | blueprint §3 | `stock_ledger`, plus the rebuildable `stock_cache` |
| `tax_group` | blueprint §3 | `tax_category` + effective-dated `tax_rate` |
| `product_barcode` | blueprint §3 | `barcode`, which also carries `pack_qty_milli` so an outer-case code adds its pack quantity |
| `user` | blueprint §3 | `app_user` — `user` is reserved enough in enough engines to be a permanent nuisance |
| `role_perm` | blueprint §3 | `role_capability`, against the `capability` catalogue |
| A mutable `loyalty_points` column | blueprint §3 | `loyalty_ledger` — append-only, conflict-free across offline registers, balance = Σ |
| `sale_line.qty INTEGER` (unit counts) | `0001_init.sql` | `qty_milli`, migrated ×1000 by `0002` (gap G-12) |
| Migration `0002` owned by `1.1.1` | earlier implementation schedule and the retained schema heading | **Shipped and frozen; runtime ownership is `1.1.7`.** The schema keeps its old heading only as a link-safe concordance anchor |
| Migrations with `up` and `down`, both exercised in CI | blueprint §3 | **Forward-only.** A `down` that has never run against real data is fiction; recovery is a snapshot restore or a roll-forward patch. [`01-conventions.md`](01-conventions.md) §9 |
| Blueprint Phase-0 gate: "a signed installer on all 3 OSes, plus a cart and tax" | blueprint §10 | Phase 0 closed **unsigned**, with the cart and tax engine in Phase 1 and signing in Phase 5 milestone 5.5. See [`phase-0-closeout.md`](phase-0-closeout.md) |
| "Signed, reproducible, auto-updating releases from day one" | blueprint §1 | Not true and not repeated. The honest claim is *signed and verifiably traceable before external pilot*; "reproducible" is unused until two clean builds per platform produce equal payload digests. [`ref/security-compliance.md`](ref/security-compliance.md) §6b |
| Master-plan Part G: fiscal **production** cutover in Phase 3 | master plan Part G | Phase 5 milestone 5.2, and before the Phase-4 pilot. Phase 3 contacts no ISTD endpoint |
| Master-plan C.6 expected-cash formula | master plan C.6 | [`ref/domain-api.md`](ref/domain-api.md) §11. The old formula omitted change given out, omitted two of five movement kinds, and double-counted cash rounding |
| Master-plan C.10 default permission matrix | master plan C.10 | [`ref/domain-api.md`](ref/domain-api.md) §8.2 — the full grid over `cap::ALL`. C.10 is 15 prose rows bundling several capability strings each; it is not a usable fixture |
| Master-plan J.3: a defective claim "may bypass the window per policy" | master plan J.3 | Interim default pending the counsel-owned OPEN item at 2.3.2: only `ReturnReason::Defective` bypasses `window_days`, under audited `refund.outside_window`; `Damaged`, `ChangeOfMind`, and `WrongItem` remain subject to the store window. The default is not a settled statement of Jordanian law |
| Master-plan J.1: store credit is Phase 4 | master plan J.1 | The **minimum** instrument — issue, redeem, balance — is Phase 2 migration `0009`, because Phase 2's receiptless-return default already depends on it. Gift cards stay Phase 4 |
| Blueprint's "3-2-1 backups" as Phase-5 scope | blueprint §10 | Restored as microstep `1.8.6b`: a second, off-machine destination, verified, with its age on device health |
| Blueprint's WebDriver packaged-app smoke suite | blueprint §8 | Restored as microstep `2.9.5`. It was dropped without a reason, leaving the only artefact a merchant runs as the one nothing executes |
| `sale_tender` settlement columns mutating after completion | `0002_sale_integrity.sql` | `tender_status_event` — an append-only transition fact, with `tender_status_current` as a rebuildable projection. `0003` closes the shipped exception |
| Shift close as an `UPDATE` on `shift` | master plan C.6 | `shift_close_event` + `shift_state`, for the same reason: the server revokes `UPDATE` on fact tables |
| `doc_sequence PRIMARY KEY (register_id, kind)` | earlier schema drafts | `PRIMARY KEY (scope_kind, scope_id, kind)`. Receipts and Z reports stay register-scoped; fiscal ICV is store-scoped by default, and its authoritative scope is open at `2.7.0` |
| `store.utc_offset_minutes` | earlier schema drafts | `store.tz_id TEXT NOT NULL DEFAULT 'Asia/Amman'`. A stored offset is wrong twice a year in most zones and cannot be repaired retroactively |
| `ApprovalToken` | earlier IPC drafts | `ApprovalHandle` — one-use, bound to actor, approver, capability, entity, amount, reason and expiry, and consumed in the same transaction as the effect and its audit row |
| `cart_add_line { … unit_price_minor? }` | earlier IPC drafts | Removed. A caller-supplied price under `sale.create` is a price override with no reason, no floor, no ceiling and no audit row. Deliberate changes go through `cart_override_price`; a price-embedded barcode arrives as a typed scan result; a department sale is its own capped, audited command |
| `sale.receipt_printed_at` as print state | earlier schema drafts | `receipt_artifact` (immutable bytes + hash) and `print_job`/`print_attempt` (mutable operational state). A completed sale cannot be updated, so print status could never have been recorded on it |
| `012` / `022` as "payment method codes" | `ref/plan-validation.md` §3, first revision | Composite `InvoiceTypeCode@name` values — scope + settlement + taxpayer type — composed by `compose_invoice_type_name` in `codes.rs`. Digits provisional until `2.7.0` |
| Fiscal issue date `dd-mm-yyyy` | earlier fiscal drafts | `YYYY-MM-DD`. `cbc:IssueDate` is `xs:date`, whose lexical form is normative, and the golden is validated against the real XSD rather than a string pattern |
| `prop_both_databases_converge_byte_identical` | blueprint §4, Phase-3 gate | Three checkable properties: `prop_server_facts_equal_the_union_of_register_outboxes`, `prop_reference_tables_converge_across_all_three_nodes`, `prop_apply_is_idempotent_under_any_replay_order`, over the canonical dump specified in [`ref/sync-protocol.md`](ref/sync-protocol.md) §6 |
| `prop_discount_percentage_roundtrip_is_exact` | correction C-2, first revision | `prop_document_allowance_recap_equals_sum_of_line_allowances` |
| `Money::split_evenly` as the proration tool | correction C-2, first revision | `Money::split_proportional`. Equal splitting is not proportional-by-line-value allocation; `split_evenly` keeps its narrower job |
| "The four golden fiscal documents" | correction C-1, first revision | **Five** — plain, discounted, multi-rate, weighed, credit note — plus a training-absence case |
| "E.1–E.72" | master plan Part E, and this set's own earlier text | **92 numbered cases**, reconciled against the suite by `scripts/check-test-catalog.py` |
| "The tax report by rate is the accountant's filing input" | master plan C.12, `ref/tax-jordan.md` first revision | A **sales-side tax reconciliation**. The statutory return also needs purchases, imports, input-tax deductibility and apportionment, credits, adjustments and box mapping, which arrive in Phase 4 |
| "The electronic controller/processor registry is not yet activated" | `ref/plan-validation.md`, first revision | MoDEE publishes the register. Each deployment needs its own dated controller/processor/DPO determination and entry before customer PII is processed |
| "The engineering posture does not change; only the claim changes" (PCI) | `ref/plan-validation.md` §4, first revision | It does change. SAQ B-IP carries eligibility and isolation requirements; SAQ C pulls the store network, patching, access control, monitoring and policy evidence into scope |
| "Key in the OS credential store, **not on the disk**" | blueprint §6 | A credential store *is* an encrypted database on the disk. What it buys is cold-disk protection, and nothing against a process running as the cashier's own OS user. [`ref/security-compliance.md`](ref/security-compliance.md) §6a |
| "The backup is SQLCipher-encrypted with the same key" | earlier Phase-1 drafts | A wrapped data key plus a merchant-held recovery code (`1.8.5b`). The old design made the wiped-keychain demonstration impossible to pass in a release build |
| Fiscal microsteps `5.2.1` and `5.2.2` | earlier Phase-5 drafts | Microstep `2.7.0`. Phase 5 milestone 5.2 keeps the credentialed steps and renumbers nothing else, so `5.2.3`–`5.2.11` still mean what they meant |
| "`merchant-decisions.md` §11" as the home of the offline-clearance answer | `ref/fiscal-jofotara.md`, first revision | [`ref/merchant-decisions.md`](ref/merchant-decisions.md) **section F, row 6.7**. There is no §11; §K is *Payments* |
| Source-plan companion filenames | the three source plans cross-reference each other under older names | The current names are [`business-functional-master-plan.md`](../plan/business-functional-master-plan.md), [`engineering-blueprint.md`](../plan/engineering-blueprint.md) and [`phase-0-setup-guide.md`](../plan/phase-0-setup-guide.md). Where a source plan names a file that does not exist, this set's names win |
| `docs/plan/phase-0-setup-guide.md` line 87 → `../justfile` | source plan | Resolves to `docs/justfile`, which does not exist; the target is `../../justfile`. The link checker allowlists exactly this one immutable-source path and says so rather than reporting unconditional success |

### 4a.3 · Open items that the corrections did not close

Each is a greppable `⚠️ OPEN` block in the reference document that owns it, in one shape: the
question, the default until it is answered, the owning microstep, and the source that settles it.
**A default is what the code does today, not an answer.** The full set lives in the reference files;
these are the ones that can change an architecture rather than a value.

| Question | Owner | Settled by |
|---|---|---|
| The authoritative ICV namespace — register, store, income source, credential, or TIN across stores | `2.7.0` | pinned ISTD business rules, or a written Directorate ruling |
| Whether a sale completed during an outage may be handed to the customer as a pending fiscal document | `2.7.0` | an official ISTD outage procedure or a written Directorate ruling |
| The ISTD validator's real arithmetic tolerance | `2.7.0` | pinned Schematron/business rules plus accepted boundary vectors |
| Discount percentage precision, if a percentage must be emitted at all | `2.7.0` | the pinned XSD and business rules |
| Whether a deferred fiscal issue date may differ from the sale date, and which source may establish it | `2.7.0` | the pinned ISTD guide and outage procedure, or a written Directorate ruling |
| Which entity is controller, processor and DPO here, and whether the register entry is complete | `3.4.1` | MoDEE register instructions and dated Jordanian counsel advice |
| Hosting jurisdiction, sub-processors, and the cross-border transfer basis | `3.1.6` | signed hosting contracts, the PDPL assessment, and counsel |
| Which exact PCI SAQ this deployment completes | `2.1.1` collects, `5.3.3` determines | the acquirer's written responsibility matrix and a QSA |
| Statutory retention clocks per record class, and what extends them | `5.3.4` | the merchant's accountant, and counsel on the dispute hold |

**Nothing in this set may state one of these as resolved.** Claiming an unearned compliance
validation is a standing repository rule, and a fabricated tax fact is worse than a visible gap.

#### The seven that block Phase 1

Every owner above is Phase 2 or later, which read as though nothing in Phase 1 waits on an outside
answer. Seven `⚠️ OPEN` blocks say otherwise, and two of them contradict Phase 1's own exit gate.
Find them all with `grep -rn 'OPEN — blocks 1\.' docs/implementation/`.

| Question | Owner | Default today | Settled by |
|---|---|---|---|
| Which tie rule, cash-rounding step/direction and tax treatment the Jordan jurisdiction policy requires | `1.3.4` | **no `tax_computation_policy` row is approved and store provisioning/finalization stays blocked** | the official ISTD arithmetic/business-rule package, or a written clarification reviewed by the merchant's tax advisor |
| Which effective-dated categories, components and jurisdiction packs apply to the merchant's assortment | `1.3.7` | `0003` seeds no guessed rate rows; unknown categories fail closed | the official ISTD rate catalogue plus the merchant's accountant-approved classification |
| Which JSMO mark or certificate proves a trade scale is verified, when it expires, and what forces reverification | `1.2.4` | **`embedded_barcode_rule.is_active` stays `0`; no scale-derived price reaches checkout** | current JSMO metrology instructions, or written JSMO confirmation for the commissioned scale |
| Whether the bundled SQLCipher/SQLite runtime carries the upstream WAL-reset corruption fix for every source-connection and checkpoint pattern | `1.8.1`, constants at `1.8.0` | one source connection only — no concurrent checkpoint, backup, reporting or sync connection | the resolved runtime's own advisory and release evidence |
| The taxable base when a line carries both General and Special Sales Tax | `1.3.5` | the engine fails closed; no SST rule is seeded | the merchant's tax advisor on their own SST position |
| Which `ZeroRatingReason` values the filing return distinguishes | `1.3.2` | — | the return's own instructions, via the accountant |
| Which second factor exists on a Jordanian minimarket counter for high-value refunds, user administration and recovery | `1.6.2` | manager PIN + audited reason + exception report | the merchant, on what hardware and process they will actually operate |

**Two of these are in direct conflict with the Phase-1 exit gate, and the conflict is not
resolvable by writing code.** Exit demonstration 2 requires selling "a weighed item via
price-embedded barcode", which `1.2.4`'s default refuses outright; and all ten demonstrations
require finalizing a sale, which `1.3.4`'s default blocks. Phase 1 cannot be *declared* done until
those two answers arrive, however complete the implementation is. The `1.8.1` row is the one that
can still change an architecture: it decides whether `1.8.6`'s online backup may open a second
connection or must serialize with checkout.

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

**Two columns are new, and they are the ones that make a register work.** A **trigger** is the
observation that makes a risk live — without one, nobody notices until it has happened. A **review**
date is when the row is re-read whether or not anything triggered; the per-phase ritual in
[`02-development-workflow.md`](02-development-workflow.md) §16 is where it happens.

| Risk | Likelihood | Impact | Mitigation | Trigger | Owner | Review |
|---|---|---|---|---|---|---|
| **Offline clearance is ruled unacceptable** — a pending-clearance receipt is not a lawful document, so a sale completed during an outage cannot be handed over as one | unknown | **critical** | Request the official outage procedure or a written ISTD E-Invoicing Directorate ruling in Phase 1. Design the interim `clearance_required` fallback (non-fiscal payment acknowledgement now, tax invoice only through the approved path) so the official answer is a controlled change rather than a rewrite | the official procedure or Directorate ruling arrives, in either direction | `2.7.0` | every phase gate until answered |
| **ISTD spec differs from the reconstruction** | **high** | **high** — it gates certification, every conformance rule and all five goldens, and cannot be checked later without regenerating them | Everything reconstructed lives in one module (`pos-fiscal/src/codes.rs`). Microstep **2.7.0** pins the official package *before* the builder or any golden is frozen, instead of discovering the difference in Phase 5 | the `2.7.0` diff reports any provisional row that does not match | `2.7.0` | at `2.7.0`, then quarterly |
| **No sandbox ⇒ first real submission is on a merchant's tax record** | certain | high | Conformance harness + mock ISTD + five goldens; certification uses low-value invoices immediately credit-noted; **merchant consent in writing** | milestone 5.2 is scheduled | 5.2 | before 5.2 |
| **JoFotara changes its API, code lists or validation mid-build** | **high** over a 12–18 month build | high | The pinned `2.7.0` manifest records a version and hash, so a change is a diff rather than a surprise. `rejection_rate_24h` above 2%, or three consecutive rejections with the same error code, is treated as an ISTD change until proven otherwise | the rejection-rate alarm, or a new ISTD publication | `2.7.0`, then 3.9.3 | quarterly |
| **Signing certificate or entitlement key lost, expired or leaked** | expiry is **certain** and annual | **high** — a lost updater key means a site visit per register; a lost entitlement key means no merchant can renew | Two independently encrypted recovery copies per key, `kid` in every entitlement so two keys can be valid at once, an old-key-signed bridge for updater rotation, and expiry dates on a calendar. [`ref/security-compliance.md`](ref/security-compliance.md) §6a | 90 days before any expiry; immediately on any suspected exposure | 5.5.1 | quarterly |
| **A cross-tenant leak on the shared server** | medium | **critical** — it is a personal-data breach *caused by the vendor*, it hits every merchant on the instance at once, and both statutory clocks start for all of them | `org_id NOT NULL`, tenant-scoped unique keys, composite foreign keys, forced row-level security under a non-owner role, and an adversarial two-org property test rather than a review | any query that reaches the database without a principal-derived `org_id`; any new merchant-owned table without an RLS policy | 3.6.6 | every phase gate from 3 |
| **Jordan changes a rate by decree mid-build** | medium | low | Rates are time-effective data. A decree is a row, not a release | an ISTD rate-catalogue publication | 1.3 | quarterly |
| **Learning Rust slows Phase 1 badly** | medium | medium | Groups 1.1–1.3 are the steep part. The §3 estimate now prices it, and the implied microstep rate makes the slip visible in week 3 rather than week 16 | actual rate below 4 microsteps/week for two consecutive weeks | 1.x | weekly through Phase 1 |
| **No acquirer will give a pre-revenue vendor a test terminal** | medium | **high** — it blocks microstep 2.1.1, the real driver, and the hardware lab | Start the conversation in Phase 1 (§6a) and establish the legal entity first. Stated fallback: Phase 2 ships against the simulator and the real driver moves to Phase 5 with the terminal | four weeks of no answer from the second candidate acquirer | 2.1.1 | monthly from Phase 1 |
| **Acquirer terminal lacks a status query** | medium | **high** | Ask before choosing (11.3 in the questionnaire). Without it there is no safe timeout recovery — **choose a different acquirer** | any candidate's written protocol omits it | 2.1.1 | at 2.1.1 |
| **Arabic receipt looks wrong on real paper** | medium | medium | Raster pipeline + seven goldens + a committed PNG beside every raster golden + the hardware lab, where a native reader confirms on paper | any diff under the golden directory, or a `cosmic-text`/`rustybuzz`/`tiny-skia`/font bump | 1.7, 2.9.4 | every release |
| **Performance degrades at year-one volume** | medium | medium | Budgets are failing CI benchmarks from Phase 1 against named reference hardware, **and the soak dataset is generated at the end of Phase 2** rather than Phase 5 — a Phase-5 soak is a detector, not a mitigation, because an index or an archival strategy found then lands after the schema and the reports have shipped | any budget within 20% of its limit | 5.1.1, dataset at 2.9 | every phase gate |
| **PDPL enforcement arrives suddenly** | low | medium | Built to the law, not to the lag. Consent as an event ledger, export, anonymisation, scrubbed logs, both breach clocks — all tested. The electronic register is live, so registration is a dated determination rather than a wait | any MoDEE publication; the registration determination at 3.4.1 | 3.4 | quarterly |
| **Scope creep from J.1's deferred list** | **high** | medium | Every deferred item has a named hook and a merchant-decisions row. A "yes" is a phase, not a rewrite | any merchant answer flipping an L-section row to "yes" | ongoing | weekly |
| **Runway runs out before Phase 3** | medium | **critical** — Phases 1–2 produce a register nobody can be billed for; there is no revenue path until entitlement issuance exists in Phase 3 | Front-load the commercial decision: the unit of sale is decided **before** microstep 3.8.1 because it determines what an entitlement asserts. §6a orders the legal entity in Phase 1 for the same reason | remaining runway below two phases at the current rate | 3.8, 5.0 | monthly |
| **Solo-developer bus factor — documentation** | certain | high | This document set *is* the mitigation. The last item of the Phase-5 gate tests it: someone else must install, provision, and sell from the docs alone | — | 5.6 | every phase gate |
| **Solo-developer unavailability — trading** | certain over a decade | **critical** if entitlements expire on a vendor timetable: every register at every merchant degrades at roughly the same moment | The licensing decision in [`ref/security-compliance.md`](ref/security-compliance.md) §7: expiry blocks **enrollment and updates**, never a sale on a register that was entitled when it last synced, and entitlements are dated to the paid term plus a stated buffer so trading needs no online validation at all | any absence longer than the stated buffer | 3.8.1 | at 3.8.1, then annually |

---

## 6a · The long-lead register

None of these is effort, so none of them is in the §3 week count — they are queue time, and each one
can stop a phase at its first microstep. **Order by** is when the request goes out, not when the item
is used.

| Item | Order by | Needed for | If it is late |
|---|---|---|---|
| **Legal entity established** | Phase 1 | an acquirer relationship, Apple and Microsoft signing identities, a processing agreement, and ISTD credentials in the vendor's own name | everything below it slips; this is the root dependency |
| **The official ISTD package** — guide, XSD, business rules, code lists | Phase 1, so `2.7.0` is not waiting | microstep `2.7.0`, and therefore **every** fiscal microstep | group 2.7 stalls at its first step, or worse, proceeds on a reconstruction and regenerates five goldens in Phase 5 with a merchant watching |
| **Written answers from the ISTD E-Invoicing Directorate** on anything the package leaves silent | Phase 1, alongside the package | the four fiscal `⚠️ OPEN` items owned by `2.7.0` | the architecture defaults hold, and the offline-clearance answer in particular could still invalidate them |
| **Acquirer conversation opened, then a physical test terminal** | Phase 1 | `2.1.1`, the real driver `2.1.5`, and the hardware lab `2.9.4` | Phase 2 ships against the simulator and `2.1.5` moves to Phase 5 with the terminal — stated, not improvised |
| **Thermal printers, 80 mm *and* 58 mm, and one scanner** | before group 1.7 | the Arabic raster pipeline, the goldens, and the lab checklist that runs before every release | Arabic is proven by a simulator, which proves nothing about paper |
| **Merchant recruitment for the pilot and for certification** | Phase 3 | the Phase-4 pilot and Phase-5 milestone 5.2, which needs *their* portal account and *their* written consent | 5.2 has no host; the whole fiscal claim stalls |
| **Apple Developer ID and Windows Authenticode identity** | Phase 3 | `5.5.1`, and every installer a merchant runs | releases stay unsigned, and the Phase-5 gate's first item fails |
| **QSA engagement and an independent security assessment** | Phase 4, before the pre-pilot gate | `5.3.3`, and the pre-pilot gate that lets real cards into three shops | the pilot cannot start; do not start it anyway |
| **Jordanian counsel and the merchant's tax advisor, on retainer** | Phase 1 | interpretation of the official fiscal material, the PDPL determination, the retention clocks, and the defective-goods position | the non-protocol `⚠️ OPEN` items in §4a.3 that can change an architecture stay open into Phase 4; advisors do not substitute for an ISTD outage procedure or Directorate ruling |

---

## 7 · What "done" means

Nine claims. Each is a passing test, a signed checklist, or a named advisor — **never an intention**.
A claim whose named test cannot hold is worse than no claim, because it gets weakened at the keyboard
by the one person who can also edit the sentence; two of these were in that state and are restated.

1. **Sells offline.** A cut cable changes nothing — including fiscal: the sale completes and the
   document queues with `icv IS NULL`.
2. **Money is exact.** No float touches it; every total is a property test.
3. **Tax is correct.** The **sales-side reconciliation** by rate reconciles to a hand-check on a
   scripted day. It is not a completed GST return, and it is not described as one.
4. **Arabic is right.** Confirmed on paper, by a native reader, and defended by golden files that a
   human can actually review — every raster golden ships a committed PNG beside it.
5. **Cards never double-charge.** `prop_no_input_sequence_yields_two_tenders_for_one_auth`.
6. **Refunds cannot exceed what was sold.** `prop_cumulative_refunds_never_exceed_sold_qty`, in any
   order, against a `refund_line_link` ledger that is itself a guarded fact table.
7. **Registers and the server converge.** Three properties, through a chaos week:
   `prop_server_facts_equal_the_union_of_register_outboxes`,
   `prop_reference_tables_converge_across_all_three_nodes`, and
   `prop_apply_is_idempotent_under_any_replay_order`, over the canonical dump specified in
   [`ref/sync-protocol.md`](ref/sync-protocol.md) §6. The old single claim — a byte-identical dump of
   both registers and the server — could not hold: facts travel up only, so the two registers' fact
   sets are disjoint by design.
8. **Fiscal clears.** The pinned official package (`2.7.0`) plus nine credentialed certification
   items, dated and signed, **before any real store trades fiscally** — the *only* thing that makes
   "JoFotara compliant" true. Until every one is signed, the claim is *"passes our conformance
   harness against the pinned specification"*, and it is said in those words.
9. **Data is safe.** Restored on real hardware by someone who did not write the code — including the
   case where the machine **and** the credential store are both gone and only the printed recovery
   code survives — twice, and timed.

---

## 8 · Standing work after launch

- **Re-run [`ref/plan-validation.md`](ref/plan-validation.md) quarterly**, and re-diff the `2.7.0`
  manifest against the current ISTD package in the same pass. Rates move by decree, JoFotara adds
  waves and changes validation, the PDPL register is live. **A compliance claim has a shelf life.**
  Write down what was re-checked and against which source; "re-audited" with no artefact is a memory.
- **Watch the fiscal rejection rate**, and treat it as the primary detector rather than the quarterly
  audit. A quarter is a quarter of uncleared documents. The alarm is a number, not a habit:
  `rejection_rate_24h` above 2%, or three consecutive rejections carrying the same pinned ISTD error
  code, is an ISTD change until proven otherwise, and it reaches a person through microstep `3.9.3`.
- **Keep [`ref/test-catalog.md`](ref/test-catalog.md) honest.** Every new surprise becomes E.93,
  E.94 — with a test, an accepted risk, an open question with a stated default, or an out-of-scope.
  **A surprise that becomes none of those will happen again.** `scripts/check-test-catalog.py` is
  what stops the matrix drifting back into a hand-maintained table.
- **Re-read §4a when a source plan is quoted at you.** The concordance is the reason a superseded
  table name in `docs/plan/` does not become a schema.
- **The hardware lab runs before every release**, and its record is a dated file in `docs/drills/`.
  A golden file proves bytes; only paper proves a receipt; and a drill nobody wrote down did not
  happen.

---

*The blueprint says how to build. The master plan says what to build and why the law demands it. This set says what to type, in what order, and how you will know it worked.*
