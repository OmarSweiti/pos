# Conventions — the engineering law

Read once, then keep open. Every microstep in every phase file assumes these. Where a phase file and this file disagree, this file wins and the phase file is a bug.

---

## 1. The nine invariants

These are not style preferences. Each one, violated, produces a class of bug that costs money or a day of forensics.

**I-1 · Money is `i64` minor units. Always.**
No `f64` touches money, ever, in Rust, TypeScript, SQL, or JSON. Intermediate math (tax extraction, proration, percentages) happens in `rust_decimal`, rounds **once**, and returns to `i64`. A float in a money path is a rejected PR with no discussion.

**I-2 · The minor-unit exponent is per-currency data.**
JOD = 3 (1 dinar = 1000 fils). USD/EUR = 2. It is a column and a `Currency` field, never a constant, never `100`.

**I-3 · Quantities are `i64` milli-units.**
3 decimal places, same integer discipline as money. `1 unit = 1000`. Weighed goods (0.347 kg = `347`) and discrete goods (2 = `2000`) share one representation, so nothing branches on "is this weighed" in arithmetic.

**I-4 · Completed sales are immutable.**
No `UPDATE` on a `Complete` sale, ever. Corrections are new documents referencing the original. Enforced in the storage engine by the triggers in `0002_sale_integrity.sql` — which hold against a repository that has a bug in it, and against a hand-typed `sqlite3` session — and in the code by the absence of any repository method that could do it. `crates/pos-db/tests/sale_immutability.rs` holds both. Tender settlement and shift close are not exceptions: they append `tender_status_event` and `shift_close_event` facts, and current state is a rebuildable projection. Otherwise the register would update rows the server correctly protects with `REVOKE UPDATE`, leaving central reconciliation permanently stale.

**I-5 · Price and name are copied onto the sale line at capture time.**
Reports and refunds read `sale_line`, never `product`. A refund six months later uses the price the customer paid, automatically, because it was never anywhere else.

**I-6 · Stock is a ledger.**
Every quantity change is an append-only event with a kind and a reference document. On-hand is `SUM(qty_delta)`, cached in `stock_cache`, and **the cache must be rebuildable from the ledger by a command that Phase 1 microstep 1.10.3 wires into CI**. Ledger append, cache projection and the event-head watermark commit in one transaction; startup and periodic verification alarm and rebuild on mismatch. If the cache and the ledger can disagree until the next verification, the cache is a liability on the merchant's register.

**I-7 · Ordering comes from owned sequences, never from device clocks.**
Pull order is the server's `version`; push order is `(register_id, sync_outbox.seq)`. UUIDv7 supplies identity and index locality, not causal order. Registers drift and cashiers change the system time, so time-dependent rules use persisted `ClockState` and `effective_now` (§11), while document and delivery order never use the clock.

**I-8 · `pos-domain` is pure.**
No I/O, no SQLite, no Tauri, no network, no `std::time::SystemTime::now()`, no filesystem,
no randomness. Time and IDs are *arguments*. This is what makes it property-testable and
shareable with the server. `pos-domain/Cargo.toml` has no dependency that can perform I/O;
UUID generation features are disabled, and `scripts/check-domain-purity.py` audits the resolved
normal dependency graph and direct calls. Adding clock, random, or I/O capability is a design
review.

**I-9 · Every fact graph and its delivery envelope commit in one transaction.**
One business transaction commits its facts, one `sync_commit`, the complete `fact_commit_member` manifest and the corresponding `sync_outbox` delivery rows together. A sale without its manifest never syncs; a delivery row without its fact is a phantom; a partial manifest lets the server accept a header without its lines or tenders. One `BEGIN`, one `COMMIT`.

---

## 2. Naming

