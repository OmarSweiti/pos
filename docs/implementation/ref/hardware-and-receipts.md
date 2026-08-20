# Hardware and receipts

Blueprint §5. Capability traits in `pos-hardware`; drivers behind them. **The UI never talks to a device** — it invokes a Tauri command and Rust does the work.

The two things this document exists to get right: **Arabic receipts**, and **a printer that fails after the money is taken**.

---

## 1 · The traits

```rust
// crates/pos-hardware/src/lib.rs
pub trait ReceiptPrinter: Send + Sync {
    fn print(&self, doc: &RenderedReceipt) -> Result<(), HwError>;
    fn open_drawer(&self) -> Result<(), HwError>;       // ESC p pulse via the printer port
    fn status(&self) -> PrinterStatus;                  // paper-out, cover-open, offline
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
pub trait Scale: Send + Sync { /* only if grocery/deli */ }
```

`ReceiptPrinter` and `SimulatedPrinter` already exist and are correct. Everything else grows behind the same pattern.

**The simulator implements every trait, with fault injection.** CI and demos run hardware-free. This is not a testing convenience — it is why a new developer can be productive on day one without a printer on the desk, and why every fault path has a test.

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

Keep a plain-codepage fallback for exotic printers that reject raster, and make it a store setting — but the default is raster and the Arabic goldens are raster.

**The font** ([1.7.2](../phase-1-sellable-mvp.md)) is embedded in the app, covers Arabic and Latin, and is **the same file the UI uses**. The receipt then looks like the screen, which is what a merchant expects and what makes discrepancies obvious.

### 2.2 Golden files

Six fixtures, byte-diffed in CI:

| Golden | Proves |
|---|---|
| `receipt_ar_80mm.bin` | Arabic shaping, RTL order, column alignment |
| `receipt_ar_58mm.bin` | the narrow profile reflows rather than truncates |
| `receipt_bilingual_80mm.bin` | mixed Arabic/English runs in one line |
| `receipt_multirate_80mm.bin` | the tax summary renders exempt and standard as distinct rows |
| `receipt_duplicate_80mm.bin` | the DUPLICATE watermark |
| `receipt_training_80mm.bin` | the TRAINING watermark |

Regenerating is deliberate: `UPDATE_GOLDEN=1 cargo test`, then **look at the diff** — ideally by printing it — before committing. A golden file proves bytes; **only paper proves a receipt.**

### 2.3 Receipt anatomy

Per master plan B.6 and C.11, in order:

