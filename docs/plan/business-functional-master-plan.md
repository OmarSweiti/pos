# POS Business & Functional Master Plan

**Companion to:** `pos-engineering-blueprint.md` (architecture decisions) and `pos-phase0-setup-guide.md` (working foundation).
**Audience:** a developer who has the architecture running but has never worked retail.
**Primary market:** Jordan (JOD, Arabic-first, ISTD jurisdiction). Designed so other markets are configuration + one pluggable fiscal module, not a rewrite.

**How to use this document:** Part A teaches you the business. Part B is the law — what you *must* build. Part C specifies every feature with its business rules and where it lives in your crates. Part D is the UI. Part E is the "everything that can happen" catalog. Parts F–H turn it into schema, build order, and the questions only your client can answer.

> ⚠️ Compliance sections synthesize public sources current as of Aug 2026 and are engineering guidance, not legal or tax advice. Before launch, validate JoFotara, GST treatment, PDPL, and PCI scope with the merchant's accountant, a Jordanian tax advisor, and (for PCI) a QSA.

---

# Part A — How a retail store actually works (developer's primer)

## A.1 A day in the life of the register

1. **Store open.** A manager unlocks; a cashier signs in with a PIN and **opens a shift** on a register, declaring the **opening float** (the counted cash placed in the drawer, e.g. 100.000 JOD in small denominations for change).
2. **Selling.** Items enter the cart by barcode scan (most), search (no barcode / damaged label), or a tile grid (produce, bakery, services). The cashier may change quantity, apply a permitted discount, or call a manager for anything beyond their rights. Customer pays — cash, card, or a mix — receipt prints, drawer opens for cash, next customer. Target rhythm: a few seconds per item, under a minute per customer.
3. **Interruptions.** Customer forgot their wallet → **park** the sale, serve the next person, **resume** later. Customer changes their mind after an item scanned → void the line. After payment started → that's a refund/void with manager approval, not a line edit.
4. **Mid-day cash control.** Too much cash in the drawer is a robbery/theft risk → manager performs a **cash drop** (moves excess to the safe, recorded). Petty expenses (courier, cleaning) → **paid-out** with a reason. Adding change → **paid-in**.
5. **Returns.** Customer brings an item back with a receipt. The system finds the original sale, verifies what's still refundable on it, takes the item back into stock (or marks it damaged), and refunds **to the original tender** (card refund goes back to the card via the PSP, cash to cash).
6. **Shift close / end of day.** Cashier counts the drawer **blind** (system doesn't show the expected figure first), enters the counted amount; system computes **over/short** versus expected (float + cash sales − cash refunds − drops − paid-outs + paid-ins). Manager runs the **Z report** (the immutable end-of-day summary that resets period counters; an **X report** is the same summary mid-day without closing anything). Card terminal batch is settled and later reconciled against the PSP's report.
7. **Back office, async.** Owner updates prices, receives purchase orders into stock, runs stock counts, reviews reports, manages staff — from the back-office app, synced to every register.

## A.2 Glossary you'll see everywhere

| Term | Meaning |
|---|---|
| SKU | Stock-keeping unit — one sellable variant (e.g., "Cola 330 ml can"). |
| PLU | Price look-up code — short numeric code keyed for unbarcoded items (produce). |
| Tender | A payment instrument applied to a sale (cash, card, voucher). One sale may have many (split tender). |
| Float / opening balance | Cash placed in the drawer at shift open to make change. |
| Cash drop / paid-in / paid-out | Non-sale drawer movements, all recorded with reason + who. |
| X report / Z report | Shift summary without / with closing the period. Z is immutable and numbered. |
| Over/short | Counted cash minus expected cash at close. Chronic short = training issue or theft. |
| Void vs refund | Void kills a sale **before completion** (nothing fiscal happened). Refund reverses **after** completion (fiscal document, stock back, money back). Never blur these. |
| Shrinkage | Inventory lost to theft/damage/error — why stock counts and adjustment reasons exist. |
| GST | Jordan's General Sales Tax — a VAT: charged on sales, business reclaims tax paid on inputs, remits the difference to ISTD. |
| Tax-inclusive price | Shelf price already contains tax (the retail norm in Jordan/EU). The engine *extracts* tax from the price. Tax-exclusive (US-style) *adds* it. Support both; default inclusive. |
| Cost vs price; margin | What you paid vs what you charge; margin = (price − cost)/price. Cost lives on stock receipts and feeds inventory valuation, never on receipts. |
| WAC | Weighted-average cost — the inventory costing method to implement first (simpler than FIFO layers, accepted for retail). |

## A.3 The three ledgers mindset

Everything in this plan reduces to three append-only ledgers your blueprint already mandates:
- **Sales facts** (immutable documents: sales, refunds, voids-after-the-fact as reversing documents),
- **Stock ledger** (every quantity change is an event with a type and reference),
- **Money ledger** (tenders + drawer movements per shift).
If a feature can't be expressed as events in these ledgers plus catalog/config data, its design is wrong.

---

# Part B — Laws & compliance that shape the build (Jordan)

## B.1 General Sales Tax (GST) — the tax engine's requirements

- **Standard rate 16%** on most goods and services, administered by the **Income & Sales Tax Department (ISTD)**.
- **Zero-rated (0%, still reported):** exports; supplies to free zones; **Aqaba Special Economic Zone (ASEZ)** and development areas have special treatment — a store located there is a *store-level tax profile*, not a hack.
- **Exempt (no GST, not reclaimable):** staples such as bread, water (<5 L packs), tea, sugar, gold, electricity; certain services. Exempt ≠ zero-rated in reporting — model them as distinct categories.
- **Reduced rates exist by Cabinet resolution** (examples reported: 1%, 2%, 4%, 5%, 10% on specific items like hygiene products, salt, oils, corn, live animals/cheese). Rates change by decree → **rates are data with effective dates, never code**.
- **Special (excise-style) Sales Tax** on specific goods (tobacco, alcohol, vehicles, telecom at 24%…). If the merchant sells such items, that's an additional per-item tax component — schema should allow >1 tax component per item even if v1 ships only GST.
- **Registration thresholds** (annual): ~JOD 50,000 goods / 30,000 services — below them a merchant may be unregistered and charge **no GST at all** → "tax-disabled merchant" must be a supported configuration.
- **Filing:** GST returns are periodic (bi-monthly for general tax) → your **tax report must total sales and tax by rate/category per date range** — that report *is* the accountant's filing input.

**Engine consequences (pos-domain):** per-item `tax_category`; category → time-effective rate rules resolved per store profile; inclusive & exclusive modes; per-line tax computation with a defined rounding rule; receipt-level tax summary grouped by rate; exempt/zero/reduced/standard all first-class.

## B.2 JoFotara — Jordan's mandatory e-invoicing (the big one)

**What it is.** Jordan's national e-invoicing system ("الفوترة الوطني"), run by ISTD with the Ministry of Digital Economy. Legal basis: GST Law No. 38 of 2018 (Art. 23), Regulation No. 34 of 2019, and the Amended Billing & Control Regulation No. 2 of 2025.

**Status.** Registration was mandatory for taxpayers by 31 May 2024; **Phase 2 made e-invoicing mandatory from 1 April 2025 for B2B, B2C, and B2G** — i.e., **ordinary retail receipts are in scope**, not just corporate invoices. (Some advisories describe the obligation as rolling out by taxpayer size — confirm the merchant's specific obligation and wave with their tax advisor, but architect as if every receipt must clear.)

**The model — Continuous Transaction Control (clearance):**
1. Your system generates the invoice as **UBL 2.1 XML**, transmitted **as JSON** to the JoFotara **API** (credentials: Client ID + Secret; separate sandbox and production).
2. ISTD validates and returns a **QR code / signed reference (UUID)**.
3. **The QR must appear on the document given to the customer.** An invoice that never cleared has no legal standing, and the buyer can't use it for VAT deduction.
4. Two invoice types: **cash invoices** (paid now — the POS default) and **receivable invoices** (credit). Buyer identification is reportedly not required under ~JOD 10,000 (verify current threshold) — retail B2C receipts normally carry no buyer details; a B2B customer will *ask* for their TIN on the invoice, so support optional buyer TIN capture at checkout.
5. **Penalties:** reported up to JOD 500 per violation, plus the commercial damage of issuing invoices customers can't deduct.

**The hard design problem — clearance vs. offline-first.** The regulation's letter is "clear before issue," but your architecture's soul is "the store sells through any outage." Real-world Jordanian POS integrations resolve this with a **durable fiscal queue**: submit each completed sale immediately; on success print/attach the ISTD QR; on failure (offline, API down, validation error) the sale still completes locally, enters a retry queue, and the receipt is marked pending clearance. Build exactly that — **but treat the sanctioned offline procedure as an open compliance question the merchant's tax advisor must answer in writing** (e.g., is a pending-clearance paper receipt acceptable, and within what window must it clear?). Surface "uncleared invoices: N" as a first-class health metric in register status and back office; never let it silently grow.

**Known validation gotchas (from field integrations):** order-level discounts must be **prorated across lines** (negative or unbalanced lines get rejected — your largest-remainder `Money::split_evenly` is exactly the right tool); item/tax/UoM codes must map to ISTD's expected code lists; totals must reconcile exactly, so the fiscal document must be generated **from the same pos-domain math as the receipt**, never recomputed.

**Module consequence:** a `pos-fiscal` concern (start as a module in `pos-sync` or its own crate later) owning: UBL 2.1 document builder fed by the finalized sale, JSON envelope + API client, credential storage (keyring, like the DB key), the durable queue table + retry policy with backoff, response persistence (UUID, QR payload) onto the sale, QR rendering into the ESC/POS receipt, sandbox/production switch, and a reconciliation report (local sales ↔ cleared invoices). Credit notes/refunds go through the same pipeline as their own fiscal documents referencing the original.

## B.3 Personal Data Protection Law (PDPL) — because you store customers

Jordan's **PDPL, Law No. 24 of 2023** — first comprehensive data-protection law; published 17 Sep 2023, in force 17 Mar 2024, grace period ended Mar 2025, and it applies **retroactively** to previously collected data. GDPR-like core: **explicit informed consent**, purpose limitation, data-subject rights (access, correction, erasure), restricted cross-border transfer, **breach notification to affected individuals within 24 hours** for serious breaches, special "sensitive data" category. Enforcement institutions have been standing up gradually — build to the law, not to enforcement lag.

**Product consequences:** loyalty/CRM is **opt-in with recorded consent** (timestamp + wording version stored); collect the minimum (name, phone/email, consent flags — no ID numbers unless a real requirement emerges); erasure implemented as **anonymization** (blank the person, keep the immutable financial facts — your blueprint's tombstone approach); customer data export function; marketing-consent flag honored by any messaging feature; retention periods documented in settings; the 24-hour breach clock means SQLCipher-at-rest + keyring + no-PII-in-logs (already blueprint policy) are not optional; sync of customer data over TLS with server-side access control.

## B.4 Card payments & PCI DSS

Blueprint §6 already made the right call: **semi-integrated certified terminals only** — amount goes to the terminal, result/reference comes back; PAN/track/CVV never touch your process, DB, or logs. That keeps you in short-SAQ territory (e.g., SAQ P2PE-family) instead of a full audit — **confirm the exact SAQ with a QSA**, and never claim validation you haven't done. Jordan-specific action: PSP/terminal availability differs by market — evaluate what actually operates in Jordan (regional PSPs/bank-provided terminals vs. Adyen/Stripe coverage) *before* writing driver code; the `PaymentTerminal` trait is your insulation. Non-negotiables restated: store the terminal's transaction reference on every card `sale_tender`; treat timeout as *unknown* → status-query before retry; support partial approval + split tender from day one; card refunds go through the PSP against the original reference.

## B.5 Money in Jordan — currency mechanics the code must respect

- **JOD has a minor-unit exponent of 3**: 1 dinar = 1000 **fils** (= 100 **qirsh/piastres** of 10 fils). Your `Money` i64-minor-units design is perfect — but the **exponent must be per-currency data** (JOD 3, USD/EUR 2), used for parsing, display, and rounding. The Phase-0 sample row `250 JOD-minor = 0.250 JOD` was already fils.
- **Display convention:** retail commonly shows 2 or 3 decimals (e.g., `1.25 JD` or `1.250 JD`) — make display precision a store setting; storage is always fils.
- **Cash rounding:** the smallest coin in everyday circulation is effectively **1 qirsh (10 fils)** (5-fils pieces are rare). Cash-tendered totals should round to a configurable step (default 10 fils; direction default: nearest, half away from zero), with the **rounding difference recorded as an explicit receipt line/field** so books and JoFotara totals still reconcile exactly; card payments charge the exact unrounded total. Verify the store's actual coin practice with the merchant.
- **Tax-inclusive shelf prices** are the Jordanian retail norm — default the price mode to inclusive; the engine extracts GST (`tax = gross − gross/(1+r)`, computed in `rust_decimal`, rounded once per line by the configured rule, stored back as i64 fils).
- **Multi-currency:** out of MVP scope. If tourist areas demand it later, model a second *display* currency with a fixed daily rate; settlement remains JOD. (True multi-currency settlement drags in accounting complexity you should refuse until a paying customer insists.)

## B.6 Other document & operational rules

- **Receipt content (baseline for a compliant Jordanian tax receipt):** merchant legal name + **TIN (tax number)**, address, date/time, receipt number, line items with quantity/unit price/line total, tax summary by rate, grand total, tender breakdown + change, **JoFotara QR** once cleared, and Arabic as the primary language (bilingual Arabic/English is common and safe). Keep the exact legal minimum on the tax advisor's checklist.
- **Numbering:** your per-register receipt sequence stands; the *fiscal* identity of a document is the JoFotara UUID — store both, print both (receipt no. prominently; UUID/QR for verification).
- **Record retention:** keep sale documents and Z reports for the statutory period (align with the accountant; regionally this is multi-year) — reinforces "never hard-delete financial facts."
- **Labor/shift rules** (clock-in/out) have legal weight for payroll but are out of POS-MVP scope; shifts here are *cash-accountability* shifts.

---

# Part C — Feature catalog: functional spec, business rules, and where it lives

Each feature lists: what it is, the rules that make it *correct* (the domain knowledge you asked for), and its home in your architecture (`pos-domain` = pure rules, `pos-db` = SQLite schema/queries, `pos-sync` = replication, `pos-hardware` = devices, `terminal` = UI + IPC commands, `server`/`backoffice` = central). Phase numbers refer to the blueprint's build order (Part G updates it).

## C.1 Catalog & pricing (Phase 1 core; price lists Phase 4)

**Entities:** product (name ar/en, category, tax_category, active flag, sell-by-weight flag, unit of measure) → 1..n **barcodes** (a product often has several: multipacks, supplier relabels) → prices.

**Rules that matter:**
- Barcode is a *lookup key*, never the identity — identity is the UUID. Deleting/reassigning a barcode must not touch history.
- **Price captured at sale time is copied onto the sale line** (name too). Reports and refunds read the line, never today's catalog. This one rule prevents half the historical-data bugs in POS systems.
- Weighted/measured items: quantity is a decimal (kg at 3 dp), price is per-unit; support **price-embedded barcodes** (EAN-13 prefix 2x encodes weight or price — common on deli scales) as a parser in `pos-domain` with property tests.
- PLU quick codes + a configurable tile grid for unbarcoded items.
- Price lists (Phase 4): store-scoped and time-effective (`valid_from/valid_to`); resolution order: promotion > store price list > base price. Until then: one base price per product.
- Cost (WAC) lives with inventory (C.7), not on the product's sell price.

**Home:** schema + FTS5 search index in `pos-db` (search-as-you-type < 50 ms per budget); lookup/parse rules in `pos-domain`; CRUD in back office → sync down; minimal local product editor in terminal for emergencies (permission-gated).

## C.2 Checkout & the cart machine (Phase 1)

Extends the blueprint §8 state machine. Cart operations, all as `pos-domain` transition functions returning `Result`:
- add item (scan/search/tile), set quantity, remove line, **line void** (before completion; audited if after tendering started — which is only reachable by returning `Tendering → Building`),
- **price override** (permission `price.override`, reason required, audited; floor = configurable min-margin or cost),
- line discount / cart discount (C.9 does automatic promotions; manual discounts are permission-scoped: cashier ≤ X%, manager above),
- customer attach/detach (C.8), buyer-TIN capture for B2B invoices (B.2),
- **park** (persists full cart, prints optional park slip with barcode to re-pull) / **resume**; parked carts expire end-of-day with a report,
- returns mode entry (C.5) and training mode (§9 blueprint: watermark + excluded from everything fiscal — implemented as a sale flag checked *everywhere*, incl. the JoFotara queue which must skip it).

**Invariants (proptest targets, extending Phase 0's):**
- `total = Σ line_gross − Σ discounts + Σ tax_added (exclusive mode only) + cash_rounding_adjustment`
- Σ line taxes = receipt tax summary per rate, exactly.
- No operation on a `Complete` sale mutates it — corrections are new documents.
- Recompute after any op < 16 ms (budget) — keep the recompute pure and incremental-friendly.

**Finalizing is atomic (the money moment):** one SQLite transaction writes sale + lines + tenders + stock-ledger events + outbox rows + fiscal-queue row + audit entries; *then* hardware side effects (print, drawer) run; print failure never un-finalizes a sale (E-catalog covers recovery). On restart, an in-flight `Finalizing` re-runs idempotently from persisted state.

## C.3 Tax engine (Phase 1, before first real sale)

Pure `pos-domain` module:
- Inputs: line (qty, unit price), product's `tax_category`, store tax profile (standard/ASEZ/unregistered), price mode (inclusive default), date.
- Category → components: v1 = single GST component per category with time-effective rate rows (`16%`, `4%`, `0% zero`, `exempt`); schema allows multiple components for future Special Tax.
- Inclusive: `net = gross / (1+r)`, `tax = gross − net`; exclusive: `tax = net × r`. Compute in `rust_decimal`, **round once per line** (default: half away from zero, at line level) to i64 fils; receipt summary is the exact sum of line taxes (no re-derivation — that's how you fail JoFotara total checks).
- Exempt customers/documents (rare in retail; flag exists for e.g. diplomatic sales) force a category override with audit.
- Output per line: net, tax per component, gross — all stored on the line forever.

## C.4 Tenders & payments (cash Phase 1; card Phase 2)

- **Cash:** amount-tendered entry with denomination quick-keys; change computed; cash rounding per B.5 applied only when the *final* tender is cash; drawer kicks on completion (and on manual open — permission `drawer.open`, audited, reason).
- **Card (semi-integrated):** `PaymentTerminal` trait (`authorize(amount, ref) → Approved{psp_ref, masked_pan, scheme} | PartialApproval{amount} | Declined{code} | Unknown`); the `Unknown`/timeout path *must* call `last_transaction_status(ref)` before any retry (double-charge prevention); partial approval leaves a remaining-due that flows naturally because **split tender is the core model**: a sale holds `tenders: Vec<Tender>` until `collected ≥ due`.
- Overtender only cash (change); card auth is always ≤ remaining due.
- Tender types are extensible rows (voucher/gift, "on account", wallet QR later) — each with flags: `opens_drawer`, `allows_change`, `is_cash_counted`, `refundable_to`.
- Every card tender stores `psp_ref` (reconciliation, refunds) and only masked PAN/scheme for the receipt — nothing else, ever (B.4).

## C.5 Returns, refunds, voids, exchanges (Phase 2)

The fraud-and-money feature — most rules here are anti-abuse:
- **Receipted return (the happy path):** look up original sale (scan receipt barcode / number / card-ref search / customer history). System shows lines with **remaining refundable qty** (original − already refunded — enforced as a `pos-domain` invariant with a proptest: cumulative refunds per line never exceed sold qty; refund amounts derive from the *original* line prices incl. their discounts, never current prices).
- **Refund tender routing:** default and hard rule for cards — **refund to the original card via PSP `refund(psp_ref, amount)`**; cash sales refund cash; mixed sales refund proportionally or by manager choice within originals. Cash-refund-for-card only as an explicit permission with threshold (money-laundering vector).
- **Restock decision per line:** back to stock / damaged (writes stock event `return_restock` or `return_damage`).
- **Receiptless return** (business decision, H): if allowed → current lowest price, store-credit-only recommended, hard threshold, manager approval, ID note optional (PDPL: minimize), always audited.
- **Void:** `Any → Voided` pre-completion (manager permission, reason, audited; parked carts included). **Post-completion "void"** does not exist — it's a same-day full refund document referencing the original (and a JoFotara **credit note** through the fiscal queue).
- **Exchange** = return + new sale in one flow, settling only the difference; under the hood it's exactly those two documents linked.
- Manager-PIN **escalation thresholds** (refund > X, receiptless, cash-for-card) are settings, enforced in Rust command handlers (blueprint §7), not UI.

## C.6 Shifts & cash management (Phase 2)

- **Register session (shift):** `open(cashier, float)` → sales attach to it → `close(counted_by_denomination)`. Blind close: UI collects the count *before* revealing expected; over/short computed and stored; > threshold triggers a manager acknowledgment.
- **Drawer movements:** paid-in / paid-out / drop / bank-deposit — amount, reason code, note, who, when; all feed expected-cash.
- Expected cash = float + cash tenders − cash refunds − cash rounding given away + paid-ins − paid-outs − drops.
- **X report** (read-only anytime) and **Z report** (immutable, sequentially numbered per register, closes the shift): totals by tender, by tax rate, by category; counts of voids/refunds/price-overrides/drawer-opens (the fraud tells); over/short. Z is a stored document, reprintable, synced.
- One open shift per register; sales impossible without an open shift; app relaunch resumes the open shift.

## C.7 Inventory (ledger from Phase 1; receiving/counts/transfers Phase 4)

- **Stock ledger only** (blueprint §3): `(product, store, qty_delta, type, ref_doc, cost_at_event?, who, when)` with types: `sale`, `refund_restock`, `refund_damage`, `receive`, `adjust+reason` (damage, theft, expiry, correction), `count_correction`, `transfer_out/in`, `waste`. On-hand = Σ deltas (cached, rebuildable).
- **Negative stock:** allow by default but *flag loudly* (blocking sales because the ledger is wrong punishes the customer at the register); per-store setting to hard-block for tightly run stores; negative-on-hand report for the buyer.
- **Receiving** (Phase 4): against a supplier delivery — updates qty and **WAC**: `new_wac = (on_hand×wac + qty_in×unit_cost)/(on_hand+qty_in)` (guard divide-by-zero and negative on-hand edge in domain tests). Purchase orders proper can wait; receiving can start standalone.
- **Stock count** (Phase 4): freeze-less counting — snapshot expected at count start, count physically (scanner-driven count screen), post differences as `count_correction` events with variance report.
- **Transfers** (Phase 4, multi-store): out at source (in transit) → in at destination; discrepancies become adjustments at receive.
- Low-stock alerts from `reorder_point` per product/store → back-office worklist.

## C.8 Customers & loyalty (Phase 3)

- Profile: name, phone (primary key for lookup at register), email?, **consent records** (loyalty T&C version + timestamp; marketing opt-in separately) — PDPL B.3 is the spec here. Lookup at register by phone/QR card.
- **Loyalty = append-only points ledger:** `earn` (rule: points per JOD, category multipliers later), `redeem` (as a discount line or a tender — pick *discount* first: it keeps tax math standard), `adjust` (manager, audited), `expire` (job). Balance = Σ. Redemption value rate is a setting.
- Purchase history view (register: last N; back office: full) — powers receipted-return lookup by customer.
- Anonymize-on-request: null PII, keep ledger rows against the anonymized id; export-my-data produces profile + history file.

## C.9 Promotions engine (Phase 4 — resist building it earlier)

Manual discounts (C.2) cover Phases 1–3. The automatic engine, when it comes:
- **Types, in order of real-world frequency:** % off item/category; amount off; **multibuy** ("3 for 1 JD", "buy 2 get 1 free" — the same mechanism: qty threshold → price for the group); **mix & match** (any 3 from set S for X); basket threshold ("5% off over 50 JD"); time-boxed (happy hour) and customer-group (loyalty tier) variants of all of the above.
- **Stacking rules are the actual hard part.** Ship the strict simple model: promotions have an integer **priority**; per line, best single promotion wins (no stacking); basket-level promotions apply after line promotions; manual discount excludes automatic ones on the same line unless a setting allows. Document it; never "improvise" combination behavior.
- Applied promotions become **explicit discount attributions on lines** (promotion id + amount) — reporting ("what did this campaign cost?") and JoFotara proration (B.2) both depend on it. Basket-level discounts are **prorated to lines by largest-remainder** before any fiscal document is built.
- Engine is a pure `pos-domain` function `(cart, active_promotions, now, customer?) → priced cart`, property-tested (e.g., applying promotions never increases total; proration conserves the discount to the fils).

## C.10 Users, roles, permissions (Phase 1)

Argon2id-hashed PINs, auto-lock on idle, fast cashier switch (blueprint §7). **Permissions are flat capability strings checked in Rust command handlers**; roles are named bundles (editable in back office). Default matrix:

| Capability | Cashier | Shift lead | Manager | Owner/BO |
|---|---|---|---|---|
| sale.create / park / resume | ✓ | ✓ | ✓ | — |
| line.void (pre-tender) | ✓ | ✓ | ✓ | — |
| discount.manual ≤ limit% | ✓ (limit low) | ✓ | ✓ | sets limits |
| price.override | — | ✓ | ✓ | — |
| sale.void (pre-completion) | — | ✓ | ✓ | — |
| refund.receipted ≤ threshold | ✓ | ✓ | ✓ | sets threshold |
| refund.above_threshold / receiptless / cash_for_card | — | — | ✓ | — |
| drawer.open (no sale) | — | ✓ | ✓ | — |
| cash.paid_in_out / drop | — | ✓ | ✓ | — |
| shift.open/close | ✓ (own) | ✓ | ✓ | — |
| zreport.run | — | ✓ | ✓ | — |
| product.edit (local emergency) | — | — | ✓ | ✓ |
| catalog/price/promotion admin | — | — | — | ✓ |
| user.admin, settings, reports.all | — | — | ✓ (store) | ✓ |
| training_mode.toggle | — | ✓ | ✓ | — |

Every ✓ that reverses money or opens the drawer also writes the hash-chained audit log with actor, reason, and (for escalations) the approving manager.

## C.11 Receipts, documents & the fiscal pipeline (receipts Phase 1; JoFotara Phase 2–3)

- **Receipt anatomy (thermal, 80 mm default / 58 mm supported):** logo (raster) → merchant legal name, address, **TIN** → doc type (SALE / REFUND / **DUPLICATE** watermark on reprints / TRAINING watermark) → receipt no. + register + cashier + datetime → lines (name, qty × unit, line total; discount attributions beneath) → subtotal, discounts, **tax summary by rate (net / tax / gross per rate)**, cash-rounding line, total → tenders + change → loyalty balance if attached → **JoFotara QR + UUID** → footer (return policy, thank-you, ar/en).
- **Arabic on ESC/POS:** do not fight printer codepages — **render receipts as raster images** (layout engine → bitmap → `GS v 0`) for perfect Arabic shaping/RTL and bilingual mixing; keep a plain-codepage fallback for exotic printers. Golden-file tests render fixtures to bytes and diff (blueprint §8) — add an Arabic fixture from day one.
- **Templates:** receipt layout as data (header/footer text, logo, toggles) editable in back office, versioned; the renderer lives with `pos-hardware`'s printer boundary, fed by a `ReceiptModel` built in `pos-domain`.
- **Email/SMS receipt (Phase 2/3):** PDF or hosted-link render of the same `ReceiptModel`; requires consent capture at request time.
- **Fiscal pipeline:** per B.2 — builder (UBL 2.1 from the finalized sale), queue, client, QR persistence, credit notes for refunds, reconciliation view. Gate Phase 2 exit on sandbox clearance of: plain sale, discounted sale (prorated), multi-rate sale, refund credit note.

## C.12 Reporting (X/Z Phase 2; back-office suite Phase 4)

All reports are queries over the three ledgers — no report writes data.
- Register-side: X, Z (C.6), today-so-far, uncleared-fiscal count.
- Back office: sales by day/hour/register/cashier; by product/category (qty, net, tax, gross, margin once WAC lands); **tax report by rate for a date range** (the GST filing input, B.1); tender summary vs. PSP settlement (reconciliation); discounts/promotions cost; refunds & voids by user (fraud lens); inventory on-hand & valuation (Σ qty×WAC), movement, negative stock, low stock; loyalty liability (outstanding points × value).
- Every report exports CSV. Timezone: reports bucket by **store-local calendar day** (Asia/Amman) regardless of UTC storage.

## C.13 Back office & multi-store (Phase 3–4)

- Back office (React app on Axum/Postgres) owns: catalog, price lists, promotions, users/roles, customers, suppliers/receiving, counts/transfers, settings, reports, device health (blueprint §8 observability), fiscal reconciliation.
- **Multi-store scoping:** org → stores → registers. Catalog is org-global; prices/promotions/tax-profile/settings resolve org → store; stock and shifts are per-store; users org-level with per-store role grants.
- Central Postgres is the source of truth for *reference data*; terminals are the source of truth for *facts* (sales, stock events, shifts) — sync accordingly (C.14).

## C.14 Sync — per-entity direction & conflict policy (Phase 3)

On the blueprint §4 outbox/changelog protocol:

| Entity | Direction | Conflict rule |
|---|---|---|
| Sales, refunds, Z reports, stock events, audit log | up only | None possible — immutable facts; dedupe by UUID (idempotent apply) |
| Fiscal clearance results | up (submission) / down (QR to other registers for reprint) | Server-authoritative |
| Products, barcodes, prices, promotions, tax rules, settings, users/roles | down only | Server wins; terminal-local emergency edits (C.1) sync **up as change-requests**, flagged for back-office approval — never silently merged |
| Customers & consents | bidirectional | Last-writer-wins per field + full audit trail; ledgers (loyalty) are append-only ⇒ conflict-free |
| Parked carts | local only | Never sync (register-physical concept) |

Ordering: reference data applies in dependency order (tax rules → products → prices); facts apply in any order (append-only). The chaos test (blueprint §8) replays, drops, and duplicates — both DBs must converge byte-identical.

## C.15 Hardware behaviors (traits Phase 0–1; drivers grow per phase)

- **Printer:** status poll before finalize (paper-out warning *before* taking money); print failure after finalize → sale stands, receipt marked unprinted, one-tap reprint (DUPLICATE), incident logged; cutter + drawer-kick (`ESC p`) via printer port is the default drawer path.
- **Scanner:** keyboard-wedge first (it's just fast keystrokes + Enter — the UI's hidden input must capture scans anywhere on the sale screen, distinguishing scan bursts from typing by inter-key timing); serial/HID-POS mode later behind `BarcodeSource`.
- **Customer display / second screen (later):** running cart + total + QR for wallet pay; nice-to-have, trait it when real.
- **Scale (only if grocery/deli):** serial protocol behind a trait; until then price-embedded barcodes (C.1) cover most cases.
- **Payment terminal:** per C.4/B.4. **Simulator implements every trait** with fault injection (paper out, timeout, partial approval) — CI and demos run hardware-free (Phase 0 already started this).
- Diagnostics screen (Phase 2): fire test print, drawer kick, scanner echo test, terminal ping — the pre-release hardware-lab checklist (blueprint §8) automated.

---

# Part D — UI specification (terminal)

**Design law:** the sale screen is where the business makes money; every other screen exists to keep it honest. Optimize for a cashier's 8-hour day: ≥48 px targets, zero hover-dependence, on-screen numpad wherever numbers happen, full keyboard operability (scanning *is* typing), < 100 ms scan-to-line, optimistic UI on local data. **Arabic-first RTL with English toggle** — build every screen RTL-primary from the first commit (blueprint §9); numerals: Western Arabic digits (0–9) are standard in Jordanian retail.

**Screen map:**
1. **Lock / PIN pad** → fast user switch; shows register name, sync status, open-shift owner.
2. **Shift open** (blocking if none): float entry by denomination.
3. **Sale (home).** Three zones — *Left/Right per RTL:* cart list (line: name, qty stepper, unit, total; swipe/long-press → line menu: qty, discount, price override, void). *Center-bottom:* totals block (subtotal, discounts, tax, **TOTAL** huge), always-visible status strip (🔵 synced / 🟡 offline n queued / fiscal-pending n / training banner). *Opposite panel:* search box (FTS-as-you-type) + PLU/tile grid (tabs: favorites, categories). Global: hidden scan capture. Action bar: Park, Resume (badge = parked count), Customer, Returns, Pay.
4. **Tender.** Amount due huge; tender-type buttons (Cash / Card / …); cash: numpad + denomination quick keys + change display; card: terminal progress states (Waiting card → Processing → result) with cancel + the timeout "checking last transaction…" state made *visible*; split: list of collected tenders + remaining; Complete triggers finalize.
5. **Post-sale toast:** change due big, print/reprint/email buttons, auto-return to Sale in ~3 s.
6. **Returns flow:** find sale (scan receipt / number / card last-4 / customer) → line picker with refundable qty → restock choice → refund tender (per C.5 rules) → manager PIN modal when escalated.
7. **Manager approval modal (shared pattern):** action summary, reason picker, PIN pad — logs approver distinctly from operator.
8. **Cash management:** paid in/out, drop, count helper (denomination grid).
9. **Shift close wizard:** blind count → over/short reveal → Z preview → print & close.
10. **Settings/diagnostics (permission-gated):** device tests, printer/terminal selection, sync detail, about + update state.
11. **Local product quick-add (manager):** emergency SKU/price so the queue never stalls; syncs as change-request (C.14).

**Keyboard map (memorize-able):** `F2` search · `F4` pay · `F6` park · `F7` resume · `F9` returns · `Del` void line · `+/-` qty · `F12` lock. Barcode scans need no focus.

**Empty/edge states are designed, not defaulted:** offline banner says "Sales are safe and will sync"; fiscal-pending badge explains itself on tap; printer-out warning appears *at* Pay, not after.

---

# Part E — "Everything that can happen": the edge-case catalog

Grouped, numbered, each with the required behavior. Turn these into your test backlog — most are `pos-domain` unit/property tests or chaos-test scenarios.

**Power, crash, and state**
1. Power cut mid-`Finalizing` → on restart, resume idempotently from persisted state; no double stock event, no double outbox row (transactional write in C.2 guarantees it).
2. Power cut during card `Tendering` → terminal status query decides: approved ⇒ attach tender & finalize; declined/unknown ⇒ remain in Tendering with the truth on screen.
3. App killed with parked carts → parks persist; resume list intact.
4. SQLite `BadKey` on open (keychain wiped by OS reinstall) → explicit recovery screen (restore from server after re-auth), never silent data loss.
5. Disk full → refuse new sales gracefully (a POS that "sells" without persisting is corrupting the ledgers); alarm state.
6. Clock skew / cashier changes system time → timestamps monotonic-guarded; receipts use server-offset-corrected time when known; audit any backward jump. Never trust local clock for sequence — sequences are counters.
7. DST/timezone: storage UTC, reporting Asia/Amman calendar day (C.12); Z report day-boundary belongs to the *shift*, not the wall clock.

**Offline & sync**
8. Offline for days → sales/outbox unbounded-ish queue with disk budget monitoring; sync resumes in batches; UI stays calm.
9. Same product edited centrally while terminal offline → server wins on reference data (C.14); terminal re-prices *only* unfinalized carts on catalog apply; finalized facts untouched.
10. Duplicate push (network retry) → server dedupes by UUID; apply is idempotent (chaos test).
11. Partial batch failure → per-item ack; failed items retry; poison-pill items (schema drift) parked in a dead-letter with alert, never block the queue.
12. Two registers sell the last unit while offline → both sales stand; stock goes negative and is flagged (C.7) — inventory is a ledger, not a lock.
13. Register clone/restore from image → device id collision detection at registration; refuse to sync until re-provisioned.

**Money & rounding**
14. Split cash+card where cash rounding applies → rounding computed only on the final cash remainder; adjustment line keeps totals exact.
15. Partial card approval (gift-card-like balances) → accept as partial tender, remaining due continues; cashier can void the partial (PSP reversal) if customer bails — reversal failure ⇒ manager flow.
16. Refund exceeding remaining refundable → impossible by invariant (C.5 proptest).
17. Change due but drawer empty of denominations → paid-in flow from safe; system doesn't care about denominations for correctness, but count helper does.
18. 0.000 JOD total (100% discount / full loyalty redemption) → legal sale, tender "zero-due", fiscal doc still issued.
19. Negative-price line attempts (bottle-return style deposits later) → blocked in v1; deposits modeled as their own feature when needed, never raw negative lines (JoFotara rejects them anyway, B.2).

**Card terminal**
20. Timeout after customer tapped → the `Unknown` protocol (C.4): status query, then decide; *never* blind retry.
21. Terminal offline/unpaired at Pay → card button disabled with reason; cash path unaffected.
22. Refund to expired/cancelled card → PSP handles (funds route to bank); if PSP refund API errors, offer store-credit/cash per policy with manager approval, audit trail links attempts.
23. Settlement mismatch (PSP report ≠ POS card total) → reconciliation report pinpoints by `psp_ref`; unmatched PSP entries and unmatched tenders listed separately.

**Fiscal (JoFotara)**
24. API down at sale time → queue + receipt marked pending clearance (B.2); health counter rises; auto-retry with backoff.
25. Validation rejection (bad code mapping, rounding mismatch) → dead-letter with the ISTD error surfaced verbatim to back office; fix mapping, requeue; the *local* sale is never mutated — corrections that change amounts are credit note + new invoice.
26. Refund of a not-yet-cleared sale → hold the credit note in queue until the original clears (dependency ordering in the queue).
27. Duplicate submission after ambiguous timeout → idempotency via invoice UUID; on "already exists" responses, fetch & persist the existing QR.
28. Sandbox credentials in production (or vice versa) → environment banner + hard config check at startup; mismatched TIN in response ⇒ alarm.
29. Merchant not yet in a mandatory wave / unregistered micro-merchant → fiscal module cleanly disabled per store; receipts print without QR; flipping it on later backfills nothing (only forward) — document that to the merchant.

**Returns & fraud**
30. Return of an exchanged item → chain of linked documents; refundable qty follows the chain.
31. Serial refund abuse (same receipt attempted at two stores) → refunds sync as facts; server-side remaining-refundable check on connected registers; offline window risk accepted + surfaced in the refunds-by-user report.
32. Receiptless return of a never-sold (stolen) item → policy feature (C.5): store-credit only + threshold + manager caps the damage.
33. Price-override abuse (cashier discounts for friends) → override report by user, reason strings, margin-floor setting.
34. Refund after price change → refund uses original line price (C.1 rule), automatically correct.
35. Drawer-open-without-sale spikes → audit + count on X/Z; the classic theft tell.

**Catalog & pricing**
36. Barcode collision (two products share a code — happens with local relabeling) → newest active wins at scan + warning; back-office conflict report.
37. Price changed while item is in an open cart → cart keeps captured price (customer saw the shelf); *new* adds get the new price; if that offends policy, a "reprice cart" manual action exists.
38. Product deactivated with stock remaining → sellable=false blocks *adding*, not refunds; stock still counted.
39. Unknown barcode scan → fast path: open quick-add (permission) or "unknown item" prompt with department + price (auditable, taxed by department category) — the queue must not stall (policy toggle, H).
40. Price-embedded barcode with checksum error → reject scan, honest error, no guessing.
41. Unicode names (Arabic + emoji in product names) → full UTF-8 through DB, receipt raster path (C.11) makes printing safe.

**Inventory**
42. Count during trading → snapshot-based variance (C.7) tolerates sales mid-count.
43. Receiving with wrong cost (fat finger 10× ) → WAC guard: cost deviation > x% from last ⇒ confirm; corrective `adjust` recomputes.
44. Transfer arrives short/damaged → receive-with-discrepancy creates adjustment at destination + notification to source.
45. Expiry-dated goods (if grocery) → lot tracking is a *later* module; v1: expiry-waste adjustments by reason.

**Documents & hardware**
46. Paper out mid-receipt → printer status polling; reprint produces DUPLICATE; JoFotara QR reprints identically (persisted payload).
47. Reprint requests days later → any synced register can reprint from facts + stored QR (C.14 down-sync of clearance).
48. Email receipt bounce → logged, no retry storm; receipt remains printable.
49. 58 mm printer at a kiosk → responsive template (two width profiles), golden files for both.
50. Cash drawer jammed/open at shift close → close proceeds; drawer state logged; hardware alert.

**People & access**
51. Cashier forgets PIN → manager resets; old PIN hash retired; audit.
52. Manager approval for the manager's own sale → approval requires a *different* user id than operator when policy demands (setting).
53. Shift left open overnight → next open detects stale shift → force-close flow with manager, flagged in reports.
54. Training mode left on → glaring banner + watermark + excluded everywhere (C.2); auto-off at shift close.
55. Terminated employee's PIN → deactivation syncs down at next contact; offline terminals honor a max-offline-auth window (setting) — a real limit of offline-first, disclosed.

**Platform & lifecycle**
56. Auto-update mid-shift → never (blueprint §8): download background, apply at register close; failed update rolls back (Tauri updater + staged rollout).
57. License expiry offline (blueprint §7) → generous grace → read-only degrade, never a locked register mid-day.
58. Migration failure on update → app refuses to run on half-migrated DB, offers rollback path; migrations tested up *and* down in CI.
59. Sentry/telemetry offline → buffered, capped, never blocks selling; PII scrubbing verified by test.
60. Multi-monitor / resolution chaos → sale screen min-size guard; kiosk fullscreen mode.

---

# Part F — Schema additions (SQLite; mirror on Postgres)

Beyond Phase 0's `0001_init` (product, sale, sale_line, sale_tender, sync_outbox, sync_cursor), the feature set above implies migrations — all tables follow blueprint §3 principles (UUIDv7 BLOB PKs, i64 minor units, soft-delete tombstones on reference data, append-only facts):

- `0002` catalog depth: `barcode(product_id, code UNIQUE, kind)`, `category`, `tax_category`, `tax_rate(tax_category_id, rate_ppm, valid_from, valid_to)`, `store`, `register`, product columns (`name_ar`, `category_id`, `tax_category_id`, `is_weighed`, `unit`, `active`).
- `0003` people: `user(pin_hash, active)`, `role`, `role_capability`, `user_role(store_id?)`, `audit_log(prev_hash, hash, actor, action, entity, payload, at)`.
- `0004` money ops: `shift(register_id, opened_by, float_minor, opened_at, closed_at, counted_minor, over_short_minor, z_number)`, `cash_movement(shift_id, kind, amount_minor, reason, by, at)`; `sale` gains `shift_id`, `status`, `doc_type(sale|refund)`, `ref_sale_id`, `training bool`, `customer_id?`, `buyer_tin?`, `rounding_adj_minor`.
- `0005` stock: `stock_ledger(product_id, store_id, qty_delta_milli, kind, ref_id, cost_minor?, by, at)`, `stock_cache(product_id, store_id, on_hand_milli, wac_minor)`.
- `0006` fiscal: `fiscal_queue(sale_id, doc_kind(invoice|credit_note), payload_json, state(queued|sent|cleared|rejected|dead), attempts, last_error, depends_on?)`, `fiscal_result(sale_id, uuid, qr_payload, cleared_at)`.
- `0007` customers/loyalty: `customer(name, phone UNIQUE?, email, anonymized bool)`, `consent(customer_id, kind, version, at)`, `loyalty_ledger(customer_id, points_delta, kind, ref_id, at)`.
- `0008` pricing/promos: `price_list(store_id?, valid_from/to)`, `price(price_list_id, product_id, unit_minor)`, `promotion(kind, config_json, priority, valid_from/to, store_scope)`, `sale_line_discount(sale_line_id, promotion_id?, manual_by?, amount_minor)`.
- `0009` receiving/counts: `supplier`, `goods_receipt(+lines: qty, unit_cost_minor)`, `stock_count(+lines: expected, counted)`, `transfer(+lines)`.

Quantities in **milli-units** (i64, 3 dp) to carry weighed items with the same integer discipline as money.

# Part G — Build order, restated with this plan folded in

| Phase | Add from this document | Exit criterion additions |
|---|---|---|
| 0 (done) | — | ✔ as shipped |
| 1 Sellable MVP | C.1 (base), C.2, C.3, C.4 cash, C.7 ledger-only, C.10, C.11 receipts (Arabic raster + goldens), D screens 1–5, 10, 11; migrations 0002–0003 + sale columns | Cash store trades all day offline; tax report by rate correct vs. hand-check; Arabic receipt golden passes |
| 2 Money-grade | C.4 card, C.5, C.6, C.11 fiscal **sandbox**, C.12 X/Z, D 6–9; migrations 0004–0006; diagnostics | Card reconciles to the fils vs. PSP report; the four sandbox fiscal docs clear (C.11); blind Z balances a scripted day incl. drop/paid-out; edge cases 1, 2, 14, 20, 24–27 have automated tests |
| 3 Connected | C.8, C.13 core, C.14, fiscal **production** cutover with advisor sign-off (B.2 open question answered in writing); migration 0007 | Blueprint chaos-convergence + fiscal reconciliation report clean over a chaos week |
| 4 Depth | C.1 price lists, C.7 receiving/counts/transfers, C.9, C.12 suite, C.13 multi-store; 0008–0009 | 3-store pilot week; a real promotion runs and its cost report matches finance's arithmetic |
| 5 Harden/launch | Blueprint §10 + PDPL walkthrough (consent, export, anonymize demo), QSA SAQ, JoFotara reconciliation drill, restore drill incl. keychain-loss recovery (E.4) | Pilot merchants + signed compliance story |

# Part H — Decisions only the merchant/owner can make (ask before coding each)

1. Returns: window (14/30 days?), receiptless allowed? store-credit-only? thresholds?
2. Cash rounding step & direction (default 10 fils nearest) — and does the store *want* 3-decimal shelf prices or 2?
3. Manual discount caps per role; price-override floor (cost? cost+x%?).
4. Negative stock: allow-and-flag (default) or hard block?
5. Refund cash-for-card ever allowed? Ceiling?
6. Loyalty: earn rate, redemption value, expiry?
7. Unknown-barcode policy: quick-add vs. department-sale vs. block (E.39)?
8. Escalation thresholds (refund amount, drawer opens) and whether manager-self-approval is banned (E.52).
9. Receipt footer legal text + return policy wording (ar/en); logo.
10. Store tax profile: standard / ASEZ / unregistered (B.1) — per store.
11. JoFotara: merchant's wave/obligation status, credentials custody, and the offline-clearance procedure in writing (B.2).
12. Data retention periods (sales docs, audit, customer inactivity purge) with the accountant (B.6, B.3).

# Part I — Sources (key)

- JoFotara mandate, phases, clearance/QR/UBL 2.1, penalties: Pagero regulatory updates (Jordan); VATupdate briefing (Mar 2026); EDICOM Jordan e-invoicing pages; VATit Jordan guide; ClearTax Jordan e-invoicing pages; OrchidaTax JoFotara guide; RTC Suite overview; field integration behavior: Odoo JoFotara POS modules (tax2gov / POS JoFawtara).
- GST rates & scope: PwC Worldwide Tax Summaries (Jordan); SalesTaxHandbook Jordan; Lloyds Bank Trade tax profile (reduced rates); BDO Jordan VAT Navigator; Quaderno Jordan GST guide.
- PDPL: official text (MoDEE PDF); Securiti, Clyde & Co, Ardent Privacy, DLA Piper summaries (dates, 24-hour breach notice, retroactivity).
- PCI semi-integrated scoping: PCI SSC SAQ family guidance (validate with QSA).
- Retail domain practices (shifts, X/Z, blind counts, WAC, returns controls): standard retail-operations literature; encoded here as explicit rules for testability.

*End of master plan. The blueprint says how to build; this says what to build and why the law and the shop floor demand it.*

---

# Part J — Comprehensiveness audit: the full universe map

*(Added after cross-checking the plan against external taxonomies — this part is both the audit result and your ongoing checklist.)*

## J.0 Method — how "comprehensive" was verified, and how you can re-verify

The feature set above was diffed against four independent references: **(1)** industry POS RFP taxonomies (TEC's POS template — ~1,471 criteria across POS Transaction / Register / Sale Slip / Price / Inventory / Reporting management — and the OMG-hosted retail RFP templates that carry the NRF/ARTS standards lineage); **(2)** **Oracle Retail Xstore** functional documentation — the reference enterprise POS, whose user guide enumerates the full transaction universe (sales, returns, layaway, special order, send sale, work orders, warranty, gift registry, serialized items, non-merchandise items, post-void, electronic journal, till/banking, time & attendance); **(3)** vertical checklists (Shopify et al.) for grocery/fashion/beauty deltas; **(4)** Jordan's Ministry of Industry, Trade & Supply enforcement practice and Consumer Protection Law No. 7 of 2017.

**Your falsifiable test:** take *any* POS vendor's feature page. Every bullet on it must map to a row in J.1 (or a section of Part C). Anything that doesn't map is a genuine gap — add it to this table with a status. That's what "comprehensive" means operationally: not "nothing exists beyond this document," but "everything that exists has a deliberate status here."

## J.1 Capability universe & status

Status: ✅ specified (section) · 🔜 planned (phase) · 🧩 designed-out for v1 *with the architectural hook that makes adding it cheap* · 🚫 out of product scope (rationale).

**Selling & documents**
| Capability | Status |
|---|---|
| Sale, return/exchange, void, park/resume, receipt reprint | ✅ C.2/C.5/C.11 |
| **Post-void** (voiding a *completed* transaction) | ✅ as "same-day full refund credit note" C.5 — deliberately never an in-place mutation |
| **Electronic journal** (cashier-facing searchable log of every document & event, receipt lookup) | 🔜 Phase 2 — thin UI over the facts + audit tables; add to D as screen 12 |
| Gift receipt (price-hidden) → return to store credit at hidden value | 🧩 hook: `doc_type` variant + masked template; Phase 4 if merchant gifts matter |
| Quotation / proforma (no stock, no fiscal, convertible to sale) | 🧩 hook: new doc_type, no ledger events; JoFotara *not* invoked |
| **Layaway** (deposit → installments → release; forfeiture policy) | 🧩 hook: doc_type + payments ledger against it; Jordan minimarkets rarely use it — build on demand |
| **Special order** (item not in store; deposit; arrival notify) | 🧩 same hook family as layaway |
| **Send sale / delivery + COD** (ship after purchase; driver cash reconciliation) | 🧩 hook: fulfillment status on sale + COD tender flag; Phase 5+ if merchant delivers |
| Work orders (repairs/alterations), rentals, consignment, subscriptions | 🚫 v1 — service-industry epics; domain core (documents+ledgers) supports them later |
| Omnichannel: e-commerce sync, click & collect | 🧩 hook: server API is the integration point; sale doc gains `channel` + `pickup` status; Phase 5+ |

**Items & catalog**
| Capability | Status |
|---|---|
| Standard / weighed / price-embedded / PLU / tiles | ✅ C.1 |
| **Non-merchandise items**: gift-card sale & top-up, gift wrap, fees, **telecom e-recharge top-up** (a Jordanian minimarket staple — sold via supplier API, no stock) | 🔜 fees Phase 2 (bag fee etc. = non-stock product with own tax cat); e-recharge 🧩 hook: `is_service` product + supplier-API driver with the *same Unknown-outcome discipline as card terminals* (E.66) |
| Bottle/container deposits | 🧩 hook: linked deposit line (positive on sale, negative on return doc) — *not* raw negative lines |
| Age-restricted items (tobacco 18+) | 🔜 Phase 2 — `min_age` on product ⇒ confirmation prompt, decline = line removal, audited (E.69) |
| Serialized items (IMEI/serial capture at sale & return) | 🧩 hook: `sale_line_serial` table; Phase 4+ for electronics merchants |
| Lot/expiry tracking | 🧩 grocery/pharmacy epic; expiry-waste adjustments cover v1 (C.7) |
| Kits/bundles (sell one, deplete many) | 🔜 Phase 4 — BOM table + exploding stock events |
| **Matrix variants** (fashion: style × size × color grid UX) | 🧩 data model already = variant-as-SKU; the *matrix editor/picker UI* is the Phase 4+ add |
| RFID/EPC | 🚫 v1 — enterprise apparel tech |

**Tenders**
| Capability | Status |
|---|---|
| Cash, card (semi-integrated), split, partial approval | ✅ C.4 |
| **Gift card / stored value** (sell, top-up, redeem, balance check) + **store credit** (from returns) | 🔜 Phase 4 — stored-value **ledger** (append-only, like loyalty) + liability report (C.12); redemption is a tender; **online-authorized only** or explicitly capped offline risk (E.61) |
| Coupons: store vs manufacturer | 🧩 store coupons = promotions with codes (C.9); manufacturer coupons (tender-like, reimbursable) 🚫 until a Jordanian clearinghouse reality demands it |
| **On-account / house credit** (B2B; ties to JoFotara *receivable* invoice type, B.2) | 🧩 hook: customer credit limit + AR ledger; receivable fiscal doc already anticipated — Phase 5 / on demand |
| **CliQ / local wallet QR** (Jordan's instant-payment rail) | 🔜 evaluate with the merchant's bank in Phase 2 alongside terminals — behaves like a terminal driver: request-to-pay by reference, poll on lost callback (E.65) |
| Cheque | 🧩 tender row with `is_cash_counted=false`; trivially enable if a wholesale merchant asks |
| Tips / gratuity / service charge | 🚫 retail v1 — hospitality epic (J.2) |
| DCC / multi-currency settlement | 🚫 per B.5 |

**Store operations**
| Capability | Status |
|---|---|
| Shifts, blind close, X/Z, drawer movements | ✅ C.6 |
| **Till attach/detach** (removable drawer inserts moved between registers) | 🧩 v1 collapses till=shift-on-register (the small-store reality); hook: `till_id` on shift if a supermarket client appears |
| Time & attendance (clock in/out), commissions | 🚫 v1 — payroll domain; shifts here are cash-accountability only (B.6) |
| Store messaging/tasks, price-check station mode | 🧩 later; price-check = read-only scan screen, cheap Phase 4 add |

**Inventory & supply**
| Capability | Status |
|---|---|
| Ledger, receiving+WAC, counts, transfers, adjustments | ✅ C.7 |
| Purchase orders (suggested ordering from reorder points) | 🔜 Phase 4/5 — receiving already stands alone |
| **Vendor returns (RTV)** | 🔜 with PO work — stock event `rtv` + supplier credit note record |
| **Label & shelf-tag printing** (barcode + **price**) | 🔜 **promoted to Phase 4 compliance feature** — see J.3: price display is actively enforced in Jordan; price-change ⇒ reprint worklist |
| Electronic shelf labels, dropship | 🚫 v1 |

**Customers & channels**
| Capability | Status |
|---|---|
| Profiles, consent, loyalty ledger, history | ✅ C.8 |
| Gift registry | 🚫 v1 (department-store feature) |
| Segments/campaigns (SMS/WhatsApp marketing) | 🧩 back-office export honors marketing consent (B.3); actual messaging = integration, Phase 5+ |
| Customer-facing display; self-checkout; handheld queue-busting | 🧩 display = C.15 later-trait; self-checkout & handheld are new shells over the same domain — the hexagonal payoff, not v1 |

## J.2 Vertical deltas (make the v1 target explicit)

**v1 target: general retail / minimarket / small chain (Jordan).** Deltas if the merchant is:
- **Grocery/supermarket:** scale integration + certified weights (JSMO metrology applies to the *scale hardware*), lot/expiry, deposits, high-volume PLU produce, price-check stations, controlled-price staples (J.3).
- **Fashion:** matrix variants UI, gift receipts, seasonal markdown management (a promotions-engine mode), higher exchange traffic.
- **Electronics:** serialized items, warranty records, IMEI on receipts, supplier RTV weight.
- **Hospitality (café/restaurant):** a **separate epic**, same domain core — tables/tabs, coursing & kitchen display, forced modifier groups, split-by-seat, tips & service charge, open-price items. Do not promise it casually; the checkout state machine grows states (Open tab ↔ Building) and the UI is a different shell.
- **Pharmacy:** 🚫 — regulated dispensing is its own product.

## J.3 Jordan consumer-trade compliance (addendum to Part B)

Beyond tax/fiscal/PDPL, the **Ministry of Industry, Trade & Supply actively inspects retail**: its Market Monitoring Directorate runs tens of thousands of visits, and its violation statistics are dominated by **failure to display prices** and **selling above set prices** on price-controlled basic commodities; it also oversees **clearances and promotional offers** and tracks refusal-to-sell/hoarding of staples. The Consumer Protection Law No. 7 of 2017 mandates **price transparency** (clear, visible prices; no hidden fees), truthful promotion, and **redress** (refund/replacement) for defective goods.

**Product consequences (now folded into the plan):**
- **Shelf/label printing with price is a compliance feature** (J.1 inventory) — plus a *price-changed ⇒ labels-to-reprint* worklist in back office.
- **Displayed price wins:** a dedicated override reason `displayed_price` (permission-light, always audited, auto-feeds the label worklist) — E.70.
- **Controlled-ceiling items:** optional `max_price` on product; sale above it hard-blocks (E.71). Rare, but the fine is real.
- **Defective-goods returns** are a distinct return reason (rights-based, may bypass the change-of-mind window per policy) — extends C.5.
- **Promotions must be honest:** the C.9 attribution reporting doubles as your inspection-day evidence of what an offer actually charged.

## J.4 Edge cases 61–72 (extending Part E)

61. Gift card sold offline, redeemed at another store before sync → stored value is **online-authorize-only** by default; an explicit "offline redeem up to X" setting exists only as accepted, quantified risk.
62. Store credit issued offline at two stores to the same customer → credits are append-only ledger entries (conflict-free); *redemption* checks server balance when online, capped offline.
63. Photocopied store coupon code → single-use codes marked used on redemption sync; offline window risk surfaced in promo report.
64. E-recharge: supplier API accepted but app crashed before receipt → same `Unknown` discipline as card terminals: idempotency key per request, status-query before retry; never resell the same key.
65. CliQ/wallet: customer paid, callback lost → poll by payment reference before declaring unpaid; tender stays `pending` state, never silently dropped.
66. E-recharge supplier down → item unsellable with honest message (it's a service, not stock).
67. Layaway (if enabled) lapses unpaid → forfeiture per policy: deposit handling (refund vs fee) is a merchant decision (H), events audited, stock released.
68. Serialized return where serial ≠ any sold serial → block with manager override path; the anti-swap fraud control.
69. Age-check declined → line removed, decline audited (pattern reporting protects the merchant at inspection).
70. Shelf tag says 0.99, system says 1.09 → charge 0.99 via `displayed_price` override; item lands on label-reprint worklist (J.3).
71. Controlled staple priced above ministry ceiling → hard block at catalog save *and* at sale (belt and braces).
72. House account (if enabled) hits credit limit while register offline → per-customer offline exposure cap; same philosophy as 61.

## J.5 Additions to Part H (merchant decisions)

13. Gift cards / store credit offered? Expiry & liability policy? 14. Telecom e-top-up: which supplier/aggregator, commission model? 15. Accept CliQ/wallet QR — via bank terminal or direct? 16. Age-restricted assortment list? 17. Fees charged (bags, delivery)? 18. Layaway or house accounts for any customers (B2B)? 19. Label printer hardware & shelf-tag format? 20. Any price-controlled staples in assortment (J.3)?

## J.6 Sources added by this audit

TEC POS RFP taxonomy; OMG retail (ARTS-lineage) RFP templates; Oracle Retail Xstore POS User Guide 24/25 (transaction & function taxonomy: layaway, special orders, send sale, work orders, warranty, gift registry, serialized items, non-merchandise incl. gift-card top-up & phone recharge, post-void, electronic journal, till/banking, time & attendance); Shopify POS requirements (vertical deltas); Petra (Jordan News Agency) MoITS Market Monitoring enforcement reports (price-display & set-price violations, promotion oversight); Consumer Protection Law No. 7 of 2017 summaries (price transparency, redress).
