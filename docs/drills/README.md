# Drills — the index

**A drill produces a record or it did not happen.** This is the directory
[`02-development-workflow.md`](../implementation/02-development-workflow.md) §5.10 specifies: every
run is a dated file here, `YYYY-MM-DD-<drill>.md`, and a row in the table below. The drills that gate
a phase read their evidence from this directory, and so does the release checklist. *"Did the
hardware lab run for `v0.4.0`?"* is a question about a file, not about a memory.

The same record covers a **review done by a person**, such as the native reader's sign-off `1.7.5`
requires for every changed Arabic receipt golden and the cashier-guide review `1.11.14` requires. A
hexdump cannot show a lost medial form, and only a named person looking at the output can.

## The record

Each run's file carries these fields, in this order. They are
[`05-drill-result.yml`](../../.github/ISSUE_TEMPLATE/05-drill-result.yml)'s fields too, so a result
can be filed from a phone in the lab and transcribed here afterwards.

| Field | What it holds |
|---|---|
| **Drill** | its name and case number from [`ref/test-catalog.md`](../implementation/ref/test-catalog.md), or the microstep whose `Done when` requires the review |
| **Ran against** | the exact commit SHA or tag. A branch name is not evidence |
| **Hardware** | the machine and OS, and for a register drill its row in [`ref/hardware-and-receipts.md`](../implementation/ref/hardware-and-receipts.md) §6a |
| **Operator** | the person who ran it. Phase 5 requires someone who did not write the code |
| **Started, ended, elapsed** | wall-clock times, and the difference |
| **Outcome** | passed, failed or could not complete, judged against the drill's *Must happen* column in §5.10 |
| **Surprises** | anything unexpected, and the case number or issue each one became. *None* is an answer |

A record is written once, like any other evidence here. A later run is a new file, and a correction
is a new file that names the one it corrects.

## Runs

| Date | Drill | Ran against | Operator | Outcome | Record |
|---|---|---|---|---|---|
| — | *No drill has run yet.* | | | | |

The directory was created on 24 September 2026, ahead of the first drill (#241), so that the first
run has an agreed shape to land in rather than inventing one.
