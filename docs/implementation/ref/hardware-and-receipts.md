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

    fn status(&self) -> PrinterStatus;                  // paper-out, cover-open, offline — or unsupported (§2)

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

`SimulatedPrinter` implements every trait, **with fault injection at each of the four outcomes and at an arbitrary byte offset**. CI and demos run hardware-free. That is not a testing convenience — it is why a new developer is productive on day one without a printer on the desk, and why every fault path has a test rather than a hope. It is also the first rung of the ladder the whole build stands on until the hardware qualification gate (§6b): no device is bought before it.

---

## 2 · Receipt printers

**Raw ESC/POS**, over TCP port 9100 for network printers, `serialport` for RS-232, `rusb`/`hidapi` for USB. **Never webview printing** — spooler latency and per-OS quirks make it slow and non-deterministic; raw bytes are identical everywhere.

**The driver is [1.7.10](../phase-1-sellable-mvp.md), and it is network first.** Port 9100 needs no driver on any operating system. USB does not share that property: current Epson and Star printers default to a vendor-specific USB class, which Windows reaches only through the vendor's driver or a WinUSB binding, so serial and USB transports wait for a qualified device that needs them. A printer that never answers a real-time status request reports *status unsupported* rather than ready — some answer `DLE EOT` on serial only — and §3 rule 1 then knows it cannot warn before the money is taken.

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

**And a height limit per command, which is a device fact.** Epson's ESC/POS reference caps one `GS v 0` at 2303 dot rows on the TM-T20III, TM-T20IV and TM-m30III, and at 4095 on the TM-T88VII, and marks the command obsolete in favour of `GS ( L`. A taller page goes out in strips at the profile's limit ([1.7.9](../phase-1-sellable-mvp.md)); the strips join to the same bitmap, so the paper cannot show the seam.

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
2. Read the image diff in the pull request. If the change touches shaping, font metrics, or the font file itself, print it as well — paper is the only place some defects appear. Before milestone 5.8 no printer has been bought (§6b), so the image is the review and 5.8.4 prints every golden on paper before a customer is handed a receipt.
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

**The layout trap.** A wedge scanner sends key presses, and the operating system's active layout turns them into characters. Under Arabic (101), a scanner on its default US layout types Arabic letters wherever its code had Latin ones — a *different* code, not an unreadable one — while digit-only EAN and UPC codes usually survive, which is why the defect hides in a demo. [1.11.6b](../phase-1-sellable-mvp.md) decodes a recognised burst by physical key (`KeyboardEvent.code`) rather than by character. A scanner's own keyboard-country or ALT/keypad setting is the device-side answer, and the matrix's scanner row records which one a device was qualified with.

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
| `transport` | TCP 9100 · serial · USB — TCP first ([1.7.10](../phase-1-sellable-mvp.md)) |
| `dot_width`, `paper_mm` | 576 px / 80 mm, 384 px / 58 mm — the layout input |
| `raster_command`, `raster_max_rows` | `GS v 0` and `GS ( L` are not interchangeable, and one command carries a device-specific number of dot rows — 2303 on an Epson TM-T20III, 4095 on a TM-T88VII. A taller page is sent in strips at that limit ([1.7.9](../phase-1-sellable-mvp.md)) |
| `cut_command`, `drawer_pulse` | pin (2 or 5), on-time and off-time; wrong timings do not open the drawer, and the drawer's solenoid voltage must match the port's (usually 24 V) |
| `status_protocol` | real-time status is what makes rule 1 of §3 possible; a device without it — or with it on serial only, when it is bound over the network — cannot warn before the money is taken |
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

**The matrix ships empty except the simulator, and that is the honest state.** Filling it with model numbers nobody has held would be the same defect as a compliance claim nobody earned. Until a class has a row, this product supports no device in that class and says so. It stays empty until milestone 5.8 qualifies the first devices (§6b), and a device profile does not change that: a profile describes a candidate, a row describes a device somebody held.

### 6a.1 · The register-hardware rows, and the reference register

