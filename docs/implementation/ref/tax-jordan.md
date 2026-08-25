# Tax — Jordan GST, as an engine

The tax engine must be right before the first real sale, because every later sale inherits its
rules and no completed sale can be corrected in place (conventions I-4). A merchant-specific tax
status or product classification comes from current ISTD evidence and the merchant's accountant,
not from assortment guesses or a software default.

Jurisdictional facts here are tied to the official sources in §7. Correction **C-4** is restated
below with its merchant categories corrected. Protocol and arithmetic questions that the public
material does not settle remain explicit open items rather than compliance claims.

---

## 1 · What the merchant's tax life looks like

General Sales Tax is a VAT administered by the **Income & Sales Tax Department (ISTD)**. A
registered merchant charges output tax on taxable sales, may deduct eligible input tax, and pays or
carries the resulting balance under the rules in the [General Sales Tax Law][gst-law]. The official
[GST declaration][gst-declaration] contains both sides of that calculation.

The Phase-1 `report_tax_by_rate(store, from, to)` output is therefore a **sales-side tax
reconciliation**, not a statutory return and not, by itself, the accountant's filing input. It
proves that immutable sales and credit documents add up by treatment and rate. It does not contain
supplier invoices, imports, input-tax deductibility, prior credit, adjustments, or the merchant's
refund/carry-forward elections. §6 states exactly what it can and cannot support.

---

## 2 · Rates and treatments

| Treatment | Rate source | Reclaimable input tax | Classification rule | Reported |
|---|---|---|---|---|
| **Standard** | 16% under Article 6 of the [GST Law][gst-law] | yes, subject to deductibility rules | the residual rate after an official schedule has been checked | yes |
| **Reduced** | effective-dated [ISTD rate catalogue][rate-catalogue] | yes, subject to deductibility rules | the exact tariff/category entry, confirmed for the merchant's product | yes |
| **Zero-rated** | Article 7 and the current official schedules | **yes**, subject to deductibility rules | either an inherently zero-rated category or an evidenced supply context from §3 | **yes** |
| **Exempt** | the current official exempt schedules | **no** for input tax attributable to exempt sales | the exact official category, confirmed for the merchant's product | yes, separately |

The current ISTD catalogue contains reduced bands beyond the old `RED04` seed. That list is not
compiled into the engine: Phase 1 imports every band actually used by the merchant from a pinned
catalogue snapshot, and the accountant confirms every enabled category. A band absent from that
store's approved pack is `TaxError::NoRuleInEffect`, not an invitation to substitute a familiar
rate.

> **Exempt ≠ zero-rated, and collapsing them is a filing error.** Both can charge the customer no
> GST. Zero-rated supplies can preserve input-tax recovery; exempt supplies do not. They remain
> distinct `TaxTreatment` variants, distinct `tax_category` rows, and distinct reconciliation rows.

### Special Sales Tax (excise)

Special Sales Tax is not a percentage-only extension. Article 6 of the [GST Law][gst-law] permits
specific amounts and ad-valorem rates, the current [Special Tax Regulation][sst-regulation]
contains collection-unit schedules, and Article 15 requires Special Tax to be added to the taxable
value before General Sales Tax is calculated.

Every enabled component rule therefore carries enough data to calculate and audit either shape:

| Field | Rule | Why |
|---|---|---|
| `calculation_kind` | `ad_valorem` or `fixed_per_quantity` | a percentage cannot represent a statutory fils-per-unit charge |
| `rate_ppm` | present only for `ad_valorem` | keeps percentage math integer and effective-dated |
| `fixed_amount_minor` | present only for `fixed_per_quantity` | keeps a specific tax in signed minor units |
| `fixed_currency` | present only for `fixed_per_quantity` | prevents applying a fils amount under the wrong currency exponent |
| `charge_unit_code` | official schedule's taxable unit kind | prevents treating a litre, package, line, or subscription as interchangeable |
| `fixed_basis_qty_milli` | milli-units of that unit represented by one fixed charge | makes the quantity basis explicit without floating-point conversion |
| `calculation_order` | deterministic component order | makes tax-on-tax reproducible |
| `base_kind` | `line_net`, `line_net_plus_prior_components`, or `quantity` | prevents two components from silently using different taxable values |
| `depends_on_component_codes` | components whose carried amounts enter this component's base | represents General Tax calculated on value plus Special Tax |
| `source_ref` / effective dates | pinned official schedule and period | a rate change must not rewrite historical sales |

