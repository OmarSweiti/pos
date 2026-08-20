# Implementation documentation

The buildable plan for this POS: what to type, in what order, and how you will know it worked.

---

## Start here

1. **[`00-master-plan.md`](00-master-plan.md)** — the spine. Verdict on the master plan, the four corrections research turned up, the phase map, the risk register. **20 minutes.**
2. **[`01-conventions.md`](01-conventions.md)** — the engineering law. Nine invariants, naming, errors, testing, definition of done. **Read once, then keep it open.**
3. **[`phase-0-closeout.md`](phase-0-closeout.md)** — where the repository actually is, and the 1–2 days it takes to make it a foundation.

Then work the phase you are in, consulting `ref/` as the microsteps point you there.

---

## Layout

```
docs/implementation/
├── README.md                      ← you are here
├── 00-master-plan.md              the spine: verdict, corrections, phases, risks
├── 01-conventions.md              the engineering law
│
├── phase-0-closeout.md            finish what's started        1–2 days
├── phase-1-sellable-mvp.md        cash · tax · Arabic receipts 8–12 weeks
├── phase-2-money-grade.md         cards · refunds · fiscal     8–10 weeks
├── phase-3-connected.md           sync · back office · CRM     8–10 weeks
├── phase-4-depth.md               promos · supply · reports    8–10 weeks
├── phase-5-harden-and-launch.md   certification · launch       6–10 weeks
│
└── ref/
    ├── plan-validation.md         the audit + every source
    ├── domain-api.md              every pos-domain type & signature
    ├── schema.md                  migrations 0002→0011
    ├── tax-jordan.md              GST as an engine
    ├── fiscal-jofotara.md         the highest-risk component
    ├── ipc-contract.md            every Tauri command
    ├── sync-protocol.md           ownership, chaos, accepted risks
    ├── hardware-and-receipts.md   traits, Arabic raster, lab checklist
    ├── ui-spec.md                 screens, RTL, keyboard map
    ├── test-catalog.md            E.1–E.72 → named tests
    ├── security-compliance.md     PDPL · PCI · audit chain · secrets
    └── merchant-decisions.md      the questionnaire
```

Source documents live in [`../plan/`](../plan/): the business & functional master plan, the engineering blueprint, and the original Phase-0 setup guide.

`status-page.html` is a shareable summary of the spine — verdict, corrections, phase map, risk register, live Phase-0 status — published as an Artifact for people who need the picture without the microsteps. Edit the file and republish to the same URL; it is a view of this set, never the source of truth.

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

## The four corrections

Research found four claims in the business master plan that are wrong. One invalidates a phase gate. They appear in [`ref/plan-validation.md`](ref/plan-validation.md) with sources, and again inline at the exact microstep they change — so reading only a phase file cannot lead you to implement the wrong thing.

| | Correction |
|---|---|
| **C-1** | **JoFotara has no sandbox.** The master plan's Phase-2 exit gate is unbuildable as written |
| **C-2** | ISTD rejects **global discounts entirely** — per-line, as a percentage, not merely prorated |
| **C-3** | ISTD recomputes totals at **9 decimals**, tolerating **< 0.001 JOD** drift |
| **C-4** | GST registration thresholds are **75k goods / 30k services / 10k special-tax goods** |

---

## Conventions in one screen

If you read nothing else:

- **Money is `i64` minor units, always.** No float touches money, anywhere.
- **The minor-unit exponent is per-currency data.** JOD = 3.
- **Quantities are `i64` milli-units.** Weighed and discrete share one representation.
- **Completed sales are immutable.** Corrections are new documents.
- **Price and name are copied onto the sale line** at capture time.
- **Stock is a ledger.** On-hand is a rebuildable cache.
- **Ordering comes from server versions and UUIDv7**, never a device clock.
- **`pos-domain` is pure.** No I/O, no clock, no randomness — they are arguments.
- **Every fact write and its outbox row commit in one transaction.**

---

## Questions this set answers

| Question | Where |
|---|---|
| Is the business plan any good? | [`ref/plan-validation.md`](ref/plan-validation.md) |
| What do I do first? | [`phase-0-closeout.md`](phase-0-closeout.md) |
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

- **Re-run the validation audit quarterly.** Jordanian rates move by Cabinet decree, JoFotara is still adding waves and changing validation, and the PDPL authority is still standing up. A compliance claim has a shelf life.
- **When a merchant or a pilot surfaces something new, it becomes E.73** in the test catalog — with a test, an accepted risk, or an explicit out-of-scope. A surprise that becomes none of the three will happen again.
- **When a microstep turns out to be wrong at the keyboard, fix the microstep.** These files are the plan of record, not a historical artefact.

*Verified against the repository and current sources on 20 August 2026.*
