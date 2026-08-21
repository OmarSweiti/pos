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
No `UPDATE` on a `Complete` sale, ever. Corrections are new documents referencing the original. Enforced in the storage engine by the triggers in `0002_sale_integrity.sql` — which hold against a repository that has a bug in it, and against a hand-typed `sqlite3` session — and in the code by the absence of any repository method that could do it. `crates/pos-db/tests/sale_immutability.rs` holds both. The one deliberate exception is a tender's settlement columns: a semi-integrated card capture confirms after the sale closes, so `tender_state`/`captured_at` stay writable while the amount does not.

**I-5 · Price and name are copied onto the sale line at capture time.**
Reports and refunds read `sale_line`, never `product`. A refund six months later uses the price the customer paid, automatically, because it was never anywhere else.

**I-6 · Stock is a ledger.**
Every quantity change is an append-only event with a kind and a reference document. On-hand is `SUM(qty_delta)`, cached in `stock_cache`, and **the cache must be rebuildable from the ledger by a command that CI runs**. If the cache and the ledger can disagree without a test noticing, the cache is a liability.

**I-7 · Ordering comes from server versions and UUIDv7, never from device clocks.**
Registers drift; cashiers change the system time. Record device time for humans to read. Never branch on it.

**I-8 · `pos-domain` is pure.**
No I/O, no SQLite, no Tauri, no network, no `std::time::SystemTime::now()`, no filesystem, no randomness. Time and IDs are *arguments*. This is what makes it property-testable and shareable with the server. `pos-domain/Cargo.toml` has no dependency that can perform I/O, and adding one is a design review.

**I-9 · Every fact write and its outbox row commit in one transaction.**
A sale that exists without its outbox row is a sale that never syncs. A outbox row without its sale is a phantom. One `BEGIN`, one `COMMIT`.

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
| Migration file | `NNNN_short_name.sql` | `0004_shifts_and_cash.sql` |
| Test (example) | `<subject>_<behaviour>` | `inclusive_16pct_extracts_exactly` |
| Test (property) | `prop_<invariant>` | `prop_line_tax_sum_equals_receipt_tax` |
| Golden file | `tests/golden/<name>.<ext>` | `tests/golden/receipt_ar_80mm.bin` |
| Capability string | `<noun>.<verb>` | `sale.void`, `price.override` |
| i18n key | `<screen>.<element>.<variant>` | `sale.action.park` |