The sale snapshot persists each component's `calculation_kind`, `taxable_base_minor`, `rate_ppm` or
`fixed_amount_minor`, `fixed_currency`, `charge_unit_code`, `fixed_basis_qty_milli`,
`quantity_basis_milli`, `calculation_order`, `base_kind`, `depends_on_component_codes`, and resulting
`tax_minor`. Calculation follows a validated dependency graph; a missing dependency or cycle is an
error. No current telecom, tobacco, fuel, or other Special Tax value is seeded from prose in this
document.

v1 hard-blocks checkout for a Special-Tax-liable product or `special_sales` taxpayer profile until
the component engine, the complete current official schedule, merchant tax-point evidence, and
fixed/ad-valorem golden fixtures all exist. This prevents an ordinary reseller from charging a tax
that was already borne upstream, and prevents a producer or importer from omitting a fixed
component that the percentage-only engine cannot represent.

### Store tax profile

A store's jurisdiction changes its tax rules. The profile is first-class and fail-closed:

| `store.tax_profile` | Behaviour |
|---|---|
| `standard` | resolves only the approved standard-Jordan jurisdiction pack |
| `asez` | resolves only a complete, effective-dated ASEZ pack with its own return mapping; otherwise `TaxError::ProfilePackIncomplete` |
| `development_area` | resolves only a complete, effective-dated development-area pack with its own return mapping; otherwise `TaxError::ProfilePackIncomplete` |
| `unregistered` | charges no GST; it does **not** decide JoFotara obligation |

An unscoped rule belongs to `standard` only. It never falls through into `asez` or
`development_area`. The official [ASEZA declaration][aseza-declaration] has its own rows and the
[ASEZA regulation][aseza-regulation] has zone-specific tax and period rules; silently inheriting a
generic 16% pack would make both the customer charge and the return mapping wrong. Development-area
support follows the same completeness rule even when no merchant currently uses it.

GST registration and national e-invoicing are separate axes:

| Decision | Evidence | Controls |
|---|---|---|
| GST registration/profile | ISTD registration certificate, activity and jurisdiction | whether and how GST components are calculated |
| JoFotara obligation | merchant-specific obligation or exemption evidence | whether `store.fiscal_profile` is enabled |
| JoFotara taxpayer category | the merchant's ISTD registration/profile evidence | `income`, `general_sales`, or `special_sales` document composition |

An `unregistered` GST profile can still require an income invoice. Conversely, fiscal issuance is
disabled only from recorded merchant-specific obligation/exemption evidence, never because
`store.tax_profile = 'unregistered'`. Enabling or disabling JoFotara does not backfill or erase
historical documents.

### Registration thresholds — correction **C-4**

The [Registration Threshold Regulation][threshold-regulation] attaches the amounts to the
merchant's activity, not to whatever happens to be on a retailer's shelf:

| Registered activity | Threshold |
|---|---:|
| Producer/manufacturer of goods subject to Special Sales Tax | **JOD 10,000** |
| Seller of goods other than that producer/manufacturer category | **JOD 75,000** |
| Service supplier | **JOD 30,000** |
| More than one activity | the lowest threshold applicable to any of those activities |

An ordinary tobacco reseller is not moved to JOD 10,000 merely because tobacco is in the
assortment. Article 13 of the [GST Law][gst-law] also requires the assessment to cover forecast
turnover, rolling actual turnover, and a first taxable import rather than checking only the last 12
months after the fact.

Onboarding records the registered activity class, producer/manufacturer role, importer role,
Special Tax certificate or designated tax point where applicable, mixed activities, trailing and
forecast taxable turnover, and first taxable import. The merchant's registration certificate and
accountant decision set `store.tax_profile`; the POS does not infer registration or Special Tax
liability from product categories.

---

## 3 · Rates are data, with effective dates

Jordanian category and rate schedules change. A rate compiled into code is a release cycle every
time the official schedule changes, at the merchant's expense.

```sql
tax_rate(tax_category_id, component_code, treatment, calculation_kind,
         rate_ppm, fixed_amount_minor, fixed_currency,
         charge_unit_code, fixed_basis_qty_milli,
         calculation_order, base_kind,
         valid_from, valid_to, profile_scope)
tax_rate_dependency(tax_rate_id, depends_on_component_code)
```

Resolution rules, implemented in `resolve_components` ([`domain-api.md`](domain-api.md) §5) and
property-tested:

1. `valid_from` is **inclusive** and `valid_to` is **exclusive**. A change at midnight has no
   ambiguous instant.
2. Exactly one rule per `(category, component, profile)` may be in effect at a timestamp. Overlap is
   `TaxError::OverlappingRules` and is caught before publication to a register.
3. `profile_scope = NULL` means `standard`, not every profile. `asez` and `development_area`
   require explicit rules from their own complete jurisdiction pack.
4. No applicable rule is `TaxError::NoRuleInEffect`. An incomplete zone pack is
   `TaxError::ProfilePackIncomplete`. The engine never guesses 16%.
5. Resolution happens at sale time and the resolved calculation facts are copied onto
   `sale_line_tax` (conventions I-5). A later refund reads those immutable facts, not the current
   schedule.
6. An enabled jurisdiction pack includes a source version/hash, effective dates, every category and
   component used by the assortment, the return-box mapping, the filing calendar, and dated
   accountant approval. Missing any one of those items keeps the profile disabled.

Rates use **parts-per-million**: 16% = `160_000`, 4% = `40_000`, 0.5% = `5_000`. Specific Special
Tax amounts remain `i64` minor units and never pass through a percentage.

Zero-rating can depend on the supply, not merely the SKU. A transaction that relies on export,
free-zone destination, or eligible-body status carries an immutable `SupplyTaxContext` containing
at least `destination_code`, `zero_rate_reason_code`, `eligible_entity_authority`, and `evidence_ref`.
The context and resolved return-box reason are copied onto the sale. The same catalog item can therefore
be standard-rated in one sale and zero-rated in another without changing the product. Until this
context exists, those supply types fail closed; `ZERO` is not a shortcut for every future sale of
the product.

---

## 4 · Inclusive pricing — the default, and the arithmetic

Jordanian retail shelf prices are represented as tax-inclusive for the supported merchant
configuration. The engine extracts the included components; it does not add them again.

```text
Inclusive, one component: net = gross / (1 + r)    tax = gross - net
Exclusive, one component: tax = net * r            gross = net + tax
```

Intermediate math uses `rust_decimal`; money returns to signed `i64` minor units at the single
jurisdiction-defined rounding point. For a simple 16% inclusive line priced at 1.250 JOD:

```text
gross = 1250
net   = 1250 / 1.16 = 1077.5862068965517... -> round -> 1078
tax   = 1250 - 1078                            = 172
check: 1078 + 172 = 1250
```

For one inclusive component, tax is the residual (`gross - net`), never an independently rounded
second calculation. Multi-component and fixed-component lines use the ordered component model in
§2 and still conserve the carried gross exactly.

### Rounding rule

Tax rounding is a versioned jurisdiction policy, not an arbitrary store preference. The policy
names its decimal scale, tie rule, component order, and effective period; a sale snapshots the
policy version so a later configuration change cannot alter it.

> ⚠️ **OPEN — blocks 2.7.0.** What line scale and tie rule do the current ISTD arithmetic rules require for each supported component and inclusive/exclusive calculation? Default until answered: `HalfAwayFromZero`, once per line, under one versioned Jordan policy; the default remains provisional and cannot be frozen into fiscal goldens.
> Owner: 2.7.0. Source that settles it: the current official ISTD Technical Integration Guide, XSD/business-rule package, or written ISTD clarification with accepted boundary vectors.

### The receipt tax summary

The summary is the exact sum of the carried line components, grouped by `(component, treatment,
rate, supply_reason)`:

```text
summary_row.net_minor = sum(carried line-component net_minor)
summary_row.tax_minor = sum(carried line-component tax_minor)
summary_row.gross_minor = sum(carried line-component gross_minor)
```

It is never re-derived from the receipt total. Exact identities over the document's carried values
are what keep the receipt, fiscal document, and sales reconciliation consistent.

---

## 5 · Cash rounding is not tax rounding

Tax rounding determines immutable line and tax facts. Cash rounding is a settlement operation on
the final cash remainder. The configured step is an operational merchant choice, not a claim about
legal tender or tax law; `10` fils is the provisional merchant default and is confirmed during
provisioning.

Rules:

1. `store.cash_round_step_minor` and direction are explicit, effective-dated settings.
2. A sale rounds only the remaining amount when the final tender is cash. A card tender remains the
   exact unrounded amount.
