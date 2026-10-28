# Phase 1 — Sellable MVP

> **Exit:** a real Jordanian minimarket could sell for cash, all day, fully offline, in Arabic, with correct GST and a printed receipt.

**Effort:** 15–21 weeks for a solo developer learning Rust on the job: 8–11 weeks for Phase 1A (groups 1.1–1.7 through the pure receipt pipeline, plus the early 1.8.9 commit-manifest writer and schema-spine steps 1.9.1 and 1.10.1 needed before migration 0007) and 7–10 weeks for Phase 1B. In 1B, land 1.8.0–1.8.2b and 1.7.7, build the emulated printer and its conformance suite (1.7.12–1.7.13) once 1.7.10's driver exists, complete the backend halves of the sequence/shift and stock repositories in 1.9.2–1.10.5, build 1.11.0 before their UI tests, then land the checkout-journal/repository half of 1.8.4 before atomic finalize in 1.8.3. Finish 1.8.4's startup recovery orchestration and printer-unavailable handling in 1.7.6b before the remaining recovery, UI and gate work. This phase now contains the tax engine, Arabic raster pipeline, recovery-key custody, crash-safe fact commits, a minimal shift lifecycle, stock opening and the register UI; treating those integrations as an acceleration would hide a slip until the final gate.
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

1.6, 1.9 and 1.10 can proceed in parallel with the cart work and are good places to go when you are stuck on the state machine. Build 1.1.0 before any property, 1.2.0 before any benchmark — and note that 1.2.0's **deferred hardware half** is milestone 5.8's, so no `just bench-gate` command can *pass* before it: each budget step closes on its benchmark and 1.12.3's relative comparison, and 5.8.2 proves the limit — the database half of 1.1.9 after 1.9.1, and the database half of 1.2.4 after 1.2.5. Build 1.8.9 after migration `0003` and before any 1.6, shift or stock repository writes an append-only fact. Build 1.6.3 and 1.6.4 before the privileged transitions in 1.4.7 and 1.4.12, and 1.6.7 before completing the registry contract in 1.4.11. Build 1.11.0 before the UI halves of 1.8.5b, 1.9.5 and 1.10.5, and 1.12.1 before the seeded performance, screenshot and manual steps 1.11.13–1.11.15. Document numbering is a stable reference, not permission to ignore an explicit dependency.

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
The high-count property lane stays separate so the ordinary CI path does not repeat 100,000 cases on every change; `ci.yml` deliberately has no `schedule:` trigger. That last clause still holds, and it is meant to keep holding: when a weekly cross-platform build was wanted later, it became its own workflow — `.github/workflows/cross-platform-canary.yml` — for exactly this reason rather than a cron being added to `ci.yml`. Two workflows now rest on the rule, so do not resolve the apparent inconsistency by deleting the canary or by putting a `schedule:` on `ci.yml`; a cron there would put all of `ci.yml`'s jobs on a clock to gain one of them.
Every property from API reference §1.6 uses 1.1.0's shared helper and names the strategy it attacks. The scheduled domain lane runs `PROPTEST_CASES=100000` and prints its replayable seed. This is the layer that finds what you did not imagine; do not replace a listed property with a smaller example suite.
Microstep 1.1.2a owns `prop_add_sub_roundtrip`, `prop_split_preserves_total` and `prop_currency_mismatch_never_silently_coerces`; 1.1.2b owns the proportional, quantity, exact-format and truncation properties.
**Tests:** `prop_round_to_step_is_idempotent` · `prop_round_to_step_within_half_step`
**Done when:** `cargo nextest run -p pos-domain money::tests::prop_ && PROPTEST_CASES=100000 cargo nextest run -p pos-domain money::tests::prop_` exits zero, and the scheduled lane runs these same properties at the same case count — as part of a crate-wide selection rather than by repeating this command. Its filter is `-E 'test(/::prop_/)'`, not this microstep's `money::tests::prop_` prefix, because a money-only filter quietly left the `ids`, `audit` and `time` properties at their committed 4,096 cases, and two of those are precisely where the case count *is* the test: `prop_seq_ids_never_collide` is a collision search and `prop_seq_ids_keep_the_v7_layout` guards invariant 7's identity layout. The consequence for every later microstep that adds a property: the `prop_` prefix on the test name **is** the selection, so renaming one out of it drops it from the 100,000-case lane with nothing going red.

### 1.1.6 — `RoundingRule` and `RoundingDirection`
**Files:** `crates/pos-domain/src/money.rs`
Both enums from [`ref/domain-api.md`](ref/domain-api.md) §1.2, **and the one rounding primitive
`RoundingRule::round_to_i64`** that 1.1.2b's arithmetic calls. Two bare enums have no behaviour, and
I-1's "rounds once" needs exactly one implementation rather than one per caller, so the primitive
belongs to the step that introduces the parameter. `RoundingDirection` gained its behaviour at
1.1.2b, which built `round_to_step`; 1.5.3 builds the cash-rounding policy on top of that primitive
rather than the primitive itself. Default `HalfAwayFromZero`, not banker's — see
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
**Scheduled in:** the pure domain value/policy types and terminal zone-resolution shell at 1.1.9; the database half immediately after 1.9.1 created `trusted_time_state`. **Both halves have landed** — the second on 14 September, recorded under 1.9.1.
**Files (current half):** `Cargo.toml`, `Cargo.lock`, `apps/terminal/src-tauri/Cargo.toml`, `crates/pos-domain/src/lib.rs`, `crates/pos-domain/src/time.rs` (new), `apps/terminal/src-tauri/src/lib.rs`, `apps/terminal/src-tauri/src/time.rs` (new)
Per API reference §3, including `business_date_of`, `MonotonicClock`, the pure `ClockState` value type, `clock_confidence` and `effective_now`. The store persists the IANA zone id `Asia/Amman`; the shell resolves its offset for each instant with `jiff` and passes that value into pure `pos-domain`. Do not encode a seasonal Jordan rule: Jordan is year-round UTC+3, and the old rule moves winter sales across the 04:00 cutover.
**This step also owns the `Clock` port and `FixedClock`**, which API reference §2 originally placed in 1.1.8: `pub trait Clock { fn now(&self) -> Timestamp; }` cannot exist before the `Timestamp` this file defines, and `MonotonicClock<C: Clock>` needs the trait anyway. `IdSource` and `SeqIdSource` stayed at 1.1.8, where they compile.
**Tests:** `shift_opened_at_0030_belongs_to_previous_day` · `prop_business_date_stable_across_shift` · `prop_cutover_boundary_never_skips_a_day` · `prop_monotonic_clock_never_decreases` · `prop_effective_now_never_precedes_high_water` · `prop_clock_confidence_is_monotone_in_skew` · `clock_jump_back_reports_anomaly` · `a_never_synced_register_is_untrusted_not_trusted` · `wall_clock_moved_forward_a_year_is_suspect` · `a_reboot_without_an_anchor_is_a_monotonic_reset` · `no_clock_confidence_refuses_a_sale` · `business_date_uses_the_offset_in_force_at_the_instant` · `a_january_sale_and_a_july_sale_agree_in_asia_amman` · `resolving_an_unknown_zone_id_is_a_named_error_not_a_default_offset`
**Done when:** `cargo nextest run -p pos-domain time:: && cargo nextest run -p terminal time::tests:: && cargo nextest run -p pos-db clock::` passes, including a shift opened at `2026-08-21T00:30` local with a `04:00` cutover resolving to business date `2026-08-20`, both January and July resolving through `Asia/Amman`, and the same `ClockState` read back after the database is closed and reopened.
> It became one `Done when` on 14 September, when the deferred database half landed. While the halves were split this line was a `**Current half done when:**`, which is deliberately **not** the literal string `scripts/check-implementation-frontier.py` matches — so rule 3's `Full-step status:` marker refused a completion claim and rule 4 never got the chance. Clearing the marker without also promoting this line reds the checker from the other direction, which is the pair working as designed.
---

## Group 1.2 — Catalog, barcodes, search

> **Build order inside this group is not step order.** Create schema `0003` in 1.2.1, then build
> the pure catalogue types in 1.2.2 and pure scan parser in 1.2.4. After migrations `0004`–`0006`
> exist, land `0007` in 1.2.5; only then can 1.2.3 build its FTS repository and the database half
> of 1.2.4 build trade-scale commissioning. Finish with 1.2.6–1.2.8. This order keeps a public
> repository signature from naming an unwritten `ScanLookup` and keeps FTS tests from running
> against tables that do not exist.

### 1.2.0 — Benchmark gate harness
**Scheduled in:** build the gate, the recipe, every refusal path and the deliberately blank reference-register profile at 1.2.0. **Selecting and filling the lowest register row is deferred to the hardware qualification gate, milestone 5.8, because no device is bought before it** ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b). [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6a ships the device matrix empty on purpose — "filling it with model numbers nobody has held would be the same defect as a compliance claim nobody earned" — so this step commits the profile with its full field structure and every identity value blank, and the gate **refuses** that pair. That refusal is conventions §7.1 working as written ("no baseline is accepted while either record is blank or they disagree"), not a defect in the harness. The microstep therefore remains partly open until 5.8.2.
**Files (current half):** `justfile`, `scripts/bench-gate.py` (new), `scripts/tests/bench_gate_test.py` (new), `benchmarks/reference-register.toml` (new), `benchmarks/baselines/README.md` (new), `.gitignore`, `docs/implementation/ref/hardware-and-receipts.md`

```text
just bench-gate [search|price-cart|pin-verify|scan-to-line]
```

The gate compares a measurement under `benchmarks/measurements/<slug>.json` against a committed baseline under `benchmarks/baselines/<slug>.json`, each carrying median, p99 and median absolute deviation as **integer nanoseconds** — a verdict that depends on binary float rounding at the 20% boundary is a verdict that argues with itself. It exits non-zero when an absolute limit is exceeded, or when the median is more than 20% slower **and** more than three baseline MADs slower; the `and` is load-bearing in both directions, and `a_noisy_regression_within_three_mads_is_not_a_failure` and `a_tight_regression_beyond_three_mads_is_not_a_failure` are what keep it from silently becoming an `or`. The absolute limit applies to p99, except cold start's explicitly median §7 entry and PIN verification's **two-sided** 200–350 ms median band beside its 500 ms p99 ceiling — a 40 ms PIN median passes the ceiling and is still a failure, because Argon2 that fast is a weak parameter set. `cold-start` is designed for and named in the slug list; it stays unimplemented until Phase 2 (2.9.3 / 2.9.5) and says so.

Refusals, all of them non-zero and all of them naming the reason: a missing, blank, structurally divergent or mismatched reference register; a baseline or measurement whose `profile_identity` is not the reference register's; a baseline sitting outside its own absolute limit, so a red gate cannot be repaired by republishing the slow number; and **an empty budget set** — omitting the argument while nothing is implemented cannot exit zero, because a green `just bench-gate` in a release checklist reads as "budgets met". A hosted runner may exercise fixed pass/fail fixtures through `--fixture-root` and is refused both the live comparison and `--publish-baseline`; detection is environment-based, `RUNNER_ENVIRONMENT=self-hosted` is the exception §7.1's future job needs, and the refusal is checked before any record is read so it cannot be shadowed.
**Tests (current half):** `bench_gate_fails_an_absolute_budget` · `bench_gate_fails_a_significant_regression` · `bench_gate_refuses_a_missing_reference_profile` · `hosted_runner_cannot_publish_a_performance_baseline` · `reference_profile_fields_mirror_the_device_matrix_columns` · `a_blank_reference_profile_and_matrix_are_detected_and_refused` · `a_noisy_regression_within_three_mads_is_not_a_failure` · `a_tight_regression_beyond_three_mads_is_not_a_failure` · `a_pin_verify_median_below_its_band_is_a_failure` · `a_baseline_outside_its_own_absolute_limit_is_refused` · `a_record_from_another_machine_is_refused` · `an_empty_budget_set_cannot_report_success` · `a_synthetic_fixture_identity_can_never_become_the_committed_profile` · `a_recipe_argument_never_becomes_shell_source`
**Current half done when:** `python3 scripts/tests/bench_gate_test.py` exits zero, proving every refusal path fires *and* says why, and `python3 scripts/bench-gate.py --check-profile` exits **non-zero** naming both blank records. Those two commands do not compose with `&&` in this half, and the second one is the point of it.
> **`reference_profile_matches_supported_device_matrix` is not in the current half, and nothing green stands in for it.** It compares a filled §6a.1 row with a filled profile, so it needs a machine; it is scheduled in the deferred half below. Two current tests cover what is honestly checkable without one, and neither passes by comparing two blanks: `reference_profile_fields_mirror_the_device_matrix_columns` asserts both records declare the same twelve identity fields in the same order and that a fixture dropping or reordering one is refused, and `a_blank_reference_profile_and_matrix_are_detected_and_refused` asserts the committed blank pair is *refused* while a filled synthetic pair through the same code path is accepted. Fixture identities are obviously synthetic in every field, and the gate refuses a synthetic value on its live path so one can never be pasted into the committed record.

**Deferred half — at milestone 5.8, executed as 5.8.2, where the reference register is bought against the frozen matrix ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b).** Run §6a's "Qualifying a new device" procedure against the machine, add its row to §6a.1's table (rows are ordered lowest-capability first; row 1 is the reference register), and copy the twelve cells into `benchmarks/reference-register.toml` byte for byte. Do not invent a machine to close this early: every §7 budget would then be measured against a referent that does not exist, and the four budget microsteps would inherit it.
**Files (deferred half):** `docs/implementation/ref/hardware-and-receipts.md`, `benchmarks/reference-register.toml`, `scripts/tests/bench_gate_test.py`
**Tests (deferred half):** `reference_profile_matches_supported_device_matrix`
**Deferred half done when:** `python3 scripts/bench-gate.py --check-profile` exits **zero**, printing the machine it read, and `python3 scripts/tests/bench_gate_test.py` still exits zero with the deferred test asserting the committed row and the committed profile agree field for field.
**Full-step status:** 1.2.0 is not complete until a reference register exists, its §6a.1 row is filled, `benchmarks/reference-register.toml` mirrors it, and `--check-profile` exits zero — at 5.8.2. The gate, the recipe and the refusal paths are complete and enforced now; the benchmarks they will judge arrive at 1.2.7, 1.4.9, 1.6.2 and 1.11.13, the relative CI comparison at 1.12.3, and the budgets themselves are proven at 5.8.2.

### 1.2.1 — Migration `0003`: org / store / register / taxonomy
**Delivery-history correction — now closed.** Commit `321c4f5` / PR #35 carried `[1.2.1]` but delivered only staging-recipe tests and documentation; it did not add or register migration `0003`. **Commit `ed090a2` / PR #56 shipped it**, and this microstep is complete. The correction is retained because the failure it records is the repository's recurring one: `crates/pos-db/tests/strict_rebuild.rs` applies **reference SQL** through `crates/pos-db/tests/common/mod.rs`, not a registered migration, so a green run there was never evidence that `0003` existed.
**Files:** `crates/pos-db/migrations/0003_strict_rebuild_and_catalog_depth.sql` (new) · `crates/pos-db/src/lib.rs` (append to `MIGRATIONS`) · `apps/server/migrations/<ts>_product_catalog_depth.sql` (new mirror) · `crates/pos-db/tests/migration_0003.rs` (new) · [`ref/schema.md`](ref/schema.md) (the two §0003 `SHIPPED` markers) · `crates/pos-db/tests/{strict_rebuild,sale_immutability,encryption_smoke,common/mod}.rs` · `scripts/check-test-catalog.py`
> **This list was one file until 1.2.1 was built.** Shipping a migration here also retires its
> `PLANNED` catalogue entry, marks its `ref/schema.md` section `SHIPPED` so `verify-schema.py`
> stops replaying it from the document — **both** the `##` heading and the `###` subheading that
> owns the later fences — and repairs every fixture that encoded the pre-migration schema.
> A later migration should expect the same shape of work rather than rediscover it.
Per [`ref/schema.md`](ref/schema.md) §0003, **in that order**: the STRICT rebuild of the six tables 0001/0002 created loose, then the `org`, `store`, `register`, `category`, tax-rule-pack tables, `barcode`, `setting`, commit-envelope tables and the `product` `ALTER`s. `barcode.pack_qty_milli INTEGER NOT NULL DEFAULT 1000` makes a six-pack code add six units rather than one; `regulated_kind` and `sale_form` make a tobacco row a sealed pack rather than an individual-cigarette SKU.

The rebuild goes first, and its statements are the most dangerous in the chain — they drop and recreate the tables holding every completed sale. Read the recipe before you type it: SQLite's twelve-step procedure begins by turning foreign keys off, which is a no-op inside a transaction, and `defer_foreign_keys` does not substitute. Staging tables are why this commits with foreign keys enforced.

**Accepted transition window:** `0003` closes the completed-tender UPDATE path before `0005` adds append-only `tender_status_event`. No tender repository or register has shipped in that interval, so the safe result is temporarily no settlement path at all. Do not build a tender repository into the gap; settlement persistence begins at 1.9.1/0005.
**Tests:** `migration_0003_creates_all_tables` · `barcode_live_uniqueness_allows_reissue_after_tombstone` · `barcode_pack_qty_defaults_to_1000_milli` · `a_pack_quantity_of_zero_is_refused_at_save` · `tobacco_product_must_be_a_sealed_pack` · `the_rebuild_keeps_every_row_of_a_completed_sale` · `the_rebuilt_tables_are_all_strict` · `the_rebuild_restores_the_immutability_triggers` · `no_staging_table_survives_the_rebuild` · `after_the_rebuild_the_six_tables_enforce_their_types`
**Done when:** a tombstoned barcode code can be reassigned to a different product; two live rows with the same code cannot exist.

### 1.2.2 — `Product` and `UnitOfMeasure` in the domain
**Files:** `crates/pos-domain/src/catalog.rs` (new) · `crates/pos-domain/src/lib.rs` (module declaration and re-exports) · [`ref/domain-api.md`](ref/domain-api.md) (§4 and the new §4.0) · this file (this microstep's prose and `Tests:` line)
Per API reference §4. Include `min_age`, `max_price`, `is_service` and the regulated pair on `Product`, and `pack_qty: Qty` on the barcode lookup result. The database columns remain `product.max_price_minor` and `barcode.pack_qty_milli`; the domain fields are `Option<Money>` and `Qty`, which carry the unit the suffix exists to supply — **this line said `max_price_minor` before 1.2.2 was built, and §4 was right**. The age gate does not make an individual-cigarette SKU lawful, so the sealed-pack invariant belongs beside the product type: [`ref/domain-api.md`](ref/domain-api.md) §4.0 carries `regulated_kind` and `sale_form` as ONE `RegulatedSaleForm` field with accessors of both column names, because two independent fields can express the SKU the rules forbid and a constructor that refused it would be bypassed by the first `Product { .. }` literal. The pack quantity belongs on the code because the same product can be sold by unit and by case.
**Tests:** `a_barcode_pack_quantity_is_a_qty` · `tobacco_product_requires_a_sealed_pack_sale_form` · `the_same_product_sells_by_unit_and_by_case_through_two_barcodes` · `the_regulated_pair_round_trips_through_its_two_columns` · `an_unlawful_regulated_pair_is_refused_on_the_wire` · `every_unit_of_measure_declares_whether_it_divides` · `the_unit_table_lists_every_variant_exactly_once` · `each_stored_token_is_spelled_once` · `golden_catalog_json_is_stable` · `a_price_ceiling_in_another_currency_cannot_be_compared_silently`
**Done when:** `cargo nextest run -p pos-domain catalog::` passes with the same product resolving to `Qty::ONE` and `Qty::from_units(6)` through two different barcode records.

### 1.2.3 — `ProductRepository`
**Files:** `crates/pos-db/src/repo/product.rs` (new — the repository, plus `add_barcode`, the write E.36 and E.78 need and the signature above lacked), `crates/pos-db/src/repo/mod.rs` (one `pub mod` line; 1.8.9 created the module and its `lib.rs` declaration), `crates/pos-db/src/search.rs` (`match_expression`, the typed text as an FTS5 expression), `crates/pos-db/src/lib.rs` (four `DbError` variants), `crates/pos-db/tests/product_repository.rs` (new), `crates/pos-domain/src/catalog.rs` (`UnitOfMeasure` loses `Gram` and `Millilitre`, the choice 1.2.2 left here; `Product::check_saveable`) · [`ref/domain-api.md`](ref/domain-api.md) §4 and §4.0 (both, recorded) · [`ref/ipc-contract.md`](ref/ipc-contract.md) (where the tokenising lives) · `scripts/check-test-catalog.py` (five names: `PLANNED` → `PLANNED_RETIRED`, one of them from 4.6.3) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line)
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
**Tests:** `a_second_live_barcode_claim_is_refused` (E.36) · `a_multipack_barcode_adds_its_pack_quantity` (E.78) · `by_id_ignores_tombstones` · `catalog_save_above_ceiling_is_rejected` (E.71) · `search_survives_every_fts5_metacharacter` · `prop_no_query_string_produces_a_database_error` (E.39b) · `every_unit_round_trips_through_the_unit_column` · `check_saveable_refuses_what_a_catalogue_save_must_not_store` · `prop_a_saveable_price_never_exceeds_its_ceiling`
**Done when:** `cargo nextest run -p pos-db a_second_live_barcode_claim_is_refused && cargo nextest run -p pos-db search_survives_every_fts5_metacharacter && cargo nextest run -p pos-db prop_no_query_string_produces_a_database_error` exits zero; the first test proves two live products cannot share a barcode and the latter two exercise the complete literal corpus above.

### 1.2.4 — Price-embedded barcode parser
**Scheduled in:** build the pure parser at 1.2.4; add commissioning persistence after 1.2.5 creates the trade-scale tables
**Files:** `crates/pos-domain/src/catalog.rs` (`mod scan;` and its re-exports), `crates/pos-domain/src/catalog/scan.rs` (new — the parser, `PriceSource`, `ScanLookup`, `resolve_scan`; and `claiming_rule`, the parser's own choice of layout, made public for the shell half), `crates/pos-domain/src/trade_scale.rs` (new), `crates/pos-domain/src/lib.rs`, `crates/pos-domain/tests/scan_lookup_ui.rs` (new), `crates/pos-domain/tests/ui/price_embedded_lookup_is_not_constructible.rs` and `.stderr` (new), `crates/pos-domain/tests/ui/parsed_scan_cannot_be_forged.rs` and `.stderr` (new — rustc reports privacy only after the type errors stop the first fixture), [`ref/domain-api.md`](ref/domain-api.md) (§4.1–§4.3 as delivered, and §4.4) · `crates/pos-db/src/repo/trade_scale.rs` (new — the database half: current evidence, active layouts, and evidence recorded only with its envelope), `crates/pos-db/src/lib.rs` (three `DbError` variants), `crates/pos-db/tests/trade_scale.rs` (new), [`ref/sync-protocol.md`](ref/sync-protocol.md) §3 (the payload scale evidence travels as) · `apps/terminal/src-tauri/src/scan.rs` (new — the shell half: `handle_scan` reads with every layout, makes a label answer for its scale, then looks the item code up and resolves it), `apps/terminal/src-tauri/src/lib.rs` (`pub mod scan;`), `crates/pos-db/src/repo/trade_scale.rs` (`rules`, every layout with `is_active`, so a switched-off layout's label is still known to be a label) · [`ref/ipc-contract.md`](ref/ipc-contract.md) (the two refusals a scan can end in before it is looked up) · `scripts/check-test-catalog.py` (two names: `PLANNED` → `PLANNED_RETIRED`) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line and its `Full-step status:` marker)
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
**Tests:** `prop_ean13_checksum_matches_reference` · `prop_embedded_parse_roundtrip` · `prop_corrupt_digit_never_parses_clean` · `non_utf8_scan_is_a_named_error` · `weight_embedded_2xxxxxwwwww_parses` · `price_embedded_2xxxxxpmmmmmc_parses` · `price_embedded_resolves_item_code_before_constructing_lookup` · `only_scan_handler_constructs_price_embedded_lookup` · `price_embedded_line_total_equals_the_label` · `price_embedded_line_is_one_unit_not_a_weight` · `price_embedded_after_a_price_per_kilo_change_still_charges_the_label` · `from_label_refuses_a_weight_rule` · `unverified_trade_scale_cannot_activate_an_embedded_rule` · `maintenance_pending_disables_scale_pricing` · `trade_scale_evidence_travels_with_its_envelope` · `trade_scale_current_verification_is_the_newest_event` · `trade_scale_rules_read_back_as_the_parser_reads_them` · `trade_scale_maintenance_stops_label_pricing_end_to_end` · `trade_scale_storage_alone_accepts_evidence_without_an_envelope` · `the_claiming_rule_is_the_rule_the_parse_reads_with` · `prop_a_claimed_code_is_never_read_as_plain` · `a_plain_barcode_is_found_with_its_pack_quantity` · `a_price_label_from_a_verified_scale_prices_its_line` · `a_weight_label_carries_its_weight` · `a_scale_in_maintenance_stops_its_labels_by_name` · `label_pricing_needs_a_trusted_clock_and_current_evidence` · `a_switched_off_layout_is_not_read_as_a_barcode` · `a_damaged_label_is_a_checksum_failure` · `an_unknown_code_is_unknown_and_the_queue_moves_on` · `an_unreadable_scan_is_named`
**Done when:** `cargo nextest run -p pos-domain catalog:: && cargo nextest run -p pos-domain trade_scale:: && cargo nextest run -p pos-domain --test scan_lookup_ui && cargo nextest run -p pos-db trade_scale_ && cargo nextest run -p terminal scan::tests::` exits zero; E.40 rejects every one-digit corruption, the compile-fail fixture proves external code cannot construct `PriceEmbedded`, the handler resolves the extracted item code before returning it, and a rule without current signed verification evidence cannot produce a checkout price.
> **Completed at 1.4.3, 28 October 2026.** `price_embedded_line_is_one_unit_not_a_weight` now runs against the cart's `AddLine::from_scan` (§6.1), where a price label becomes one package at the label's price with its derived weight kept as the stock basis; it was the last named test, so the `Full-step status:` marker that stood here is retired. As before: the database half (`crates/pos-db/src/repo/trade_scale.rs`) passes `cargo nextest run -p pos-db trade_scale_`, the shell half (`apps/terminal/src-tauri/src/scan.rs`) passes `cargo nextest run -p terminal scan::tests::`, and the pure half passes `cargo nextest run -p pos-domain catalog:: trade_scale::` and `--test scan_lookup_ui`.