This is the one table in this document that a program reads. `just bench-gate`
([`scripts/bench-gate.py`](../../../scripts/bench-gate.py), microstep 1.2.0) parses the rows between
the two `bench-gate:register-hardware-matrix` markers and compares the **first** one, field for
field, with [`benchmarks/reference-register.toml`](../../../benchmarks/reference-register.toml).
Neither record is allowed to be the only one: [`../01-conventions.md`](../01-conventions.md) §7.1
accepts no baseline "while either record is blank or they disagree", and the gate exits non-zero
until both are filled and identical.

**Rows are ordered lowest-capability first, and row 1 is the reference register** — that is what
the table row above means by "the lowest row", made mechanical so a budget cannot be measured on
whichever machine was nearest. The twelve columns are the identity §7.1 mirrors (CPU, RAM, storage,
OS version, power mode, device-matrix identity, release-build profile) plus this section's
qualification triple, because a profile with no qualifying commit is a claim. The release-build
profile is a column because a measurement is a property of a machine **and** a build: without it a
debug build could claim this row's budget. Every cell is compared as text, and a unit belongs
inside the value — a bare `8` could be GB or GiB.

<!-- bench-gate:register-hardware-matrix -->

| `profile_id` | `maker` | `model` | `cpu` | `ram` | `storage` | `os_version` | `power_mode` | `release_profile` | `qualified_at` | `qualified_by` | `qualifying_commit` |
|---|---|---|---|---|---|---|---|---|---|---|---|

<!-- /bench-gate:register-hardware-matrix -->

**That table has no rows, and the mirror file's twelve values are blank to match.** No register has
been bought, held or qualified, so there is nothing to write in it, and inventing a machine here
would hand every performance budget in §7 a referent that does not exist. The consequence is
deliberate and is the state of the repository today: `python3 scripts/bench-gate.py
--check-profile` exits non-zero and names both blank records. **They stay blank until the hardware
qualification gate** (§6b): the register is bought there, against a frozen matrix, and filling this
row plus the mirror file is the deferred half of microstep 1.2.0 in
[`../phase-1-sellable-mvp.md`](../phase-1-sellable-mvp.md), executed at 5.8.2. Until then every
budget is watched relatively and none is claimed ([`../01-conventions.md`](../01-conventions.md) §7.2).

**The first printer row is written at the gate, and the eye reads an image until then.** Group 1.7's own acceptance is that Arabic letter joining is *"verified by eye once and by golden file forever"*, and it is met in two steps: the native reader confirms each golden's PNG in the pull request that changes it (§2.2), and confirms the same seven on paper at 5.8.4, before a customer is handed one. The first printer rows, their profiles and their qualification dates are 5.8.3's, and so is every other class's first row, from devices ordered when Phase 4 begins (§6b.6). The terminal is the acquirer's to supply: the conversation starts early because it is the long lead, and the device is qualified at the same gate.

**The matrix is what the vendor supports; what a store owns is a different list.** The store's own devices are recorded on the questionnaire ([`merchant-decisions.md`](merchant-decisions.md) §G) — maker, model, firmware, and for a scale its serial and verification (§7). A store running a device with no matrix row is an unsupported deployment, and that is a sentence somebody has to be able to say before a pilot rather than after a support call.

**Qualifying a new device** — the whole procedure, because "it worked on my desk" is how a fleet acquires a device nobody can support:

1. Obtain the device — at milestone 5.8, or on a recorded exception (§6b) — and record maker, model and firmware version.
2. Write or select its profile. A device that needs a code change rather than a profile row is a driver, and a driver is a microstep.
3. Run its class's conformance suite against it with that profile (§6b.3), then the §9 lab checklist **against that device**, not against the class. Where the device disagrees with its profile, the profile gets a new version and the device's capture becomes a fixture (§6b.6) — unless the device breaks §3's or §4's contract, in which case it does not qualify. Record the result per device.
4. If the width is new, commit its golden pair (`.bin` + `.png`) and get the native-reader confirmation in the same pull request (§2.2).
5. Add the row, with the qualifying commit. A profile with no qualifying commit is a claim.
6. Record what it cost and where it was bought. §9's checks 2 and 11 each need their own device on the desk, and a device nobody ordered is a release that slips.

**What protects a device whose behaviour changed between releases.** Nothing in this repository executes against hardware except the conformance suite's device target, and that runs only when somebody points it at one (§6b.3), so the honest controls are narrow and stated:

- The profile records the firmware range it was qualified for. A device reporting a firmware outside that range raises a **device-health warning and never refuses to print** — a printer that stops working because it updated itself is worse than one that prints with an unverified profile.
- From milestone 5.8 on, the lab checklist runs before every release **against the matrix's own devices**, and its record is per device, per firmware. One tick for "printers" hides the printer that changed.
- A firmware change discovered to break a profile is a new profile row and a new qualification, never an edit to the old one — the old row is what a merchant still running that firmware is using.

---

## 6b · Simulator-first, and the hardware qualification gate

**No device is bought before the hardware qualification gate, milestone [5.8](../phase-5-harden-and-launch.md).** Until then every device class is developed and verified against software that stands in for it, and the gate is where physical devices are bought, held and qualified — before any register stands in front of a merchant or a customer. Decided on 20 October 2026; it replaces *"order the hardware before group 1.7 starts"*.

**Why, in four costs the old order paid in advance:**

- **A device bought in Phase 1 is bought against a guess.** The pilot merchants, their assortment and the devices some of them already own are Phase-3 and Phase-4 facts, and a reference register chosen now can be the wrong floor by the time a merchant names theirs. Bought at the gate, a device is bought against a frozen matrix and a known pilot.
- **An emulator injects a fault on command.** Paper out at byte *n*, a cover opened mid-strip, a printer that answers `DLE EOT` late or never, a drawer that reports nothing, a link that drops after a partial write, a scanner on the wrong keyboard layout: each is one line of a test, repeated on every commit. On a desk device each is a person with a hand on the lid, once.
- **One emulator stands in for many devices.** A profile is data (§6b.2), so the suites run every candidate printer, scanner and screen on every commit — a wider matrix than any lab would buy.
- **The money stays in the bank until the product can use what it buys,** and the device generation bought is the one current at the pilot rather than at the first golden.

**What it costs, stated rather than discovered.** Until the gate no absolute performance budget is proven, because none can be measured without the reference register (§6a.1, [`../01-conventions.md`](../01-conventions.md) §7.2); Arabic on paper is unproven, because a golden proves bytes and its PNG proves the rasteriser's own image; durability under a real power cut is unproven (case 1b); and no device is supported, because the matrix ships empty (§6a). Each of those is a named item of the gate (§6b.4), not a gap left open. And the gate finds late what a desk device would have found early — the risk register in [`../00-master-plan.md`](../00-master-plan.md) §6 carries that, with its triggers.

**When a device may be bought earlier.** Only with a written reason, recorded beside the purchase: a risk-register trigger has fired — a provisional measurement puts the floor machine in danger — or a question only a device can answer would change a design decision now. A device an acquirer or a merchant supplies at no cost is not a purchase and is welcome; it qualifies through the same procedure, and its evidence counts once it is a dated record.

### 6b.1 · The fidelity ladder

Four rungs. Each proves something the rung below cannot, and none proves what the rung above it proves.

| Rung | What it is | What it proves | What it cannot prove |
|---|---|---|---|
| **1 · In-process simulator** | `SimulatedPrinter` and the other trait implementations ([1.7.8](../phase-1-sellable-mvp.md), [2.1.4](../phase-2-money-grade.md)) | the domain's handling of every outcome, at every byte boundary, on every commit | anything about bytes on a wire |
| **2 · Protocol emulator** | a separate process speaking the device's protocol, parameterised by a device profile: the emulated network printer on TCP 9100 ([1.7.12](../phase-1-sellable-mvp.md)), keyboard-wedge burst injection ([1.11.6c](../phase-1-sellable-mvp.md)), register-screen profiles ([1.11.14b](../phase-1-sellable-mvp.md)), virtual serial ports — `socat` pseudo-terminals on macOS and Linux, `com0com` on Windows — for serial scanners, scales and pole displays, a second application window for a customer screen, and the acquirer's own test host or terminal simulator where it offers one | the real driver against a real wire, across every profile, under every scripted fault | that the profile is true of the device |
| **3 · Recorded captures** | bytes and status replies captured from a physical device at the gate, committed as fixtures and replayed through the emulator | that the emulator still behaves like the device it was corrected against | anything about a device nobody has captured |
| **4 · The physical device** | the gate's devices, bought against the frozen matrix (§6b.5) | everything above, on paper, in a hand, on the floor machine | — |

