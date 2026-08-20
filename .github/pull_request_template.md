## What

<!-- The group and the microsteps it lands. "Group 1.3 — the tax engine. Microsteps 1.3.1 → 1.3.7." -->

## Why now

<!-- One sentence. What was blocked without it. -->

## Invariants touched

<!-- conventions §1, by number, with what specifically touches them.
     "I-1 (all intermediate math in rust_decimal, one rounding), I-2 (exponent from Currency)."
     "None" is a legitimate answer for a docs or chore PR. -->

## Verification

<!-- Commands and their results — not "tests pass".
     - `cargo nextest run -p pos-domain tax::` — 14 tests
     - `prop_line_tax_sum_equals_receipt_tax` — 4096 cases
     - Manual: hand-checked a 5-line mixed-rate basket against ref/tax-jordan.md §3, to the fil. -->

## Test catalog

<!-- Which `E.n` rows this closes, and which it deliberately leaves open. -->

## Not in this PR

<!-- Scope that a reviewer will look for and not find, and where it is tracked instead. -->

---

- [ ] `just pre-push` green locally (lint · test · guards)
- [ ] I read my own diff, after a break — §4.8
- [ ] the manual `Done when` was run on a **fresh** database — §4.10
- [ ] no float in a money path; one rounding, at the boundary
- [ ] no PII or card value reachable by a log line, an `IpcError.detail`, or a test fixture
- [ ] permission checked in Rust, not only reflected in the UI
- [ ] the Postgres mirror of any SQLite migration, and a data-migration test if the shape moved
- [ ] the docs loop closed — §4.13 — and `just docs-links` passes
- [ ] base branch is `development` (only a promotion PR targets `staging` or `main`)
