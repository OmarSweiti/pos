# Implementation documentation

The buildable plan for this POS: what to type, in what order, and how you will know it worked.

---

## Start here

1. **[`00-master-plan.md`](00-master-plan.md)** — the spine. Verdict on the master plan, the phase map, the risk register, the long-lead register, and **§4a, the errata and concordance**: every correction to the immutable source plans, every superseded name, and every open item that can still change an architecture. **25 minutes**, and §4a is the part you cannot skip if you have read `docs/plan/` first.
2. **[`01-conventions.md`](01-conventions.md)** — the engineering law. Nine invariants, naming, errors, testing, definition of done. **Read once, then keep it open.**
3. **[`02-development-workflow.md`](02-development-workflow.md)** — how the work gets done: every command, the feature lifecycle, manual testing, the drills, the rituals. **Keep it open next to the law.**
4. **[`03-github-workflow.md`](03-github-workflow.md)** — how the work gets tracked and shipped: the four branches, issues, the board, pull requests, the two release channels. **Read §3 before your first push** — it says which rules a machine enforces and which are only written down.
5. **[`phase-0-closeout.md`](phase-0-closeout.md)** — the dated Phase-0 close-out record. It is
   historical evidence, not a current setup or GitHub runbook; use files 2–4 above for live commands
   and repository posture.

Then work the phase you are in, consulting `ref/` as the microsteps point you there.

