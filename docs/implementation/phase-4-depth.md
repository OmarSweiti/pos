# Phase 4 — Depth

> **Exit:** three stores run a full week unattended, with promotions whose cost report matches finance's arithmetic, stock that receives and counts, and shelf labels that keep the merchant out of trouble with the ministry.

**Effort:** 8–10 weeks.
**Scope from the master plan:** C.1 price lists, C.7 receiving/counts/transfers, C.9 promotions, C.12 back-office report suite, C.13 multi-store, migration 0011.
**Plus, promoted from J.3:** shelf-label printing, treated as a **compliance feature** rather than a convenience.

Nothing here is on the money-critical path. That is exactly why it comes fourth — dashboards never dictate architecture, and a promotions engine built before a merchant has opinions about stacking is a promotions engine built twice.

---

## Group dependency graph

```
4.1 price lists ──┬──→ 4.4 promotions engine ──→ 4.5 promotion reporting
                  │
4.2 receiving & WAC ──┬──→ 4.3 counts & transfers
                      └──→ 4.6 label printing (needs prices AND stock)
4.7 report suite ─────────────────────────────────────────────────────┐
4.8 multi-store rollout ──────────────────────────────────────────────┤
                                                                       ▼
                                                            4.9 three-store pilot
```

---

## Group 4.1 — Price lists

### 4.1.1 — Migration `0011`, part one
**Files:** `crates/pos-db/migrations/0012_pricing_promotions_supply.sql`
`price_list`, `price` per [`ref/schema.md`](ref/schema.md) §0011.

### 4.1.2 — Resolution order
**Files:** `crates/pos-domain/src/pricing.rs`
**Promotion > store price list > base price.** Time-effective via `valid_from` / `valid_to`, inclusive/exclusive as everywhere else.
**Tests:** `store_price_list_overrides_base` · `expired_price_list_falls_back_to_base` · `prop_resolution_is_deterministic_at_boundaries` · `resolution_order_is_promotion_then_list_then_base`

### 4.1.3 — Price change queues a label reprint
**Files:** `crates/pos-db/src/repo/price.rs` (new)
Any effective price change writes a `label_reprint_queue` row. Not optional — see 4.6.
**Tests:** `price_change_queues_exactly_one_label_row`

---

## Group 4.2 — Receiving and weighted-average cost

### 4.2.1 — Migration `0011`, part two
**Files:** `crates/pos-db/migrations/0012_pricing_promotions_supply.sql`
`supplier`, `goods_receipt`, `goods_receipt_line`.

### 4.2.2 — Receiving posts stock and updates WAC
**Files:** `crates/pos-domain/src/stock.rs`, `crates/pos-db/src/repo/receiving.rs` (new)
```
new_wac = (on_hand × wac + qty_in × unit_cost) / (on_hand + qty_in)
```
Guard divide-by-zero and the negative-on-hand edge, both in domain tests.
**Tests:** `prop_wac_never_negative` · `wac_on_zero_on_hand_takes_the_receipt_cost` · `wac_with_negative_on_hand_is_handled_not_panicked` · `receiving_posts_one_stock_event_per_line`
> Purchase orders proper can wait for Phase 5. **Receiving stands alone** and is what the merchant actually needs first — they already know what arrived.

### 4.2.3 — Cost deviation guard
**Files:** `crates/pos-domain/src/stock.rs`
A cost more than *x*% from the last receipt requires confirmation (E.43). A fat-fingered 10× cost silently poisons inventory valuation and every margin report downstream.
**Tests:** `ten_times_cost_requires_confirmation` · `corrective_adjust_recomputes_wac`

### 4.2.4 — Vendor returns (RTV)
**Files:** `crates/pos-db/src/repo/receiving.rs`
Stock event `rtv` plus a supplier credit-note record.
**Tests:** `rtv_reduces_stock_and_records_supplier_credit`

---

## Group 4.3 — Counts and transfers

### 4.3.1 — Freeze-less stock count
**Files:** `crates/pos-domain/src/stock.rs`, `crates/pos-db/src/repo/count.rs` (new)
Snapshot expected at count start; count physically; post differences as `count_correction` events with a variance report. **Sales during the count are fine** — that is the whole point of snapshotting (E.42).
**Tests:** `count_tolerates_sales_mid_count` · `variance_equals_counted_minus_snapshot` · `posting_a_count_writes_correction_events_not_absolute_quantities`

### 4.3.2 — Scanner-driven count screen
**Files:** `apps/terminal/src/screens/StockCount.tsx`
Scan → increment → running variance. Designed for a person on a ladder, so: large targets, audible confirmation, undo-last.

