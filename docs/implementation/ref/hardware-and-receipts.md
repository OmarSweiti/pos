# Hardware and receipts

Blueprint §5. Capability traits in `pos-hardware`; drivers behind them. **The UI never talks to a device** — it invokes a Tauri command and Rust does the work.

Three things this document exists to get right: **Arabic receipts**, **a printer that fails after the money is taken**, and **a cash drawer that opens exactly as many times as somebody authorised**.

---

## 1 · The traits

```rust
// crates/pos-hardware/src/lib.rs
pub trait ReceiptPrinter: Send + Sync {
    /// Moves one artifact's bytes. Those bytes are a DOCUMENT: they carry the
    /// cut and never a drawer pulse (§4).
    fn print(&self, doc: &RenderedReceipt) -> PrintOutcome;

    /// A separate call, made once per authorised opening by the checkout or
    /// no-sale flow — never by the print worker, and never by a retry (§4).
    fn open_drawer(&self) -> Result<(), HwError>;

    fn status(&self) -> PrinterStatus;                  // paper-out, cover-open, offline

    /// The profile this driver was qualified against (§6a). A driver that
    /// cannot name a qualified profile cannot be bound to a register.
    fn profile(&self) -> PrinterProfileId;
}

/// One `receipt_artifact` row, ready to move. The bytes are the persisted
/// document; the hash is re-checked before the write, so a corrupted queue
/// row fails loudly instead of printing garbage at a customer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedReceipt {
    pub artifact_id: Uuid,
    pub content_hash: [u8; 32],
    pub profile: PrinterProfileId,
    pub bytes: Vec<u8>,
}

/// A profile's stable name (§6a), not a UUID: it is what a support call and a
/// driver-selection setting both say out loud.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrinterProfileId(pub String);

/// Four outcomes, because "it failed" is three different physical facts and
/// only one of them is safe to retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintOutcome {
    /// The device accepted the whole stream and reported no fault.
    Printed,
    /// Nothing was sent: a pre-flight status read said paper-out, cover-open
    /// or offline. Safe to retry, because no paper can exist.
    Failed  { error: HwError },
    /// Some bytes were acknowledged and then the device or the link failed.
    /// Paper probably exists and is probably incomplete.
    Partial { acknowledged: usize, error: HwError },
    /// The write timed out, or the transport gave no answer at all. Whether
    /// paper exists is unknown and cannot be discovered from here.
    Unknown { error: HwError },
}

pub trait BarcodeSource: Send + Sync {                  // [1.11.6]
    fn subscribe(&self, sink: Box<dyn Fn(ScanEvent) + Send>) -> Result<Subscription, HwError>;
    fn mode(&self) -> ScanMode;                         // KeyboardWedge | Serial | HidPos
}

pub trait PaymentTerminal: Send + Sync { /* see phase-2-money-grade.md §2.1.2 */ }

pub trait LabelPrinter: Send + Sync {                   // [4.6.1]
    fn print_labels(&self, labels: &[RenderedLabel]) -> Result<(), HwError>;
}

pub trait CustomerDisplay: Send + Sync { /* Phase 4+, when a merchant has one */ }
pub trait Scale: Send + Sync { /* only if grocery/deli — §7 */ }
```

**`print` no longer returns `Result<(), HwError>`.** A stream write that returns `Err` at byte *N* has an unknown physical outcome: the paper may already be in the customer's hand. Collapsing that into `Err` is what let the plan describe a byte-exact automatic retry, and a byte-exact automatic retry of an ambiguous outcome prints a second unwatermarked original. The three failure variants exist so the queue can retry the one case where retrying is provably free (§3).

**No `pos-domain` type appears in this crate.** `pos-hardware` depends on `thiserror` today and gains only `uuid`, because a queue row has to be identified. A device layer that knows about `Money` or `SaleId` becomes a place domain rules get re-implemented against a driver's convenience. It also carries **no clock**: the print worker owns the clock and stamps `started_at`, `sent_at` and `finished_at` onto the `print_attempt` row it appends. A driver that reported its own times would be a driver whose times a test cannot control.

