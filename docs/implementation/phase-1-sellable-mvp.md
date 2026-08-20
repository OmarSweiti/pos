# Phase 1 — Sellable MVP

> **Exit:** a real Jordanian minimarket could sell for cash, all day, fully offline, in Arabic, with correct GST and a printed receipt.

**Effort:** 8–12 weeks for a solo developer learning Rust on the job. The first three groups are slow because they are new language and new discipline; groups 1.8 onward accelerate sharply.
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
1.9 sequences & business date ────────────────────────────────────────────┤
1.10 stock ledger ────────────────────────────────────────────────────────┤
                                                                          ▼
                                                          1.8 persistence & finalize
                                                                          │
                                                          1.11 terminal UI (RTL, i18n)
                                                                          │
                                                          1.12 seed, benchmarks, gate
```

1.6, 1.9 and 1.10 are independent of the cart work and are good places to go when you are stuck on the state machine.

---

## Group 1.1 — Domain foundations

*Gap G-11. Nothing else can start until `Money` knows what currency it is.*

### 1.1.1 — `Currency`
**Files:** `crates/pos-domain/src/money.rs`
Add the `Currency` type from [`ref/domain-api.md`](ref/domain-api.md) §1.1, with `JOD` (exponent 3) and `USD` (exponent 2) constants. `Copy`, four bytes, interned `code()` returning `&'static str` with no allocation.
**Tests:** `jod_exponent_is_three` · `unknown_currency_code_errors`
**Verify:** `cargo nextest run -p pos-domain money::`
**Done when:** `Currency::JOD.minor_per_major() == 1000`.

### 1.1.2 — `Money` carries `Currency`
**Files:** `crates/pos-domain/src/money.rs`, `apps/terminal/src-tauri/src/lib.rs` (fix `split_tender`)
Thread `Currency` through every constructor and operation. `checked_add`/`checked_sub` return `CurrencyMismatch` rather than coercing. **Do not rewrite `split_evenly`** — its largest-remainder implementation and property test are correct; only add the currency field.
Add `mul_qty`, `mul_percent`, `split_proportional`, `round_to_step`, `to_decimal`, `from_decimal`, `format`, `parse`.
**Tests:** `prop_currency_mismatch_never_silently_coerces` · `prop_format_parse_roundtrip` · `prop_mul_qty_whole_units_is_repeated_add` · `prop_split_proportional_preserves_total`
**Done when:** `Money::from_minor(1250, JOD).format(3) == "1.250"` and `format(2) == "1.25"`.

> `format(2)` on a 3-exponent currency **truncates for display only**. Storage is always fils. Display precision is a store setting (master plan B.5) and this is the one place it applies.

### 1.1.3 — `Qty` in milli-units
**Files:** `crates/pos-domain/src/money.rs`
Per §1.3 of the API reference. `Qty::ONE == 1000`.
**Tests:** `prop_qty_add_sub_roundtrip` · `weighed_formats_three_decimals` · `whole_units_format_without_decimals`
**Done when:** `Qty::from_milli(347).format(true) == "0.347"` and `Qty::ONE.format(false) == "1"`.

### 1.1.4 — `Percent` in parts-per-million
**Files:** `crates/pos-domain/src/money.rs`
**Tests:** `sixteen_percent_is_160000_ppm` · `prop_percent_decimal_roundtrip`

### 1.1.5 — Money property suite
**Files:** `crates/pos-domain/src/money.rs`
All eight properties from API reference §1.6. This is the layer that finds what you did not imagine; do not shortcut it.
**Verify:** `cargo nextest run -p pos-domain money::prop_`

### 1.1.6 — `RoundingRule` and `RoundingDirection`
**Files:** `crates/pos-domain/src/money.rs`
Default `HalfAwayFromZero`, not banker's — see [`ref/tax-jordan.md`](ref/tax-jordan.md) §4 for why.
**Tests:** `half_away_from_zero_rounds_1_5_to_2_and_neg_1_5_to_neg_2` · `half_even_rounds_1_5_and_2_5_both_to_2`

### 1.1.7 — Migration `0002`, part one: the qty fix
*Gap G-12. **Must land before any sale row exists.***
**Files:** `crates/pos-db/migrations/0002_catalog_depth.sql`, `crates/pos-db/src/lib.rs` (`MIGRATIONS` array)
The `sale_line` rebuild from [`ref/schema.md`](ref/schema.md) §0002, plus `sale_line_tax` and `sale_line_discount`.
**Tests:** `crates/pos-db/tests/migrations.rs::migration_0002_converts_qty_to_milli` — seed a `0001`-shaped row, migrate, assert `qty_milli == qty * 1000`.
**Done when:** `PRAGMA user_version` is 2 and the seeded row survives with `qty_milli = 2000`.

### 1.1.8 — Typed ids and the `Clock` / `IdSource` ports
**Files:** `crates/pos-domain/src/ids.rs` (new), `crates/pos-domain/src/lib.rs`
The `typed_id!` macro and fourteen id types from API reference §2, plus the two traits and their deterministic doubles (`SeqIdSource`, `FixedClock`).
**Tests:** `typed_ids_do_not_interconvert` (a compile-fail test via `trybuild`) · `seq_id_source_is_reproducible`
**Done when:** `fn f(s: SaleId, l: SaleLineId)` cannot be called with the arguments swapped, proven by a `trybuild` fixture.

