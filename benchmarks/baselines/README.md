# Committed performance baselines

One JSON file per budget, named for its slug: `search.json`, `price-cart.json`,
`pin-verify.json`, `scan-to-line.json`, and from Phase 2 `cold-start.json`.
[`../../docs/implementation/01-conventions.md`](../../docs/implementation/01-conventions.md) §7 is
the budget table and §7.1 is the methodology; `just bench-gate` is the command that reads these
files and exits non-zero.

**This directory is empty of baselines, and that is the current honest state.** A baseline is what
makes a budget enforceable, and it can only be recorded on the reference register — which
[`../../docs/implementation/ref/hardware-and-receipts.md`](../../docs/implementation/ref/hardware-and-receipts.md)
§6a.1 does not yet have a row for. Microsteps `1.2.7` (search), `1.4.9` (cart total), `1.6.2` (PIN
verify) and `1.11.13` (scan to line) add the benchmarks and their first baselines; `1.12.3` adds the
live measurement job. Until a slug has a file here, **it is not implemented at this gate** and
`just bench-gate <slug>` says so and exits non-zero rather than reporting a pass.

## A baseline is the record of one machine, on one build, at one commit

```json
{
  "budget": "search",
  "samples": 50,
  "median_ns": 31200000,
  "p99_ns": 44800000,
  "mad_ns": 1400000,
  "taken_at": "2026-09-01T10:15:00+03:00",
  "taken_by": "who ran it",
  "commit": "the 40-hex commit the measurement was taken at",
  "reason": "why this number is the baseline",
  "profile_identity": {
    "profile_id": "", "maker": "", "model": "", "cpu": "", "ram": "",
    "storage": "", "os_version": "", "power_mode": "", "release_profile": "",
    "qualified_at": "", "qualified_by": "", "qualifying_commit": ""
  }
}
```

Every key is required and an unknown key is refused: a mistyped `median_ns` that was silently
ignored would be a baseline with no median at all. A fresh measurement uses the same shape without
`reason`, and lives in `benchmarks/measurements/<slug>.json`, which is machine-local output and is
not committed — a committed measurement would be a baseline nobody reviewed.

**Durations are integer nanoseconds.** Not floats, and not milliseconds with a decimal point. The
gate's whole verdict is integer arithmetic, so `median_ns * 100 > baseline * 120` either holds or it
does not, and the number printed beside a verdict cannot disagree with it.

**`profile_identity` is the twelve fields of `benchmarks/reference-register.toml`, copied.** The gate
refuses a baseline whose identity disagrees with that file: a number with no machine attached is not
evidence, and a laptop's median compared against the register's baseline is a fiction in both
directions.

## Moving a baseline is a commit, and it needs a reason

§7.1: "Updating a baseline requires a `perf(...)` change with before/after measurements and the
reason, because moving the baseline without explaining the slower till deletes the budget."

`python3 scripts/bench-gate.py --publish-baseline=<slug> --reason='...'` writes the file, prints the
before and after numbers for the commit body, and refuses without a reason. It also refuses on a
hosted runner and from a `--fixture-root` run: §7.1 lets a hosted runner exercise fixed pass/fail
fixtures and never lets one produce or bless a baseline.