`SimulatedPrinter` implements every trait, **with fault injection at each of the four outcomes and at an arbitrary byte offset**. CI and demos run hardware-free. That is not a testing convenience — it is why a new developer is productive on day one without a printer on the desk, and why every fault path has a test rather than a hope.

---

## 2 · Receipt printers

**Raw ESC/POS**, over TCP port 9100 for network printers, `serialport` for RS-232, `rusb`/`hidapi` for USB. **Never webview printing** — spooler latency and per-OS quirks make it slow and non-deterministic; raw bytes are identical everywhere.

### 2.1 The Arabic problem, and its answer

Thermal printers in text mode expect a codepage (Windows-1256 for Arabic). Codepage text mode does two things wrong that cannot be worked around:

1. **It does not shape.** Arabic letters change form by position — initial, medial, final, isolated. A codepage emits isolated forms, producing text a Jordanian cashier will tell you is not Arabic.
2. **It does not reorder.** RTL runs print left-to-right, so the words come out backwards.

**Therefore: render the receipt as a raster image.** This is the field consensus and the only approach that also handles bilingual Arabic/English mixing correctly.

```
ReceiptModel                          (pure, from pos-domain)
   ↓ layout engine                    boxes, columns, RTL runs, wrapping
   ↓ cosmic-text  (rustybuzz under)   shaping + bidi + line breaking
   ↓ tiny-skia                        1-bit bitmap at the printer's dot width
   ↓ ESC/POS GS v 0                   raster transfer
```

Widths: **576 px at 80 mm**, **384 px at 58 mm** (E.49). Two profiles, two sets of golden files.

Keep a plain-codepage fallback for exotic printers that reject raster, and make it a store setting — but the default is raster and the Arabic goldens are raster. A device that can only take the fallback is recorded in the matrix (§6a) as *not qualified for Arabic*, because a codepage receipt is not an Arabic receipt and calling it one is the failure this section exists to prevent.

**The font** ([1.7.2](../phase-1-sellable-mvp.md)) is embedded in the app, covers Arabic and Latin, and is **the same file the UI uses**. The receipt then looks like the screen, which is what a merchant expects and what makes discrepancies obvious.

The layout and raster entry point is a **fuzz target** ([`test-catalog.md`](test-catalog.md)): product names are merchant data, and a name carrying bidi overrides, combining marks or ten thousand characters must not panic a register or overrun the profile's width.

### 2.2 Golden files

Seven fixtures, byte-diffed in CI:

| Golden | Proves |
|---|---|
| `receipt_ar_80mm.bin` | Arabic shaping, RTL order, column alignment |
| `receipt_ar_58mm.bin` | the narrow profile reflows rather than truncates |
| `receipt_bilingual_80mm.bin` | mixed Arabic/English runs in one line |
| `receipt_multirate_80mm.bin` | the tax summary renders exempt and standard as distinct rows |
| `receipt_duplicate_80mm.bin` | the DUPLICATE watermark |
| `receipt_training_80mm.bin` | the TRAINING watermark |
| `receipt_b2b_80mm.bin` | the buyer block — name and TIN on the document the buyer files |

**Every `.bin` ships a committed `.png` beside it, and both are diffed.** A 1-bit raster is a change-detector, not a review: a hexdump cannot show that a letter lost its medial form, so "look at the diff before committing" was an instruction nobody could follow. GitHub renders the image diff, and **the image diff is the review**. The `.png` is produced by the same rasteriser from the same `ReceiptModel`, so the two cannot drift apart into a green tick over a wrong receipt.

**Regenerating a golden is a deliberate act with a procedure:**

1. `UPDATE_GOLDEN=1 cargo test -p pos-hardware` rewrites the `.bin` and its `.png` together. Regenerating one without the other fails the check.
2. Read the image diff in the pull request. If the change touches shaping, font metrics, or the font file itself, print it as well — paper is the only place some defects appear.
3. **Any regenerated golden whose content is Arabic or bilingual carries the native reader's confirmation in that same pull request**, recorded in the drill record (§9), not deferred to the next release. The named reader for the release checklist is the named reader here; the goldens are the only continuous defence Arabic has, and a defence that is checked twice a year is not continuous.
4. A `cosmic-text`, `rustybuzz`, `tiny-skia` or font-file bump is its own pull request under the same rule. Those four produce a byte diff indistinguishable from a shaping regression, and `UPDATE_GOLDEN=1` is otherwise the only available response to a red test.