**Current implementation frontier (29 August 2026):** Phase 0 is closed by transfer: `0.3.2`
remains open in [`phase-0-closeout.md`](phase-0-closeout.md), with updater signing owned by
microstep `5.5.0`. Phase 1 has **16 of 112 executable microsteps fully complete (~14%)**: `1.1.0`
(the shared property harness), `1.1.1` (`Currency`), `1.1.2a` (`Money` carries `Currency`), `1.1.6`
(`RoundingRule` and the one rounding point), `1.1.3` (`Qty` in milli-units), `1.1.4` (`Percent` in
parts-per-million), `1.1.2b` (`Money` arithmetic and formatting), `1.1.7` (migration `0002`,
shipped earlier), `1.1.5` (the complete money property suite), `1.1.8` (the fifteen typed ids,
the `IdSource` port and `SeqIdSource`), `1.2.1` (migration `0003`, the STRICT rebuild and the
org / store / register / taxonomy tables), `1.2.2` (`Product`, `UnitOfMeasure` and the regulated
pair), `1.3.1` (the tax engine's value and evidence types), `1.8.9` (the outbox writer, so every
fact graph commits with its delivery envelope), `1.8.5` (the release build's key policy, proven
in a release build), and `1.11.2` (the RTL lint, whose escape hatch now requires its reason).
Group 1.1 has **no immediately buildable work remaining**:
`1.1.9`'s pure-domain time values, clock policy, and terminal IANA-zone resolution have landed, but
its database persistence half remains deferred until `1.9.1` creates `trusted_time_state`, including
`clock_state_survives_restart`. The `1.1.9` microstep is therefore **partially delivered**, not
complete. `1.6.3` is **partially delivered** on the same terms: its domain grid, limits and shape
rules are complete and enforced, but the seed comparison it also names cannot run until `1.6.1`
commits migration `0004` and creates the `role` and `role_capability` tables it would query.
Group 1.2 begins with the **partially delivered** `1.2.0` benchmark gate: `just
bench-gate`, `scripts/bench-gate.py`, its fourteen refusal-path tests, and
`benchmarks/reference-register.toml` are committed with **every identity value deliberately blank**
beside §6a.1's empty register-hardware table. `python3 scripts/bench-gate.py --check-profile`
therefore exits **non-zero**, which is conventions §7.1 working rather than failing; filling both
records is `1.2.0`'s deferred half and waits on hardware nobody has bought
([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6a: order it before group 1.7
starts). **`1.8.9` has landed, so the gate it held is open**: groups 1.6, 1.9 and 1.10 may now write
append-only facts, because every fact graph commits with its delivery envelope and the writer refuses
to return success on an incomplete one. Two microsteps now have every dependency met and none
blocks another: `1.2.4` (the scan parser's pure half, next in §1.2's build order) and `1.6.5` (the
audit hash chain); `1.6.3`'s remaining seed comparison waits on `1.6.1` alone. None of them is **blocked on the merchant legal name or TIN**,
which instead gate store provisioning and the issuing of a valid tax receipt. `1.3.2` is the next
code step on the phase's longest critical path, but its `ZeroRatingReason` vocabulary and evidence
requirements remain externally blocked by the `⚠️ OPEN` item in [`ref/domain-api.md`](ref/domain-api.md)
§5 rather than by a missing code dependency.
Repository-hardening and documentation work may land between numbered product steps, but it does
not advance that frontier.

**This pointer is maintained by station 13 of the feature lifecycle** — the documentation loop in
[`02-development-workflow.md`](02-development-workflow.md) §4.13 — and by nothing else. It is a
dated convenience, not a source of truth: the delivery board and the merged history are. If the two
disagree, the history is right and this line is a bug; fix it in the same commit that noticed.

---

## Layout

```
docs/implementation/
├── README.md                      ← you are here
├── 00-master-plan.md              the spine: verdict, corrections, phases, risks
├── 01-conventions.md              the engineering law
├── 02-development-workflow.md     how the work gets done: commands, loops, manual tests
├── 03-github-workflow.md          branches, issues, the board, PRs, release channels
│
├── phase-0-closeout.md            historical close-out record
├── phase-1-sellable-mvp.md        cash · tax · Arabic receipts 14–20 weeks
├── phase-2-money-grade.md         cards · refunds · fiscal     10–13 weeks
├── phase-3-connected.md           sync · back office · CRM     11–14 weeks
├── phase-4-depth.md               promos · supply · reports     9–12 weeks
├── phase-5-harden-and-launch.md   certification · launch        9–13 weeks
│
└── ref/
    ├── plan-validation.md         the audit of record + every source
    ├── domain-api.md              every pos-domain type & signature
    ├── schema.md                  migrations 0002→0012 — authoritative for every migration number
    ├── tax-jordan.md              GST as an engine
    ├── fiscal-jofotara.md         the highest-risk component
    ├── ipc-contract.md            every Tauri command
    ├── sync-protocol.md           ownership, chaos, accepted risks
    ├── hardware-and-receipts.md   traits, Arabic raster, lab checklist
    ├── ui-spec.md                 screens, RTL, keyboard map
    ├── test-catalog.md            92 edge cases → named tests
    ├── security-compliance.md     PDPL · PCI · tenancy · keys · audit chain
    └── merchant-decisions.md      the questionnaire
```

Source documents live in [`../plan/`](../plan/): the business & functional master plan, the engineering blueprint, and the original Phase-0 setup guide.

`status-page.html` is a locally shareable summary of the spine — verdict, corrections, phase map, risk register, and live Phase-0 status — for people who need the picture without the microsteps. No publication workflow or stable hosted URL is configured; it is a view of this set, never the source of truth.

---

## How to read a phase file

Each opens with its exit statement, a group dependency graph, and a note on which groups are independent — useful when you are stuck. Each closes with an exit gate written as runnable commands plus demonstrations.

Microsteps are numbered `<phase>.<group>.<step>` and **the numbers are stable** — they are commit-message references and checklist IDs.

```
### 1.3.4 — Tax engine: inclusive extraction
Depends on:  1.3.1, 1.1.2
Files:       crates/pos-domain/src/tax.rs  (new)

<the actual signatures / SQL / types to write>

Tests to add:  inclusive_16pct_extracts_exactly
               prop_line_tax_sum_equals_receipt_tax
Verify:        cargo nextest run -p pos-domain tax::
Done when:     Σ line tax == receipt tax, exactly, ∀ inputs
```

**A microstep is done when its `Done when` line is objectively true, checked by running its command** — not when the code looks finished. The full definition of done is [`01-conventions.md`](01-conventions.md) §6.

---

## The four corrections, and what became of them

Research found four claims in the business master plan that are wrong. A later independent audit
found that **two of the corrections were themselves wrong**, in the direction that matters: each
would have rejected valid fiscal documents. The current disposition of all four, the concordance
that maps every superseded name to its current authority, and the open items none of them closed
live in one place — **[`00-master-plan.md`](00-master-plan.md) §4a, "Errata and concordance"**.
[`ref/plan-validation.md`](ref/plan-validation.md) is the audit of record and carries a dated
revision note against each correction in place.

| | Original correction | Now |
|---|---|---|
| **C-1** | **JoFotara has no sandbox.** The master plan's Phase-2 exit gate is unbuildable as written | **Retained.** But the specification *is* obtainable, so pinning it moved from Phase 5 into microstep **2.7.0**, ahead of every fiscal build step |
| **C-2** | ISTD rejects global discounts — per-line, as a percentage, with an exact round-trip | **Superseded.** Exact line allowance **amounts** plus a document recap equal to their sum. A percentage round-trip is not a representation of the money, and gating on it dead-letters correct baskets |
| **C-3** | ISTD recomputes totals at 9 decimals, tolerating < 0.001 JOD drift | **Superseded.** The tolerance is not sourced. The local check is a half-fil per-line comparison plus exact identities over the document's own carried values |
| **C-4** | GST thresholds are 75k goods / 30k services / 10k special-tax goods | **Numbers retained, categories corrected.** The 10k class is the *producer* of SST goods. A minimarket that resells tobacco does not enter it |

Each still appears inline at the exact microstep it changes, so reading only a phase file cannot
lead you to implement the wrong thing.

---

## Conventions in one screen

If you read nothing else:

- **Money is `i64` minor units, always.** No float touches money, anywhere.
- **The minor-unit exponent is per-currency data.** JOD = 3.
- **Quantities are `i64` milli-units.** Weighed and discrete share one representation.
- **Completed sales are immutable.** Corrections are new documents. What looks like a mutation —
  tender settlement, shift close — is an append-only transition fact plus a rebuildable projection.
- **Price and name are copied onto the sale line** at capture time.
- **Stock is a ledger.** On-hand is a rebuildable cache.
- **Ordering comes from server versions and the outbox sequence**, never a device clock. UUIDv7 is
  identity and index locality; it embeds a device timestamp and cannot be the ordering authority.
- **`pos-domain` is pure.** No I/O, no clock, no randomness — they are arguments.
- **Every fact write and its outbox row commit in one transaction**, and a business transaction's
  facts travel as one commit group that the server applies whole or not at all.
- **No base sale command accepts an uncontrolled price.** Price-bearing IPC arguments exist only on
  audited `cart_override_price`, capped audited `cart_add_department_sale`, and inert,
  content-hashed `product_quick_add_prepare`; every privileged effect consumes its bound
  `ApprovalHandle` in the effect-and-audit transaction.

---

## Questions this set answers

| Question | Where |
|---|---|
| Is the business plan any good? | [`ref/plan-validation.md`](ref/plan-validation.md) |
| The source plan says X and this set says Y — which wins? | [`00-master-plan.md`](00-master-plan.md) §4a |
| What is still unanswered, and what do we do meanwhile? | [`00-master-plan.md`](00-master-plan.md) §4a.3, then the `⚠️ OPEN` blocks in `ref/` |
| What do I do first? | [`02-development-workflow.md`](02-development-workflow.md) §1 |
| What happened during Phase 0? | [`phase-0-closeout.md`](phase-0-closeout.md) — historical record |
| Which command do I type, and how do I test it by hand? | [`02-development-workflow.md`](02-development-workflow.md) |
| Which branch do I work from, and how does a change reach a merchant? | [`03-github-workflow.md`](03-github-workflow.md) |
| Is this rule actually enforced, or only written down? | [`03-github-workflow.md`](03-github-workflow.md) §3 |
| Should we use Jira, and is it free? | [`03-github-workflow.md`](03-github-workflow.md) §9 |
| What does this function look like? | [`ref/domain-api.md`](ref/domain-api.md) |
| What columns does this table have? | [`ref/schema.md`](ref/schema.md) |
| How does Jordanian GST actually work? | [`ref/tax-jordan.md`](ref/tax-jordan.md) |
| How do I do e-invoicing without a sandbox? | [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §6 |
| How do I print Arabic on a thermal printer? | [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §2.1 |
| Which test covers edge case N? | [`ref/test-catalog.md`](ref/test-catalog.md) |
| What do I need to ask the merchant? | [`ref/merchant-decisions.md`](ref/merchant-decisions.md) |
| Am I allowed to say "PCI compliant"? | [`ref/security-compliance.md`](ref/security-compliance.md) §3 — probably not yet |

---

## Keeping this set alive

- **Re-run the validation audit quarterly**, and re-diff the pinned ISTD manifest in the same pass. Jordanian rates move by Cabinet decree, JoFotara is still adding waves and changing validation, and the PDPL register is now live. A compliance claim has a shelf life.
- **When a merchant or a pilot surfaces something new, it becomes E.93** in the test catalog — with a test, an accepted risk, an open question with a stated default, or an explicit out-of-scope. A surprise that becomes none of those will happen again.
- **When a correction turns out to be wrong, correct it in [`00-master-plan.md`](00-master-plan.md) §4a and mark it in place in `ref/plan-validation.md`.** Two of the original four were wrong; leaving an audit reading as still-true is how a fix becomes a defect.
- **When a microstep turns out to be wrong at the keyboard, fix the microstep.** These files are the plan of record, not a historical artefact.

*Verified against the repository and current sources on 20 August 2026; remediated against an independent seven-lens audit on 25 August 2026.*