### 4.3.3 — Transfers
**Files:** `crates/pos-db/src/repo/transfer.rs` (new)
Out at source (in transit) → in at destination. Short or damaged arrivals create an adjustment at the destination plus a notification to the source (E.44).
**Tests:** `transfer_out_then_in_conserves_total_stock` · `short_receipt_creates_destination_adjustment_and_notifies`

### 4.3.4 — Low-stock worklist
**Files:** `apps/backoffice/src/pages/inventory/`
From `reorder_point_milli` per product per store.

---

## Group 4.4 — Promotions engine

*Resisted for three phases. Manual discounts genuinely covered Phases 1–3.*

### 4.4.1 — Migration `0011`, part three
**Files:** `crates/pos-db/migrations/0012_pricing_promotions_supply.sql`
`promotion` with `config_json` and `priority`.

### 4.4.2 — Types, in order of real-world frequency
**Files:** `crates/pos-domain/src/promo.rs` (new)
Percent off item/category · amount off · **multibuy** ("3 for 1 JD", "buy 2 get 1 free" — the same mechanism: a quantity threshold gives a price for the group) · **mix & match** (any 3 from set S for X) · basket threshold ("5% off over 50 JD") · time-boxed and customer-group variants of all of the above.
**Tests:** one example test per type, plus `multibuy_and_bogof_share_one_implementation`

### 4.4.3 — Stacking, decided and documented
**Files:** `crates/pos-domain/src/promo.rs`
**The strict simple model, shipped as-is:**
- promotions carry an integer `priority`;
- per line, the **single best** promotion wins — no stacking;
- basket-level promotions apply **after** line promotions;
- a manual discount **excludes** automatic ones on the same line unless a setting allows.

**Tests:** `prop_best_single_promotion_is_chosen` · `basket_applies_after_line` · `manual_excludes_auto_by_default` · `prop_promotions_are_order_independent`
> Stacking is the actual hard part of every promotions engine. Ship the simple model, document it in the merchant's language, and **never improvise combination behaviour** — improvised stacking is how a campaign costs three times its budget and nobody can explain why.

### 4.4.4 — Attribution and proration
**Files:** `crates/pos-domain/src/promo.rs`
Applied promotions become **explicit `DiscountAttribution` rows on lines** carrying the promotion id and amount. Basket-level promotions prorate to lines by largest remainder **before any fiscal document is built** — correction **C-2** applies here exactly as it does to manual basket discounts.
**Tests:** `prop_promotions_never_increase_total` · `prop_promotion_proration_conserves_to_the_fil` · `promotion_discount_survives_the_fiscal_percentage_roundtrip`

### 4.4.5 — Back-office promotion editor
**Files:** `apps/backoffice/src/pages/promotions/`
With a **preview**: pick a promotion, build a sample basket, see the resulting attributions. A merchant who can see what a promotion does before it runs writes fewer angry emails afterwards.

---

## Group 4.5 — Promotion reporting

### 4.5.1 — Campaign cost report
**Files:** `apps/server/src/reports/promotions.rs` (new)
Per promotion: transactions, units moved, discount given, gross before and after. Reads `sale_line_discount.promotion_id` — which is why attribution has existed since Phase 1.
**Done when:** it matches finance's own arithmetic on a real campaign, checked once by hand.
> This report doubles as inspection-day evidence of what an offer actually charged (master plan J.3): Jordan's ministry oversees promotional offers, and *"honest promotion"* is easier to demonstrate with a per-line attribution table than with a marketing plan.

---

## Group 4.6 — Shelf labels — a compliance feature

*Master plan J.3. Jordan's Ministry of Industry, Trade & Supply actively inspects retail, and its violation statistics are dominated by **failure to display prices** and **selling above set prices**. This is not a convenience feature.*

### 4.6.1 — Label templates and printing
**Files:** `crates/pos-hardware/src/render/label.rs` (new)
Reuse the Phase-1 raster pipeline: Arabic name, price, barcode, unit price where applicable. Two or three sizes.
**Tests:** `golden_label_ar_is_byte_stable`

### 4.6.2 — The reprint worklist
**Files:** `apps/backoffice/src/pages/labels/`
Fed by price changes (4.1.3), new products, and `displayed_price` overrides (Phase 1, 1.4.7). Print in batches, mark printed.
**Tests:** `worklist_clears_only_what_was_printed`

### 4.6.3 — Regulated price ceilings
**Files:** `crates/pos-domain/src/cart.rs`, back office
`max_price_minor` blocks a sale above the ceiling **and** blocks the catalogue save — belt and braces (E.71).
**Tests:** `catalog_save_above_ceiling_is_rejected` · `sale_above_ceiling_is_hard_blocked`

### 4.6.4 — Price-check station mode
**Files:** `apps/terminal/src/screens/PriceCheck.tsx`
A read-only scan screen. Cheap now that everything else exists, and directly serves the price-transparency duty.

---