**The emulators are tools, not product.** They live in `pos-hardware` behind a feature no register build enables, and nothing a register ships depends on them.

### 6b.2 · Device profiles — one emulator, many devices

Every emulator is parameterised by a **device profile**: a versioned, data-only file, one per model or one per class, under `crates/pos-hardware/profiles/<class>/` ([1.7.11](../phase-1-sellable-mvp.md)). The same emulator therefore stands in for every device its class can describe, and adding a candidate is a file, not a driver.

Three rules make a profile evidence rather than decoration:

1. **Every value names its source** — a maker's document, a seller's written answer, or a capture from a device — and the source is a citation, not an adjective.
2. **`unknown` is a value, and it is never filled by a guess.** Where the class bounds a parameter, the suite sweeps the bound; where it does not, the case reports *device only* and the parameter joins the gate's list (§6b.5). A profile with an unknown in a parameter the driver needs cannot be bound to a register — 1.7.9's rule for the raster limit, generalised.
3. **A profile is versioned, never edited in place.** A changed value — a datasheet correction, a firmware note, the gate's evidence — is a new version, and the old one stays because a capture, a fixture or a qualification may name it. It is §6a's firmware rule, applied before the matrix has a row.

**A profile is only as true as its source until a physical device has confirmed it.** And a profile is not a matrix row: §6a's matrix is what the vendor *supports*, and a row needs a qualifying commit against a device somebody held. A profile describes a candidate, and claims nothing more.

What each class's profile carries:

| Class | Parameters |
|---|---|
| Receipt printer | paper width; dots per line; raster command, and the most dot rows one command takes; code pages, for the §2.1 fallback only; cutter, and the feed it needs; real-time status — which replies, over which transport, or none |
| Cash drawer | kick channel — a printer's port, or an interface of its own; pin and pulse timings; solenoid voltage and resistance; whether a sensor exists, and its polarity in the status reply |
| Barcode scanner | interface — USB HID keyboard, USB-COM, serial, HID-POS; prefix and suffix; inter-key timing; keyboard country, ALT or keypad emulation; symbologies |
| Register screen | resolution; display scale; pointer — capacitive touch, resistive touch, mouse; panel size |
| Register machine | §6a.1's identity columns: CPU, RAM, storage, OS edition and build, power mode. Its speed cannot be emulated (§6b.4) |
| Customer display | a pole display's lines, columns and character set — a 5 × 7 dot-matrix pole cannot show Arabic — or a second screen's resolution |
| Scale | protocol, units and resolution, and the verification facts §7 requires |
| Payment terminal | the acquirer's protocol and transport, last-transaction-status query, partial approval, refund by reference, timeouts, model and firmware — written from [2.1.1](../phase-2-money-grade.md)'s answers |
| Label printer | command language, dots per line, label sizes ([4.6.1](../phase-4-depth.md)) |

**The seed.** Profiles start from the candidates [`../../hardware-lab.md`](../../hardware-lab.md) researched on 3 October 2026, value by value from the sources that file cites; what no source states is `unknown`. Rows marked *(class)* are synthetic sweeps that describe a class rather than a model, and say so in their source. Once 1.7.11 lands, the files are authoritative and this table is the seed they grew from.

