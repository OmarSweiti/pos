**Runtime change: none.** This PR changes the plan's documents only. The screenshots are the spike's measured evidence, the reason the plan changes.

**The spike (20 October 2026).** Real Arabic receipts were rendered through this repository's pipeline (1.7.1–1.7.4) by a throwaway program outside the repository. Each was sent over TCP 9100 to third-party emulators, each running in a throwaway Docker container bound to `127.0.0.1` and removed afterwards. Synthetic fixture data only.

| Sent to emupos 0.4.0 (as an Epson TM-T20III) | Its render vs ours |
|---|---|
| 80 mm receipt, one `GS v 0` (576 × 1238) | **identical**: 0 of 713 088 dots differ |
| 80 mm receipt, ≤ 900-row `GS ( L` store-and-print strips | **identical** |
| 60-line basket, ≤ 2303-row `GS v 0` strips (576 × 5450) | **identical**, no visible seam |
| 60-line basket, one 5450-row `GS v 0` | identical: emupos **does not enforce** the TM-T20III's 2303-row limit |
| 58 mm receipt on the 80 mm profile | printed on a 576-dot page: **no width check** |

| emupos status and faults | Reply | Matches Epson's definitions |
|---|---|---|
| Ready | `DLE EOT` 1–4 → `0x12` | ✅ |
| Paper out | `DLE EOT 1` offline bit; `DLE EOT 2` paper-end stop; `DLE EOT 4` → `0x7E`; a job sent then is held, and completes on reload | ✅ |
| Cover open | `DLE EOT 2` cover bit | ✅ |
| `ESC p` | drawer opens; `DLE EOT 1` drawer bit set | ✅ |

**escpos-netprinter 3.2** rendered a tiny 8 × 1 raster, but **failed** on our real 80 mm receipt (`esc2html` exit 255, *"zero-byte-long argument specified"*). It is rejected.

**What the screenshots show:** 01 is our own render; 02–04 are emupos's renders of the same bytes in three framings, dot-identical to ours.

**Docs checks on this head:** `check-implementation-frontier.py` reconciles (Phase 1: 121) and its `--self-test` passes; `check-test-catalog.py` reconciles 95 cases (self-test 27/27); `check-doc-links.sh` passes (54 files); `just pre-push` is green.

**Not validated here, by design:** nothing about a physical device. That is milestone 5.8's job.
