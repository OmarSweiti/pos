# The embedded typeface

Microstep `1.7.2`, gap G-5. **IBM Plex Sans Arabic**, Regular and Bold.

## Why these files exist in the repository

`ref/hardware-and-receipts.md:110` requires the receipt font to be *"embedded in the app"* and to be
**the same file the UI uses**, so that a receipt looks like the screen and a discrepancy between the
two is visible rather than plausible. A network font cannot satisfy either half: a register trades
offline by design, and a font fetched at runtime is not the file the rasteriser embedded.

So the bytes are committed, and `crates/pos-hardware/src/font.rs` is the single place that reads
them. `1.7.3` builds the rasteriser on the same constants; `1.11.1` wires the UI to them and owns
the test that the two paths agree.

## Provenance

| | |
|---|---|
| Family | IBM Plex Sans Arabic |
| Faces | `Regular` (236,708 bytes), `Bold` (247,896 bytes) |
| Format | TrueType (`sfnt` version `0x00010000`) |
| Source | <https://github.com/IBM/plex>, release `@ibm/plex-sans-arabic@1.0.0` |
| Path in the archive | `ibm-plex-sans-arabic/fonts/complete/ttf/` |
| Licence | SIL Open Font License 1.1 — `LICENSE.txt`, from the archive root |

Chosen on 20 September 2026 from the three candidates
[`phase-1-sellable-mvp.md`](../../docs/implementation/phase-1-sellable-mvp.md) § 1.7.2 names: Noto
Sans Arabic, IBM Plex Sans Arabic, Cairo. All three cover Arabic and Latin and all three are OFL;
the choice between them is a product one and was the operator's.

## What the licence permits, and the one clause that binds a later microstep

OFL 1.1 permits embedding in and redistribution with an application, **provided the licence travels
with the font**. That is why `LICENSE.txt` sits beside the faces rather than only in a dependency
manifest, and it is what `embedded_font_has_a_repository_licence` asserts.

**The licence text is unchanged; its line endings are not.** IBM ships it with CRLF and
`.gitattributes`' `* text=auto` normalises it to LF on commit — 4,456 bytes became 4,363, which is
exactly one byte per line across 93 lines. The wording is byte-for-byte what IBM published, so the
OFL is satisfied, but "verbatim" would be the wrong word for the file and this says so instead. The
two `.ttf` faces are binary to Git and are committed unchanged at 236,708 and 247,896 bytes — which
is worth knowing, because a font silently normalised would not be a font.

The header reserves the font name: *"Copyright © 2017 IBM Corp. with Reserved Font Name 'Plex'"*.
OFL 1.1 §3 forbids redistributing a **modified** version under a reserved name. Nothing here is
modified, so the clause is inert today — but it is the one that binds a later step that **subsets**
the file to shrink the binary, which `1.7.3` might reasonably want. A subset is a modified version:
rename it, or do not ship it.

## Replacing or adding a face

Two constraints, both measured rather than assumed:

* `scripts/check-staged-policy.py:11` refuses a staged blob over **2,000,000 bytes**. The two static
  TTF faces are far inside that; the full family archive and the variable-font build are not
  necessarily, so add a *face*, not a family.
* `ref/hardware-and-receipts.md:135` makes a font-file bump **its own pull request**, because a
  font change produces a golden-receipt byte diff indistinguishable from a shaping regression, and
  `UPDATE_GOLDEN=1` would otherwise be the only available response to a red test.
