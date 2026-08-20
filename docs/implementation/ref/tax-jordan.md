# Tax — Jordan GST, as an engine

The tax engine must be right before the first real sale, because every sale after it inherits the error and no sale before it can be corrected in place (conventions I-4).

Jurisdictional facts here were verified in [`plan-validation.md`](plan-validation.md); one correction (**C-4**, registration thresholds) originates in this area.

---

## 1 · What the merchant's tax life looks like

General Sales Tax is a VAT administered by the **Income & Sales Tax Department (ISTD)**. The merchant charges it on sales, reclaims what they paid on inputs, and remits the difference. Returns are periodic — bi-monthly for general tax.

**This means the tax report by rate for a date range *is* the accountant's filing input.** It is not a nice-to-have report; it is the reason the engine exists. Design it as a deliverable, not a query someone might write later (§6).

---

## 2 · Rates and treatments

| Treatment | Rate | Reclaimable input tax | Examples | Reported |
|---|---|---|---|---|
| **Standard** | 16% | yes | most goods and services | yes |
| **Reduced** | 1 / 2 / 4 / 5 / 10% by Cabinet resolution | yes | hygiene products, salt, oils, corn, live animals, cheese *(reported examples — verify per item)* | yes |
| **Zero-rated** | 0% | **yes** | exports; supplies to free zones and markets; ASEZ; development areas | **yes** |
| **Exempt** | none | **no** | bread; water in packs < 5 L; tea; sugar; gold; currency; electricity. Services: air transport, education, sewage & waste disposal, public health, religious organisations, social organisations | yes, separately |

> **Exempt ≠ zero-rated, and collapsing them is a filing error.** Both charge the customer nothing. Zero-rated supplies let the merchant reclaim input tax; exempt supplies do not. They must be distinct `TaxTreatment` variants, distinct `tax_category` rows, and distinct lines on the filing report. The master plan is right to insist on this and it is the single most commonly botched thing in small POS systems.

### Special Sales Tax (excise)

An **additional** per-item tax on specific goods — cement, tobacco, wines, spirits, cars, beer, fuel, lubricants; telecom services at 24%. If the merchant sells any of these, the line carries **two** tax components, not one.

v1 ships GST only. The schema (`sale_line_tax` is a child table, not columns) and the domain (`Vec<TaxComponent>`, not `Option<Tax>`) both allow more than one component from day one, so enabling special tax is **data plus a rate rule**, never an engine migration. This also matters for JoFotara: the `special_sales` invoice category expects "one percentage tax plus one fixed tax per line."

### Store tax profile

A store's location changes its whole tax behaviour (master plan B.1). Not a hack — a first-class store-level profile:

| `store.tax_profile` | Behaviour |
|---|---|
| `standard` | normal rates |
| `asez` | Aqaba Special Economic Zone — special treatment; rate rules scoped to this profile |
| `development_area` | as above |
| `unregistered` | **charges no GST at all** — below the registration threshold |

`unregistered` is a supported configuration, not an error state. A micro-merchant below threshold must be able to use this product legally, with receipts that carry no tax lines and no fiscal QR (`fiscal_profile = 'disabled'`).

### Registration thresholds — correction **C-4**

Over any rolling 12-month period:

| Supply | Threshold |
|---|---|
| Goods not subject to Special Sales Tax | **JOD 75,000** |
| Services | **JOD 30,000** |
| Goods subject to Special Sales Tax | **JOD 10,000** |

The master plan stated ~50,000 for goods and omitted the special-tax tier. The tier matters: a minimarket selling tobacco crosses at JOD 10,000, not 75,000 — a very different conversation about whether the merchant must register at all.