**A golden file updated to make a test pass is a test deleted.** A golden file proves bytes; **only paper proves a receipt.**

Tests: `golden_receipts_are_byte_stable` · `every_binary_receipt_golden_has_a_png_projection` · `each_golden_png_is_the_rasterisation_of_its_bin` — the second proves the pair exists, the third proves the pair still describes the same document, and only the third catches a `.png` left behind by a regeneration that touched the `.bin`.

### 2.3 Receipt anatomy

Per master plan B.6 and C.11, in order:

```
logo (raster)
merchant legal name · address · TIN                ← legally required
doc type: SALE / REFUND / ACKNOWLEDGEMENT
          [+ DUPLICATE / TRAINING watermark]
buyer name · buyer TIN                             ← when captured (B2B)
receipt no. · register · cashier · date & time
─────────────────────────────────────────────
lines: name · qty × unit · line total
       └ discount attributions beneath the line
─────────────────────────────────────────────
subtotal
discounts
tax summary BY RATE: net / tax / gross per rate
cash-rounding adjustment                            ← only when non-zero
TOTAL
─────────────────────────────────────────────
tenders + change
loyalty balance (if a customer is attached)
─────────────────────────────────────────────
JoFotara QR + UUID                                  ← once cleared
footer: return policy · thank-you (ar/en)
```

Four rules the renderer may not negotiate:

- **Every money field renders through `Money::format_exact`** ([`domain-api.md`](domain-api.md) §1.2) — at the currency's own exponent, with the store's `money_decimals` nowhere in the path. `money_decimals` is a shelf-display setting. A receipt whose visible rows do not add up to its own printed total is not proof of anything, and a three-fil rounding line rendered as `0.00` is money the document hides.
- **A department line prints the department's name**, never "unknown item" ([`domain-api.md`](domain-api.md) §6.5). A customer's proof of purchase has to describe what they bought.
- **The buyer block prints whenever `sale.buyer_tin` was captured.** The capture path, the two columns and the fiscal conformance rule all existed while the printed document had nowhere to put them, so the one customer who explicitly asked for something got a receipt they could not file.
- **`ACKNOWLEDGEMENT` needs its own `DocKind`, not a watermark.** While the offline-clearance ruling is outstanding, the interim default for a sale made without clearance is a clearly marked **non-fiscal payment acknowledgement**, not a tax invoice ([`fiscal-jofotara.md`](fiscal-jofotara.md) §2.1). A watermark would leave the underlying document calling itself a tax invoice, so this variant changes the header and drops the QR line. It does not claim the acknowledgement is the legally permitted outage artifact; the `2.7.0` open item decides that. `ReceiptModel.doc_kind` therefore gains the variant ([`domain-api.md`](domain-api.md) §13).

The exact legal minimum belongs on the tax advisor's checklist ([`merchant-decisions.md`](merchant-decisions.md) §G). Everything above is the defensible baseline.

### 2.4 Templates as data

Header and footer text, logo, and toggles are editable in the back office and **versioned**. The renderer lives at `pos-hardware`'s printer boundary and is fed a `ReceiptModel` built in `pos-domain` — so the printed receipt, the emailed PDF, and the fiscal document all derive from one source and cannot disagree.

**The template version is snapshotted onto the artifact**, not looked up at reprint time (`receipt_artifact.template_version`, [`schema.md`](schema.md)). A merchant who changes their footer in March must not thereby change what January's receipt says, for exactly the reason a refund reads the sale line rather than today's catalogue (I-5).

---

## 3 · The print-failure rules

These are the rules that decide whether a paper jam costs a receipt, a sale, or a drawer of cash.

1. **Poll status before finalize, at Pay.** Paper-out warns *before* the money is taken, not after (master plan C.15). A warning after payment is a warning that helps nobody.

2. **A print failure after finalize never un-finalizes the sale.** The money moved. The sale stands. An incident is logged, an alarm is raised, and a reprint is offered — marked DUPLICATE (E.46).