| Thing | Convention | Example |
|---|---|---|
| Money column | `*_minor` | `total_minor`, `unit_price_minor` |
| Quantity column | `*_milli` | `qty_milli`, `on_hand_milli` |
| Rate column | `*_ppm` (parts per million) | `rate_ppm` — 16% = `160_000` |
| Timestamp column | `*_at`, ISO-8601 UTC TEXT | `completed_at`, `opened_at` |
| Calendar day column | `*_date`, `YYYY-MM-DD` store-local | `business_date` |
| Boolean column | `is_*` / `has_*`, INTEGER 0/1 | `is_active`, `is_weighed` |
| Foreign key | `<table>_id`, BLOB(16) | `sale_id`, `product_id` |
| Enum column | TEXT + `CHECK (x IN (…))` | `status TEXT CHECK (status IN ('completed','voided'))` |
| Rust domain type | `PascalCase`, no `Pos` prefix | `Money`, `Cart`, `TaxCategory` |
| Rust error enum | `<Module>Error`, `thiserror` | `TaxError`, `CartError` |
| Tauri command | `snake_case` verb-first | `cart_add_line`, `sale_finalize` |
| TS type from Rust | identical name, generated | `Money`, `CartSnapshot` |
| Migration file | `NNNN_short_name.sql` | `0004_people_and_audit.sql` |
| Test (example) | `<subject>_<behaviour>` | `inclusive_16pct_extracts_exactly` |
| Test (property) | `prop_<invariant>` | `prop_line_tax_sum_equals_receipt_tax` |
| Golden file | `tests/golden/<name>.<ext>` | `tests/golden/receipt_ar_80mm.bin` |
| Capability string | `<noun>.<verb>` | `sale.void`, `price.override` |
| i18n key | `<screen>.<element>.<variant>` | `sale.action.park` |

**Rate as parts-per-million, not basis points.** The blueprint says basis points (`rate_bp`). Parts-per-million is used instead: Jordanian reduced rates include values like 1% and 2%, and future decrees are not guaranteed to land on whole basis points. `rate_ppm` costs nothing and removes a class of "we cannot represent 0.125%" conversation. 16% = `160_000`; 4% = `40_000`; 0% = `0`.

**Tax rounding is jurisdiction policy, not a merchant preference.** `HalfAwayFromZero` is only the provisional implementation vector. Microstep `1.3.4` blocks live provisioning and finalization until an approved policy records the source and hash that settle it; `2.7.0` rechecks that policy against the pinned fiscal package before fiscal work. A cashier or store setting cannot select `HalfEven`, floor or ceil and make two registers compute different tax. [`00-master-plan.md`](00-master-plan.md) §4a, “Errata and concordance”, records both source-plan overrides.

---

## 3. Repository law

`pos-domain` knows nothing about storage. `pos-db` knows nothing about business rules. The seam is explicit:

```
pos-db repository  →  returns owned domain types (Money, Qty, TaxCategory…)
                   →  never returns rusqlite::Row, never leaks rusqlite::Error
                   →  never computes a total, a tax, or a discount
pos-domain         →  takes those types, returns new ones
                   →  never opens a connection
terminal/src-tauri →  the only place that orchestrates: read → domain → write
```

Every repository is a struct holding `&Connection`, with methods returning `Result<T, DbError>`. Every write that produces a fact takes an explicit `&Transaction`, so the caller — never the repository — decides transaction boundaries. That is how I-9 stays true.

---

## 4. Errors

- **Domain errors** are `thiserror` enums, exhaustive, and carry the data needed to render a message. Never `String`. Never `anyhow` inside `pos-domain`.
- **Adapter errors** (`pos-db`, `pos-hardware`, `pos-fiscal`) are `thiserror` enums that wrap source errors with `#[from]`.
- **Shell errors** (Tauri commands, axum handlers) may use `anyhow` internally but serialize to a **typed** payload:
  ```rust
  #[derive(Serialize)]
  struct IpcError {
      code: &'static str,
      message_key: String,
      detail: Option<&'static str>,
      trace_id: Uuid,
  }
  ```
  `code` is what the UI branches on. `message_key` is what the UI translates. `detail` is a reviewed static explanation, never a database, PSP or fiscal error converted to text. The source error goes only to the separately scrubbed sink under `trace_id`, so a bind value cannot reach the webview or a screenshot.
- **`unwrap()` and `expect()` are banned outside tests and `main()`.** A panic in a register is a lost sale. `clippy::unwrap_used` and `clippy::expect_used` are denied at the workspace level from Phase 1 (microstep 1.0.3).