3. The signed difference is persisted as `sale.rounding_adj_minor` and printed as its own receipt
   line, so `sum(tenders) - change == total_minor + rounding_adj_minor`.
4. Under the provisional default, it does not mutate a line, a line tax component, or the tax
   summary. That tax/fiscal treatment remains open below.
5. The `exchange` tender is never cash-counted and never receives cash rounding.

> ⚠️ **OPEN — blocks 2.7.0.** Does a collected or paid cash-rounding adjustment change taxable consideration, a JoFotara monetary total, or a return box, and how must the adjustment be represented? Default until answered: treat it only as a signed tender-level adjustment; leave line tax, tax summary, and fiscal tax totals unchanged.
> Owner: 2.7.0. Source that settles it: the current official ISTD business-rule package or a written ISTD ruling covering both positive and negative cash-rounding adjustments.

A cash **refund payout** also needs an amount that can be physically paid. The refund document first
reconstructs the immutable line/tax value. If cash is the payout route, the payout is then rounded
to the configured cash step, with a separate signed `rounding_adj_minor` on the refund; the line and
tax facts remain unchanged. The provisional direction rounds in the customer's favour so a drawer
cannot retain an unrecorded remainder.

> ⚠️ **OPEN — blocks 2.3.3.** What payout direction, customer disclosure, and tax/fiscal treatment apply when a cash refund is not divisible by the configured coin step? Default until answered: round the cash payout in the customer's favour, persist and print the signed refund adjustment, and never alter the credited line or tax facts.
> Owner: 2.3.3. Source that settles it: current ISTD cash/credit-note rules plus written Jordanian consumer and tax counsel advice for the merchant's refund policy.

Properties: `prop_rounding_adjustment_keeps_total_exact` for collections and
`prop_refund_rounding_keeps_expected_cash_exact` for payouts.

---

## 6 · The filing report

### Sales-side tax reconciliation

`report_tax_by_rate(store, from, to)` is the Phase-1 sales reconciliation:

| Column | Source |
|---|---|
| Component | `sale_tax_summary.component_code` |
| Treatment | `standard` / `reduced` / `zero` / `exempt`, kept distinct |
| Rate | `rate_ppm`, rendered as a percentage |
| Supply reason | the immutable `SupplyTaxContext` reason, when present |
| Net | sum of carried `net_minor` |
| Tax | sum of carried `tax_minor` |
| Gross | sum of carried `gross_minor` |
| Documents | count of sales and credit/refund documents separately |

It:

- buckets by the store-local `business_date`, never by a UTC timestamp;
- includes credit/refund documents as negatives on their own issue date while retaining their
  original-document and original-period lineage;
- excludes training sales and reports the excluded count;
- exports the same carried values to CSV; and
- reconciles to the fil against the committed hand-checked fixture.

An arbitrary `from`/`to` range is useful for reconciliation only. It does not select a statutory
tax period, determine a due date, or decide which return box receives a later credit note.

### Full return workpaper

The official [GST declaration][gst-declaration] also requires facts this sales report does not own:

| Required area | Facts that must exist before a full workpaper is claimed |
|---|---|
| Prior position | credit brought forward and prior-period adjustments |
| Domestic inputs | supplier invoice net/tax/gross by component and rate, expenses, assets, supplier credits |
| Cross-border inputs | imports, deferred imports, imported services, customs references |
| Deductibility | deductible, nondeductible, and common-input allocation class |
| Output mapping | domestic standard/reduced, zero-rated by reason, exports, exempt and non-taxable sales |
| Adjustments | taxpayer/Department adjustments and later-period credit-note disposition |
| Election | refund claim, carry-forward, and supporting invoice details |

Phase 4 receiving must persist those supplier and import facts before the server calls anything a
filing workpaper. Weighted-average cost includes net cost plus **nondeductible** input tax only;
deductible input tax is a recoverable tax asset, not inventory cost. A mixed taxable/exempt merchant
also needs an accountant-approved common-input apportionment policy and evidence.

### Filing calendar and later-period credit notes

Articles 16 and 19 of the [GST Law][gst-law] and the current [return filing
manuals][return-manuals] supply the General Tax and Special Tax period, boundary, nil-return, filing,
and payment rules. The standard evidence pack records the assigned cycle rather than inferring it
from an arbitrary report range. ASEZA has its own [regulation][aseza-regulation] and
[declaration][aseza-declaration], so its calendar and box mapping come from the enabled jurisdiction
pack rather than from the standard profile.

