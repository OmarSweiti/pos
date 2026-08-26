# Phase 1 — Sellable MVP

> **Exit:** a real Jordanian minimarket could sell for cash, all day, fully offline, in Arabic, with correct GST and a printed receipt.

**Effort:** 14–20 weeks for a solo developer learning Rust on the job: 8–11 weeks for Phase 1A (groups 1.1–1.7 through the pure receipt pipeline, plus the early 1.8.9 commit-manifest writer and schema-spine steps 1.9.1 and 1.10.1 needed before migration 0007) and 6–9 weeks for Phase 1B. In 1B, land 1.8.0–1.8.2b and 1.7.7, complete the backend halves of the sequence/shift and stock repositories in 1.9.2–1.10.5, build 1.11.0 before their UI tests, then land the checkout-journal/repository half of 1.8.4 before atomic finalize in 1.8.3. Finish 1.8.4's startup recovery orchestration and printer-unavailable handling in 1.7.6b before the remaining recovery, UI and gate work. This phase now contains the tax engine, Arabic raster pipeline, recovery-key custody, crash-safe fact commits, a minimal shift lifecycle, stock opening and the register UI; treating those integrations as an acceleration would hide a slip until the final gate.
**Scope from the master plan:** C.1 (base), C.2, C.3, C.4 cash, C.7 ledger-only, C.10, C.11 receipts, D screens 1–5 and 10–11.
**Plus, from this plan's gap analysis:** G-1 backup, G-2 sequences, G-4 business date, G-5 i18n, G-6 permission guards, G-7 audit chain, G-9 benchmarks, G-10 seed fixture, G-11 Money completion, G-12 qty fix.

**Do not skip the gap items.** Every one of them is cheap now and expensive later, and three of them (G-11, G-12, G-2) are impossible to add once real sale rows exist.

---

## Group dependency graph

```
1.1 domain foundations ─┬─→ 1.2 catalog & search ─┬─→ 1.4 cart machine ─┬─→ 1.5 cash tenders
   Money/Qty/ids/time   │                          │                     │
                        └─→ 1.3 tax engine ────────┘                     ├─→ 1.7 receipts
                                                                          │
1.6 users & audit ────────────────────────────────────────────────────────┤
1.9 shifts, sequences & business date ────────────────────────────────────┤
1.10 stock ledger ────────────────────────────────────────────────────────┤
                                                                          ▼
                                                          1.8 persistence & finalize
                                                                          │
                                                          1.11 terminal UI (RTL, i18n)
                                                                          │
                                                          1.12 seed, benchmarks, gate
```

1.6, 1.9 and 1.10 can proceed in parallel with the cart work and are good places to go when you are stuck on the state machine. Build 1.1.0 before any property, 1.2.0 before any `just bench-gate` completion command, the database half of 1.1.9 after 1.9.1, and the database half of 1.2.4 after 1.2.5. Build 1.8.9 after migration `0003` and before any 1.6, shift or stock repository writes an append-only fact. Build 1.6.3 and 1.6.4 before the privileged transitions in 1.4.7 and 1.4.12, and 1.6.7 before completing the registry contract in 1.4.11. Build 1.11.0 before the UI halves of 1.8.5b, 1.9.5 and 1.10.5, and 1.12.1 before the seeded performance, screenshot and manual steps 1.11.13–1.11.15. Document numbering is a stable reference, not permission to ignore an explicit dependency.

---

## Group 1.1 — Domain foundations

*Gap G-11. Nothing else can start until `Money` knows what currency it is.*

> **Build order inside this group is not step order.** 1.1.2 asks for `mul_qty`, `mul_percent` and
> `round_to_step`, but `Qty` is 1.1.3, `Percent` is 1.1.4 and `RoundingRule` is 1.1.6 — so written in
> the numbered order, 1.1.2 does not compile. Split it in two and build in this order:
>
> | Order | Step | What |
> |---|---|---|
> | 1 | 1.1.0 | shared property-test harness — every property from the first commit uses one case-count, seed and persistence policy |
> | 2 | 1.1.1 | `Currency` |
> | 3 | 1.1.2a | `Money` gains `Currency` — constructors, `checked_add`/`sub`/`neg`, `sum`, `split_evenly`, `checked_cmp` |
> | 4 | 1.1.6 | `RoundingRule`, `RoundingDirection` — rounding is a *parameter*, so it precedes the arithmetic that takes one |
> | 5 | 1.1.3 | `Qty` |
> | 6 | 1.1.4 | `Percent` |
> | 7 | 1.1.2b | `Money`'s arithmetic — `mul_qty`, `mul_percent`, `split_proportional`, `split_proportional_by`, `round_to_step`, `to_decimal`, `from_decimal`, fallible `format`, `format_exact`, `parse` |
> | 8 | 1.1.5 | the complete money property suite |
> | 9 | 1.1.8 | typed ids, `IdSource`, `SeqIdSource` |
> | 10 | 1.1.9 | `Timestamp`, `BusinessDate`, `DayBoundary`, `MonotonicClock`, **and the `Clock` port with `FixedClock`** |
>
> The original 1.1.2 heading remains as the concordance heading; implementation commits use
> `[1.1.2a]` and `[1.1.2b]` so the two independently green changes stay distinguishable.
>
> **The same argument moves `Clock` out of 1.1.8.** `Clock::now` returns `Timestamp`, `Timestamp` is
> 1.1.9's, and a `Clock` written at position 9 does not compile for exactly the reason 1.1.2 was
> split. It travels with the type it returns, and so does `FixedClock`.

### 1.1.0 — Shared property-test harness
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-domain/Cargo.toml`, `crates/pos-test-support/Cargo.toml` (new), `crates/pos-test-support/src/lib.rs` (new), `crates/pos-test-support/src/proptest.rs` (new), `crates/pos-test-support/tests/config.rs` (new)

```rust
pub fn domain_proptest_config() -> ProptestConfig; // default 4_096 cases
pub fn io_proptest_config() -> ProptestConfig;     // default 256 cases
```

Create the workspace test-support crate before the first property is changed. The helper fixes the deterministic seed, enables committed `proptest-regressions/`, then applies `PROPTEST_CASES` as a raising override after the crate default; an invalid or lower override is refused. Individual properties name their strategy but cannot silently choose fewer cases.
**Tests:** `shared_domain_config_uses_4096_cases` · `shared_io_config_uses_256_cases` · `scheduled_case_override_is_not_shadowed_by_shared_default` · `a_failed_property_persists_its_seed_and_minimized_case`
**Done when:** `cargo nextest run -p pos-test-support` exits zero and the `PROPTEST_CASES=100000` fixture observes exactly 100,000 effective cases.

### 1.1.1 — `Currency`
**Files:** `crates/pos-domain/src/money.rs`
Add the `Currency` type from [`ref/domain-api.md`](ref/domain-api.md) §1.1, with `JOD` (exponent 3) and `USD` (exponent 2) constants. `Copy`, four bytes, interned `code()` returning `&'static str` with no allocation.
**Tests:** `jod_exponent_is_three` · `unknown_currency_code_errors` · `currency_serialises_as_its_iso_code` · `unknown_currency_code_is_a_deserialisation_error` · `the_exponent_never_appears_on_the_wire` · `golden_money_json_is_stable`
**Verify:** `cargo nextest run -p pos-domain money::`
**Done when:** `Currency::JOD.minor_per_major() == 1000`.

### 1.1.2 — `Money` carries `Currency`
This stable concordance heading is implemented by 1.1.2a and 1.1.2b; do not combine them into a commit that cannot compile before `Qty`, `Percent` and `RoundingRule` exist.
**Concordance only:** this retained anchor is not an executable microstep; 1.1.2a and 1.1.2b each carry the required files, tests and runnable completion command.

### 1.1.2a — Currency-safe `Money` core
**Files:** `crates/pos-domain/src/money.rs`, `apps/terminal/src-tauri/src/lib.rs` (fix `split_tender`)

```rust
pub struct Money { minor: i64, currency: Currency }
pub fn checked_add(self, other: Money) -> Result<Money, MoneyError>;
pub fn checked_cmp(self, other: Money) -> Result<Ordering, MoneyError>;
```

Thread `Currency` through every constructor and operation. `checked_add`/`checked_sub` return `CurrencyMismatch` rather than coercing. **Do not rewrite `split_evenly`** — its largest-remainder implementation and property test are correct; only add the currency field.
**`PartialOrd` and `Ord` come off the derive list.** Derived over `(minor, currency)` they would order a JOD amount against a USD one and answer confidently. `checked_cmp` replaces them, returning `Result<Ordering, MoneyError>`, so a mixed-currency comparison is a handled error and not a wrong answer. Every `<`, `>`, `min`, `max` and `sort` on a `Money` becomes a compile error that has to be looked at, which is the point.
**Tests:** `prop_currency_mismatch_never_silently_coerces` · `prop_add_sub_roundtrip` · `prop_split_preserves_total` · `mixed_currency_comparison_is_refused`
**Done when:** `cargo nextest run -p pos-domain money::` exits zero before 1.1.3, 1.1.4 or 1.1.6 is implemented.

### 1.1.2b — Complete `Money` arithmetic and formatting
**Files:** `crates/pos-domain/src/money.rs`

```rust
pub fn split_proportional_by(self, weights: &[i64])
    -> Result<Vec<Money>, MoneyError>;

pub fn format(self, decimals: u8) -> Result<String, MoneyError>;
pub fn format_exact(self) -> String;
```

Add `mul_qty`, `mul_percent`, `split_proportional`, `split_proportional_by`, `round_to_step`, `to_decimal`, `from_decimal`, fallible `format`, `format_exact`, and `parse`. The primitive conserves over caller-supplied weights; `pricing.rs` owns stable line sorting before it calls the primitive, because a shared money helper must not know product identifiers.
**Tests:** `prop_format_exact_parse_roundtrip` · `format_truncating_a_fil_is_refused` · `prop_mul_qty_whole_units_is_repeated_add` · `prop_split_proportional_preserves_total` · `prop_split_proportional_by_preserves_total`
**Done when:** `cargo nextest run -p pos-domain money::` passes, including `matches!(Money::from_minor(1259, JOD).format(2), Err(MoneyError::NotRepresentableAtPrecision(..)))` and `format_exact() == "1.259"`.

> A shorter catalogue display is permitted only when it is exact. Totals, tax, tenders, change, receipts and fiscal views use `format_exact()` at the currency exponent, because hiding a fil that is later charged is a price-display defect. The source-plan rate and rounding overrides are recorded in [`00-master-plan.md`](00-master-plan.md) §4a, “Errata and concordance”.

### 1.1.3 — `Qty` in milli-units
**Files:** `crates/pos-domain/src/money.rs`
Per §1.3 of the API reference. `Qty::ONE == 1000`.
**Tests:** `prop_qty_add_sub_roundtrip` · `weighed_formats_three_decimals` · `whole_units_format_without_decimals`
**Done when:** `Qty::from_milli(347).format(true) == "0.347"` and `Qty::ONE.format(false) == "1"`.

### 1.1.4 — `Percent` in parts-per-million
**Files:** `crates/pos-domain/src/money.rs`
Per §1.4 of the API reference, whose two decimal projections run in opposite directions:
`from_percent_decimal` reads the **percentage** a decree or a settings row carries, and
`to_decimal` returns the **fraction** the tax arithmetic multiplies by. `to_percent_decimal` is the
inverse of the constructor, and the reason the named round-trip can be written at all. Excess
precision and excess magnitude are refused, never rounded.
**Tests:** `sixteen_percent_is_160000_ppm` · `prop_percent_decimal_roundtrip`
**Verify:** `cargo nextest run -p pos-domain money::`
**Done when:** `Percent::from_percent_decimal(Decimal::new(125, 3)) == Ok(Percent::from_ppm(1_250))`, `Percent::from_ppm(1_250).format() == "0.125%"` and `Percent::from_ppm(160_000).to_decimal() == Decimal::new(16, 2)` — the 0.125% rate basis points cannot hold, rendered without the four trailing zeros the representation carries, and the fraction `to_decimal` answers rather than the percentage the rate was built from.

### 1.1.5 — Money property suite
**Files:** `crates/pos-domain/src/money.rs`, `crates/pos-domain/Cargo.toml`, `.github/workflows/proptest-scheduled.yml` (new)
The high-count property lane stays separate because Actions minutes are metered and `ci.yml` deliberately has no `schedule:` trigger.
Every property from API reference §1.6 uses 1.1.0's shared helper and names the strategy it attacks. The scheduled domain lane runs `PROPTEST_CASES=100000` and prints its replayable seed. This is the layer that finds what you did not imagine; do not replace a listed property with a smaller example suite.
Microstep 1.1.2a owns `prop_add_sub_roundtrip`, `prop_split_preserves_total` and `prop_currency_mismatch_never_silently_coerces`; 1.1.2b owns the proportional, quantity, exact-format and truncation properties.
**Tests:** `prop_round_to_step_is_idempotent` · `prop_round_to_step_within_half_step`
**Done when:** `cargo nextest run -p pos-domain money::tests::prop_ && PROPTEST_CASES=100000 cargo nextest run -p pos-domain money::tests::prop_` exits zero and the scheduled CI job invokes the same high-count command.

### 1.1.6 — `RoundingRule` and `RoundingDirection`
**Files:** `crates/pos-domain/src/money.rs`
Both enums from [`ref/domain-api.md`](ref/domain-api.md) §1.2, **and the one rounding primitive
`RoundingRule::round_to_i64`** that 1.1.2b's arithmetic calls. Two bare enums have no behaviour, and
I-1's "rounds once" needs exactly one implementation rather than one per caller, so the primitive
belongs to the step that introduces the parameter. `RoundingDirection` stays behaviourless until
1.5.3 builds `round_to_step`. Default `HalfAwayFromZero`, not banker's — see
[`ref/tax-jordan.md`](ref/tax-jordan.md) §4 for why, and note it is a jurisdiction default rather
than a `Default` impl.
**Tests:** `half_away_from_zero_rounds_1_5_to_2_and_neg_1_5_to_neg_2` · `half_even_rounds_1_5_and_2_5_both_to_2`
**Verify:** `cargo nextest run -p pos-domain money::`
**Done when:** `RoundingRule::HalfAwayFromZero.round_to_i64(Decimal::new(-15, 1)) == Ok(-2)` and `RoundingRule::Floor.round_to_i64(Decimal::new(-12, 1)) == Ok(-2)` — the two below-zero answers a plausible implementation gets wrong.

### 1.1.7 — Migration `0002`: the qty fix — **SHIPPED**
*Gap G-12. **Must land before any sale row exists**, which is why it went first.*
**Files:** `crates/pos-db/migrations/0002_sale_integrity.sql`, `crates/pos-db/src/lib.rs` (`MIGRATIONS` array)
The `sale_line` rebuild from [`ref/schema.md`](ref/schema.md) §0002 — `qty` → `qty_milli`, multiplying
by 1000 because existing rows hold unit counts — plus the eight I-4 immutability triggers, the
per-register receipt-number uniqueness index, and the two foreign-key indexes. All in one file,
because a rebuild silently takes its triggers and indexes with it.
**Tests:** `crates/pos-db/tests/migration_0002_qty_milli.rs` —
`quantities_are_multiplied_by_a_thousand_not_merely_renamed` seeds a `0001`-shaped row, migrates,
and asserts `qty_milli == qty * 1000`; `the_rebuilt_table_is_still_guarded_by_the_immutability_triggers`
proves the rebuild did not drop them. `crates/pos-db/tests/sale_immutability.rs` holds all eight.
**Done when:** `PRAGMA user_version` is 2 and the seeded row survives with `qty_milli = 2000`. ✅
> `sale_line_tax` and `sale_line_discount` are **not** here. They belong to `0003_strict_rebuild_and_catalog_depth.sql`
> (§0003), because they reference `tax_category`, which does not exist until that migration.
> `sale_line`'s remaining capture-time columns arrive there too, by `ALTER TABLE` rather than a
> second rebuild — and `0003` must drop and recreate the three `sale_line` triggers around its
> backfill, since an `UPDATE` on a completed sale's line is exactly what they refuse.

### 1.1.8 — Typed ids and the `IdSource` port
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-domain/Cargo.toml`, `crates/pos-domain/src/ids.rs` (new), `crates/pos-domain/src/lib.rs`, `crates/pos-domain/tests/typed_ids_ui.rs` (new), `crates/pos-domain/tests/ui/typed_ids_do_not_interconvert.rs` (new), `crates/pos-domain/tests/ui/typed_ids_do_not_interconvert.stderr` (new)
The `typed_id!` macro and fifteen id types from API reference §2, including `ApprovalId`, plus `IdSource` and its deterministic double `SeqIdSource`. Count the types: the reference records a revision that declared thirteen while this file said fourteen, and `OrgId` and `CategoryId` are the two a reader skips.
**`Clock` and `FixedClock` are not here; they land with 1.1.9.** `Clock::now` returns `Timestamp`, which `time.rs` defines one microstep later, so a `Clock` written here does not compile — the same dependency that split 1.1.2, and the reason this step's `Done when` names neither of them. Do not invent a placeholder `Timestamp` to make the trait compile early: a second definition of time is worse than a deferred trait.
`SeqIdSource` is **not** behind `#[cfg(test)]` — the server and the cross-crate integration tests construct the same id stream a domain property did. It is v7-*shaped*: the layout is RFC 9562's, while the millisecond field is a caller-supplied anchor plus the call index and `rand_a`/`rand_b` hold a stream tag and the sequence number. Purity (I-8) is what forces that: the crate may not add a `uuid` version feature, so the bytes are composed by hand and handed to `Uuid::from_u128`, and `scripts/check-domain-purity.py` refuses both the feature and the generating constructor by name.
**Tests:** `typed_ids_do_not_interconvert` (a compile-fail test via `trybuild`) · `seq_id_source_is_reproducible` · `seq_ids_carry_the_v7_layout` · `the_stream_tag_and_the_sequence_occupy_their_own_fields` · `all_fifteen_typed_ids_round_trip_through_json` · `a_typed_id_displays_as_the_plain_uuid` · `a_typed_id_costs_nothing_over_its_uuid` · `typed_ids_order_by_their_bytes_and_never_by_causality` · `prop_seq_id_sources_agree_when_constructed_alike` · `prop_seq_ids_never_collide` · `prop_seq_ids_keep_the_v7_layout`
**Done when:** `cargo nextest run -p pos-domain --test typed_ids_ui && cargo nextest run -p pos-domain seq_id_source_is_reproducible` exits zero; the `trybuild` fixture rejects `fn f(s: SaleId, l: SaleLineId)` when its arguments are swapped.
> The `.stderr` golden is rustc-version-sensitive: a compiler release can reword E0308. `rust-toolchain.toml` pins the compiler, so it is stable today, and whoever bumps that pin regenerates the golden in the same change with `TRYBUILD=overwrite cargo test -p pos-domain --test typed_ids_ui`. Keep the fixture minimal for the same reason — every diagnostic it emits lands in the golden.

