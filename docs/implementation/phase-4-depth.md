# Phase 4 — Depth

> **Exit:** three stores run a full week unattended, with promotions whose cost report matches finance's arithmetic, stock that receives and counts, and shelf labels that keep the merchant out of trouble with the ministry.

**Effort:** 9–12 weeks.
**Scope from the master plan:** C.1 price lists, C.7 receiving/counts/transfers, C.9 promotions, C.12 back-office report suite, C.13 multi-store, migration **0012**.
**Plus, promoted from J.3:** shelf-label printing, treated as a **compliance feature** rather than a convenience.
**Plus:** the supplier-tax, filing-period and regulated-display facts the accounting and compliance work needs, and **a pre-pilot gate** (4.9.0) that has to pass before three real shops trade on this.

Nothing here is on the money-critical path. That is exactly why it comes fourth — dashboards never dictate architecture, and a promotions engine built before a merchant has opinions about stacking is a promotions engine built twice.

---

## Group dependency graph

```
4.1 price lists ──┬──→ 4.4 promotions engine ──→ 4.5 promotion reporting
  (4.1.1 is the    │
   0012 spine)     │
                  ├──→ 4.2 receiving, supplier tax & WAC ──┬──→ 4.3 counts & transfers
                  │                                        └──→ 4.6 label printing
                  └──→ 4.7 report suite ────────────────────────────────────────┐
4.8 multi-store rollout ──────────────────────────────────────────────────────  ┤
                                                                                ▼
                                                        4.9.0 pre-pilot gate → 4.9 pilot
```

**One migration, one microstep.** `0012` is created complete by **4.1.1** and never reopened. Groups
4.2 and 4.4 therefore depend on 4.1.1 being merged, and their own first steps are behavioural rather
than schema steps. Three groups each editing one migration file across three branches and three
commits — which is what "part one / part two / part three" asked for — contradicts the law that a
committed migration is never edited, and produces either a protected-path refusal or a cross-branch
conflict repair before the migration has shipped at all.

---

## Group 4.1 — Price lists and the `0012` schema spine

### 4.1.1 — Migration `0012` — the complete Phase-4 schema
**Files:** `crates/pos-db/migrations/0012_pricing_promotions_supply.sql`, `apps/server/migrations/`
The whole of [`ref/schema.md`](ref/schema.md) §0012, in one file, in one commit: `price_list` and `price`; `promotion` with its immutable `promotion_version`, regulated exclusions, publications and attributions; `supplier`, supplier tax invoices and their line taxes; goods receipts; stock counts; transfers; and the filing-period, adjustment, allocation, credit-ledger and election tables.

Its **PostgreSQL mirror ships in the same commit**. Phase 3's mirror stopped at SQLite `0011`, which left the cloud without the pricing, promotion, supply and filing tables that Phase 4's own reports read.
**Verify:** `just verify-schema && just verify-pg`
**Done when:** both verifiers pass, and no later Phase-4 microstep names this file in its `Files:` list.

### 4.1.2 — Resolution order
**Files:** `crates/pos-domain/src/pricing.rs`
**Promotion > store price list > base price.** Time-effective via `valid_from` / `valid_to`, inclusive/exclusive as everywhere else.
**Tests:** `store_price_list_overrides_base` · `expired_price_list_falls_back_to_base` · `prop_resolution_is_deterministic_at_boundaries` · `resolution_order_is_promotion_then_list_then_base`

### 4.1.3 — Price change queues a label reprint
**Files:** `crates/pos-db/src/repo/price.rs` (new)
Any effective price change writes a `label_reprint_queue` row. Not optional — see 4.6.
**Tests:** `price_change_queues_exactly_one_label_row`

---

## Group 4.2 — Receiving, supplier tax, and weighted-average cost

### 4.2.1 — Supplier tax invoices, as facts
**Depends on:** 4.1.1 merged. This microstep writes **no** migration.
**Files:** `crates/pos-db/src/repo/supplier.rs` (new), `crates/pos-domain/src/supply.rs` (new)
Receiving used to store a quantity and an undefined `unit_cost_minor`. That is enough for inventory and not enough for tax: the merchant's return needs the supplier invoice's net, tax and gross **by component and rate**, imports and imported services, exempt purchases, deductibility class, common-input allocation for a mixed taxable/exempt business, supplier credit notes and adjustments.