This is documentation and a seeded default (merchant decision #10). No code branches on it; the engine only ever reads `store.tax_profile`.

---

## 3 · Rates are data, with effective dates

Jordanian reduced rates change by Cabinet resolution. A rate compiled into code is a release cycle every time a decree lands, at the merchant's expense.

```sql
tax_rate(tax_category_id, component_code, treatment, rate_ppm,
         valid_from, valid_to, profile_scope)
```

Resolution rules, implemented in `resolve_components` ([`domain-api.md`](domain-api.md) §5) and property-tested:

1. `valid_from` **inclusive**, `valid_to` **exclusive**. A rate change at midnight has no ambiguous instant.
2. Exactly one rule per `(category, component, profile)` may be in effect at any timestamp. Overlap is `TaxError::OverlappingRules` and is caught at back-office save time, not at the register.
3. A rule with `profile_scope = NULL` applies to every profile; a scoped rule overrides it.
4. No rule in effect is `TaxError::NoRuleInEffect` — a loud failure. The engine never guesses 16%.
5. **Resolution happens at sale time and the resolved rate is copied onto `sale_line_tax.rate_ppm`** (conventions I-5). A refund six months after a rate change automatically uses the rate the customer actually paid (E.34).

Rates are stored in **parts-per-million** so 1%, 2%, and any future fractional decree are representable: 16% = `160_000`, 4% = `40_000`, 0.5% = `5_000`.

---

## 4 · Inclusive pricing — the default, and the arithmetic

Jordanian retail shelf prices **contain** the tax. The engine *extracts* it; it does not add it.

```
Inclusive (default):   net = gross / (1 + r)        tax = gross − net
Exclusive:             tax = net × r                gross = net + tax
```

Both are computed in `rust_decimal` and rounded **once per line**, to fils, by the store's `rounding_rule`.

Worked example — 16% inclusive, shelf price 1.250 JOD (1250 fils):

```
gross = 1250
net   = 1250 / 1.16 = 1077.5862068965517…  → round → 1078
tax   = 1250 − 1078                        = 172
check: 1078 + 172 = 1250 ✓
```

**Tax is computed as the residual (`gross − net`), never independently.** If both were rounded separately you could produce `1078 + 173 = 1251` and the receipt would not add up. This is a one-line decision with a large blast radius; property `prop_inclusive_net_plus_tax_equals_gross` guards it forever.

### Rounding rule

Default **half away from zero**, at line level. The blueprint suggests banker's rounding; regional retail practice and — more importantly — the arithmetic a merchant's accountant does by hand both expect half-away-from-zero. It is a store setting (`store.rounding_rule`) either way, but the default nobody changes should be the one that matches their hand-check.

### The receipt tax summary

**The exact sum of the line taxes.** Never re-derived from the total. Grouped by `(component, treatment, rate)`, each row carrying net, tax, and gross.

```
summary_row.tax = Σ (line_tax.tax for lines at that rate)
```

Re-deriving the summary from the receipt total is how JoFotara total checks fail (master plan C.3), and it is why `sale_tax_summary` is a stored table rather than a view.

---

## 5 · Cash rounding is not tax rounding

Two different things that both round, and confusing them corrupts the books.

- **Tax rounding** happens per line, produces the numbers on the receipt and in the fiscal document.
- **Cash rounding** (master plan B.5) happens once, at tender time, only when the **final** tender is cash, and only on the remaining cash amount.

Jordan's smallest coin in everyday circulation is effectively **1 qirsh = 10 fils** (5-fils pieces are rare). So a 1.247 JOD total, paid in cash, is collected as 1.250.

Rules:

1. Configurable step (`store.cash_round_step_minor`, default `10`) and direction (default `nearest`).
2. Applies **only** to the remaining amount when the last tender is cash. A split cash+card sale rounds only the cash remainder (E.14). Card is charged the exact unrounded amount.
3. The difference is recorded as an explicit field (`sale.rounding_adj_minor`) and printed as its own receipt line, **so the books and the fiscal totals still reconcile exactly.**
4. It never changes any line, any tax, or the tax summary. It is a tender-level adjustment.
5. Verify the store's actual coin practice with the merchant (merchant decision #2) — some accept 5 fils, some round to 25.

Property: `prop_rounding_adjustment_keeps_total_exact` — `Σ tenders − change == total + rounding_adj`, for every combination.

---

## 6 · The filing report

The deliverable this whole engine exists for. `report_tax_by_rate(store, from, to)`:

| Column | Source |
|---|---|
| Component | `sale_tax_summary.component_code` |
| Treatment | `standard` / `reduced` / `zero` / `exempt`, kept **distinct** |
| Rate | `rate_ppm`, rendered as a percentage |
| Net | Σ `net_minor` |
| Tax | Σ `tax_minor` |
| Gross | Σ `gross_minor` |
| Documents | count of sales and of refunds separately |

Requirements that make it usable rather than merely correct:

- **Buckets by store-local calendar day** (`Asia/Amman`), from `business_date`, not from UTC timestamps (conventions §11).
- **Refunds appear as negatives in the same rows**, not a separate report. The accountant files a net figure.
- **Training-mode sales are excluded**, and the report says so with a count, so the exclusion is visible rather than assumed.
- **Exports to CSV** with the same numbers, because that is what actually reaches the accountant.
- **Reconciles to the fils against a hand-check** of a scripted day. This is Phase 1's exit criterion (master plan Part G) and it is worth doing literally: print the report, add the receipts up by hand, compare.

---

## 7 · Seeded tax data

Migration `0002` seeds the categories; `phase-1` microstep 1.3.7 seeds the rules. These are **starting defaults for a Jordanian minimarket**, to be confirmed with the merchant's accountant before trading.

| `tax_category.code` | Treatment | Rate | Typical assortment |
|---|---|---|---|
| `STD16` | standard | 16% | most goods |
| `RED04` | reduced | 4% | reduced-rate staples *(verify per item)* |
| `ZERO` | zero | 0% | exports, free-zone supplies |
| `EXEMPT` | exempt | — | bread, water < 5 L, tea, sugar, gold, electricity |

Every seeded rule carries `valid_from` set to the store's go-live date and `valid_to = NULL`. When a decree lands, the back office closes the old rule (`valid_to`) and inserts a new one — the engine needs no change and historical sales keep their historical rates.

> ⚠️ **Which product sits in which category is the merchant's accountant's decision, not this document's.** The reduced-rate list circulates in secondary sources with variations; a wrong category is a filing error the merchant pays for. Seed the categories, seed a defensible default per product, and put the review on the pre-launch checklist (merchant decision #10).

---

## 8 · Tests that must exist before the first real sale

| Test | Guards |
|---|---|
| `inclusive_16pct_extracts_exactly` | the worked example in §4, to the fil |
| `prop_inclusive_net_plus_tax_equals_gross` | the residual rule, for every rate and amount |
| `prop_line_tax_sum_equals_receipt_tax` | summary is the exact sum, never re-derived |
| `prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting` | the distinction survives all the way to the report |
| `prop_tax_never_exceeds_gross` | |
| `prop_rate_resolution_is_deterministic_at_boundaries` | `valid_from` inclusive / `valid_to` exclusive; no gap, no overlap |
| `prop_unregistered_profile_yields_no_tax` | the tax-disabled merchant configuration |
| `prop_multi_component_line_sums_correctly` | special sales tax, before anyone needs it |
| `prop_rounding_adjustment_keeps_total_exact` | cash rounding never breaks the books |
| `prop_refund_uses_original_rate` | E.34, after a rate change |
| `tax_report_matches_hand_check_fixture` | a scripted trading day, totalled by hand, committed as a fixture |

The last one is the only test that proves the *product* is right rather than the *code*. Write it against the Jordanian minimarket seed fixture (gap G-10) with mixed rates, a weighed item, a discount, a refund, and a cash-rounded tender — then check the arithmetic on paper once, and let CI defend it forever.