3. **The document is persisted before it is printed, and print state never touches the sale.** Finalize writes the `receipt_artifact` and its `queued` `print_job` in the **same transaction** as the sale; the schema refuses a completed sale that has neither. There is no `sale.receipt_printed_at`: a completed sale is immutable (I-4) and the trigger refuses the update the old design needed, so "print succeeded" was unrecordable and every successful print stayed on the unprinted worklist forever. Print state lives on the mutable, register-local `print_job`; the bytes live on the immutable, synced `receipt_artifact`.

4. **The retry queue retries `Failed` and nothing else.** `Failed` means the device refused before accepting a byte, so no paper can exist and a retry is free. `Partial` and `Unknown` mean paper may exist, and a byte-exact retry of those would print a second **original** — a second document, with the same receipt number, that nothing marks as a copy. The worker's index deliberately does not select them.

5. **An `Unknown` or `Partial` job is resolved by a person, and only into a DUPLICATE.** The terminal asks one question — *did a readable receipt come out?* — and the answer is the only thing that closes the job:
   - *Yes* → the job is closed with a `cancelled` attempt. No new artifact. The paper in the customer's hand is the document.
   - *No* → a **new** `receipt_artifact` is created with `artifact_kind = 'duplicate'` and `source_artifact_id` pointing at the original, carrying the DUPLICATE watermark, with its own `print_job`. The original job is closed too.

   `cancelled` reads oddly for the first branch and is the right state: it means *no further attempt will be made*, which is exactly what the answer established. The schema deliberately offers no path from `unknown` back to `printed`, because nothing inside the machine can assert that paper exists — only the person looking at it can, and their assertion is the audit entry, not a state change.

   Closing an unknown job is an operator action with money-safety consequences, so it needs its own audited command in [`ipc-contract.md`](ipc-contract.md) §3 — `print_resolve_unknown` — and never a side effect of navigating away from a screen.

6. **A print retry never emits a drawer pulse, because no persisted artifact contains one.** See §4; this is the rule the rest of §4 exists to keep.

7. **A reprint is byte-exact, or it is not a reprint.** The stored bytes are re-sent, including the fiscal QR that was on them. Nothing is re-rendered, so a template change, a font bump or a catalogue edit cannot alter a historical document. A register whose `PrinterProfileId` differs from the artifact's cannot reproduce it and says so rather than printing the wrong width — see §6a, where mixed printer widths inside one store are a procurement decision with this exact consequence.

8. **Clearance arriving after the paper produces a new artifact, never a changed one.** When a queued document clears after its acknowledgement was printed, the QR belongs to a `fiscal_supplement` artifact linked to the original. It prints on request — nobody is standing at the counter an hour later — and the original artifact's bytes are never touched.

**What a `reprint_bundle` contains.** [`sync-protocol.md`](sync-protocol.md) §3 owns the direction it travels and who may ask for it; this file owns what is in it, because "reprint from facts plus the stored QR" was three documents describing something nobody had enumerated — and a QR by itself is not a receipt.

| Field | Source | Why it is in the bundle |
|---|---|---|
| `artifact_id`, `content_hash` | `receipt_artifact` | the requester verifies the bytes before printing them |
| `content_bytes` | `receipt_artifact` | the document. The requester renders nothing and recomputes nothing |
| `artifact_kind`, `source_artifact_id` | `receipt_artifact` | an original, a duplicate, or a fiscal supplement — a reprint of a duplicate is not a reprint of the original |
| `format`, `printer_profile`, `template_version`, `fiscal_version` | `receipt_artifact` | what tells the requester whether its own device can reproduce these bytes (rule 7) |
| `sale_id`, `receipt_no`, `register_id`, `business_date` | `sale` | what a cashier types to find it, and what the audit entry names |
| the fiscal result, where one exists | `fiscal_result` | UUID, ICV and QR payload. The clearance result syncing down was never sufficient on its own, and this is where it becomes sufficient |
| **every artifact in the sale's chain**, not just the newest | `receipt_artifact` | a sale cleared after its acknowledgement printed has two documents. Serving one hands the customer the wrong one |

It contains nothing else — no cart, no tender detail, no customer record. The rendered bytes may themselves carry personal data, a loyalty balance and a buyer's TIN, which is exactly why the fetch is permission-gated, never written to the local database and never cached: replicating every sale to every register would put an unbounded copy of the PII estate on every machine in the store for the convenience of an occasional reprint.

