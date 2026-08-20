# Merchant decisions — the questionnaire

Master plan Parts H and J.5, turned into a form you fill in **with the merchant, before coding the feature that depends on it.**

Every row has a **default** you can ship with, so no decision blocks the build. Every row also names **where the answer lives** — almost always a `setting` row, occasionally a column — and **which microstep consumes it**.

Print this. Sit with the merchant. Fill it in. Date it. A decision made by a developer at 2 a.m. is a decision the merchant will dispute in month three.

---

## How to use

| | |
|---|---|
| **Store** | ________________________ |
| **Completed with** | ________________________ (name, role) |
| **Date** | ________________________ |
| **Reviewed by** | ________________________ (accountant / tax advisor, where marked ⚖) |

⚖ = needs a professional, not the shop owner. Do not accept "it's probably fine."

---

## A · Returns and refunds

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 1.1 | Return window, in days? | **14** | | `refund_policy.window_days` | 2.3.1 |
| 1.2 | Receiptless returns allowed at all? | **no** | | `.allow_receiptless` | 2.3.5 |
| 1.3 | If yes — maximum value? | — | | `.receiptless_max_minor` | 2.3.5 |
| 1.4 | If yes — store credit only? | **yes** | | `.receiptless_store_credit_only` | 2.3.5 |
| 1.5 | Refund above which amount needs a manager? | **20.000 JOD** | | `.escalate_above_minor` | 2.3.8 |
| 1.6 | Cash refund for a card sale — ever allowed? | **no** | | `.allow_cash_for_card` | 2.3.3 |
| 1.7 | If yes — ceiling? | — | | `.cash_for_card_max_minor` | 2.3.3 |
| 1.8 | May a manager approve their own transaction? | **no** | | `.ban_self_approval` | 1.6.4 |

> 1.6 is a money-laundering vector, which is why the default is off and the capability is separate. If the merchant wants it, make sure they understand *why* it is separate.

---

## B · Money and rounding

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 2.1 | Cash rounding step | **10 fils (1 qirsh)** | | `store.cash_round_step_minor` | 1.5.3 |
| 2.2 | Rounding direction | **nearest** | | `.cash_round_direction` | 1.5.3 |
| 2.3 | Shelf prices shown to 2 or 3 decimals? | **3** | | `.money_decimals` | 1.1.2 |
| 2.4 | Tax-inclusive or tax-exclusive shelf prices? | **inclusive** | | `.price_mode` | 1.3.4 |
| 2.5 | Rounding rule for tax | **half away from zero** | | `.rounding_rule` | 1.1.6 |

> **Verify 2.1 against the store's actual coin drawer.** Some accept 5 fils; some round to 25. The default reflects that 5-fils pieces are rare in everyday circulation, not that they do not exist.

---

## C · Discounts and price control

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 3.1 | Maximum manual discount — cashier | **5%** | | `role_capability.limit_json` | 1.4.5 |
| 3.2 | Maximum manual discount — shift lead | **15%** | | same | 1.4.5 |
| 3.3 | Maximum manual discount — manager | **unlimited** | | same | 1.4.5 |
| 3.4 | Price-override floor | **cost** | | `setting price.override_floor` | 1.4.7 |
| 3.5 | Are any items subject to a **ministry price ceiling**? ⚖ | **none** | | `product.max_price_minor` | 4.6.3 |
| 3.6 | May a manual discount stack with an automatic promotion? | **no** | | `setting promo.allow_manual_with_auto` | 4.4.3 |

> 3.5 matters more than it looks. Jordan's MoITS enforcement statistics are dominated by price-display failures and selling above set prices. Ask which staples in the assortment are controlled, then let the system hard-block above the ceiling at both catalogue save and sale.

---

## D · Inventory

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 4.1 | Negative stock: allow-and-flag, or hard block? | **allow and flag** | | `store.allow_negative_stock` | 1.10.4 |
| 4.2 | Cost deviation tolerance before confirmation | **30%** | | `setting stock.cost_deviation_ppm` | 4.2.3 |
| 4.3 | Adjustment reason codes in use | damage · theft · expiry · correction | | `setting stock.reason_codes` | 1.10.1 |

> The default on 4.1 is deliberate: blocking a sale because the ledger is wrong punishes the customer at the register for a back-office error. Hard block only for tightly run stores that have said they want it.

---