---

## 5. Testing

Nine layers. A microstep is not done until every applicable layer is green.

| Layer | Tool | Where | Rule |
|---|---|---|---|
| Example tests | `#[test]` | inline `mod tests` | Every branch of every rule that a human would argue about |
| **Property tests** | `proptest` | inline `mod tests` | Every invariant in §1 and every one in `test-catalog.md`. This is the layer that finds the bugs you did not imagine |
| Golden files | byte diff | `tests/golden/` | Receipts (per width, per language), fiscal XML documents. Regenerate deliberately, review the diff, commit |
| Integration | real SQLite / real Postgres | `crates/*/tests/` | Migrations run; repositories round-trip; transactions roll back on error |
| Concurrency | barriers or `loom` | beside the owning repository | Force the contested interleaving; never use sleeps or scheduler luck for sequence, audit or lease correctness |
| Fuzz | `cargo-fuzz` | `crates/*/fuzz/` | Every parser reachable from a scanner, renderer input or network; crashes become committed corpus regressions |
| Packaged app | WebdriverIO + `tauri-driver` | `apps/terminal/tests/e2e/` | Launch the artifact a merchant runs and cross the real IPC boundary; Playwright cannot drive a Tauri webview |
| Chaos | scripted | `crates/pos-sync/tests/` | Replay, drop, duplicate and reorder; prove `prop_server_facts_equal_the_union_of_register_outboxes`, `prop_reference_tables_converge_across_all_three_nodes` and `prop_apply_is_idempotent_under_any_replay_order` against the canonical semantic dump in [`ref/sync-protocol.md`](ref/sync-protocol.md), never storage bytes |
| Soak / long chaos | `cargo nextest run --profile soak` | `crates/*/tests/soak.rs`, `chaos.rs` | Excluded from default `just test`; nightly and phase-gate only, so the three-minute inner loop remains usable |

**Property tests are not optional and not a later phase.** `Money::split_evenly` already has one and it is the model: state the invariant in a comment in the words a human would use, then let `proptest` attack it.

**Determinism.** No test may read the wall clock, generate a random UUID outside `proptest`'s control, or depend on filesystem ordering. Time and IDs are injected. A flaky test in a money system is worse than no test — it trains you to ignore red.

### 5.1 Property-test configuration

`crates/pos-test-support/src/proptest.rs`, owned by prerequisite microstep `1.1.0`, holds the configuration once; individual properties do not choose a case count that makes themselves convenient:

```rust
pub fn domain_proptest_config() -> ProptestConfig; // default cases = 4_096
pub fn io_proptest_config() -> ProptestConfig;     // default cases = 256
```

- Every property names its `Strategy` — for example `terminal_event_sequences()` — beside the property, with a comment stating the input space it covers and what it deliberately excludes. The generator is part of the proof; an anonymous tuple of ranges is not reviewable evidence against a double charge.
- Default and pull-request runs use a repository-recorded deterministic seed. Every failure prints the seed, persists the minimized case, and commits it under `proptest-regressions/`. A scheduled higher-count run may use another seed only when the log records it well enough to replay; a failed scheduled seed becomes a committed regression before the fix merges.
- The shared counts are defaults. When `PROPTEST_CASES` is present, the helper applies it after selecting the crate default and refuses an invalid value or one below that default; a local helper assignment may not shadow the environment override. `pos-domain` runs a scheduled `PROPTEST_CASES=100000` lane, with `scheduled_case_override_is_not_shadowed_by_shared_default` proving that its effective configuration is exactly 100,000 cases. I/O-bound crates stay at the lower shared default because a slow database property that nobody runs protects nothing.
  - **The helper is not the last writer, and the refusal is what makes this rule true.** `proptest!` calls `contextualize_config` on whatever configuration it is handed, so `PROPTEST_CASES` is applied a second time *after* the helper returns — the macro writes `cases` last and would obey a lowering value. Two things close that: the helper's parser agrees with proptest's own by construction (plain `u32::FromStr`, no trimming), so for every value proptest accepts the second pass computes the same number and is a no-op; and for every value proptest would merely *warn about and ignore*, and for every value below the crate default, the helper panics from inside the `#![proptest_config(...)]` expression before the macro runs at all. A warning that keeps the old default is the failure this exists to prevent: a lane whose log says 100,000 and whose run was 4,096 has a coverage claim nobody can check. The naive spelling is also the bug — `ProptestConfig { cases: 4_096, ..ProptestConfig::default() }` overwrites the override with the crate default, because `default()` has already read the environment.