### 1.1.9 — `Timestamp`, `BusinessDate`, `DayBoundary`, and the `Clock` port
*Gap G-4.*
**Scheduled in:** build the pure domain value/policy types and terminal zone-resolution shell at 1.1.9. The database half remains deferred: only after 1.9.1 creates `trusted_time_state` does it add `ClockState` persistence, as recorded under 1.9.1. This microstep therefore remains partly open until that deferred gate lands.
**Files (current half):** `Cargo.toml`, `Cargo.lock`, `apps/terminal/src-tauri/Cargo.toml`, `crates/pos-domain/src/lib.rs`, `crates/pos-domain/src/time.rs` (new), `apps/terminal/src-tauri/src/lib.rs`, `apps/terminal/src-tauri/src/time.rs` (new)
Per API reference §3, including `business_date_of`, `MonotonicClock`, the pure `ClockState` value type, `clock_confidence` and `effective_now`. The store persists the IANA zone id `Asia/Amman`; the shell resolves its offset for each instant with `jiff` and passes that value into pure `pos-domain`. Do not encode a seasonal Jordan rule: Jordan is year-round UTC+3, and the old rule moves winter sales across the 04:00 cutover.
**This step also owns the `Clock` port and `FixedClock`**, which API reference §2 originally placed in 1.1.8: `pub trait Clock { fn now(&self) -> Timestamp; }` cannot exist before the `Timestamp` this file defines, and `MonotonicClock<C: Clock>` needs the trait anyway. `IdSource` and `SeqIdSource` stayed at 1.1.8, where they compile.
**Tests:** `shift_opened_at_0030_belongs_to_previous_day` · `prop_business_date_stable_across_shift` · `prop_cutover_boundary_never_skips_a_day` · `prop_monotonic_clock_never_decreases` · `prop_effective_now_never_precedes_high_water` · `prop_clock_confidence_is_monotone_in_skew` · `clock_jump_back_reports_anomaly` · `a_never_synced_register_is_untrusted_not_trusted` · `wall_clock_moved_forward_a_year_is_suspect` · `a_reboot_without_an_anchor_is_a_monotonic_reset` · `no_clock_confidence_refuses_a_sale` · `business_date_uses_the_offset_in_force_at_the_instant` · `a_january_sale_and_a_july_sale_agree_in_asia_amman` · `resolving_an_unknown_zone_id_is_a_named_error_not_a_default_offset`
**Current half done when:** `cargo nextest run -p pos-domain time:: && cargo nextest run -p terminal time::tests::` passes, including a shift opened at `2026-08-21T00:30` local with a `04:00` cutover resolving to business date `2026-08-20` and both January and July resolving through `Asia/Amman`.
**Full-step status:** 1.1.9 is not complete until the deferred database-half gate recorded under 1.9.1 also passes.

---

## Group 1.2 — Catalog, barcodes, search

> **Build order inside this group is not step order.** Create schema `0003` in 1.2.1, then build
> the pure catalogue types in 1.2.2 and pure scan parser in 1.2.4. After migrations `0004`–`0006`
> exist, land `0007` in 1.2.5; only then can 1.2.3 build its FTS repository and the database half
> of 1.2.4 build trade-scale commissioning. Finish with 1.2.6–1.2.8. This order keeps a public
> repository signature from naming an unwritten `ScanLookup` and keeps FTS tests from running
> against tables that do not exist.

### 1.2.0 — Benchmark gate harness
**Files:** `justfile`, `scripts/bench-gate.py` (new), `scripts/tests/bench_gate_test.py` (new), `benchmarks/reference-register.toml` (new), `benchmarks/baselines/` (new), `docs/implementation/ref/hardware-and-receipts.md`

```text
just bench-gate [search|price-cart|pin-verify|scan-to-line]
```

Select and fill the lowest supported register row in the device matrix and mirror its exact identity in `benchmarks/reference-register.toml`. The gate reads committed JSON measurements, refuses blank or mismatched hardware, and exits non-zero on an absolute-limit breach or a regression beyond conventions §7. A hosted runner may exercise fixed fixtures but cannot publish a baseline bearing the reference-register identity.
**Tests:** `bench_gate_fails_an_absolute_budget` · `bench_gate_fails_a_significant_regression` · `bench_gate_refuses_a_missing_reference_profile` · `reference_profile_matches_supported_device_matrix` · `hosted_runner_cannot_publish_a_performance_baseline`
**Done when:** `python3 scripts/tests/bench_gate_test.py && python3 scripts/bench-gate.py --check-profile` exits zero after proving all refusal paths and matching every committed physical-register identity field to `benchmarks/reference-register.toml`.

### 1.2.1 — Migration `0003`: org / store / register / taxonomy
**Files:** `crates/pos-db/migrations/0003_strict_rebuild_and_catalog_depth.sql`
Per [`ref/schema.md`](ref/schema.md) §0003, **in that order**: the STRICT rebuild of the six tables 0001/0002 created loose, then the `org`, `store`, `register`, `category`, tax-rule-pack tables, `barcode`, `setting`, commit-envelope tables and the `product` `ALTER`s. `barcode.pack_qty_milli INTEGER NOT NULL DEFAULT 1000` makes a six-pack code add six units rather than one; `regulated_kind` and `sale_form` make a tobacco row a sealed pack rather than an individual-cigarette SKU.

The rebuild goes first, and its statements are the most dangerous in the chain — they drop and recreate the tables holding every completed sale. Read the recipe before you type it: SQLite's twelve-step procedure begins by turning foreign keys off, which is a no-op inside a transaction, and `defer_foreign_keys` does not substitute. Staging tables are why this commits with foreign keys enforced.
**Tests:** `migration_0003_creates_all_tables` · `barcode_live_uniqueness_allows_reissue_after_tombstone` · `barcode_pack_qty_defaults_to_1000_milli` · `a_pack_quantity_of_zero_is_refused_at_save` · `tobacco_product_must_be_a_sealed_pack` · `the_rebuild_keeps_every_row_of_a_completed_sale` · `the_rebuilt_tables_are_all_strict` · `the_rebuild_restores_the_immutability_triggers` · `no_staging_table_survives_the_rebuild` · `after_the_rebuild_the_six_tables_enforce_their_types`
**Done when:** a tombstoned barcode code can be reassigned to a different product; two live rows with the same code cannot exist.

### 1.2.2 — `Product` and `UnitOfMeasure` in the domain
**Files:** `crates/pos-domain/src/catalog.rs` (new)
Per API reference §4. Include `min_age`, `max_price_minor`, `is_service`, `regulated_kind` and `sale_form` on `Product`, and `pack_qty: Qty` on the barcode lookup result. The database column remains `barcode.pack_qty_milli`; the domain type carries its unit in `Qty`. The age gate does not make an individual-cigarette SKU lawful, so the sealed-pack invariant belongs beside the product type; the pack quantity belongs on the code because the same product can be sold by unit and by case.
**Tests:** `a_barcode_pack_quantity_is_a_qty` · `tobacco_product_requires_a_sealed_pack_sale_form`
**Done when:** `cargo nextest run -p pos-domain catalog::` passes with the same product resolving to `Qty::ONE` and `Qty::from_units(6)` through two different barcode records.

### 1.2.3 — `ProductRepository`
**Files:** `crates/pos-db/src/repo/product.rs` (new), `crates/pos-db/src/repo/mod.rs` (new)
```rust
impl<'c> ProductRepository<'c> {
    pub fn by_id(&self, id: ProductId) -> Result<Option<Product>, DbError>;
    pub fn by_barcode(&self, code: &str) -> Result<Vec<ProductHit>, DbError>;
    pub fn by_plu(&self, code: &str) -> Result<Option<Product>, DbError>;
    pub fn search(&self, q: &str, limit: u32) -> Result<Vec<ProductHit>, DbError>;
    pub fn upsert(&self, tx: &Transaction, p: &Product) -> Result<(), DbError>;
}
```
Repository law (conventions §3): returns owned domain types, never a `rusqlite::Row`, never computes a total. Writes take an explicit `&Transaction`.

`search` issues **two branches**, not one — see 1.2.5:

```
name_ar_exact : <query with tashkeel and tatweel stripped>
     OR
name_ar_fold  : <query fully folded>
```

Folding only, which is what a first pass reaches for, makes Arabic search *work* and makes it *wrong*: once the query is folded, an exact spelling and a near-miss are indistinguishable, so the product the cashier meant can sort second. Querying both columns costs nothing and fixes it — the exactly-spelled row matches both branches and a variant-only row matches one, so `ORDER BY rank` puts the exact match first with no manual scoring.

The raw cashier string never becomes an FTS5 expression. Tokenise it on the index boundaries, quote every token with internal `"` doubled, join with `AND`, and append `*` only to the final token for prefix search. Binding a raw string does not neutralise FTS5 operators. The table-driven corpus is exactly `"`, `(`, `)`, `:`, `*`, `^`, `-`, `AND`, `OR`, `NOT`, `NEAR`, empty and whitespace-only input; none may stall the scanner-fallback path or change the query grammar.
**Tests:** `a_second_live_barcode_claim_is_refused` (E.36) · `a_multipack_barcode_adds_its_pack_quantity` (E.78) · `by_id_ignores_tombstones` · `catalog_save_above_ceiling_is_rejected` (E.71) · `search_survives_every_fts5_metacharacter` · `prop_no_query_string_produces_a_database_error` (E.39b)
**Done when:** `cargo nextest run -p pos-db a_second_live_barcode_claim_is_refused && cargo nextest run -p pos-db search_survives_every_fts5_metacharacter && cargo nextest run -p pos-db prop_no_query_string_produces_a_database_error` exits zero; the first test proves two live products cannot share a barcode and the latter two exercise the complete literal corpus above.

### 1.2.4 — Price-embedded barcode parser
**Scheduled in:** build the pure parser at 1.2.4; add commissioning persistence after 1.2.5 creates the trade-scale tables
**Files:** `crates/pos-domain/src/catalog.rs`, `crates/pos-domain/src/trade_scale.rs` (new), `crates/pos-domain/tests/scan_lookup_ui.rs` (new), `crates/pos-domain/tests/ui/price_embedded_lookup_is_not_constructible.rs` (new), `crates/pos-domain/tests/ui/price_embedded_lookup_is_not_constructible.stderr` (new), `crates/pos-db/src/repo/trade_scale.rs` (new), `apps/terminal/src-tauri/src/scan.rs` (new)
```rust
pub fn parse_scan_bytes(input: &[u8], rules: &[EmbeddedBarcodeRule])
    -> Result<ParsedScan, ScanError>;
pub fn parse_scan(raw_code: &str, rules: &[EmbeddedBarcodeRule])
    -> Result<ParsedScan, ScanError>;
impl ParsedScan {
    pub fn item_code(&self) -> &str;
}

pub enum ScanLookup {
    // Other variants omitted.
    #[non_exhaustive]
    PriceEmbedded {
        hit: ProductHit,
        price: PriceSource,
        derived: Option<DerivedWeight>,
    },
}

pub fn resolve_scan(parsed: ParsedScan, candidates: Vec<ProductHit>, currency: Currency)
    -> Result<ScanLookup, ScanError>; // pure domain scan handler
```
`ParsedScan` has private fields and carries the extracted item code/PLU plus the weight or price value; the pure parser never pretends it has resolved a product. The shell may read only `item_code()`, looks up every matching `ProductHit` through `ProductRepository`, then passes those hits and the opaque parse result to the pure domain scan handler. Only that handler constructs the exact price-bearing variant `ScanLookup::PriceEmbedded { hit, price: PriceSource, derived }`; `#[non_exhaustive]` on the variant prevents a dependent crate constructing it while still allowing consumers to match it with `..`. There is no public `ScanResult`, `EmbeddedAmount` or label-price constructor, and the repository never returns a resolved price. `PriceSource::from_label` is private to the domain handler and derives the amount from the matched rule and raw-code evidence inside `ParsedScan`. The line is one labelled package at the label total; any derived weight is an estimated stock basis, never the sale quantity. Ordinary `cart_add_line` never receives a caller-supplied price; a deliberate change goes through audited `cart_override_price`.

Commission each source scale with immutable maker, model and serial identity plus append-only verification evidence, its hash, seal/mark reference, effective timestamp and any expiry. The existing OPEN item in [`ref/schema.md`](ref/schema.md) §0007 owns the current JSMO evidence form and reverification cadence; until it is answered, every `embedded_barcode_rule` remains inactive and checkout refuses scale-derived pricing. A damaged label is a parsing error, while absent legal-metrology evidence is a commissioning refusal—the operator must see which one stopped the scan.
**Tests:** `prop_ean13_checksum_matches_reference` · `prop_embedded_parse_roundtrip` · `prop_corrupt_digit_never_parses_clean` · `non_utf8_scan_is_a_named_error` · `weight_embedded_2xxxxxwwwww_parses` · `price_embedded_2xxxxxpmmmmmc_parses` · `price_embedded_resolves_item_code_before_constructing_lookup` · `only_scan_handler_constructs_price_embedded_lookup` · `price_embedded_line_total_equals_the_label` · `price_embedded_line_is_one_unit_not_a_weight` · `price_embedded_after_a_price_per_kilo_change_still_charges_the_label` · `from_label_refuses_a_weight_rule` · `unverified_trade_scale_cannot_activate_an_embedded_rule` · `maintenance_pending_disables_scale_pricing`
**Done when:** `cargo nextest run -p pos-domain catalog:: && cargo nextest run -p pos-domain trade_scale:: && cargo nextest run -p pos-domain --test scan_lookup_ui && cargo nextest run -p pos-db trade_scale_ && cargo nextest run -p terminal scan::tests::` exits zero; E.40 rejects every one-digit corruption, the compile-fail fixture proves external code cannot construct `PriceEmbedded`, the handler resolves the extracted item code before returning it, and a rule without current signed verification evidence cannot produce a checkout price.

### 1.2.5 — Migration `0007`: FTS5, PLU, tiles, scan rules
**Files:** `crates/pos-db/migrations/0007_search_and_seed.sql`, `crates/pos-db/tests/migration_0007_search_and_seed.rs` (new)
Per [`ref/schema.md`](ref/schema.md) §0007: FTS/index triggers, PLU/tile data, immutable trade-scale identity, append-only scale-verification events and the guards that keep an embedded rule inactive without current verified evidence. This migration executes only after 0003–0006 have landed, even though the domain/search work is described before groups 1.6, 1.9 and 1.10; deriving the migration number from document order would collide with their schema.

`remove_diacritics 2` folds **Latin** diacritics only — it does not fold Arabic, and treats tashkeel as token separators, so `قَهْوَة` indexes as four single-letter tokens and a search for `قهوة` finds nothing (verified on SQLite 3.51). Arabic matching comes from `name_ar_fold`, the generated column added in 0003 and indexed here, plus `prefix='2 3'` for the 1–3 character search 1.2.7 benchmarks.

**Both sides must transform, and precision needs its own branch.** The repository applies the same two transforms to the query that 0003 applies to the columns — a folded index searched with an unfolded string returns zero rows — and then queries `name_ar_exact` and `name_ar_fold` together so the spelling the cashier typed outranks a variant. `search()` in 1.2.3 owns that, and `prop_sql_and_rust_folding_agree` is what stops the two implementations drifting.

Raw `name_ar` is not indexed, on purpose: tashkeel are token separators, so a vocalised name would contribute single-letter tokens and make one-character prefix search match unrelated products.

Search is the fallback for every unbarcoded item and the only path a cashier has when the scanner fails. 0007 is forward-only, so getting this wrong costs a compensating migration plus a full reindex on every installed register.
**Tests:** `fts_matches_arabic_with_and_without_diacritics` · `fts_matches_alef_and_yaa_spelling_variants` · `fts_matches_taa_marbuta_spelled_as_haa` · `fts_ignores_tatweel` · `prop_sql_and_rust_folding_agree` · `fts_prefix_search_works_at_two_characters` · `exact_spelling_outranks_a_folded_variant` · `a_single_letter_query_does_not_match_a_vocalised_name` · `fts_matches_english_and_sku` · `unicode_names_roundtrip_through_db_and_fts` (E.41) · `fts_survives_product_update` · `fts_row_removed_on_tombstone` · `embedded_rule_requires_verified_trade_scale` · `scale_verification_evidence_is_append_only` · `status_loss_disables_an_active_embedded_rule`
**Done when:** `just verify-schema` applies `0001`–`0007` and `cargo nextest run -p pos-db --test migration_0007_search_and_seed` exits zero after proving both FTS trigger coverage and fail-closed trade-scale activation.