Persist a `TaxReturnCalendar` with `taxpayer_number`, `return_type`, `jurisdiction_profile`,
`assigned_cycle_code`, `period_start_date`, `period_end_date`, `due_date`, `filing_status`,
`is_nil_required`, `nil_return_status`, `filed_at`, and the evidence/version that assigned the period.
A required nil return remains visibly due until filing evidence changes that status; arbitrary report
dates never substitute for it.

A credit note stores both `original_invoice_id` / `original_period_id` and
`credit_note_period_id`. Its statutory `box_disposition` remains an explicit reviewed value; the
system does not silently subtract it from whichever report range the operator happened to request.

> ⚠️ **OPEN — blocks 4.7.2.** Which return period and box must receive a credit note issued after the original invoice's filed period for each supported return type and jurisdiction? Default until answered: show the credit as a negative in sales reconciliation on the credit-note date, preserve the original and credit periods, and leave statutory `box_disposition` unresolved rather than auto-populating a return.
> Owner: `4.7.2`. Source that settles it: the current official ISTD credit-note return instructions for General Tax, Special Tax, and each enabled zone profile or a written ISTD ruling; the merchant's accountant confirms how that authority applies to the merchant.

---

## 7 · Seeded tax data

Migration `0003` and Phase-1 microstep 1.3.7 create the tax tables and the restricted configuration
path. Seed data is a starting point, never a product-classification opinion:

| `tax_category.code` | Treatment | Seed rule |
|---|---|---|
| `STD16` | standard | standard-profile 16% rule from Article 6 of the [GST Law][gst-law] |
| `REDUCED_<rate_ppm>` | reduced | created only for a merchant-used band imported from the pinned [ISTD catalogue][rate-catalogue] and approved by the accountant |
| `ZERO` | zero | enabled only for an official inherent category or a valid `SupplyTaxContext` reason |
| `EXEMPT` | exempt | mapped only to a current official exempt category approved by the accountant |

Each imported rule records the official source/version/hash, its real legal effective dates, and
the dated accountant approval. `valid_from` is not invented from the store's go-live date. Unknown
categories and unconfigured reduced bands fail closed. The Phase-1 fixture covers every band the
merchant enables; it does not pretend one hard-coded `RED04` row is Jordan's complete reduced-rate
catalogue.

ASEZ and development-area rules are not generic seeds. A profile becomes selectable only after its
complete jurisdiction pack and return mapping pass the §3 completeness check. Special Tax values
are not seeded until the component model in §2, a pinned current [Special Tax
Regulation][sst-regulation], and the merchant's producer/importer/tax-point evidence are all present.

> **Product classification belongs to the merchant's accountant.** A wrong category changes the
> customer's charge, the input-tax position, and the return box. Import the official schedule,
> review the actual assortment, and retain the approval evidence before trading.

### Authoritative sources

| Source | Settles |
|---|---|
| [General Sales Tax Law and amendments][gst-law] | standard rate, specific/ad-valorem Special Tax, tax point, GST-on-SST base, registration triggers, periods, deductibility |
| [Registration Threshold Regulation][threshold-regulation] | activity classes and 75k / 30k / 10k thresholds |
| [ISTD tax-rate catalogue][rate-catalogue] | current product/category rates; must be pinned because it changes |
| [Official GST declaration][gst-declaration] and [return filing manuals][return-manuals] | sales, input-tax, adjustment, refund, carry-forward, period, and filing requirements |
| [Special Tax Regulation][sst-regulation] | current Special Tax collection units, rates and amounts; must be pinned before use |
| [ASEZA Regulation 54/2005 as amended][aseza-regulation] and [ASEZA declaration][aseza-declaration] | zone-specific tax, periods and return rows |

---

## 8 · Tests that must exist before the first real sale