**A failure is injected at every byte boundary, not at a convenient one.** The simulator fails after *n* bytes for every *n* across a golden's stream, and the sweep asserts that each outcome is one of the four, that the drawer count never moves, and that no second `artifact_kind = 'original'` is ever created.

**Tests:** `paper_out_warns_before_tender_not_after` · `print_failure_after_finalize_leaves_sale_complete` · `reprint_is_byte_identical_including_qr` · `queue_survives_restart` · `an_unknown_print_outcome_never_auto_retries_the_drawer_pulse` · `receipt_retry_never_repeats_the_drawer_pulse` · `simulator_fails_midway_when_scripted` · `duplicate_artifact_links_the_original_and_adds_the_duplicate_watermark`.

---

## 3a · When there is no printer

The edge-case catalogue handled paper running out mid-receipt and never handled the printer being absent: unplugged at shift open, dead at 09:00 on a Saturday, or simply never bought. That is the more common failure, and a merchant who cannot trade because of it loses a day's takings to a cable.

**The behaviour is that selling never stops.**

| Moment | What happens |
|---|---|
| Shift open with no printer reachable | the shift opens; device health shows *printer unavailable*; the status strip carries it ([`ui-spec.md`](ui-spec.md) §3) |
| At Pay | the paper warning is replaced by *"no printer — the receipt will be kept and printed later"*. It is a statement, not a confirmation dialogue |
| Finalize | unchanged. The artifact and its `print_job` are written in the sale's transaction exactly as always; the job simply never reaches a device |
| The printer returns | queued jobs drain **unchanged** — the same bytes, the same numbers, no re-render |
| A customer wants something now | the artifact is shown on screen as the bitmap the rasteriser produced — the §2.2 projection applied to a live document instead of a golden. From Phase 3 it can also be emailed, where the merchant enabled it and the customer consented (E.48). Both are the same document; neither re-renders it |

Two things this does **not** do. It does not offer to skip the receipt: the document exists whether or not paper does, because it is the sale's evidence and the reprint bundle's content. And it does not block on the queue's depth — an undrained print queue raises device health and never refuses a sale, on the same reasoning as the sync outbox ([`sync-protocol.md`](sync-protocol.md) §5).

The fiscal question sits on top of this and is not settled: a cleared QR is supposed to appear on the document handed to the customer, and with no printer there is no document to hand over. Case 85 in [`test-catalog.md`](test-catalog.md) carries the open item, its default — the one tabled above — and the microstep that owns the answer. The behaviour here is written against that default and changes with it.

**Tests:** `a_sale_completes_with_no_printer_and_queues_the_artifact` · `the_missing_printer_is_an_alarm_not_a_modal` · `a_queued_artifact_prints_unchanged_once_a_printer_returns`.

---

## 4 · Cash drawer

Almost always kicked via the printer's drawer port (`ESC p m t1 t2`). That is the default path and needs no separate driver — but it is issued by `open_drawer()`, as its own call, and **never embedded in a document's bytes**.

**Why the pulse left the byte stream.** An earlier version of this section said the cutter and the drawer kick were part of the same stream as the receipt, while the shipped trait already separated them. The stream is persisted and retried; the pulse is a physical, non-idempotent, cash-access effect. Retrying a stream that contains one opens the drawer again, with no cashier action, no audit entry and nobody watching — and it does so precisely when something has already gone wrong. The cut stays in the stream, because cutting twice wastes a few millimetres of paper and cutting is what makes the document a document. The pulse does not.

**The opening is recorded before it happens, and happens once.**

1. The `drawer_event` row — actor, approver, cause, register, shift, and the sale when there is one — is committed **in the sale's own transaction**, and the schema refuses to complete a sale whose tenders open the drawer without it.
2. The pulse is fired **after** that commit.
3. If the pulse fails, the record still stands and the drawer is opened by hand or by the manual path. The system never re-fires it on its own, because the record cannot tell it whether the drawer already opened.
4. There is exactly **one** software drawer command per sale — the schema's unique index says so. Anything after it, for the same sale, is a **no-sale open** with its own reason, its own audit entry and its own authorisation.