### 1.2.6 — Assert FTS5 exists at open
**Files:** `crates/pos-db/src/lib.rs`
`rusqlite` has no `fts5` feature flag; FTS5 arrives through the bundled build, and this project builds SQLCipher. Verify rather than hope:
```rust
fn assert_fts5(conn: &Connection) -> Result<(), DbError> {
    let has: i64 = conn.query_row(
        "SELECT count(*) FROM pragma_compile_options WHERE compile_options = 'ENABLE_FTS5'",
        [], |r| r.get(0))?;
    if has == 0 { return Err(DbError::MissingFeature("FTS5")); }
    Ok(())
}
```
**Tests:** `open_asserts_fts5_available`
**Done when:** a build without FTS5 fails at `open()` with a named error, not at the first search with empty results.

### 1.2.7 — Search benchmark
*Gap G-9.*
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-db/benches/search.rs` (new), `crates/pos-db/Cargo.toml` (`criterion`), `benchmarks/baselines/search.json` (new)
Seed 50 000 products with Arabic names and benchmark `search()` at 1, 2, 3 and 5 characters under the reference-register sample and variance policy in conventions §7.
**Tests:** `search_benchmark_fixture_contains_50000_products` · `search_benchmark_exercises_every_query_length`
**Done when:** `just bench-gate search` exits zero with p99 below `50 ms` and a slower fixture proves the gate exits non-zero under the shared regression rule.

### 1.2.8 — Fuzz the scan parser
**Files:** `justfile`, `crates/pos-domain/fuzz/Cargo.toml` (new), `crates/pos-domain/fuzz/fuzz_targets/parse_scan.rs` (new), `crates/pos-domain/fuzz/corpus/parse_scan/` (new)

```rust
fuzz_target!(|input: &[u8]| {
    let rules = bounded_fixture_rules();
    let _ = parse_scan_bytes(input, &rules);
});
```

The target feeds arbitrary bytes and lengths through the scanner boundary and asserts termination without panic. Seed it with every plain, weight-embedded, price-embedded and corrupt code in the Phase-1 fixture; a discovered crash input becomes a permanent corpus entry because the same damaged label can return tomorrow.
**Tests:** `parse_scan_fuzz_seed_corpus_never_panics`
**Done when:** from `crates/pos-domain`, `cargo fuzz run parse_scan -- -runs=100000` exits zero over the committed corpus and arbitrary inputs.

---

## Group 1.3 — Tax engine

*Everything in [`ref/tax-jordan.md`](ref/tax-jordan.md). Right before the first real sale, or never.*

### 1.3.1 — Tax types
**Files:** `crates/pos-domain/src/tax.rs` (new)
`TaxTreatment`, `TaxBasis`, `TaxBase`, `TaxComponent`, `PriceMode`, `StoreTaxProfile`, `SupplyTaxContext`, `LineTax`, `ComponentTax`, `TaxSummaryRow`, `TaxRateRule` and `TaxError` per API reference §5. Fixed and ad-valorem components, dependency order, charge unit and immutable supply evidence are types rather than optional comments, because GST-on-SST and a supply-specific zero rate cannot be reconstructed from one percentage later.

### 1.3.2 — `resolve_components`
**Files:** `crates/pos-domain/src/tax.rs`
`valid_from` inclusive, `valid_to` exclusive; scoped rules override unscoped; overlap and absence are both errors, never a guessed 16%.
**Tests:** `prop_rate_resolution_is_deterministic_at_boundaries` · `overlapping_rules_error` · `no_rule_in_effect_errors` · `scoped_rule_overrides_unscoped`

### 1.3.3 — `compute_line_tax`, exclusive mode
**Files:** `crates/pos-domain/src/tax.rs`
Simpler direction first: `tax = net × r`, one rounding.
**Tests:** `exclusive_16pct_adds_exactly`

### 1.3.4 — `compute_line_tax`, inclusive mode
**Files:** `crates/pos-domain/src/tax.rs`, `crates/pos-db/src/tax_policy.rs` (new), `apps/terminal/src-tauri/src/provisioning.rs`, `apps/terminal/src-tauri/src/commands/sale.rs`
`net = gross / (1+r)`, then **`tax = gross − net` as a residual** — never rounded independently, or a receipt can fail to add up.
The existing OPEN item in [`ref/schema.md`](ref/schema.md) §0003 owns the authoritative Jordan tie rule and cash-rounding treatment. `HalfAwayFromZero` is a provisional implementation vector only: until the official source settles the question and an approved, versioned computation-policy row records its source and hash, store provisioning and sale finalization remain blocked. A merchant setting cannot choose a different result on another register.
**Tests:** `inclusive_16pct_extracts_exactly` (the 1250 → 1078 + 172 worked example) · `inclusive_extraction_is_exact_over_the_whole_fils_range` (an exhaustive arithmetic-vector loop over `{0, 1, 2, 4, 5, 10, 16}%` and gross `1..=1_000_000`, not a claim that every band is enabled for a merchant) · `prop_inclusive_net_plus_tax_equals_gross` · `prop_tax_never_exceeds_gross` · `unapproved_tax_computation_policy_blocks_provisioning_and_finalize`
**Done when:** `cargo nextest run -p pos-domain inclusive_extraction_is_exact_over_the_whole_fils_range && cargo nextest run -p pos-db tax_policy:: && cargo nextest run -p terminal unapproved_tax_computation_policy_` exits zero after checking all seven vector rates and every gross `1..=1_000_000`, and both provisioning and finalize refuse the missing-policy fixture.

### 1.3.5 — Multiple components per line
**Files:** `crates/pos-domain/src/tax.rs`
Represent ordered ad-valorem and fixed-per-quantity components, their base dependency, taxable quantity and carried component base. GST-on-SST cannot be represented as two percentages over one base, and retrofitting the component snapshot through live sale lines would be a migration of every receipt and refund. No SST rule is seeded until the official regulation and merchant tax-point evidence settle it; the engine fails closed meanwhile.
**Tests:** `prop_multi_component_line_sums_correctly` · `prop_per_unit_component_scales_with_quantity` · `sst_fixed_and_ad_valorem_components_compound_in_order` · `gst_base_includes_sst` · `tax_component_dependency_cycle_is_refused` · `an_incomplete_profile_pack_fails_closed`
**Done when:** `cargo nextest run -p pos-domain tax::tests::` exits zero, including the fixed-plus-ad-valorem compound fixture and the dependency-cycle refusal.

### 1.3.6 — `summarize_tax`
**Files:** `crates/pos-domain/src/tax.rs`
Grouped by `(component, treatment, rate)`. The **exact sum** of line taxes, never re-derived.
**Tests:** `prop_line_tax_sum_equals_receipt_tax` · `prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting`
**Done when:** exempt and zero-rated items on one receipt produce two summary rows, not one.

### 1.3.7 — Seed the Jordanian tax data
**Files:** `crates/pos-db/src/tax_pack.rs` (new), `crates/pos-db/tests/tax_pack.rs` (new)
Migration `0003` creates and seeds only the closed structural treatment/category vocabulary in 1.2.1; this later step never edits that migration. Every enabled band, including the standard band, arrives through one pinned pack carrying the official source version and hash, real effective dates and dated accountant approval:

```rust
pub fn import_tax_pack(tx: &Transaction, pack: &PinnedTaxPack,
                       approval: &AccountantApproval) -> Result<TaxPackVersion, DbError>;
```

Do not hard-code `STD16` or `RED04` as an unevidenced regulatory row, invent `valid_from` from the store's go-live date, or let a generic rule fall through to ASEZ/development profiles. `Zero` and `Exempt` remain distinct treatments, but an actual zero/exempt category still needs the pack's classification evidence. An unknown category, an unconfigured band or an incomplete jurisdiction pack fails closed because substituting a familiar percentage silently overcharges the customer.
The existing OPEN item in [`ref/schema.md`](ref/schema.md) §0003 owns which current categories and fixed/percentage components the merchant may enable; until its official catalogue and accountant-approved classification arrive, the default is an empty regulatory pack that cannot price a taxable live sale.
**Tests:** `imported_rule_records_source_version_hash_and_approval` · `standard_rate_records_the_same_pack_evidence_as_reduced_rates` · `unconfigured_reduced_band_fails_closed` · `asez_profile_without_complete_pack_fails_closed` · `fixture_covers_every_enabled_band`
**Done when:** `cargo nextest run -p pos-db --test tax_pack` passes against a fixture containing every merchant-enabled band and fails after any one band or approval record is removed.

### 1.3.8 — `unregistered` profile short-circuit
**Files:** `crates/pos-domain/src/tax.rs`

```rust
pub fn compute_line_tax(
    taxable: Money, qty: Qty, mode: PriceMode, components: &[TaxComponent],
    profile: StoreTaxProfile, supply: &SupplyTaxContext, rule: RoundingRule,
) -> Result<LineTax, TaxError>;
```

This controls GST calculation only. JoFotara obligation and its income/general/special taxpayer category are separate evidenced settings; being GST-unregistered never disables fiscal issuance by implication.
**Tests:** `prop_unregistered_profile_yields_no_tax` · `unregistered_gst_profile_does_not_disable_fiscal_obligation`
**Done when:** `cargo nextest run -p pos-domain unregistered_` passes with independent GST and fiscal-profile fixtures.

---

## Group 1.4 — The cart state machine

*Blueprint §8 as a Rust enum. Illegal transitions do not compile.*

### 1.4.1 — The `Sale` enum and `Cart` / `CartLine`
**Files:** `crates/pos-domain/src/cart.rs` (new)
Per API reference §6. `shift_id` is non-optional, because `business_date` has no meaning without its shift. `SupplyTaxContext` is captured as `Domestic` in Phase 1; non-domestic supplies remain refused until their reason and evidence path exists. `is_training` is on the cart from the first commit and checked by every report and later fiscal path.

### 1.4.2 — `CartError`
**Files:** `crates/pos-domain/src/cart.rs`
Every variant from API reference §6.3. Exhaustive and data-carrying — the UI renders from `code`, not by parsing a message.

### 1.4.3 — `add_line`, `set_qty`, `void_line`
**Files:** `crates/pos-domain/src/cart.rs`
`add_line` copies `name_snapshot`, the resolved `unit_price` and `price_origin` onto the line (I-5). It takes no caller-selected amount, refuses inactive products for *adding* while leaving refunds unaffected (E.38), and inserts an age-restricted line only in a visibly blocked state until 1.4.3b confirms it. Tender refuses any blocked line. Keeping the pending item in the cart makes the exact scanned pack and quantity visible while the cashier checks evidence; silently rejecting it would lose that context and invite a second scan.
**Tests:** `add_line_snapshots_name_and_price` · `client_supplied_price_cannot_bypass_override` · `inactive_product_cannot_be_added_but_can_be_refunded` · `age_restricted_line_requires_confirmation` · `set_qty_zero_is_rejected` · `prop_discrete_products_never_carry_a_fractional_quantity`

### 1.4.3b — Age confirmation
**Files:** `crates/pos-domain/src/cart.rs`, `apps/terminal/src-tauri/src/commands/cart.rs`

```rust
pub fn confirm_age(cart: Cart, line_id: SaleLineId, confirmed: bool,
                   actor: UserId) -> Result<(Cart, AuditIntent), CartError>;
```

An age-restricted line remains visibly blocked and `begin_tender` refuses the cart until an operator confirms it. A decline removes the line and audits the actor. Keeping the blocked line gives `cart_confirm_age { line_id, confirmed }` a stable target across a rerender; rejecting before insertion would expose a command whose `line_id` could never exist.
**Tests:** `age_restricted_line_blocks_tender_until_confirmation` · `age_confirmation_is_audited` · `age_confirmation_for_a_different_line_is_refused` · `age_decline_removes_line_and_audits` · `unconfirmed_age_line_survives_cart_rerender`
**Done when:** `cargo nextest run -p pos-domain cart::tests::age_` passes and the seeded sealed-pack tobacco line reaches `Tendering` only after an audit intent identifies the confirming actor.

### 1.4.4 — `park` / `resume`
**Files:** `crates/pos-domain/src/cart.rs`
**Tests:** `prop_park_resume_roundtrip_is_identity` (E.3)

### 1.4.5 — Discounts
**Files:** `crates/pos-domain/src/pricing.rs` (new)
`LineDiscount`, `BasketDiscount`, `DiscountRequest`, `DiscountAttribution`. Manual discounts are permission-scoped with a per-role percentage cap.
**Tests:** `prop_discount_never_makes_a_line_negative` (E.19) · `discount_above_role_cap_is_denied`

### 1.4.6 — Basket-discount proration
**Files:** `crates/pos-domain/src/pricing.rs`
`Money::split_proportional_by` prorates the exact amount by line value, producing one `DiscountAttribution` per line. The largest-remainder tie-break sorts immutable business content `(tax_category_id, product_id, unit_price_minor, qty_milli)`, never line position or a generated `line_id`; exact duplicate lines are observationally interchangeable. A document-level recap is the exact sum of line allowances. Entered percentages remain provenance only and never decide whether a correct basket may be sold or submitted.
**Tests:** `prop_basket_discount_prorates_to_the_fil` · `prop_proration_is_invariant_under_line_reordering` · `prop_proration_is_invariant_under_fresh_line_ids`; fiscal recap ownership belongs to 2.7.3 because a cart has no UBL document recap.
**Done when:** `cargo nextest run -p pos-domain pricing::tests::prop_` passes with every generated line-allowance sum equal to the requested basket discount and its recap.

### 1.4.7 — Price override
**Files:** `crates/pos-domain/src/pricing.rs`
Requires `Authorized<cap::PriceOverride>`, a reason code, and respects a floor (cost, or cost + x%) and a ceiling.
**Reasons ship with a dedicated `displayed_price` variant** — Jordan's ministry inspects price display, and "the shelf tag says 0.99" must be a one-tap, always-audited action that also feeds the label-reprint worklist (J.3, E.70).
**Tests:** `override_below_floor_is_denied` · `sale_above_ceiling_is_hard_blocked` (E.71) · `displayed_price_override_queues_a_label_reprint`

### 1.4.8 — Tendering transitions
**Files:** `crates/pos-domain/src/cart.rs`
`begin_tender`, `back_to_building` (only with zero tenders collected), `add_tender`, `remove_tender`, `begin_finalize`, `complete`, `void_sale`.
**Tests:** `back_to_building_denied_after_first_tender` · `complete_requires_settled` · `prop_no_operation_mutates_a_complete_sale` (I-4)

### 1.4.9 — `price_cart`
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-domain/src/cart.rs`, `crates/pos-domain/benches/price_cart.rs` (new), `crates/pos-domain/Cargo.toml` (`criterion`), `benchmarks/baselines/price-cart.json` (new)
The one function that turns a cart into money — the sole source of every number on the receipt *and* in the fiscal document.
**Tests:** `prop_total_equals_lines_minus_discounts_plus_tax` · `prop_price_cart_is_deterministic` · `prop_price_cart_is_invariant_under_line_reordering` (E.19b) · `prop_zero_total_cart_is_valid` (E.18)
**Bench:** `crates/pos-domain/benches/price_cart.rs` — 200 lines, p99 < 16 ms (G-9).
**Done when:** `cargo nextest run -p pos-domain cart::tests::prop_ && just bench-gate price-cart` exits zero for the stable 200-line fixture on the reference register.

### 1.4.10 — `AuditIntent` emission
**Files:** `crates/pos-domain/src/cart.rs`, `crates/pos-domain/src/audit.rs`
Every money-reversing transition returns `(NewState, AuditIntent)`. A pure function cannot write a row; it returns the intent and the shell persists it in the same transaction.
**Tests:** `every_privileged_transition_returns_an_audit_intent` — an exhaustive match over the transition list, so adding one without an intent fails to compile.

### 1.4.11 — Phase-1 cart IPC boundary
**Files:** `apps/terminal/src-tauri/src/commands/cart.rs` (new), `apps/terminal/src-tauri/src/ipc/registry.rs`, `apps/terminal/src-tauri/tests/ipc_contract.rs`

```text
cart_add_line            { product_id, qty_milli? }
cart_add_scan            { raw_code }
cart_confirm_age         { line_id, confirmed }
cart_add_department_sale { department_id, amount_minor, scanned_code?, note?, approval_id? }
```

`cart_add_line` has no `unit_price_minor`. Catalogue prices come from `PriceSource::from_catalog`; price-embedded labels flow only through `cart_add_scan`; deliberate changes flow through `cart_override_price` under `price.override`. The department command is a capped, audited fallback: a cashier proceeds at or below `DepartmentPolicy.escalate_above`, a higher amount requires a bound `ApprovalHandle`, and anything above `max_line_amount` is refused. This keeps an unknown code sellable without letting an optional handle become a wildcard for arbitrary open prices.
**Tests:** `no_command_argument_carries_a_price`; 1.6.7 owns the registry-wide approval and conditional-threshold assertions.
**Done when:** `cargo nextest run -p terminal --test ipc_contract` passes and the generated schema for `cart_add_line` contains no price argument while the department command's conditional branch refuses a missing, mismatched or already-consumed approval.