Without them, recoverable GST is either lost or silently rolled into weighted-average cost — the merchant overpays tax, or overstates inventory and understates margin, and either way the accountant cannot reconcile it. **WAC includes net plus non-deductible tax only.**
**Tests:** `a_supplier_invoice_records_net_tax_and_gross_by_component` · `deductible_input_tax_is_excluded_from_wac` · `non_deductible_input_tax_is_included_in_wac` · `a_mixed_taxable_and_exempt_business_apportions_common_input` · `a_supplier_credit_note_reverses_its_invoice_by_component`

### 4.2.2 — Receiving posts stock and updates WAC
**Files:** `crates/pos-domain/src/stock.rs`, `crates/pos-db/src/repo/receiving.rs` (new)
```
new_wac = (on_hand × wac + qty_in × unit_cost) / (on_hand + qty_in)
```
**When `on_hand ≤ 0`, the new WAC is the receipt's `unit_cost`.** "Handled, not panicked" is a licence to choose, and the choices give materially different numbers: blending a phantom negative inventory into the average can land the WAC far from any real cost, and inventory valuation is an audited balance-sheet figure. A negative on-hand carries no cost basis to average, so there is nothing to blend. A receipt into negative stock is also reported on the variance report, because it means the ledger was wrong before the delivery arrived.
**Tests:** `prop_wac_never_negative` · `prop_wac_stable_under_zero_qty_receipt` · `prop_wac_is_between_the_min_and_max_cost_ever_received` — the constraint that actually catches a blended phantom cost, where "never negative" passes for any nonsense above zero · `wac_on_zero_on_hand_takes_the_receipt_cost` · `wac_with_negative_on_hand_takes_the_receipt_cost` · `receiving_posts_one_stock_event_per_line`
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
**Tests:** `transfer_out_then_in_conserves_total_stock` · `a_transfer_preserves_an_unknown_cost_basis` · `short_receipt_creates_destination_adjustment_and_notifies`

### 4.3.4 — Low-stock worklist
**Files:** `apps/backoffice/src/pages/inventory/`
From `reorder_point_milli` per product per store.

### 4.3.5 — Waste and expiry adjustments by reason
**Files:** `crates/pos-db/src/repo/stock.rs`
Lot and expiry tracking are deliberately deferred; the compensating control is that an expiry write-off is a stock adjustment carrying its **reason code**, and shrinkage is reportable by reason (4.7.5). Case 45 named that compensating test with no microstep to own it, which is how a deferral quietly becomes an absence.
**Tests:** `waste_adjustment_by_reason_code` (E.45) · `shrinkage_reports_group_by_reason_code`

---

## Group 4.4 — Promotions engine

*Resisted for three phases. Manual discounts genuinely covered Phases 1–3.*

### 4.4.1 — Promotion versions, and why terms are never edited
**Depends on:** 4.1.1 merged. This microstep writes **no** migration.
**Files:** `crates/pos-domain/src/promo.rs` (new)
A promotion's **terms are versions, not edits**: `promotion_version` carries the name, kind, `config_json`, eligibility, priority, requalification policy and validity window, immutably, and every attribution names the version that applied. Joining an old sale to a promotion someone has since edited shows terms different from the ones charged — which is exactly what the "inspection-day evidence" claim in 4.5.1 needs to survive, and exactly what a mutable `promotion_id` cannot supply.

Where the register itself publishes the offer, the copy, the channel and an artifact hash are retained with the version. Where the merchant advertises elsewhere, the product's claim narrows honestly to **charged-price attribution** and the merchant keeps their own advertisements.
**Tests:** `an_edit_creates_a_new_version_and_never_mutates_an_applied_one` · `an_attribution_resolves_to_the_version_that_applied` · `a_published_offer_retains_its_copy_and_hash`

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
Applied promotions become **explicit attribution rows on lines** carrying the promotion **version**, the group key, the threshold quantity and the group price — the provenance `RequalifyPolicy::DealBreak` needs as an input when part of the group comes back (2.3.2). Basket-level promotions prorate to lines by largest remainder **before any fiscal document is built**, to exact fils, exactly as manual basket discounts do.

There is no percentage round-trip here, because there is none in the fiscal document either: the superseded C-2 gate would have refused correct promotions for being unrepresentable at some precision ([`00-master-plan.md`](00-master-plan.md) §4a).
**Tests:** `prop_promotions_never_increase_total` · `prop_promotion_proration_conserves_to_the_fil` · `an_attribution_carries_the_group_key_and_threshold_a_refund_needs`

