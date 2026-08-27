# Merchant decisions — the questionnaire

Master plan Parts H and J.5, turned into a form you fill in **with the merchant, before coding the feature that depends on it.**

This questionnaire contains **117 questions**. **17 have no default** — `1.3`, `1.7`, `5.2b`, `5.4`, `7.1`, `7.2`, `7.4`, `7.5`, `7.8`, `10.5`, `10.6`, `11.1`, `11.2`, `11.6`, `11.7`, `12.2b`, and `13.5` — and must be answered before their consuming gate. Every other row ships with a **default**, so no other decision blocks the build. Every row names **where the answer lives** — almost always a `setting` row, occasionally a column — and **which gate consumes it**.

Print this. Sit with the merchant. Fill it in. Date it. A decision made by a developer at 2 a.m. is a decision the merchant will dispute in month three.

**A default is what the code does until somebody answers, not a recommendation for this business.** Several defaults here exist because the honest answer is "we do not know yet" — and where the unknown is an external legal, tax or regulatory fact, the greppable `⚠️ OPEN` block in the owning reference document carries the question, the default, the owning microstep and the source that settles it. [`00-master-plan.md`](../00-master-plan.md) §4a.3 lists the ones that can still change an architecture.

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
| 1.1 | Return window for a **change-of-mind** return, in days? | **14** | | `refund_policy.window_days` | 2.3.1 |
| 1.2 | Receiptless returns allowed at all? | **no** | | `.allow_receiptless` | 2.3.5 |
| 1.3 | If yes — maximum value? | — | | `.receiptless_max_minor` | 2.3.5 |
| 1.4 | If yes — store credit only? | **yes** | | `.receiptless_store_credit_only` | 2.3.5 |
| 1.5 | Refund above which amount needs a manager? | **20.000 JOD** | | `.escalate_above_minor` | 2.3.8 |
| 1.6 | Cash refund for a card sale — ever allowed? | **no** | | `.allow_cash_for_card` | 2.3.3 |
| 1.7 | If yes — ceiling? | — | | `.cash_for_card_max_minor` | 2.3.3 |
| 1.8 | Which otherwise permitted operations require a second person rather than proceeding under the actor's own capability? | **every configured escalation; own-shift close is ordinary and excluded** | | `.ban_self_approval` selects escalation; `ApprovalHandle.actor_id <> approver_id` is unconditional | 1.6.4 |
| 1.9 | **Defective goods: does a claim bypass the change-of-mind window?** ⚖ | **yes — interim pending counsel** | | `.defective_bypasses_window` | 2.3.2 |
| 1.10 | Is repair or replacement offered instead of a refund, and how is consent recorded? ⚖ | refund on request; repair only with recorded consent | | `.defect_resolution_policy` | 2.3.2 |
| 1.11 | Returning part of a multibuy: reprice what the customer keeps, or refund their share? | **reprice (`DealBreak`)** | | `.requalify_policy` | 2.3.2 |
| 1.12 | May a cash refund be rounded to the coin step, and in whose favour? ⚖ | **customer's favour**, recorded on the document | | `.refund_round_direction` | 2.3.3 |
| 1.13 | Cumulative refund cap per cashier per shift | **50.000 JOD** | | `.shift_refund_cap_minor` | 2.3.8 |

> 1.6 is a money-laundering vector, which is why the default is off and the capability is separate. If the merchant wants it, make sure they understand *why* it is separate.
>
> **1.9 is not a merchant-selected waiver.** Until Jordanian counsel closes the OPEN item owned by
> `2.3.2`, the conservative product default lets `Defective` claims bypass the change-of-mind
> window and records the claim for review; it does not state the governing remedy or period as
> settled law. Master-plan J.3's "may bypass the window per policy" is corrected in
> [`00-master-plan.md`](../00-master-plan.md) §4a.
>
> **1.11 is worth two minutes of the merchant's time.** With "3 for 1.000" on a 0.500 item, refunding one unit at its discounted share lets the customer keep two units for 0.667 when the shelf price for two is 1.000. Repricing is the retail answer and the default; the alternative is a per-transaction leak that scales with the depth of the offer.
>
> **1.13 exists because every other control here is per-transaction** and the actual pattern is many small refunds, each below the threshold. The report that catches it is Phase 4; this cap ships with the capability.

---