### 1.1.9 — `Timestamp`, `BusinessDate`, `DayBoundary`
*Gap G-4.*
**Files:** `crates/pos-domain/src/time.rs` (new)
Per API reference §3, including `business_date_of` and `MonotonicClock`.
**Tests:** `shift_opened_at_0030_belongs_to_previous_day` · `prop_business_date_stable_across_shift` · `prop_cutover_boundary_never_skips_a_day` · `prop_monotonic_clock_never_decreases` · `clock_jump_back_reports_anomaly` (E.6)
**Done when:** a shift opened 2026-08-21T00:30 local, cutover 04:00, has business date 2026-08-20.

---

## Group 1.2 — Catalog, barcodes, search

### 1.2.1 — Migration `0002`, part two: org / store / register / taxonomy
**Files:** `crates/pos-db/migrations/0002_catalog_depth.sql`
The `org`, `store`, `register`, `category`, `tax_category`, `tax_rate`, `barcode`, `setting` tables and the `product` `ALTER`s from [`ref/schema.md`](ref/schema.md).
**Tests:** `migration_0002_creates_all_tables` · `barcode_live_uniqueness_allows_reissue_after_tombstone`
**Done when:** a tombstoned barcode code can be reassigned to a different product; two live rows with the same code cannot exist.

### 1.2.2 — `Product` and `UnitOfMeasure` in the domain
**Files:** `crates/pos-domain/src/catalog.rs` (new)
Per API reference §4. Include `min_age`, `max_price_minor`, `is_service` now — they cost one field each and Phase 2/4 needs them.

### 1.2.3 — `ProductRepository`
**Files:** `crates/pos-db/src/repo/product.rs` (new), `crates/pos-db/src/repo/mod.rs` (new)
```rust
impl<'c> ProductRepository<'c> {
    pub fn by_id(&self, id: ProductId) -> Result<Option<Product>, DbError>;
    pub fn by_barcode(&self, code: &str) -> Result<Option<(Product, BarcodeKind)>, DbError>;
    pub fn by_plu(&self, code: &str) -> Result<Option<Product>, DbError>;
    pub fn search(&self, q: &str, limit: u32) -> Result<Vec<ProductHit>, DbError>;
    pub fn upsert(&self, tx: &Transaction, p: &Product) -> Result<(), DbError>;
}
```
Repository law (conventions §3): returns owned domain types, never a `rusqlite::Row`, never computes a total. Writes take an explicit `&Transaction`.
**Tests:** `by_barcode_returns_newest_active_on_collision` (E.36) · `by_id_ignores_tombstones`
**Done when:** two live products sharing a barcode is impossible; a tombstoned one is invisible to lookup but still resolvable by id for history.

### 1.2.4 — Price-embedded barcode parser
**Files:** `crates/pos-domain/src/catalog.rs`
`parse_scan`, `ean13_checksum_ok`, `EmbeddedBarcodeRule`, `ScanError` per API reference §4.1.
**Tests:** `prop_ean13_checksum_matches_reference` · `prop_embedded_parse_roundtrip` · `prop_corrupt_digit_never_parses_clean` · `weight_embedded_2xxxxxwwwww_parses` · `price_embedded_parses`
**Done when (E.40):** flipping any single digit of a valid embedded barcode either fails the checksum or produces a *different* item code — it never silently produces a wrong price for the right item.

### 1.2.5 — Migration `0006`: FTS5, PLU, tiles, scan rules
**Files:** `crates/pos-db/migrations/0006_search_and_seed.sql`
Per [`ref/schema.md`](ref/schema.md) §0006. Tokeniser `unicode61 remove_diacritics 2` so Arabic tashkeel folds.
**Tests:** `fts_matches_arabic_with_and_without_diacritics` · `fts_matches_english_and_sku` · `fts_survives_product_update` · `fts_row_removed_on_tombstone`

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
**Files:** `crates/pos-db/benches/search.rs` (new), `crates/pos-db/Cargo.toml` (`criterion`)
Seed 50 000 products with Arabic names, benchmark `search()` at 1, 2, 3 and 5 characters.
**Done when:** p99 < 50 ms and CI fails on regression beyond 20%.

---

## Group 1.3 — Tax engine

*Everything in [`ref/tax-jordan.md`](ref/tax-jordan.md). Right before the first real sale, or never.*

### 1.3.1 — Tax types
**Files:** `crates/pos-domain/src/tax.rs` (new)
`TaxTreatment`, `TaxComponent`, `PriceMode`, `StoreTaxProfile`, `LineTax`, `ComponentTax`, `TaxSummaryRow`, `TaxRateRule`, `TaxError` per API reference §5.

### 1.3.2 — `resolve_components`
**Files:** `crates/pos-domain/src/tax.rs`
`valid_from` inclusive, `valid_to` exclusive; scoped rules override unscoped; overlap and absence are both errors, never a guessed 16%.
**Tests:** `prop_rate_resolution_is_deterministic_at_boundaries` · `overlapping_rules_error` · `no_rule_in_effect_errors` · `scoped_rule_overrides_unscoped`