### 4.4.5 — Back-office promotion editor
**Files:** `apps/backoffice/src/pages/promotions/`
With a **preview**: pick a promotion, build a sample basket, see the resulting attributions. A merchant who can see what a promotion does before it runs writes fewer angry emails afterwards. Saving publishes a **new version**; it never edits one that has already been applied.

### 4.4.6 — Single-use coupon codes
**Files:** `crates/pos-domain/src/promo.rs`, `apps/server/src/promotions/codes.rs` (new)
A coupon code is redeemable once. Redemption marks it used, and the mark **syncs**: a photocopied code presented at a second till is refused when connected, and the offline window is the same disclosed, bounded risk as every other cross-register uniqueness claim — surfaced in the promotion report rather than implied to be impossible.
**Tests:** `single_use_code_marked_used_on_redemption_sync` (E.63) · `a_reused_code_is_refused_when_connected` · `an_offline_reuse_is_surfaced_in_the_campaign_report`

### 4.4.7 — Gift cards, on the Phase-2 stored-value instrument
**Files:** `crates/pos-domain/src/stored_value.rs`
The ledger, the balance and the online-authorise-only rule already exist (2.3.11), because Phase 2's own refund policy needed them. Gift cards add what a *sold* instrument needs and store credit does not: sale as a non-stock line, top-up, an expiry policy if the merchant has one, and the unclaimed-balance treatment. The liability report is 4.7.6.

Whether the sale of a gift card is itself a taxable supply, or only its redemption is, is not a domain decision — it is recorded per merchant against every ledger event and the instrument stays disabled until it is.
**Tests:** `a_gift_card_sale_writes_an_issue_event_and_no_stock_movement` · `top_up_extends_the_balance_without_a_new_instrument` · `an_expiry_policy_is_required_before_gift_cards_can_be_enabled`

> **The stored-value tax point is already an open item**, owned by `2.3.2` and recorded in [`ref/schema.md`](ref/schema.md) — it covers issue, top-up, redemption, adjustment and expiry for every funded-value model, gift cards included. Its default is that the tables ship and no ledger event is posted, so gift cards stay disabled here for the same reason store credit did in Phase 2. Do not open a second item; answer that one, and record the expiry and unclaimed-balance policy against it in questionnaire rows 12.1b and 12.1c.

---

## Group 4.5 — Promotion reporting

### 4.5.1 — Campaign cost report
**Files:** `apps/server/src/reports/promotions.rs` (new)
Per promotion **version**: transactions, units moved, discount given, gross before and after. Reads the attribution rows from 4.4.4 — which is why attribution has existed since Phase 1.
**Done when:** it matches finance's own arithmetic on a real campaign, checked once by hand.
> This report doubles as inspection-day evidence of what an offer actually charged (master plan J.3): Jordan's ministry oversees promotional offers, and *"honest promotion"* is easier to demonstrate with a per-line attribution table than with a marketing plan. **The evidence claim is only as good as the version snapshot**, which is why 4.4.1 makes terms immutable — a report that resolves to today's edited promotion proves the opposite of what it claims. Where the offer was published outside this product, the report compares the promised terms only if the copy was retained with the version; otherwise it says "charged-price attribution" and means it.

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

### 4.6.5 — Regulated goods: what may not be sold, advertised or displayed
**Files:** `crates/pos-domain/src/cart.rs`, `crates/pos-domain/src/promo.rs`, back office
The age gate is correct and is not the whole of the rule. Jordan's Public Health Law also prohibits selling individual cigarettes and prohibits tobacco advertising, with restrictions on display — and the generic catalogue can create a single-cigarette SKU while the generic promotion engine can advertise tobacco. A store can pass every age-gate test and still make a prohibited sale or publish a prohibited offer.

So a product carries `regulated_kind`, a sealed-pack sale-form invariant blocks a below-pack quantity for `tobacco`, and the promotion engine **refuses** to attach a regulated product to a published offer or a label. This is a hard block, not a warning: the exposure is a statutory fine and, in some cases, imprisonment for the merchant.
**Tests:** `an_individual_cigarette_sale_is_refused` · `a_promotion_cannot_include_a_regulated_product` · `a_label_template_cannot_advertise_a_regulated_product`