## B · Money and rounding

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 2.1 | Cash rounding step | **10 fils (1 qirsh)** | | `store.cash_round_step_minor` | 1.5.3 |
| 2.2 | Rounding direction | **nearest** | | `.cash_round_direction` | 1.5.3 |
| 2.3 | Shelf prices shown to 2 or 3 decimals? | **3** | | `.money_decimals` | 1.1.2 |
| 2.4 | Tax-inclusive or tax-exclusive shelf prices? | **inclusive** | | `.price_mode` | 1.3.4 |
| 2.5 | Rounding rule for tax ⚖ | **half away from zero**, as jurisdiction policy v1 | | `.rounding_rule` | 1.1.6 |

> **Verify 2.1 against the store's actual coin drawer.** Some accept 5 fils; some round to 25. The default reflects that 5-fils pieces are rare in everyday circulation, not that they do not exist.
>
> **2.5 is not a merchant preference, and a blank Answer column here invites the wrong build.** The
> tie rule changes tax facts rather than presentation: a 13-fil 4%-inclusive line has an exact net of
> 12.5 fils, so half-away records net 13 and tax 0 while half-even records net 12 and tax 1. Two
> registers under one taxpayer that disagree file inconsistent returns and nothing diagnoses it. It
> therefore belongs to a **versioned jurisdiction policy** pinned per store — [`domain-api.md`](domain-api.md)
> §1.2 says in terms that it "does not offer four options to a settings screen", and
> [`tax-jordan.md`](tax-jordan.md) §4 makes it policy with an effective period. Once fiscal issuance is
> enabled for a store it may not change without a new policy version and a recorded reason, because a
> mid-year change makes the merchant's own filing history internally inconsistent. What the advisor
> confirms is *which policy applies*; the tie rule ISTD's validator actually applies is still the
> `⚠️ OPEN` owned by `2.7.0`, and the default is provisional until it answers.

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
| 4.4 | **Is opening stock being loaded, and how?** | **CSV column on the catalogue import** | | `stock_ledger` opening events | 3.6.7 |
| 4.5 | Until opening stock exists, is on-hand shown at all? | **no — suppressed with a stated reason** | | `store.stock_tracking_enabled` | 1.10.4 |

> The default on 4.1 is deliberate: blocking a sale because the ledger is wrong punishes the customer at the register for a back-office error. Hard block only for tightly run stores that have said they want it.
>
> **4.4 and 4.5 exist because "allow and flag" is worthless without a stock-in path.** Every path that *increases* stock arrived in Phase 4, so from the first sale every product goes negative and stays negative: a 1 500-SKU minimarket generates 1 500 negative-stock rows in a fortnight, the flag is a hundred-per-cent false positive, and the merchant learns to ignore it — permanently, including after it starts meaning something. `stock_adjust` lands in Phase 1 with the ledger for the same reason.

---

## E · Catalog operations

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 5.1 | Unknown barcode: quick-add / department sale / block? | **department sale**, with quick-add behind `product.edit` | | `setting catalog.unknown_barcode_policy` | 1.11.10 |
| 5.2 | Are deli/produce scales in use? Which barcode layout? | none | | `embedded_barcode_rule` | 1.2.4 |
| 5.2b | For each trade scale: maker, model, serial, **JSMO verification evidence**, seal status ⚖ | — | | `trade_scale`, `trade_scale_verification` | 1.2.4 |
| 5.3 | Age-restricted items in the assortment? | tobacco 18+ | | `product.min_age` | 1.4.3 |
| 5.3b | Any product class with **sale-form or advertising restrictions** beyond the age gate? ⚖ | tobacco: no single sticks, no promotion, no label advertising | | `product.regulated_kind` | 4.6.5 |
| 5.4 | Label printer hardware and shelf-tag size | — | | `setting label.*` | 4.6.1 |
| 5.5 | **Any multipack or outer-case barcodes in the assortment?** | **no** | | `barcode.pack_qty_milli` | 1.2.4 |