### 1.2.5 — Migration `0007`: FTS5, PLU, tiles, scan rules
**Files:** `crates/pos-db/migrations/0007_search_and_seed.sql` (new), `crates/pos-db/src/lib.rs` (`MIGRATIONS`, `pub mod search;`), `crates/pos-db/src/search.rs` (new — the query-side `exact` and `fold` that `prop_sql_and_rust_folding_agree` holds to `0003`'s columns, for 1.2.3's `search()`), `crates/pos-db/Cargo.toml` (`proptest` and `pos-test-support` for that property), `crates/pos-db/tests/migration_0007_search_and_seed.rs` (new), `crates/pos-db/tests/common/mod.rs` (`reference_blocks_at_or_after(8)`), `crates/pos-db/tests/migration_0006_stock_ledger.rs` (its migrated version is the current one, not 6), `apps/server/migrations/20261022150000_search_and_seed.sql` (new — the mirror, which creates nothing and records why), `Cargo.lock` (two edges, no new package) · [`ref/schema.md`](ref/schema.md) (§0007 marked SHIPPED) · `scripts/check-test-catalog.py` (eight names: `PLANNED` → `PLANNED_RETIRED`) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line)
Per [`ref/schema.md`](ref/schema.md) §0007: FTS/index triggers, PLU/tile data, immutable trade-scale identity, append-only scale-verification events and the guards that keep an embedded rule inactive without current verified evidence. This migration executes only after 0003–0006 have landed, even though the domain/search work is described before groups 1.6, 1.9 and 1.10; deriving the migration number from document order would collide with their schema.

`remove_diacritics 2` folds **Latin** diacritics only — it does not fold Arabic, and treats tashkeel as token separators, so `قَهْوَة` indexes as four single-letter tokens and a search for `قهوة` finds nothing (verified on SQLite 3.51). Arabic matching comes from `name_ar_fold`, the generated column added in 0003 and indexed here, plus `prefix='2 3'` for the 1–3 character search 1.2.7 benchmarks.

**Both sides must transform, and precision needs its own branch.** The repository applies the same two transforms to the query that 0003 applies to the columns — a folded index searched with an unfolded string returns zero rows — and then queries `name_ar_exact` and `name_ar_fold` together so the spelling the cashier typed outranks a variant. `search()` in 1.2.3 owns that, and `prop_sql_and_rust_folding_agree` is what stops the two implementations drifting.

Raw `name_ar` is not indexed, on purpose: tashkeel are token separators, so a vocalised name would contribute single-letter tokens and make one-character prefix search match unrelated products.

Search is the fallback for every unbarcoded item and the only path a cashier has when the scanner fails. 0007 is forward-only, so getting this wrong costs a compensating migration plus a full reindex on every installed register.
**Tests:** `fts_matches_arabic_with_and_without_diacritics` · `fts_matches_alef_and_yaa_spelling_variants` · `fts_matches_taa_marbuta_spelled_as_haa` · `fts_ignores_tatweel` · `prop_sql_and_rust_folding_agree` · `fts_prefix_search_works_at_two_characters` · `exact_spelling_outranks_a_folded_variant` · `a_single_letter_query_does_not_match_a_vocalised_name` · `fts_matches_english_and_sku` · `unicode_names_roundtrip_through_db_and_fts` (E.41) · `fts_survives_product_update` · `fts_row_removed_on_tombstone` · `embedded_rule_requires_verified_trade_scale` · `scale_verification_evidence_is_append_only` · `status_loss_disables_an_active_embedded_rule`
**Done when:** `just verify-schema` applies `0001`–`0007` and `cargo nextest run -p pos-db --test migration_0007_search_and_seed` exits zero after proving both FTS trigger coverage and fail-closed trade-scale activation.

### 1.2.6 — Assert FTS5 exists at open
**Files:** `crates/pos-db/src/lib.rs` (`assert_fts5`, `fts5_verdict`, `DbError::MissingFeature`, the call in `open`, and a `#[cfg(test)] mod tests` in `lib.rs` — `key.rs` and `repo/outbox.rs` already had their own)
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
**Done when:** `cargo nextest run -p pos-db open_asserts_fts5_available` exits zero, proving both halves — `fts5_verdict(0)` returns the named `MissingFeature` error, and a real `fts5` virtual table on an opened register matches a row. A build without FTS5 then fails at `open()` with that named error rather than at the first search with empty results.
> **The decision is split from the query on purpose, and one thing stays untested.** Every connection this build opens reports FTS5, so a test that could only go through `assert_fts5` would exercise the success branch and nothing else; `fts5_verdict` takes the count so the refusal can be reached. What no test here can reach is whether `open` still *calls* it — delete that line and every assertion stays green, because on an FTS5-enabled build a checking open and a non-checking open are indistinguishable. Only a build without FTS5 could tell them apart, and `rusqlite` has no feature flag to produce one, which is the same fact that makes the assertion necessary. The call site is a reviewed one-liner, not a tested one. The compile-options string is likewise evidence of configuration rather than of a working module, which is why the test builds a `temp` `fts5` table and searches it.

### 1.2.7 — Search benchmark
*Gap G-9.*
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-db/benches/search.rs` (new), `crates/pos-db/Cargo.toml` (`criterion`, and the `[[bench]]` target) — its baseline, `benchmarks/baselines/search.json`, is 5.8.2's · `crates/pos-db/src/fixtures/search_catalogue.rs` (new — the 50 000 products and the four queries, shared by the bench and its tests because neither target can see the other's files), `crates/pos-db/src/fixtures/mod.rs`, `crates/pos-db/tests/search_benchmark.rs` (new) · `crates/pos-db/src/repo/product.rs` (`upsert` prepares its statement once: saving the fixture recompiled every trigger on `product` per product) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line and its note)
Seed 50 000 products with Arabic names and benchmark `search()` at 1, 2, 3 and 5 characters under the reference-register sample and variance policy in conventions §7. The 50 ms limit is the reference register's, so it is proven at milestone 5.8 (5.8.2); until then 1.12.3 watches this benchmark relatively and nothing claims the budget (conventions §7.2).
**Tests:** `search_benchmark_fixture_contains_50000_products` · `search_benchmark_exercises_every_query_length`
**Done when:** `cargo nextest run -p pos-db search_benchmark_ && cargo test -p pos-db --bench search` exits zero, the second running every query length once over the 50 000-product fixture; `just bench-gate search`, with its p99 below `50 ms` on the reference register, is 5.8.2's.
> **What the first run found, 25 October 2026 — indicative, and a trigger.** The fixture is the worst case on purpose: every name carries a brand beginning with ال, so the first keystroke matches all 50 000 products. On the development machine the benchmark was written on (an Apple M3 laptop), the 1- and 2-character queries took longer than the 50 ms limit itself, and the 3- and 5-character queries a small fraction of it. That proves nothing about the reference register (conventions §7.2), but a floor machine is slower than a development laptop, not faster. Matching is not the cost: finding every match takes a few milliseconds, and a first page in index order with no ranking is faster still. The cost is `search`'s `ORDER BY f.rank, p.sku` — bm25 computed for every match, and every match joined to `product`, before twenty rows are kept. It fires the trigger in [`00-master-plan.md`](00-master-plan.md) §6, *an indicative development-machine measurement above a quarter of any budget's absolute limit*. The response — buying the floor machine early under the written exception in [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b, changing what a short query ranks, or both — is the owner's decision, and 1.2.7 makes neither: it delivers the measurement that shows the problem.

### 1.2.8 — Fuzz the scan parser
**Files:** `justfile` (`fuzz`), `crates/pos-domain/fuzz/Cargo.toml` and `Cargo.lock` (new — a workspace of its own), `crates/pos-domain/fuzz/rust-toolchain.toml` (new — the dated nightly `cargo fuzz` needs, for that directory only), `crates/pos-domain/fuzz/.gitignore` (new), `crates/pos-domain/fuzz/src/lib.rs` and `src/parse_scan.rs` (new — the layouts and the check, shared with the test), `crates/pos-domain/fuzz/fuzz_targets/parse_scan.rs` (new), `crates/pos-domain/fuzz/corpus/parse_scan/` (new — thirty named seeds), `crates/pos-domain/tests/parse_scan_fuzz.rs` (new) · [`02-development-workflow.md`](02-development-workflow.md) (the recipe, and the gap it narrows) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Done when:` lines, its target and its note, and 1.12.1's corpus clause)

```rust
fuzz_target!(|input: &[u8]| {
    let rules = bounded_fixture_rules();
    check(input, &rules); // parse_scan_bytes, then what must hold of its result
});
```

The target feeds arbitrary bytes and lengths through the scanner boundary and asserts termination without panic. Seed it with every plain, weight-embedded, price-embedded and corrupt code in the Phase-1 fixture; a discovered crash input becomes a permanent corpus entry because the same damaged label can return tomorrow.
**Tests:** `parse_scan_fuzz_seed_corpus_never_panics`
**Done when:** `just fuzz parse_scan` exits zero — `cargo fuzz run parse_scan` for 100 000 inputs on the fuzz crate's pinned nightly, from the committed corpus and libFuzzer's mutations of it — and `cargo nextest run -p pos-domain --test parse_scan_fuzz` passes.
> **As delivered, 25 October 2026.** `check` asserts more than termination. Bytes that are not text are refused as such, an ordinary barcode is looked up whole, and a clean parse of a label is thirteen digits, with a check digit that agrees when its layout verifies one and the item code its layout's span names — so a clean parse of a damaged label fails a run as a panic does, and an input still running after ten seconds fails it as a loop. The layouts are fixed, because a store's configuration is not the hostile input, but they include what `0007` accepts and no register should have: a divisor of zero, a value span past the code or empty, a span whose end overflows `usize`, and an empty prefix. `cargo fuzz` needs a nightly compiler and the repository pins stable, so the fuzz crate is a workspace of its own with a dated nightly pinned beside it, and `just fuzz` runs there, reading the committed corpus without writing to it. The Phase-1 fixture is 1.12.1's minimarket seed, which does not exist yet, so the corpus is seeded from the codes and layouts today's fixtures use — the cola's two barcodes, the cheese's PLU, the deli scale's labels — and with at least one seed for every refusal `parse_scan` can make; 1.12.1 adds its own codes. Each seed is named for what the parser must make of it, and `parse_scan_fuzz_seed_corpus_never_panics` replays the corpus through the target's own `check` on every pull request and holds each name to its outcome. Two things the fuzz crate does not settle are recorded here. **The fuzz crate is outside the workspace, so CI's `cargo deny` does not read its lockfile.** Run by hand, it passes advisories, bans and sources and fails licences on one crate: `libfuzzer-sys` carries LLVM's NCSA licence, which `deny.toml` does not list. It is a development tool that never reaches a register; whether to allow it is the owner's decision, and 1.7.8b's weekly job is where both fuzz workspaces would be audited. **A divisor of zero is refused as `ValueOutOfRange`**, the error for a damaged label, though the module's own rule is that an unusable layout is `RuleMalformed`; that is 1.2.4's parser, so it is recorded here and not changed.

---

## Group 1.3 — Tax engine

*Everything in [`ref/tax-jordan.md`](ref/tax-jordan.md). Right before the first real sale, or never.*

### 1.3.1 — Tax types
**Files:** `crates/pos-domain/src/tax.rs` (new) · `crates/pos-domain/src/lib.rs` (module declaration and re-exports) · [`README.md`](README.md) (implementation frontier) · [`ref/domain-api.md`](ref/domain-api.md) (§15 dependency correction) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
`TaxTreatment`, `TaxBasis`, `TaxBase`, `TaxComponent`, `PriceMode`, `StoreTaxProfile`, `SupplyTaxContext`, `SupplyDestination`, `ZeroRatingReason`, `LineTax`, `ComponentTax`, `TaxSummaryRow`, `TaxRateRule` and `TaxError` per API reference §5. Fixed and ad-valorem components, dependency order, charge unit and immutable supply evidence are types rather than optional comments, because GST-on-SST and a supply-specific zero rate cannot be reconstructed from one percentage later.
**Tests:** `compound_tax_basis_round_trips_without_loss` · `tax_base_preserves_named_component_dependency_order` · `supply_tax_context_carries_destination_reason_and_evidence` · `tax_error_names_the_component_and_unknown_base_code` · `golden_tax_json_is_stable`
**Done when:** `cargo nextest run -p pos-domain tax::tests::compound_tax_basis_round_trips_without_loss` exits zero with the exact synthetic `Percent` ppm and the per-unit `Money` minor amount and currency unchanged after a serde JSON round-trip.