> ⚠️ **OPEN — blocks 4.6.1.** What current tobacco-display layout, marking and customer-facing label restrictions apply to each enabled tobacco product, and which other product classes carry equivalent restrictions? Default until answered: no customer-facing display feature is built, promotions exclude tobacco, the label worklist refuses tobacco labels, and only the sale-form and advertising blocks above ship.
> Owner: `4.6.1`. Source that settles it: the current official Tobacco Products Display Regulation and written implementation guidance from the responsible authority.

---

## Group 4.7 — The report suite

*All reports are queries over the three ledgers. **No report writes data.***

### 4.7.1 — Sales reports
**Files:** `apps/server/src/reports/sales.rs` (new)
By day / hour / register / cashier; by product / category with quantity, net, tax, gross, and margin.

**Margin reads the cost captured on the sale event, not today's WAC.** `stock_ledger.unit_cost_minor` is written on **every** kind including `sale`, carrying the cost in force at that moment with `is_cost_estimated` set where no basis existed. Deriving margin from the current `stock_cache.wac_minor` instead makes January's reported profit change in June, because WAC moved — the same class of error as invariant I-5, applied to cost, and unfixable afterwards because the ledger is append-only.
**Tests:** `margin_report_is_stable_when_wac_changes_later` · `sales_before_a_cost_basis_existed_are_counted_visibly_rather_than_costed_at_zero`

### 4.7.2 — The sales-side tax reconciliation, and the filing workpaper
**Files:** `apps/server/src/reports/tax.rs` (new)
The register's `report_tax_by_rate` is built in Phase 1 (it is the deliverable the tax engine exists for, and it had no owner in any phase); here it becomes the back-office deliverable per [`ref/tax-jordan.md`](ref/tax-jordan.md) §6.

**It is a sales-side reconciliation, not a return, and it is named that way in the UI, the export header and the owner guide.** The statutory declaration also needs prior credit, domestic purchases and expenses by rate, assets, imports and imported services, exempt purchases, non-deductible input tax, adjustments and the refund or carry-forward election. Those arrive with 4.2.1's supplier facts and the filing-period tables in `0012`; the workpaper maps them to return boxes, tracks the assigned filing cycle and its boundaries, and carries nil-return status — because a normal general-tax return, a special-tax return and an ASEZA return do not share a calendar.

A refund issued in period N+1 against a sale in period N keeps **both** period references and its disposition, rather than becoming a negative row in an arbitrary date range.
**Tests:** `the_workpaper_maps_every_populated_box` · `a_nil_period_still_produces_a_return_row` · `a_refund_in_the_next_period_preserves_both_period_references`

> ⚠️ **OPEN — blocks 4.7.2.** Which return period and box must receive a credit note issued after the original invoice's filed period for each supported return type and jurisdiction? Default until answered: show the credit as a negative in sales reconciliation on the credit-note date, preserve the original and credit periods, and leave statutory `box_disposition` unresolved rather than auto-populating a return.
> Owner: `4.7.2`. Source that settles it: the current official ISTD credit-note return instructions for General Tax, Special Tax, and each enabled zone profile or a written ISTD ruling; the merchant's accountant confirms how that authority applies to the merchant.

### 4.7.3 — Tender vs. PSP settlement
**Files:** `apps/server/src/reports/settlement.rs` (new)
Reconciliation by `psp_ref`, with unmatched PSP entries and unmatched tenders listed **separately** so the direction of the discrepancy is obvious (E.23).

### 4.7.4 — The fraud lens
**Files:** `apps/server/src/reports/exceptions.rs` (new)
Refunds and voids by user · price overrides by user with reason strings (E.33) · no-sale drawer opens by user (E.35) · training-mode transactions · over/short trend by cashier. Chronic short is a training issue or a theft issue, and the report is how you tell.
**Tests:** `override_report_groups_by_user_with_reasons`

### 4.7.5 — Inventory reports
**Files:** `apps/server/src/reports/inventory.rs` (new)
On-hand and valuation (Σ qty × WAC) · movement · negative stock · low stock · shrinkage by reason code.

### 4.7.6 — Loyalty and stored-value liability
**Files:** `apps/server/src/reports/loyalty.rs` (new)
Outstanding points × redemption value, **and** outstanding store-credit balances. An accountant will ask about both; both are balance-sheet items, and store credit is money owed to customers that has been accruing since Phase 2.