> 5.1's real constraint: **the queue must not stall.** A cashier facing an unknown code needs a path forward in under five seconds — and the old default could not deliver one, because `product.edit` is manager-only and at 22:00 the manager is the owner and the owner is at home. The cashier's remaining options were to abandon the item or ring it up as something else, which puts the wrong product, the wrong tax category and a misdescribed receipt into an immutable sale. The department sale is the retail answer: a capped, capability-gated, audited line against a department category with its own tax treatment.
>
> **5.2b is a metrology question, not a POS one, and it is still yours.** A price-embedded barcode is a price derived from a scale used for trade; JSMO verifies legal measuring instruments and conducts surprise verification of trade balances. Live weighed pricing waits for signed evidence, and reverification after maintenance is part of the answer.
>
> **5.5 is money.** Both documents that mention multipacks name them as the *reason* a product carries several barcodes, and nothing carried the quantity. A six-pack scanned on its outer code charged one can's price and decremented one unit — around five-sixths of the item's value, every time, discovered months later as an unexplainable hole in a stock count.

---

## F · Tax and fiscal ⚖

**These require the accountant and the tax advisor. Do not guess any of them.**

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 6.1 | Store tax profile: standard / ASEZ / development area / unregistered ⚖ | **standard** | | `store.tax_profile` | 1.3.1 |
| 6.1b | If not standard — the complete, effective-dated **jurisdiction rule pack** and its return mapping ⚖ | **none; the profile fails closed** | | `tax_rule_pack` | 1.3.1 |
| 6.2 | **Registered activity class**, and is the merchant a producer/manufacturer or an importer? ⚖ | ordinary goods seller, not a producer, not an importer | | `org.registered_activity` | 1.3.1 |
| 6.2b | GST registration status and the evidence for it — trailing turnover, forward forecast, first taxable import, mixed activities ⚖ | assumed registered | | `store.tax_profile` | 1.3.1 |
| 6.3 | Which products sit in which tax category? ⚖ | seeded defaults, from the current official catalogue | | `product.tax_category_id` | 1.3.7 |
| 6.4 | Does the merchant have a **Special Sales Tax** liability — a certificate, or a designated domestic tax point? ⚖ | **no** | | `tax_rate` components | 1.3.5 |
| 6.5 | **JoFotara obligation or exemption**, on the merchant's own official evidence ⚖ | mandatory | | `store.fiscal_profile` | 2.7.1 |
| 6.5b | Assigned **filing cycle** and return types — general monthly/bi-monthly, special, zone ⚖ | general, bi-monthly | | `tax_filing_profile` | 4.7.2 |
| 6.6 | JoFotara credentials custody — who holds them, at what scope, and how are they rotated? | merchant; scope confirmed at `2.7.0` | | credential store reference | 5.2.3 |
| 6.7 | **The offline-clearance procedure, in writing** ⚖ | non-fiscal payment acknowledgement until the ruling arrives | | `docs/compliance/` | 2.7.0 |
| 6.8 | Buyer TIN capture offered at checkout for B2B? | **yes** | | `setting fiscal.capture_buyer_tin` | 2.7.2 |
| 6.9 | **ICV scope**, once `2.7.0` confirms it — register, store, income source, credential, or TIN | **store** | | `doc_sequence.scope_kind` | 2.7.0 |
| 6.10 | Alarm threshold for the **oldest uncleared** fiscal document | **4 hours** | | `setting fiscal.uncleared_alarm_hours` | 3.9.3 |
| 6.11 | Who receives fiscal alarms, on which channel, and who escalates? | merchant owner **and** vendor | | `setting alerts.fiscal_recipients` | 3.9.3 |
| 6.12 | **Cash-rounding treatment for tax and fiscal purposes** ⚖ | tender-level adjustment only; no line or tax row moves | | `store.cash_round_*` | 1.5.3 |
| 6.13 | **The pilot's fiscal posture**, per store ⚖ | disabled on dated obligation evidence | | `docs/compliance/pre-pilot.md` | 4.9.0 |