```
logo (raster)
merchant legal name · address · TIN                ← legally required
doc type: SALE / REFUND  [+ DUPLICATE / TRAINING watermark]
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

The exact legal minimum belongs on the tax advisor's checklist (merchant decision #9). Everything above is the defensible baseline.

### 2.4 Templates as data

Header and footer text, logo, and toggles are editable in the back office and **versioned**. The renderer lives at `pos-hardware`'s printer boundary and is fed a `ReceiptModel` built in `pos-domain` — so the printed receipt, the emailed PDF, and the fiscal document all derive from one source and cannot disagree.

---

## 3 · The print-failure rules

These are the rules that decide whether a paper jam costs a receipt or a sale.

1. **Poll status before finalize, at Pay.** Paper-out warns *before* the money is taken, not after (master plan C.15). A warning after payment is a warning that helps nobody.
2. **A print failure after finalize never un-finalizes the sale.** The money moved. The sale stands. `receipt_printed_at` stays NULL, an incident is logged, and one-tap reprint is offered, marked DUPLICATE (E.46).
3. **Persist the rendered bytes with the sale.** A reprint is then byte-exact, including the fiscal QR — which is what makes E.47 (reprint days later, from another register) work at all.
4. **Print through a retry queue.** A jam must never lose a receipt.
5. **Cutter and drawer kick** are part of the same byte stream: `GS V` cut, `ESC p` drawer pulse.

**Tests:** `paper_out_warns_before_tender_not_after` · `print_failure_after_finalize_leaves_sale_complete` · `reprint_is_byte_identical_including_qr` · `queue_survives_restart`.

---

## 4 · Cash drawer

Almost always kicked via the printer's drawer port (`ESC p m t1 t2`). That is the default path and needs no separate driver.

**Every drawer open is logged with the actor and the cause — including no-sale opens.** Spikes in no-sale opens are the classic theft tell (E.35), and the count appears on X and Z reports. A drawer jammed or left open at shift close does not block the close; the state is logged and an alert raised (E.50).

---

## 5 · Barcode scanners

**Keyboard wedge is the default** — the scanner is a keyboard that types very fast and presses Enter.

The UI keeps a hidden input capturing keystrokes anywhere on the sale screen and distinguishes a scan from typing by **inter-key timing**: a burst with < 30 ms between characters, terminated by Enter, is a scan.

> **The detail that breaks most implementations: scans must route correctly even when focus is in the search box.** A cashier types two letters, then scans; the scan must become a line, not extra text in the search field. Test `scan_routes_while_search_focused` exists for exactly this.

Serial and HID-POS modes come later behind `BarcodeSource`, for merchants whose environment makes wedge mode unreliable.

---

## 6 · Payment terminals

Full treatment in [`phase-2-money-grade.md`](../phase-2-money-grade.md) §2.1 and [`security-compliance.md`](security-compliance.md) §3. The hardware-layer summary:

- **Semi-integrated only.** Amount and reference go to the terminal; result and reference come back. **PAN, track data and CVV never exist in this process, this database, or these logs.**
- **Timeout is `Unknown`, never `Declined`.** Status-query before any retry. Always.
- **Store `psp_ref` on every card tender** — reconciliation and refunds both depend on it.
- **Refunds go through the PSP against the original reference**, never as a fresh charge in the opposite direction.
- CliQ / wallet QR (J.1) is a `PaymentTerminal` implementation, not a new integration class: it reaches merchants through bank POS devices. Its one distinctive failure mode is a **lost callback** — poll by payment reference before declaring unpaid; the tender stays `Pending`, never silently dropped (E.65).

---

## 7 · Later devices

Slot in behind the same trait pattern when a merchant actually has one:

| Device | When | Note |
|---|---|---|
| Customer display / second screen | Phase 4+ | running cart, total, wallet-pay QR |
| Scale (serial) | only if grocery/deli | until then, price-embedded barcodes cover most cases |
| Label printer | Phase 4 | **a compliance feature** in Jordan — see [`../phase-4-depth.md`](../phase-4-depth.md) §4.6 |
| Kitchen printer | hospitality epic | a different product; the trait pattern still holds |

---

## 8 · Diagnostics screen

Where support teams live. Every trait gets a test action:

- test print (all six goldens, on paper)
- drawer kick
- scanner echo — shows raw scan input and the parsed result
- payment terminal ping and last-transaction query
- printer status and paper level
- fiscal queue depth, oldest uncleared age, last error
- database health, size, backup age
- clock skew against the server
- disk space
- audit-chain verification

**Done when** a support person can be walked through the whole screen by phone and reach a diagnosis without remote access.

---

## 9 · The hardware-lab checklist

**Before every release.** One real thermal printer, one real scanner, one real payment terminal.

| # | Check |
|---|---|
| 1 | All six receipt goldens print on paper; Arabic read and confirmed **by a native reader** |
| 2 | 58 mm profile prints on a 58 mm printer without truncation |
| 3 | Drawer kicks on cash completion and on no-sale, and both are logged |
| 4 | Paper removed mid-print → sale stands, reprint produces DUPLICATE with the identical QR |
| 5 | Scanner: plain EAN-13, price-embedded, weight-embedded, and a deliberately corrupted code (must reject, not guess) |
| 6 | Scanner while the search box has focus → becomes a line |
| 7 | Terminal: approve, decline, partial approval, timeout-then-approved, timeout-then-declined |
| 8 | Terminal unplugged at Pay → card disabled with a reason; cash unaffected |
| 9 | Label printer: Arabic label with price and barcode, readable on a shelf |
| 10 | Cold start on the slowest register hardware you support, timed against the 3 s budget |

Signed and dated. **A golden file proves bytes; only paper proves a receipt.**