### 4.7.7 — Export and timezone discipline
**Files:** `apps/server/src/reports/mod.rs`
Every report exports CSV. **Every report buckets by store-local calendar day** from `business_date`, resolved through the store's IANA zone id rather than a stored offset, regardless of UTC storage.
**Tests:** `every_report_exports_csv_with_identical_numbers` · `reports_bucket_by_business_date_not_utc`

### 4.7.8 — Cash reconciliation, drawer to bank
**Files:** `apps/server/src/reports/cash.rs` (new)
Opening safe balance, drops in, deposits out, closing safe, expected against counted — the chain the shift-level over/short cannot see. A manager who takes a 300 JOD drop and never banks it produces no variance anywhere without this: the shift balances because the drop was recorded, and nothing reconciles to the bank statement. *"How much is in my safe?"* and *"did the bank get Thursday's deposit?"* are week-one questions.
**Tests:** `the_safe_balance_equals_its_movement_replay` · `a_bank_deposit_appears_as_in_transit_until_it_is_reconciled`

### 4.7.9 — Takings by hour and by cashier, across stores
**Files:** `apps/backoffice/src/pages/reports/`
The register's own day-so-far screen is Phase 2 (2.5.4); this is its back-office half — the same breakdown over any date range, across registers and across stores, with the store comparison from 4.8.2.

**"By cashier" carries the same caveat here as it does at the register**: over/short is a shift-level fact, so where a drawer is shared (decision 8.6) the column is by shift and its opener. The till/shift/register collapse is a deliberate simplification and a good one; what has to be written down is that cash accountability is per drawer-session and **not** per person.
**Tests:** `takings_by_hour_reconcile_to_the_days_z_reports`

### 4.7.10 — Catalogue data-quality exceptions
**Files:** `apps/server/src/reports/exceptions.rs`
Two live barcodes claiming the same code (E.36), products with no price in an active price list, products active with no tax category, and PLUs colliding across departments. The scan path already resolves a collision to the newest active code and warns; this is the other half — the back-office list of the codes a buyer has to actually fix, because a warning a cashier dismisses forty times a day is not a fix.

Not in the fraud lens (4.7.4): a duplicated barcode is almost always a relabelling mistake, and filing it beside refund abuse would train the reader to read one as the other.
**Tests:** `barcode_conflict_report_lists_both`

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

*This is the sharpest sequencing defect the audit found, and it is fixed by a gate rather than by a caveat.* The pilot put three real shops, real staff, real customers and real cards into trade **before** fiscal certification, before the PDPL determination, before the breach runbook existed, before the SAQ was determined, and before anything adversarial had been pointed at the system — while its own preparation step demanded merchant answers that the plan scheduled for Phase 5.

### 4.9.0 — The pre-pilot gate
**Files:** `docs/compliance/pre-pilot.md` (new)
**Nothing in group 4.9 starts until every row below is true and dated.** Each is work that exists elsewhere in this set; what was missing was the rule that it happens *first*.

| # | Must be true before a real customer transacts | Owner |
|---|---|---|
| 1 | **Fiscal posture settled per store** — one of the three options in 4.9.1, in writing, with the merchant's tax advisor named | 2.7.0, 4.9.1 |
| 2 | **Fiscal certification complete** for any store that will issue fiscal documents through this product | Phase 5, milestone 5.2, pulled forward |
| 3 | **PDPL roles determined and the register entry complete** — controller, processor, recipients, DPO applicability, and the electronic-register entry itself | 3.4.1 |
| 4 | **Hosting jurisdiction and transfer basis approved**, with signed contracts and a sub-processor list | 3.1.6 |
| 5 | **The breach runbook exists and has been exercised** — both statutory clocks, timed independently from discovery | 5.3.2, pulled forward |
| 6 | **The PCI SAQ is determined** by the acquirer and a QSA, and the store baseline matches it | 2.1.1, 5.3.3 |
| 7 | **An independent security assessment** has been performed and every critical and high finding fixed or accepted in writing | 5.4.1, pulled forward |
| 8 | **The retention matrix is agreed** with the accountant, with its legal-hold rule | 5.3.4 |
| 9 | **Server operations are real** — a tested restore, alerts that reach a person, and a stated on-call expectation the merchant has been told | group 3.10 |
| 10 | **The cashier guide exists in Arabic** and has been read by a cashier who is not you | 5.6.3, started at the Phase-1 gate |