> **6.7 is the single most important row on this form.** The regulation says "clear before issue"; the architecture says "sell through any outage." Somebody must answer, in writing: *is a pending-clearance paper receipt acceptable, and within what window must it clear?* Record the answer with a name and a date. Do not launch fiscal on an assumption.
>
> **But note who answers what.** A tax advisor is the right authority for the merchant's own classification, elections and obligation status. They are the wrong authority for a protocol fact — the issuance event, an outage grace period, the ICV namespace, duplicate recovery, a validator's tolerance. Those come from the official ISTD package pinned at `2.7.0`, or from a written answer from the E-Invoicing Directorate. Ask the right party or the answer is worth nothing.
>
> **6.2 and 6.5 are two different questions and used to be one.** GST registration and JoFotara obligation are independent axes: a merchant below a GST threshold may still owe income invoices, so "GST unregistered" must never imply `fiscal_profile = 'disabled'`. And an ordinary minimarket does **not** enter the JOD 10,000 threshold class merely because it resells tobacco — that class is the *producer* of SST goods, and SST is generally charged at import or a designated domestic tax point. Getting this wrong tells a merchant to register at the wrong threshold and to double-charge tax already borne upstream.
>
> **6.1b fails closed on purpose.** An unscoped rate rule applies only to the `standard` profile. Selecting ASEZ or a development area without its own complete pack used to inherit the generic 16%, which is a different regime and a different return — silently.
>
> **6.13 has exactly three permitted answers** (4.9.1), and "we'll sort it out during the week" is not one of them.

---

## G · Receipts and documents

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 7.1 | Merchant legal name exactly as registered ⚖ | — | | `org.legal_name` | store provisioning / valid tax receipt |
| 7.2 | TIN ⚖ | — | | `org.tin` | store provisioning / valid tax receipt |
| 7.3 | Receipt language: Arabic / bilingual? | **Arabic** | | `store.receipt_locale` | 1.7.1 |
| 7.4 | Return-policy wording (ar/en) | — | | receipt template | 1.7.4 |
| 7.5 | Footer text and logo | — | | receipt template | 1.7.4 |
| 7.6 | Printer width: 80 mm / 58 mm | **80 mm** | | `setting printer.width` | 1.7.3 |
| 7.7 | **Printer dead or absent at shift open — block sales, or sell and queue the receipt?** ⚖ | **sell; queue the artifact; raise an alarm** | | `setting printer.absent_policy` | 1.7.6b |
| 7.8 | The supported hardware list: printer models per width, scanner, drawer, terminal, label printer | — | | `docs/compliance/` per store | 2.9.4 |

> **7.1 and 7.2 gate store provisioning and the issuing of a valid tax receipt; they do not gate migration `0003`.** Both must match the merchant's ISTD registration **exactly**. A mismatch is a fiscal rejection and, before that, an invalid tax receipt.
>
> **7.7 is the 9 a.m. Saturday support call, and the plan had no answer.** Paper running out mid-receipt was handled; the printer being unplugged was not. The default sells, because blocking closes the shop — but a fiscal QR is meant to appear on the document handed to the customer, so where the store is fiscally enabled the ⚖ matters: ask the advisor whether a queued artifact is acceptable, and record it beside 6.7.
>
> **7.8 is why the hardware lab is a lab.** The Arabic pass needs a real printer at **each** supported width, not one 80 mm unit and an assumption about the other.

---

## H · Shifts and cash control

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 8.1 | Standard opening float | **100.000 JOD** | | `setting shift.default_float_minor` | 2.4.2 |
| 8.2 | Over/short threshold requiring manager acknowledgement | **5.000 JOD** | | `setting shift.variance_ack_minor` | 2.4.6 |
| 8.3 | Cash drop threshold — drawer maximum | **300.000 JOD** | | `setting cash.drop_prompt_minor` | 2.4.3 |
| 8.4 | Paid-out reason codes | courier · cleaning · supplies · other | | `setting cash.reason_codes` | 2.4.3 |
| 8.5 | Store trading-day cutover time | **04:00 local** | | `store.day_cutover_minutes` | 1.1.9 |
| 8.6 | **May two cashiers transact on one drawer in one shift?** | **no — a handover closes and reopens the shift** | | `setting shift.allow_shared_drawer` | 2.4.2 |
| 8.7 | No-sale drawer opens: how many in a shift before it escalates? | **3**, then a manager reason | | `setting drawer.no_sale_escalate_count` | 2.4.8 |
| 8.8 | Who may bank, and how often? | manager or owner; at least weekly | | `setting cash.bank_policy` | 2.4.3 |
| 8.9 | Is the counted cash at close left in the drawer as the next float, or dropped to the safe? | **dropped** | | `setting shift.carry_float` | 2.4.10 |

> **8.6 decides whether "over/short by cashier" means anything.** Over/short is a shift-level fact and cannot be apportioned between two people who shared the drawer, so with sharing allowed the Phase-4 report is by *shift and its opener* and must say so. The till/shift/register collapse is a good simplification; its accountability consequence is the part that has to be written down.
>
> **8.9 is a reconciliation trap.** A carried float declared twice is counted twice; the answer has to be one declaration reconciling across both shifts.

