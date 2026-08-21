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
- **Every business rule ships with a property test.** Name it for the invariant it holds, in
  the words a human would use — `split_preserves_total`, `add_sub_roundtrip` — with the
  invariant restated in a comment. `Money::split_evenly` is the model. A `prop_` prefix is
  not used: these live in a `proptest!` block, which already says what they are.
  Example tests are `<subject>_<behaviour>`.
- **If the implementation must deviate from `docs/implementation/ref/domain-api.md`, fix the
  doc in the same commit.** Silent divergence turns the reference into a liability.
- **The module graph stays acyclic** (`docs/implementation/ref/domain-api.md` §15).
  `just acyclic` enforces it.