### 1.4.12 — Department sale
**Files:** `crates/pos-domain/src/catalog.rs`, `crates/pos-domain/src/cart.rs`, `apps/terminal/src-tauri/src/commands/cart.rs`

```rust
pub fn add_department_line(cart: Cart, req: AddLine,
                           policy: &DepartmentPolicy,
                           auth: &Authorized<cap::DepartmentSale>,
                           approval: Option<&ApprovalHandle>,
                           actor: UserId, now: Timestamp)
    -> Result<(Cart, AuditIntent), CartError>;
```

A department sale is a category-backed, tax-classified line with `product_id = None`, quantity one, no stock event, a configured escalation threshold and hard amount cap, and an audit row. `sale.department` is granted to the cashier by default so an unknown barcode at 22:00 does not become a manager wait. Above `escalate_above`, the pure transition derives `ApprovalBinding { entity_id: cart.id, amount_minor: req.amount_minor, content_hash: None }` and calls `ApprovalHandle::matches(actor, binding, now)`; the command consumes that same handle with the durable cart effect and audit row. Approval never bypasses `max_line_amount`.
**Tests:** `department_line_carries_its_department_tax_category` · `department_line_is_marked_non_stock` · `department_line_is_always_audited` · `department_above_cap_is_refused` · `department_above_escalation_threshold_requires_approval` · `add_line_refuses_a_department_price_source` · `queue_never_stalls_on_unknown_code`
**Done when:** `cargo nextest run -p pos-domain department_ && cargo nextest run -p terminal queue_never_stalls_on_unknown_code` passes with quantity one, `product_id = None`, the selected department tax category and a returned audit intent.

---

## Group 1.5 — Cash tenders

### 1.5.1 — `TenderType` and `Tender`
**Files:** `crates/pos-domain/src/tender.rs` (new)
Per API reference §7. `TenderState` includes `Pending` from day one so Phase 2's CliQ callback case (E.65) is a state, not a schema change.

### 1.5.2 — `remaining_due`, `change_due`, `is_settled`
**Files:** `crates/pos-domain/src/tender.rs`
Split tender is the **core model**, not a feature: a sale holds `Vec<Tender>` until collected ≥ due. Bolting it on later deforms the whole checkout (master plan C.4).
**Tests:** `prop_split_tender_sums_to_total` · `prop_change_never_negative` · `overtender_only_allowed_for_cash`

### 1.5.3 — Cash rounding
**Files:** `crates/pos-domain/src/tender.rs`
`compute_cash_rounding`, applied only when the **final** tender is cash and only to the remaining cash amount. The existing cash-rounding-treatment OPEN item in [`ref/tax-jordan.md`](ref/tax-jordan.md) §5 keeps the default tender-level adjustment provisional; the POS does not silently move it into a tax base or fiscal line.
**Tests:** `prop_cash_rounding_only_on_final_cash_tender` (E.14) · `prop_rounding_adjustment_keeps_total_exact` · `card_charged_exact_unrounded_total` · `mixed_tender_1247_card_624_cash_620_adjustment_minus_3` · `half_away_tie_1245_rounds_to_1250` · `cash_overtender_and_change_are_separate_from_rounding`
**Done when:** `cargo nextest run -p pos-domain mixed_tender_1247_card_624_cash_620_adjustment_minus_3` exits zero, proving that a `1.247` JOD sale charged `0.624` to card leaves `0.623`, rounds the final cash tender to `0.620`, records `-0.003`, and settles `1.244` exactly.

### 1.5.4 — Denomination helper
**Files:** `crates/pos-domain/src/tender.rs`
JOD denominations (50, 20, 10, 5, 1 dinar; 500, 250, 100, 50, 25, 10 fils) for the numpad quick-keys and the shift-close count grid.
**Tests:** `denominations_are_descending_and_complete`

---

## Group 1.6 — Users, permissions, audit

*Gaps G-6 and G-7. Independent of the cart work.*

> **Build order inside this group is not step order.** Build the pure PIN hash/verify/benchmark
> half of 1.6.2 and define the capability matrix in 1.6.3, then commit migration `0004` in 1.6.1
> with that matrix. Complete the database tests in 1.6.3 and the approval machinery in 1.6.4
> before adding `user_reset_pin` from 1.6.2. The command cannot bind an `ApprovalHandle` that has
> not been implemented yet, and the migration cannot be reopened later to add its role seeds.

### 1.6.1 — Migration `0004`
**Files:** `crates/pos-db/migrations/0004_people_and_audit.sql`
Per [`ref/schema.md`](ref/schema.md) §0004, including the complete role/capability matrix defined by 1.6.3. Design that matrix first and land its seed here, because adding it to `0004` after this microstep commits would violate the forward-only migration law. Note `app_user`, not `user` — reserved in Postgres.

### 1.6.2 — Argon2id PINs
**Scheduled in:** build hash/verify and its benchmark first; add the privileged reset command after 1.6.4
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-db/src/auth.rs` (new), `crates/pos-db/src/repo/user.rs` (new), `crates/pos-db/benches/pin_verify.rs` (new), `crates/pos-db/Cargo.toml` (`argon2`, `criterion`), `apps/terminal/src-tauri/src/commands/user.rs` (new), `apps/terminal/src-tauri/src/ipc/registry.rs`, `benchmarks/baselines/pin-verify.json` (new)
```rust
pub fn hash_pin(pin: &str) -> Result<String, AuthError>;              // PHC string
pub fn verify_pin(pin: &str, hash: &str) -> Result<bool, AuthError>;  // constant-time
```

```text
user_reset_pin { user_id, new_pin, approval_id } -> ()
```

Named Argon2 parameters are reviewed against the reference register, and failed-attempt state persists across restart. The minimum manager-approval PIN defaults to six digits until the merchant answers the existing security open item; weakening the profile or restarting to clear throttling would turn a copied database into an offline guessing oracle.
Reset requires `user.admin` and a bound approval, retires the old PHC hash, clears no unrelated audit or throttle history, and commits its audit row with the replacement. Otherwise “forgot PIN” becomes either an unrecoverable cashier account or an unaudited credential takeover.
**Tests:** `hash_verify_roundtrip` · `wrong_pin_rejected` · `hash_is_salted_and_differs_per_call` · `argon2_parameters_match_the_reviewed_profile` · `failed_attempt_state_survives_restart` · `manager_reset_retires_old_hash_and_audits` (E.51; the same fixture refuses a short manager PIN, clears a lockout only through the audited manager path, and retires the old hash)
**Bench:** `crates/pos-db/benches/pin_verify.rs` measures the reviewed target under conventions §7; no wall-clock assertion lives in a unit test.
**Done when:** `cargo nextest run -p pos-db auth:: && cargo nextest run -p terminal manager_reset_ && just bench-gate pin-verify` passes the named reset/audit and Argon2 assertions plus the 200–350 ms median / 500 ms p99 reference-register budget; 1.6.8 proves neither PIN nor PHC hash reaches a sink.

### 1.6.3 — Capabilities and the default role matrix
**Scheduled in:** define the matrix before 1.6.1; the immutable seed lands only in migration `0004`, while this step implements and verifies the domain view
**Files:** `crates/pos-domain/src/permissions.rs` (new), `crates/pos-db/tests/role_matrix.rs` (new)
The `cap` module and normative default matrix from API reference §8 are seeded by 1.6.1 as `role` + `role_capability` rows. This step does not reopen the migration. The test iterates `cap::ALL`; a hard-coded capability count or the bundled source-plan table already drifted and cannot prove a newly added capability was deliberately granted or denied.
**Tests:** `default_matrix_covers_every_capability_in_cap_all` · `journal_view_is_scoped_to_the_holders_own_shift_without_reports_all` · `customer_lookup_refuses_a_prefix_query`
**Done when:** `cargo nextest run -p pos-domain permissions:: && cargo nextest run -p pos-db --test role_matrix` passes after comparing every `cap::ALL` entry with all four roles.

### 1.6.4 — `Authorized<C>` and `authorize`
**Files:** `crates/pos-domain/src/permissions.rs`, `crates/pos-db/src/repo/approval.rs` (new), `crates/pos-db/tests/approval.rs` (new), `apps/terminal/src-tauri/src/commands/auth.rs` (new)
The proof-carrying token, as a **marker type** — `Authorized<C: Capability>` with a private `PhantomData<fn() -> C>`. Not `Authorized<const C: &'static str>`, which does not compile on any stable rustc: `&'static str` is forbidden as the type of a const generic parameter. Resist "fixing" it with a runtime `&str` field, which throws away the property the design exists for.

`authorize` is the only way to obtain a `&Authorized<C>`, and privileged domain functions require one. Runtime escalation adds a persisted one-use handle:

```rust
pub struct ApprovalHandle {
    id: ApprovalId, capability: String, actor: UserId, approver: UserId,
    entity_id: Uuid, amount_minor: i64,
    content_hash: Option<PreparedIntentHash>, reason: String,
    issued_at: Timestamp, expires_at: Timestamp, nonce: [u8; 16],
}

impl ApprovalHandle {
    pub fn issue<C: Capability>(
        id: ApprovalId, actor: UserId, approver: &Authorized<C>,
        binding: &ApprovalBinding, reason: String, now: Timestamp,
        ttl_ms: i64, nonce: [u8; 16],
    ) -> Result<Self, PermissionError>;
    pub fn matches<C: Capability>(&self, actor: UserId,
        binding: &ApprovalBinding, now: Timestamp) -> Result<(), PermissionError>;
}

pub fn consume_approval(tx: &Transaction, id: ApprovalId,
                        binding: &ApprovalBinding, audit_id: Uuid)
    -> Result<(), PermissionError>;
```

Every always-privileged IPC command accepts `approval_id`; a conditionally privileged command accepts `approval_id?` and refuses its privileged branch when the handle is absent. The handler resolves the immutable handle, verifies capability, actor, entity, amount, optional prepared-content hash, reason and expiry, then inserts `approval_consumption` in the same transaction as the effect and audit row. `actor != approver` is unconditional on every handle path; `ban_self_approval` decides whether an operation needs escalation at all and never permits a self-issued handle. Private fields and the sole `issue` constructor stop Rust callers forging a larger amount or altered prepared intent before persistence. The handle itself remains evidence; deleting it would contradict the append-only audit design. JavaScript receives only `ApprovalRef`, never the nonce, content hash or a bearer proof.
**Tests:** `cashier_cannot_void_a_sale` · `an_actor_cannot_approve_their_own_handle` (E.52) · `deactivated_user_denied` · `a_handle_used_twice_is_refused` · `an_altered_amount_is_refused` · `a_different_sale_is_refused` · `a_different_actor_is_refused` · `a_consumed_handle_is_still_consumed_after_restart` · `an_expired_handle_is_refused` · `the_effect_and_the_consumption_commit_together_or_not_at_all`
**Done when:** `cargo nextest run -p pos-domain permissions:: && cargo nextest run -p pos-db --test approval` passes, including rollback of both the effect and consumption under an injected failure.

### 1.6.5 — Audit hash chain
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-domain/Cargo.toml`, `crates/pos-domain/src/audit.rs` (new)
`canonical_bytes(&CanonicalAuditEntry)` includes `register_id`, `seq` and `id`; `chain_hash` is BLAKE3 of `prev ‖ bytes`; `verify_chain` accepts the last `ChainAnchor`. The chain alone cannot detect deletion of its newest tail, so every verified backup exports the head and the verifier compares against that external anchor.
**Tests:** `golden_canonical_bytes_are_stable` · `prop_chain_detects_any_single_entry_mutation` · `prop_chain_detects_deletion_before_the_anchor` · `prop_chain_detects_tail_deletion_against_an_anchor` · `prop_chain_detects_reordering` · `mutating_an_identity_column_breaks_the_chain`
**Done when:** mutating any byte of any historical entry makes `verify_chain` return `Broken { at_seq }` pointing at it.

### 1.6.6 — `AuditRepository`
**Files:** `crates/pos-db/src/repo/audit.rs` (new)
Append-only. Reads the previous hash and writes the new row **inside the caller's transaction**. There is no update method and no delete method — not private ones, none.
**Tests:** `chain_survives_process_restart` · `concurrent_appends_serialize` · `verify_chain_over_1000_entries`

### 1.6.6b — Local audit verifier
**Files:** `crates/pos-db/src/bin/verify-audit.rs` (new), `crates/pos-db/tests/audit_verifier.rs` (new)

```text
verify-audit --database <copy> [--anchor <backup-manifest>]
```

The command opens an explicitly named database through the normal key provider, walks `CanonicalAuditEntry` rows and reports `Intact` or `Broken { at_seq }`. It exists in Phase 1 because the exit gate must prove tamper detection before a merchant's register is the only copy; waiting until Phase 5 would leave the Phase-1 claim untestable.
**Tests:** `verifier_reports_the_first_broken_sequence` · `tail_deletion_is_detected_against_the_last_anchor` · `the_original_database_still_refuses_an_audit_update`
**Done when:** `cargo nextest run -p pos-db --test audit_verifier` passes and `cargo run -p pos-db --bin verify-audit -- --help` exits zero.

### 1.6.7 — Capability exhaustiveness test
**Files:** `apps/terminal/src-tauri/src/ipc/registry.rs` (new), `apps/terminal/src-tauri/tests/ipc_contract.rs` (new)
Every IPC command registers `(name, required_capability, audited, approval_requirement)`, where `approval_requirement` is `Never`, `Always { binding }`, or `Conditional { predicate, binding }`. A binding is mandatory for both privileged forms; otherwise the registry could say a command needs approval without saying which entity and amount the handle must match. The tests walk `tauri::generate_handler!`'s list and fail on an absent command, an incomplete privileged entry, or a conditional command whose generated schema omits `approval_id?`.
**Tests:** `ipc_commands_all_declare_a_capability` · `every_privileged_command_binds_its_approval` · `conditional_privilege_cannot_cross_threshold_without_approval` · `a_registry_fixture_without_a_command_spec_is_refused` · `a_privileged_fixture_without_a_binding_is_refused`
**Done when:** `cargo nextest run -p terminal --test ipc_contract` exits zero, including all negative registry fixtures and the department threshold boundary.

### 1.6.8 — PII scrubbing in the log layer
*Gap G-8.*
**Files:** `apps/terminal/src-tauri/src/telemetry.rs` (new), `apps/terminal/src-tauri/src/ipc/error.rs` (new)
Define one `pub const SENSITIVE_FIELD_RULES` registry carrying these exact names: `pin`, `pin_hash`, `pan`, `card_number`, `cvv`, `track`, `phone`, `email`, `customer_name`, `buyer_name`, `secret_key`, `client_id`, `db_key`, `token`, `password`, `entitlement`, `recovery_code`, `enrollment_code`, `wrapped_key`. It also carries suffix rules for `_token`, `_secret`, `_key`, `_pin`, `_hash` and a contains rule for `password`, exactly as [`ref/security-compliance.md`](ref/security-compliance.md) §6 specifies. The tracing layer, audit-payload assertion, diagnostic bundle, telemetry transport and parameterized tests all iterate it; none is copied into a second hand-maintained list. `IpcError.detail` is static and typed, while the scrubbed source error is correlated by `trace_id`.
Microstep 3.9.1 owns the Sentry-bound `no_pii_in_a_captured_panic` test.
**Tests:** `scrubber_redacts_every_known_pii_field` · `scrubber_redacts_nested_json` · `scrubber_redacts_every_suffix_rule` · `no_pii_in_a_full_sale_trace` · `ipc_errors_carry_no_source_detail_in_release`
**Done when:** `cargo nextest run -p terminal telemetry::tests:: && cargo nextest run -p terminal ipc_errors_` passes with every scrubber, full-trace and panic test generated from `SENSITIVE_FIELD_RULES`.

---

## Group 1.7 — Receipts

*The Arabic problem. Get it right once and every later document format inherits it.*

### 1.7.1 — `ReceiptModel`
**Files:** `crates/pos-domain/src/receipt.rs` (new)
Per API reference §13. The ESC/POS rasteriser, the PDF renderer, and the email renderer all consume this — so an emailed receipt can never disagree with the printed one.

### 1.7.2 — Font decision and embedding
*Gap G-5.*
**Scheduled in:** embed, license and prove the raster font at 1.7.2; wire the UI to the same asset in 1.11.1
**Files:** `assets/fonts/` (new), `crates/pos-hardware/Cargo.toml`, `crates/pos-hardware/tests/font_asset.rs` (new)
One family covering Arabic and Latin, embeddable, licence-clear, shipped with the app — **no network font**. The same file feeds the UI and the receipt rasteriser so the receipt looks like the screen. Candidates: Noto Sans Arabic, IBM Plex Sans Arabic, Cairo.
**Tests:** `embedded_font_has_a_repository_licence` · `embedded_font_bytes_are_not_empty`
**Done when:** `cargo nextest run -p pos-hardware --test font_asset` exits zero with the font and its licence in the repository; 1.7.3 owns raster loading and 1.11.1 owns the later UI-path equality check.

### 1.7.3 — The raster pipeline
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-hardware/Cargo.toml`, `crates/pos-hardware/src/render/mod.rs` (new), `layout.rs`, `raster.rs`
```
ReceiptModel → layout engine (boxes, RTL runs, columns)
             → cosmic-text shaping (rustybuzz under it: Arabic joining + bidi)
             → tiny-skia 1-bit bitmap at printer width (576 px @ 80 mm, 384 @ 58 mm)
             → GS v 0 raster bytes