### 1.3.2 — `resolve_components`
**Files:** `crates/pos-domain/src/tax.rs` · [`ref/domain-api.md`](ref/domain-api.md) §5 (as delivered, and the zero-rating `⚠️ OPEN` item) · [`ref/merchant-decisions.md`](ref/merchant-decisions.md) (row 6.14) · [`00-master-plan.md`](00-master-plan.md) §4a.3 · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Done when:` lines and its note, and 1.3.3's refusal)
`valid_from` inclusive, `valid_to` exclusive; scoped rules override unscoped; overlap and absence are both errors, never a guessed 16%.
**Tests:** `prop_rate_resolution_is_deterministic_at_boundaries` · `overlapping_rules_error` · `no_rule_in_effect_errors` · `scoped_rule_overrides_unscoped`
**Done when:** `cargo nextest run -p pos-domain tax::tests::` exits zero: the boundary property resolves the old rule up to the millisecond before every change and the new one from the instant itself, in any rule order, and an overlap and an absence are both refused by name.
> **As delivered, 26 October 2026.** The override is per component and per profile: a rule scoped to the store's profile beats an unscoped one, an unscoped rule is `Standard`'s alone, and two rules left in effect at the same tier are `OverlappingRules`. Components come back by `sequence`, then code, whatever order the pack lists them in. A rule carries no inclusive-capability field, so `is_inclusive_capable` is derived — true only for a percentage of the line net, the one shape 1.3.4's extraction inverts — and 1.3.5 widens it with the arithmetic. Naming an incomplete zone pack as `ProfilePackIncomplete` rather than `NoRuleInEffect` is 1.3.5's too (`an_incomplete_profile_pack_fails_closed`), and an unregistered store's short-circuit is 1.3.8's. **This step was waiting on the zero-rating `⚠️ OPEN` item, and its own code never reads the supply.** The owner decided on 26 October that, while it is unknown whether the merchant makes non-domestic supplies, every one is refused ([`ref/merchant-decisions.md`](ref/merchant-decisions.md) 6.14); the item now belongs to 2.7.0, and 1.3.3 enforces the refusal.

### 1.3.3 — `compute_line_tax`, exclusive mode
**Files:** `crates/pos-domain/src/tax.rs` · `crates/pos-domain/src/lib.rs` (re-exports) · `crates/pos-domain/src/money.rs` (`Percent`'s note on where a negative tax rate is refused) · [`ref/domain-api.md`](ref/domain-api.md) §5 (as delivered, the new `TaxError` variants and `PendingTaxShape`, and the zero-rating `⚠️ OPEN` item) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines, and its note)
Simpler direction first: `tax = net × r`, one rounding. It is also the first code to read a `SupplyTaxContext`, so it enforces the owner's decision of 26 October 2026 ([`ref/merchant-decisions.md`](ref/merchant-decisions.md) 6.14): a supply whose destination is anything but `Domestic` is refused by name, whatever reason or evidence it carries, until the accountant names the return's zero-rated lines.
**Tests:** `exclusive_16pct_adds_exactly` · `a_non_domestic_supply_is_refused` · `a_domestic_supply_claiming_zero_rating_is_refused` · `a_line_the_engine_does_not_compute_yet_is_refused` · `a_negative_rate_is_refused` · `an_overflowing_line_is_refused_not_wrapped` · `prop_exclusive_tax_rounds_once_and_adds_exactly`
**Done when:** `cargo nextest run -p pos-domain tax::tests::` exits zero: a 16% exclusive line adds its tax to the fil under the caller's rounding rule, the property holds every rate from 0% to 100% within one rounding of the exact product, and every destination but `Domestic` is refused in every store and either price mode.
> **As delivered, 26 October 2026.** The signature is the whole engine's, and the rest of the group fills it in. Until each step lands, the lines it computes are refused by name as `TaxError::NotComputedYet`, never approximated: a tax-inclusive price (1.3.4); anything but one percentage of the line net — no component, several, a fixed amount per unit, or a tax on another tax (1.3.5); and a store not registered for GST (1.3.8). Each of those steps removes its `PendingTaxShape` variant. Two refusals go beyond this step's sentence. **A domestic supply that claims a zero-rating reason is refused too** (`ZeroRatingReasonRefused`), so the reason vocabulary stays unreachable, as [`ref/domain-api.md`](ref/domain-api.md) §5 says, and no line reaches a summary under a reason the accountant has not named. **A rate below zero is refused** (`NegativeRate`): `0003` already stores none, and `Percent`'s own note assigned the check to group 1.3, which had not yet made it. The 16% in the tests is an arithmetic vector, `ref/tax-jordan.md` §4's worked example run forwards (net 1 078 → tax 172 → gross 1 250), not a seeded rate; that is 1.3.7's.

### 1.3.4 — `compute_line_tax`, inclusive mode
**Files:** `crates/pos-domain/src/tax.rs`, `crates/pos-db/src/tax_policy.rs` (new), `apps/terminal/src-tauri/src/provisioning.rs`, `apps/terminal/src-tauri/src/commands/sale.rs` · the pure half, as delivered: `crates/pos-domain/src/tax.rs` (inclusive extraction, `PendingTaxShape::InclusivePrice` removed, and every tax property given a named strategy) · [`ref/domain-api.md`](ref/domain-api.md) §5 (as delivered) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Tests:` lines, its `Full-step status:` marker and its note) · the database half, as delivered: `crates/pos-db/src/tax_policy.rs` (new — `TaxPolicyRepository::approved_for_store`, and the schema's spelling of `RoundingRule` and `RoundingDirection` mapped explicitly), `crates/pos-db/src/lib.rs` (`pub mod tax_policy;` and four `DbError` variants) · [`ref/domain-api.md`](ref/domain-api.md) (§1.2's mapping and §5's pointer, as delivered) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line, its `Full-step status:` marker and its note)
`net = gross / (1+r)`, then **`tax = gross − net` as a residual** — never rounded independently, or a receipt can fail to add up.
The existing OPEN item in [`ref/schema.md`](ref/schema.md) §0003 owns the authoritative Jordan tie rule and cash-rounding treatment. `HalfAwayFromZero` is a provisional implementation vector only: until the official source settles the question and an approved, versioned computation-policy row records its source and hash, store provisioning and sale finalization remain blocked. A merchant setting cannot choose a different result on another register.
**Tests:** `inclusive_16pct_extracts_exactly` (the 1250 → 1078 + 172 worked example) · `inclusive_extraction_is_exact_over_the_whole_fils_range` (an exhaustive arithmetic-vector loop over `{0, 1, 2, 4, 5, 10, 16}%` and gross `1..=1_000_000`, not a claim that every band is enabled for a merchant) · `prop_inclusive_net_plus_tax_equals_gross` · `prop_tax_never_exceeds_gross` · `inclusive_half_fil_tie_follows_the_rule` · `an_inclusive_price_needs_an_inclusive_capable_component` · `unapproved_tax_computation_policy_blocks_provisioning_and_finalize`
**Done when:** `cargo nextest run -p pos-domain inclusive_extraction_is_exact_over_the_whole_fils_range && cargo nextest run -p pos-db tax_policy:: && cargo nextest run -p terminal unapproved_tax_computation_policy_` exits zero after checking all seven vector rates and every gross `1..=1_000_000`, and both provisioning and finalize refuse the missing-policy fixture.
**Full-step status:** 1.3.4 is not complete until its terminal half lands. That half is `unapproved_tax_computation_policy_blocks_provisioning_and_finalize`, and it cannot be written before the two files it tests exist: `commands/sale.rs` is new at 1.8.3 and `provisioning.rs` at 1.8.5b, so it lands with whichever of them comes second; each reads the store's policy through `TaxPolicyRepository::approved_for_store` and refuses on its error. The pure and database halves are complete and enforced now. By `cargo nextest run -p pos-domain tax::tests::`: the worked example extracts to the fil, every gross from one fil to a thousand dinars at all seven vector rates agrees with integer division, net plus tax is the price for every `i64` gross and every rate to 1000% under all four rounding rules, the 12.5-fil tie parts the rules as `ref/domain-api.md` §1.2 says, and an inclusive price on a component that cannot be extracted is refused. By `cargo nextest run -p pos-db tax_policy::`: a store's approved policy is read into the rule the engine computes with, every rule and coin-step direction reads back from the schema's spelling while the serde wire spelling is refused, and a store this register does not hold, a store with no policy, an unapproved policy and a row this build cannot read are each refused by name.
> **As delivered, 26 October 2026 — the pure half.** The net is computed in `rust_decimal` and rounded once by the rule the caller passes; the tax is the residual. Two choices are recorded here. **The exhaustive loop runs two rounding rules, not four.** The quotient is the same whatever the rule; what a rule changes is which boundary the quotient must land the right side of. Half-away-from-zero meets every half-fil boundary and floor every whole one, and half-even and ceil share those boundaries, so the loop runs those two over all seven million grosses — fifteen seconds in a debug build rather than thirty — and `prop_inclusive_net_plus_tax_equals_gross` runs all four against the same integer-division oracle at any `i64`. **Every property in `tax.rs` now names its strategy**, as [`01-conventions.md`](01-conventions.md) §5 requires: 1.3.2's and 1.3.3's had used anonymous ranges. The order of this step's files is also recorded: the plan names `provisioning.rs` and `commands/sale.rs` here without `(new)`, but both are created by later steps, which is why the terminal half waits for them.
> **As delivered, 27 October 2026 — the database half.** `TaxPolicyRepository::approved_for_store` returns a `TaxComputationPolicy` only from a row with `approved_at` set, and its fields are private, so holding one is the evidence that its rule was approved. A finalize reads it through its own transaction, so the rule its lines are computed under and the policy id the sale records, which `0005` holds to the store's, come from one snapshot. Three refusals are named. `TaxPolicyStoreUnknown` is today's likeliest: `0003` refuses to write a store without an approved policy, and none is approved until the `⚠️ OPEN` item is answered, so a register being set up finds no store at all. `TaxPolicyMissing` and `TaxPolicyUnapproved` are what `0003`'s triggers and foreign key already refuse when a store is written; the read checks again because the read is what a sale's arithmetic depends on, and the tests prove it on scratch databases with the trigger or the foreign key removed. **The mapping** follows [`ref/domain-api.md`](ref/domain-api.md) §1.2 in the shape `repo/trade_scale.rs` first gave `VerificationState` at 1.2.4: an exhaustive `const fn` to the schema's spelling, and the reverse found through that same function. **One discrepancy is recorded, not resolved.** The coin step and direction are read from the policy row because `0003` stores them there, but `0003`'s own comment, [`ref/tax-jordan.md`](ref/tax-jordan.md) §5 rule 1 and [`ref/merchant-decisions.md`](ref/merchant-decisions.md) 2.1–2.2 place them on `store` as effective-dated settings, and `store` has no such column. Until a migration moves them, a merchant's coin step changes only with a new policy version. `cash_round_tax_treatment` is not read, because its vocabulary is the `⚠️ OPEN` item in [`ref/tax-jordan.md`](ref/tax-jordan.md) §5 that 2.7.0 owns, and neither are `source_ref` and `content_hash`, because nothing on the register checks a policy's provenance yet.

### 1.3.5 — Multiple components per line
**Files:** `crates/pos-domain/src/tax.rs` · as delivered: `crates/pos-domain/src/tax.rs` (several components in application order, fixed amounts, tax on tax, incomplete zone packs, and three `PendingTaxShape` variants removed) · [`ref/domain-api.md`](ref/domain-api.md) §5 (as delivered, the new `TaxError` variants, `PendingTaxShape`, and the Special Sales Tax `⚠️ OPEN` item) · [`00-master-plan.md`](00-master-plan.md) (that item's row in the Phase-1 table) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Tests:` lines, and its note)
Represent ordered ad-valorem and fixed-per-quantity components, their base dependency, taxable quantity and carried component base. GST-on-SST cannot be represented as two percentages over one base, and retrofitting the component snapshot through live sale lines would be a migration of every receipt and refund. No SST rule is seeded until the official regulation and merchant tax-point evidence settle it; the engine fails closed meanwhile.
**Tests:** `prop_multi_component_line_sums_correctly` · `prop_per_unit_component_scales_with_quantity` · `sst_fixed_and_ad_valorem_components_compound_in_order` · `gst_base_includes_sst` · `tax_component_dependency_cycle_is_refused` · `an_incomplete_profile_pack_fails_closed` · `a_malformed_component_list_is_refused` · `an_inclusive_price_on_several_components_is_refused` · `inclusive_fixed_amounts_extract_exactly` · `prop_inclusive_fixed_amount_extracts_exactly`
**Done when:** `cargo nextest run -p pos-domain tax::tests::` exits zero, including the fixed-plus-ad-valorem compound fixture and the dependency-cycle refusal.
> **As delivered, 27 October 2026.** The engine follows the default arrangement of the Special Sales Tax `⚠️ OPEN` item in [`ref/domain-api.md`](ref/domain-api.md) §5, and reads order and bases from the rules, so answering it changes data rather than code; only a fixed charge per line, rather than per unit, would need a new basis. No such rule is seeded. Five choices are recorded here. **Each component rounds once, on a base made of amounts already carried**, so every component re-derives from its own persisted base; §5's "rounded once per line" is now "once per carried amount", which is the same thing for one component. **A tax-inclusive price on several components is refused** (`InclusiveSplitUndefined`). With more than one, the fil left over by rounding has to land in one of them, and that is a tax fact no source states, so the question joins the `⚠️ OPEN` item. One component of any shape is extracted: the net is the price less the fixed amount, over one plus the rate, rounded once, and the tax is the residual. No v1 line is affected, because v1 charges GST alone. **A component may only be charged on one with a lower `sequence`.** A dependency the other way is `CircularComponentBase` when it closes a loop and `BaseComponentNotEarlier` when it does not; a code named twice in one base counts once; a code twice on one line is `OverlappingRules`; no component at all is `NoComponents`; and a fixed amount larger than an inclusive price is `FixedTaxExceedsPrice`. `PendingTaxShape` keeps only 1.3.8's variant. **An incomplete zone pack is a zone store whose pack lacks a component the standard pack charges on that category.** The item's default also said a store whose assortment needs a special tax "fails closed on `ProfilePackIncomplete`", and the engine cannot do that: its inputs carry no liability flag, so a special-tax category with only a GST rule resolves to GST. That refusal belongs to the pack's approval at 1.3.7 ([`ref/tax-jordan.md`](ref/tax-jordan.md) §3 rule 6) and to §2's checkout block on a Special-Tax-liable product, which no microstep owns yet. **The shipped schema does not yet hold every shape the engine computes**, and whoever maps rules in (1.3.7) and persists lines (1.8.3) reconciles it: `tax_rate` and `sale_line_tax` store one `calculation_kind` per row, so a `Compound` component is two rows; `line_net_plus_prior_components` means every component with a lower order rather than the codes a base names; a fixed charge is per `fixed_basis_qty_milli` rather than per unit; and `UNIQUE (sale_line_id, calculation_order)` does not admit two components sharing a sequence.

### 1.3.6 — `summarize_tax`
**Files:** `crates/pos-domain/src/tax.rs` · as delivered: `crates/pos-domain/src/tax.rs` (`summarize_tax`, `TaxError::LineTaxUnbalanced`, and `TaxError::UntaxedTreatmentCharges` in `compute_line_tax`) · [`ref/domain-api.md`](ref/domain-api.md) §5 (as delivered: the `Result`, what a row's net is, and the two new variants) · [`ref/tax-jordan.md`](ref/tax-jordan.md) §4 (the grouping key, and what a row's net is) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Tests:` lines, and its note)
Grouped by `(component, treatment, rate)`. The **exact sum** of line taxes, never re-derived.
**Tests:** `prop_line_tax_sum_equals_receipt_tax` · `prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting` · `exempt_and_zero_rated_lines_make_two_summary_rows` · `summary_rows_are_keyed_by_every_rate_defining_fact` · `a_stacked_line_summarizes_each_component_on_its_own_base` · `a_summary_refuses_a_line_that_disagrees_with_itself` · `a_summary_refuses_mixed_currencies_and_overflow` · `an_untaxed_treatment_that_charges_is_refused`
**Done when:** exempt and zero-rated items on one receipt produce two summary rows, not one.

> **As delivered, 27 October 2026.** A row is keyed by five facts, not the three above: the component, its treatment, its rate, its fixed amount per unit and the line's supply reason. [`ref/domain-api.md`](ref/domain-api.md) §5 and [`ref/tax-jordan.md`](ref/tax-jordan.md) §4 already named the last two, because two fixed amounts at one percentage are two rates in law and the zero-rated box splits by reason. Four choices are recorded here. **A zero-rated or exempt component that charges anything is refused**, by `compute_line_tax` as `UntaxedTreatmentCharges`: nothing stopped a pack's exempt component from carrying 16%, which the engine would have charged and the summary reported as exempt, so the property's "produce zero tax" was true only of the rates it was handed. A rate of 0% or a fixed amount of nothing is what such a component charges. **A row's net is the base its rate was charged on**, the sum of its components' carried `base_amount`, and its gross is that net plus its tax, so each row reads as a rate on its own base, which is what a return needs. On a receipt of one-component lines, which is every receipt v1 sells, the rows add up to the receipt in net, tax and gross; a line carrying several components counts its net under each, and only the taxes add up across it. **It returns a `Result`**, where §5 first read `Vec`: a sum can overflow, lines can come in two currencies, and a line whose components no longer add up to its tax, or whose net and tax no longer add up to its gross, is refused as `LineTaxUnbalanced` rather than summarized into a receipt that disagrees with itself; a line with no component is `NoComponents`. **Rows come back ordered** by code, then treatment, rate, fixed amount and reason, so the order a basket was scanned in never moves a row. How a row maps onto `sale_tax_summary`, which stores a compound component as two rows and carries `taxable_base_minor` beside `net_minor`, is 1.8.3's, with the storage gaps 1.3.5 recorded.

### 1.3.7 — Seed the Jordanian tax data
**Files:** `crates/pos-db/src/tax_pack.rs` (new), `crates/pos-db/tests/tax_pack.rs` (new) · the import path, as delivered: `crates/pos-db/src/tax_pack.rs` (`PinnedTaxPack`, `content_hash`, `import_tax_pack` and `TaxPackRepository::components_for`) · `crates/pos-db/src/lib.rs` (the module, `DbError::TaxPackRefused` and the read's refusals) · `crates/pos-db/tests/tax_pack.rs` (on a synthetic fixture) · `crates/pos-domain/src/lib.rs` (`summarize_tax` re-exported, which 1.3.6 left out) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Tests:` lines, its `Full-step status:` marker and its note)
Migration `0003` creates and seeds only the closed structural treatment/category vocabulary in 1.2.1; this later step never edits that migration. Every enabled band, including the standard band, arrives through one pinned pack carrying the official source version and hash, real effective dates and dated accountant approval:

```rust
pub fn import_tax_pack(tx: &Transaction, pack: &PinnedTaxPack,
                       approval: &AccountantApproval) -> Result<TaxPackVersion, DbError>;
```

Do not hard-code `STD16` or `RED04` as an unevidenced regulatory row, invent `valid_from` from the store's go-live date, or let a generic rule fall through to ASEZ/development profiles. `Zero` and `Exempt` remain distinct treatments, but an actual zero/exempt category still needs the pack's classification evidence. An unknown category, an unconfigured band or an incomplete jurisdiction pack fails closed because substituting a familiar percentage silently overcharges the customer.
The existing OPEN item in [`ref/schema.md`](ref/schema.md) §0003 owns which current categories and fixed/percentage components the merchant may enable; until its official catalogue and accountant-approved classification arrive, the default is an empty regulatory pack that cannot price a taxable live sale.
**Tests:** `imported_rule_records_source_version_hash_and_approval` · `standard_rate_records_the_same_pack_evidence_as_reduced_rates` · `unconfigured_reduced_band_fails_closed` · `asez_profile_without_complete_pack_fails_closed` · `fixture_covers_every_enabled_band` · `a_pack_that_is_not_what_was_pinned_is_refused` · `a_pack_hashes_the_same_whatever_order_it_lists_its_bands_in` · `a_malformed_pack_is_refused_and_writes_nothing` · `a_pack_that_disagrees_with_the_register_is_refused` · `a_store_rate_this_build_cannot_read_is_refused_not_guessed` · `a_store_on_a_retired_pack_is_refused_even_without_its_trigger`
**Done when:** `cargo nextest run -p pos-db --test tax_pack` passes against a fixture containing every merchant-enabled band and fails after any one band or approval record is removed.
**Full-step status:** 1.3.7 is not complete until the merchant's real pack is imported through this path: a pinned snapshot of the current official ISTD catalogue, every band the merchant enables, and their accountant's dated classification, which is what [`ref/schema.md`](ref/schema.md) §0003's `⚠️ OPEN` item asks for. Until then no pack is approved, no store can be provisioned, and the `Done when` passes against a visibly synthetic fixture standing in for the merchant's bands: `fixture_covers_every_enabled_band` holds it to every band of `ENABLED_BANDS` and fails without any one band or without its approval. When the real pack arrives, it replaces the fixture and that list becomes the merchant's.

> **As delivered, 28 October 2026 — the import path.** Seven choices are recorded here.
> - **A pack says one thing per band**, in the format `pos-tax-pack/1`: one component, a percentage of the line's net, effective over business dates. That is every Phase-1 band: standard, reduced, zero-rated and exempt general sales tax. A fixed amount, a second component or a tax on a tax cannot be written in it. Those are Special Sales Tax, which [`ref/tax-jordan.md`](ref/tax-jordan.md) §2 keeps out and whose base is [`ref/domain-api.md`](ref/domain-api.md) §5's `⚠️ OPEN` item; their arrival is a new format, hashed differently.
> - **The pin is BLAKE3 of the pack's canonical content**: everything but the pin, as sorted-key JSON with bands and rates sorted, so the order a pack lists them in does not move it and every value it states does. A pack that no longer hashes to its pin is refused before anything is read or written.
> - **The approval is the accountant's date, recorded by a user.** `0003` stores `approved_by` and `approved_at`. The accountant is not a user of the register, so `approved_by` names the user who records their sign-off, and the date is the accountant's. Nothing stores the accountant's own name or the signed document yet; that is a column for when the real approval arrives.
> - **`import_tax_pack` takes an `IdSource`** beside the signature above, because `pos-db` mints no ids of its own.
> - **A store's rates are read by business date.** `0003` stores a rate's period as business dates and `0005`'s completion gate compares them with `sale.business_date`; `TaxPackRepository::components_for` filters exactly as that gate does and returns the engine's components. `TaxRateRule` carries instants, so `resolve_components` is not on the database path: the store's pack is already its profile's, and its periods were checked when it was imported. The step that prices a line from the database (1.8.3) calls `components_for`.
> - **A zone pack is complete against the approved standard pack**: it must charge every band that pack charges, or it is refused, and with no approved standard pack it cannot be shown complete and is refused too.
> - **Not yet in a pack**: §3 rule 6 also names the return-box mapping and the filing calendar, which are 4.7.2's `tax_filing_profile`, and the command that will call this import, `tax_rate_upsert` under `tax.rate.edit` with its approval handle and audit row ([`ref/ipc-contract.md`](ref/ipc-contract.md), Phase 3), is not built. This is the repository function it will call.

### 1.3.8 — `unregistered` profile short-circuit
**Files:** `crates/pos-domain/src/tax.rs` · as delivered: `crates/pos-domain/src/tax.rs` (the short-circuit, `PendingTaxShape` and `TaxError::NotComputedYet` retired, and `summarize_tax` given an unregistered store's receipt) · `crates/pos-domain/src/receipt.rs` (the fiscal-independence test, beside the receipt fixture it uses) · `crates/pos-domain/src/lib.rs` (the re-export) · [`ref/domain-api.md`](ref/domain-api.md) §5 (as delivered) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Tests:` lines and its note, and the notes it leaves at 1.8.3 and 1.8.5b)

```rust
pub fn compute_line_tax(
    taxable: Money, qty: Qty, mode: PriceMode, components: &[TaxComponent],
    profile: StoreTaxProfile, supply: &SupplyTaxContext, rule: RoundingRule,
) -> Result<LineTax, TaxError>;
```

This controls GST calculation only. JoFotara obligation and its income/general/special taxpayer category are separate evidenced settings; being GST-unregistered never disables fiscal issuance by implication.
**Tests:** `prop_unregistered_profile_yields_no_tax` · `unregistered_gst_profile_does_not_disable_fiscal_obligation` · `an_unregistered_store_charges_no_gst` · `unregistered_lines_summarize_to_no_tax_rows`
**Done when:** `cargo nextest run -p pos-domain unregistered_` passes with independent GST and fiscal-profile fixtures.

> **As delivered, 28 October 2026.** A store not registered for GST charges none. Once its supply is read, `compute_line_tax` returns its line with no component, no tax, and a net and gross that are both the price, in either mode, and never reads the components it was handed; `prop_unregistered_profile_yields_no_tax` holds that for any components, price, quantity and rule. Three choices are recorded here. **The supply is still read first**, so the owner's refusal of every non-domestic supply (`ref/merchant-decisions.md` 6.14) binds every store, registered or not. **The line carries no component**, not an exempt one: an exempt row is a box on a return this store does not file, so `summarize_tax` now gives a receipt of such lines no rows at all, and refuses one beside taxed lines as `NoComponents`, because a store is registered or it is not. **`PendingTaxShape` and `TaxError::NotComputedYet` are gone**, as their own documentation said they would be with the group's last pending shape. `unregistered_gst_profile_does_not_disable_fiscal_obligation` lives in `receipt.rs`, beside the receipt fixture it builds on: every pairing of an unregistered or registered store with a receipt that does or does not carry an invoice block is a valid sale document, and the block is always what the fiscal fixture says. Three findings leave this step for the database and the shell, recorded at 1.8.3 and 1.8.5b: `0005` refuses a completed line with no tax component, `StoreTaxProfile` folds two columns `store` keeps apart, and `store.gst_profile` defaults to `'unregistered'` where merchant decision 6.2b assumes a registered merchant.

---

## Group 1.4 — The cart state machine

*Blueprint §8 as a Rust enum. Illegal transitions do not compile.*

### 1.4.1 — The `Sale` enum and `Cart` / `CartLine`
**Files:** `crates/pos-domain/src/cart.rs` (new) · as delivered: `crates/pos-domain/src/cart.rs` (`Sale`, `Cart`, `CartLine`, `CartContext` and `open`) · `crates/pos-domain/src/lib.rs` (the module and its re-exports) · [`ref/domain-api.md`](ref/domain-api.md) §6 (as delivered, `CartContext`, and the module cycle 1.4.8 inherits) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines and its note, 1.4.2's lines, and the notes left at 1.4.8, 1.5.2, 1.7.1 and 1.8.3)
Per API reference §6. `shift_id` is non-optional, because `business_date` has no meaning without its shift. `SupplyTaxContext` is captured as `Domestic` in Phase 1; non-domestic supplies remain refused until their reason and evidence path exists. `is_training` is on the cart from the first commit and checked by every report and later fiscal path.
**Tests:** `a_new_cart_is_empty_domestic_and_its_context_is_copied` · `a_training_cart_says_so_from_its_first_moment` · `golden_cart_json_is_stable` · `a_sale_round_trips_in_either_state` · `buyer_name_never_reaches_debug_output`
**Done when:** `cargo nextest run -p pos-domain cart::tests::` exits zero, proving that `open` builds an empty, `Domestic`, `Building` cart carrying its shift and training flag, that a cart's wire format is pinned, and that no `Debug` rendering of a cart reveals a buyer name.

> **As delivered, 28 October 2026.** The plan named no test and no `Done when` for this step; the lines above are its own. **The enum holds the states whose data exists**, `Building(Cart)` and `Parked(Cart)`: `Tendering`, `Finalizing`, `Complete` and `Voided` arrive with their transitions at 1.4.8, and each later field of a cart or a line with the step that gives it meaning — discounts at 1.4.5, a price override at 1.4.7, a promotion group when promotions are first captured. §6 specified those payloads only in part, and a type invented before its transition would be a guess the transition then had to live with. **`open` and `CartContext` land here**, because a cart that is `Domestic`, carries its shift and knows whether it is training from its first moment is what this step's own description asks for. **`Cart`'s `Debug` redacts `buyer_name`**, as the receipt's buyer block does. **`build_receipt_model` is assigned to 1.8.3**, its first consumer, as 1.7.1's note asked of whoever built this step.

### 1.4.2 — `CartError`
**Files:** `crates/pos-domain/src/cart.rs`
Every variant from API reference §6.3. Exhaustive and data-carrying — the UI renders from `code`, not by parsing a message.
**Tests:** `cart_errors_carry_their_data` · `tax_and_money_errors_pass_through_unchanged`
**Done when:** `cargo nextest run -p pos-domain cart::tests::` exits zero with every variant of §6.3 present and each one's data in its message. The code a screen renders from is the shell's mapping of the variant (1.4.11), so the domain carries no code string.

### 1.4.3 — `add_line`, `set_qty`, `void_line`
**Files:** `crates/pos-domain/src/cart.rs` · as delivered: `crates/pos-domain/src/cart.rs` (`AddLine` and its product and scan constructors, `VoidReason`, `add_line`, `set_qty`, `void_line`, `check_age_confirmed`, and two `CartError` variants) · `crates/pos-domain/tests/cart_ui.rs` and `crates/pos-domain/tests/ui/add_line_cannot_be_deserialized.rs` (new) · [`ref/domain-api.md`](ref/domain-api.md) §6.1 and §6.3 (as delivered) · `scripts/check-test-catalog.py` (this microstep's two `PLANNED` names retired) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Tests:` lines and its note, and 1.2.4's retired `Full-step status:` marker)
`add_line` copies `name_snapshot`, the resolved `unit_price` and `price_origin` onto the line (I-5). It takes no caller-selected amount, refuses inactive products for *adding* while leaving refunds unaffected (E.38), and inserts an age-restricted line only in a visibly blocked state until 1.4.3b confirms it. Tender refuses any blocked line. Keeping the pending item in the cart makes the exact scanned pack and quantity visible while the cashier checks evidence; silently rejecting it would lose that context and invite a second scan.
**Tests:** `add_line_snapshots_name_and_price` · `client_supplied_price_cannot_bypass_override` · `inactive_product_cannot_be_added_but_can_be_refunded` · `age_restricted_line_requires_confirmation` · `set_qty_zero_is_rejected` · `prop_discrete_products_never_carry_a_fractional_quantity` · `void_line_removes_it_and_says_who_and_why` · `a_scan_adds_its_code_s_pack_quantity_or_its_label_s_weight` · `an_add_line_never_arrives_from_outside_with_its_price`
**Done when:** `cargo nextest run -p pos-domain cart::tests:: && cargo nextest run -p pos-domain --test cart_ui` exits zero, proving that a line keeps the name and price it was captured at, that no caller-chosen price and no inactive product enters a cart, that an age-restricted line is held blocked, that a counted unit is never fractional by any entry path, that a void is audited, and that a price label is one package (the plan gave this step no `Done when`; this is its own).

> **As delivered, 28 October 2026.** `add_line` is the one place a line enters a cart, and every refusal is there: a department price or a line with no product (1.4.12's capped path), a quantity of zero or less, a fraction of a counted unit, a price in another currency. An inactive product is refused by `AddLine`'s constructors, so it cannot be described at all, while a line already captured never reads the catalogue again — that is what keeps it refundable, since a refund reads the line (I-5), and the test shows the captured line unchanged after deactivation. Five choices are recorded here. **An age-restricted line goes in blocked**, as this entry says, where §6.1 had said `add_line` refused it; `check_age_confirmed` is what 1.4.8's `begin_tender` calls. **A price label is one `Package`** at the label's price with its derived weight on the line, which is 1.2.4's last test, so 1.2.4 completes here. **`set_qty` refuses a label's line**, which is the one package its label was printed for, and a department line, whose quantity would multiply a typed amount; both are new `CartError` variants, with `ScanNotOneProduct` for a scan that found several products, none, or a damaged label. **`void_line` takes `at`**, because its audit intent records when and this crate reads no clock; its payload carries the line's ids, quantity and price, never its name. **`AddLine` has no `max_price` and no `age_confirmed`**: the ceiling is 1.4.7's to check when a price changes, and only an operator confirms an age. A compile-fail fixture proves `AddLine` cannot be deserialized, so no request from the screen carries a price into a line.

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
**This step completes the `Sale` enum 1.4.1 began.** It adds `Tendering`, `Finalizing`, `Complete` and `Voided` with the payload types its transitions need, and breaks the module cycle [`ref/domain-api.md`](ref/domain-api.md) §6 records: `Tendering` lives in `cart.rs` with the rest of the state machine, beside 1.5.2's functions over it, so `cart` depends on `tender` and §15's arrow between them reverses.
**`add_tender` inherits an obligation from 1.5.3.** `final_tender_rounding` answers what a tender of a given kind is asked for *if* it settles the sale; whether it settles is decided here. A cash tender that covers the rounded remainder is final, and its `CashRounding` is what `Tendering.cash_rounding` stores; one that does not is a partial cash tender, applied exactly, and the next tender is asked again — so a sale carries at most one rounding and never rounds a card. A remainder under half a step is asked as `0.000` under `Nearest`, and one under a whole step under `Down`, so the final cash tender can be empty and the adjustment alone settles the sale: accept that as final rather than refuse it as an empty tender. §6.1's `add_tender(t, tender)` carries no step and no direction, so this step also decides where they reach the transition from. They live on `tax_computation_policy` (`0003`), which a store references — not on `store`, whatever [`ref/tax-jordan.md`](ref/tax-jordan.md) §5 rule 1 and merchant decisions 2.1–2.2 say, and whatever `0003`'s own comment says two lines above the columns.
**Tests:** `back_to_building_denied_after_first_tender` · `complete_requires_settled` · `prop_no_operation_mutates_a_complete_sale` (I-4)

### 1.4.9 — `price_cart`
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-domain/src/cart.rs`, `crates/pos-domain/benches/price_cart.rs` (new), `crates/pos-domain/Cargo.toml` (`criterion`) — its baseline, `benchmarks/baselines/price-cart.json`, is 5.8.2's
The one function that turns a cart into money — the sole source of every number on the receipt *and* in the fiscal document.
**Tests:** `prop_total_equals_lines_minus_discounts_plus_tax` · `prop_price_cart_is_deterministic` · `prop_price_cart_is_invariant_under_line_reordering` (E.19b) · `prop_zero_total_cart_is_valid` (E.18)
**Bench:** `crates/pos-domain/benches/price_cart.rs` — 200 lines, p99 < 16 ms on the reference register (G-9), proven at 5.8.2.
**Done when:** `cargo nextest run -p pos-domain cart::tests::prop_ && cargo test -p pos-domain --bench price_cart` exits zero, the second running the stable 200-line fixture once; `just bench-gate price-cart` on the reference register is 5.8.2's.

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
**Files:** `crates/pos-domain/src/tender.rs` (new) · `crates/pos-domain/src/lib.rs` (`pub mod tender;` and the re-export block) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
Per API reference §7. `TenderState` includes `Pending` from day one so Phase 2's CliQ callback case (E.65) is a state, not a schema change.
**This is 1.6.3 in reverse, and the direction decides which side gives way.** 1.6.3's grid is what 1.6.1 wrote its seed *from*; here the **seed came first**, inside migration `0005` at 1.9.1, and a committed migration is never reopened. So §7.1's table is written to match `0005`'s `tender_type` insert column for column, and a disagreement is fixed in the domain type.
**The domain type carries the behavioural half only.** `tender_type` has ten columns and §7's `TenderType` has six: `name_ar`, `name_en`, `sort_order` and `is_active` are display, ordering and row state. `is_active` is the one worth naming — the seed ships `cash` active and the other five inactive because *"later payment/refund microsteps enable behavior; they never reopen 0005 to append codes"* — so activation is a store's row state rather than a property of a kind of tender, and putting it on a pure type would invite a pure function to answer a question only the database can.
**`is_internal` is not `is_cash_counted == false`.** `0005`'s own comment: *"`is_internal` is NOT implied by `is_cash_counted = 0`: `card` and `cliq` count no drawer cash but do move real value through a PSP."* `exchange` must carry both, or §7.1's failure follows — *"the offset would appear in expected drawer cash on both documents and the shift would close short by twice the exchanged value."*
**No property test, and that is conventions §5.1 rather than an omission.** The rules here are bounded — six rows, six columns — and §5.1 says a bounded universal claim uses an exhaustive `#[test]` loop when that loop is feasible. A generator over six fixed rows would sample what a loop enumerates. The unbounded rules are 1.5.2's and 1.5.3's and already carry `prop_` names.
**Tests:** `the_standard_grid_matches_the_committed_seed` · `only_cash_opens_the_drawer_and_gives_change` · `an_internal_tender_is_not_merely_uncounted_cash` · `an_internal_tender_can_be_refunded_nowhere` · `every_standard_code_is_unique_and_resolvable` · `tender_state_carries_pending_from_day_one` · `a_tender_carries_only_what_a_card_may_leave_behind` · `a_tender_round_trips_through_canonical_json` · `the_grid_is_exhaustive_over_every_routing_it_uses` · `the_wire_form_and_the_storage_form_are_the_same_word`
**Done when:** `cargo nextest run -p pos-domain tender::` exits zero, proving the six standard tender types match migration `0005`'s committed seed column for column, that `cash` is the only one that opens a drawer or gives change, that `exchange` is the only internal one and that being internal is not the same predicate as counting no drawer cash, and that a `Tender` can carry nothing from a card beyond the PSP reference, the masked PAN and the scheme.

### 1.5.2 — `remaining_due`, `change_due`, `is_settled`
**Files:** `crates/pos-domain/src/tender.rs`
> **Recorded at 1.4.1: these live in `cart.rs`, not `tender.rs`.** They take a `Tendering`, which holds a `Cart`, and `Sale::Tendering` holds a `Tendering`; in `tender.rs` they would make `tender` and `cart` depend on each other. 1.4.8 places `Tendering` in `cart.rs` ([`ref/domain-api.md`](ref/domain-api.md) §6), and these three go beside it.
Split tender is the **core model**, not a feature: a sale holds `Vec<Tender>` until collected ≥ due. Bolting it on later deforms the whole checkout (master plan C.4).
**Tests:** `prop_split_tender_sums_to_total` · `prop_change_never_negative` · `overtender_only_allowed_for_cash`

### 1.5.3 — Cash rounding
**Files:** `crates/pos-domain/src/tender.rs` · `crates/pos-domain/src/lib.rs` (the tender re-export block) · [`ref/domain-api.md`](ref/domain-api.md) (§7 — `final_tender_rounding`, and the refused negative remainder) · [`README.md`](README.md) (implementation frontier) · `scripts/check-test-catalog.py` (this microstep's two `PLANNED` names retired) · this file (this microstep's `Files:` and `Tests:` lines, the obligation it writes onto 1.4.8, and 1.1.6's clause claiming this step builds `round_to_step`)
`compute_cash_rounding`, applied only when the **final** tender is cash and only to the remaining cash amount. The existing cash-rounding-treatment OPEN item in [`ref/tax-jordan.md`](ref/tax-jordan.md) §5 keeps the default tender-level adjustment provisional; the POS does not silently move it into a tax base or fiscal line.
**"Final" is a property of a sequence, and the sequence type does not exist yet.** §7's `Tendering` holds a `Cart` and a `PricedCart` (1.4.1, 1.4.9), so this step states the E.14 rule over the vocabulary 1.5.1 left behind: `final_tender_rounding(kind, remaining, step, dir)` answers what the tender that settles a sale is asked for — a cash tender the remainder moved to the coin step, any other kind the exact remainder and `None`. Its result is the `Option<CashRounding>` that `Tendering.cash_rounding` holds, so 1.4.8's `add_tender` stores it rather than re-deriving it. Deciding *which* tender is final is 1.4.8's, and its entry carries that obligation.
**The rule follows `is_cash_counted`, not the code `"cash"`.** Rounding exists because an amount paid in coin has to be one the coins can make, and `is_cash_counted` is the flag that makes a tender's amount coin: [`ref/domain-api.md`](ref/domain-api.md) §11 counts `amount − change` into expected drawer cash over exactly those tenders, with no rounding term of its own because the rounded amount arrives as the tender's amount. Every other kind can carry any fil, so rounding it would charge fils nothing required. [`ref/tax-jordan.md`](ref/tax-jordan.md) §5 rule 5 pairs the two — the `exchange` tender *"is never cash-counted and never receives cash rounding"*. On today's six-row grid four predicates select the same row, so constructed kinds are what make the choice a test rather than a coincidence.
**A negative remainder is refused, because it is change.** A sale's remainder is never negative, and rounding a negative balance would be rounding change — the confusion `cash_overtender_and_change_are_separate_from_rounding` exists to prevent. A refund payout is `compute_refund_rounding`, 2.3.3's, with its own direction default.
**A zero adjustment is still `Some`.** The tender was cash and nothing moved. 1.7.1's receipt model refuses a zero rounding line, so whatever maps a sale into it filters zero.
**Tests:** `prop_cash_rounding_only_on_final_cash_tender` (E.14) · `prop_rounding_adjustment_keeps_total_exact` · `card_charged_exact_unrounded_total` · `mixed_tender_1247_card_624_cash_620_adjustment_minus_3` · `half_away_tie_1245_rounds_to_1250` · `cash_overtender_and_change_are_separate_from_rounding` · `rounding_follows_the_drawer_count_not_the_code` · `each_direction_moves_the_remainder_its_own_way` · `an_already_payable_remainder_is_rounded_by_nothing` · `an_unroundable_request_is_refused_whatever_the_tender` · `a_cash_rounding_round_trips_through_canonical_json`
**Done when:** `cargo nextest run -p pos-domain mixed_tender_1247_card_624_cash_620_adjustment_minus_3` exits zero, proving that a `1.247` JOD sale charged `0.624` to card leaves `0.623`, rounds the final cash tender to `0.620`, records `-0.003`, and settles `1.244` exactly.

### 1.5.4 — Denomination helper
**Files:** `crates/pos-domain/src/tender.rs` · `crates/pos-domain/src/lib.rs` (the tender re-export block) · [`ref/domain-api.md`](ref/domain-api.md) (§7 — `denominations`, which the reference does not yet name) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
JOD denominations (50, 20, 10, 5, 1 dinar; 500, 250, 100, 50, 25, 10 fils) for the numpad quick-keys and the shift-close count grid.
**Keyed by currency, and `None` is not an error.** `denominations(currency) -> Option<&'static [Money]>`. The table is currency data (I-2): a USD amount has two minor digits, so the dinar's table applied to it would be wrong by a factor of ten. A currency with no table gets `None`, meaning *no helper*. Master plan E.17 says why that is enough: *"system doesn't care about denominations for correctness, but count helper does"*, so the numpad works without quick-keys and the count grid without rows.
**The eleven values are the plan's, transcribed, with no 5-fil row.** `the_smallest_denomination_is_one_qirsh` ties the table to merchant decision 2.1's default ten-fil step. A store whose answer to 2.1 is a five-fil step changes that test and this table together, deliberately.
**The plan's set is not consistent with its own step, and the first test run found it.** This entry's docs-first commit named a test *"every denomination is a whole number of qirsh"*, and the plan's table failed it: 25 fils is not. A customer who hands a 25-fil piece over against a remainder rounded to `0.020` is owed five fils of change, and no piece in the table pays it. Either the 25-fil piece is not in everyday use or the 5-fil piece is and belongs here. B.5 already says *"Verify the store's actual coin practice with the merchant"*, and #237 carries the question. The test was corrected to what is true, the values stay the plan's until #237 is answered, and the inconsistency is written on the function rather than asserted by a test of the hole.
**The table is not a greedy change-maker, and nothing may use it as one.** With 25 fils and no 5 fils, counting out largest-first fails on 80 of the 200 qirsh-multiples up to 2.000 JOD: 30 fils takes 25 and strands a 5 no coin can cover, where the answer is 10 + 10 + 10. Quick-keys add and the count grid counts, so nothing here makes change. The prohibition is written on the function rather than asserted by a test, because a test of a hole has to be deleted the day the hole closes.
**No property test, by conventions §5.1** — eleven rows is a bounded claim and an exhaustive loop enumerates it, the same call 1.5.1 made.
**Tests:** `denominations_are_descending_and_complete` · `the_smallest_denomination_is_one_qirsh` · `no_other_currency_borrows_the_dinar_denominations`
**Done when:** `cargo nextest run -p pos-domain denomination` exits zero, proving the JOD table is exactly 50, 20, 10, 5 and 1 dinar and 500, 250, 100, 50, 25 and 10 fils, largest first, that its smallest piece is one qirsh, and that no other currency is given the dinar's table.

---

## Group 1.6 — Users, permissions, audit

*Gaps G-6 and G-7. Independent of the cart work.*

> **Build order inside this group is not step order.** Build the pure PIN hash/verify/benchmark
> half of 1.6.2 and define the capability matrix in 1.6.3, then commit migration `0004` in 1.6.1
> with that matrix. Complete the database tests in 1.6.3 and the approval machinery in 1.6.4
> before adding `user_reset_pin` from 1.6.2. The command cannot bind an `ApprovalHandle` that has
> not been implemented yet, and the migration cannot be reopened later to add its role seeds.

### 1.6.1 — Migration `0004`
**Files:** `crates/pos-db/migrations/0004_people_and_audit.sql` (new), `crates/pos-db/src/lib.rs`, `crates/pos-db/tests/migration_0004_people_and_audit.rs` (new), `crates/pos-db/tests/common/mod.rs`, `crates/pos-db/Cargo.toml`, `Cargo.lock`, `apps/server/migrations/<14-digit>_people_and_audit.sql` (new), `docs/implementation/ref/schema.md`, `crates/pos-domain/src/permissions.rs`
Per [`ref/schema.md`](ref/schema.md) §0004, including the complete role/capability matrix defined by 1.6.3. Design that matrix first and land its seed here, because adding it to `0004` after this microstep commits would violate the forward-only migration law. Note `app_user`, not `user` — reserved in Postgres.
> **Why the `Files:` line is nine entries and not one.** Shipping a migration is never one file: it
> registers in `MIGRATIONS`, moves `tests/common/mod.rs`'s reference-replay boundary, marks its
> §0004 heading `SHIPPED` so `verify-schema` stops replaying it, and gains a Postgres counterpart
> or `verify-pg-migrations.py` refuses it. 1.2.1 recorded the same correction after PR #56.
> The two entries that were not obvious in advance: `crates/pos-db/Cargo.toml` gains
> `pos-domain` as a **dev**-dependency, because `every_capability_in_cap_all_has_a_seeded_row`
> iterates `cap::ALL` rather than a second hand-typed list of capability names, and `Cargo.lock`
> records that edge or `--locked` refuses the build.

**The three shape decisions, settled before the SQL, because `0004` cannot be reopened.**
`role_capability` carries **one row per (role, capability) cell — 128 rows, not 75**. The column is
`decision TEXT NOT NULL CHECK (decision IN ('granted','withheld','sets_the_limit'))` with **no
default**: a default turns "nobody decided" into a silent denial that no query can distinguish from
a deliberate one, and the forward-only law makes that permanent. Three values rather than a
boolean because `Grant::SetsTheLimit` is a distinct decision from `Grant::Withheld` — the owner
defines the ceiling others work under — and a binary column collapses the two.
`role.code` gains a real `CHECK (code IN ('cashier','shift_lead','manager','owner'))`, which
`crates/pos-domain/src/permissions.rs` already documents as existing. The four `role.id` values are
**deterministic UUIDv7-shaped literals**, not `randomblob(16)`: the same logical role must carry the
same id on every register, or synced `role_capability` rows cannot be reconciled centrally.
**Tests:** `migration_0004_creates_all_tables` · `every_capability_in_cap_all_has_a_seeded_row` · `every_role_carries_an_explicit_grant_for_every_capability` · `audit_log_refuses_update_and_delete` · `an_approval_handle_can_be_consumed_only_once` · `the_user_table_is_named_app_user`
**Done when:** `just verify-schema` applies `0001`–`0004` and `cargo nextest run -p pos-db --test migration_0004_people_and_audit` passes, with **every** `cap::ALL` entry present in `capability` and **every one of the four roles carrying an explicit grant or denial for each** — so a capability added after this migration cannot inherit a silent default, which is the failure the forward-only law makes permanent.

### 1.6.2 — Argon2id PINs
**Scheduled in:** build hash/verify and its benchmark first; add the privileged reset command after 1.6.4
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-db/src/auth.rs` (new), `crates/pos-db/src/repo/user.rs` (new), `crates/pos-db/benches/pin_verify.rs` (new), `crates/pos-db/Cargo.toml` (`argon2`, `criterion`), `apps/terminal/src-tauri/src/commands/user.rs` (new), `apps/terminal/src-tauri/src/ipc/registry.rs` — the baseline, `benchmarks/baselines/pin-verify.json`, is 5.8.2's
```rust
pub fn hash_pin(pin: &str) -> Result<String, AuthError>;              // PHC string
pub fn verify_pin(pin: &str, hash: &str) -> Result<bool, AuthError>;  // constant-time
```

```text
user_reset_pin { user_id, new_pin, approval_id } -> ()
```

Named Argon2 parameters are a reviewed profile, chosen before the reference register exists and re-tuned on it at 5.8.2 if it puts verification outside the band — before any real PIN exists, and a PHC string carries its own parameters — and failed-attempt state persists across restart. The minimum manager-approval PIN defaults to six digits until the merchant answers the existing security open item; weakening the profile or restarting to clear throttling would turn a copied database into an offline guessing oracle.
Reset requires `user.admin` and a bound approval, retires the old PHC hash, clears no unrelated audit or throttle history, and commits its audit row with the replacement. Otherwise “forgot PIN” becomes either an unrecoverable cashier account or an unaudited credential takeover.
**Tests:** `hash_verify_roundtrip` · `wrong_pin_rejected` · `hash_is_salted_and_differs_per_call` · `argon2_parameters_match_the_reviewed_profile` · `failed_attempt_state_survives_restart` · `manager_reset_retires_old_hash_and_audits` (E.51; the same fixture refuses a short manager PIN, clears a lockout only through the audited manager path, and retires the old hash)
**Bench:** `crates/pos-db/benches/pin_verify.rs` measures the reviewed target under conventions §7; no wall-clock assertion lives in a unit test.
**Done when:** `cargo nextest run -p pos-db auth:: && cargo nextest run -p terminal manager_reset_ && cargo test -p pos-db --bench pin_verify` passes the named reset/audit and Argon2 assertions and runs the benchmark once; the 200–350 ms median / 500 ms p99 band on the reference register is 5.8.2's, and 1.6.8 proves neither PIN nor PHC hash reaches a sink.

### 1.6.3 — Capabilities and the default role matrix
**Scheduled in:** define the matrix before 1.6.1, because the immutable seed lands only in migration `0004` and a committed migration is never reopened. The domain grid is what 1.6.1 writes its seed *from*, so the two microsteps stop pointing at each other, and the seed comparison then reads that seed back — the half that closes gap G-6's fixture claim.
**Files:** `crates/pos-domain/src/permissions.rs` (new) · `crates/pos-domain/src/lib.rs` (module declaration and re-exports) · `crates/pos-db/tests/role_matrix.rs` (new) · [`ref/domain-api.md`](ref/domain-api.md) (§8's declarations and §8.2's framing) · this file (this microstep's prose and `Tests:` line)
Per API reference §8: the `cap` module and the normative default matrix, which 1.6.1 then seeds as `role` + `role_capability` rows. This step does not reopen the migration. The tests iterate `cap::ALL`; a hard-coded capability count or the bundled source-plan table already drifted and cannot prove a newly added capability was deliberately granted or denied.
The grid is declared **with** each capability rather than beside it, so the compiler maintains it: a new capability that decides no grants has no matching macro rule, a hand-written `Capability` impl without `DEFAULT_GRANTS` is `E0046`, and a fifth role is thirty-four compile errors — `E0004` at the two wildcard-free matches over `Role`, `E0063` at every one of the thirty-two rows. `Authorized<C>`, `authorize` and the approval handle stay in 1.6.4 — `authorize` is the only constructor of a token and cannot validate an `ApprovalHandle` that does not exist, so a token written here would be a type with no way to make one, and its two `trybuild` proofs would have nothing to prove.
**Tests:** `default_matrix_covers_every_capability_in_cap_all` · `journal_view_is_scoped_to_the_holders_own_shift_without_reports_all` · `customer_lookup_refuses_a_prefix_query` · `an_owner_holds_no_till_capability` · `a_cell_that_only_sets_a_limit_grants_nothing` · `a_grant_set_contains_only_held_capabilities` · `capability_names_are_unique_and_follow_noun_dot_verb` · `seeded_role_capability_rows_equal_the_domain_default_matrix`
**Done when:** `cargo nextest run -p pos-domain permissions::` and `cargo nextest run -p pos-db --test role_matrix` both pass — the first comparing every `cap::ALL` entry with all four roles and asserting no capability is granted to nobody, because an accidental blanket denial is what a newly added capability looks like when its grants were never decided; the second comparing all 128 seeded cells with `cap::DEFAULT_MATRIX`, and failing when a seeded row is added, removed, given a different `decision`, or given a different limit.

**The seed comparison.** `role`, `role_capability` and the `capability` code list arrive in `0004`, seeded from `cap::DEFAULT_MATRIX`. `role_matrix.rs` reads them back and compares: one `role_capability` row for **every** one of the 128 (role, capability) cells, its `decision` column holding `granted`, `withheld` or `sets_the_limit` to match the domain `Grant` exactly — an absent row is the failure, not a denial — and `capability(code)` holding exactly `cap::ALL`. **It opens the database through the registered migration chain, never through `tests/common/mod.rs`'s `full_schema`**, which turns foreign keys off and layers on [`ref/schema.md`](ref/schema.md) reference blocks the shipped chain does not carry. That boundary is 1.2.1's recorded failure declining to repeat: `authorization_scope.rs` and `fact_table_guards.rs` were green against `user_role` and `approval_handle` for weeks before `0004` existed, and a green run against reference SQL is not evidence that a migration shipped.

### 1.6.4 — `Authorized<C>` and `authorize`
**Files (domain and persistence halves):** `crates/pos-domain/src/permissions.rs` · `crates/pos-domain/src/lib.rs` · `crates/pos-domain/tests/typed_ids_ui.rs` · `crates/pos-domain/tests/ui/` (three compile-fail cases and their goldens) · `crates/pos-db/Cargo.toml` · `crates/pos-db/src/lib.rs` · `crates/pos-db/src/repo/mod.rs` · `crates/pos-db/src/repo/approval.rs` (new) · `crates/pos-db/tests/approval.rs` (new) · `docs/implementation/README.md` · `docs/implementation/phase-1-sellable-mvp.md` · `docs/implementation/ref/domain-api.md` · `docs/implementation/ref/test-catalog.md` · `scripts/check-test-catalog.py`
The proof-carrying token, as a **marker type** — `Authorized<C: Capability>` with a private `PhantomData<fn() -> C>`. Not `Authorized<const C: &'static str>`, which does not compile on any stable rustc: `&'static str` is forbidden as the type of a const generic parameter. Resist "fixing" it with a runtime `&str` field, which throws away the property the design exists for.

`authorize` is the only way to obtain a `&Authorized<C>`, and privileged domain functions require one. Runtime escalation adds a persisted one-use handle:

```rust
pub struct ApprovalHandle {
    id: ApprovalId, capability: String, actor: UserId, approver: UserId,
    entity_id: Uuid, amount_minor: i64,
    content_hash: Option<PreparedIntentHash>, reason: String,
    issued_at: Timestamp, expires_at: Timestamp, nonce: [u8; 16],
}

pub struct StoredApprovalHandle {
    pub id: ApprovalId, pub capability: String,
    pub actor: UserId, pub approver: UserId, pub entity_id: Uuid,
    pub amount_minor: i64, pub content_hash: Option<[u8; 32]>,
    pub reason: String, pub issued_at: Timestamp,
    pub expires_at: Timestamp, pub nonce: [u8; 16],
}

impl ApprovalHandle {
    pub fn issue<C: Capability>(
        id: ApprovalId, actor: UserId, approver: &Authorized<C>,
        binding: &ApprovalBinding, reason: String, now: Timestamp,
        ttl_ms: i64, nonce: [u8; 16],
    ) -> Result<Self, PermissionError>;
    pub fn id(&self) -> ApprovalId;
    pub fn approver(&self) -> UserId;
    pub fn reason(&self) -> &str;
    pub fn matches<C: Capability>(&self, actor: UserId,
        binding: &ApprovalBinding, now: Timestamp) -> Result<(), PermissionError>;
    pub fn to_stored(&self) -> StoredApprovalHandle;
    pub fn restore(stored: StoredApprovalHandle) -> Result<Self, PermissionError>;
}

pub struct ApprovalRepository<'c> { conn: &'c Connection }

impl<'c> ApprovalRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self;
    pub fn insert(&self, tx: &Transaction<'_>, handle: &ApprovalHandle)
        -> Result<(), DbError>;
    pub fn load_for_consumption(&self, tx: &Transaction<'_>, id: ApprovalId)
        -> Result<Option<ApprovalHandle>, DbError>;
    pub fn ensure_unconsumed(&self, tx: &Transaction<'_>, id: ApprovalId)
        -> Result<(), DbError>;
    pub fn consume(&self, tx: &Transaction<'_>, handle_id: ApprovalId,
        effect_id: Uuid, audit_log_id: Uuid, consumed_at: Timestamp)
        -> Result<(), DbError>;
    pub fn is_consumed(&self, id: ApprovalId) -> Result<bool, DbError>;
}
```

Every always-privileged IPC command accepts `approval_id`; a conditionally privileged command accepts `approval_id?` and refuses its privileged branch when the handle is absent. The handler resolves the immutable handle, verifies capability, actor, entity, amount, optional prepared-content hash, reason and the full half-open validity interval, then inserts `approval_consumption` in the same transaction as the effect and audit row. `actor != approver` is unconditional on every handle path; `ban_self_approval` decides whether an operation needs escalation at all and never permits a self-issued handle. Private fields and the absence of `Deserialize` prevent direct construction or deserialization of `ApprovalHandle`; `issue` remains the only minting path, while trusted persistence may reconstruct only through `restore`, which rechecks the structural invariants. As limit 3 records below, that explicit trusted seam is not proof that the approver authenticated. The handle itself remains evidence; deleting it would contradict the append-only audit design. JavaScript receives only `ApprovalRef`, never the nonce, content hash or a bearer proof.

The adapter distinguishes `DbError::ApprovalAlreadyConsumed`, `DbError::ApprovalConsumptionUnbound` and `DbError::InvalidStoredApproval { reason }`. None carries a domain type and there is deliberately no automatic `From<PermissionError>` conversion: the shell must map business policy explicitly. A malformed stored row records only the domain error's scrub-safe display text; no digest, nonce or PIN enters the message.

The caller owns the transaction and the write order: refuse an already-consumed id, load and validate the handle, write the complete commit envelope, then write the effect, all thirteen audit columns — with `reason` copied from the handle — and the consumption before one commit. `consume` repeats the one-use check so a caller that omits the early guard receives `DbError::ApprovalAlreadyConsumed`, not an unrelated manifest uniqueness error. The shell maps that storage result to `PermissionError::ApprovalAlreadyUsed(id)` while retaining the id itself; `ApprovalConsumptionUnbound` remains an integrity failure.

**Tests:** `cashier_cannot_void_a_sale` · `an_actor_cannot_approve_their_own_handle` (E.52) · `an_altered_amount_is_refused` · `a_different_sale_is_refused` · `a_different_actor_is_refused` · `an_expired_handle_is_refused` · `an_approval_before_issuance_is_refused` · `approval_debug_output_redacts_content_hash_and_nonce` · `a_handle_used_twice_is_refused` · `a_consumed_handle_is_still_consumed_after_restart` · `the_effect_and_the_consumption_commit_together_or_not_at_all` · `a_stored_handle_round_trips_through_restore` · `a_malformed_stored_handle_is_refused` · `authorized_tokens_cannot_be_forged_or_substituted` · `approval_handles_cannot_be_deserialized`.
**Domain half done when:** `cargo nextest run -p pos-domain permissions:: && cargo nextest run -p pos-domain --test typed_ids_ui` passes, including the inclusive issuance boundary, compile-time refusal to deserialize `ApprovalHandle`, and compile-time refusal to forge or substitute an `Authorized<C>` token.
**Persistence half done when:** `cargo nextest run -p pos-db --test approval` passes through the registered migration chain, including restart, rehydration and rollback coverage.

**Deferred half — terminal orchestration only.** The IPC command resolves the stored handle and coordinates validation, effect, audit and consumption after 1.8.x wires `pos-db` into the terminal.
**Files (deferred half):** `apps/terminal/src-tauri/src/commands/auth.rs` (new)
**Deferred half done when:** `cargo nextest run -p terminal` passes with stored-handle resolution, explicit permission-error mapping and current-user activity denial exercised across the real IPC orchestration.
> **The terminal file waits on something this microstep does not own.** `apps/terminal/src-tauri/Cargo.toml` does not depend on `pos-db`, and §17 of [`02-development-workflow.md`](02-development-workflow.md) assigns that wiring to 1.8.x persistence. A command that resolves a persisted handle cannot be written against a crate the terminal does not link, so `commands/auth.rs` follows the persistence wiring rather than leading it.

**Persistence boundary and stated limits.** This half proves only the guarantees its present schema and tests can enforce:

1. Migration `0004`'s `approval_consumption_matches_handle_and_audit` trigger does **not** compare `content_hash`. `ApprovalHandle::matches` enforces that binding in Rust, while the per-request database triggers arrive with the prepared-intent tables in migration `0005` and later (the `product_quick_add_request` and stock-request blocks in [`ref/schema.md`](ref/schema.md)). Until those tables and triggers land, no content-hash-bearing capability may ship, and this microstep does not claim to bind prepared content in SQL.
2. `approval_consumption.effect_id` has no foreign key and no trigger requiring a row with that id. `the_effect_and_the_consumption_commit_together_or_not_at_all` proves the rollback direction and the shared-transaction plumbing; it does not prove that a consumption cannot exist without a financial effect. The end-to-end guarantee belongs to 1.8.3's orchestration and fault-injection work.
3. `ApprovalHandle::restore` proves structural validity, not that the approver authenticated. The trusted issuance transaction and its immutable evidence own that claim. Rust has no friend-crate visibility, so `pos-db` is a trusted boundary by convention; making reconstruction explicit exposes that trust rather than creating it.
4. Whether deactivating an **approver** retroactively revokes an already-issued, unexpired handle remains unresolved. No code or prose in this step selects either behavior.
5. `app_user` carries both `is_active` and `deleted_at`, and no trigger enforces either. Every future current-user activity check must read both; checking only `is_active` would let a soft-deleted user spend an approval. This is command-handler policy, not an `ApprovalRepository` business rule.

> ⚠️ **OPEN — blocks the 1.8.x terminal approval handler.** Does deactivating an approver retroactively revoke an already-issued, unexpired handle? Default until answered: no terminal behavior depending on either answer is declared complete or described as guaranteed. Owner: the 1.8.x command-handler integration. Source that settles it: the reviewed authorization policy and its recorded threat-model decision.

**Full-step status:** 1.6.4 remains incomplete only until the 1.8.x terminal handler resolves a stored handle and orchestrates its explicit permission mapping and same-transaction use. The domain proof and persistence halves, including restart and rollback evidence, are complete.
**Done when:** `cargo nextest run -p pos-domain permissions:: && cargo nextest run -p pos-domain --test typed_ids_ui && cargo nextest run -p pos-db --test approval && cargo nextest run -p terminal` passes the domain, repository and eventual terminal-handler suites, including `deactivated_user_denied` at the handler boundary.

### 1.6.5 — Audit hash chain
**Files:** `Cargo.toml`, `Cargo.lock`, `crates/pos-domain/Cargo.toml`, `crates/pos-domain/src/audit.rs` (new)
`canonical_bytes(&CanonicalAuditEntry)` includes `register_id`, `seq` and `id`; `chain_hash` is BLAKE3 of `prev ‖ bytes`; `verify_chain` accepts the last `ChainAnchor`. The chain alone cannot detect deletion of its newest tail, so every verified backup exports the head and the verifier compares against that external anchor.
**Tests:** `golden_canonical_bytes_are_stable` · `prop_chain_detects_any_single_entry_mutation` · `prop_chain_detects_deletion_before_the_anchor` · `prop_chain_detects_tail_deletion_against_an_anchor` · `prop_chain_detects_reordering` · `mutating_an_identity_column_breaks_the_chain`
**Done when:** mutating any byte of any historical entry makes `verify_chain` return `Broken { at_seq }` pointing at it.

### 1.6.6 — `AuditRepository`
**Files:** `crates/pos-db/src/repo/audit.rs` (new), `crates/pos-db/tests/audit.rs` (new), `crates/pos-db/src/repo/mod.rs`, `crates/pos-db/src/lib.rs` (four `DbError` variants), `crates/pos-db/Cargo.toml` (`serde_json`), `Cargo.lock`, [`README.md`](README.md) (implementation frontier), [`ref/security-compliance.md`](ref/security-compliance.md) (§4's stale owner for `verify-audit`), this file
Append-only. Reads the previous hash and writes the new row **inside the caller's transaction**. There is no update method and no delete method — not private ones, none.
> **Why the `Files:` line is eight entries and not one.** `AuditIntent.payload` is a
> `serde_json::Value` and `audit_log.payload` is canonical JSON by DDL, so the read path cannot
> exist without `serde_json` in `pos-db` — a dependency edge `Cargo.lock` records or `--locked`
> refuses the build. The rest is the five-part shape `1.9.2` records: the module, its suite, the
> `pub mod` line, the error variants and the frontier.
>
> **`seq` is allocated here, explicitly, before the hash.** `audit_log.seq` is `INTEGER PRIMARY KEY AUTOINCREMENT` but it is *inside* the hashed bytes, and `audit_log_no_update` refuses every `UPDATE` — so the number cannot be learned after the insert, and a wrong one is permanently unfixable. The append reads `IFNULL(MAX(seq),0)` and the register's head `hash` in one statement through the caller's `&Transaction`. Not `sqlite_sequence`: it carries no trigger guard at all, and a rewritten counter diverges from the chain.
>
> **An unrecognised `action` or `entity` is never a read error.** Both are `&'static str` on the domain type and SQLite returns `String`, so the read path interns — one leak per distinct spelling, never one per row. Refusing an unknown spelling instead would let one inserted row disable the whole verifier, which is the opposite of what gap G-7 asks for.
>
> **And no single row can silence a register.** `chain()` returns the rows it could rebuild *plus* an optional located stop, never an error that discards the prefix. The first draft of this microstep did the latter for six row-level conditions, which was the same hole one paragraph up: `audit_log_no_update` and `audit_log_no_delete` guard mutation but **nothing guards `INSERT`**, so adding one unreadable row — with an envelope the same SQL console can write — made every honest row below it unreachable, permanently and irreparably. A stop is also not a tamper verdict: the commonest cause, once this product has two versions in the field, is a row written under a canonical layout the reading binary does not know.
**Tests:** `chain_survives_process_restart` · `concurrent_appends_serialize` · `verify_chain_over_1000_entries` · `a_negative_seq_stops_the_walk_at_that_row` · `a_null_entity_id_stops_the_walk_at_that_row` · `a_payload_that_is_not_canonically_encodable_is_refused` · `a_rewritten_sqlite_sequence_does_not_move_the_chain` · `a_rolled_back_append_leaves_no_row` · `a_row_this_build_cannot_rebuild_stops_the_walk_without_hiding_the_rows_below_it` · `a_second_registers_rows_do_not_break_either_chain` · `an_append_reads_the_head_through_its_own_transaction` · `an_append_without_its_delivery_envelope_is_refused` · `an_unknown_action_is_read_back_rather_than_refused` · `an_unreadable_head_refuses_the_append_rather_than_chaining_onto_it` · `audit_log_still_refuses_update_and_delete` · `the_chain_read_rebuilds_every_hashed_column` · `the_stored_hash_matches_a_pinned_golden` · `the_stored_payload_is_canonical_json`
**Done when:** `cargo nextest run -p pos-db --test audit` exits zero — the chain read back out of SQLite after a reopen verifies as `Intact { entries: 1000 }`, a row appended by a transaction that provably contended for the write lock carries the other transaction's `hash` as its `prev_hash`, no two rows of one register share a `prev_hash`, and one row's stored `hash` equals `BLAKE3(GENESIS ‖ the canonical bytes written out by hand in the test)` rather than whatever the encoder produced.

### 1.6.6b — Local audit verifier
**Files:** `crates/pos-db/src/bin/verify-audit.rs` (new), `crates/pos-db/tests/audit_verifier.rs` (new), `crates/pos-db/Cargo.toml` (`[[bin]]`), `crates/pos-db/src/repo/audit.rs` (`AuditRepository::registers`, `AuditRegisters`), `crates/pos-db/tests/audit.rs` (that reader's own tests), [`../../scripts/check-test-catalog.py`](../../scripts/check-test-catalog.py) (one name, `PLANNED` → `PLANNED_RETIRED`), [`README.md`](README.md) (implementation frontier), this file

```text
verify-audit --database <copy> [--anchor <backup-manifest>]
```

The command opens an explicitly named database through the normal key provider, walks `CanonicalAuditEntry` rows and reports `Intact` or `Broken { at_seq }`. It exists in Phase 1 because the exit gate must prove tamper detection before a merchant's register is the only copy; waiting until Phase 5 would leave the Phase-1 claim untestable.
> **Why the `Files:` line is eight entries and not two.** `Cargo.lock` is deliberately absent: the binary takes no new dependency, because two flags and `--help` are parsed by hand rather than by an argument-parser crate that would enter the graph `pos-db` puts on a register. The rest is what the usage line above forces. It names a database and not a register, so the tool must discover which tills the file holds rows for, and `AuditRepository::chain` takes one `RegisterId` — conventions §3 puts that query in the storage seam rather than in a binary, and [`ref/ipc-contract.md`](ref/ipc-contract.md) §3's `diag_verify_audit_chain` will need the same list. `tests/audit.rs` proves that reader where the rest of the module's API is proved; `check-test-catalog.py` must retire the one catalogue name this step lands.
>
> **The anchor file is shaped like `audit_checkpoint`, not like `ChainAnchor`.** `5.4.4` extends `--anchor` to the server's last accepted checkpoint, and that table already exists (`0004:478`) with the columns `register_id`, `last_seq`, `last_hash`, `source_kind` and `anchored_at` — so those are the JSON keys, and `5.4.4` is an extension rather than a rename. `ChainAnchor`'s own derived `Deserialize` writes its digest as an array of thirty-two integers, which is not a format anybody writes by hand or a backup tool emits.
>
> **An anchor recording `last_seq: 0` anchors nothing and is not a tamper.** `audit_checkpoint.last_seq` is `CHECK (last_seq >= 0)`, so a Z close on a register that has audited nothing can honestly record zero; handed to `verify_chain` it returns `Broken { at_seq: 0 }`, because no row is ever numbered zero. That is tampering reported where none happened, which `repo/audit.rs` names as the one answer a forensic tool must never give.
>
> **The set of chains to walk is what the file holds *union* what the anchor names**, and the union is load-bearing. Deleting *every* row of a register removes it from the enumeration, so a verifier reading only `audit_log` would walk nothing, print nothing and exit zero while holding an anchor that says there were four rows.
>
> **An anchor above a stopped read is withheld rather than applied**, and this is the one place where a correct anchor and a correct stop produce a wrong answer between them. A read that stops at seq 3 hands `verify_chain` the rows below it; an anchor at seq 5 then makes it answer `Truncated { anchored_seq: 5, found_seq: 2 }` — *the rows between were removed*. They were not. Rows 3 to 5 are all still on the disk, and the tool would be accusing a merchant of deleting them because it could not read one of them itself. An anchor **below** the stop is untouched, because the row it names was read.
>
> **Nothing raw from the anchor file reaches the report.** The anchor is supplied by whoever is being investigated and the report is a document somebody reads as evidence, so every field is parsed and printed back from the parsed value — the register as a `Uuid`, the digest through the binary's own hex encoder, `source_kind` as the matched `&'static str` out of `audit_checkpoint`'s own three, and `anchored_at` through `Timestamp`. A field echoed byte for byte hands its author a terminal: one newline inside `source_kind` is enough to forge a register block, heading and `INTACT` verdict included, into the middle of the report. CodeQL's `rust/cleartext-logging` is what pointed at the line beside it.
>
> **It refuses a database that is not already a file.** `Connection::open` creates a missing one and `pos_db::open` then migrates the empty result, so a mistyped path would otherwise be answered with "no audit rows" and exit 0 — indistinguishable from a clean register, and the worst wrong answer this tool can give.
**Tests:** `verifier_reports_the_first_broken_sequence` · `tail_deletion_is_detected_against_the_last_anchor` · `the_original_database_still_refuses_an_audit_update` · `a_malformed_anchor_is_a_tool_failure_not_an_unanchored_pass` · `a_missing_database_is_a_tool_failure_not_a_verdict` · `a_row_this_build_cannot_rebuild_is_inconclusive_rather_than_intact` · `a_row_whose_register_id_is_no_id_is_counted_rather_than_hidden` · `an_anchor_above_a_stop_is_withheld_rather_than_read_as_a_truncation` · `an_anchor_below_a_stop_is_still_applied` · `an_anchor_for_another_register_anchors_nothing` · `an_anchor_recording_an_empty_chain_reports_no_tamper` · `an_anchored_register_whose_rows_are_all_gone_is_still_reported` · `an_intact_chain_exits_zero_and_reports_the_tail_as_unanchored` · `every_register_in_the_database_is_verified` · `help_exits_zero_and_names_every_exit_code` · `registers_lists_every_till_that_has_written_a_row` · `rows_that_belong_to_no_register_are_counted_rather_than_hidden` · `tamper_evidence_outranks_a_row_that_cannot_be_rebuilt` · `the_report_never_prints_the_database_key` · plus the command line and anchor parsers in the binary's own `mod tests`: `an_unknown_argument_is_refused` · `a_flag_without_its_value_is_refused` · `a_repeated_flag_is_refused` · `help_beats_every_other_argument` · `a_digest_must_be_sixty_four_hex_characters` · `a_signed_hex_pair_is_not_a_digest` · `an_anchor_needs_all_three_required_keys`
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
**Files:** `crates/pos-domain/src/receipt.rs` (new) · `crates/pos-domain/src/lib.rs` (`pub mod receipt;` and the re-export block) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines, and 1.7.3's validation obligation)
Per API reference §13. The ESC/POS rasteriser, the PDF renderer, and the email renderer all consume this — so an emailed receipt can never disagree with the printed one.
**`ReceiptLocale` carries language and direction and NOT `money_decimals`**, although §13's inline comment lists all three. Two paragraphs below that comment the same section says `money_decimals` *"does not govern a document the customer is handed"*, and [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §2.3 makes it the first of four rules the renderer may not negotiate — every money field through `Money::format_exact`, *"with the store's `money_decimals` nowhere in the path"*. The paragraph wins over the comment because it states its reason, and the field's absence is what enforces it: a comment cannot be violated, a field can. Same move as [`ref/ui-spec.md`](ref/ui-spec.md) §1's `MoneyDisplay`.
**`build_receipt_model` is not this microstep's and is owned by nobody.** [`ref/domain-api.md`](ref/domain-api.md) §13 specifies it, it needs `CompletedSale` from 1.4.1, and no `Files:` or `Tests:` line in any phase file claims it — the same class of plan defect as the `apps/terminal/src/lib/ipc.ts` the 21 September audit found. §13 splits across microsteps exactly as §7 does, where 1.5.1 owns `TenderType` and `Tender` while `Tendering` and `remaining_due` are 1.5.2's, so this is a gap to assign rather than a half delivered here. **Whoever builds 1.4.1 or 1.8.3 assigns it**, and it is a pure function over a `CompletedSale`, so `pos-domain` is where it belongs. **1.4.1 assigned it to 1.8.3**, the first step that has a completed sale to print.
**Five rules are properties of the whole document rather than of any field**, so `validate()` carries them: an `Acknowledgement` drops the QR line (`hardware-and-receipts.md` §2.3), the cash-rounding line prints *"only when non-zero"* so `Some(zero)` is a different and illegal document, a `BuyerBlock` exists because a TIN was captured, **a line with no name describes nothing it was charged for** — §2.3's *"never 'unknown item'"* rule in the form a model can break it, since the builder chooses which name and this type can refuse none at all — and every `Money` on one document shares one `Currency`, naming the field that disagrees rather than only the fact that one does. The struct stays public-fielded as §13 specifies rather than hiding behind a fallible constructor; **1.7.3's entry point calls `validate` before laying anything out**, which is where the call becomes mandatory.
**Tests:** `a_non_fiscal_acknowledgement_cannot_carry_a_fiscal_block` · `a_buyer_block_without_a_tin_is_refused` · `a_line_that_describes_nothing_is_refused` · `a_zero_rounding_adjustment_is_absent_rather_than_zero` · `a_zero_amount_in_another_currency_is_still_another_currency` · `buyer_name_never_reaches_debug_output` · `merchant_phone_never_reaches_debug_output` · `a_masked_pan_is_deliberately_still_visible_in_debug` · `a_receipt_round_trips_through_canonical_json` · `every_doc_kind_is_covered_by_the_validator` · `dock_kind_all_lists_every_variant` · `prop_a_model_mixing_two_currencies_is_refused` · `prop_validate_accepts_every_consistent_model`
**Done when:** `cargo nextest run -p pos-domain receipt::` exits zero, proving that `ReceiptModel::validate` refuses a document that mixes two currencies, one that calls itself a non-fiscal acknowledgement while carrying a fiscal block, one that records a zero cash-rounding line and one whose line describes nothing — and that no `Debug` rendering of a model reveals a buyer name or a merchant phone.

### 1.7.2 — Font decision and embedding
*Gap G-5.*
**Scheduled in:** embed, license and prove the raster font at 1.7.2; wire the UI to the same asset in 1.11.1
**Files:** `assets/fonts/` (new — two TTF faces, `LICENSE.txt`, and a `README.md` recording provenance and the OFL clause that binds a later subsetting step) · `crates/pos-hardware/src/font.rs` (new) · `crates/pos-hardware/src/lib.rs` (`pub mod font;`) · `crates/pos-hardware/tests/font_asset.rs` (new) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line). **`crates/pos-hardware/Cargo.toml` needed no change** and this line used to name it: `include_bytes!` requires no dependency, and the workspace is `publish = false`, so no `include` key is needed to package the asset
One family covering Arabic and Latin, embeddable, licence-clear, shipped with the app — **no network font**. The same file feeds the UI and the receipt rasteriser so the receipt looks like the screen. Candidates: Noto Sans Arabic, IBM Plex Sans Arabic, Cairo.
**Tests:** `embedded_font_has_a_repository_licence` · `embedded_font_bytes_are_not_empty`
**Done when:** `cargo nextest run -p pos-hardware --test font_asset` exits zero with the font and its licence in the repository; 1.7.3 owns raster loading and 1.11.1 owns the later UI-path equality check.

### 1.7.3 — The raster pipeline
**Files:** `Cargo.toml` and `Cargo.lock` (three new dependencies), `crates/pos-hardware/Cargo.toml` (those three, plus `pos-domain` and `rust_decimal`), `crates/pos-hardware/src/render/mod.rs` (new), `layout.rs` (new), `raster.rs` (new) · `crates/pos-hardware/src/lib.rs` (`pub mod render;`) · `scripts/check-test-catalog.py` (one name: `PLANNED` → `PLANNED_RETIRED`) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line, and 1.7.1's rule count)
**The stack is `harfrust` + `skrifa` + `unicode-bidi` + `tiny-skia`, not `cosmic-text`, and the swap is recorded rather than silent — for two independent reasons.** First, `cosmic-text` brings `fontdb`, which enumerates the machine's fonts, and `sys-locale`, which reads its language — 55 crates against 20, measured. A register draws with the face 1.7.2 embedded and speaks the language on the `ReceiptModel`; a stack that can answer either question from the host is the wrong one for this device. Second, the obvious lighter substitutes are **retired**: RUSTSEC-2026-0206 declares `rustybuzz` unmaintained and RUSTSEC-2026-0192 says the same of `ttf-parser`, each naming its fontations successor, and `cosmic-text` 0.19 depends on `rustybuzz` — so the advisory reaches the plan's own choice too and `just audit` fails either way. `harfrust` is HarfBuzz's own Rust port and `skrifa` is Google's font reader; they share `read-fonts`, so the pair costs two crates over one. Measured before the swap: **identical glyph ids, advances and mark offsets** for the pinned Arabic word. `unicode-bidi` is the *"+ bidi"* half of the diagram above.
**Every measurement is integer.** `float_arithmetic` is `forbid` workspace-wide, so the layout works in font units and `advance_px = advance_units * px_per_em / units_per_em` is `i32` end to end. Exactly one float survives — the `tiny-skia` transform that scales an outline — and it comes from a `Decimal` division rather than an `f32` one. That is not a concession to a lint: 1.7.5 needs `golden_receipts_are_byte_stable`, and an accumulated `f32` pen position is how a golden starts differing between a developer's machine and CI.
**This microstep stops at a bitmap.** The diagram's last arrow is 1.7.4's, whose `Files:` line owns `escpos.rs`. The output is 1 bit per pixel, MSB first, row-major — already the layout `GS v 0` takes, so the emitter is a header and a copy rather than a re-encode.
```
ReceiptModel → layout engine (boxes, RTL runs, columns)
             → harfrust shaping (HarfBuzz's own port: Arabic joining) + unicode-bidi
             → tiny-skia 1-bit bitmap at printer width (576 px @ 80 mm, 384 @ 58 mm)
             → GS v 0 raster bytes
```
Do not fight printer codepages. Windows-1256 text mode does not shape Arabic letters or reorder RTL runs; the field consensus is rasterisation and it is also the only way bilingual mixing looks correct.
**The entry point calls `ReceiptModel::validate` before laying anything out**, and refuses rather than rendering. 1.7.1 put **five** whole-document rules there — an acknowledgement carrying a fiscal QR, a zero cash-rounding line, a buyer block with no TIN, a line that describes nothing, and two currencies on one document — and deliberately left `ReceiptModel`'s fields public, as §13 specifies, so nothing forces the call except this line and the test that proves it. `an_invalid_model_is_refused_before_anything_is_laid_out` is that test.
**Tests:** `rasterizer_loads_the_embedded_font` · `arabic_joining_uses_contextual_glyphs` · `rtl_run_orders_visual_positions` · `layout_wraps_long_arabic_names` · `layout_columns_align_in_rtl` · `raster_width_matches_profile` · `narrow_profile_reflows_rather_than_truncates` (E.49) · `the_arabic_glyph_sequence_is_pinned` · `a_combining_mark_keeps_its_own_offset` · `a_line_prints_its_quantity_and_its_total` · `the_totals_block_is_labelled_in_the_documents_language` · `a_receipt_prints_its_date_and_its_business_date` · `a_long_label_never_collides_with_its_amount` · `an_overlong_word_is_broken_rather_than_lost` · `glyphs_sit_above_their_baseline` · `a_rule_runs_between_the_margins_and_reads_back_msb_first` · `an_invalid_model_is_refused_before_anything_is_laid_out` · `prop_no_glyph_is_placed_outside_the_page`
**Done when:** `cargo nextest run -p pos-hardware render::tests::` exits zero for the embedded font, contextual Arabic glyphs, RTL run order and both raster widths; 1.7.5 owns the dated native-reader review of the viewable golden.

### 1.7.4 — ESC/POS emitter
**Files:** `crates/pos-hardware/src/escpos.rs` (new) · `crates/pos-hardware/src/lib.rs` (`pub mod escpos;`) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
`ESC @` init, `GS v 0` raster and `GS V` cut. The `ESC p` drawer pulse is a separate audited hardware effect, never part of printable bytes and never replayed when a receipt job retries. Two width profiles, 80 mm and 58 mm (E.49).
**The pulse is absent by construction, and a test that greps the stream for it would be wrong.** The raster payload is an arbitrary bitmap, so `1B 70` occurs inside it by coincidence about once every 65 536 byte pairs — which a 576-dot receipt reaches within roughly 900 rows. The assertion is structural: a stream is exactly `INIT ++ header ++ payload ++ CUT`, and the pulse is absent from everything that is not the payload.
**One `GS v 0`, not a banded sequence.** A single command is spec-correct — `yL yH` is a 16-bit line count — and the right band size for a printer whose buffer is smaller is a **device** fact. [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6a is why that matters: *"'ESC/POS' is a family, not a standard"*, a profile record carries `raster_command` for exactly this reason, and the matrix **ships empty except the simulator**. Inventing a band size against no device would be the defect §6a.1 refuses for the reference register, so banding belongs to a qualified profile behind #68.
**`encode` takes a `Bitmap` and not a profile**, because a bitmap already carries its width, height and row stride; a second argument naming the paper would be a second source of truth for the same numbers and a way to hand the emitter a mismatched pair. The profile decided the width at 1.7.3, where the layout happened.
**⚠️ The cut carries no feed, and nothing checks where it lands.** On many thermal printers the cutter sits ten to fifteen millimetres above the print head, so a bare `GS V` severs the paper at a point the document has not reached and the last lines stay inside the machine. The usual answer feeds first — `ESC d n`, or `GS V 66 n` in one command — and **the distance is a device property**, which is why §6a makes `cut_command` a profile field beside `raster_command`. Choosing one with no printer to measure is the same defect as choosing a band size, so this microstep emits the baseline. What is missing is not the feed but the **check**: [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §9's lab checklist covers truncation *across* the paper (check 2) and nothing about truncation *along* it. Whoever qualifies the first printer adds both, behind #68.
**A page with no lines is refused rather than emitted.** The stream would be an initialise and a cut with nothing between them — paper and a cutter cycle spent to hand a customer a blank tab. `render_receipt` cannot produce one, so reaching it is a caller's bug and a loud one is cheaper.
**Tests:** `escpos_init_cut_bytes_are_exact` · `receipt_retry_never_repeats_the_drawer_pulse` · `the_raster_header_declares_the_bitmaps_own_dimensions` · `the_raster_payload_is_the_bitmaps_bytes_unchanged` · `a_stream_is_exactly_init_raster_and_cut` · `a_page_whose_rows_do_not_fill_its_buffer_is_refused` · `a_page_with_no_lines_is_not_a_document` · `a_page_too_tall_for_a_sixteen_bit_header_is_refused` · `a_page_too_wide_for_a_sixteen_bit_header_is_refused` · `both_profiles_emit_their_own_row_width` · `a_bitmap_containing_the_pulse_bytes_is_still_a_document` · `a_refused_document_still_leaves_the_drawer_shut` · `prop_emitting_is_deterministic` · `prop_the_stream_is_the_page_plus_a_fixed_frame`
**Done when:** `cargo nextest run -p pos-hardware escpos::` exits zero, proving an emitted stream is exactly `ESC @` then one `GS v 0` whose header carries the bitmap's own row-byte and line counts then the bitmap's bytes unchanged then `GS V`, that it carries **no `ESC p` outside the raster payload** at any offset including across a re-emission, and that a page whose dimensions or buffer length do not agree is refused rather than truncated.

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
**Files:** `apps/terminal/src-tauri/src/print_queue.rs` (new), `apps/terminal/src-tauri/src/lib.rs` (`pub mod print_queue;`), `apps/terminal/src-tauri/Cargo.toml` (`pos-db`, `pos-hardware`, `rusqlite`, `blake3` and `uuid`, all already in the workspace, and `pos-db`'s `fixtures` for the tests), `Cargo.lock` (edges only, no new package), `crates/pos-db/src/repo/receipt.rs` (new), `crates/pos-db/src/repo/mod.rs`, `crates/pos-db/src/lib.rs` (`DbError::PrintLeaseInvalid` and `StoredPrintRecordInvalid`, and the `fixtures` module behind its feature), `crates/pos-db/Cargo.toml` (the `fixtures` feature), `crates/pos-db/src/fixtures/` (the registered chain, moved from `tests/common/registered_chain.rs` so the terminal's tests can complete a sale too, with its receipt write split from the line taxes), the eleven `pos-db` suites that import it, `crates/pos-db/tests/receipt.rs` (new), `crates/pos-db/tests/fixtures_feature.rs` (new) · `scripts/check-test-catalog.py` (two names: `PLANNED` → `PLANNED_RETIRED`) · [`ref/sync-protocol.md`](ref/sync-protocol.md) (the payload an `audit_log` row travels as) · [`ref/ipc-contract.md`](ref/ipc-contract.md) (where `print_resolve_unknown`'s logic lives) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line and its signatures)

```rust
pub fn append_artifact(&self, tx: &Transaction,
                       artifact: &ReceiptArtifact) -> Result<[u8; 32], DbError>;
pub fn queue_initial(&self, tx: &Transaction, job_id: &Id, artifact_id: &Id,
                     created_at: Timestamp) -> Result<(), DbError>;
pub fn lease_next_job(&self, tx: &Transaction,
                      lease: Lease<'_>) -> Result<Option<PrintJob>, DbError>;
pub fn append_attempt(&self, tx: &Transaction,
                      attempt: &PrintAttempt) -> Result<(), DbError>;

// apps/terminal/src-tauri/src/print_queue.rs
pub fn queue_original(tx: &Transaction, artifact: &ReceiptArtifact, job_id: &Id,
                      created_at: Timestamp) -> Result<[u8; 32], PrintQueueError>;
impl PrintWorker<'_> {
    pub fn recover(&self, conn: &Connection) -> Result<Vec<Id>, PrintQueueError>;
    pub fn run_once(&self, conn: &Connection) -> Result<Option<Attempted>, PrintQueueError>;
    pub fn drain(&self, conn: &Connection) -> Result<Drained, PrintQueueError>;
}
impl Resolver<'_> {
    pub fn resolve_unknown(&self, conn: &Connection,
                           authorized: &Authorized<cap::SaleReprint>, job_id: &Id,
                           answer: Answer<'_>) -> Result<Resolution, PrintQueueError>;
}
```

**Ids and instants are arguments** (conventions §5): the first sketch of these signatures had `queue_initial` return an id it had no way to mint and `lease_next_job` take only a worker id, with no clock to tell which failed job is due or when a lease lapses. `append_artifact` returns the BLAKE3 digest it stores, taken from the bytes themselves, so a caller can name it in the commit envelope; due jobs are leased in the order they were queued, by `rowid`, never by a device clock (I-7); and an expired lease is read back by `expired_leases` for the worker to resolve as `unknown`, never re-queued, as `0005` documents.

**The worker holds no transaction while the device is talking.** It commits the lease, sends the bytes, then appends the outcome in a second transaction. It refuses to send what it cannot vouch for. An artifact whose bytes no longer match their digest, or one rendered for another printer than the register's (`ref/hardware-and-receipts.md` §3 rule 7), is recorded as a failure with no retry, never sent. It never calls `open_drawer`, so no retry and no restart can pulse a drawer. A person's answer is `Resolver::resolve_unknown`, the logic behind `print_resolve_unknown` (`ref/ipc-contract.md` §3). It takes `Authorized<cap::SaleReprint>` and refuses a proof that carries an approval, because the command is not escalatable. **A duplicate is the original plus its watermark, and nothing else.** The caller hands in the original's `ReceiptModel`. The model must re-render the original byte for byte without the watermark before the duplicate is rendered from it with one, so a model for another sale, template or printer is refused rather than printed as a copy. The answer, its audit fact, the duplicate and its queued job commit in one transaction under one envelope (I-9). No Tauri command calls it yet.

A normal finalize always appends one initial print job before the post-commit hardware attempt; printer health changes only the warning/alarm path. A print failure after finalize never mutates the completed sale. Migration `0005` stores immutable `receipt_artifact.content_bytes`, a leased mutable `print_job`, and append-only `print_attempt` outcomes. An `unknown` or partial hardware outcome never retries automatically, because replaying ambiguous bytes may print a second customer document or pulse a drawer twice. A requested duplicate is a new artifact linked to the original and visibly watermarked.
**Tests:** `every_finalized_sale_has_one_initial_print_job` · `print_failure_after_finalize_leaves_sale_complete` · `duplicate_artifact_links_the_original_and_adds_the_duplicate_watermark` · `an_unknown_print_outcome_never_auto_retries_the_drawer_pulse` · `queue_survives_restart` · `successful_print_appends_an_attempt_without_updating_sale`
**Done when:** `cargo nextest run -p terminal print_queue::` passes after a restart with the original bytes, job state and attempt history unchanged.

### 1.7.8 — Simulator fault injection
**Files:** `crates/pos-hardware/src/lib.rs` (`PrintOutcome`, `FailReason`, `Fault`, and `ReceiptPrinter::print` returning an outcome) · `crates/pos-hardware/src/escpos.rs` (its tests read the outcome) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Tests:` lines, and its `Full-step status:` marker)
Extend `SimulatedPrinter` with scripted faults: paper-out at byte N, cover-open, offline, slow. CI and demos run hardware-free (master plan C.15).
**`print` returns one of [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §3's four outcomes, never a bare error** — `Printed`, `Failed` with its reason, `Partial` with the bytes acknowledged, or `Unknown` — because the outcome is what the queue's retry rule reads. Cover-open, offline and paper-out before a document are states the simulator is in, and refuse it before its first byte; paper-out at byte *N*, a device gone silent after *N* bytes, and a device too slow to answer are scripted faults the next document meets. Paper running out mid-document leaves the printer out of paper, and a slow device's paper exists while the register reads `Unknown` — the case that makes an automatic retry of `Unknown` print a second original.
**Tests:** `simulator_fails_midway_when_scripted` — sweep every byte boundary of a golden and require one typed `PrintOutcome`, no drawer-count change and no second original artifact · `a_printer_that_is_not_ready_fails_before_a_byte` · `a_slow_printer_is_unknown_though_its_paper_exists` · `paper_out_midway_leaves_the_printer_out_of_paper`
**Full-step status:** 1.7.8 is not complete until its sweep runs over a golden and asserts that no second original artifact is created. Today it injects paper-out and silence at every byte boundary of the 80 mm receipt the goldens will be rendered from, and asserts one outcome per boundary, an unmoved drawer, and at most one document's paper — a whole one only when the outcome is `Printed`. The golden arrives with 1.7.5; the artifact clause needs 1.7.7's queue, because the simulator holds paper, not artifacts.

### 1.7.8b — Fuzz receipt layout input
**Files:** `crates/pos-hardware/fuzz/Cargo.toml` (new), `crates/pos-hardware/fuzz/fuzz_targets/receipt_layout.rs` (new), `crates/pos-hardware/fuzz/corpus/receipt_layout/` (new), `.github/workflows/security.yml`, `scripts/tests/fuzz_ci_contract_test.py` (new)

```rust
fuzz_target!(|input: ReceiptFuzzInput| { let _ = render_receipt(input.model); });
```

Feed arbitrary UTF-8, bidi controls, combining marks, long product names and extreme legal line counts through the layout and raster entry point. Catalogue text crosses no trusted language boundary before printing, so one malformed name must not panic or allocate without bound. A weekly security job runs this target and 1.2.8's `parse_scan` target for exactly 15 minutes each with their committed corpora; fuzzing stays out of the per-PR gate because a time-based search is not a deterministic merge check.
**Tests:** `receipt_layout_seed_corpus_covers_arabic_bidi_combining_and_long_lines` · `weekly_fuzz_job_runs_both_phase_1_targets_with_fixed_budgets`
**Done when:** from `crates/pos-hardware`, `cargo fuzz run receipt_layout -- -runs=100000` exits zero, `python3 scripts/tests/fuzz_ci_contract_test.py` proves the weekly job names both targets and fixed budgets, and every minimized crash input is committed to its corpus.

### 1.7.9 — Raster strips, and the profile that sizes them
**Files:** `crates/pos-hardware/src/profile.rs` (new) · `crates/pos-hardware/src/escpos.rs` · `crates/pos-hardware/src/lib.rs` (`pub mod profile;`) · `crates/pos-hardware/src/render/mod.rs`, `layout.rs`, `raster.rs` (the renderer's paper type becomes `PaperWidth`, so `PrinterProfile` names §6a's record alone) · [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6a · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
**1.7.4 emitted one `GS v 0` because no device had been held, and the device now has a number.** Epson's ESC/POS reference limits a single `GS v 0` to **1–2303 dot rows** on the TM-T20III, TM-T20IV and TM-m30III, and to 1–4095 on the TM-T88VII. 2303 rows is about 29 cm of receipt, which a long basket reaches — so a single command is a stream a TM-T20III will not take whole. The same page marks `GS v 0` *"[obsolete command]"* that *"will not be supported by future models"*, and recommends the graphics function `GS ( L` (function 112 to store, function 50 to print). Both facts are per device, which is exactly where 1.7.4 said banding belonged.
**So the profile becomes a type, and the emitter reads two of its fields.** `PrinterProfile` is [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6a's record as a struct, every field present. `raster_command` chooses `GS v 0` or `GS ( L`; `raster_max_rows` is the most dot rows one command may carry. A page taller than the limit becomes consecutive commands, each a whole number of rows and none taller than the limit, whose payloads joined are the bitmap's bytes unchanged — the seam cannot show on paper. `GS ( L` adds a limit of its own: it counts its parameters in 16 bits, so one function-112 store carries at most 65 525 data bytes — 910 rows at 576 dots, 1365 at 384 — and a `GS ( L` strip is the smaller of that and `raster_max_rows`, unless the extended `GS 8 L` is chosen ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b.8, where an independent emulator printed this receipt in 900-row `GS ( L` strips dot-identical). `ESC @` stays first, the cut stays once and last, and `ESC p` stays absent outside the payloads: 1.7.4's structural assertion, extended from one raster command to a sequence of strips.
**`encode` now takes the profile, and 1.7.4's reason for refusing it is kept rather than dropped.** 1.7.4 kept the profile out so the width could not have two sources. The width still comes from the bitmap; the profile adds the one property a bitmap cannot carry, the device's per-command limit. A bitmap whose width disagrees with its profile's `dot_width` is refused, so the second source is checked rather than trusted.
**A profile whose row limit is unknown cannot be bound to a register**, for the reason a profile with no qualifying commit cannot: §6a.1 refuses a referent nobody measured. The simulator's profile states its own limit, the matrix still ships empty except the simulator, and milestone 5.8's first qualified rows are the first real ones. The persisted artifact holds one profile's bytes — `receipt_artifact.printer_profile` records which — so §3 rule 7's refusal to reprint on a register bound to a different profile now protects strip boundaries as well as width.
**Tests:** `a_page_one_row_taller_than_the_limit_becomes_two_commands` · `joined_strip_payloads_are_the_bitmap_unchanged` · `the_cut_appears_once_after_every_strip` · `gs_paren_l_frames_the_same_rows_within_its_own_limit` · `a_width_disagreeing_with_the_profile_is_refused` · `an_unknown_row_limit_refuses_the_document` · `a_profile_with_an_unknown_row_limit_cannot_be_bound` · `prop_strips_join_to_the_bitmap_and_respect_the_limit` — edge case E.93. 1.7.4's `prop_the_stream_is_the_page_plus_a_fixed_frame` still holds for a page one command carries.
**Done when:** `cargo nextest run -p pos-hardware escpos::` exits zero proving that a page taller than its profile's `raster_max_rows` is emitted as consecutive raster commands none taller than the limit, whose payloads join to the bitmap's bytes unchanged, framed by one `ESC @` and one cut with no `ESC p` outside the payloads. The paper half — a receipt taller than 2303 dot rows printed whole on a TM-T20III — is milestone 5.8's (5.8.3), because no printer is bought before it ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b).

### 1.7.10 — The network printer: bytes out, status back
**Scheduled in:** Phase 1B, after 1.7.8 gives the trait its four outcomes and 1.7.9 gives it a profile; before 1.7.6's poll is demonstrated against the emulated printer, and before 1.8.3 prints a sale through it
**Files:** `crates/pos-hardware/src/transport.rs` (new) · `crates/pos-hardware/src/net.rs` (new) · `crates/pos-hardware/src/lib.rs` (`pub mod transport;` and `pub mod net;`) · [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §2 and §6a · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
**Nothing owned the driver.** [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §2 says the bytes travel *"over TCP port 9100 for network printers, `serialport` for RS-232, `rusb`/`hidapi` for USB"*. 1.7.4 produces them, 1.7.6 polls a status, 1.7.7 queues jobs and 1.7.8 simulates every fault — and no `Files:` line in any phase wrote one byte to a real printer or read one back. This step is that driver, network first.
**Network first, because it is the only transport that needs nothing.** Raw TCP on port 9100 needs no driver on any operating system and reaches the printer from every register on the LAN. USB is not equivalent: Epson's TM-T88VII and Star's TSP143IV default to a vendor-specific USB class, which on Windows needs the vendor's driver or a WinUSB binding before a byte moves. Serial and USB transports are later steps, written when a qualified device needs one, and the profile's `transport` says which. `Transport` is a trait — connect with a deadline, write all, read with a deadline — so serial, USB, Bluetooth or an Android maker's print service can later replace the socket without touching outcomes, status or the drawer.
**`print` classifies; it never retries.** A refused connection, or a pre-flight status of paper-out, cover-open or offline, is `Failed` — nothing was sent. Bytes accepted and then an error is `Partial`, carrying the acknowledged count. A write that times out, or a peer that goes silent, is `Unknown`. These are 1.7.8's four outcomes, and the rules that make them safe are [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §3's, unchanged: only `Failed` is retried, and only by the queue.
**`status` reads the device's real-time status, and admits when a device has none.** It sends `DLE EOT` for printer, offline-cause, error-cause and paper-sensor status and parses each reply under a short deadline. A device that never answers reports *status unsupported* — neither ready nor offline — because that is a fact about the device rather than about this moment: Xprinter's own 80-series manual marks its `DLE EOT` as valid only on serial printers. §3 rule 1 has to know that such a device cannot warn before tender, and the profile's `status_protocol` records it.
**`open_drawer` sends `ESC p m t1 t2` from the profile**: the pin, 2 or 5 — which one fires is a property of the drawer — and the on- and off-times. It goes on its own connection and is never part of an artifact (§4 of the reference). The driver connects only to the address bound in the register's configuration and never discovers printers by broadcast: port 9100 has no authentication, and a register that printed to whatever answered would print a customer's receipt on a stranger's device.
**A write counts what the socket took, a piece at a time.** A socket reports bytes it buffered, not bytes the device read, so `Partial` carries the count the transport accepted. The document goes out in pieces smaller than any socket buffer, because one blocking write of the whole document that meets a reset fails without saying how much it had queued — and would report a device that took bytes as one that took none: `Failed`, the outcome the queue retries, over paper that exists. The error is always in the safe direction: a device that dies early costs a person one question, never a second original.
**Tests:** `a_refused_connection_is_failed_and_nothing_is_sent` · `a_document_the_device_takes_whole_is_printed` · `a_device_that_closes_mid_document_is_partial` · `a_device_that_falls_silent_is_unknown` · `a_paper_out_pre_flight_fails_the_document_before_a_byte` · `a_device_that_never_answers_dle_eot_reports_status_unsupported` · `a_profile_that_says_it_has_no_status_is_never_asked` · `the_status_bits_are_parsed` · `the_drawer_pulse_is_the_profiles_pin_and_timings` · `an_unknown_drawer_pulse_is_refused_not_guessed` · `a_profile_for_another_transport_is_refused`
**Done when:** `cargo nextest run -p pos-hardware net::` exits zero against an in-process TCP fake, proving each socket event is classified as one of the four outcomes and that a device which never answers `DLE EOT` reports status unsupported. The device half — a golden printed, paper-out and cover-open read back, and the drawer kicked exactly once, on a TM-T20III and an SRP-332III — is milestone 5.8's (5.8.3); until then 1.7.13 runs this driver against every printer profile through the emulator.

### 1.7.11 — Device profiles, as data
**Scheduled in:** Phase 1A, after 1.7.9 makes the printer profile a type; before 1.7.12 has anything to emulate
**Files:** `crates/pos-hardware/profiles/` (new — one JSON file per profile version, under a directory per class, with `versions.blake3` and a `README.md`) · `crates/pos-hardware/src/profile.rs` · `crates/pos-hardware/src/profile/data.rs` (new — the format and its loader) · `crates/pos-hardware/src/escpos.rs` (width and framing read through the profile) · `crates/pos-hardware/Cargo.toml` and `Cargo.lock` (`serde` and `serde_json`; `blake3` for the tests) · `crates/pos-hardware/tests/profiles.rs` (new) · [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b.2 (the seed table made exact) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
**No device is bought before milestone 5.8, so the devices this product is built against are data** ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b). 1.7.9 made `PrinterProfile` a struct; this step makes every device profile a versioned file — one per model, or one per class where the file describes a sweep rather than a model — read by the Rust emulators and suites and by the register's web tests alike. It starts the receipt-printer, cash-drawer, scanner and register-screen classes from the reference's seed table (§6b.2); every later class starts in the microstep that builds its trait.
**Three rules, each with a test.** Every value names its source. `unknown` is a value and never an omitted field, and a profile with an unknown in a parameter the driver needs cannot be bound to a register — 1.7.9's rule for the raster limit, generalised to every field. A changed value is a new profile version and the old one stays, because a capture, a fixture or a qualification may name it; a committed hash per version is what makes an in-place edit fail. The parameters a printer's driver needs are `pos_hardware::profile::BINDING`, and `PrinterProfile::unresolved` names the ones a profile leaves for the gate; the format, and how to add a version, is `crates/pos-hardware/profiles/README.md`.
**Tests:** `every_device_profile_names_a_source_for_every_value` · `an_unknown_profile_value_cannot_be_bound_to_a_register` · `a_changed_profile_value_is_a_new_profile_version` · `every_seeded_profile_is_unknown_exactly_where_the_seed_table_says`
**Done when:** `cargo nextest run -p pos-hardware --test profiles` exits zero proving that every seeded profile loads with a source on every value and `unknown` exactly where the reference's seed table has one, that a profile whose raster limit is unknown refuses register binding, and that a value edited without a new version fails against its committed hash.

### 1.7.12 — The emulated network printer
**Scheduled in:** Phase 1B, after 1.7.10's driver and 1.7.11's profiles
**Files:** `crates/pos-hardware/src/emulator/` (new — the device model and its fault script, the command parser, and the PNG page) · `crates/pos-hardware/src/bin/escpos-emulator.rs` (new) · `crates/pos-hardware/tests/emulator.rs` (new) · `crates/pos-hardware/Cargo.toml` (an `emulator` feature, off by default, and a dev-dependency on the crate itself with it on) · `Cargo.toml` and `Cargo.lock` (`png`, already in the register's graph) · `crates/pos-hardware/src/lib.rs` (`pub mod emulator;` behind that feature) · `justfile` (`just printer-emulator <profile>`) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines, and its `Full-step status:` marker)
**The second rung of the ladder** ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b.1): a process that listens on TCP 9100 and behaves as one printer profile and one drawer profile say a device does. It accepts `ESC @`, `GS v 0`, `GS ( L` and `GS V`; it refuses a raster command taller than the profile's limit in the way the profile records, and where the profile says `unknown` it reports *unknown behaviour* rather than inventing one; it answers `DLE EOT` with the profile's replies, late, or not at all; it records each `ESC p` with its pin and timings and reports the drawer sensor at the profile's polarity; and it renders every raster it accepts to a PNG through this crate's own renderer, beside the raw bytes it received. Faults are scripted per connection — paper-out, cover-open, offline, slow, and a peer that goes silent after *n* bytes — so 1.7.8's in-process sweep has a twin on a real socket.
**A receipt can be read before a printer exists.** `just printer-emulator <profile>` starts it on the developer's machine, and the terminal prints to it exactly as it will print to a device; the image it writes is what a manual pass reads ([`02-development-workflow.md`](02-development-workflow.md) §5).
**It is a tool, not product.** Nothing a register ships enables the `emulator` feature, and a test holds the terminal's dependency tree to that. The crate's own tests enable it through a dev-dependency on the crate itself, so `just test` and CI run every emulator test without a flag anyone has to remember; a dev-dependency's features never reach a build that ships.
**Where a profile says `unknown`, the emulator prints or opens nothing and says so.** A raster against an unknown limit or framing, a status request to a device whose status protocol nobody stated, a cut whose command is unknown, and a pulse to a drawer whose pin nobody measured are each an `UnknownBehaviour` event naming the value. The drawer opens only when the pulse's pin is the drawer profile's and the printer port's voltage lies within the solenoid's range, and its sensor reads back through `DLE EOT 1` at the drawer profile's polarity.
**Tests:** `emulated_printer_renders_every_golden_to_its_committed_png` · `emulated_printer_answers_status_as_its_profile_declares` · `emulated_printer_refuses_a_raster_taller_than_its_profile_allows` · `emulated_printer_records_each_drawer_pulse_with_its_pin_and_timings` · `the_register_bundle_never_enables_the_emulator` · `emulated_printer_renders_what_the_driver_sends_dot_for_dot` · `each_scripted_fault_reaches_the_driver_as_its_outcome` · `each_fault_answers_with_epsons_status_bits`
**Done when:** `cargo nextest run -p pos-hardware --features emulator` exits zero proving that the seven goldens sent through 1.7.10's driver come back as their committed PNGs under each 80 mm and 58 mm profile, that status, refusal and drawer behaviour follow each profile, and that the terminal's resolved feature set excludes `emulator`.
**Full-step status:** 1.7.12 is not complete until `emulated_printer_renders_every_golden_to_its_committed_png` exists and passes, which needs 1.7.5's seven committed goldens. Everything else is complete and enforced now: status, refusal and drawer behaviour follow each seeded profile, every scripted fault reaches 1.7.10's driver as its outcome, the terminal's resolved feature set excludes `emulator`, and — in the goldens' place until they exist — the receipt they will be rendered from comes back dot for dot at both widths, with its PNG reading back as the same page.

### 1.7.13 — One conformance suite, two targets
**Scheduled in:** Phase 1B, after 1.7.12
**Files:** `crates/pos-hardware/tests/conformance/` (new) · `crates/pos-hardware/tests/captures/` (new — a synthetic capture until 5.8.3 commits the first real one) · `crates/pos-hardware/tests/support/receipt.rs` (new — the synthetic receipt, moved out of `tests/emulator.rs` so both targets print the same one) · `infra/emulators/emupos/` (new — the pinned independent emulator: a `Dockerfile` and hash-locked requirements) · `.github/workflows/ci.yml` (the suite, once per printer profile, and the cross-check job) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines, and its `Full-step status:` marker)
**This is what makes deferring the purchase safe** ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b.3). One suite, written against `ReceiptPrinter` and a profile, asserting the reference's §3 and §4 contract rather than any device's convenience: the four outcomes; status read, or reported unsupported; strips at the profile's limit (case 93); exactly one drawer pulse per authorised opening and none on a retry, a reprint or a queue drain; paper pulled mid-print; and a printer absent when the shift opens. CI runs it against 1.7.12's emulator once per printer profile — the wide matrix, on every commit. At milestone 5.8 the same suite runs against a physical device, and only when both the device's address and the profile it is being qualified against are given explicitly: a suite that could find a printer by itself could print on a stranger's.
**A second reader nobody here wrote** ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b.8). Our emulator and our encoder read the same specification, so one misreading passes both. CI therefore also replays every golden and every conformance byte stream through emupos — one exact version, installed from hash-locked requirements into an image built from `infra/emulators/emupos/`, never published and never on a register — and requires a dot-identical render and a `DLE EOT` reading that agrees with ours. Where emupos models nothing (row limits, width agreement), 1.7.12 stays the only judge; a disagreement fails the build and is investigated, never suppressed.
**A capture is a fixture.** When 5.8.3 records what a device actually sent back, the bytes and replies land under `tests/captures/<profile>/` and the emulator must replay them from then on, so a profile corrected by evidence stays corrected.
**Tests:** `the_printer_conformance_suite_passes_for_every_profile` · `the_device_target_refuses_to_run_without_an_explicit_device_and_profile` · `the_printer_conformance_suite_passes_against_the_named_device` (the device target itself, run at the gate) · `a_captured_device_trace_replays_through_the_emulator` · `every_conformance_stream_renders_dot_identical_through_the_independent_emulator` · `every_golden_renders_dot_identical_through_the_independent_emulator`
**Done when:** `cargo nextest run -p pos-hardware --features emulator --test conformance` exits zero with one run per profile under `profiles/receipt_printer/`, the device target refuses unless both `HW_DEVICE_ADDR` and `HW_PROFILE` are set, the synthetic capture replays through the emulator, and CI's cross-check job renders every conformance byte stream and every golden dot-identical through the pinned emupos, with `DLE EOT` replies that agree with ours in every state.
**Full-step status:** 1.7.13 is not complete until `every_golden_renders_dot_identical_through_the_independent_emulator` exists and passes, which needs 1.7.5's seven committed goldens. Everything else is complete and enforced now: the suite runs for every receipt-printer profile, the device target refuses unless both inputs are named, the synthetic capture replays, and CI's `printer-conformance` job replays every conformance stream through the pinned emupos dot-identically, with status bytes that agree in every state. Two of §6b.3's clauses are asserted here at the driver and finished elsewhere: no print ever emits a drawer pulse, and 1.7.7's `receipt_retry_never_repeats_the_drawer_pulse` holds the queue to it; an absent printer is `Failed(Unreachable)`, and §3a's tests hold the shift to opening anyway. `Partial` cannot be produced by the emulator, which has no fault that closes mid-document; 1.7.10's `a_device_that_closes_mid_document_is_partial` proves it.

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

The gate passes only when every command exits zero, every paired receipt `.bin`/`.png` golden is clean, and the exact mixed-rate and `mixed_tender_1247_card_624_cash_620_adjustment_minus_3` fixtures pass. It does not include the storage-backed queue steps 1.7.6b/1.7.7, the network driver 1.7.10, or the emulator and conformance suite 1.7.12–1.7.13 that drive it — all Phase-1B work, and none of it waiting on a device. Build migrations in numeric order despite the document's domain-first reading order: execute `1.9.1`/`0005` and `1.10.1`/`0006` before committing `1.2.5`/`0007`.

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
**Settled on 24 September by #234.** The compiled build is SQLite `3.50.4` under SQLCipher `4.10.0 community`, and it lacks the WAL-reset fix: SQLite fixed it in `3.51.3`, and SQLCipher `4.14.0` is the first release found to carry it. The multi-connection minimums are therefore **SQLite `3.51.3` and SQLCipher `4.14.0`**, and [`ref/plan-validation.md`](ref/plan-validation.md) §5 carries the sources this step's policy must name and hash. The bump that reaches them is #244, blocked by `sqlx-sqlite`'s `libsqlite3-sys <0.38.0` bound. **One design question comes with the answer:** read literally, *"refuse an unsupported … build"* with those two minimums would refuse **today's** build at every open. So the policy must separate what a build may do. Either the minimums gate only a second source connection, so a build below them opens in single-connection mode, or the policy carries two tiers: a floor to open at all, and a floor for concurrency. `multi_connection_wal_is_disabled_until_the_storage_gate_closes` already assumes the first reading. Whoever builds this step writes the choice down.
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

`deactivated_user_denied` belongs to this command-handler gate, not to 1.6.4's repository suite. `app_user.is_active` is stored state, but observing that state in `pos-db` does not prove a command was denied, and repository law excludes business policy. The handler must treat either `is_active = 0` or a non-NULL `deleted_at` as inactive before the financial write. This test names the acting user only; 1.6.4 records the still-open question of whether later deactivation of the **approver** revokes an unexpired handle.
**Tests:** `finalize_write_point_catalog_is_exhaustive` · `finalize_is_atomic_under_injected_failure` · `manifest_and_delivery_rows_precede_guarded_facts` · `hardware_failure_after_commit_leaves_sale_complete` · `a_complete_sale_has_one_ready_commit_manifest` · `no_ready_commit_omits_a_required_member` · `approval_effect_audit_and_consumption_commit_together` · `deactivated_user_denied` · `finalize_removes_the_checkout_operation_in_the_same_commit` · `department_line_writes_no_stock_event`
**Done when:** `cargo nextest run -p terminal commands::sale::tests::` injects failure at every `FinalizeWritePoint::ALL` entry and leaves either the whole fact graph plus ready manifest or no financial fact, artifact, job, consumption or delivery row.
> **1.3.4's terminal half waits on this file.** Finalize must refuse a store whose tax computation policy is not approved, through `crates/pos-db/src/tax_policy.rs`, and 1.3.4's terminal test proves it for both finalize and provisioning. It lands with whichever of this step and 1.8.5b comes second; 1.3.4's `Full-step status:` marker is what it clears.

> **Group 1.3 leaves three things to this file.** A line's components come from `pos_db::tax_pack::TaxPackRepository::components_for` (1.3.7), which filters a store's pack by business date exactly as `0005`'s completion gate does, rather than from `resolve_components`, whose `TaxRateRule` carries instants. The `StoreTaxProfile` handed to `compute_line_tax` is read from two columns `store` keeps apart: `gst_profile = 'unregistered'` is `Unregistered` whatever the jurisdiction, and otherwise `tax_profile` names it (1.3.8). And a GST-unregistered store's lines carry no component, while `0005`'s `sale_completed_requires_tax_components_*` triggers refuse a completed line with no `sale_line_tax` row, so such a store cannot finish a sale until a forward migration lets a line go untaxed when its store's `gst_profile` is `'unregistered'`. That migration is this step's, before its first unregistered sale.

> **1.4.1 assigns `build_receipt_model` to this step** ([`ref/domain-api.md`](ref/domain-api.md) §13), as 1.7.1's note asked. It is a pure function from a `CompletedSale` to the `ReceiptModel` 1.7.1 validates, so it belongs in `crates/pos-domain/src/receipt.rs`; this is the first step with a completed sale to print, and it calls `ReceiptModel::validate` on the result before the receipt bytes are rendered.

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
> **1.3.4's terminal half waits on this file too.** Provisioning must refuse a store whose tax computation policy is not approved, and 1.3.4's terminal test proves it for both provisioning and finalize. It lands with whichever of this step and 1.8.3 comes second; 1.3.4's `Full-step status:` marker is what it clears.

> **1.3.8 leaves provisioning one setting to write explicitly.** `0003` defaults `store.gst_profile` to `'unregistered'`, while merchant decision 6.2b assumes the merchant is registered (and still names `store.tax_profile` as its column). Nothing reads the column yet; once 1.8.3 does, a store provisioned without it would charge no GST. Provisioning therefore writes `gst_profile` from the merchant's registration evidence and never leaves the default in place.

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
**Files:** `crates/pos-db/src/repo/outbox.rs` (new) · `crates/pos-db/src/repo/mod.rs` (new) · `crates/pos-db/src/lib.rs` (`pub mod repo;`, and `DbError::{EmptyCommitRefused, CommitEnvelopeIncomplete, IdWidthInvalid}`) · `crates/pos-db/tests/outbox.rs` (new) · `Cargo.toml` (`blake3` in `[workspace.dependencies]`) · `crates/pos-db/Cargo.toml` (`blake3.workspace = true`) · `Cargo.lock`
> **This list was one file until 1.8.9 was built**, which is the shape 1.2.1 predicted. Two
> reasons, both structural rather than incidental. This is the crate's **first repository**, so it
> creates `repo/mod.rs` and the `pub mod repo;` that carries it — 1.2.3 no longer creates either.
> And `commit_hash` is the workspace's **first hash**: [`ref/sync-protocol.md`](ref/sync-protocol.md)
> specifies BLAKE3, [`ref/security-compliance.md`](ref/security-compliance.md) §4 records why, and
> no BLAKE3 crate was in the graph, so `blake3` joins `[workspace.dependencies]` and the lockfile
> moves with it. The reference pins the algorithm and the subject and leaves the byte layout open;
> the layout chosen — version byte, domain separator, then length-prefixed members — is documented
> at `canonical_commit_bytes` in `repo/outbox.rs`, beside the named constants that pin it.
>
> What this step does **not** create is `crates/pos-sync/src/canonical.rs`, and `pos-db` does not
> gain a dependency on `pos-sync`. The shared canonical projection arrives with the sync engine;
> until then the writer hashes exactly the canonical bytes its caller hands it, which is what keeps
> `payload_hash` incapable of describing a different payload from the one stored beside it.

One business transaction creates one immutable `sync_commit`, a permanent `fact_commit_member` row for every constituent fact, and corresponding `sync_outbox` delivery rows in the same transaction (I-9). The manifest survives delivery-row pruning, so the server can validate the complete sale graph as one atomic commit in Phase 3. No pusher exists yet, but complete envelopes accumulate from the first sale.
**Tests:** `a_completed_sale_has_one_ready_sync_commit` · `every_fact_member_is_in_the_commit_manifest` · `delivery_rows_can_be_pruned_without_losing_the_manifest` · `outbox_commit_rolls_back_with_the_fact_graph`
**Done when:** `cargo nextest run -p pos-db outbox::` passes after pruning acknowledged delivery rows and reconstructing the original commit membership unchanged.

---

## Group 1.9 — Sequences and business date

*Gap G-2.*

### 1.9.1 — Migration `0005`
**Files:** `crates/pos-db/migrations/0005_sale_columns_and_sequences.sql`, `crates/pos-db/tests/migration_0005_sale_columns_and_sequences.rs`, `crates/pos-db/tests/common/registered_chain.rs` (new) · `crates/pos-db/src/lib.rs` (`MIGRATIONS`; `SCHEMA_VERSION` follows the array length) · `crates/pos-db/tests/common/mod.rs` (`reference_blocks_at_or_after(5)` → `6`) · `crates/pos-db/tests/{approval,durability,migration_0003,outbox,sale_immutability}.rs` (the seventeen pre-0005 tests, repaired against the registered chain) · `apps/server/migrations/20260914090000_sale_columns_and_sequences.sql` (new — the mirror, which creates nothing and records why) · [`ref/schema.md`](ref/schema.md) (§0005 heading gains `· SHIPPED`; the two ICV `⚠️ OPEN` blocks rewritten) · [`README.md`](README.md) (implementation frontier) · this file (this `Files:` line and the `⚠️ OPEN` block below)
> **The test files are scope, not a scope leak.** 0005 repairs `sale.register_id` with a trigger and
> puts seven gates in front of completion, so every pre-0005 test that completed a *minimal* sale
> against no register at all turns red the moment the migration is registered — seventeen of them
> did. Sixteen fail on the register repair alone; one reaches the durable-outputs gate. The repair
> is one shared fixture seeding org, an approved rate pack and policy, an evidenced store, registers
> and open shifts, plus the auxiliary facts a completion must prove — not seventeen local patches.
> Two consequences outlive it: a sale can no longer be born `completed`, because its original
> receipt artifact carries `REFERENCES sale(id)` and the gate wants that artifact already there; and
> the smallest fact graph a checkout produces is **seven** facts rather than three, which is what
> `sale_commit_base_complete` means by a whole sale.

Per [`ref/schema.md`](ref/schema.md) §0005: immutable `shift` opens, append-only `shift_close_event`, rebuildable `shift_state` and the one-open-per-register index; sale shift/store/business-date and tax-snapshot guards; persisted `trusted_time_state`; `sale_tax_summary`; `tender_status_event` plus its projection; the full `tender_type` seed including `exchange` with `opens_drawer=0`, `allows_change=0`, `is_cash_counted=0`, `is_internal=1`; durable parked/active `parked_cart` working states and session claims; `checkout_operation`; register-local, content-hashed `product_quick_add_request`, keyed by the eventual `product.id`; immutable receipt artifacts, print jobs and attempts; completed-fact manifest guards; and scoped `doc_sequence`.
**Tests:** `opening_a_second_shift_for_the_register_is_refused` · `a_completed_sale_requires_an_open_matching_shift` · `migration_0005_preserves_completed_sale_guards` · `every_completed_tender_has_an_initial_status_event` · `exchange_tender_seed_matches_internal_contract` · `an_exchange_tender_never_opens_or_counts_the_drawer`
**Done when:** `just verify-schema` applies `0001`–`0005` and `cargo nextest run -p pos-db` passes whole — the six tests above green and the seventeen pre-0005 tests repaired rather than deleted or ignored, with the existing completed-sale immutability guards intact.

> ⚠️ **DECIDED for this migration; the question itself is still open and now binds `2.7.4`.** The "scoped `doc_sequence`" above carries unratified merchant decision 6.9 — the authoritative ICV namespace, one of register, store, income source, credential, or one TIN across stores. On 13 September 2026 **#113 took option 4: freeze `store`, knowingly**, and this migration shipped `CHECK (scope_kind IN ('register','store'))`. Migrations are forward-only and are never edited once committed, so that `CHECK` is uneditable now. The choice could not be deferred because one table serves two counters: `('register', register_id, 'receipt')` numbers receipts for `1.9.3`, and deferring `doc_sequence` to wait on a fiscal answer would have taken Phase-1 receipt numbering with it. The mistake stays cheap only while no `fiscal_icv` row exists — none is allocated at checkout, and none at all before `2.7.4` — so until then a wrong scope costs one further forward-only migration and no data repair, and after it a migration plus a repair on a gapless sequence. **`2.7.4` must re-check 6.9 before allocating the first value.** `ref/merchant-decisions.md` row 6.9's Answer cell stays **empty** and **#113 stays open**: what is settled is what `0005` does, not what the namespace is. Owner: `2.7.0` ratifies 6.9 via #69, on a timeline outside this project's control. Source that settles it: the official ISTD business rules or a written ISTD E-Invoicing Directorate ruling.

**Deferred 1.1.9 database half — scheduled immediately after this migration:** `trusted_time_state` must exist before the repository can persist the pure domain `ClockState`; do not invent the table early. That reconciliation is **done**: [`ref/schema.md`](ref/schema.md)'s planned table now maps `ClockState` field for field — `last_trusted_at`, `device_at_trust`, `monotonic_since_trust_ms`, a `NOT NULL` `high_water`, the three structured `anomaly_*` columns and `boot_token` — and the stale pre-§3.2 `confidence` / `observed_skew_milli` snapshot columns are gone, because both are outputs of `clock_confidence(...)` and a stored verdict can disagree with its own inputs. The shell/repository integration must still persist the opaque shell-owned boot-continuity token beside the value, compare it on startup, and call `ClockState::note_monotonic_reset` before use when it changes; the numeric counter alone cannot distinguish every reboot.
**Files:** `crates/pos-db/src/repo/clock.rs` (new) · `crates/pos-db/tests/clock.rs` (new) · `crates/pos-db/src/repo/mod.rs` (`pub mod clock;`) · `crates/pos-db/src/lib.rs` (`DbError::ClockStateInvalid`) · [`README.md`](README.md) (implementation frontier) · this file (the `Full-step status:` marker deleted, and 1.1.9's `Scheduled in:`)
**Tests:** `clock_state_survives_restart`
**Done when:** `cargo nextest run -p pos-db clock::` passes with the same `ClockState` after a restart.
> **The module name is load-bearing**, as it was for 1.8.9: nextest matches `clock::` against the test's own name, not the binary id `pos-db::clock`, so the suite lives in a `mod clock { … }` or the proving command passes by running no tests at all. Six further tests ship beside the named one, because a round-trip that only ever stores `Some` proves half the mapping: the three independently nullable columns, each `ClockAnomaly` variant through the flattened `anomaly_*` group, the upsert replacing rather than appending, a cleared anomaly clearing all three columns, and an `anomaly_kind` a newer build wrote being **refused rather than read as the absence of an anomaly** — the one failure here that would downgrade a register from "something is wrong with the clock" to "nothing is wrong with it". `trusted_time_state` is register-local, not a fact (`ref/schema.md` §"Convergence"), so the repository writes an upsert and takes no `sync_commit` envelope; it still takes an explicit `&Transaction`, because `repo/mod.rs` makes the caller own the boundary.

### 1.9.2 — `SequenceRepository`
**Files:** `crates/pos-db/src/repo/sequence.rs` (new), `crates/pos-db/tests/sequence.rs` (new) · `crates/pos-db/src/repo/mod.rs` (`pub mod sequence;`) · `crates/pos-db/src/lib.rs` (five `DbError` variants, as `1.1.9` did for `ClockStateInvalid`) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line)
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
**Files:** `crates/pos-db/migrations/0006_stock_ledger.sql` (new), `crates/pos-db/src/lib.rs` (`MIGRATIONS`), `crates/pos-db/tests/migration_0006_stock_ledger.rs` (new), `apps/server/migrations/20261022120000_stock_ledger.sql` (new — the mirror, which creates nothing and records why), `crates/pos-db/src/fixtures/registered_chain.rs` (a stock event per line, and the receipt written after the envelope), `crates/pos-db/tests/common/mod.rs` (`reference_blocks_at_or_after(7)`), the `pos-db` suites whose sale graph grew by its stock event (`outbox.rs`, `receipt.rs`, `migration_0005_sale_columns_and_sequences.rs`), `apps/terminal/src-tauri/src/print_queue.rs` (a duplicate's envelope written before the duplicate) · [`ref/schema.md`](ref/schema.md) (§0006 marked SHIPPED, with #197's guards and the derived-weight correction) · `scripts/check-test-catalog.py` (one name: `PLANNED` → `PLANNED_RETIRED`) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line and the #197 note below)
Create append-only `stock_ledger` with `qty_delta_milli`, the captured `qty_step_milli`, nullable `unit_cost_minor INTEGER`, `is_cost_estimated INTEGER NOT NULL DEFAULT 0`, `is_weight_derived INTEGER NOT NULL DEFAULT 0` and the post-event projection watermark, plus rebuildable `stock_cache` and the register-local working table `stock_adjustment_request`. Every sale event captures the cost basis in force; a later WAC change must not rewrite historical margin, and a price-embedded label must preserve that its stock weight was derived rather than measured. The request table freezes the product, quantity, reason and note before manager approval; it is not a stock fact and is removed only by the transaction that posts the ledger event.
**Tests:** `stock_ledger_is_append_only` · `a_sale_event_records_the_cost_basis_at_capture_time` · `missing_cost_is_null_and_reported_as_estimated` · `price_embedded_stock_event_carries_the_derived_weight_flagged_estimated` · `migration_0006_preserves_quantity_steps`
**Done when:** `just verify-schema` applies `0001`–`0006` and `cargo nextest run -p pos-db --test migration_0006_stock_ledger` proves a later cost change leaves the original sale event's `unit_cost_minor` unchanged.

> **Delivered with `0006`: the three guards `0005` did not ship** (#197, inherited from #179). The
> second took the trigger route for `receipt_artifact` and the recorded-reason route for
> `print_attempt`, which is register-local and never syncs; `0006`'s header records why.
>
> **`0006` also carries three guards `0005` did not ship, and it is the only migration that can.**
> Tracked as **#197**, inherited from #179. They are absent guards rather than live defects, and the
> reason they ride here is arithmetic: `0007` is named by `1.2.5`, and `verify-schema.py` requires
> migration numbers contiguous from `0001`, so a dedicated `0008` could not land until both exist.
>
> 1. `CHECK (is_training IN (0,1))` on `sale` — the fiscal-decision gate keys on `is_training = 0`,
>    so today a value of `2` is a completed non-training sale with no fiscal decision behind it.
>    `ref/schema.md`'s line has the same gap and is corrected in the same change.
> 2. `receipt_artifact_has_ready_commit` and `print_attempt_has_ready_commit`, **or** a recorded
>    reason in `ref/schema.md` that these two declared fact tables are deliberately unlike their five
>    siblings. Either closes it. **If the triggers are added,
>    `sale_completion_requires_a_manifest_naming_every_fact` must omit a different member** — it
>    omits `receipt_artifact` today precisely because that table has no envelope gate, and it already
>    iterates four other members that would serve.
> 3. `doc_sequence_no_delete` — `DELETE FROM doc_sequence` silently resets a G-2 gapless counter to
>    1, and every sibling append-only table in `0005` has a delete guard. `1.9.2` closed the test
>    half of this one; the guard itself needs a migration.
>
> Each wants a negative test in the shape `1.9.1`'s gates now use — withhold one precondition, assert
> the exact refusal message — and the Postgres mirror per `verify-pg-migrations.py`'s two-way check.
> **Unrelated to #197 but due with `0006` regardless:** `reference_blocks_at_or_after(6)` becomes
> `(7)` in `crates/pos-db/tests/common/mod.rs`, or every `full_schema()` test double-applies §0006.

### 1.10.2 — `StockRepository`
**Files:** `crates/pos-db/src/repo/stock.rs` (new), `crates/pos-db/src/repo/mod.rs`, `crates/pos-db/src/lib.rs` (eight `DbError` variants) · `crates/pos-domain/src/stock.rs` (new — the event types and the cost-capture rule `append` needs, which no earlier microstep delivered), `crates/pos-domain/src/lib.rs` · [`ref/domain-api.md`](ref/domain-api.md) §12 (as delivered) · [`ref/sync-protocol.md`](ref/sync-protocol.md) §3 (the payload a stock event travels as) · [`phase-2-money-grade.md`](phase-2-money-grade.md) 2.3.4 and [`phase-4-depth.md`](phase-4-depth.md) 4.2.2 (the WAC blend each meets) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` and `Tests:` lines, its signatures and its note)
```rust
pub fn prepare(&self, tx: &Transaction, register: RegisterId, movements: &[NewStockEvent])
    -> Result<Vec<PreparedStockEvent>, DbError>;                                // reads, writes nothing
pub fn append(&self, tx: &Transaction, events: &[PreparedStockEvent]) -> Result<(), DbError>;
pub fn on_hand(&self, p: ProductId, s: StoreId) -> Result<Qty, DbError>;      // from cache
pub fn rebuild_cache(&self, tx: &Transaction, s: StoreId) -> Result<u64, DbError>;
pub fn negative_stock(&self, s: StoreId) -> Result<Vec<NegativeStockRow>, DbError>;
```
`append` writes the ledger event, cache projection, audit row and fact-commit membership in one transaction. A partial cache update is not a performance problem; it is a false on-hand figure shown to the cashier.
**Tests:** `stock_append_projects_cache_and_manifest_atomically` · `stock_event_captures_unit_cost_and_estimate_flag` · `prop_an_outgoing_event_carries_the_basis_in_force` · `prop_the_wac_after_an_event_is_a_recorded_cost`
**Done when:** `cargo nextest run -p pos-db repo::stock::tests::` rolls back every companion row under injected failure at each write boundary.
> **As delivered, 25 October 2026.** `append` became two steps, because `0006` refuses a stock event whose ready envelope is not already written, and the envelope carries the event's payload. `prepare` reads each product's position and the register's ledger head through the caller's transaction and asks the domain's `StockEvent::capture` for the event and the position it leaves; the caller writes the envelope with each `PreparedStockEvent::payload`, then any audit row; `append` inserts the events and refuses one its envelope does not describe exactly. A batch prepares in order, each movement from the position the one before it left, so a sale with two lines of one product records two consecutive events. The domain types the signature assumed did not exist, so `pos-domain/src/stock.rs` arrives here with the cost-capture rule: an outgoing event carries the basis in force and leaves it; an incoming event into stock that is not positive sets the WAC to its own cost, at the WAC in force keeps it, and with no cost keeps it and marks it estimated. **A different cost into positive stock is refused by name rather than blended**: the blend is `recompute_wac`, whose properties are 4.2.2's, and 2.3.4's restock is the first step that will meet it. `rebuild_cache` replays a store's ledger — on-hand as the sum, the WAC and the watermark from each product's highest `event_seq` — and refuses a ledger that holds two registers' events, which no owned sequence orders; `stock_ledger` travels up only, so a register's database holds its own. The write boundaries are injected by a temporary trigger in front of each of the seven writes — the commit, its members, their delivery rows, the audit row, the event, and the cache row inserted or updated — rather than by a seam in the code: the storage engine is the boundary, and the repository does not know it is under test.

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
**Files:** `apps/terminal/package.json` (four test devDependencies) · `pnpm-lock.yaml` (their resolutions) · `apps/terminal/vite.config.ts` (the jsdom environment, its document fixture and the setup file) · `apps/terminal/src/test/setup.ts` (new) · `apps/terminal/src/screens/Sale.test.tsx` (new) · `apps/backoffice/vite.config.ts` (the justifying comment that called the terminal's suite DOM-free) · [`02-development-workflow.md`](02-development-workflow.md) (§4.3's Vitest row and §17's DOM-harness gap) · [`ref/test-catalog.md`](ref/test-catalog.md) (the DOM-harness row) · [`ref/ui-spec.md`](ref/ui-spec.md) (§8's harness paragraph) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line and prose)

```ts
export function renderWithProviders(ui: React.ReactElement): RenderResult;
```

Add `@testing-library/react`, `@testing-library/user-event`, `@testing-library/jest-dom` and `jsdom`; configure Vitest with `environment: "jsdom"`. Scan-burst tests use fake timers and `user-event`'s `advanceTimers`, so the `< 30 ms` heuristic is deterministic rather than scheduler-dependent.

Three things the harness owes, each because leaving it out fails later as something other than itself. **`dir="rtl"` comes from `index.html`, read by `vite.config.ts`** — vitest never loads that file, and a literal retyped into the config leaves the canary green when the served document itself regresses. **`cleanup` is registered explicitly**: Testing Library installs that hook only when a global `afterEach` exists, and `globals` is off here, so without it the second rendering test in any file fails as a duplicate-element selector error rather than as a leaking harness. **A minimal `jest` global exposing `advanceTimersByTime` bridges to `vi`**: Testing Library gates its async wrapper's clock advance on that global, so until it exists every `await user.*()` under `vi.useFakeTimers()` waits on a timeout the frozen clock never fires — the sentence above is a promise the harness has to keep, not a property it inherits.
**Tests:** `sale_screen_renders_in_rtl_by_default`
**Done when:** `pnpm --filter terminal exec vitest run src/screens/Sale.test.tsx` executes the canary in a DOM and exits zero.

### 1.11.1 — i18n infrastructure
*Gap G-5.*
**Files:** `apps/terminal/src/i18n/` (new: `index.ts`, `ar.ts`, `en.ts`, `catalog.test.ts`), `apps/terminal/src/styles/font.css` (new), `packages/ui/src/i18n.ts` (new) · `packages/ui/src/index.ts` · `packages/ui/package.json` (`types`) · `apps/terminal/src/lib/direction.ts` (the four primitives now come from `@pos/ui`) · `apps/terminal/src/main.tsx` (the font stylesheet, and the locale applied at boot) · `apps/terminal/vite.config.ts` (the font-resolution inputs the suite cannot read itself) · `apps/terminal/package.json` and `pnpm-lock.yaml` (the `@pos/ui` workspace dependency) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:` line)
Typed catalog; keys per conventions §2; `<html dir="rtl" lang="ar">` by default.
**The RTL default is a line of product code, not a served attribute.** `index.html` already ships `<html lang="ar" dir="rtl">` and 1.11.0's harness feeds that very file to jsdom, so asserting on it proves the fixture. `useLocale` seeds `DEFAULT_LOCALE` into React state and calls `applyLocale` only on a *toggle*, so before this microstep the register's direction was whatever document it was booted from. `installLocale` is that decision, `main.tsx` makes it before the first render, and `the_entry_point_applies_the_locale_before_the_first_render` imports the entry point against a document flipped to `ltr` so no pass can come from the fixture.
**The locale contract moves to `@pos/ui` rather than being copied there.** Which locales exist, which is the default and which direction each writes in are conventions §10 product law — the back office renders the same two languages — so they have one definition and `lib/direction.ts` imports it. That file keeps what is this app's own: applying a locale to a document root, and the toggle's label.
**Tests:** `catalogs_have_identical_key_sets` · `ui_and_rasterizer_resolve_the_same_embedded_font`
**Done when:** `pnpm --filter terminal exec vitest run src/i18n/catalog.test.ts` exits zero with Arabic as the rendered default, identical typed keys in both catalogues and the UI resolving the exact font asset proven by 1.7.2.

### 1.11.2 — RTL lint
**Files:** `scripts/check-logical-css.sh`, `justfile`
Extend the existing guard that `just lint` and CI's `web` job run. Ban physical direction utilities (`pl-`, `pr-`, `ml-`, `mr-`, `left-`, `right-`, `text-left`, `text-right`) in favour of logical ones (`ps-`, `pe-`, `ms-`, `me-`, `start-`, `end-`, `text-start`, `text-end`). A genuinely physical rule carries `physical-ok: <reason>` on the same line; a bare suppression is refused because unexplained exceptions become the RTL regression path.
**Tests:** `physical_direction_utility_is_rejected`
**Done when:** using `pl-4` fails `just lint`.

### 1.11.3 — Formatting helpers
**Files:** `apps/terminal/src/lib/format.ts`, `apps/terminal/src/lib/format.test.ts` (new) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Tests:` line and `Done when`, from which the bidi test is struck — 1.11.12 owns it)
`formatMoney(minor, currency, locale)`, `formatQty(milli, weighed)`, `formatDate(iso, tz, locale)`. Western Arabic digits. Never `toLocaleString` inline. Transaction totals use the currency exponent; a shorter catalogue display is allowed only when exact.
**Tests:** `formats_jod_at_the_currency_exponent` · `catalog_short_format_refuses_to_hide_fils` · `uses_western_digits_in_arabic_locale`
**Done when:** `pnpm --filter terminal exec vitest run src/lib/format.test.ts` exits zero for exact JOD fils and Arabic locale digits.

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
**Files:** `apps/terminal/src/lib/scanner.ts`, `apps/terminal/src/lib/scanner.test.ts` (new) · [`README.md`](README.md) (implementation frontier) · [`ref/test-catalog.md`](ref/test-catalog.md) and [`02-development-workflow.md`](02-development-workflow.md) (both list these two tests as still owed) · this file (this microstep's `Files:` line)
A hidden input capturing keystrokes anywhere on the sale screen, distinguishing a scan burst from typing by inter-key timing (< 30 ms between characters, terminated by Enter). **Scans must route correctly even when focus is in the search box** — that detail is where most implementations break.
**Tests:** `scan_burst_detected_over_typing` · `scan_routes_while_search_focused` — both use fake timers from 1.11.0.
**Done when:** `pnpm --filter terminal exec vitest run src/lib/scanner.test.ts` exits zero with fake time advancing across both the scan-burst and human-typing thresholds.

### 1.11.6b — Scans that survive the keyboard layout
**Files:** `apps/terminal/src/lib/scanner.ts` · `apps/terminal/src/lib/scanner.test.ts` · [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §5 · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
**A wedge scanner sends key presses, not characters, and the active layout decides what they become.** 1.11.6 reads `KeyboardEvent.key`, the character after that translation. A register in Jordan runs Arabic (101) beside English (US) — the lab installs both ([`../hardware-lab.md`](../hardware-lab.md) §6.5) — and under the Arabic layout a scanner left on its default US layout types Arabic letters wherever its code had Latin ones. A Code 128 label, a QR code or an alphanumeric GS1 element string then arrives as a *different* code rather than an unreadable one, which is worse: a wrong code can match a product. Digit-only EAN and UPC codes usually survive, because the number row gives digits in both layouts — which is exactly why the defect hides in a demo.
**Inside a burst the heuristic has already recognised, decode from `KeyboardEvent.code`** — the physical key, which the platform defines as unaltered by the layout — plus the shift state, through the US-layout table that matches the scanner's default. Outside a burst nothing changes: typing keeps `event.key`, so a cashier's Arabic search is untouched, and the < 30 ms heuristic itself does not move. A scanner configured instead for ALT/keypad emulation produces characters a key table cannot see; the lab records which mode each scanner was qualified in (§6a's scanner row), and a burst that cannot be decoded is refused rather than guessed — E.40's rule, applied one layer earlier.
Its tests are named in the pull request that builds it ([`01-conventions.md`](01-conventions.md) §6): at least that a burst whose keys carry Arabic `key` values and US `code` values decodes to the Latin code, that typed Arabic outside a burst is untouched, and that an undecodable burst is refused. Edge case E.94.
**Done when:** `pnpm --filter terminal exec vitest run src/lib/scanner.test.ts` exits zero proving that a burst whose `key` values are Arabic letters and whose `code` values spell a Code 128 value decodes to that value, that typing outside a burst is unchanged, and that an undecodable burst is refused. The device half — a real scanner reading alphanumeric and digit-only codes under both Arabic (101) and English (US) — is milestone 5.8's (5.8.3), because no device is bought before it ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b).

### 1.11.6c — The keyboard-wedge emulator and the scanner matrix
**Scheduled in:** after 1.11.6b and 1.7.11
**Files:** `apps/terminal/src/test/wedge-emulator.ts` (new) · `apps/terminal/src/lib/scanner.conformance.test.ts` (new) · `crates/pos-hardware/profiles/scanner/` (read, not written) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
1.11.6 and 1.11.6b are proven with bursts built by hand. This step builds them from data, so every scanner a merchant might own is exercised on every commit without one on the desk ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b). A generator turns a scanner profile — prefix and suffix, inter-key timing, keyboard country, ALT or keypad emulation — and a host keyboard layout into the `KeyboardEvent` stream the webview would receive, `key` and `code` both, at the profile's timing under fake time. The suite then runs every scanner profile under the US and Arabic (101) host layouts: every code in the Phase-1 fixture, plus alphanumeric Code 128 and QR values, must reach the same `ScanLookup` however the device was configured. A profile whose timing falls outside the < 30 ms burst heuristic is recorded as not supported in wedge mode — the suite proves where the heuristic's boundary is rather than pretending a slow scanner works.
**Tests:** `every_scanner_profile_decodes_under_every_host_layout` · `alt_keypad_emulation_decodes_to_the_same_value` · `a_scanner_too_slow_for_the_burst_heuristic_is_reported_not_misrouted`
**Done when:** `pnpm --filter terminal exec vitest run src/lib/scanner.conformance.test.ts` exits zero for every profile under `profiles/scanner/`, under both host layouts.

### 1.11.6d — HID-POS scanners, which no keyboard layout can garble
**Scheduled in:** Phase 1B, after 1.11.6c and 1.7.11
**Files:** `crates/pos-hardware/src/scanner.rs` (new — `BarcodeSource`, `ScanEvent` and `SimulatedScanner`) · `crates/pos-hardware/src/lib.rs` (`pub mod scanner;`) · `apps/terminal/src-tauri/src/scanner.rs` (new — the Windows POS API source behind `cfg(windows)`, emitting each scan as one IPC event) · `apps/terminal/src/lib/scanner.ts` (one consumer for wedge bursts and HID-POS events) · `crates/pos-hardware/profiles/scanner/` (the `hid-pos` interface) · [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §5 · [`ref/ipc-contract.md`](ref/ipc-contract.md) (the scan event) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
**1.11.6b repairs what a layout did to a key press; this step removes the key press.** Windows ships an in-box class driver for USB scanners in HID POS mode — the USB.org HID POS Scanner Usage Table (0x8C) — and hands each scan to `Windows.Devices.PointOfService.BarcodeScanner` as data, with its symbology, never as keystrokes ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §5). A scan that never becomes a key press cannot be re-mapped by Arabic (101), so on a scanner that supports the mode, case E.94 cannot happen at all. Microsoft's compatibility list names HID-POS models from Honeywell, Zebra, HP and Code, the seed's Zebra DS2208 among them.
**One trait, two sources, one routing rule.** `BarcodeSource` is the trait `lib.rs` has reserved since Phase 0. The register's Rust shell claims a HID-POS scanner through the POS API and emits each scan as one event; a wedge scanner keeps 1.11.6's burst path. Both reach the same `ScanLookup` through the same routing — `scan_routes_while_search_focused` holds for both — and a profile's `interface` decides which source a register binds. `SimulatedScanner` implements the trait from a scanner profile and a script of codes, so every test runs with no scanner, on every platform; the Windows source compiles in the `cross-platform` job, and its behaviour on a device is 5.8.3's.
**Tests:** `a_hid_pos_scan_reaches_the_same_lookup_as_a_wedge_scan` · `a_hid_pos_scan_ignores_the_host_keyboard_layout` · `the_bound_profile_chooses_the_scan_source`
**Done when:** `cargo nextest run -p pos-hardware scanner::` and `pnpm --filter terminal exec vitest run src/lib/scanner.test.ts` exit zero, proving that a simulated HID-POS scan and a wedge burst of the same Code 128 value reach the same `ScanLookup` under both host layouts, and that the bound profile selects the source. The device half — a scanner switched to USB HID POS mode and claimed through the POS API on a Windows register — is milestone 5.8's (5.8.3).

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
**Files:** `apps/terminal/src/lib/keymap.ts`, `apps/terminal/src/lib/keymap.test.ts` (new) · [`README.md`](README.md) (implementation frontier) · [`ref/test-catalog.md`](ref/test-catalog.md) and [`02-development-workflow.md`](02-development-workflow.md) (both list this test as still owed) · this file (this microstep's `Files:` line)
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
**Files:** `apps/terminal/tests/e2e/performance/scan-latency.e2e.ts` (new), `apps/terminal/wdio.conf.ts` (new), `apps/terminal/package.json`, `pnpm-lock.yaml` — the baseline, `benchmarks/baselines/scan-to-line.json`, is 5.8.2's

```ts
export async function measureScanToVisibleLine(code: string): Promise<number>;
```

Measure simulated scanner input through the packaged Tauri application, Rust lookup, cart repricing and the visible React line over the seeded catalogue using WebdriverIO and `tauri-driver`. Use conventions §7's sample and variance policy. The 100 ms limit is the reference register's and is proven at milestone 5.8 (5.8.2); before then the scenario runs through the packaged app on the development machine, and any timing it prints is indicative only (conventions §7.2). Measuring only the parser or browser build hides the IPC, webview and rendering delay the cashier experiences. Phase 2 expands this minimal driver into the full packaged-app smoke suite at 2.9.5.
**Tests:** `scan_latency_scenario_reaches_a_visible_priced_line` (deterministic scenario validation; timing belongs only to the benchmark gate)
**Done when:** `pnpm --filter terminal exec wdio run ./wdio.conf.ts` exits zero, proving the scenario reaches a visible priced line and records at least 50 post-warm-up scans; `just bench-gate scan-to-line`, with its p99 below `100 ms` on the reference register, is 5.8.2's.

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

### 1.11.14b — Register-screen profiles
**Scheduled in:** after 1.11.14 and 1.7.11
**Files:** `apps/terminal/playwright.config.ts` (one project per register-screen profile) · `apps/terminal/tests/visual/screen-profiles.spec.ts` (new) · `crates/pos-hardware/profiles/register_screen/` (read, not written) · [`README.md`](README.md) (implementation frontier) · this file (this microstep's `Files:`, `Tests:` and `Done when:` lines)
1.11.14 commits its baselines at the declared minimum, `1024×640`. The registers this product will meet are not that window: the reference's seed holds a 15″ `1024×768` capacitive panel and a 15.6″ `1920×1080` resistive one, and Windows may scale either ([`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b.2). This step renders the sale, tender and approval screens at every register-screen profile and every display scale it declares, with touch emulation where the profile's pointer is touch, and asserts what must hold on every panel instead of committing another image per panel: nothing overflows the viewport or truncates a total, every interactive target meets [`ref/ui-spec.md`](ref/ui-spec.md)'s 48 px floor on a touch profile, and no action is reachable only by hover. Image baselines stay where 1.11.14 put them; one more image per panel would be one more review surface for the same layout.
**Tests:** `every_screen_profile_keeps_the_sale_screen_inside_its_viewport` · `touch_profiles_meet_the_hit_target_floor` · `no_action_is_reachable_only_by_hover_on_a_touch_profile`
**Done when:** `pnpm --filter terminal test:visual` exits zero with one project per register-screen profile, at every scale each declares.

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
**Files:** `crates/pos-db/src/seed/minimarket.rs` (new), `crates/pos-db/src/seed/data/*.csv`, `crates/pos-db/tests/minimarket_seed.rs` (new), `justfile`, `crates/pos-domain/fuzz/corpus/parse_scan/`
~200 products with real Arabic names across the merchant's approved tax pack: exempt and standard items, one item for every imported reduced band, loose vegetables sold by weight, a deli item with a price-embedded barcode, a sealed-pack tobacco line (`min_age = 18`, `regulated_kind = 'tobacco'`), and a price-controlled staple (`max_price_minor`). `opening_stock.csv` goes through 1.10.5's ledger loader, never directly into `stock_cache`. Its plain, weight-embedded and price-embedded codes also join 1.2.8's `parse_scan` corpus, each named for what it must parse as, because that corpus was seeded before this fixture existed.
**Tests:** `seed_uses_every_enabled_tax_band` · `seed_tobacco_is_a_sealed_age_restricted_pack` · `seed_opening_stock_posts_ledger_events`
**Done when:** `just seed` produces positive on-hand through ledger facts for every stock item and `cargo nextest run -p pos-db --test minimarket_seed` passes; every demo and screenshot uses this fixture.

### 1.12.2 — Hand-checked tax report fixture
**Files:** `crates/pos-domain/tests/tax_report.rs`
A scripted trading day over the fixture: mixed rates, a weighed line, a basket discount, a refund, and a cash-rounded tender. **Total it by hand once**, commit the expected figures, and let CI defend them forever.
**Tests:** `tax_report_matches_hand_check_fixture`
**Done when:** `cargo nextest run -p pos-domain --test tax_report` exits zero against the committed hand-calculated net, tax and gross values for every enabled component.
> This is the only test that proves the *product* is right rather than the *code*. Do the arithmetic on paper.

### 1.12.3 — Benchmark suite in CI
**Files:** `.github/workflows/ci.yml`, `scripts/bench-gate.py` (the relative comparison), `scripts/tests/benchmark_ci_contract_test.py` (new)
**Until milestone 5.8 the job this step adds is relative, and says so.** A hosted job builds a pull request's base and head, runs each `criterion` budget — search, price-cart, pin-verify — for both, interleaved on the one runner, and fails when the head is slower by §7.1's rule: more than 20% **and** more than three median absolute deviations. Comparing two revisions inside one job removes the runner-to-runner variance that makes a hosted runner useless for an absolute limit. It reads no baseline, writes none, never consults the reference profile, and prints that it claims no budget ([`01-conventions.md`](01-conventions.md) §7.2). It becomes a required check only after a month without a false alarm, because a gate that cries wolf is disabled within one ([`02-development-workflow.md`](02-development-workflow.md) §11).
**The live job is defined here and switched on at 5.8.2**, when there is a runner to pin it to: `just bench-gate` on `runs-on: [self-hosted, reference-register]`, on `push`, `schedule` and `workflow_dispatch` only and never on `pull_request`, with no secrets, behind a repository variable 5.8.2 sets so that no job waits for a runner that does not exist yet. `cargo bench` printing a slower number and exiting zero is not evidence.
**Tests:** `hosted_benchmark_job_compares_two_revisions_and_publishes_nothing` · `benchmark_ci_live_job_uses_reference_runner` · `benchmark_ci_live_job_never_runs_on_a_pull_request`
**Done when:** `python3 scripts/tests/benchmark_ci_contract_test.py` exits zero, proving the hosted job compares two revisions on one runner without reading or writing a baseline, and that the live job is pinned to the self-hosted reference runner and absent from every `pull_request` trigger; and the relative job has run on at least one pull request.

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
cargo nextest run -p pos-hardware --features emulator --test conformance   # every printer profile, emulated
```

**`just bench-gate` is deliberately not in that list.** It refuses until milestone 5.8 fills the reference-register records (5.8.2); what this gate asks instead is a green relative comparison from 1.12.3 on the merge that closes the phase, and it claims no budget ([`01-conventions.md`](01-conventions.md) §7.2).

The Arabic cashier guide and its independent native-speaker review record must also pass 1.11.15; deferring the manual until launch would make the first non-author operator the person who discovers the instructions are incomplete.

And, by demonstration — do this literally, with the network cable unplugged:

1. **Open the packaged app offline.** Time ten clean launches and record the median as indicative — the 3-second budget is the reference register's, proven at 5.8.2. Sign in with a PIN and open a shift with an opening float.
2. **Load opening stock, then sell a basket** containing: a scanned multipack, an item found by Arabic search, a weighed item via price-embedded barcode, an exempt item, every enabled reduced band, a 16% item, and the sealed-pack age-restricted item after confirmation. Scan an unknown code and use the cashier's department-sale path without waiting for `product.edit`.
3. **Apply a basket discount.** Confirm it prorates to the lines to the fil.
4. **Pay cash** with overtender. Confirm change, and confirm cash rounding produced an explicit adjustment line.
5. **The receipt prints in Arabic** — through 1.7.10's driver to the emulated network printer (1.7.12), whose image of the bytes it received is read by the native reader — correctly shaped and right-to-left, with a tax summary showing exempt and standard as separate rows. The same receipt on paper is 5.8.4's.
6. **Park a sale, serve another customer, resume it.** Nothing is lost.
7. **Run both power-loss drills on a virtual machine, and know what that proves.** First hard-stop it mid-finalize — a power-off, not a shutdown — restart, and prove exactly one completed sale exists, with one stock event per stock-bearing line, one ready `sync_commit` whose manifest contains every constituent fact, one original receipt artifact and queued print job, and no duplicate financial effect. Then complete and print a second sale, hard-stop it again without an OS shutdown, reboot, and prove its sale, manifest, artifact and audit entry survived. A hard stop discards the guest's page cache exactly as a power cut discards a register's, so it catches a commit acknowledged before it was flushed — which `pkill` cannot. It cannot catch storage that acknowledges a flush it never made, so E.1b, the cut of physical power, is repeated on the reference register at 5.8.4.
8. **Switch to a principal with `reports.all` and open the sales-side tax reconciliation** for the shift's business date. Check it against the receipts by hand; it reconciles to the fil and does not call itself a return. Then close the Phase-1 shift with a bound approval: an append-only close event exists, no expected-cash figure or count is exposed, and the next shift can open on the register.
9. **Verify the audit chain.** Intact. Take and verify an online snapshot with 1.8.6; only on that consistent copy, drop `audit_log_no_update`, alter one row, and run `verify-audit` against it. It reports `Broken { at_seq }` at that row. The original still refuses the same `UPDATE`.
10. **Create and verify the off-machine backup and wrapped-key envelope on a removable volume, then destroy the database, local backup destination and credential-store entry.** On a clean machine — a fresh virtual machine qualifies — restore that removable-volume copy using only the printed recovery code. Every unsynced sale, fact manifest, delivery row and audit anchor is still there; `POS_DB_KEY` is not used.

**If any of the ten fails, Phase 1 is not done.** Number 7 and number 10 are the ones most likely to be skipped and the two most likely to matter on a merchant's worst day.

**Not claimed at this gate:** that any device works. Every demonstration above that touches one ran against the emulators of [`ref/hardware-and-receipts.md`](ref/hardware-and-receipts.md) §6b, and repeating 1, 5 and 7 on the devices is milestone 5.8's (5.8.4). The exit line at the top of this file describes what the software can do; no store does it before the pre-pilot gate.

→ **Next:** [`phase-2-money-grade.md`](phase-2-money-grade.md)