- A bounded universal claim uses an exhaustive `#[test]` loop when that loop is feasible. A few hundred generated gross amounts cannot prove “every gross from 1 through 1,000,000.”
- Wall-clock assertions are forbidden inside `proptest!` and ordinary unit tests. Performance belongs to the failing benchmark gate in §7 and never uses the `prop_` prefix. Concurrency tests force an interleaving with a barrier or `loom`; they do not sleep and hope.
- `just test` must finish in under three minutes on the reference register. A test that pushes the gate over that budget belongs in the soak profile, which is selected explicitly and may never report a silent skip.

---

## 6. Definition of done

A microstep is done when **all** of these hold. Not most.

1. The named files exist with the named items.
2. The named tests exist, are named exactly as specified, and pass.
3. `just lint` is clean — `cargo fmt --check`, `clippy -D warnings`, `biome ci`.
4. `just test` is clean.
5. The step's **Done when** line is objectively true, checked by running its command.
6. Nothing outside the step's `Files:` list changed, except imports and module declarations.
7. It is committed with the step number in the message (§8).

---

## 7. Performance budgets — measured, not asserted

| Budget | Absolute limit | Measurement | Samples | Measured on | Added in |
|---|---|---|---|---|---|
| Scan → line visible | < 100 ms | packaged-app WebDriver trace through the hardware simulator | ≥ 50 scans after warm-up | reference register | 1.11.13 |
| Cart total recompute | < 16 ms | `criterion`, 200-line cart | 50 measured samples after warm-up | reference register | 1.4.9 |
| Search-as-you-type, 50k SKUs | < 50 ms | `criterion` over the seeded fixture | 50 measured samples after warm-up | reference register | 1.2.7 |
| Cold start → sellable | < 3 s | packaged-app WebDriver timer | median of 10 clean launches | reference register | 2.9.3 / 2.9.5 |
| PIN verification | target 250 ms; median 200–350 ms and p99 < 500 ms | `criterion` `pin_verify` | 50 measured samples after warm-up | reference register | 1.6.2 |

### 7.1 Benchmark methodology