```
Do not fight printer codepages. Windows-1256 text mode does not shape Arabic letters or reorder RTL runs; the field consensus is rasterisation and it is also the only way bilingual mixing looks correct.
**Tests:** `rasterizer_loads_the_embedded_font` · `arabic_joining_uses_contextual_glyphs` · `rtl_run_orders_visual_positions` · `layout_wraps_long_arabic_names` · `layout_columns_align_in_rtl` · `raster_width_matches_profile` · `narrow_profile_reflows_rather_than_truncates` (E.49)
**Done when:** `cargo nextest run -p pos-hardware render::tests::` exits zero for the embedded font, contextual Arabic glyphs, RTL run order and both raster widths; 1.7.5 owns the dated native-reader review of the viewable golden.

### 1.7.4 — ESC/POS emitter
**Files:** `crates/pos-hardware/src/escpos.rs` (new)
`ESC @` init, `GS v 0` raster and `GS V` cut. The `ESC p` drawer pulse is a separate audited hardware effect, never part of printable bytes and never replayed when a receipt job retries. Two width profiles, 80 mm and 58 mm (E.49).
**Tests:** `escpos_init_cut_bytes_are_exact` · `receipt_retry_never_repeats_the_drawer_pulse`

### 1.7.5 — Golden receipts
**Files:** `crates/pos-hardware/tests/golden/` (new: paired `.bin` and `.png` files), `scripts/check-golden-review.py` (new), `docs/drills/` review record
Seven fixtures: Arabic 80 mm · Arabic 58 mm · bilingual 80 mm · multi-rate tax summary · duplicate watermark · training watermark · B2B buyer block. The `.bin` proves printer bytes; the renderer-generated `.png` makes Arabic joining, bidi order and layout reviewable in a pull-request diff.
**Tests:** `golden_receipts_are_byte_stable` · `golden_receipt_ar_80mm` (E.41) · `golden_receipt_ar_58mm` (E.49) · `every_binary_receipt_golden_has_a_png_projection` · `each_golden_png_is_the_rasterisation_of_its_bin`
**Done when:** `cargo nextest run -p pos-hardware golden_ && python3 scripts/check-golden-review.py` exits zero, proving every `.bin` has a same-stem `.png` and every changed Arabic/bilingual pair has a dated `docs/drills/` record naming the commit and native reader. A hexdump cannot show a lost medial form.

### 1.7.6 — Printer status before finalize
**Files:** `crates/pos-hardware/src/lib.rs`
`status()` is polled **at Pay, before money is taken** (master plan C.15). Paper-out warns then, not after the customer has paid.
**Tests:** `paper_out_warns_before_tender_not_after`

### 1.7.6b — Printer-unavailable operating mode
**Scheduled in:** Phase 1B, after 1.7.7 and 1.8.3
**Files:** `apps/terminal/src-tauri/src/print_queue.rs` (new), `apps/terminal/src-tauri/src/health.rs` (new), `crates/pos-db/src/repo/receipt.rs`

```rust
pub fn enqueue_unprinted(tx: &Transaction, receipts: &ReceiptRepository<'_>,
                         artifact: &ReceiptArtifact, reason: PrinterUnavailable)
    -> Result<Uuid, PrintQueueError>;
```

At Pay, warn before tender when the printer is unavailable. The shell owns the transaction and delegates the same unconditional initial job write as every sale to `ReceiptRepository::queue_initial`; `reason` drives the health alarm and does not invent a separate print-job schema. The shell never writes SQL or constructs persistence rows itself. Finalize still persists the immutable receipt artifact and queued print job, then raises an operator alarm; selling never invents a printed receipt or loses the document. The unresolved fiscally-enabled-store rule remains owned by `2.7.0` in [`ref/test-catalog.md`](ref/test-catalog.md) E.85.
**Tests:** `printer_unavailable_at_pay_warns_before_tender` · `a_sale_completes_with_no_printer_and_queues_the_artifact` · `the_missing_printer_is_an_alarm_not_a_modal` · `a_queued_artifact_prints_unchanged_once_a_printer_returns`
**Done when:** `cargo nextest run -p terminal print_queue::` passes and a simulated offline printer leaves one completed sale, one original artifact, one queued job and no claimed print success.

### 1.7.7 — Print retry queue and unprinted flag
**Scheduled in:** Phase 1B, after 1.9.1 creates migration `0005` and before 1.8.3 consumes the repository
**Files:** `apps/terminal/src-tauri/src/print_queue.rs` (new), `crates/pos-db/src/repo/receipt.rs` (new)

```rust
pub fn append_artifact(&self, tx: &Transaction,
                       artifact: &ReceiptArtifact) -> Result<(), DbError>;
pub fn queue_initial(&self, tx: &Transaction,
                     artifact: &ReceiptArtifact) -> Result<Uuid, DbError>;
pub fn lease_next_job(&self, tx: &Transaction,
                      worker_id: Uuid) -> Result<Option<PrintJob>, DbError>;
pub fn append_attempt(&self, tx: &Transaction,
                      attempt: &PrintAttempt) -> Result<(), DbError>;
```

A normal finalize always appends one initial print job before the post-commit hardware attempt; printer health changes only the warning/alarm path. A print failure after finalize never mutates the completed sale. Migration `0005` stores immutable `receipt_artifact.content_bytes`, a leased mutable `print_job`, and append-only `print_attempt` outcomes. An `unknown` or partial hardware outcome never retries automatically, because replaying ambiguous bytes may print a second customer document or pulse a drawer twice. A requested duplicate is a new artifact linked to the original and visibly watermarked.
**Tests:** `every_finalized_sale_has_one_initial_print_job` · `print_failure_after_finalize_leaves_sale_complete` · `duplicate_artifact_links_the_original_and_adds_the_duplicate_watermark` · `an_unknown_print_outcome_never_auto_retries_the_drawer_pulse` · `queue_survives_restart` · `successful_print_appends_an_attempt_without_updating_sale`
**Done when:** `cargo nextest run -p terminal print_queue::` passes after a restart with the original bytes, job state and attempt history unchanged.

### 1.7.8 — Simulator fault injection
**Files:** `crates/pos-hardware/src/lib.rs`
Extend `SimulatedPrinter` with scripted faults: paper-out at byte N, cover-open, offline, slow. CI and demos run hardware-free (master plan C.15).
**Tests:** `simulator_fails_midway_when_scripted` — sweep every byte boundary of a golden and require one typed `PrintOutcome`, no drawer-count change and no second original artifact.

### 1.7.8b — Fuzz receipt layout input
**Files:** `crates/pos-hardware/fuzz/Cargo.toml` (new), `crates/pos-hardware/fuzz/fuzz_targets/receipt_layout.rs` (new), `crates/pos-hardware/fuzz/corpus/receipt_layout/` (new), `.github/workflows/security.yml`, `scripts/tests/fuzz_ci_contract_test.py` (new)

```rust
fuzz_target!(|input: ReceiptFuzzInput| { let _ = render_receipt(input.model); });
```

Feed arbitrary UTF-8, bidi controls, combining marks, long product names and extreme legal line counts through the layout and raster entry point. Catalogue text crosses no trusted language boundary before printing, so one malformed name must not panic or allocate without bound. A weekly security job runs this target and 1.2.8's `parse_scan` target for exactly 15 minutes each with their committed corpora; fuzzing stays out of the per-PR gate because a time-based search is not a deterministic merge check.
**Tests:** `receipt_layout_seed_corpus_covers_arabic_bidi_combining_and_long_lines` · `weekly_fuzz_job_runs_both_phase_1_targets_with_fixed_budgets`
**Done when:** from `crates/pos-hardware`, `cargo fuzz run receipt_layout -- -runs=100000` exits zero, `python3 scripts/tests/fuzz_ci_contract_test.py` proves the weekly job names both targets and fixed budgets, and every minimized crash input is committed to its corpus.

---

## Phase 1A intermediate gate — domain through the pure receipt pipeline

This is a schedule gate, not permission to trade. It makes a slip visible before finalize, backup and UI integration can hide whether the money and paper foundations are actually complete. The narrow storage exception is 1.8.9: schema guards require the shared commit-manifest writer before Phase-1A audit and approval facts can be tested honestly.

```bash
just lint && just test
just verify-schema
cargo nextest run -p pos-domain -E 'test(prop_)'
cargo nextest run -p pos-hardware
cargo nextest run -p pos-db outbox::
```

The gate passes only when every command exits zero, every paired receipt `.bin`/`.png` golden is clean, and the exact mixed-rate and `mixed_tender_1247_card_624_cash_620_adjustment_minus_3` fixtures pass. It does not include the storage-backed queue steps 1.7.6b/1.7.7. Build migrations in numeric order despite the document's domain-first reading order: execute `1.9.1`/`0005` and `1.10.1`/`0006` before committing `1.2.5`/`0007`.

---

## Group 1.8 — Persistence and the money moment

### 1.8.0 — Storage-engine prerequisite
**Files:** `crates/pos-db/src/lib.rs`, `crates/pos-db/src/storage_version.rs` (new)

```rust
pub struct StorageVersionPolicy {
    pub min_cipher_version: Version, pub min_sqlite_version: Version,
    pub source_hash: [u8; 32],
}
pub fn assert_storage_versions(conn: &Connection,
                               policy: &StorageVersionPolicy) -> Result<(), DbError>;
```

At database open, read and parse `PRAGMA cipher_version` and `sqlite_version()`, compare both with repository-reviewed minimum constants, and refuse an unsupported or unparseable build with `DbError::UnsupportedStorageVersion` **before** enabling WAL or opening another source connection. Report both versions in scrubbed diagnostics.

The documented `rusqlite`/`sqlx` `libsqlite3-sys` links collision prevents a straightforward dependency bump. Do not guess an upstream safe boundary: the existing SQLCipher/SQLite WAL-safety OPEN item in [`ref/plan-validation.md`](ref/plan-validation.md) blocks 1.8.1 and settles the two minimum constants from official advisories and upstream regression coverage. Until then, the default is one source connection; backup, reporting, checkpoint and later sync workers must not open concurrent source connections.
**Tests:** `open_reports_compiled_storage_versions` · `storage_minimum_policy_has_source_and_hash` · `open_refuses_cipher_below_the_reviewed_minimum` · `open_refuses_sqlite_below_the_reviewed_minimum` · `open_refuses_an_unparseable_storage_version` · `multi_connection_wal_is_disabled_until_the_storage_gate_closes`
**Done when:** `cargo nextest run -p pos-db storage_version::` exits zero, the pinned policy metadata names and hashes the source that set each minimum, and every injected lower or malformed version fails before WAL is enabled.

### 1.8.1 — Repository module and transaction discipline
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-db/Cargo.toml` (`trybuild`), `crates/pos-db/src/repo/mod.rs`, `crates/pos-db/tests/repository_transaction_api.rs` (new), `crates/pos-db/tests/ui/fact_write_without_transaction.rs` (new), `crates/pos-db/tests/ui/fact_write_without_transaction.stderr` (new)
Every write method takes `&Transaction`. The caller owns the boundary; that is how conventions I-9 stays true.
**Tests:** `every_fact_repository_write_requires_a_transaction`
**Done when:** `cargo nextest run -p pos-db --test repository_transaction_api` passes and its `trybuild` fixture rejects a fact write with only `&Connection`.

### 1.8.1b — Half-migrated database refusal
**Files:** `crates/pos-db/src/lib.rs`, `crates/pos-db/tests/migration_refusal.rs` (new)

```rust
pub fn verify_runtime_schema(conn: &Connection,
                             migrations: &[Migration]) -> Result<SchemaVersion, DbError>;
```

Persist and verify the runtime migration identity around each forward-only step. A file whose schema shape and `user_version` disagree refuses to open with the named recovery state; it never guesses which half committed, because running business writes against that ambiguity can make the next migration destroy facts.
**Tests:** `half_migrated_db_refuses_to_open_with_a_named_error` · `schema_from_a_newer_build_is_refused`
**Done when:** `cargo nextest run -p pos-db --test migration_refusal` exits zero for fixtures interrupted on both sides of the `user_version` update.

### 1.8.2 — `SaleRepository`
**Files:** `crates/pos-db/src/repo/sale.rs` (new), `crates/pos-db/tests/sale_repository_api.rs` (new), `crates/pos-db/tests/ui/completed_sale_update.rs` (new), `crates/pos-db/tests/ui/completed_sale_update.stderr` (new)
```rust
pub fn insert_complete(&self, tx: &Transaction, sale: &CompletedSale) -> Result<(), DbError>;
pub fn by_id(&self, id: SaleId) -> Result<Option<CompletedSale>, DbError>;
pub fn by_receipt_number(&self, r: &str) -> Result<Option<CompletedSale>, DbError>;
pub fn for_business_date(&self, store: StoreId, d: BusinessDate) -> Result<Vec<CompletedSale>, DbError>;
```
**There is no `update` and no `delete`.** Not private ones. Not "just for corrections."
The round-trip covers `shift_id`, `SupplyTaxContext`, component bases, tender transition projection, approval consumption, receipt-artifact identity and `sync_commit_id`; omitting a captured field makes a refund or reprint silently consult today's state.
**Tests:** `sale_repository_exposes_no_mutation` (a `trybuild` fixture) · `roundtrip_preserves_every_field`
**Done when:** `cargo nextest run -p pos-db --test sale_repository_api` exits zero and its compile-fail fixture proves no completed-sale update or delete API exists.

### 1.8.2b — Parked-cart repository
**Files:** `crates/pos-db/src/repo/parked_cart.rs` (new), `crates/pos-db/tests/parked_cart.rs` (new), `apps/terminal/src-tauri/src/commands/cart.rs`

```rust
pub fn park(&self, tx: &Transaction, cart: &BuildingCart,
            actor: UserId, now: Timestamp) -> Result<Uuid, DbError>;
pub fn resume(&self, tx: &Transaction, id: Uuid,
              actor: UserId, session_nonce: Uuid) -> Result<BuildingCart, DbError>;
pub fn save_active(&self, tx: &Transaction, id: Uuid,
                   session_nonce: Uuid, cart: &BuildingCart) -> Result<(), DbError>;
pub fn consume_on_finalize(&self, tx: &Transaction, id: Uuid,
                           session_nonce: Uuid) -> Result<(), DbError>;
```

Persist the complete building-cart snapshot under migration `0005`. Resume atomically claims the row as `active` under a fresh `session_nonce`; it never deletes the only durable copy before the IPC response. Each later cart mutation replaces that register-local working snapshot under the same nonce, re-park changes its state, and finalize removes it only inside the complete-sale transaction. Startup restores an `active` claim after a process death, while a second live session cannot claim it. A process restart cannot turn “park this customer” into a lost basket or two simultaneously resumable copies.
**Tests:** `parked_carts_survive_restart` (E.3) · `resume_claims_the_working_row_atomically` · `process_death_after_resume_commit_restores_the_active_cart` · `another_actor_cannot_resume_without_the_required_grant` · `finalize_consumes_the_working_cart_with_the_sale`
**Done when:** `cargo nextest run -p pos-db --test parked_cart` exits zero after restarts injected before resume commit, after commit but before the IPC response, after an active-cart mutation and during finalize; each fixture leaves exactly one current basket or one completed sale.

### 1.8.3 — The atomic finalize
**Files:** `apps/terminal/src-tauri/src/commands/sale.rs` (new)
Generate every identity before opening the transaction. One SQLite transaction allocates the register-scoped receipt number, renders the deterministic receipt bytes without performing hardware I/O, and computes the canonical payload for every constituent fact. It then inserts one immutable `sync_commit`, every permanent `fact_commit_member` and every delivery row **before** any guarded fact; the `sync_commit_ready` view becomes true from those rows and has no mutable “mark ready” operation. Only then does the transaction write the complete sale graph, initial tender facts, stock facts, any `approval_consumption`, audit entry, original `receipt_artifact` and initial `print_job`, and remove `checkout_operation`. This order is required because the schema's `BEFORE INSERT` guards refuse a financial fact whose complete delivery envelope is not already visible in the same transaction. `fiscal_queue` does not exist until migration `0010` and is not a Phase-1 write.

```rust
pub enum FinalizeWritePoint { /* one variant per repository boundary */ }
impl FinalizeWritePoint { pub const ALL: &'static [Self]; }
```

The fault seam fires before every variant in `FinalizeWritePoint::ALL`. This is an exhaustive catalog, not prose claiming eleven writes while variable line/tax/member counts make that number false. **After commit only**, the drawer and printer effects run.
**Tests:** `finalize_write_point_catalog_is_exhaustive` · `finalize_is_atomic_under_injected_failure` · `manifest_and_delivery_rows_precede_guarded_facts` · `hardware_failure_after_commit_leaves_sale_complete` · `a_complete_sale_has_one_ready_commit_manifest` · `no_ready_commit_omits_a_required_member` · `approval_effect_audit_and_consumption_commit_together` · `finalize_removes_the_checkout_operation_in_the_same_commit` · `department_line_writes_no_stock_event`
**Done when:** `cargo nextest run -p terminal commands::sale::tests::` injects failure at every `FinalizeWritePoint::ALL` entry and leaves either the whole fact graph plus ready manifest or no financial fact, artifact, job, consumption or delivery row.