`drawer_event` and its triggers arrive with the shifts-and-cash migration in Phase 2 ([`schema.md`](schema.md)). Until then a Phase-1 opening is an `audit_log` entry and nothing more: the actor and the cause are recorded, the one-per-sale guarantee and the no-sale count are not. Rules 2 and 3 hold from the first cash sale regardless, because they are about when the pulse is fired rather than about where it is written down.

**The manual path is privileged, and it is bound.** `drawer_open_no_sale` requires `drawer.open`, writes an audit entry, and escalates: it takes an `ApprovalHandle` bound to this actor, this reason and this shift, consumed in the same transaction as the drawer event it authorises ([`ipc-contract.md`](ipc-contract.md) §3, [`security-compliance.md`](security-compliance.md) §5). A capability-only approval would let one manager PIN, typed once at 08:00 and watched over a shoulder, open the drawer all day with every opening attributed to the manager. The diagnostics screen calls this same command; it has no drawer-specific bypass, because changing the screen label does not change the cash-access effect.

**What the log can see, and what it cannot.** The default interface is a one-way kick. It reports nothing back, so `drawer_event` records **software-commanded** openings and only those. `source_kind` names that limit in the data rather than leaving it to a footnote:

| `source_kind` | Produced by | Guaranteed? |
|---|---|---|
| `software_command` | this product asking the drawer to open | yes — it is the act of asking |
| `sensor_observation` | a device that reports drawer state, where one exists | only on hardware that has the sensor |

An observation with no matching command inside a short window is an **unexplained opening** and is reported as one. A manual key, a wedged drawer, or a cash sale never rung up remains invisible to any software, and [`security-compliance.md`](security-compliance.md) §9 states that residual rather than letting the anti-theft claim outrun the mechanism. Spikes in no-sale opens are still the classic theft tell (E.35), and the count still appears on X and Z reports.

A drawer jammed or left open at shift close does not block the close; the state is logged and an alert raised (E.50).

**Tests:** `no_sale_open_is_logged_and_counted` · `receipt_retry_never_repeats_the_drawer_pulse` · `every_privileged_command_binds_its_approval` · `no_sale_past_the_threshold_requires_a_manager_reason` · `an_observed_drawer_transition_without_a_command_is_reported` · `jammed_drawer_does_not_block_shift_close`.

---

## 5 · Barcode scanners

**Keyboard wedge is the default** — the scanner is a keyboard that types very fast and presses Enter.

The UI keeps a hidden input capturing keystrokes anywhere on the sale screen and distinguishes a scan from typing by **inter-key timing**: a burst with < 30 ms between characters, terminated by Enter, is a scan.

> **The detail that breaks most implementations: scans must route correctly even when focus is in the search box.** A cashier types two letters, then scans; the scan must become a line, not extra text in the search field. Test `scan_routes_while_search_focused` exists for exactly this.

A scan resolves to a `ScanLookup` ([`domain-api.md`](domain-api.md) §4.3) and never to a bare price. The **only** price a scanned label may carry is a `PriceSource` derived from a matched `embedded_barcode_rule`, and that type has no constructor taking a number the caller chose — which is what keeps the scan path from quietly becoming the price-entry path. A corrupt code is rejected, never guessed (E.40), and `parse_scan` is a fuzz target because a scanner emits whatever it read off a damaged label.

Serial and HID-POS modes come later behind `BarcodeSource`, for merchants whose environment makes wedge mode unreliable.

---

## 6 · Payment terminals

Full treatment in [`phase-2-money-grade.md`](../phase-2-money-grade.md) §2.1 and [`security-compliance.md`](security-compliance.md) §3. The hardware-layer summary:

- **Semi-integrated only.** Amount and reference go to the terminal; result and reference come back. **PAN, track data and CVV never exist in this process, this database, or these logs.** A driver that returns a full PAN is an integration to reject, not data to discard.
- **Timeout is `Unknown`, never `Declined`.** Status-query before any retry. Always.
- **Store `psp_ref` on every card tender** — reconciliation and refunds both depend on it.
- **Refunds go through the PSP against the original reference**, never as a fresh charge in the opposite direction.
- CliQ / wallet QR (J.1) is a `PaymentTerminal` implementation, not a new integration class: it reaches merchants through bank POS devices. Its one distinctive failure mode is a **lost callback** — poll by payment reference before declaring unpaid; the tender stays `Pending`, never silently dropped (E.65).