---

## I · Users and authentication

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 9.1 | Idle auto-lock timeout | **120 s** | | `setting auth.idle_lock_seconds` | 1.6.4 |
| 9.2 | PIN length | **4** | | `setting auth.pin_length` | 1.6.2 |
| 9.3 | Max offline-auth window (how long an offline register honours stale permissions) | **72 h** | | `setting auth.max_offline_hours` | 3.7.3 |
| 9.4 | Role names in Arabic | cashier / shift lead / manager / owner | | `role.name_ar` | 1.6.3 |
| 9.5 | **Manager PIN length** — longer than a cashier's? | **6** | | `setting auth.manager_pin_length` | 1.6.2 |
| 9.6 | Is a second factor available at the counter for high-value refunds, user administration and recovery? ⚖ | **no** — manager PIN + audited reason + exception report | | `setting auth.second_factor` | 1.6.2 |

> Explain 9.3 to the merchant plainly: a terminated employee's PIN keeps working on an offline register until it reconnects or the window expires. Shortening the window increases safety and increases the chance a legitimate cashier is locked out during an outage. **It is their call, and they should make it knowingly.** The window is enforced by a server-signed lease plus trusted-time state (3.7.3), not by the device clock — an unenforced window is a disclosure, not a control.
>
> **9.5 is arithmetic, not preference.** A four-digit PIN has 10 000 candidates; at the plan's own ~250 ms verification cost that is 42 minutes of serial search at worst, and about half that on average. Argon2 parameters, persistent per-user and per-register attempt state across restarts, escalating delays and an attempt ceiling are the real defences; PIN length is the cheapest additional bit. Anything that authorises money — refunds above the threshold, user administration, recovery, key operations — should not sit behind four digits.

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
| 11.4 | Accept **CliQ / wallet QR**? | **only through the bank terminal or a CBJ-licensed merchant acquirer** | | driver | 2.1.1, 2.2.6 |
| 11.5 | Accept cheques? | no | | `tender_type` row | 1.5.1 |
| 11.6 | The acquirer's **written responsibility matrix**, legal entity, licence, and a funds-flow diagram ⚖ | — | | `docs/compliance/pci.md` | 2.1.1 |
| 11.7 | Store network topology the terminal requires, and the remote-support model ⚖ | — | | `docs/compliance/pci.md` | 2.1.1, 5.3.3 |

> **11.3 is not negotiable.** Without a status query there is no safe recovery from a timeout, and the alternative is either double charges or lost sales. If a candidate terminal lacks it, that is a reason to choose a different acquirer.
>
> **11.4's default changed from "evaluate" to a boundary.** Direct CliQ participation is a bank capability; businesses receive merchant services through banks and acquirers. Whether a vendor-operated QR, acceptance or settlement path needs a CBJ licence depends on its funds flow, and a generic terminal trait does not decide a legal classification. For v1 the answer is the acquirer's terminal; a direct path stays blocked until the Central Bank classifies it in writing and every required licence is held.
>
> **11.6 and 11.7 are what a QSA will ask for**, and 11.2's listing number alone does not answer them. The SAQ determination changes engineering and operations, not only the sentence in a brochure.

---

## L · Deferred features — ask, but expect "no" for v1