### 1.8.4 — Crash recovery for `Finalizing`
**Scheduled in:** build `checkout_operation.rs` and the pre-effect journal before 1.8.3; complete startup recovery orchestration after finalize exists
**Files:** `apps/terminal/src-tauri/src/recovery.rs` (new), `crates/pos-db/src/repo/checkout_operation.rs` (new)
Persist `checkout_operation` before the first irreversible payment or finalization effect. At startup, `Tendering` resumes from its priced snapshot and queries any outstanding terminal reference before retry; `Finalizing` replays under the same idempotency key. Finalize deletes the journal row inside the fact transaction, so a committed sale can never leave an operation that authorizes a second effect.
**Tests:** `an_interrupted_tendering_is_recovered_and_status_queried` · `a_checkout_operation_row_never_outlives_its_commit` · `finalize_replays_under_the_same_idempotency_key` · `interrupted_finalize_resumes_without_double_stock_event` (E.1) · `interrupted_finalize_resumes_without_double_outbox_row` (E.1; no duplicate delivery row for any manifest member) · `a_card_approval_before_a_power_cut_is_found_and_attached` · `prop_resume_never_produces_a_second_authorisation`
**Done when:** `cargo nextest run -p terminal recovery::` passes after restarting fixtures in every persisted operation state.

### 1.8.5 — Key handling hardening
**Files:** `crates/pos-db/src/key.rs`
`POS_DB_KEY` is honoured in debug builds and ignored in release, where credential-store lookup continues. A production register reading its data key from an environment variable exposes it to process inheritance; treating a stray variable as a fatal error would instead stop a safe register from opening.
**Tests:** `release_build_ignores_env_key_and_falls_through` (`#[cfg(not(debug_assertions))]`)
**Done when:** `cargo nextest run --release -p pos-db release_build_ignores_env_key_and_falls_through` exits zero, proving an environment value is neither returned nor a startup error.

### 1.8.5b — Key custody and recovery
**Scheduled in:** build the backend first; run its provisioning-screen assertions after 1.11.0 creates the DOM harness
**Files:** `crates/pos-db/src/key.rs`, `crates/pos-db/src/recovery.rs` (new), `apps/terminal/src-tauri/src/provisioning.rs` (new), `apps/terminal/src-tauri/src/ipc/registry.rs`, `packages/api-types/src/ipc/` (generated), `apps/terminal/src/screens/RecoveryCodeProvisioning.tsx` (new), `apps/terminal/src/screens/RecoveryCodeProvisioning.test.tsx` (new), `crates/pos-hardware/src/recovery_sheet.rs` (new)

```rust
pub struct RecoveryEnvelope {
    pub key_id: String,
    pub wrap_algorithm: String,
    pub kdf_algorithm: String,
    pub kdf_params: KdfParams,
    pub wrapped_key: Vec<u8>,
}

pub fn wrap_data_key(data_key: &DatabaseKey, recovery_code: &RecoveryCode,
                     params: &KdfParams) -> Result<RecoveryEnvelope, KeyError>;
pub fn unwrap_data_key(envelope: &RecoveryEnvelope,
                       recovery_code: &RecoveryCode) -> Result<DatabaseKey, KeyError>;
```

```text
provision_recovery_code   {}                          -> RecoveryCodeDisplay { provisioning_id, formatted_code }
print_recovery_code       { provisioning_id }         -> ()
acknowledge_recovery_code { provisioning_id }         -> ()
```

Generate a random per-register data key. Before any key generation on an empty machine, first run requires an explicit choice between **new register** and **restore existing register**. The restore choice enters 1.8.7 without creating a database or credential-store entry; otherwise exit demonstration 10 would silently provision an unrelated register before it could restore the old one. At new-register provisioning, issue and display one merchant-held recovery code once, derive the wrapping key from it, and store the versioned envelope beside every backup; from Phase 3 the organisation record holds another envelope copy. Never log or persist the recovery code. If a database exists and its credential-store entry is absent, refuse to mint a replacement data key: an unrelated key makes both the database and its backups unreadable.
The provisioning screen shows the formatted code and offers one direct recovery-sheet print before requiring the merchant to confirm it was recorded. The sheet is a hardware effect, never a receipt artifact or retry job; the code is dropped from Rust and webview memory on confirmation. If the app crashes before confirmation and no backup exists, provisioning rewraps the same credential-store data key under a newly issued code and invalidates the unacknowledged envelope, because silently completing with an unseen code creates a backup nobody can restore.
**Tests:** `a_backup_opens_with_the_recovery_code_alone` · `wrong_recovery_code_is_refused` · `key_generation_refuses_when_a_database_already_exists` · `clean_machine_restore_path_precedes_key_generation` · `recovery_envelope_survives_restart` · `the_wrapped_envelope_travels_with_every_backup` · `recovery_code_is_displayed_exactly_once_at_provisioning` · `recovery_sheet_is_never_queued_or_persisted` · `unacknowledged_provisioning_rewraps_with_a_new_code` · `recovery_code_and_wrapped_key_are_redacted`
**Done when:** `cargo nextest run -p pos-db key_custody:: && cargo nextest run -p pos-hardware recovery_sheet_ && cargo nextest run -p terminal recovery_code_provisioning_ && pnpm --filter terminal exec vitest run src/screens/RecoveryCodeProvisioning.test.tsx` exits zero and a fixture backup opens after deleting its credential-store entry using only its envelope and the once-displayed code.

### 1.8.6 — Encrypted backup
*Gap G-1.*
**Files:** `crates/pos-db/src/backup.rs` (new)
```rust
pub fn snapshot(conn: &Connection, data_key: &DatabaseKey,
                envelope: &RecoveryEnvelope,
                dest: &Path) -> Result<BackupInfo, DbError>;
pub fn restore(src: &Path, envelope: &RecoveryEnvelope,
               recovery_code: &RecoveryCode, dest: &Path) -> Result<(), DbError>;
pub fn verify(path: &Path, data_key: &DatabaseKey,
              envelope: &RecoveryEnvelope) -> Result<BackupInfo, DbError>;
```
Use SQLite's online backup API through the register's sole source connection. Until 1.8.0's storage gate permits multiple source connections, the storage scheduler serializes the copy with source writes; a queued sale takes precedence and makes the backup abort and retry rather than opening another connection or stalling checkout. Open and key the staged destination with the live `DatabaseKey` before copying; the envelope alone cannot key it, and a plaintext destination is a data breach rather than a backup. Write to that encrypted staged generation, verify it with the live data key, fsync file and directory, then atomically publish without replacing the last verified generation. The recovery code is merchant-held and not available to an hourly job; only clean-machine restore uses it. Restore verifies into a new file, quarantines the old main/WAL/SHM set, and activates only after opening and checking the restored schema and audit anchor. Retention is hourly for 24 h and daily for 30 days per destination.
**Tests:** `snapshot_serializes_with_source_writes_and_is_consistent` · `scheduled_verify_uses_the_live_key_not_the_recovery_code` · `restore_produces_identical_data` · `verify_detects_truncation` · `unverified_copy_never_replaces_the_last_verified_generation` · `restore_quarantines_main_wal_and_shm_before_activation`
**Done when:** `cargo nextest run -p pos-db backup::` restores a register holding unsynced sales into a new path and verifies every sale, commit manifest and audit anchor.

### 1.8.6b — Second backup destination
**Files:** `crates/pos-db/src/backup.rs`, `apps/terminal/src-tauri/src/health.rs`

```rust
pub enum BackupDestination { Local(PathBuf), OffMachine(PathBuf) }
pub fn snapshot_to_all(conn: &Connection, data_key: &DatabaseKey,
                       envelope: &RecoveryEnvelope,
                       destinations: &[BackupDestination])
    -> Result<Vec<BackupInfo>, DbError>;
pub fn destination_health(destinations: &[BackupDestination], now: Timestamp)
    -> Result<Vec<BackupDestinationHealth>, DbError>;
```

Require one local destination and one off-machine destination, either a removable volume or a network path. Verify each generation independently and report each destination's last verified age to device health; theft, disk failure and ransomware take a machine and its local backup directory together.
**Tests:** `off_machine_copy_survives_local_destination_loss` · `failed_second_destination_is_reported` · `backup_age_is_reported_per_destination` · `unverified_copy_never_replaces_the_last_verified_generation`
**Done when:** `cargo nextest run -p pos-db backup::` passes after deleting the local destination and restoring the verified off-machine generation.

### 1.8.7 — Keychain-loss recovery screen
**Files:** `apps/terminal/src-tauri/src/recovery.rs`, `apps/terminal/src-tauri/src/ipc/registry.rs`, `packages/api-types/src/ipc/` (generated)
```text
recovery_state          {}                      -> RecoveryState
recovery_restore_backup { path, recovery_code } -> ()
```

`DbError::BadKey`, a missing credential-store entry with an existing database, or **restore existing register** on a clean machine leads to the out-of-band recovery screen. `recovery_state` inspects both configured destinations without opening the main database and returns only generations whose file, envelope and manifest are structurally present; restore performs the cryptographic and database verification. `recovery_restore_backup` works before a database, user session or capability table exists, restores from either destination and writes the recovered key back to the credential store only after verification. Phase 1 has no server and makes no server-reprovision promise. **Never silent data loss, never a blank register** (E.4, E.4d).
The rendered pre-database UI lands in 1.11.9b; this step owns the recovery state and command contract it consumes.
**Tests:** `bad_key_yields_recovery_state_not_panic` · `clean_machine_restore_path_precedes_key_generation` · `recovery_state_lists_candidates_without_opening_the_database` · `recovery_restore_requires_no_open_database_or_session` · `recovery_restore_rejects_a_wrong_code_without_touching_the_live_files`
**Done when:** `cargo nextest run -p terminal recovery_` passes with no initialized application session and both backup destinations represented by the fixture.

### 1.8.8 — Disk-space guard
**Files:** `apps/terminal/src-tauri/src/health.rs` (new)
Below a threshold, refuse new sales with a clear alarm. A POS that "sells" without persisting is corrupting its ledgers (E.5).
**Tests:** `low_disk_blocks_new_sales_and_alarms`
**Done when:** `cargo nextest run -p terminal low_disk_` exits zero with the threshold fixture refusing a new sale before any tender or financial fact is written.

### 1.8.9 — Outbox writer
**Scheduled in:** Phase 1A, immediately after migration `0003` and before any 1.6, shift or stock fact repository
**Files:** `crates/pos-db/src/repo/outbox.rs` (new)
One business transaction creates one immutable `sync_commit`, a permanent `fact_commit_member` row for every constituent fact, and corresponding `sync_outbox` delivery rows in the same transaction (I-9). The manifest survives delivery-row pruning, so the server can validate the complete sale graph as one atomic commit in Phase 3. No pusher exists yet, but complete envelopes accumulate from the first sale.
**Tests:** `a_completed_sale_has_one_ready_sync_commit` · `every_fact_member_is_in_the_commit_manifest` · `delivery_rows_can_be_pruned_without_losing_the_manifest` · `outbox_commit_rolls_back_with_the_fact_graph`
**Done when:** `cargo nextest run -p pos-db outbox::` passes after pruning acknowledged delivery rows and reconstructing the original commit membership unchanged.

---

## Group 1.9 — Sequences and business date

*Gap G-2.*

### 1.9.1 — Migration `0005`
**Files:** `crates/pos-db/migrations/0005_sale_columns_and_sequences.sql`, `crates/pos-db/tests/migration_0005_sale_columns_and_sequences.rs` (new)
Per [`ref/schema.md`](ref/schema.md) §0005: immutable `shift` opens, append-only `shift_close_event`, rebuildable `shift_state` and the one-open-per-register index; sale shift/store/business-date and tax-snapshot guards; persisted `trusted_time_state`; `sale_tax_summary`; `tender_status_event` plus its projection; the full `tender_type` seed including `exchange` with `opens_drawer=0`, `allows_change=0`, `is_cash_counted=0`, `is_internal=1`; durable parked/active `parked_cart` working states and session claims; `checkout_operation`; register-local, content-hashed `product_quick_add_request`, keyed by the eventual `product.id`; immutable receipt artifacts, print jobs and attempts; completed-fact manifest guards; and scoped `doc_sequence`.
**Tests:** `opening_a_second_shift_for_the_register_is_refused` · `a_completed_sale_requires_an_open_matching_shift` · `migration_0005_preserves_completed_sale_guards` · `every_completed_tender_has_an_initial_status_event` · `exchange_tender_seed_matches_internal_contract` · `an_exchange_tender_never_opens_or_counts_the_drawer`
**Done when:** `just verify-schema` applies `0001`–`0005` and `cargo nextest run -p pos-db --test migration_0005_sale_columns_and_sequences` passes with the existing completed-sale immutability guards intact.

**Deferred 1.1.9 database half — scheduled immediately after this migration:** `trusted_time_state` must exist before the repository can persist the pure domain `ClockState`; do not invent the table early. Before writing the repository, reconcile [`ref/schema.md`](ref/schema.md)'s planned table with the value type: it must carry `device_at_trust`, the captured monotonic anchor, `high_water` and structured anomaly state rather than the stale pre-§3.2 confidence snapshot columns. The shell/repository integration must also persist an opaque shell-owned boot-continuity token beside the value, compare it on startup, and call `ClockState::note_monotonic_reset` before use when it changes; the numeric counter alone cannot distinguish every reboot.
**Files:** `crates/pos-db/src/repo/clock.rs` (new)
**Tests:** `clock_state_survives_restart`
**Done when:** `cargo nextest run -p pos-db clock::` passes with the same `ClockState` after a restart.

### 1.9.2 — `SequenceRepository`
**Files:** `crates/pos-db/src/repo/sequence.rs` (new), `crates/pos-db/tests/sequence.rs` (new)
```rust
pub enum SequenceScope { Register(RegisterId), Store(StoreId) }
pub fn next(&self, tx: &Transaction, scope: SequenceScope,
            kind: SeqKind) -> Result<u64, DbError>;
pub fn gaps(&self, scope: SequenceScope,
            kind: SeqKind) -> Result<Vec<u64>, DbError>;
```
Bumped in the same transaction as the document it numbers, so a crash cannot consume a receipt or Z number without producing its document. `receipt` and `zreport` accept only `Register`; `fiscal_icv` accepts only `Store` and is allocated at first submission in Phase 2, never in the sale transaction.
**Tests:** `sequence_is_gap_free_under_crash_injection` · `rollback_does_not_consume_a_number` · `concurrent_next_never_duplicates` (barrier-synchronised) · `invalid_scope_for_sequence_kind_is_refused`
**Done when:** `cargo nextest run -p pos-db --test sequence` exits zero after the deterministic fault schedule covers one hundred rollback points without a receipt gap or duplicate.

### 1.9.3 — Receipt numbering
**Files:** `apps/terminal/src-tauri/src/commands/sale.rs`, `apps/terminal/src-tauri/tests/receipt_number.rs` (new)
`REG01-000123`: per-register prefix plus a zero-padded counter. Globally unique by prefix, because a central counter cannot exist offline.
**Tests:** `receipt_number_format_is_stable` · `two_registers_never_collide`
**Done when:** `cargo nextest run -p terminal --test receipt_number` exits zero for two register prefixes allocating the same local counter value without a document-number collision.

### 1.9.4 — Business date at finalize
**Files:** `apps/terminal/src-tauri/src/commands/sale.rs`
Copied from the open shift (conventions §11), whose date was resolved at open from the store's IANA zone and persisted `ClockState`; never recompute it from wall-clock midnight or a stored fixed offset during finalize.
**Tests:** `sale_at_0100_belongs_to_previous_business_date` · `business_date_survives_timezone_change` · `a_sale_uses_its_shifts_business_date`
**Done when:** `cargo nextest run -p terminal business_date_` passes after the OS zone and wall clock change between shift open and sale finalize.

### 1.9.5 — Minimal shift lifecycle
**Scheduled in:** build the domain/repository/IPC half after 1.8.9; run its screen assertions after 1.11.0
**Files:** `crates/pos-domain/src/shift.rs` (new), `crates/pos-db/src/repo/shift.rs` (new), `crates/pos-db/tests/shift_lifecycle.rs` (new), `apps/terminal/src-tauri/src/commands/shift.rs` (new), `apps/terminal/src-tauri/src/ipc/registry.rs`, `apps/terminal/src-tauri/tests/shift_lifecycle.rs` (new), `packages/api-types/src/ipc/` (generated), `apps/terminal/src/screens/ShiftOpen.tsx` (new), `apps/terminal/src/screens/ShiftOpen.test.tsx` (new), `apps/terminal/src/screens/ShiftClose.tsx` (new), `apps/terminal/src/screens/ShiftClose.test.tsx` (new)

```text
shift_open    { float_by_denomination, business_date? } -> Shift
shift_current {}                                       -> Option<Shift>
shift_close   { shift_id }                             -> ShiftCloseEvent
```

Opening records the actor and opening float and refuses a second open shift on that register. With trusted clock confidence, the shell derives the business date from `effective_now`, the store's IANA zone and cutover and refuses a caller-supplied date. With `Suspect` or `Untrusted` confidence, `business_date` is required, visibly operator-confirmed and audited; a bad clock never silently assigns a filing day and never closes the store. Closing requires `shift.close` and the authenticated actor must be the user who opened `shift_id`; this ordinary path takes no `ApprovalHandle`, appends an audited `shift_close_event`, and turns off training mode for the next shift without updating the opening fact. A different user's stale shift remains open until Phase 2's manager-approved `shift_force_close_stale` path. The Phase-1 close records no count and exposes no expected cash. Phase 2 extends this path with blind count, over/short, drawer movements, X and Z; none is smuggled into this skeleton.
**Tests:** `opening_a_second_shift_for_the_register_is_refused` · `trusted_clock_refuses_a_supplied_business_date` · `untrusted_clock_requires_and_audits_a_confirmed_business_date` · `shift_open_records_float_and_business_date` · `a_user_can_close_the_shift_they_opened_without_approval` · `a_user_cannot_close_another_users_shift` · `shift_close_appends_an_event_without_mutating_the_open_fact` · `training_auto_off_at_shift_close` (E.54) · `sale_without_an_open_shift_is_refused` · `a_sale_uses_its_shifts_business_date`
**Done when:** `cargo nextest run -p pos-domain shift::tests:: && cargo nextest run -p pos-db --test shift_lifecycle && cargo nextest run -p terminal --test shift_lifecycle && pnpm --filter terminal exec vitest run src/screens/ShiftOpen.test.tsx src/screens/ShiftClose.test.tsx` opens with a float, completes a sale, appends a close event and opens the next shift on the same register.