### 1.3.3 — `compute_line_tax`, exclusive mode
**Files:** `crates/pos-domain/src/tax.rs`
Simpler direction first: `tax = net × r`, one rounding.
**Tests:** `exclusive_16pct_adds_exactly`

### 1.3.4 — `compute_line_tax`, inclusive mode
**Files:** `crates/pos-domain/src/tax.rs`
`net = gross / (1+r)`, then **`tax = gross − net` as a residual** — never rounded independently, or a receipt can fail to add up.
**Tests:** `inclusive_16pct_extracts_exactly` (the 1250 → 1078 + 172 worked example) · `prop_inclusive_net_plus_tax_equals_gross` · `prop_tax_never_exceeds_gross`
**Done when:** for every rate in {0, 1, 2, 4, 5, 10, 16}% and every gross 1…1 000 000 fils, `net + tax == gross` exactly.

### 1.3.5 — Multiple components per line
**Files:** `crates/pos-domain/src/tax.rs`
GST + a hypothetical special tax on one line. Ship it now even though v1 sells no excise goods — retrofitting a second component through a live schema is a migration of every sale.
**Tests:** `prop_multi_component_line_sums_correctly`

### 1.3.6 — `summarize_tax`
**Files:** `crates/pos-domain/src/tax.rs`
Grouped by `(component, treatment, rate)`. The **exact sum** of line taxes, never re-derived.
**Tests:** `prop_line_tax_sum_equals_receipt_tax` · `prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting`
**Done when:** exempt and zero-rated items on one receipt produce two summary rows, not one.

### 1.3.7 — Seed the Jordanian tax data
**Files:** `crates/pos-db/migrations/0002_catalog_depth.sql` (seed block)
`STD16` 16%, `RED04` 4%, `ZERO` 0%, `EXEMPT`. `valid_from` at go-live, `valid_to` NULL.
**Done when:** `resolve_components(STD16, …, now)` returns one GST component at 160 000 ppm.
> Which product sits in which category is the merchant's accountant's call, not this seed's. Flagged as merchant decision #10.

### 1.3.8 — `unregistered` profile short-circuit
**Files:** `crates/pos-domain/src/tax.rs`
**Tests:** `prop_unregistered_profile_yields_no_tax`
**Done when:** a store below the registration threshold produces receipts with no tax lines and a zero tax summary — legally, not as an error state.

---

## Group 1.4 — The cart state machine

*Blueprint §8 as a Rust enum. Illegal transitions do not compile.*

### 1.4.1 — The `Sale` enum and `Cart` / `CartLine`
**Files:** `crates/pos-domain/src/cart.rs` (new)
Per API reference §6. `is_training` on the cart from the first commit — it is checked *everywhere*, including the fiscal queue, and adding it later means auditing every call site.

### 1.4.2 — `CartError`
**Files:** `crates/pos-domain/src/cart.rs`
Every variant from API reference §6.3. Exhaustive and data-carrying — the UI renders from `code`, not by parsing a message.

### 1.4.3 — `add_line`, `set_qty`, `void_line`
**Files:** `crates/pos-domain/src/cart.rs`
`add_line` copies `name_snapshot` and `unit_price` onto the line (I-5). It refuses inactive products for *adding* while leaving refunds unaffected (E.38), and refuses without age confirmation when `min_age` is set (E.69).
**Tests:** `add_line_snapshots_name_and_price` · `inactive_product_cannot_be_added_but_can_be_refunded` · `age_restricted_line_requires_confirmation` · `set_qty_zero_is_rejected`

### 1.4.4 — `park` / `resume`
**Files:** `crates/pos-domain/src/cart.rs`
**Tests:** `prop_park_resume_roundtrip_is_identity` (E.3)

### 1.4.5 — Discounts
**Files:** `crates/pos-domain/src/pricing.rs` (new)
`LineDiscount`, `BasketDiscount`, `DiscountRequest`, `DiscountAttribution`. Manual discounts are permission-scoped with a per-role percentage cap.
**Tests:** `prop_discount_never_makes_a_line_negative` (E.19) · `discount_above_role_cap_is_denied`