---

## 6a · The supported-device matrix

"ESC/POS" is a family, not a standard. Two printers that both claim it disagree about the raster command, the status protocol, the cut sequence, the drawer pulse timings and the width in dots. The plan named no device, named no OS version, and still wrote performance budgets against "the slowest supported hardware" — a phrase with no referent, which means a budget nobody can fail.

**A profile is a record, and a driver may only be bound to a register through one.**

| Field | Why it is here |
|---|---|
| `profile_id`, `maker`, `model` | what a support call is about |
| `firmware_qualified` | a range, because behaviour changes under a firmware update |
| `transport` | TCP 9100 · serial · USB |
| `dot_width`, `paper_mm` | 576 px / 80 mm, 384 px / 58 mm — the layout input |
| `raster_command` | `GS v 0` and its alternates are not interchangeable |
| `cut_command`, `drawer_pulse` | pin, on-time and off-time; wrong timings do not open the drawer |
| `status_protocol` | real-time status is what makes rule 1 of §3 possible; a device without it cannot warn before the money is taken |
| `supports_raster_arabic` | `false` means the device is **not qualified for Arabic** and may not be sold to an Arabic-first store |
| `qualified_at`, `qualified_by`, `qualifying_commit` | when this was last true, and against what |

**The matrix itself.** One row per device the vendor supports, per class:

| Class | Rows the matrix must carry |
|---|---|
| Receipt printer | at least one 80 mm and one 58 mm device, each with a profile and a qualification date |
| Barcode scanner | wedge devices confirmed against the burst heuristic and the Arabic keyboard layout |
| Payment terminal | model, firmware, acquirer, and its PCI listing reference or the written confirmation that it has none |
| Register hardware | the reference register — the **lowest** row is what "the slowest supported hardware" means in every performance budget in this plan |
| Register OS | the exact Windows, macOS and Linux versions CI builds for and the vendor supports |

**The matrix ships empty except the simulator, and that is the honest state.** Filling it with model numbers nobody has held would be the same defect as a compliance claim nobody earned. Until a class has a row, this product supports no device in that class and says so.

**The first printer row is written in Phase 1, not Phase 2.** Group 1.7's own acceptance is that Arabic letter joining is *"verified by eye once and by golden file forever"*, and an eye needs paper. So the 80 mm device, its profile and its qualification date exist before the first golden is frozen; the remaining classes and the full checklist arrive at microstep 2.9.4. **Order the hardware before group 1.7 starts** — a printer, a 58 mm printer and a scanner are a lead time, not a task, and the terminal in §6 has a longer one still.

**The matrix is what the vendor supports; what a store owns is a different list.** The store's own devices are recorded on the questionnaire ([`merchant-decisions.md`](merchant-decisions.md) §G) — maker, model, firmware, and for a scale its serial and verification (§7). A store running a device with no matrix row is an unsupported deployment, and that is a sentence somebody has to be able to say before a pilot rather than after a support call.

**Qualifying a new device** — the whole procedure, because "it worked on my desk" is how a fleet acquires a device nobody can support:

1. Obtain the device and record maker, model and firmware version.
2. Write or select its profile. A device that needs a code change rather than a profile row is a driver, and a driver is a microstep.
3. Run the §9 lab checklist **against that device**, not against the class. Record the result per device.
4. If the width is new, commit its golden pair (`.bin` + `.png`) and get the native-reader confirmation in the same pull request (§2.2).
5. Add the row, with the qualifying commit. A profile with no qualifying commit is a claim.
6. Record what it cost and where it was bought. §9's checks 2 and 11 each need their own device on the desk, and a device nobody ordered is a release that slips.

**What protects a device whose behaviour changed between releases.** Nothing in this repository executes against hardware, so the honest controls are narrow and stated:

- The profile records the firmware range it was qualified for. A device reporting a firmware outside that range raises a **device-health warning and never refuses to print** — a printer that stops working because it updated itself is worse than one that prints with an unverified profile.
- The lab checklist runs before every release **against the matrix's own devices**, and its record is per device, per firmware. One tick for "printers" hides the printer that changed.
- A firmware change discovered to break a profile is a new profile row and a new qualification, never an edit to the old one — the old row is what a merchant still running that firmware is using.