| Profile | Known, and where from | Unknown until the gate |
|---|---|---|
| Epson TM-T20III, Ethernet — 80 mm | 576 dots; `GS v 0` at most 2303 rows, with `GS ( L` recommended; status over the socket; auto-cutter; drawer port at 24 V — Epson's technical reference and ESC/POS reference (hardware-lab §4.2) | the cut feed; the firmware range |
| Epson TM-T20IV — 80 mm | 576 dots; `GS v 0` at most 2303 rows — Epson (hardware-lab §4.2) | status over Ethernet; the cut feed |
| Epson TM-T88VII — 80 mm | **512 dots** at 180 dpi; `GS v 0` at most 4095 rows; a vendor-class USB interface — Epson's technical reference (hardware-lab §4.2) | — seeded *because* its width differs from the goldens: it exercises the new-width path |
| Epson TM-m10 — 58 mm | **420 dots** — Epson's specifications (hardware-lab §4.3) | the raster limit; status transport |
| Bixolon SRP-332III — 80 mm and 58 mm, two profiles | 576 and **384** dots at 203 dpi, the second through its 2-inch partition; USB, Ethernet and serial; auto-cutter; 24 V drawer port; `DLE EOT` and port 9100 documented — Bixolon's manual and specification sheet (hardware-lab §4.3) | **status over Ethernet**; the raster limit |
| Xprinter XP-N160II — 80 mm | `DLE EOT` valid on serial only, so status over Ethernet is *none* — Xprinter's manual (hardware-lab §4.2) | dots per line; the raster limit |
| HPRT TP585 — 58 mm | native 58 mm; cutter; 24 V drawer port; USB only — HPRT (hardware-lab §4.3) | dots per line; status |
| Datalogic QuickScan QD2590 | wired USB HID keyboard; 2D; keyboard countries Arabic 101, 102 and 102 AZERTY; alternative key encoding; ALT output — Datalogic's reference guide (hardware-lab §4.4) | inter-key timing at its fastest setting |
| Zebra DS2208 | keyboard country Arabic (101); keypad emulation — Zebra's reference guide (hardware-lab §4.4) | inter-key timing |
| Honeywell Voyager 1470g | keyboard countries without Arabic, and no ALT mode — Honeywell's user guide (hardware-lab §4.4) | — seeded as the profile that proves 1.11.6b alone carries an Arabic layout |
| Posiflex CR-4000-G2 drawer | printer-driven; 12–24 V — Posiflex (hardware-lab §4.5) | pin; solenoid resistance; sensor and polarity |
| APG Vasario VB320 drawer | printer-driven at 24 V, RJ45 jack — POSGuys (hardware-lab §4.5) | pin; solenoid resistance; sensor |
| Posiflex XT-3815-G2 register | Celeron J6412; up to 32 GB; M.2 SATA; TPM 2.0; 15″ **1024 × 768 capacitive** panel — Posiflex (hardware-lab §4.1) | display scale |
| MSI PRO AP162T register | Intel N100; 8 GB; 128 GB NVMe; 15.6″ **1920 × 1080 resistive single-touch** panel — MSI (hardware-lab §4.1) | display scale |
| The minimum window *(class)* | **1024 × 640**, the declared minimum ([1.11.14](../phase-1-sellable-mvp.md)) | — |
| Wedge scanner *(class)* | inter-key timing swept across and beyond §5's 30 ms threshold, under the US and Arabic (101) host layouts | — synthetic by definition |
| Pole display *(class)* | a 5 × 7 dot matrix that cannot show Arabic (hardware-lab §5) | everything model-specific |

The scale, the payment terminal and the label printer have no seed: no device is named, and the terminal's profile is written from 2.1.1's written answers. Star's mC-Print models speak StarPRNT rather than ESC/POS (hardware-lab §4.2), so they would need an emulator of their own and are not seeded.

### 6b.3 · One conformance suite, two targets

What makes deferral safe is that **the gate runs the suite CI already runs**. Each class has one device-contract suite, written once against the trait and a profile — [1.7.13](../phase-1-sellable-mvp.md) for receipt printers and their drawers, [1.11.6c](../phase-1-sellable-mvp.md) for scanners, [1.11.14b](../phase-1-sellable-mvp.md) for register screens — and it runs against either of two targets:

- **the emulator**, once per profile, on every commit;
- **a physical device**, with the profile it is being qualified against, at the gate — and only when the device's address and the profile id are both given explicitly, so no test can reach a stranger's printer on a shared network.

CI also replays the suite's byte streams through an independent emulator nobody here wrote (§6b.8), so the bytes are never graded only by this project's own reading of the specification.

The suite asserts §3's and §4's contract, never a device's convenience: the four print outcomes, status read or reported unsupported, strips at the profile's limit (case 93), exactly one drawer pulse per authorised opening and none on a retry, a reprint or a queue drain, paper pulled mid-print, a printer absent at shift open (§3a), and §9's scan kinds. A device that fails it has disagreed with its profile or with the contract, and §6b.6 says how the gate tells which.