---

## Group 1.10 — Stock ledger

### 1.10.1 — Migration `0006`
**Files:** `crates/pos-db/migrations/0006_stock_ledger.sql`, `crates/pos-db/tests/migration_0006_stock_ledger.rs` (new)
Create append-only `stock_ledger` with `qty_delta_milli`, the captured `qty_step_milli`, nullable `unit_cost_minor INTEGER`, `is_cost_estimated INTEGER NOT NULL DEFAULT 0`, `is_weight_derived INTEGER NOT NULL DEFAULT 0` and the post-event projection watermark, plus rebuildable `stock_cache` and the register-local working table `stock_adjustment_request`. Every sale event captures the cost basis in force; a later WAC change must not rewrite historical margin, and a price-embedded label must preserve that its stock weight was derived rather than measured. The request table freezes the product, quantity, reason and note before manager approval; it is not a stock fact and is removed only by the transaction that posts the ledger event.
**Tests:** `stock_ledger_is_append_only` · `a_sale_event_records_the_cost_basis_at_capture_time` · `missing_cost_is_null_and_reported_as_estimated` · `price_embedded_stock_event_carries_the_derived_weight_flagged_estimated` · `migration_0006_preserves_quantity_steps`
**Done when:** `just verify-schema` applies `0001`–`0006` and `cargo nextest run -p pos-db --test migration_0006_stock_ledger` proves a later cost change leaves the original sale event's `unit_cost_minor` unchanged.

### 1.10.2 — `StockRepository`
**Files:** `crates/pos-db/src/repo/stock.rs` (new)
```rust
pub fn append(&self, tx: &Transaction, e: &StockEvent) -> Result<(), DbError>;
pub fn on_hand(&self, p: ProductId, s: StoreId) -> Result<Qty, DbError>;      // from cache
pub fn rebuild_cache(&self, tx: &Transaction, s: StoreId) -> Result<u64, DbError>;
pub fn negative_stock(&self, s: StoreId) -> Result<Vec<NegativeStockRow>, DbError>;
```
`append` writes the ledger event, cache projection, audit row and fact-commit membership in one transaction. A partial cache update is not a performance problem; it is a false on-hand figure shown to the cashier.
**Tests:** `stock_append_projects_cache_and_manifest_atomically` · `stock_event_captures_unit_cost_and_estimate_flag`
**Done when:** `cargo nextest run -p pos-db repo::stock::tests::` rolls back every companion row under injected failure at each write boundary.

### 1.10.3 — Cache rebuild equivalence
**Files:** `crates/pos-db/tests/stock.rs`
**Tests:** `prop_cache_rebuild_matches_ledger` — after any sequence of events, rebuilding produces byte-identical cache rows (conventions I-6).
**Done when:** `cargo nextest run -p pos-db --test stock` deliberately corrupts a cache projection, detects the watermark mismatch at verification, rebuilds atomically and returns the same rows as a full ledger replay.

### 1.10.4 — Negative stock: allow and flag
**Files:** `crates/pos-db/src/repo/stock.rs`
Default allow, flag loudly; per-store hard-block setting. Blocking a sale because the ledger is wrong punishes the customer at the register for a back-office error (master plan C.7).
**Tests:** `negative_stock_allowed_by_default_and_flagged` · `hard_block_setting_refuses_the_line` · `two_offline_registers_selling_the_last_unit_both_succeed` (E.12)
**Done when:** `cargo nextest run -p pos-db repo::stock::tests::` exits zero with the default path completing and flagging the sale and the configured hard-block path refusing before tender.

### 1.10.5 — Audited stock adjustment and opening stock
**Scheduled in:** build the domain/repository/IPC half after 1.8.9; run its screen assertions after 1.11.0
**Files:** `crates/pos-domain/src/stock.rs`, `crates/pos-db/src/repo/stock.rs`, `crates/pos-db/src/seed/opening_stock.rs` (new), `crates/pos-db/tests/stock_adjustment.rs` (new), `apps/terminal/src-tauri/src/commands/stock.rs` (new), `apps/terminal/src-tauri/src/ipc/registry.rs`, `packages/api-types/src/ipc/` (generated), `apps/terminal/src/screens/StockAdjust.tsx` (new), `apps/terminal/src/screens/StockAdjust.test.tsx` (new), `crates/pos-db/src/seed/data/opening_stock.csv` (new)

```rust
pub struct OpeningStockRow {
    pub product_id: ProductId,
    pub qty_milli: i64,
    pub unit_cost_minor: Option<i64>,
}
pub fn load_opening_stock(tx: &Transaction,
                          rows: impl IntoIterator<Item = OpeningStockRow>)
    -> Result<usize, DbError>;

pub enum StockAdjustmentReason {
    OpeningStock, Damage, Theft, Expiry, CountCorrection,
}

pub struct StockAdjustmentRequest {
    pub stock_event_id: StockEventId,
    pub product_id: ProductId,
    pub qty_delta_milli: i64,
    pub reason: StockAdjustmentReason,
    pub note: Option<String>,
}
```

```text
stock_on_hand       { product_id }             -> StockPosition
stock_adjust_prepare { product_id, qty_delta_milli, reason_code, note? }
                                                -> StockAdjustmentRequest
stock_adjust         { stock_event_id, approval_id } -> StockPosition
```

The CSV columns are exactly `product_id`, `qty_milli` and nullable `unit_cost_minor`. The wire reason codes are the matching snake-case values `opening_stock`, `damage`, `theft`, `expiry` and `count_correction`; no free-text reason substitutes for the enum. `stock_adjust_prepare` preallocates the eventual `stock_ledger.id`, persists the exact proposed effect under that id and writes its canonical `PreparedIntentHash`. The privileged `stock_adjust` command accepts only that `stock_event_id` plus approval, binds the handle's `entity_id` to the durable event, uses `amount_minor = 0` and binds the request hash. It reloads the row and recomputes the hash immediately before the effect; a mismatch refuses the command, and a database trigger independently refuses every `UPDATE` after an approval references the request. It consumes the handle, removes the working request, posts the ledger event with the same id and writes the audit row in one transaction. The opening loader is a provisioning path, not that IPC command; it calls the same append path with reason `opening_stock` and never inserts `stock_cache` directly. Missing cost is captured as `unit_cost_minor = NULL, is_cost_estimated = 1` and shown as unknown, because silently substituting zero makes margin look trustworthy when it is not.
**Tests:** `stock_on_hand_returns_the_current_projection` · `stock_adjust_is_audited_with_a_reason_code` · `unknown_stock_adjustment_reason_is_refused` · `stock_adjust_approval_for_another_event_is_refused` · `altering_a_stock_request_after_approval_is_refused` (table-driven over `stock_event_id`, `product_id`, `qty_delta_milli`, `reason_code`, `note`, `requested_by`, `requested_at` and `content_hash`; each mutation is refused once by the recomputed-hash check and once by the database trigger) · `stock_effect_consumption_and_audit_share_the_persisted_event_id` · `opening_stock_loader_posts_ledger_events_not_cache_rows` · `missing_cost_is_null_and_reported_as_estimated` · `opening_stock_makes_the_negative_stock_flag_meaningful`
**Done when:** `cargo nextest run -p pos-db opening_stock_ && cargo nextest run -p pos-db --test stock_adjustment && cargo nextest run -p terminal commands::stock::tests:: && pnpm --filter terminal exec vitest run src/screens/StockAdjust.test.tsx` exits zero; the fixture loads `N` milli-units, sells `1000`, reports `N - 1000` on hand, and preserves the sale event's captured cost after a later adjustment.

---

## Group 1.11 — The terminal UI

*Arabic-first RTL from the first commit. Retrofitting RTL is miserable; scaffolding it is cheap.*

### 1.11.0 — Register DOM component-test harness
**Files:** `apps/terminal/package.json`, `pnpm-lock.yaml`, `apps/terminal/vite.config.ts`, `apps/terminal/src/test/setup.ts` (new), `apps/terminal/src/screens/Sale.test.tsx` (new)

```ts
export function renderWithProviders(ui: React.ReactElement): RenderResult;
```

Add `@testing-library/react`, `@testing-library/user-event`, `@testing-library/jest-dom` and `jsdom`; configure Vitest with `environment: "jsdom"`. Scan-burst tests use fake timers and `user-event`'s `advanceTimers`, so the `< 30 ms` heuristic is deterministic rather than scheduler-dependent.
**Tests:** `sale_screen_renders_in_rtl_by_default`
**Done when:** `pnpm --filter terminal exec vitest run src/screens/Sale.test.tsx` executes the canary in a DOM and exits zero.

### 1.11.1 — i18n infrastructure
*Gap G-5.*
**Files:** `apps/terminal/src/i18n/` (new: `index.ts`, `ar.ts`, `en.ts`, `catalog.test.ts`), `apps/terminal/src/styles/font.css` (new), `packages/ui/src/`
Typed catalog; keys per conventions §2; `<html dir="rtl" lang="ar">` by default.
**Tests:** `catalogs_have_identical_key_sets` · `ui_and_rasterizer_resolve_the_same_embedded_font`
**Done when:** `pnpm --filter terminal exec vitest run src/i18n/catalog.test.ts` exits zero with Arabic as the rendered default, identical typed keys in both catalogues and the UI resolving the exact font asset proven by 1.7.2.

### 1.11.2 — RTL lint
**Files:** `scripts/check-logical-css.sh`, `justfile`
Extend the existing guard that `just lint` and CI's `web` job run. Ban physical direction utilities (`pl-`, `pr-`, `ml-`, `mr-`, `left-`, `right-`, `text-left`, `text-right`) in favour of logical ones (`ps-`, `pe-`, `ms-`, `me-`, `start-`, `end-`, `text-start`, `text-end`). A genuinely physical rule carries `physical-ok: <reason>` on the same line; a bare suppression is refused because unexplained exceptions become the RTL regression path.
**Tests:** `physical_direction_utility_is_rejected`
**Done when:** using `pl-4` fails `just lint`.

### 1.11.3 — Formatting helpers
**Files:** `apps/terminal/src/lib/format.ts`, `apps/terminal/src/lib/format.test.ts` (new)
`formatMoney(minor, currency, locale)`, `formatQty(milli, weighed)`, `formatDate(iso, tz, locale)`. Western Arabic digits. Never `toLocaleString` inline. Transaction totals use the currency exponent; a shorter catalogue display is allowed only when exact.
**Tests:** `formats_jod_at_the_currency_exponent` · `catalog_short_format_refuses_to_hide_fils` · `uses_western_digits_in_arabic_locale` · `latin_runs_inside_arabic_text_are_bidi_isolated`
**Done when:** `pnpm --filter terminal exec vitest run src/lib/format.test.ts` exits zero for exact JOD fils, Arabic locale digits and an isolated Latin SKU inside Arabic text.

### 1.11.4 — Lock / PIN screen (D1)
**Files:** `apps/terminal/src/screens/Lock.tsx`, `apps/terminal/src/screens/Lock.test.tsx` (new)
Numpad, ≥48 px targets, register name, sync status, open-shift owner. Fast user switch.
**Tests:** `lock_screen_shows_register_and_shift_owner` · `pin_unlock_routes_to_the_open_shift` · `fast_user_switch_never_reuses_the_previous_principal`
**Done when:** `pnpm --filter terminal exec vitest run src/screens/Lock.test.tsx` exits zero with a different authenticated principal after the fast-switch fixture.

### 1.11.4b — Bound approval modal
**Files:** `apps/terminal/src/components/ApprovalModal.tsx` (new), `apps/terminal/src/lib/ipc.ts`, `apps/terminal/src/components/ApprovalModal.test.tsx` (new)

```ts
export interface ApprovalModalProps {
  capability: string; actorId: string; entityId: string;
  amountMinor: bigint; reason: string;
  onApproved(ref: ApprovalRef): void;
}
```

Show the exact capability, entity, amount, actor and reason being approved. `auth_verify_pin` returns only `ApprovalRef { approval_id, capability, expires_at }`; JavaScript never receives the nonce or full `ApprovalHandle`. Submit that id only to the bound pending command and discard it after one attempt.
**Tests:** `approval_modal_shows_the_bound_amount_and_entity` · `approval_modal_refuses_the_actor_as_approver` · `expired_approval_prompts_again` · `approval_id_is_sent_only_to_the_bound_command`
**Done when:** `pnpm --filter terminal exec vitest run src/components/ApprovalModal.test.tsx` passes and changing the operation amount makes the command fixture refuse the approval.

### 1.11.5 — Sale screen (D3)
**Files:** `apps/terminal/src/screens/Sale.tsx` and components, `apps/terminal/src/screens/Sale.test.tsx`
Three zones per master plan D: cart list (line menu on long-press: qty, discount, override, void; blocked age line with confirm/decline) · totals block with a huge TOTAL and the always-visible status strip (🔵 synced / 🟡 offline *n* queued / fiscal-pending *n* / training banner) · search, department and PLU/tile grid.
**Tests:** `sale_screen_renders_cart_total_and_status_strip` · `age_restricted_line_stays_blocked_until_confirmed` · `declining_age_confirmation_removes_the_blocked_line`
**Done when:** `pnpm --filter terminal exec vitest run src/screens/Sale.test.tsx` exits zero with the age-restricted fixture unable to reach tender before confirmation.

### 1.11.6 — Global scan capture
**Files:** `apps/terminal/src/lib/scanner.ts`, `apps/terminal/src/lib/scanner.test.ts` (new)
A hidden input capturing keystrokes anywhere on the sale screen, distinguishing a scan burst from typing by inter-key timing (< 30 ms between characters, terminated by Enter). **Scans must route correctly even when focus is in the search box** — that detail is where most implementations break.
**Tests:** `scan_burst_detected_over_typing` · `scan_routes_while_search_focused` — both use fake timers from 1.11.0.
**Done when:** `pnpm --filter terminal exec vitest run src/lib/scanner.test.ts` exits zero with fake time advancing across both the scan-burst and human-typing thresholds.

### 1.11.7 — Tender screen (D4)
**Files:** `apps/terminal/src/screens/Tender.tsx`, `apps/terminal/src/screens/Tender.test.tsx` (new)
Amount due huge; cash numpad plus denomination quick-keys; live change display; the split-tender list with remaining due.
**Tests:** `tender_screen_shows_remaining_due_and_live_change` · `split_tender_never_hides_the_remaining_due`
**Done when:** `pnpm --filter terminal exec vitest run src/screens/Tender.test.tsx` exits zero for cash, overtender and split-tender fixtures.

### 1.11.8 — Post-sale toast (D5)
**Files:** `apps/terminal/src/components/PostSale.tsx`, `apps/terminal/src/components/PostSale.test.tsx` (new)
Change due big, print / reprint buttons, auto-return in ~3 s.
**Tests:** `post_sale_shows_change_and_print_actions` · `post_sale_auto_return_uses_the_injected_timer`
**Done when:** `pnpm --filter terminal exec vitest run src/components/PostSale.test.tsx` exits zero with fake time advancing the auto-return exactly once.

### 1.11.9 — Settings & diagnostics (D10)
**Files:** `apps/terminal/src/screens/Diagnostics.tsx`, `apps/terminal/src/screens/Diagnostics.test.tsx` (new)
Test print, scanner echo, printer status, database health, compiled storage versions, each backup destination's verified age, audit status, about. Phase 1 deliberately has no drawer-diagnostics command: Phase 2 adds the action by reusing the audited `drawer_open_no_sale` path after `drawer_event` exists.
**Tests:** `diagnostics_reports_each_backup_destination_age` · `diagnostics_reports_storage_versions_and_audit_status`
**Done when:** `pnpm --filter terminal exec vitest run src/screens/Diagnostics.test.tsx` exits zero with separate stale and healthy backup destinations visible.

### 1.11.9b — Out-of-band recovery screen
**Files:** `apps/terminal/src/screens/Recovery.tsx` (new), `apps/terminal/src/screens/Recovery.test.tsx` (new), `apps/terminal/src/lib/ipc.ts`

```text
recovery_state          {}                      -> RecoveryState
recovery_restore_backup { path, recovery_code } -> ()
```

Before the database opens, list detected backup generations and accept the merchant recovery code for `recovery_restore_backup`. The code is held only for the command call, never placed in state persistence, telemetry or a support screenshot.
**Tests:** `recovery_screen_works_without_a_user_session` · `recovery_screen_lists_both_backup_destinations` · `recovery_code_is_cleared_after_one_attempt` · `wrong_recovery_code_leaves_live_files_untouched`
**Done when:** `pnpm --filter terminal exec vitest run src/screens/Recovery.test.tsx` passes with the application fixture in pre-database recovery state.

### 1.11.10 — Local product quick-add (D11)
**Files:** `crates/pos-db/src/repo/product_quick_add.rs` (new), `crates/pos-db/tests/product_quick_add.rs` (new), `apps/terminal/src-tauri/src/commands/product.rs` (new), `apps/terminal/src-tauri/src/ipc/registry.rs`, `packages/api-types/src/ipc/` (generated), `apps/terminal/src/screens/QuickAdd.tsx`, `apps/terminal/src/screens/QuickAdd.test.tsx` (new)

