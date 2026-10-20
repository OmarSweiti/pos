**Runtime change: yes.** `pos-hardware`'s encoder now cuts a tall page into raster strips sized by a printer profile. This validation drives the head's real `escpos::encode` and `PrinterProfile`, and prints their output on a third-party printer emulator.

**Setup**
- **Code:** PR head `3429196` with a clean tree, byte-identical to `6876bc2`. No database is involved, because 1.7.9 touches none.
- **Input:** a throwaway harness outside the repository, linked against this head's `pos-hardware` and `pos-domain`. It renders a synthetic 60-line Arabic basket at 80 mm (576 × 5450 dots, taller than the 2303-row limit) and encodes it with `PrinterProfile::simulator(PaperWidth::Mm80)`, in each of the two framings.
- **Emulator:** **emupos 0.4.0** as an **Epson TM-T20III** (`epson-tm-t20iii`, TCP 9100), wheel sha256 `d41f15caa2b286e7303b12c4bdcb02dc395c75553479d02b20566e2bacc2bb9a`. It ran in a throwaway `python:3.13-slim` container, with its control API published on `127.0.0.1` only, and started with an empty receipt store.
- **Screenshots:** headless Chrome with a throwaway profile. This terminal has no macOS Screen Recording permission, so no desktop capture was taken.
- **Data:** synthetic only. The tax numbers are placeholders, and no card data appears.

| # | Step | Expected | Observed |
|---|---|---|---|
| 1 | Open emupos's control page, then send the `GS v 0` stream: 3 commands of 2303 + 2303 + 844 rows | one job, cut once, drawer untouched | ✅ `printer.job.completed … boundary=cut`, one job, drawer `Closed` (01) |
| 2 | Send the `GS ( L` stream: 6 store-and-print pairs of 910 × 5 + 900 rows | the same | ✅ the same (01) |
| 3 | Compare each receipt emupos printed with our bitmap, dot for dot | identical | ✅ **0 of 3,139,200 dots differ**, in both framings. emupos's two PNGs are byte-identical (sha256 `1a051fdf…`), so 02 shows one of them, unedited |
| 4 | Look at every strip boundary | no seam | ✅ all 7 boundaries match ours byte for byte, including 5 that cut through a line of text (03) |
| 5 | Walk each stream with a parser that shares no code with the encoder | `ESC @` first; the cut last; every byte inside a command; no `ESC p` outside a payload; payloads joined = the bitmap | ✅ all five hold for both streams (04 §2) |
| 6 | Encode a 58 mm page against the 80 mm profile | refused | ✅ `WidthMismatch { page_px: 384, profile_px: 576 }` (04 §1) |
| 7 | Bind, then encode, with a profile whose row limit is `Unknown` | refused | ✅ `bindable()` gives `UnknownRowLimit`, and `encode()` gives `Profile(UnknownRowLimit)` (04 §1) |
| 8 | `Done when`: `cargo nextest run -p pos-hardware escpos::` | exit 0 | ✅ 21 passed (04 §4). The "47 skipped" are the crate's other tests, excluded by the filter |

**Cross-check.** The `GS v 0` stream is byte-identical to the one the 20 October spike framed independently, in Python, from the same bitmap.

**Not validated here, and why**
- **A physical printer.** None is bought before milestone 5.8, so the paper half of E.93 (a receipt printed whole on a real TM-T20III) belongs to 5.8.3.
- **A device refusing an oversized command.** emupos does not enforce per-command row limits or check width (§6b.8). It would also print one 5450-row command, so it cannot show the refusal a real TM-T20III would make. For now our own tests hold that rule (`a_page_one_row_taller_than_the_limit_becomes_two_commands` and the strip property). 1.7.12's emulated network printer, which reads a printer profile, will refuse such a command on the wire.
- **The register app was not launched.** 1.7.9 adds no UI and no IPC command, and nothing under `apps/` calls the encoder yet.
- **#266** (an Arabic label broken mid-word) shows in the fiscal block at the foot of 02. It predates this PR and is unchanged by it.