**Done when:** `docs/compliance/pre-pilot.md` carries all ten with a date and a name against each. A breach, a subject request or a card dispute arriving before these exist is one the merchant cannot answer and the vendor caused.

> **A sole-author self-review is not row 7.** For a system holding customer, fiscal, licensing and fleet-update authority, independent adversarial testing before the first real-customer pilot is the minimum, not the budget-permitting option it used to be.

### 4.9.1 — Pilot preparation
**Depends on:** 4.9.0
**Files:** `docs/implementation/ref/merchant-decisions.md` (completed for each store), `docs/compliance/pre-pilot.md`
Three stores, real assortments, real staff, real customers. Every merchant decision answered per store — including the ones the plan used to schedule for Phase 5, which is why rows 6.6 and 6.7 of the questionnaire now point at pre-pilot owners. Backups verified on **both** destinations. Device health watched, and alerting configured to reach a person rather than a dashboard.

**The pilot's fiscal posture is stated, not implied.** Exactly one of these, per store, in writing:

1. the store is genuinely outside the mandate and runs `fiscal_profile = 'disabled'`, on dated evidence of its obligation status — not on an assumption from its GST registration, which is a separate axis;
2. the merchant continues issuing cleared invoices through their existing certified system for the pilot week, and this product issues no fiscal document; or
3. milestone 5.2 has been completed for that store, and it issues through this product.

There is no fourth option. A week of three-store trading is thousands of documents; if they are neither cleared nor lawfully absent, they are uncleared tax documents on a merchant who agreed to help — and the same merchant's goodwill is what milestone 5.2 depends on.

**Assortment arrives by import, not by typing.** A real minimarket is 1 000–2 500 SKUs with Arabic names, multiple barcodes and per-item tax categories the accountant must review; three stores of that entered through a CRUD form is weeks of unbudgeted work, and the realistic outcome is a truncated assortment that produces none of the surprises 4.9.3 exists to capture. Catalogue import therefore lands in the back office at 3.6.7, and the pilot reconciles the imported list against the merchant's own.

### 4.9.2 — The week
Run unattended for seven trading days. Log everything that surprises anyone.

### 4.9.3 — The debrief
**Files:** `docs/implementation/ref/test-catalog.md` (extend), `docs/drills/`
Every surprise becomes either a fixed bug, a new edge case with a test, an open question with a stated default, or an explicitly accepted risk with a rationale. **A surprise that becomes none of those is a surprise that will happen again.** The week itself is a drill and gets a dated record.

---

## Exit gate

```bash
just lint && just test
cargo nextest run --workspace -E 'test(prop_)'
just bench-gate                               # budgets still met with 50k SKUs and promotions active
```

By demonstration:

0. **The pre-pilot gate (4.9.0) is complete**, all ten rows dated and named, before demonstration 1 begins. It is numbered zero because it is a precondition, not an achievement.
1. **Three stores trade for a full week** with no intervention from you — on a server someone operates, with alerts that reach a person.
2. **A real promotion runs**, and its cost report matches finance's arithmetic — checked by hand. Then edit the promotion and re-run the report for last week: the numbers do not move, because the attribution names a version.
3. **Receive a delivery** with a supplier tax invoice; WAC updates on net plus non-deductible tax only; a deliberate 10× cost is caught before posting.
4. **Count a category during trading hours**; sales mid-count do not corrupt the variance.
5. **Transfer stock between two stores**, arriving short; the destination adjustment and source notification both appear.
6. **Change a price**; the label worklist populates; labels print in Arabic and are readable on a shelf.
7. **Attempt to save a controlled staple above its ceiling** — blocked. Attempt to sell above it — blocked. Attempt to sell a single cigarette, and to attach a tobacco product to a promotion — both blocked.
8. **Every report exports CSV**, buckets by the store's calendar day, and reconciles against the register's own X/Z for the same day. The margin report gives the same answer for January whether it is run in March or in June.
9. **Refund the same receipt at two stores** while connected — the second is refused. Repeat offline — both succeed, and the case appears in the refunds-by-user report.
10. **Automated tests exist** for E.42, E.43, E.44, E.45, E.61, E.63, and for the Phase-4 half of E.31, E.33, E.36, E.70, E.71 and E.74.

→ **Next:** [`phase-5-harden-and-launch.md`](phase-5-harden-and-launch.md)