## Group 4.7 — The report suite

*All reports are queries over the three ledgers. **No report writes data.***

### 4.7.1 — Sales reports
**Files:** `apps/server/src/reports/sales.rs` (new)
By day / hour / register / cashier; by product / category with quantity, net, tax, gross, and **margin now that WAC exists**.

### 4.7.2 — The tax filing report
**Files:** `apps/server/src/reports/tax.rs` (new)
Already built in Phase 1 for the register; here it becomes the back-office deliverable per [`ref/tax-jordan.md`](ref/tax-jordan.md) §6. **This report *is* the accountant's filing input** — treat it as a product surface, not a query.

### 4.7.3 — Tender vs. PSP settlement
**Files:** `apps/server/src/reports/settlement.rs` (new)
Reconciliation by `psp_ref`, with unmatched PSP entries and unmatched tenders listed **separately** so the direction of the discrepancy is obvious (E.23).

### 4.7.4 — The fraud lens
**Files:** `apps/server/src/reports/exceptions.rs` (new)
Refunds and voids by user · price overrides by user with reason strings (E.33) · no-sale drawer opens by user (E.35) · training-mode transactions · over/short trend by cashier. Chronic short is a training issue or a theft issue, and the report is how you tell.

### 4.7.5 — Inventory reports
**Files:** `apps/server/src/reports/inventory.rs` (new)
On-hand and valuation (Σ qty × WAC) · movement · negative stock · low stock · shrinkage by reason code.

### 4.7.6 — Loyalty liability
**Files:** `apps/server/src/reports/loyalty.rs` (new)
Outstanding points × redemption value. An accountant will ask; it is a balance-sheet item.

### 4.7.7 — Export and timezone discipline
**Files:** `apps/server/src/reports/mod.rs`
Every report exports CSV. **Every report buckets by store-local calendar day** (`Asia/Amman`) from `business_date`, regardless of UTC storage.
**Tests:** `every_report_exports_csv_with_identical_numbers` · `reports_bucket_by_business_date_not_utc`

---

## Group 4.8 — Multi-store rollout

### 4.8.1 — Scoped settings resolution
**Files:** `apps/server/src/scope.rs`
Org → store → register, with the most specific winning. Every setting in `merchant-decisions.md` resolves this way.
**Tests:** `register_setting_overrides_store_overrides_org`

### 4.8.2 — Cross-store reporting
**Files:** `apps/backoffice/src/pages/reports/`
Store comparison, consolidated totals, per-store drill-down.

### 4.8.3 — Cross-store refund checks
**Files:** `apps/server/src/sync/push.rs`
Server-side remaining-refundable enforcement whenever connected (E.31). The offline window remains an accepted, disclosed risk surfaced in the refunds-by-user report.
**Tests:** `second_store_refund_of_the_same_receipt_is_refused_when_connected`

---

## Group 4.9 — The three-store pilot

### 4.9.1 — Pilot preparation
**Files:** `docs/implementation/ref/merchant-decisions.md` (completed for each store)
Three stores, real assortments, real staff, real customers. Every merchant decision answered per store. Backups verified. Device health dashboard watched daily.

### 4.9.2 — The week
Run unattended for seven trading days. Log everything that surprises anyone.

### 4.9.3 — The debrief
**Files:** `docs/implementation/ref/test-catalog.md` (extend)
Every surprise becomes either a fixed bug, a new edge case with a test, or an explicitly accepted risk with a rationale. **A surprise that becomes none of the three is a surprise that will happen again.**

---

## Exit gate

```bash
just lint && just test
cargo nextest run --workspace -E 'test(prop_)'
cargo bench --workspace                       # budgets still met with 50k SKUs and promotions active
```

By demonstration:

1. **Three stores trade for a full week** with no intervention from you.
2. **A real promotion runs**, and its cost report matches finance's arithmetic — checked by hand.
3. **Receive a delivery**; WAC updates correctly; a deliberate 10× cost is caught before posting.
4. **Count a category during trading hours**; sales mid-count do not corrupt the variance.
5. **Transfer stock between two stores**, arriving short; the destination adjustment and source notification both appear.
6. **Change a price**; the label worklist populates; labels print in Arabic and are readable on a shelf.
7. **Attempt to save a controlled staple above its ceiling** — blocked. Attempt to sell above it — blocked.
8. **Every report exports CSV**, buckets by Asia/Amman calendar day, and reconciles against the register's own X/Z for the same day.
9. **Refund the same receipt at two stores** while connected — the second is refused. Repeat offline — both succeed, and the case appears in the refunds-by-user report.
10. **Automated tests exist** for E.33, E.42, E.43, E.44, E.63, E.70, E.71.

→ **Next:** [`phase-5-harden-and-launch.md`](phase-5-harden-and-launch.md)