**Tests:** `every_device_profile_names_a_source_for_every_value` · `the_printer_conformance_suite_passes_for_every_profile` · `the_device_target_refuses_to_run_without_an_explicit_device_and_profile` · `a_captured_device_trace_replays_through_the_emulator`

### 6b.4 · What only a device can prove

These belong to the gate, each owned by a microstep of milestone 5.8, and none is claimed before it.

| Only a device proves | Why no emulator can | Owner |
|---|---|---|
| A receipt taller than 2303 dot rows prints whole on a real TM-T20III (case 93's device half) | a raster limit in a profile is a datasheet's claim, and firmware decides | 5.8.3 |
| Arabic joins, orders and reads correctly **on paper**, at both widths, confirmed by a native reader | the PNG is the rasteriser's own image; head, density and paper are not in it | 5.8.4 |
| The drawer opens on its solenoid, once, and its sensor reports what the profile says | pulse timing, voltage and sensor polarity are electrical facts | 5.8.3 |
| Paper-out, cover-open and offline are read back as the profile says | sensor wiring and firmware reporting differ by model | 5.8.3 |
| A real scanner, configured as recorded, reads under Arabic (101) and English (US) (case 94's device half) | the scanner's own firmware decides its timing and what its keyboard country sends | 5.8.3 |
| USB enumeration and the Windows driver path, for any USB device a matrix row binds | the operating system decides which driver claims a vendor-class device | 5.8.3 |
| The payment terminal behaves as the acquirer's protocol says, and its listing facts are true | the terminal, its firmware and its listing belong to the acquirer | 5.8.3, with [2.1.5](../phase-2-money-grade.md) |
| Every absolute performance budget, on the reference register | a CPU, its storage and its thermal limits cannot be emulated by a faster machine | 5.8.2 |
| A sale survives a real power cut on the register's own storage (case 1b) | a virtual machine's hard stop proves the write ordering, not that the disk honours a flush | 5.8.4 |
| The sale screen reads correctly on the register's own display, to the native reader (§9 check 13) | glare, colour, size and touch are properties of the panel | 5.8.4 |
| A customer display is legible at its distance | legibility is a human fact about a physical panel | 5.8.4, when a pilot store has one |

### 6b.5 · Wide in simulation, narrow on device

CI runs every suite across **every** profile: that is the wide matrix. The gate buys a **narrow** one — the fewest devices that confirm every parameter the pilot depends on. The selection rule, in order:

1. **Every class the pilot uses has a physical device**: a receipt printer at each paper width a pilot store uses, a drawer, a scanner, the reference register, the acquirer's terminal, and a label printer, customer display or scale only if the pilot uses one.
2. **The pilot merchants' own devices come first.** A device a pilot store already owns is the profile that matters most, and if their registers are slower than the candidate floor, the reference register is theirs or slower ([`../../hardware-lab.md`](../../hardware-lab.md) §3).
3. **Within a class, cover the riskiest parameters rather than the most models**: every dot width that ships, both raster command paths where the pilot's devices differ, status over the network confirmed rather than assumed, **two printer makers** — ESC/POS is a family (§6a) — and one scanner run both by its own keyboard country and by 1.11.6b's decoding alone.
4. **Every `unknown` in a profile the pilot uses is resolved by a device in the set**, or that profile is not bound to a pilot register.
5. **A profile with no physical representative stays simulated only.** It is not a matrix row, and the product supports no device it describes.

[`../../hardware-lab.md`](../../hardware-lab.md) §4's candidate order already satisfies rules 1, 3 and 4 for the classes it covers. Rule 2 can only be applied once the pilot merchants are known, which is why the set is frozen at the gate and not before.

### 6b.6 · The gate

**Milestone 5.8 is Phase-5 work pulled forward, as fiscal certification is:** row 11 of the pre-pilot gate ([4.9.0](../phase-4-depth.md)) cannot be signed until it has passed, so no register stands in front of a real merchant or customer before it. Its order goes out when Phase 4 begins ([`../00-master-plan.md`](../00-master-plan.md) §6a), because imported items take weeks and the qualification itself is about three weeks of work before 4.9.0.

What it does, in order:

1. **Freeze the narrow matrix and order it** (5.8.1).
2. **Qualify the reference register and prove every absolute budget on it** (5.8.2) — 1.2.0's deferred half, and the live measurement job 1.12.3 defined.
3. **Run each class's suite against each device, and correct the profiles from the evidence** (5.8.3). A disagreement resolves one of two ways: the profile was wrong, and it gets a new version; or the device breaks §3's or §4's contract, and it does not qualify, whatever its datasheet says. Either way **the device's recorded bytes and replies become golden fixtures** — rung 3 — and the emulator must replay them from then on. The emulator gets truer every time a device teaches it something, and stays true afterwards.
4. **Run §9's checklist on paper, the physical drills, and the demonstrations earlier phases ran against emulators** (5.8.4).

Its output is the first matrix rows (§6a), each with a qualifying commit; row 1 of §6a.1 and `benchmarks/reference-register.toml`, filled; the committed captures; and dated records in [`docs/drills/`](../../drills/README.md).

### 6b.7 · What this does not claim

- **That an emulator is a device.** Conformance against a profile proves the software against *our model* of the device. Until 5.8.3, "supports device X" is said of no device, and the matrix ships empty (§6a).
- **That a budget is met before 5.8.2.** Until then regressions are caught relatively and nothing is accepted as a baseline ([`../01-conventions.md`](../01-conventions.md) §7.2).
- **That Arabic is right on paper before 5.8.4.** Until then the goldens prove bytes, the PNGs prove the rasteriser's image, and the native reader confirms the image.
- **That a datasheet is a fact.** It is a maker's claim, and the gate exists partly to catch the ones that are wrong.
- **That an independent emulator agreeing with us proves a device will.** emupos (§6b.8) proves our bytes are read the same way by a second implementation of the specification; a device can still disagree with both, which is what 5.8.3 is for.
- **Anything about payment certification, PCI scope or the acquirer's listing.** Those were never device questions, and nothing here settles them.

### 6b.8 · The second opinion — an independent emulator

**An emulator this project writes cannot catch a misreading of the specification that this project's driver shares.** Both read the same reference and the same profiles, so one misreading can pass every suite in the repository. The professional answer is a second, independent implementation of the protocol as a cross-check — not a replacement for 1.7.12, which owns what only a profile can say, but a reader nobody here wrote.

**Measured, not assumed: a spike on 20 October 2026** rendered real receipts through this pipeline (1.7.1–1.7.4) — Arabic at 80 mm (576 × 1238 dots) and 58 mm (384 × 1266), and a 60-line basket at 80 mm (576 × 5450, past the TM-T20III's 2303-row command limit) — and sent each, in every framing 1.7.9 will produce, to the candidates, each run in a throwaway container bound to `127.0.0.1`:

| Candidate | What it did with our bytes | Verdict |
|---|---|---|
| **emupos 0.4.0** (Apache-2.0, Python) | Every render **dot-identical** to ours: the 80 mm receipt as one `GS v 0` (0 of 713 088 dots differ), the 5450-row basket as one command and as ≤ 2303-row `GS v 0` strips, and the 80 mm receipt as ≤ 900-row `GS ( L` store-and-print strips. `DLE EOT` 1–4 replies for ready, paper-out and cover-open match Epson's bit definitions; a job sent at paper-out is held and completes when paper returns; `ESC p` opens its drawer and sets the drawer bit. **It does not model** per-command raster limits — it printed whole the 5450-row command a TM-T20III refuses — nor a raster whose width disagrees with its profile | **Adopted as a cross-check** (below). Young: alpha, one maintainer, first release 14 September 2026, and the container image its documentation names does not exist |
| **escpos-netprinter 3.2** (AGPL-3.0) | Renders a tiny raster, but **fails** on our real receipts (`esc2html` exits 255: *"zero-byte-long argument specified"*); amd64 only; a 10-second print limit | Rejected |
| **CrossEscPos 1.2.0** (.NET) | Not run: it describes itself as an unfinished experiment with a partial command set | Rejected |
| **POS for .NET simulators** (Microsoft, UnifiedPOS) | API-level simulators for .NET service objects; this register is not a .NET application | Not applicable — but Microsoft's own framing of them is this section's: simulators *"during the early stages of development … and for testing configurations before deployment"* ([POS for .NET FAQ](https://learn.microsoft.com/en-us/previous-versions/windows/embedded/ee406171(v=winembedded.0))) |

**What follows from it.** 1.7.13 replays every golden and every conformance byte stream through a pinned emupos and requires a dot-identical render and a status reading that agrees with ours. Pinned means exact: one version, installed from hash-locked requirements into a container image built in CI from `infra/emulators/emupos/`, never published, never on a register, never in a release. Where emupos models nothing — row limits, width agreement — 1.7.12's profile-driven emulator remains the only judge. A disagreement fails the build and is investigated, never suppressed: it means our bytes, our emulator or theirs is wrong, and where the specification cannot settle which, a device does (5.8.3). If emupos stops being maintained, the cross-check keeps its last good version; nothing in the suite depends on it for a property our own emulator does not also prove.

**And one design fact the spike surfaced for 1.7.9.** `GS ( L` counts its parameters in a 16-bit field — `(pL + pH × 256) = 10 + k` in Epson's definition of function 112 — so one store carries at most 65 525 data bytes: **910 rows at 576 dots, 1365 at 384**, whatever the device would take. A `GS ( L` strip is therefore the smaller of the profile's `raster_max_rows` and the command's own limit, unless the extended `GS 8 L` form is chosen.

---

## 7 · Later devices

Slot in behind the same trait pattern when a merchant actually has one — and **each arrives with its profile, its emulator and its conformance suite in the microstep that builds its trait** (§6b), so the class is developed the way the receipt printer was: against software first, on a device at the gate or at the first release that ships it.

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

**From milestone 5.8 on, before every release**, against the devices in the matrix (§6a) — which is at least two thermal printers (80 mm and 58 mm), one real scanner, and one real payment terminal. **Before 5.8 no release reaches a merchant** (§6b): a tagged candidate runs every check the last column marks *emulated* or *simulated* against the emulators and simulators instead, and the checks marked *device only* wait for 5.8.4.

| # | Check | Before 5.8 |
|---|---|---|
| 1 | All seven receipt goldens print on paper; Arabic read and confirmed **by a native reader** | device only — each changed golden's PNG is read in its own pull request (§2.2) |
| 2 | 58 mm profile prints on a 58 mm printer without truncation | device only |
| 3 | Drawer kicks once on cash completion, once on a no-sale open, and both are logged with actor and cause | emulated — the emulated printer records every pulse |
| 4 | **The drawer does not kick on a reprint, on a retry, or on a queue drain after restart** | emulated |
| 5 | Paper removed mid-print → sale stands, the job lands in `unknown`, nothing retries by itself, and the operator answer produces a DUPLICATE with the identical QR | emulated — a scripted fault |
| 6 | Printer unplugged at shift open → the shift opens, the sale completes, the artifact queues, and it prints unchanged when the printer returns (§3a) | emulated — no emulator listening, then one started |
| 7 | Scanner: plain EAN-13, price-embedded, weight-embedded, and a deliberately corrupted code (must reject, not guess) | emulated — every scanner profile |
| 8 | Scanner while the search box has focus → becomes a line | emulated |
| 9 | Terminal: approve, decline, partial approval, timeout-then-approved, timeout-then-declined | simulated — and against the acquirer's test host where it offers one |
| 10 | Terminal unplugged at Pay → card disabled with a reason; cash unaffected | simulated |
| 11 | Label printer: Arabic label with price and barcode, readable on a shelf | device only |
| 12 | Cold start on the **reference register** (§6a), timed against the 3 s budget | device only — 5.8.2 |
| 13 | The sale screen read on the register's own display, in Arabic, by the same native reader who read the paper — the screen has no goldens, so this session is where a human sees it ([`ui-spec.md`](ui-spec.md) §8a) | device only — before it, the reader reads each register-screen profile |

**A drill produces a record or it did not happen.** Each run is a dated file naming the drill, the commit or tag it ran against, **each device and its firmware** — or, for a run before 5.8, each emulator and the profile versions it ran — the operator's name, start and end time, elapsed, outcome, and any surprise plus the case number it became. A normative reference document is not a log, and one tick for a class of devices hides the device that changed.

**A golden file proves bytes; only paper proves a receipt.**