| Test | Guards |
|---|---|
| `inclusive_16pct_extracts_exactly` | the worked example in §4, to the fil |
| `prop_inclusive_net_plus_tax_equals_gross` | the residual rule, for every supported rate and amount |
| `prop_line_tax_sum_equals_receipt_tax` | summary is the exact sum, never re-derived |
| `prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting` | the distinction survives to reconciliation |
| `prop_tax_never_exceeds_gross` | invalid inclusive component combinations fail rather than create impossible carried values |
| `prop_rate_resolution_is_deterministic_at_boundaries` | inclusive/exclusive effective dates; no gap or overlap |
| `unconfigured_reduced_band_fails_closed` | the engine never substitutes a familiar rate |
| `asez_profile_without_complete_pack_fails_closed` | no generic standard-rate fallthrough |
| `an_incomplete_profile_pack_fails_closed` | ASEZ and development-area profiles refuse any incomplete jurisdiction pack |
| `scoped_rule_overrides_unscoped` | every complete zone pack resolves only its pinned scoped rule at effective-date boundaries |
| `prop_unregistered_profile_yields_no_tax` | the unregistered GST engine emits no GST while fiscal obligation remains independent |
| `unregistered_gst_profile_does_not_disable_fiscal_obligation` | GST and JoFotara axes stay independent |
| `fixture_covers_every_enabled_band` | the evidenced pack fixture covers the merchant's producer/reseller and mixed-activity thresholds without substituting an assortment guess |
| `imported_rule_records_source_version_hash_and_approval` | forecast and first-import registration reviews, when applicable, remain pinned onboarding evidence rather than hard-coded conclusions |
| `scoped_rule_overrides_unscoped` | export/free-zone treatment is supply-specific |
| `prop_multi_component_line_sums_correctly` | more than one component conserves the carried gross |
| `sst_fixed_and_ad_valorem_components_compound_in_order` | both Special Tax shapes are representable |
| `gst_base_includes_sst` | Article 15 dependency is not omitted |
| `tax_component_dependency_cycle_is_refused` | a malformed pack cannot produce an order-dependent taxable base |
| `prop_rounding_adjustment_keeps_total_exact` | cash collection rounding never breaks settlement |
| `prop_refund_rounding_keeps_expected_cash_exact` | physical payout and refund ledger agree |
| `prop_refund_uses_original_rate` | a later credit reads immutable original tax facts |
| `a_refund_in_the_next_period_preserves_both_period_references` | original and credit-note filing lineage survives |
| `a_nil_period_still_produces_a_return_row` | an empty sales range does not hide a filing obligation |
| `sales_reconciliation_does_not_claim_full_return` | missing input-tax/return fields stay explicit |
| `tax_report_matches_hand_check_fixture` | every enabled rate, supply reason, refund, and cash adjustment reconciles on paper |

The hand-checked fixture proves only the merchant's enabled sales-side configuration. A full-return
fixture is owned by Phase 4 after supplier invoices, imports, deductibility, apportionment,
adjustments, and filing calendars exist.

[gst-law]: https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/general_sales_tax_law_and_its_amendments_2023-1.pdf
[threshold-regulation]: https://istd.gov.jo/EBV4.0/Root_Storage/AR/EB_Legislation/%D9%86%D8%B8%D8%A7%D9%85_%D8%AD%D8%AF_%D8%A7%D9%84%D8%AA%D8%B3%D8%AC%D9%8A%D9%84_%D9%84%D8%BA%D8%A7%D9%8A%D8%A7%D8%AA_%D8%A7%D9%84%D8%B6%D8%B1%D9%8A%D8%A8%D8%A9_%D8%A7%D9%84%D8%B9%D8%A7%D9%85%D8%A9_%D8%B9%D9%84%D9%89_%D8%A7%D9%84%D9%85%D8%A8%D9%8A%D8%B9%D8%A7%D8%AA_%D8%B1%D9%82%D9%85_81_%D9%84%D8%B3%D9%86%D8%A9_2000_%D9%88%D8%AA%D8%B9%D8%AF%D9%8A%D9%84%D8%A7%D8%AA%D9%87.pdf
[rate-catalogue]: https://istd.gov.jo/AR/List/%D8%A7%D9%84%D9%86%D8%B3%D8%A8_%D8%A7%D9%84%D8%B6%D8%B1%D9%8A%D8%A8%D9%8A%D8%A9
[gst-declaration]: https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/gst_declaration-1.pdf
[return-manuals]: https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/tax_returns_filling_manuals.pdf
[sst-regulation]: https://istd.gov.jo/EBV4.0/Root_Storage/AR/Regulations/KTA_Document_%2839%29.pdf
[aseza-regulation]: https://aseza.jo/EBV4.0/Root_Storage/EN/EB_List_Page/Regulation_no_54_of_2005_for_the_Goods_and_Services_Sales_Tax_in_Aqaba_Special_Economic_Zone_%28ASEZA%29_as_amended.pdf
[aseza-declaration]: https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/aqaba_gst_declaration-0.pdf