The **reference register** is the lowest register-hardware row in the supported-device matrix in [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6a — mechanically, the first row of §6a.1's table, which is ordered lowest-capability first. Its CPU, RAM, storage, OS version, power mode, device-matrix identity and release-build profile are mirrored in `benchmarks/reference-register.toml`; no baseline is accepted while either record is blank or they disagree. **Both records are blank today**, because no register has been bought and §6a refuses to invent one, so `python3 scripts/bench-gate.py --check-profile` exits non-zero and every budget below is unmeasurable rather than unenforced. Filling the pair is the deferred half of microstep 1.2.0. Each run records median, p99 and median absolute deviation. The absolute limit applies to p99 except cold start, whose table entry is explicitly a median, and PIN verification, whose row carries both a median security/UX band and a p99 ceiling. Cross-OS packaged-app launch coverage belongs to 2.9.5; it is not presented as a latency measurement on hardware that does not run that OS.

Committed baselines live under `benchmarks/baselines/*.json`, each carrying its three statistics as integer nanoseconds and the reference-register identity the numbers were taken on; the schema is [`../../benchmarks/baselines/README.md`](../../benchmarks/baselines/README.md). `just bench-gate [budget]` accepts the exact slugs `search`, `price-cart`, `pin-verify`, `scan-to-line` and, once Phase 2 adds it, `cold-start`; omitting the argument runs every budget implemented at that gate, and **while that set is empty it refuses rather than exiting zero** — a green `just bench-gate` in a release checklist reads as "budgets met". A budget is implemented exactly when it has a committed baseline. It exits non-zero when an absolute limit is exceeded, or when the median is more than 20% slower **and** more than three baseline median absolute deviations slower. A noisy run is investigated, not blessed. Updating a baseline requires a `perf(...)` change with before/after measurements and the reason, because moving the baseline without explaining the slower till deletes the budget — and a baseline outside its own absolute limit is refused outright, so a red gate cannot be repaired by republishing the slow number.

`cargo bench` reporting a number is not a gate. Microstep 1.2.0 owns the recipe and `scripts/bench-gate.py`, which exist and refuse; 1.12.3 owns the future live CI measurement job that runs `just bench-gate` only on `runs-on: [self-hosted, reference-register]`, and no such job exists in the current tree. Hosted runners exercise the threshold parser against fixed pass/fail fixtures through `--fixture-root` and are refused both the live comparison and `--publish-baseline`; detection is environment-based, and `RUNNER_ENVIRONMENT=self-hosted` is the one exception, which is an accident control rather than an authentication boundary. Phase and release gates repeat the live command on that physical register after those microsteps land. A budget without a command that exits non-zero is a wish.

---

## 8. Commits

```
<type>(<scope>): <summary>            [<step>]

feat(domain): tax engine, inclusive + exclusive extraction   [1.3.4]
fix(db): sale_line qty to milli-units                        [1.1.7]
test(fiscal): allowance recap conservation property         [2.7.3]
docs(impl): phase 2 fiscal conformance harness               [—]
```

`type` ∈ `feat` `fix` `test` `docs` `chore` `refactor` `perf`. `scope` is the crate or app short name — `domain`, `db`, `sync`, `hardware`, `fiscal`, `terminal`, `server`, `backoffice` — plus `repo` for the workspace itself (gates, CI, tooling) and `impl` for the implementation doc set. The list is closed: `.githooks/commit-msg` refuses anything else, and so does the `branch-flow` check on a pull-request title. One microstep, one commit, wherever possible — a bisect that lands on a microstep tells you exactly what broke.

`step` is exactly one microstep (`1.3.4`), an inclusive microstep range joined by an en dash
(`1.3.4–1.3.6`), or an em dash (`—`) for repository work outside the implementation plan.
`scripts/validate-change-title.sh` is the shared parser used by Git and GitHub; do not maintain a
second regular expression in a workflow.

**This whole format is a law, not a preference** — [`.githooks/commit-msg`](../../.githooks/commit-msg)
refuses a subject that breaks it, and CI's `branch-flow` check refuses a pull-request title that
does, because a squash-merge commits the title. Two more rules live in the same hook: the summary is
≤ 72 characters before the step tag with no trailing period, and **coding assistants remain tools,
not co-authors** — no AI `Co-Authored-By` trailer and no generated-by line. There is one narrow
history-compatibility exception: the exact Dependabot author name/email may retain the exact
Dependabot trailer, and its title still follows the same grammar with `[—]`. Those strings are
spoofable Git metadata, not proof of authenticated GitHub App provenance. Human co-authors are never
refused because of a person's display name. The same attribution policy runs in `commit-msg`,
`pre-push`, and CI so local commits, pull-request bodies, and GitHub-created squash commits agree.

Branch per group (`phase-1/group-3-tax`), **from `development`**. The flow is
`feature → development → staging → main`: a work PR is **squash-merged** into `development` and
its *title* becomes the commit; a promotion PR (`development → staging`, `staging → main`) is
merged with a **merge commit**, because squashing one forks the branches permanently.
`development` is always green; `staging` is a tagged release candidate; `main` is what a merchant
is running. The model and its enforcement are
[`03-github-workflow.md`](03-github-workflow.md).

---

## 9. Migrations

**Forward-only. No down migrations.**

The blueprint says migrations should be "tested up *and* down in CI." The shipped runner (`crates/pos-db/src/lib.rs`) is a `PRAGMA user_version` counter with no down path, and that is the right choice — down migrations in a system whose whole premise is that financial facts are never destroyed are a liability that rots unmaintained.

The operational recovery path is precise because “install the old binary” does not work after a schema change: the older binary correctly refuses the database with `SchemaTooNew`. Before the first migration of an update, take and verify an encrypted snapshot. A failure before migration may restore the previous application bundle. Once any register migrates, halt the rollout and fix forward. Restoring the previous bundle **and** its pre-migration snapshot is permitted only before that register writes a new fact; afterwards it would delete real sales, so fix-forward is mandatory. Any “one-click rollback” claim elsewhere is a bug against this section.

Rules:

1. `crates/pos-db/migrations/NNNN_name.sql`, appended to the `MIGRATIONS` array in order.
   `./scripts/verify-schema.py` requires exact ordered parity between that runtime array and every
   migration on disk. **Never edit a committed migration.** Not to fix a typo. Not "it hasn't
   shipped yet." Every entry in either migration tree must be a repository-owned regular SQL
   file; symlinks, gitlinks, devices, and other filesystem indirection are forbidden.
2. Every migration is idempotent under the runner (the runner guarantees each runs once; the SQL must not assume more).
3. A migration that changes the shape of existing data ships with a data migration in the same file and a test that seeds the old shape, migrates, and asserts the new one.
4. Postgres mirrors SQLite in `apps/server/migrations/` via sqlx, **same semantics**. The numbers cannot match — sqlx names files `<14-digit UTC timestamp>_<lower_snake>.sql`, with unique, strictly increasing versions — so the mapping is *declared*, not inferred: every mirror opens with `-- Mirrors SQLite NNNN_name.sql` or `-- Server-only: <why>`, and `./scripts/verify-pg-migrations.py` checks filenames and mapping both ways before applying the mirror to a real PostgreSQL server. SQLx runs a file transactionally unless its bytes begin exactly, case-sensitively, with `-- no-transaction`; that escape is only for statements PostgreSQL forbids inside a transaction and requires an explicit partial-failure recovery test or procedure. The name may differ where the server's half of the work differs. A register-local entity gets no mirror at all — record it in `REGISTER_LOCAL` in that script rather than committing an empty file. Undeclared divergence is a sync bug waiting.
5. The app **refuses to start on a half-migrated database** (E.58) and says so — it does not guess.

The recovery contract is held by `schema_from_a_newer_build_is_refused`, `half_migrated_db_refuses_to_open_with_a_named_error`, `a_failed_update_before_migration_restores_the_previous_bundle`, and `a_post_migration_failure_restores_the_pre_update_snapshot_or_rolls_forward`. These tests exist because a failed update at 07:55 must produce a named recovery action, not a cashier repeatedly relaunching a half-migrated register.

---

## 10. Internationalisation (G-5)

Arabic is not a translation of this product. It is the product; English is the toggle.

- **Direction:** the app is RTL by default. `<html dir="rtl" lang="ar">`; the English toggle flips `dir`/`lang` only. Every layout uses **CSS logical properties** — `margin-inline-start`, not `margin-left`; `inset-inline-end`, not `right`. Tailwind's `ps-*`/`pe-*`/`ms-*`/`me-*`/`start-*`/`end-*` throughout; `pl-*`/`left-*` is a lint failure — `./scripts/check-logical-css.sh`, in `just lint` and CI's `web` job. Biome's recommended preset knows nothing about Tailwind utilities or CSS sides, so until that script existed this rule was written down and unenforced. A case that really is physical carries `physical-ok: <reason>` on the line.
- **Numerals:** Western Arabic digits (0–9) everywhere. That is Jordanian retail practice; Eastern Arabic-Indic digits on a receipt confuse more than they serve.
- **Catalog:** a typed message catalog, keys as §2, `ar` and `en` files kept in lockstep by a test that fails when a key exists in one and not the other. The catalog is the single source for UI strings; a string literal in a component is a lint failure.
- **Money and dates render through one function.** `formatMoney(minor, currency, locale)` and `formatDate(iso, tz, locale)` — never `toLocaleString` scattered inline. Totals, tax, tenders, change, rounding adjustments, receipts and fiscal views render at the currency exponent. A shorter catalogue display is separate and permitted only when exact; hiding fils that are later charged is a price-display defect.
- **Font:** one family covering Arabic and Latin, embeddable, shipped with the app — no network font. The same font file feeds the receipt rasteriser, so the receipt looks like the screen. Chosen in microstep 1.7.2.
- **Product names carry both `name_ar` and `name_en`.** The receipt prints per store setting; search matches both.

---

## 11. Time (G-4)

- **Storage:** UTC, ISO-8601 with milliseconds, `TEXT`. `strftime('%Y-%m-%dT%H:%M:%fZ','now')` is already the schema default.
- **Zone:** the store keeps the IANA zone id `Asia/Amman`, never a fixed offset or a hand-written DST rule. The shell resolves the offset for each instant from shipped tzdata and passes only that value into pure `pos-domain`. Jordan has used UTC+3 year-round since 2022; carrying the superseded seasonal rule would move winter sales across the 04:00 business-day boundary.
- **Clock confidence:** persisted `ClockState` records the trusted anchor, boot-monotonic projection, high-water timestamp and anomaly. Business-date and effective-tax decisions use `effective_now`; `Suspect` or `Untrusted` time alarms and requires the specified audited operator confirmation, but a clock fault does not refuse a sale.
- **Business date** is *not* derived from wall-clock midnight. It is:

  ```
  business_date(sale) = the business_date of its shift
  business_date(shift) = local calendar date of shift.opened_at,
                         unless opened_at is before the store's `day_cutover_time`,
                         in which case the previous local date
  ```

  A shift opened at 00:30 belongs to yesterday's trading day. `day_cutover_time` defaults to `04:00` local and is a store setting. Z reports close a *shift*, so a Z belongs to the shift's business date, not the wall clock's (E.7).
- **Monotonic guard:** persist the last observed timestamp. On a backward jump, log an audit entry and keep issuing non-decreasing timestamps until wall-clock catches up. Document numbers and outbox order are counters and never derived from time (E.6).

---

## 12. Security posture in one page

Full treatment in [`ref/security-compliance.md`](ref/security-compliance.md). The rules you must not break without reading it:

- **Never-log rules are executable, not copied prose.** One `SENSITIVE_FIELD_RULES` registry carries exact-name, suffix and contains rules. The tracing layer, `scrubber_redacts_every_known_pii_field`, `scrubber_redacts_every_suffix_rule`, the audit-payload guard, diagnostic bundle and telemetry transport all derive from it. Adding a sensitive field in one place without covering every sink must fail CI (G-8).
- **Never store:** anything from a card except the PSP reference, the masked PAN the terminal returns for the receipt, and the scheme.
- **Permissions are checked in Rust**, in the command handler, via the guard in §5 of the security doc. Hiding a button is UX. The check is security.
- **The plaintext DB data key lives in the OS credential store.** Never a plaintext file, never an env var in production. Provisioning issues and displays the merchant-held recovery code once. A wrapped data-key envelope is stored beside **every** backup and, from Phase 3, in the organisation record; `restore` unwraps it with that code before the database or a user session exists. `POS_DB_KEY` exists for CI and dev only and is ignored in release, where credential-store lookup continues. Ignore-and-continue is deliberate: a stray inherited variable must neither supply the production key nor stop the till opening.
- **Escalation is bound, not merely recorded.** A one-use `ApprovalHandle` carries exactly `{ id, capability, actor, approver, entity_id, amount_minor, content_hash: Option<PreparedIntentHash>, reason, issued_at, expires_at, nonce }`. `actor != approver` on every handle path; `ban_self_approval` decides whether an operation requires escalation at all and never permits self-issued handles. A privileged effect with no money value binds `amount_minor = 0`; zero is an exact value, never a wildcard. A prepared non-money effect also binds the BLAKE3 hash of its versioned canonical intent, and the commit recomputes it before consuming the handle, because an unchanged row id does not prove the manager approved unchanged fields. The handle remains immutable audit evidence, while its `approval_consumption` fact commits in the same transaction as the financial effect and audit row, so a restart cannot replay approval for a different sale, a larger amount or altered prepared content (E.52, E.86).

---

## 13. Tauri IPC contract

- Commands are the **only** channel from UI to core. No `fs`, no `shell`, no `http` plugin exposed to the webview; the capability file grants nothing beyond what a command needs.
- Every command: `snake_case`, verb-first, returns `Result<T, IpcError>`, and declares its required capability in the registry (see [`ref/ipc-contract.md`](ref/ipc-contract.md)). A command with no capability declaration fails the exhaustiveness test.
- **No base sale command accepts a price.** `cart_add_line` is `{ product_id, qty_milli? }`; price-embedded labels reach `cart_add_scan` as typed `ScanLookup::PriceEmbedded { hit, price: PriceSource, derived }`, whose `#[non_exhaustive]` variant and private label constructor are available only to the pure domain scan handler after the shell resolves the parsed item code. Price-bearing command arguments exist only on three controlled entries: audited `cart_override_price` under `price.override`; capped, audited `cart_add_department_sale` under `sale.department`; and inert `product_quick_add_prepare`, which content-hashes a proposed catalogue row but creates neither a product nor a cart line until `product_quick_add { product_id, approval_id }` consumes a matching approval. At 1.6.7, `no_command_argument_carries_a_price` walks the registry and refuses every price field outside those three entries, so a base `sale.create` friend-price path fails the gate; the current tree does not yet contain that future registry.
- **Compile-time and runtime authorisation both apply.** `Authorized<C>` proves the capability inside Rust. Each registry entry declares `ApprovalRequirement::Never`, `Always { binding }`, or `Conditional { predicate, binding }`. An always-privileged command accepts `approval_id`; a conditionally privileged command accepts `approval_id?` but must reject a missing handle whenever its named predicate is true. Every privileged execution resolves the persisted `ApprovalHandle`, validates its entity, amount, reason and optional prepared-content binding, and inserts `approval_consumption` in the effect-and-audit transaction. `every_privileged_command_binds_its_approval` and `conditional_privilege_cannot_cross_threshold_without_approval` make an omitted or post-commit approval check impossible to overlook.
- **TS types are generated from Rust, not hand-written.** `ts-rs` owns `packages/api-types/src/ipc/`; Phase 3 OpenAPI owns `packages/api-types/src/http/`. A DTO crossing both boundaries is generated once from Rust and re-exported by HTTP, never emitted twice. The owning frontend and Phase-3 microsteps must make CI fail on drift and on `no_type_name_is_emitted_by_both_generators`; the current tree has neither generator gate, so prose is not credited as enforcement. Two generated copies of `Money` can disagree as surely as two hand-written ones.
- Long operations (card collection, printing, fiscal submission) are commands that return immediately with a handle and emit **events** for progress. A cashier watching a spinner with no state is a cashier who presses the button again.

---

## 14. What "sellable" means at each gate

Written down because it is the only question that matters when a phase runs long:

| After | A real store could… |
|---|---|
| Phase 0 | …nothing. It compiles and ships. |
| **Phase 1** | …**open a shift and sell for cash, all day, fully offline, in Arabic, with correct GST and a printed receipt.** |
| Phase 2 | …take cards, handle returns, run blind close and Z reports, and produce fiscal documents against the package pinned at `2.7.0`, passing every non-provisional check short of the credentialed ISTD endpoint. |
| Phase 3 | …run more than one register, administer from a back office, and keep customers; fiscal transport still uses the pinned-spec harness and mock. |
| Phase 4 | …run three stores with promotions, receiving, counts, transfers, and a full report suite under the evidenced pilot fiscal posture; this row does not authorize live ISTD submissions. |
| Phase 5 | …be sold to someone who is not you. |

Real ISTD contact occurs only under [`phase-5-harden-and-launch.md`](phase-5-harden-and-launch.md) milestone `5.2`, using its written certification procedure, production credentials and the merchant's informed consent. Every submission there is a live fiscal document against the merchant's tax record, which is why an earlier gate cannot use it as a connectivity test.

If a phase is running long, cut scope toward the next row of this table, never away from it.