**Rate as parts-per-million, not basis points.** The blueprint says basis points (`rate_bp`). Parts-per-million is used instead: Jordanian reduced rates include values like 1% and 2%, and future decrees are not guaranteed to land on whole basis points. `rate_ppm` costs nothing and removes a class of "we cannot represent 0.125%" conversation. 16% = `160_000`; 4% = `40_000`; 0% = `0`.

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
  struct IpcError { code: &'static str, message_key: String, detail: Option<String> }
  ```
  `code` is what the UI branches on. `message_key` is what the UI translates. `detail` is for the log and the diagnostics screen, never shown raw to a cashier.
- **`unwrap()` and `expect()` are banned outside tests and `main()`.** A panic in a register is a lost sale. `clippy::unwrap_used` and `clippy::expect_used` are denied at the workspace level from Phase 1 (microstep 1.0.3).

---

## 5. Testing

Five layers. A microstep is not done until its layer is green.

| Layer | Tool | Where | Rule |
|---|---|---|---|
| Example tests | `#[test]` | inline `mod tests` | Every branch of every rule that a human would argue about |
| **Property tests** | `proptest` | inline `mod tests` | Every invariant in §1 and every one in `test-catalog.md`. This is the layer that finds the bugs you did not imagine |
| Golden files | byte diff | `tests/golden/` | Receipts (per width, per language), fiscal XML documents. Regenerate deliberately, review the diff, commit |
| Integration | real SQLite / real Postgres | `crates/*/tests/` | Migrations run; repositories round-trip; transactions roll back on error |
| Chaos | scripted | `crates/pos-sync/tests/` | Replay, drop, duplicate, reorder. Both databases converge byte-identical |

**Property tests are not optional and not a later phase.** `Money::split_evenly` already has one and it is the model: state the invariant in a comment in the words a human would use, then let `proptest` attack it.

**Determinism.** No test may read the wall clock, generate a random UUID outside `proptest`'s control, or depend on filesystem ordering. Time and IDs are injected. A flaky test in a money system is worse than no test — it trains you to ignore red.

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

| Budget | Limit | Measured by | Added in |
|---|---|---|---|
| Scan → line on screen | < 100 ms | Playwright trace, hardware simulator | 1.9.4 |
| Cart total recompute | < 16 ms | `criterion`, 200-line cart | 1.4.9 |
| Search-as-you-type, 50k SKUs | < 50 ms | `criterion` over the seeded fixture | 1.2.7 |
| Cold start → sellable | < 3 s | packaged-app smoke timer | 2.9.3 |

Each becomes a CI job that **fails the build on regression**, not a dashboard nobody opens. A budget without a failing test is a wish.

---

## 8. Commits

```
<type>(<scope>): <summary>            [<step>]

feat(domain): tax engine, inclusive + exclusive extraction   [1.3.4]
fix(db): sale_line qty to milli-units                        [1.1.7]
test(fiscal): discount percentage round-trip property        [2.7.6]
docs(impl): phase 2 fiscal conformance harness               [—]
```

`type` ∈ `feat` `fix` `test` `docs` `chore` `refactor` `perf`. `scope` is the crate or app short name — `domain`, `db`, `sync`, `hardware`, `fiscal`, `terminal`, `server`, `backoffice` — plus `repo` for the workspace itself (gates, CI, tooling) and `impl` for the implementation doc set. The list is closed: `.githooks/commit-msg` refuses anything else, and so does the `branch-flow` check on a pull-request title. One microstep, one commit, wherever possible — a bisect that lands on a microstep tells you exactly what broke.

**This whole format is a law, not a preference** — [`.githooks/commit-msg`](../../.githooks/commit-msg)
refuses a subject that breaks it, and CI's `branch-flow` check refuses a pull-request title that
does, because a squash-merge commits the title. Two more rules live in the same hook: the summary is
≤ 72 characters before the step tag with no trailing period, and **no agent-attribution trailer ever
enters this history** — no `Co-Authored-By` naming a machine identity, no "Generated with" line. That
last one is decided by the trailer's *address*, not its display name, so a human co-author is never
refused whatever they are called. Seven such trailers had to be rewritten out of the first nineteen
commits once; that is why it is a gate now and not a habit.

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

The blueprint says migrations should be "tested up *and* down in CI." The shipped runner (`crates/pos-db/src/lib.rs`) is a `PRAGMA user_version` counter with no down path, and that is the right choice — down migrations in a system whose whole premise is that financial facts are never destroyed are a liability that rots unmaintained. The rollback story is **restore from an encrypted backup**, which is a real operation the business needs anyway, and which is therefore actually tested (G-1, microstep 1.8.x).

Rules:

1. `crates/pos-db/migrations/NNNN_name.sql`, appended to the `MIGRATIONS` array in order. **Never edit a committed migration.** Not to fix a typo. Not "it hasn't shipped yet."
2. Every migration is idempotent under the runner (the runner guarantees each runs once; the SQL must not assume more).
3. A migration that changes the shape of existing data ships with a data migration in the same file and a test that seeds the old shape, migrates, and asserts the new one.
4. Postgres mirrors SQLite in `apps/server/migrations/` via sqlx, same number, same name, same semantics. Divergence is a sync bug waiting.
5. The app **refuses to start on a half-migrated database** (E.58) and says so — it does not guess.

---

## 10. Internationalisation (G-5)

Arabic is not a translation of this product. It is the product; English is the toggle.

- **Direction:** the app is RTL by default. `<html dir="rtl" lang="ar">`; the English toggle flips `dir`/`lang` only. Every layout uses **CSS logical properties** — `margin-inline-start`, not `margin-left`; `inset-inline-end`, not `right`. Tailwind's `ps-*`/`pe-*`/`ms-*`/`me-*`/`start-*`/`end-*` throughout; `pl-*`/`left-*` is a lint failure — `./scripts/check-logical-css.sh`, in `just lint` and CI's `web` job. Biome's recommended preset knows nothing about Tailwind utilities or CSS sides, so until that script existed this rule was written down and unenforced. A case that really is physical carries `physical-ok: <reason>` on the line.
- **Numerals:** Western Arabic digits (0–9) everywhere. That is Jordanian retail practice; Eastern Arabic-Indic digits on a receipt confuse more than they serve.
- **Catalog:** a typed message catalog, keys as §2, `ar` and `en` files kept in lockstep by a test that fails when a key exists in one and not the other. The catalog is the single source for UI strings; a string literal in a component is a lint failure.
- **Money and dates render through one function.** `formatMoney(minor, currency, locale)` and `formatDate(iso, tz, locale)` — never `toLocaleString` scattered inline, because display precision (2 vs 3 decimals, B.5) is a store setting.
- **Font:** one family covering Arabic and Latin, embeddable, shipped with the app — no network font. The same font file feeds the receipt rasteriser, so the receipt looks like the screen. Chosen in microstep 1.7.2.
- **Product names carry both `name_ar` and `name_en`.** The receipt prints per store setting; search matches both.

---

## 11. Time (G-4)

- **Storage:** UTC, ISO-8601 with milliseconds, `TEXT`. `strftime('%Y-%m-%dT%H:%M:%fZ','now')` is already the schema default.
- **Reporting:** store-local calendar day, `Asia/Amman`, from the store's configured timezone — not the OS timezone, which a cashier can change.
- **Business date** is *not* derived from wall-clock midnight. It is:

  ```
  business_date(sale) = the business_date of its shift
  business_date(shift) = local calendar date of shift.opened_at,
                         unless opened_at is before the store's `day_cutover_time`,
                         in which case the previous local date
  ```

  A shift opened at 00:30 belongs to yesterday's trading day. `day_cutover_time` defaults to `04:00` local and is a store setting. Z reports close a *shift*, so a Z belongs to the shift's business date, not the wall clock's (E.7).
- **Monotonic guard:** persist the last observed timestamp. On a backward jump, log an audit entry and keep issuing non-decreasing timestamps until wall-clock catches up. Sequences are counters and never derived from time (E.6).

---

## 12. Security posture in one page

Full treatment in [`ref/security-compliance.md`](ref/security-compliance.md). The rules you must not break without reading it:

- **Never log:** PAN, track data, CVV, PINs, PIN hashes, DB keys, JoFotara secrets, customer name/phone/email. Enforced by a scrubbing layer *and* a test that feeds known PII through the logger and asserts absence (G-8).
- **Never store:** anything from a card except the PSP reference, the masked PAN the terminal returns for the receipt, and the scheme.
- **Permissions are checked in Rust**, in the command handler, via the guard in §6 of the security doc. Hiding a button is UX. The check is security.
- **The DB key lives in the OS credential store.** Never a file, never an env var in production. `POS_DB_KEY` exists for CI and dev only, and the release build refuses to honour it — `pos_db::key::honours_env_key()` is `cfg!(debug_assertions)`, and the policy is a pure function so a debug test can assert what a release build does. The refusal is ignore-and-continue, not an error: falling through to the credential store is the safer outcome, and a stray variable inherited from a shell must never stop a register from opening.
- **Escalation is recorded distinctly from operation.** The approving manager's id is a different column from the operating cashier's, and a setting can require them to differ (E.52).

---

## 13. Tauri IPC contract

- Commands are the **only** channel from UI to core. No `fs`, no `shell`, no `http` plugin exposed to the webview; the capability file grants nothing beyond what a command needs.
- Every command: `snake_case`, verb-first, returns `Result<T, IpcError>`, and declares its required capability in the registry (see [`ref/ipc-contract.md`](ref/ipc-contract.md)). A command with no capability declaration fails the exhaustiveness test.
- **TS types are generated from Rust, not hand-written.** `ts-rs` derives on every IPC type, emitted into `packages/api-types/`, and CI fails if the committed output differs from a fresh generation. Two hand-maintained copies of a money type is how a rounding bug ships.
- Long operations (card collection, printing, fiscal submission) are commands that return immediately with a handle and emit **events** for progress. A cashier watching a spinner with no state is a cashier who presses the button again.

---

## 14. What "sellable" means at each gate

Written down because it is the only question that matters when a phase runs long:

| After | A real store could… |
|---|---|
| Phase 0 | …nothing. It compiles and ships. |
| **Phase 1** | …**sell for cash, all day, fully offline, in Arabic, with correct GST and a printed receipt.** |
| Phase 2 | …take cards, handle returns, run shifts and Z reports, and produce fiscal documents that pass every check short of the ISTD network. |
| Phase 3 | …run more than one register, administer from a back office, keep customers, and clear invoices with ISTD for real. |
| Phase 4 | …run three stores with promotions, receiving, counts, transfers, and a full report suite. |
| Phase 5 | …be sold to someone who is not you. |

If a phase is running long, cut scope toward the next row of this table, never away from it.
