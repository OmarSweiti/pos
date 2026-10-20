**Runtime change: yes.** `pos-hardware` now reads device profiles from versioned JSON files, and its encoder reads a profile's width, framing and row limit through checks that refuse an unknown. This validation loads the committed files through the head's own loader, and puts them to work: on the encoder, on register binding, and on a third-party printer emulator.

**Setup**
- **Code:** PR head `a6e238c` with a clean tree. No database is involved, because 1.7.11 touches none.
- **Harness:** a throwaway program outside the repository, linked against this head's `pos-hardware` and `pos-domain`. It loads `crates/pos-hardware/profiles/` with `profile::data::load_dir`, then encodes the synthetic 60-line Arabic basket (576 × 5450 dots) with profiles read from their files.
- **Emulator:** **emupos 0.4.0** as an **Epson TM-T20III** (`epson-tm-t20iii`, TCP 9100), wheel sha256 `d41f15caa2b286e7303b12c4bdcb02dc395c75553479d02b20566e2bacc2bb9a`. It ran in a throwaway `python:3.13-slim` container with its control API on `127.0.0.1` only, and started with no receipts.
- **Mutation check:** run in a scratch worktree of `a6e238c`, so the repository was never edited.
- **Screenshots:** headless Chrome with a throwaway profile. The matrix (02) is rendered straight from the committed JSON.
- **Data:** synthetic only. The device facts are public, and the tax numbers on the receipt are placeholders.

| # | Step | Expected | Observed |
|---|---|---|---|
| 1 | `load_dir` over `profiles/` | all 17 seeded files load | ✅ 17 loaded: 8 printers, 2 drawers, 4 scanners, 3 screens (03 §1) |
| 2 | Show every value of every file | each value is known with a source, or `unknown`, exactly where §6b.2's seed says | ✅ 75 of 128 printer values unknown, 12 of 20 drawer values, 14 of 40 scanner values and 3 of 18 screen values. Three sample sources are shown verbatim (02) |
| 3 | Ask every seeded printer `bindable()` | all refused, each naming what it is missing | ✅ 8 of 8 refused. The TM-T20III, the best-sourced, still lacks `cut_command`, `cut_feed_rows`, `drawer_pulse` and `supports_raster_arabic` (03 §1) |
| 4 | `encode()` the basket with the **TM-T20III's file** | strips at the file's 2303 rows | ✅ 3 strips of 2303 + 2303 + 844 rows, byte-identical to 1.7.9's simulator stream (sha256 `63dac85b…`) (03 §1) |
| 5 | Print that stream on emupos as a TM-T20III, with its control page open | one job, cut once, drawer untouched, identical to our bitmap | ✅ `printer.job.completed … boundary=cut`, one job, drawer `Closed` (01). **0 of 3,139,200 dots differ** (03 §2) |
| 6 | `encode()` with the TM-T88VII's file (512 dots) | refused: a different width | ✅ "prints 512 dots across, the page is 576" (03 §1) |
| 7 | `encode()` with the Bixolon's and the Xprinter's files | refused: framing or width unknown | ✅ "does not know its raster_command" and "does not know its dot_width" (03 §1) |
| 8 | `Done when`: `cargo nextest run -p pos-hardware --test profiles` | exit 0 | ✅ 4 of 4 pass (04 §1) |
| 9 | Five mutants against the real files | each fails its rule's test | ✅ a value edited in place, a source cut to "the datasheet", a gap filled with a guess, a deleted version, and binding that checks nothing: all five caught, and all four tests green once restored (04 §2) |

**Not validated here, and why**
- **Any physical device.** None is bought before milestone 5.8. These profiles describe candidates, and the gate settles their unknowns.
- **The register app was not launched.** 1.7.11 adds no UI and no IPC command. The files' first runtime readers are 1.7.12's emulated printer and 1.7.13's conformance suite.
- **The web tests that read the scanner and screen files** do not exist yet (1.11.6c, 1.11.14b). Here, the JSON was read by Rust and by the page that drew 02.