---

## 7 · Later devices

Slot in behind the same trait pattern when a merchant actually has one:

| Device | When | Note |
|---|---|---|
| Customer display / second screen | Phase 4+ | running cart, total, wallet-pay QR |
| Scale (serial) | only if grocery/deli | until then, price-embedded barcodes cover most cases — but the **scale is still a trade instrument** |
| Label printer | Phase 4 | **a compliance feature** in Jordan — see [`../phase-4-depth.md`](../phase-4-depth.md) §4.6 |
| Kitchen printer | hospitality epic | a different product; the trait pattern still holds |

**A price-embedded barcode does not remove the scale from the transaction.** The label was produced by an instrument used for trade, and its verification status is a fact about the money, not about the peripheral: `trade_scale` and its append-only `trade_scale_verification` events live in [`schema.md`](schema.md), embedded pricing fails closed against an expired or revoked verification, and the evidence and cadence are an open item that reference already carries. Recording the scale's maker, model and serial is therefore part of commissioning an `embedded_barcode_rule`, not paperwork that can follow later.

---

## 8 · Diagnostics screen

Where support teams live. Every trait gets a test action, and every metric a support person needs to reach a diagnosis by phone:

- test print (all seven goldens, on paper)
- drawer action — invokes `drawer_open_no_sale`, including its reason, conditional approval, audit row and no-sale count
- scanner echo — shows raw scan input and the parsed `ScanLookup`
- payment terminal ping and last-transaction query
- printer status, paper level, and the bound profile with its qualified firmware range (§6a)
- print queue: depth, oldest job, and **jobs awaiting an operator answer** (§3 rule 5)
- fiscal queue depth, oldest uncleared age, last error
- database health and size
- backup age **for each destination**, local and off-machine, and the date the recovery code was issued — never the code itself
- clock confidence and skew against the last trusted server time ([`domain-api.md`](domain-api.md) §3.2)
- disk space
- audit-chain verification, and the last anchored checkpoint

**Done when** a support person can be walked through the whole screen by phone and reach a diagnosis without remote access.

---

## 9 · The hardware-lab checklist

**Before every release**, against the devices in the matrix (§6a) — which is at least two thermal printers (80 mm and 58 mm), one real scanner, and one real payment terminal.

| # | Check |
|---|---|
| 1 | All seven receipt goldens print on paper; Arabic read and confirmed **by a native reader** |
| 2 | 58 mm profile prints on a 58 mm printer without truncation |
| 3 | Drawer kicks once on cash completion, once on a no-sale open, and both are logged with actor and cause |
| 4 | **The drawer does not kick on a reprint, on a retry, or on a queue drain after restart** |
| 5 | Paper removed mid-print → sale stands, the job lands in `unknown`, nothing retries by itself, and the operator answer produces a DUPLICATE with the identical QR |
| 6 | Printer unplugged at shift open → the shift opens, the sale completes, the artifact queues, and it prints unchanged when the printer returns (§3a) |
| 7 | Scanner: plain EAN-13, price-embedded, weight-embedded, and a deliberately corrupted code (must reject, not guess) |
| 8 | Scanner while the search box has focus → becomes a line |
| 9 | Terminal: approve, decline, partial approval, timeout-then-approved, timeout-then-declined |
| 10 | Terminal unplugged at Pay → card disabled with a reason; cash unaffected |
| 11 | Label printer: Arabic label with price and barcode, readable on a shelf |
| 12 | Cold start on the **reference register** (§6a), timed against the 3 s budget |
| 13 | The sale screen read on the register's own display, in Arabic, by the same native reader who read the paper — the screen has no goldens, so this session is where a human sees it ([`ui-spec.md`](ui-spec.md) §8a) |

**A drill produces a record or it did not happen.** Each run is a dated file naming the drill, the commit or tag it ran against, **each device and its firmware**, the operator's name, start and end time, elapsed, outcome, and any surprise plus the case number it became. A normative reference document is not a log, and one tick for a class of devices hides the device that changed.

**A golden file proves bytes; only paper proves a receipt.**