## E · Catalog operations

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 5.1 | Unknown barcode: quick-add / department sale / block? | **quick-add (manager)** | | `setting catalog.unknown_barcode_policy` | 1.11.10 |
| 5.2 | Are deli/produce scales in use? Which barcode layout? | none | | `embedded_barcode_rule` | 1.2.4 |
| 5.3 | Age-restricted items in the assortment? | tobacco 18+ | | `product.min_age` | 1.4.3 |
| 5.4 | Label printer hardware and shelf-tag size | — | | `setting label.*` | 4.6.1 |

> 5.1's real constraint: **the queue must not stall.** Whichever policy the merchant picks, a cashier facing an unknown code needs a path forward in under five seconds.

---

## F · Tax and fiscal ⚖

**These require the accountant and the tax advisor. Do not guess any of them.**

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 6.1 | Store tax profile: standard / ASEZ / development area / unregistered ⚖ | **standard** | | `store.tax_profile` | 1.3.1 |
| 6.2 | Is the merchant registered for GST? (thresholds: **75k goods / 30k services / 10k special-tax goods**) ⚖ | assumed yes | | same | — |
| 6.3 | Which products sit in which tax category? ⚖ | seeded defaults | | `product.tax_category_id` | 1.3.7 |
| 6.4 | Does the merchant sell **Special Sales Tax** goods (tobacco, alcohol, fuel, telecom)? ⚖ | no | | `tax_rate` second component | 1.3.5 |
| 6.5 | JoFotara: which wave / obligation status? ⚖ | mandatory | | `store.fiscal_profile` | 2.7.1 |
| 6.6 | JoFotara credentials custody — who holds the Client-Id and Secret-Key? | merchant | | keyring | 5.2.3 |
| 6.7 | **The offline-clearance procedure, in writing** ⚖ | — | | `docs/compliance/` | 5.2.5 |
| 6.8 | Buyer TIN capture offered at checkout for B2B? | **yes** | | `setting fiscal.capture_buyer_tin` | 2.7.2 |

> **6.7 is the single most important row on this form.** The regulation says "clear before issue"; the architecture says "sell through any outage." The merchant's tax advisor must answer, in writing: *is a pending-clearance paper receipt acceptable, and within what window must it clear?*
>
> Record the answer with the advisor's name and date. Do not launch fiscal on an assumption.

---

## G · Receipts and documents

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 7.1 | Merchant legal name exactly as registered ⚖ | — | | `org.legal_name` | 1.2.1 |
| 7.2 | TIN ⚖ | — | | `org.tin` | 1.2.1 |
| 7.3 | Receipt language: Arabic / bilingual? | **Arabic** | | `store.receipt_locale` | 1.7.1 |
| 7.4 | Return-policy wording (ar/en) | — | | receipt template | 1.7.4 |
| 7.5 | Footer text and logo | — | | receipt template | 1.7.4 |
| 7.6 | Printer width: 80 mm / 58 mm | **80 mm** | | `setting printer.width` | 1.7.3 |

> 7.1 and 7.2 must match the merchant's ISTD registration **exactly**. A mismatch is a fiscal rejection and, before that, an invalid tax receipt.

---

## H · Shifts and cash control

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 8.1 | Standard opening float | **100.000 JOD** | | `setting shift.default_float_minor` | 2.4.2 |
| 8.2 | Over/short threshold requiring manager acknowledgement | **5.000 JOD** | | `setting shift.variance_ack_minor` | 2.4.6 |
| 8.3 | Cash drop threshold — drawer maximum | **300.000 JOD** | | `setting cash.drop_prompt_minor` | 2.4.3 |
| 8.4 | Paid-out reason codes | courier · cleaning · supplies · other | | `setting cash.reason_codes` | 2.4.3 |
| 8.5 | Store trading-day cutover time | **04:00 local** | | `store.day_cutover_minutes` | 1.1.9 |

---

## I · People

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 9.1 | Idle auto-lock timeout | **120 s** | | `setting auth.idle_lock_seconds` | 1.6.4 |
| 9.2 | PIN length | **4** | | `setting auth.pin_length` | 1.6.2 |
| 9.3 | Max offline-auth window (how long an offline register honours stale permissions) | **72 h** | | `setting auth.max_offline_hours` | 3.7.3 |
| 9.4 | Role names in Arabic | cashier / shift lead / manager / owner | | `role.name_ar` | 1.6.3 |