| # | Question | Default | Consequence if yes |
|---|---|---|---|
| 12.1 | **Gift cards** (store credit as a refund remedy is not deferred — it ships in Phase 2) | no | Phase 4 sale/top-up on the Phase-2 stored-value ledger; **online-authorise-only** |
| 12.1b | If yes — do gift-card balances expire, and after how long? ⚖ | no expiry | a policy version against every ledger event |
| 12.1c | If yes — what happens to an unclaimed balance, and where does the liability sit? ⚖ | held indefinitely as a liability | the liability report (4.7.6) and the accountant's treatment |
| 12.1d | If yes — is the **sale** of the instrument a taxable supply, or only its redemption? ⚖ | unresolved; gift cards stay disabled | the tax policy persisted per event (4.4.7) |
| 12.2 | Telecom e-recharge top-up? | no | supplier API driver with the card terminal's `Unknown` discipline |
| 12.2b | If yes — the supplier, the reseller contract, the commission model, and a written regulatory classification ⚖ | — | whether the vendor ever holds value or settles funds decides whether a licence is needed |
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
| 13.1 | Sale document retention period ⚖ | **ten years from the end of the tax period**, as a floor | | `setting retention.sales_days` | 5.3.4 |
| 13.2 | Audit log retention | never deleted | | same | 5.3.4 |
| 13.3 | Backup retention, and **both** destinations | hourly 24 h / daily 30 d, local **plus one off-machine target** | | `setting backup.*` | 1.8.6, 1.8.6b |
| 13.3b | **Who holds the printed recovery code, and where?** | the owner, off-site, on paper | | `docs/compliance/` per store | 1.8.5b |
| 13.4 | Hosting region and the cross-border transfer basis ⚖ | **the vendor's stated region**, accepted by the merchant; a dedicated in-country deployment is priced separately | | deployment | 3.1.6, 3.10.1 |
| 13.5 | Who receives a breach notification, who reports to the Unit, and who sends the individual notice? ⚖ | — | | `docs/runbooks/breach.md` | 5.3.2 |
| 13.6 | Is **crash and error telemetry** to our processor accepted — region, retention, opt-out? ⚖ | on, scrubbed, in the approved region | | `setting telemetry.enabled` | 3.9.1 |
| 13.7 | Statutory record classes and their clocks, from which trigger date ⚖ | ten years, with an indefinite legal hold | | `docs/compliance/retention.md` | 5.3.4 |

> **13.3b is the row that makes the backup real.** The wrapped key envelope travels with every backup, and the recovery code is what unwraps it. If the code is in the same drawer as the register, theft or fire takes both — which is the failure the second destination exists to survive.
>
> **13.4 stopped being a question the merchant could answer.** On one shared multi-tenant service the region is structurally the vendor's decision; the merchant's part is accepting it, or paying for a dedicated deployment. And transport security is not a transfer basis: TLS answers confidentiality in transit, and PDPL asks a different question about where the data lands and under what protection.
>
> **13.6 was invisible.** Telemetry is an outbound flow of merchant-controlled data to a third party that the vendor chose, and it belongs on the sub-processor list, in the privacy notice, and in the PDPL walkthrough — not only in the scrubber's tests.

---

## N · Commercial terms

*New. The plan verified entitlements and never issued one, and never stated what was being sold — while the launch gate is "it can be sold to someone who is not you". The unit of sale determines what an entitlement asserts, so it is decided before microstep 3.8.1.*

| # | Question | Default | Answer | Lives in | Step |
|---|---|---|---|---|---|
| 14.1 | **Unit of sale** — per register, per store, or subscription? | per register | | entitlement schema | 5.0.4 |
| 14.2 | Term and renewal date | 12 months | | entitlement | 5.0.4 |
| 14.3 | **Grace buffer past the paid term, as a number of days** | **90** | | entitlement validity | 3.8.1 |
| 14.4 | What happens at non-payment? | **enrollment and updates stop; selling does not** | | `licence` policy | 3.8.1 |
| 14.5 | Support hours, severity definitions, and target response times | trading hours; critical acknowledged in 4 h | | `docs/legal/support.md` | 5.0.3 |
| 14.6 | Patch SLA for a confirmed security issue | critical 7 days, high 30 days | | `docs/legal/terms.md` | 5.0.3 |
| 14.7 | Who is the controller and who is the processor, per data flow? ⚖ | merchant is controller; vendor is processor | | `docs/legal/dpa.md` | 5.0.2 |
| 14.8 | The on-call expectation, in the merchant's words | stated window, explicitly not overnight | | `docs/runbooks/server.md` | 3.10.4 |

> **14.4 is a product decision with a commercial cost, and it is deliberate.** For a point of sale, "read-only on expiry" *is* a lockout with a gentler name — and combined with a single-person vendor holding the only signing key, it means an unavailable vendor eventually stops every register at every merchant at about the same moment. Expiry therefore blocks enrollment and updates only, and 14.3 makes trading independent of any online validation at all. Non-payment is collected by asking, the way every other B2B vendor collects it. Overrule this if the commercial model demands it, and then put the grace period in the contract as a number rather than as the word "generous".
>
> **14.8 is not a formality.** "The server is down at 19:40 on a Thursday" has an answer, and the merchant is better served by an honest window than by an implied 24/7 that breaks the first weekend.

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