```text
product_quick_add_prepare { product_id, barcode, name_ar, unit_price_minor,
                            tax_category_id } -> ProductQuickAddRequest
product_quick_add         { product_id, approval_id } -> Product
```

Manager-only emergency SKU creation under `product.edit` and a bound approval. `product_quick_add_prepare` persists the complete proposed product under its preallocated eventual `product.id` and writes its canonical `PreparedIntentHash`; it creates neither a product nor a cart line. The handle binds `entity_id` to that durable product id, `amount_minor` to its `unit_price_minor` and `content_hash` to the complete request. The consuming command accepts neither product fields nor price, reloads the request and refuses unless its recomputed hash equals both persisted hashes. A database trigger independently refuses every `UPDATE` after approval. Product creation, request removal, approval consumption and audit commit together with the same effect id, so the approved product cannot be reused with another barcode, Arabic name, price, tax category, requester or timestamp. The product is marked as a local edit; Phase 3 syncs it up as a change-request, never a silent merge. This is the durable-catalogue alternative, not the cashier's default queue path and not a way to pass a price through `cart_add_line`.
**Tests:** `quick_add_requires_product_edit_and_bound_approval` · `quick_add_approval_for_a_different_product_is_refused` · `altering_a_quick_add_request_after_approval_is_refused` (table-driven over `product_id`, `barcode`, `name_ar`, `unit_price_minor`, `tax_category_id`, `requested_by`, `requested_at` and `content_hash`; each mutation is refused once by the recomputed-hash check and once by the database trigger) · `quick_add_effect_consumption_and_audit_share_the_persisted_product_id` · `quick_add_never_calls_cart_add_line_with_a_price`
**Done when:** `cargo nextest run -p pos-db --test product_quick_add && cargo nextest run -p terminal commands::product::tests:: && pnpm --filter terminal exec vitest run src/screens/QuickAdd.test.tsx` exits zero and the IPC spy observes no caller-supplied price on `cart_add_line`.

### 1.11.10b — Unknown barcode resolution
**Files:** `apps/terminal/src-tauri/src/commands/catalog.rs` (new), `apps/terminal/src-tauri/src/ipc/registry.rs`, `packages/api-types/src/ipc/` (generated), `apps/terminal/src/components/UnknownBarcode.tsx` (new), `apps/terminal/src/components/UnknownBarcode.test.tsx` (new), `apps/terminal/src/screens/Sale.tsx`

```text
department_list {} -> Vec<Department>
Unknown { code }    -> search | department_list | product_quick_add
```

Search and PLU come first. The default cashier path is a configured, capped department sale through `department_list` and `cart_add_department_sale`; a manager may instead open 1.11.10. The queue never waits for `product.edit`, and the department's tax category is captured on the line.
**Tests:** `department_list_returns_only_active_tax_configured_departments` · `unknown_barcode_offers_quick_add_or_department_sale` · `a_cashier_has_a_path_forward_without_product_edit` · `a_department_sale_carries_its_own_tax_category_and_audits`
**Done when:** `cargo nextest run -p terminal department_list_ && pnpm --filter terminal exec vitest run src/components/UnknownBarcode.test.tsx` completes a taxed department line as a cashier without leaving the sale screen.

### 1.11.11 — Keyboard map
**Files:** `apps/terminal/src/lib/keymap.ts`, `apps/terminal/src/lib/keymap.test.ts` (new)
`F2` search · `F4` pay · `F6` park · `F7` resume · `F9` returns · `Del` void line · `+/−` qty · `F12` lock. Scans need no focus.
**Tests:** `every_action_reachable_without_a_mouse`
**Done when:** `pnpm --filter terminal exec vitest run src/lib/keymap.test.ts` exits zero after dispatching every mapped action without pointer input.

### 1.11.12 — Designed empty and edge states
**Files:** `apps/terminal/src/screens/EdgeStates.test.tsx` (new), `apps/terminal/src/screens/Sale.tsx`, `apps/terminal/src/screens/Tender.tsx`, `apps/terminal/src/components/PostSale.tsx`
Offline banner reading *"Sales are safe and will sync."* Printer-out warning **at Pay**, not after. Fiscal-pending badge explaining itself on tap. Min-size guard on the sale screen (E.60).
**Tests:** `offline_banner_states_sales_are_safe` · `sale_screen_min_size_guard` · `unknown_barcode_offers_quick_add_without_stalling` · `paper_out_warns_at_pay_not_after` · `latin_runs_inside_arabic_text_are_bidi_isolated`
**Done when:** `pnpm --filter terminal exec vitest run src/screens/EdgeStates.test.tsx` executes all five named rendered-state tests and exits zero.

### 1.11.13 — Scan latency budget
**Scheduled in:** after 1.12.1 creates the shared seeded catalogue
**Files:** `apps/terminal/tests/e2e/performance/scan-latency.e2e.ts` (new), `apps/terminal/wdio.conf.ts` (new), `apps/terminal/package.json`, `pnpm-lock.yaml`, `benchmarks/baselines/scan-to-line.json` (new)

```ts
export async function measureScanToVisibleLine(code: string): Promise<number>;
```

Measure simulated scanner input through the packaged Tauri application, Rust lookup, cart repricing and the visible React line over the seeded catalogue using WebdriverIO and `tauri-driver`. Use the reference register and sample/variance policy in conventions §7; measuring only the parser or browser build hides the IPC, webview and rendering delay the cashier experiences. Phase 2 expands this minimal driver into the full packaged-app smoke suite at 2.9.5.
**Tests:** `scan_latency_scenario_reaches_a_visible_priced_line` (deterministic scenario validation; timing belongs only to the benchmark gate)
**Done when:** `just bench-gate scan-to-line` exits zero over at least 50 post-warm-up scans and its p99 is below `100 ms`.

### 1.11.14 — RTL screenshot baselines
**Scheduled in:** after 1.12.1 creates the shared seeded UI state
**Files:** `apps/terminal/package.json`, `pnpm-lock.yaml`, `apps/terminal/playwright.config.ts` (new), `apps/terminal/tests/visual/sale-rtl.spec.ts` (new), `apps/terminal/tests/visual/golden/` (new)

```ts
test("sale_screen_arabic_rtl_matches_baseline", async ({ page }) => { /* seeded */ });
test("sale_screen_english_ltr_matches_baseline", async ({ page }) => { /* seeded */ });
```

Commit sale-screen baselines in Arabic and English at `1024×640` over the same fixture. The images make flex order, mirrored icons, truncation and bidi isolation reviewable; the logical-CSS grep cannot see any of them.
**Tests:** `sale_screen_arabic_rtl_matches_baseline` · `sale_screen_english_ltr_matches_baseline`
**Done when:** `pnpm --filter terminal test:visual` exits zero after comparing both seeded `1024×640` images with the committed baselines; baseline review remains a pull-request control, not a test assertion.

### 1.11.15 — Phase-1 Arabic cashier guide
**Scheduled in:** after 1.11.14 and 1.12.1
**Files:** `docs/manual/cashier-ar.md` (new), `docs/manual/assets/phase-1/` (new), `scripts/check-manual-coverage.py` (new), `docs/drills/` review record

Write one illustrated Arabic page per Phase-1 cashier screen and recovery path, using the seeded UI screenshots: lock and shift open, sale/search/unknown barcode, age confirmation, department sale, tender/change, printer warning/retry, park/resume, shift close and recovery-code restore. Each instruction names the safe action and the state the cashier should expect; a manual that says only which button to press cannot keep a queue moving when the printer or database is unavailable.
**Tests:** `cashier_guide_covers_every_phase_1_screen` · `cashier_guide_screenshots_match_seeded_ui` · `cashier_guide_has_no_untranslated_required_step`
**Done when:** `python3 scripts/check-manual-coverage.py --phase 1` exits zero and finds a dated `docs/drills/YYYY-MM-DD-cashier-guide-review.md` naming the commit and a native-Arabic-speaking cashier who did not write the guide.

---

## Group 1.12 — Fixture, benchmarks, gate

### 1.12.1 — The Jordanian minimarket seed
*Gap G-10.*
**Files:** `crates/pos-db/src/seed/minimarket.rs` (new), `crates/pos-db/src/seed/data/*.csv`, `crates/pos-db/tests/minimarket_seed.rs` (new), `justfile`
~200 products with real Arabic names across the merchant's approved tax pack: exempt and standard items, one item for every imported reduced band, loose vegetables sold by weight, a deli item with a price-embedded barcode, a sealed-pack tobacco line (`min_age = 18`, `regulated_kind = 'tobacco'`), and a price-controlled staple (`max_price_minor`). `opening_stock.csv` goes through 1.10.5's ledger loader, never directly into `stock_cache`.
**Tests:** `seed_uses_every_enabled_tax_band` · `seed_tobacco_is_a_sealed_age_restricted_pack` · `seed_opening_stock_posts_ledger_events`
**Done when:** `just seed` produces positive on-hand through ledger facts for every stock item and `cargo nextest run -p pos-db --test minimarket_seed` passes; every demo and screenshot uses this fixture.

### 1.12.2 — Hand-checked tax report fixture
**Files:** `crates/pos-domain/tests/tax_report.rs`
A scripted trading day over the fixture: mixed rates, a weighed line, a basket discount, a refund, and a cash-rounded tender. **Total it by hand once**, commit the expected figures, and let CI defend them forever.
**Tests:** `tax_report_matches_hand_check_fixture`
**Done when:** `cargo nextest run -p pos-domain --test tax_report` exits zero against the committed hand-calculated net, tax and gross values for every enabled component.
> This is the only test that proves the *product* is right rather than the *code*. Do the arithmetic on paper.

### 1.12.3 — Benchmark suite in CI
**Files:** `.github/workflows/ci.yml`, `scripts/tests/benchmark_ci_contract_test.py` (new), `benchmarks/baselines/`
Aggregate `price_cart` < 16 ms · FTS search < 50 ms · scan-to-visible-line < 100 ms under the matrix/profile selected in 1.2.0, recording medians, p99 and median absolute deviation. The live CI job is pinned to `runs-on: [self-hosted, reference-register]`; hosted jobs run only 1.2.0's fixed threshold fixtures and may not update a baseline. `cargo bench` printing a slower number and exiting zero is not evidence.
**Tests:** `benchmark_ci_live_job_uses_reference_runner` · `hosted_benchmark_job_runs_only_threshold_fixtures` · `every_phase_1_budget_has_a_committed_baseline`
**Done when:** `python3 scripts/tests/benchmark_ci_contract_test.py && just bench-gate` exits zero, proving all three CI-contract tests and every implemented Phase-1 budget on the physical reference runner.

### 1.12.4 — Edge-case test sweep
**Files:** `scripts/check-test-catalog.py`; Phase-1 test sources under `crates/*/src/`, `crates/*/tests/`, `crates/*/fuzz/`, `apps/terminal/src/`, `apps/terminal/src-tauri/tests/` and receipt/visual golden directories
Close every Phase-1-owned assertion in [`ref/test-catalog.md`](ref/test-catalog.md): E.1, E.1b, E.2b, E.3, E.4, E.4b, E.4c, E.4d, E.5, E.6, E.7, E.12, E.14, E.18, E.19, E.19b, E.33, E.36, E.38, E.39, E.39b, E.40, E.41, E.41b, E.46, E.49, E.51, E.52, E.54, E.58, E.60, E.69, E.70, E.71, E.78, E.80, E.83, E.85, E.86 and E.91. Shared rows retain their later-phase siblings; Phase 1 does not pull a Z report or a server checkpoint forward merely because the row also contains a Phase-1 assertion.
**Tests:** `scripts/check-test-catalog.py` verifies every planned Phase-1 name has an owner, every catalogued implemented name resolves to its runner, and every normative reference name has exactly one phase owner.
**Done when:** `python3 scripts/check-test-catalog.py --phase 1` exits zero with no missing owner, missing test or stale planned name.

### 1.12.5 — Sales-side tax reconciliation
**Files:** `crates/pos-db/src/repo/report.rs` (new), `crates/pos-db/tests/tax_by_rate_report.rs` (new), `apps/terminal/src-tauri/src/commands/report.rs` (new), `apps/terminal/src-tauri/src/export/tax_by_rate_csv.rs` (new), `apps/terminal/src-tauri/src/ipc/registry.rs`, `apps/terminal/src-tauri/tests/report_tax_by_rate.rs` (new), `apps/terminal/src/screens/TaxByRateReport.tsx` (new), `apps/terminal/src/screens/TaxByRateReport.test.tsx` (new), `packages/api-types/src/ipc/` (generated)

```rust
pub struct TaxByRateRow {
    pub component_code: String,
    pub treatment: TaxTreatment,
    pub rate_ppm: Option<i64>,
    pub per_unit_minor: Option<i64>,
    pub zero_rating_reason: Option<ZeroRatingReason>,
    pub net_minor: i64,
    pub tax_minor: i64,
    pub gross_minor: i64,
    pub sale_document_count: u64,
    pub credit_document_count: u64,
}
pub struct TaxByRateReport {
    pub from_date: BusinessDate,
    pub to_date: BusinessDate,
    pub currency: Currency,
    pub rows: Vec<TaxByRateRow>,
    pub excluded_training_count: u64,
}
pub fn export_tax_by_rate_csv(report: &TaxByRateReport) -> Result<Vec<u8>, ReportError>;
```

```text
report_tax_by_rate { from_date, to_date } -> TaxByRateReport
```

Require `reports.all`. Treat `from_date` and `to_date` as inclusive store-local business dates. Return the document's own carried values grouped by component, treatment, rate or per-unit component and supply reason, including separate sale and credit-document counts; refunds are negatives in the same rows and training sales are excluded with a visible count. The report carries its currency so the UI never guesses an exponent. The screen and CSV say **sales-side tax reconciliation**, never “return” or “filing report”, because Phase 1 has no purchase, import or input-tax workpaper facts.
The report reuses `tax_report_matches_hand_check_fixture`, owned by 1.12.2; this microstep does not create a second test with the same name.
**Tests:** `refunds_appear_as_negatives_in_the_same_rate_row` · `sales_and_credits_have_separate_document_counts` · `training_sales_are_excluded_with_a_visible_count` · `sales_reconciliation_does_not_claim_full_return` · `report_tax_by_rate_uses_inclusive_business_dates` · `csv_export_matches_carried_report_rows` · `tax_report_screen_labels_it_sales_side_reconciliation`
**Done when:** `cargo nextest run -p pos-domain --test tax_report && cargo nextest run -p pos-db --test tax_by_rate_report && cargo nextest run -p terminal --test report_tax_by_rate && pnpm --filter terminal exec vitest run src/screens/TaxByRateReport.test.tsx` exits zero and both the screen and CSV reproduce the committed fixture's carried rows for its business date.

---

## Exit gate

Phase 1 is done when all of these are true:

```bash
just lint && just test                      # everything green, no ignored tests
cargo nextest run --workspace -E 'test(prop_)'   # every property passes
just bench-gate                              # absolute and regression budgets met
```

The Arabic cashier guide and its independent native-speaker review record must also pass 1.11.15; deferring the manual until launch would make the first non-author operator the person who discovers the instructions are incomplete.

And, by demonstration — do this literally, with the network cable unplugged:

1. **Open the packaged app offline.** Time ten clean launches on the reference register; the median is under 3 seconds. Sign in with a PIN and open a shift with an opening float.
2. **Load opening stock, then sell a basket** containing: a scanned multipack, an item found by Arabic search, a weighed item via price-embedded barcode, an exempt item, every enabled reduced band, a 16% item, and the sealed-pack age-restricted item after confirmation. Scan an unknown code and use the cashier's department-sale path without waiting for `product.edit`.
3. **Apply a basket discount.** Confirm it prorates to the lines to the fil.
4. **Pay cash** with overtender. Confirm change, and confirm cash rounding produced an explicit adjustment line.
5. **The receipt prints in Arabic**, correctly shaped and right-to-left, with a tax summary showing exempt and standard as separate rows.
6. **Park a sale, serve another customer, resume it.** Nothing is lost.
7. **Run both power-loss drills.** First pull the power mid-finalize, restart, and prove exactly one completed sale exists, with one stock event per stock-bearing line, one ready `sync_commit` whose manifest contains every constituent fact, one original receipt artifact and queued print job, and no duplicate financial effect. Then complete and print a second sale, cut physical power without an OS shutdown, reboot, and prove its sale, manifest, artifact and audit entry survived; this second drill is E.1b and is not simulated by `pkill`.
8. **Switch to a principal with `reports.all` and open the sales-side tax reconciliation** for the shift's business date. Check it against the receipts by hand; it reconciles to the fil and does not call itself a return. Then close the Phase-1 shift with a bound approval: an append-only close event exists, no expected-cash figure or count is exposed, and the next shift can open on the register.
9. **Verify the audit chain.** Intact. Take and verify an online snapshot with 1.8.6; only on that consistent copy, drop `audit_log_no_update`, alter one row, and run `verify-audit` against it. It reports `Broken { at_seq }` at that row. The original still refuses the same `UPDATE`.
10. **Create and verify the off-machine backup and wrapped-key envelope on a removable volume, then destroy the database, local backup destination and credential-store entry.** On a clean machine, restore that removable-volume copy using only the printed recovery code. Every unsynced sale, fact manifest, delivery row and audit anchor is still there; `POS_DB_KEY` is not used.

**If any of the ten fails, Phase 1 is not done.** Number 7 and number 10 are the ones most likely to be skipped and the two most likely to matter on a merchant's worst day.

→ **Next:** [`phase-2-money-grade.md`](phase-2-money-grade.md)