> Explain 9.3 to the merchant plainly: a terminated employee's PIN keeps working on an offline register until it reconnects or the window expires. Shortening the window increases safety and increases the chance a legitimate cashier is locked out during an outage. **It is their call, and they should make it knowingly.**

---

## J · Customers and loyalty *(Phase 3)*

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 10.1 | Loyalty offered? | **no** | | `setting loyalty.enabled` | 3.4.3 |
| 10.2 | Earn rate — points per JOD | 1 | | `setting loyalty.earn_ppm` | 3.4.3 |
| 10.3 | Redemption value — JOD per point | 0.010 | | `setting loyalty.redeem_minor` | 3.4.3 |
| 10.4 | Do points expire? After how long? | **no** | | `setting loyalty.expiry_days` | 3.4.3 |
| 10.5 | Loyalty terms wording and version ⚖ | — | | `consent.text_version` | 3.4.2 |
| 10.6 | Marketing consent wording and version ⚖ | — | | same | 3.4.2 |
| 10.7 | Customer inactivity period before anonymisation ⚖ | **36 months** | | `setting retention.customer_days` | 5.3.4 |

---

## K · Payments *(Phase 2)*

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 11.1 | Which acquirer / PSP? | — | | driver selection | 2.1.1 |
| 11.2 | Terminal model, and its **PCI P2PE listing number** (or confirmation it has none) | — | | `docs/compliance/pci.md` | 2.1.1, 5.3.3 |
| 11.3 | Does the terminal expose a **last-transaction-status query**? | **required** | | driver | 2.1.3 |
| 11.4 | Accept **CliQ / wallet QR**? Through the bank terminal or direct? | evaluate | | driver | 2.1.1 |
| 11.5 | Accept cheques? | no | | `tender_type` row | 1.5.1 |

> **11.3 is not negotiable.** Without a status query there is no safe recovery from a timeout, and the alternative is either double charges or lost sales. If a candidate terminal lacks it, that is a reason to choose a different acquirer.

---

## L · Deferred features — ask, but expect "no" for v1

| # | Question | Default | Consequence if yes |
|---|---|---|---|
| 12.1 | Gift cards / store credit? | no | Phase 4 stored-value ledger + liability report; **online-authorise-only** |
| 12.2 | Telecom e-recharge top-up? | no | supplier API driver with the card terminal's `Unknown` discipline |
| 12.3 | Layaway? | no | new `doc_type` + payments ledger; forfeiture policy needed |
| 12.4 | House accounts / B2B credit? | no | credit limit + AR ledger; JoFotara **receivable** invoice type |
| 12.5 | Delivery / COD? | no | fulfilment status on the sale + a COD tender flag |
| 12.6 | Serialized items (IMEI)? | no | `sale_line_serial` table; anti-swap control on return |
| 12.7 | Fees charged (bags, delivery)? | no | non-stock products with their own tax category |
| 12.8 | Lot / expiry tracking? | no | a grocery/pharmacy epic; v1 covers it with expiry-waste adjustments |

Each of these has a named architectural hook in the master plan's J.1. **A "yes" is a phase, not a rewrite — but only if the hook is still there.** Check it when you touch the surrounding code.

---

## M · Retention and data ⚖

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 13.1 | Sale document retention period ⚖ | statutory, ask the accountant | | `setting retention.sales_days` | 5.3.4 |
| 13.2 | Audit log retention | never deleted | | same | 5.3.4 |
| 13.3 | Backup retention and location | hourly 24 h / daily 30 d, local | | `setting backup.*` | 1.8.6 |
| 13.4 | Where is server data hosted? Any cross-border constraint? ⚖ | — | | deployment | 3.1.6 |
| 13.5 | Who receives a breach notification, and who sends it? ⚖ | — | | `docs/runbooks/breach.md` | 5.3.2 |

---

## Sign-off

> The defaults above are defensible starting positions, not recommendations for a specific business. Rows marked ⚖ carry legal, tax, or financial consequence for the merchant and must be answered by their accountant, tax advisor, or lawyer.

| | |
|---|---|
| Merchant representative | ________________________ |
| Accountant / tax advisor (⚖ rows) | ________________________ |
| Implementer | ________________________ |
| Date | ________________________ |

**File the completed form in `docs/compliance/` per store.** When something is disputed in month three — and something will be — this is the document that settles it.