### 1.4.6 — Basket-discount proration
**Files:** `crates/pos-domain/src/pricing.rs`
`Money::split_proportional` by line value, producing one `DiscountAttribution` per line.
**Tests:** `prop_basket_discount_prorates_to_the_fil` · `prop_proration_never_creates_unexpressible_percentage`
> The second property is the one that keeps JoFotara happy — see correction **C-2** in [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §4.3. A line too small to carry its share receives zero, and the remainder lands on larger lines.

### 1.4.7 — Price override
**Files:** `crates/pos-domain/src/pricing.rs`
Requires `Authorized<{cap::PRICE_OVERRIDE}>`, a reason code, and respects a floor (cost, or cost + x%) and a ceiling.
**Reasons ship with a dedicated `displayed_price` variant** — Jordan's ministry inspects price display, and "the shelf tag says 0.99" must be a one-tap, always-audited action that also feeds the label-reprint worklist (J.3, E.70).
**Tests:** `override_below_floor_is_denied` · `override_above_max_price_is_hard_blocked` (E.71) · `displayed_price_override_queues_a_label_reprint`

### 1.4.8 — Tendering transitions
**Files:** `crates/pos-domain/src/cart.rs`
`begin_tender`, `back_to_building` (only with zero tenders collected), `add_tender`, `remove_tender`, `begin_finalize`, `complete`, `void_sale`.
**Tests:** `back_to_building_denied_after_first_tender` · `complete_requires_settled` · `prop_no_operation_mutates_a_complete_sale` (I-4)

### 1.4.9 — `price_cart`
**Files:** `crates/pos-domain/src/cart.rs`
The one function that turns a cart into money — the sole source of every number on the receipt *and* in the fiscal document.
**Tests:** `prop_total_equals_lines_minus_discounts_plus_tax` · `prop_price_cart_is_deterministic` · `prop_zero_total_cart_is_valid` (E.18)
**Bench:** `crates/pos-domain/benches/price_cart.rs` — 200 lines, p99 < 16 ms (G-9).

### 1.4.10 — `AuditIntent` emission
**Files:** `crates/pos-domain/src/cart.rs`, `crates/pos-domain/src/audit.rs`
Every money-reversing transition returns `(NewState, AuditIntent)`. A pure function cannot write a row; it returns the intent and the shell persists it in the same transaction.
**Tests:** `every_privileged_transition_returns_an_audit_intent` — an exhaustive match over the transition list, so adding one without an intent fails to compile.

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
`compute_cash_rounding`, applied only when the **final** tender is cash and only to the remaining cash amount.
**Tests:** `prop_cash_rounding_only_on_final_cash_tender` (E.14) · `prop_rounding_adjustment_keeps_total_exact` · `card_charged_exact_unrounded_total`
**Done when:** a 1.247 JOD sale paid half by card, half by cash charges the card 0.624 and takes 0.630 in cash, with a +3 fils adjustment line, totalling exactly.

### 1.5.4 — Denomination helper
**Files:** `crates/pos-domain/src/tender.rs`
JOD denominations (50, 20, 10, 5, 1 dinar; 500, 250, 100, 50, 25, 10 fils) for the numpad quick-keys and the shift-close count grid.
**Tests:** `denominations_are_descending_and_complete`

---

## Group 1.6 — Users, permissions, audit

*Gaps G-6 and G-7. Independent of the cart work.*

### 1.6.1 — Migration `0003`
**Files:** `crates/pos-db/migrations/0003_people_and_audit.sql`
Per [`ref/schema.md`](ref/schema.md) §0003. Note `app_user`, not `user` — reserved in Postgres.

### 1.6.2 — Argon2id PINs
**Files:** `crates/pos-db/src/auth.rs` (new), `crates/pos-db/Cargo.toml` (`argon2`)
```rust
pub fn hash_pin(pin: &str) -> Result<String, AuthError>;              // PHC string
pub fn verify_pin(pin: &str, hash: &str) -> Result<bool, AuthError>;  // constant-time
```
Parameters tuned so verification takes ~250 ms on register-class hardware: fast enough that a cashier does not notice, slow enough that a stolen database is not a 4-digit brute force in an afternoon.
**Tests:** `hash_verify_roundtrip` · `wrong_pin_rejected` · `hash_is_salted_and_differs_per_call` · `verify_takes_at_least_100ms`
**Done when:** no PIN, and no hash, ever appears in a log line — asserted by 1.6.8.

### 1.6.3 — Capabilities and the default role matrix
**Files:** `crates/pos-domain/src/permissions.rs` (new), `0003` seed block
The `cap` module from API reference §8, and the default matrix from master plan C.10 seeded as `role` + `role_capability` rows.
**Tests:** `default_matrix_matches_master_plan_c10` — a table-driven test asserting all nineteen capabilities against all four roles. The master plan's table is the fixture.

### 1.6.4 — `Authorized<C>` and `authorize`
**Files:** `crates/pos-domain/src/permissions.rs`
The proof-carrying token. Constructing one is the *only* way to obtain a `&Authorized<C>`, and privileged domain functions require one — so the check cannot be forgotten, because the function cannot be called without it.
**Tests:** `cashier_cannot_void_a_sale` · `manager_self_approval_denied_when_policy_bans_it` (E.52) · `deactivated_user_denied` · `offline_auth_window_expires` (E.55)

### 1.6.5 — Audit hash chain
**Files:** `crates/pos-domain/src/audit.rs` (new)
`canonical_bytes` (sorted keys, no whitespace, UTF-8), `chain_hash` (BLAKE3 of `prev ‖ bytes`), `verify_chain`.
**Tests:** `golden_canonical_bytes_are_stable` · `prop_chain_detects_any_single_entry_mutation` · `prop_chain_detects_deletion` · `prop_chain_detects_reordering`
**Done when:** mutating any byte of any historical entry makes `verify_chain` return `Broken { at_seq }` pointing at it.

### 1.6.6 — `AuditRepository`
**Files:** `crates/pos-db/src/repo/audit.rs` (new)
Append-only. Reads the previous hash and writes the new row **inside the caller's transaction**. There is no update method and no delete method — not private ones, none.
**Tests:** `chain_survives_process_restart` · `concurrent_appends_serialize` · `verify_chain_over_1000_entries`

### 1.6.7 — Capability exhaustiveness test
**Files:** `apps/terminal/src-tauri/src/ipc/registry.rs` (new), `apps/terminal/src-tauri/tests/ipc_contract.rs` (new)
Every IPC command registers `(name, required_capability, audited: bool)`. The test walks `tauri::generate_handler!`'s list and fails on any command absent from the registry.
**Tests:** `ipc_commands_all_declare_a_capability`
**Done when:** adding a command without a registry entry breaks CI. Verify by adding one, watching it fail, and reverting.

### 1.6.8 — PII scrubbing in the log layer
*Gap G-8.*
**Files:** `apps/terminal/src-tauri/src/telemetry.rs` (new)
A `tracing` layer redacting known-sensitive field names (`pin`, `pin_hash`, `pan`, `cvv`, `track`, `phone`, `email`, `customer_name`, `secret_key`, `client_id`, `db_key`) at any nesting depth.
**Tests:** `scrubber_redacts_every_known_pii_field` · `scrubber_redacts_nested_json` · `no_pii_in_a_full_sale_trace` — run a complete sale with a customer attached, capture every log line, assert none contains the fixture's phone or name.
**Done when:** the PDPL position ("no PII in logs") is a passing test rather than an intention.

---

## Group 1.7 — Receipts

*The Arabic problem. Get it right once and every later document format inherits it.*

### 1.7.1 — `ReceiptModel`
**Files:** `crates/pos-domain/src/receipt.rs` (new)
Per API reference §13. The ESC/POS rasteriser, the PDF renderer, and the email renderer all consume this — so an emailed receipt can never disagree with the printed one.

### 1.7.2 — Font decision and embedding
*Gap G-5.*
**Files:** `assets/fonts/` (new), `crates/pos-hardware/Cargo.toml`
One family covering Arabic and Latin, embeddable, licence-clear, shipped with the app — **no network font**. The same file feeds the UI and the receipt rasteriser so the receipt looks like the screen. Candidates: Noto Sans Arabic, IBM Plex Sans Arabic, Cairo.
**Done when:** the font is in the repository with its licence, and both the UI and the rasteriser load it from the same path.

### 1.7.3 — The raster pipeline
**Files:** `crates/pos-hardware/src/render/mod.rs` (new), `layout.rs`, `raster.rs`
```
ReceiptModel → layout engine (boxes, RTL runs, columns)
             → cosmic-text shaping (rustybuzz under it: Arabic joining + bidi)
             → tiny-skia 1-bit bitmap at printer width (576 px @ 80 mm, 384 @ 58 mm)
             → GS v 0 raster bytes
```
Do not fight printer codepages. Windows-1256 text mode does not shape Arabic letters or reorder RTL runs; the field consensus is rasterisation and it is also the only way bilingual mixing looks correct.
**Tests:** `layout_wraps_long_arabic_names` · `layout_columns_align_in_rtl` · `raster_width_matches_profile`
**Done when:** an Arabic product name renders with correct letter joining and RTL order, verified by eye once and by golden file forever.

### 1.7.4 — ESC/POS emitter
**Files:** `crates/pos-hardware/src/escpos.rs` (new)
`ESC @` init, `GS v 0` raster, `GS V` cut, `ESC p` drawer kick. Two width profiles, 80 mm and 58 mm (E.49).
**Tests:** `escpos_init_cut_drawer_bytes_are_exact`

### 1.7.5 — Golden receipts
**Files:** `crates/pos-hardware/tests/golden/` (new)
Six fixtures, byte-diffed in CI: Arabic 80 mm · Arabic 58 mm · bilingual 80 mm · multi-rate tax summary · duplicate watermark · training watermark.
**Tests:** `golden_receipts_are_byte_stable`
**Done when:** an unintended change to layout, font metrics, or ESC/POS output shows up as a diff in a pull request.
> Regenerating a golden is deliberate: `UPDATE_GOLDEN=1 cargo test`, then **look at the diff** — ideally by printing it — before committing.

### 1.7.6 — Printer status before finalize
**Files:** `crates/pos-hardware/src/lib.rs`
`status()` is polled **at Pay, before money is taken** (master plan C.15). Paper-out warns then, not after the customer has paid.
**Tests:** `paper_out_warns_before_tender_not_after`

### 1.7.7 — Print retry queue and unprinted flag
**Files:** `apps/terminal/src-tauri/src/print_queue.rs` (new)
A print failure **after** finalize never un-finalizes the sale. It sets `sale.receipt_printed_at = NULL`, logs an incident, and offers one-tap reprint marked DUPLICATE (E.46).
**Tests:** `print_failure_after_finalize_leaves_sale_complete` · `reprint_marks_duplicate` · `queue_survives_restart`

### 1.7.8 — Simulator fault injection
**Files:** `crates/pos-hardware/src/lib.rs`
Extend `SimulatedPrinter` with scripted faults: paper-out at byte N, cover-open, offline, slow. CI and demos run hardware-free (master plan C.15).
**Tests:** `simulator_fails_midway_when_scripted`

---

## Group 1.8 — Persistence and the money moment

### 1.8.1 — Repository module and transaction discipline
**Files:** `crates/pos-db/src/repo/mod.rs`
Every write method takes `&Transaction`. The caller owns the boundary; that is how conventions I-9 stays true.

### 1.8.2 — `SaleRepository`
**Files:** `crates/pos-db/src/repo/sale.rs` (new)
```rust
pub fn insert_complete(&self, tx: &Transaction, sale: &CompletedSale) -> Result<(), DbError>;
pub fn by_id(&self, id: SaleId) -> Result<Option<CompletedSale>, DbError>;
pub fn by_receipt_number(&self, r: &str) -> Result<Option<CompletedSale>, DbError>;
pub fn for_business_date(&self, store: StoreId, d: BusinessDate) -> Result<Vec<CompletedSale>, DbError>;
```
**There is no `update` and no `delete`.** Not private ones. Not "just for corrections."
**Tests:** `sale_repository_exposes_no_mutation` (a source-level assertion) · `roundtrip_preserves_every_field`

### 1.8.3 — The atomic finalize
**Files:** `apps/terminal/src-tauri/src/commands/sale.rs` (new)
One SQLite transaction writes: `sale` → `sale_line` → `sale_line_tax` → `sale_line_discount` → `sale_tax_summary` → `sale_tender` → `stock_ledger` → `sync_outbox` → `fiscal_queue` (Phase 2) → `audit_log` → `doc_sequence` bump. **Then** — outside the transaction — hardware side effects run: print, drawer.
**Tests:** `finalize_is_atomic_under_injected_failure` — fail at each of the eleven write points and assert the database is unchanged · `hardware_failure_after_commit_leaves_sale_complete`
**Done when:** killing the process between any two writes leaves either a complete sale with every companion row, or no sale at all. Nothing in between.

### 1.8.4 — Crash recovery for `Finalizing`
**Files:** `apps/terminal/src-tauri/src/recovery.rs` (new)
On start, an in-flight `Finalizing` re-runs idempotently from persisted state (E.1).
**Tests:** `interrupted_finalize_resumes_without_double_stock_event` · `interrupted_finalize_resumes_without_double_outbox_row`

### 1.8.5 — Key handling hardening
**Files:** `crates/pos-db/src/key.rs`
`POS_DB_KEY` is honoured in debug builds and **refused in release**, with a named error. A production register reading its database key from an environment variable is a production register whose key is in a process listing.
**Tests:** `release_build_refuses_env_key` (`#[cfg(not(debug_assertions))]`)

### 1.8.6 — Encrypted backup
*Gap G-1.*
**Files:** `crates/pos-db/src/backup.rs` (new)
```rust
pub fn snapshot(conn: &Connection, dest: &Path) -> Result<BackupInfo, DbError>;
pub fn restore(src: &Path, dest: &Path, key: &str) -> Result<(), DbError>;
pub fn verify(path: &Path, key: &str) -> Result<BackupInfo, DbError>;
```
Uses SQLite's online backup API (rusqlite `backup` feature) so it is consistent without stopping trade. Retention: hourly for 24 h, daily for 30 days, on a configured path. The backup is SQLCipher-encrypted with the same key.
**Tests:** `snapshot_during_active_writes_is_consistent` · `restore_produces_identical_data` · `verify_detects_truncation`
**Done when:** a register holding unsynced sales can be restored to a different machine and the sales are all there.

### 1.8.7 — Keychain-loss recovery screen
**Files:** `apps/terminal/src-tauri/src/recovery.rs`, UI in 1.11
`DbError::BadKey` on open (OS reinstall wiped the keychain) leads to an explicit recovery screen: restore from a local backup, or re-provision from the server after re-auth. **Never silent data loss, never a blank register** (E.4).
**Tests:** `bad_key_yields_recovery_state_not_panic`

### 1.8.8 — Disk-space guard
**Files:** `apps/terminal/src-tauri/src/health.rs` (new)
Below a threshold, refuse new sales with a clear alarm. A POS that "sells" without persisting is corrupting its ledgers (E.5).
**Tests:** `low_disk_blocks_new_sales_and_alarms`

### 1.8.9 — Outbox writer
**Files:** `crates/pos-db/src/repo/outbox.rs` (new)
Every fact write appends its outbox row in the same transaction (I-9). No pusher yet — that is Phase 3 — but the rows accumulate correctly from the first sale.
**Tests:** `every_fact_write_produces_exactly_one_outbox_row` · `outbox_row_payload_roundtrips`

---

## Group 1.9 — Sequences and business date

*Gap G-2.*

### 1.9.1 — Migration `0004`
**Files:** `crates/pos-db/migrations/0004_sale_columns_and_sequences.sql`
Per [`ref/schema.md`](ref/schema.md) §0004: sale columns, `sale_tax_summary`, tender columns, `tender_type`, `parked_cart`, `doc_sequence`.

### 1.9.2 — `SequenceRepository`
**Files:** `crates/pos-db/src/repo/sequence.rs` (new)
```rust
pub fn next(&self, tx: &Transaction, reg: RegisterId, kind: SeqKind) -> Result<u64, DbError>;
pub fn gaps(&self, reg: RegisterId, kind: SeqKind) -> Result<Vec<u64>, DbError>;
```
Bumped in the **same transaction** as the document it numbers, so a crash cannot consume a number without producing a document.
**Tests:** `sequence_is_gap_free_under_crash_injection` · `rollback_does_not_consume_a_number` · `concurrent_next_never_duplicates`
**Done when:** a hundred injected crashes at random points produce a receipt sequence with no gaps and no duplicates.

### 1.9.3 — Receipt numbering
**Files:** `apps/terminal/src-tauri/src/commands/sale.rs`
`REG01-000123`: per-register prefix plus a zero-padded counter. Globally unique by prefix, because a central counter cannot exist offline.
**Tests:** `receipt_number_format_is_stable` · `two_registers_never_collide`

### 1.9.4 — Business date at finalize
**Files:** `apps/terminal/src-tauri/src/commands/sale.rs`
Derived from the shift (conventions §11), not from wall-clock midnight.
**Tests:** `sale_at_0100_belongs_to_previous_business_date` · `business_date_survives_timezone_change`

---

## Group 1.10 — Stock ledger

### 1.10.1 — Migration `0005`
**Files:** `crates/pos-db/migrations/0005_stock_ledger.sql`

### 1.10.2 — `StockRepository`
**Files:** `crates/pos-db/src/repo/stock.rs` (new)
```rust
pub fn append(&self, tx: &Transaction, e: &StockEvent) -> Result<(), DbError>;
pub fn on_hand(&self, p: ProductId, s: StoreId) -> Result<Qty, DbError>;      // from cache
pub fn rebuild_cache(&self, tx: &Transaction, s: StoreId) -> Result<u64, DbError>;
pub fn negative_stock(&self, s: StoreId) -> Result<Vec<NegativeStockRow>, DbError>;
```

### 1.10.3 — Cache rebuild equivalence
**Files:** `crates/pos-db/tests/stock.rs`
**Tests:** `prop_cache_rebuild_matches_ledger` — after any sequence of events, rebuilding produces byte-identical cache rows (conventions I-6).
**Done when:** the cache can never silently diverge from the ledger without CI noticing.

### 1.10.4 — Negative stock: allow and flag
**Files:** `crates/pos-db/src/repo/stock.rs`
Default allow, flag loudly; per-store hard-block setting. Blocking a sale because the ledger is wrong punishes the customer at the register for a back-office error (master plan C.7).
**Tests:** `negative_stock_allowed_by_default_and_flagged` · `hard_block_setting_refuses_the_line` · `two_offline_registers_selling_the_last_unit_both_succeed` (E.12)

---

## Group 1.11 — The terminal UI

*Arabic-first RTL from the first commit. Retrofitting RTL is miserable; scaffolding it is cheap.*

### 1.11.1 — i18n infrastructure
*Gap G-5.*
**Files:** `apps/terminal/src/i18n/` (new: `index.ts`, `ar.ts`, `en.ts`), `packages/ui/src/`
Typed catalog; keys per conventions §2; `<html dir="rtl" lang="ar">` by default.
**Tests:** `catalogs_have_identical_key_sets` — fails when a key exists in one language and not the other.

### 1.11.2 — RTL lint
**Files:** `biome.json`, or a custom check script
Ban physical direction utilities (`pl-`, `pr-`, `ml-`, `mr-`, `left-`, `right-`, `text-left`, `text-right`) in favour of logical ones (`ps-`, `pe-`, `ms-`, `me-`, `start-`, `end-`, `text-start`, `text-end`).
**Done when:** using `pl-4` fails `just lint`.

### 1.11.3 — Formatting helpers
**Files:** `apps/terminal/src/lib/format.ts`
`formatMoney(minor, currency, decimals)`, `formatQty(milli, weighed)`, `formatDate(iso, tz, locale)`. Western Arabic digits. Never `toLocaleString` inline — display precision is a store setting.
**Tests:** `formats_jod_with_store_decimals` · `uses_western_digits_in_arabic_locale`

### 1.11.4 — Lock / PIN screen (D1)
**Files:** `apps/terminal/src/screens/Lock.tsx`
Numpad, ≥48 px targets, register name, sync status, open-shift owner. Fast user switch.

### 1.11.5 — Sale screen (D3)
**Files:** `apps/terminal/src/screens/Sale.tsx` and components
Three zones per master plan D: cart list (line menu on long-press: qty, discount, override, void) · totals block with a huge TOTAL and the always-visible status strip (🔵 synced / 🟡 offline *n* queued / fiscal-pending *n* / training banner) · search + PLU/tile grid.

### 1.11.6 — Global scan capture
**Files:** `apps/terminal/src/lib/scanner.ts`
A hidden input capturing keystrokes anywhere on the sale screen, distinguishing a scan burst from typing by inter-key timing (< 30 ms between characters, terminated by Enter). **Scans must route correctly even when focus is in the search box** — that detail is where most implementations break.
**Tests:** `scan_burst_detected_over_typing` · `scan_routes_while_search_focused`

### 1.11.7 — Tender screen (D4)
**Files:** `apps/terminal/src/screens/Tender.tsx`
Amount due huge; cash numpad plus denomination quick-keys; live change display; the split-tender list with remaining due.

### 1.11.8 — Post-sale toast (D5)
**Files:** `apps/terminal/src/components/PostSale.tsx`
Change due big, print / reprint buttons, auto-return in ~3 s.

### 1.11.9 — Settings & diagnostics (D10)
**Files:** `apps/terminal/src/screens/Diagnostics.tsx`
Test print, drawer kick, scanner echo, printer status, database health, backup status, about.

### 1.11.10 — Local product quick-add (D11)
**Files:** `apps/terminal/src/screens/QuickAdd.tsx`
Manager-permission emergency SKU + price so an unknown barcode never stalls the queue (E.39). Marked as a local edit; Phase 3 syncs it up as a change-request, never a silent merge.

### 1.11.11 — Keyboard map
**Files:** `apps/terminal/src/lib/keymap.ts`
`F2` search · `F4` pay · `F6` park · `F7` resume · `F9` returns · `Del` void line · `+/−` qty · `F12` lock. Scans need no focus.
**Tests:** `every_action_reachable_without_a_mouse`

### 1.11.12 — Designed empty and edge states
**Files:** various
Offline banner reading *"Sales are safe and will sync."* Printer-out warning **at Pay**, not after. Fiscal-pending badge explaining itself on tap. Min-size guard on the sale screen (E.60).

---

## Group 1.12 — Fixture, benchmarks, gate

### 1.12.1 — The Jordanian minimarket seed
*Gap G-10.*
**Files:** `crates/pos-db/src/seed/minimarket.rs` (new), `crates/pos-db/src/seed/data/*.csv`
~200 products with real Arabic names across mixed tax categories: bread and water (exempt), tea and sugar (exempt), soft drinks and crisps (16%), a reduced-rate item, loose vegetables sold by weight, a deli item with a price-embedded barcode, a tobacco line (`min_age = 18`), and a price-controlled staple (`max_price_minor`).
**Done when:** `just seed` produces a store you can actually sell from, and every demo and screenshot uses it. This fixture is also the tax and RTL test corpus.

### 1.12.2 — Hand-checked tax report fixture
**Files:** `crates/pos-domain/tests/tax_report.rs`
A scripted trading day over the fixture: mixed rates, a weighed line, a basket discount, a refund, and a cash-rounded tender. **Total it by hand once**, commit the expected figures, and let CI defend them forever.
**Tests:** `tax_report_matches_hand_check_fixture`
> This is the only test that proves the *product* is right rather than the *code*. Do the arithmetic on paper.

### 1.12.3 — Benchmark suite in CI
**Files:** `.github/workflows/ci.yml`
`price_cart` < 16 ms · FTS search < 50 ms. Fail on > 20% regression.

### 1.12.4 — Edge-case test sweep
**Files:** `crates/*/tests/`
Close out every Phase-1 row of [`ref/test-catalog.md`](ref/test-catalog.md): E.1, E.3, E.4, E.5, E.6, E.7, E.12, E.17, E.18, E.19, E.36, E.37, E.38, E.39, E.40, E.41, E.60, E.69, E.70, E.71.

---

## Exit gate

Phase 1 is done when all of these are true:

```bash
just lint && just test                      # everything green, no ignored tests
cargo nextest run --workspace -E 'test(prop_)'   # every property passes
cargo bench --workspace                     # both budgets met
```

And, by demonstration — do this literally, with the network cable unplugged:

1. **Open the app offline.** Sign in with a PIN. Cold start to sellable in under 3 seconds.
2. **Sell a basket** containing: a scanned barcoded item, an item found by Arabic search, a weighed item via price-embedded barcode, an exempt item, and a 16% item.
3. **Apply a basket discount.** Confirm it prorates to the lines to the fil.
4. **Pay cash** with overtender. Confirm change, and confirm cash rounding produced an explicit adjustment line.
5. **The receipt prints in Arabic**, correctly shaped and right-to-left, with a tax summary showing exempt and standard as separate rows.
6. **Park a sale, serve another customer, resume it.** Nothing is lost.
7. **Pull the power mid-finalize.** Restart. Exactly one sale exists, with exactly one stock event and one outbox row.
8. **Run the tax report** for the day and check it against the receipts by hand. It reconciles to the fil.
9. **Verify the audit chain.** Intact. Then hand-edit one row in the database and verify it reports `Broken` at that sequence.
10. **Take a backup, wipe the keychain, restart.** The recovery screen appears. Restore. Every sale is still there.

**If any of the ten fails, Phase 1 is not done.** Number 7 and number 10 are the ones most likely to be skipped and the two most likely to matter on a merchant's worst day.

→ **Next:** [`phase-2-money-grade.md`](phase-2-money-grade.md)
