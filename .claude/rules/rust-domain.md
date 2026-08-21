---
paths: ["crates/pos-domain/**/*.rs", "crates/pos-domain/Cargo.toml"]
---

# `pos-domain` is pure (I-8)

The crown jewel. Purity is what makes it property-testable and shareable with the server.
Signatures live in `docs/implementation/ref/domain-api.md` — that file is normative.

- **No I/O of any kind.** No SQLite, no Tauri, no network, no filesystem, no
  `SystemTime::now()`, no randomness. Time and IDs are **arguments**, passed in by the shell.
- **Adding a dependency to `crates/pos-domain/Cargo.toml` that can perform I/O is a design
  review**, not an edit. Say so before you add it.
- **No `anyhow` in this crate.** Errors are exhaustive `thiserror` enums named `<Module>Error`,
  carrying the data the UI needs to render a message. Never `String`.
- **No float touches money (I-1).** Intermediate math in `rust_decimal`, round **once**, return
  `i64` minor units. The minor-unit exponent is per-currency data (I-2) — never `100`.
  Quantities are `i64` milli-units (I-3).
- **Every business rule ships with a property test named `prop_<invariant>`**, with the
  invariant restated in a comment in the words a human would use. `Money::split_evenly` is the
  model. **The prefix is load-bearing, not decorative:**
  `docs/implementation/ref/domain-api.md` is normative here and names all thirty-one property
  tests with it, and microstep 1.1.5 verifies the suite with the *filter*
  `cargo nextest run -p pos-domain money::tests::prop_` — which matches nothing, and reports
  success having run nothing, the moment a name drops the prefix.
  **Enforced** by `./scripts/check-prop-test-names.py`, in `just lint`.
  Example tests are `<subject>_<behaviour>` and must **not** carry the prefix — a `prop_`-named
  example test makes that same filter match something that is not a property test.
- **If the implementation must deviate from `docs/implementation/ref/domain-api.md`, fix the
  doc in the same commit.** Silent divergence turns the reference into a liability.
- **The module graph stays acyclic** (`docs/implementation/ref/domain-api.md` §15).
  `just acyclic` enforces it.
