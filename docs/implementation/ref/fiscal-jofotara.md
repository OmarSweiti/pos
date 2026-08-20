# Fiscal — JoFotara (Jordan e-invoicing)

The highest-risk component in the product. It touches a government API you cannot test against, on a schedule you do not control, with penalties for getting it wrong, and it sits directly in the checkout path.

**Read [`plan-validation.md`](plan-validation.md) §1 first.** Three of the four corrections there are in this document.

---

## 1 · What it is, in one paragraph

Jordan's national e-invoicing system (*الفوترة الوطني*), run by the Income & Sales Tax Department with the Ministry of Digital Economy. Legal basis: GST Law No. 38 of 2018 Art. 23, Regulation No. 34 of 2019, Amended Billing & Control Regulation No. 2 of 2025. It is a **Continuous Transaction Control (clearance)** system: you build the invoice, submit it, ISTD validates it and returns a QR code and a reference, and **that QR must appear on the document you hand the customer**. An invoice that never cleared has no legal standing and the buyer cannot deduct the tax on it. Phase 2 has been mandatory since **1 April 2025** across B2B, B2C and B2G — ordinary retail receipts are in scope. Penalties reported up to **JOD 500 per violation**.

---

## 2 · The three hard problems, and the answers

### 2.1 Clearance vs. offline-first

The regulation says "clear before issue." The architecture's soul is "the store sells through any outage." These conflict, and the conflict is real, not theoretical — Jordanian internet is not perfect and neither is ISTD's uptime.

**The answer is a durable fiscal queue.** The sale completes locally and unconditionally. Submission is asynchronous. On success, the QR is persisted onto the sale and the receipt can be reprinted with it. On failure the sale still stands, the receipt is marked *pending clearance*, and the queue retries with backoff. `uncleared invoices: N` is a first-class health metric on the register status strip and in back-office device health — it must never silently grow.

> ⚠️ **Open compliance question — must be answered in writing by the merchant's tax advisor before production** (master plan B.2, merchant decision #11): is a pending-clearance paper receipt acceptable, and within what window must it clear? Do not launch on an assumption. Record the answer in `merchant-decisions.md` §11 with the advisor's name and date.

### 2.2 There is no sandbox — correction **C-1**

Odoo's official Jordan localization states plainly that **no sandbox environment is available**; testing requires credentials issued to a real ISTD-registered entity, where every submission is a live fiscal document. Vendor pages advertising "sandbox verification" mean their own staging, not ISTD's.

**The answer is two local stand-ins**, built in Phase 2 (§6), plus a **certification milestone** in Phase 5 that performs the real hop once, with a merchant, under a written procedure.

### 2.3 You cannot get the authoritative spec

ISTD's field-level specification, code lists (unit-of-measure, tax category, city codes), and XSD are distributed through the **taxpayer's own JoFotara portal account**. Everything in §3 below is reconstructed from implementers and open-source SDKs. It is good enough to build against and **not** good enough to certify against.

**The answer:** obtaining the official spec is **microstep 5.2.1** and a hard prerequisite for certification. The conformance harness (§6.1) is written so that correcting a field name is a one-line change in a mapping table, not a rewrite. Expect corrections.

---

## 3 · The contract, as currently known

| Item | Value | Confidence |
|---|---|---|
| Endpoint | `https://backend.jofotara.gov.jo/core/invoices/` | high — two independent sources |
| Method | `POST` | high |
| Auth | headers `Client-Id`, `Secret-Key` | high |
| Body | JSON; the UBL 2.1 XML **base64-encoded** into a single field | high |
| Document standard | UBL 2.1 XML | high |
| Invoice type code | `388` invoice · `381` credit note | medium |
| Invoice category | `income` · `general_sales` · `special_sales`, keyed to taxpayer type | high |
| Payment method code | `012` cash · `022` receivable | high |
| Required counter | **ICV** — monotonically increasing per-taxpayer invoice counter | high |
| Required seller field | **income source sequence** (activity number) | high |
| Buyer ID types | `TIN` · `NIN` · `PN` | medium |
| Buyer identification | not required below **JOD 10,000** | high |
| Issue date format | `dd-mm-yyyy` | medium |
| Invoice UUID | rendered in **v4 shape** by every implementation seen | medium — **see the warning below** |
| Response | QR payload + invoice UUID + status | high, exact field names unconfirmed |
| Rounding tolerance | < **0.001 JOD** total drift; ISTD recomputes at 9 decimals | high |
| Global discounts | **unsupported** — per-line, as a percentage | high |

### Taxpayer type → invoice category

| Merchant registration | Category | Tax structure ISTD expects per line |
|---|---|---|
| Not registered for sales tax | `income` | no tax component |
| Registered, standard sales tax | `general_sales` | one percentage tax per line |
| Registered, special sales tax | `special_sales` | one percentage tax **plus** one fixed tax per line |

This maps onto `store.tax_profile` and the `sale_line_tax` component rows already in the schema — which is exactly why the schema allows more than one tax component per line from day one.

> ⚠️ **UUID version risk.** The blueprint mandates UUIDv7 primary keys. If ISTD's validator inspects the version nibble, submitting a v7 UUID as the fiscal UUID is rejected. **Mitigation, applied unconditionally:** the fiscal UUID is a separate column (`fiscal_queue.fiscal_uuid`, `fiscal_result.fiscal_uuid`) generated as **v4**, never the sale's v7 primary key. This costs nothing and eliminates the risk. Confirm during certification (5.2.4).

---

## 4 · The builder — turning a finalized sale into a document

### 4.1 The cardinal rule

**The fiscal document is built from the same `pos-domain` math as the receipt, never recomputed.** Totals must reconcile exactly; the fastest way to fail an ISTD total check is to have two code paths that agree in testing and diverge on the one basket with three discounts and a weighed item.

Concretely: the builder consumes the **persisted** `sale`, `sale_line`, `sale_line_tax`, `sale_line_discount`, and `sale_tax_summary` rows. It reads no product, no price list, no live tax rule. Those rows are the receipt's numbers, so the document carries the receipt's numbers.

### 4.2 Build order

```rust
// crates/pos-fiscal/src/builder.rs                              [2.7.2]
pub fn build_document(
    sale: &PersistedSale,           // sale + lines + taxes + discounts + tenders
    store: &StoreFiscalConfig,      // TIN, income source sequence, category, environment
    icv: u64,                       // from doc_sequence, same transaction as the sale
    fiscal_uuid: Uuid,              // v4
) -> Result<FiscalDocument, FiscalBuildError>;

pub struct FiscalDocument {
    pub xml: String,
    pub xml_base64: String,
    pub envelope: SubmitEnvelope,   // the JSON body
    pub uuid: Uuid,
    pub icv: u64,
    pub hash: String,               // of the XML; idempotency + dedupe
}
```

Steps, in order — each one is a checkable stage, and each is individually unit-tested:

1. **Skip check.** `sale.is_training` → state `skipped`, never submitted. `store.fiscal_profile = 'disabled'` → no queue row at all. Both are asserted by tests, because a training receipt cleared against a real TIN is a real tax document.
2. **Header.** Invoice type code (`388`/`381`), category from `store.tax_profile`, payment method (`012` cash / `022` receivable — POS default is cash), ICV, UUID, issue date in `dd-mm-yyyy`.
3. **Seller.** Legal name, TIN, income source sequence.
4. **Buyer.** Omitted below JOD 10,000 unless a TIN was captured at checkout. When captured: ID type + value + name.
5. **Lines.** One per `sale_line`: item name (Arabic), quantity, unit price, unit-of-measure code, tax category code, tax amount — all read from the persisted rows.
6. **Discounts** — §4.3.
7. **Totals** — tax-exclusive, tax-inclusive, total discount, total tax, payable.
8. **Self-check** — §4.4. A document failing this is *never submitted*.
9. **Serialize** → XML → base64 → JSON envelope.

### 4.3 Discounts — correction **C-2**

ISTD rejects global discounts and expects **per-line percentage** discounts. So proration is stage one of two:

```
basket discount, in fils
  ├─ 1. prorate to lines by line value          Money::split_proportional
  │       largest-remainder → Σ parts == basket discount, EXACTLY
  ├─ 2. per line: percent_ppm = round(line_discount / line_gross × 1_000_000)
  ├─ 3. per line: re-derive fils from percent_ppm
  └─ 4. assert re-derived == stored fils
          mismatch → FiscalBuildError::DiscountPercentDrift
                  → local dead-letter + alert, NEVER submitted
```

The re-derivation in step 4 is the whole point. A percentage is lossy; the assertion tells you *locally*, on your own machine, in a test, that a particular basket cannot be expressed the way ISTD wants — instead of ISTD telling you, on a customer's receipt, in production.

Where drift is unavoidable (a 3-fil discount on a 7-fil line has no clean percentage), the resolution is to **not create such a discount**: `price_cart` prorates by line value, so a line too small to carry its share receives zero and the remainder lands on larger lines. Property `prop_proration_never_creates_unexpressible_percentage` pins this.

`sale_line_discount.percent_ppm` exists to store the emitted percentage, so a reprint and a resubmission produce byte-identical documents.

### 4.4 The totals self-check — correction **C-3**

ISTD recomputes at nine decimals and tolerates less than 0.001 JOD of drift. Per-line rounding to fils is correct for the receipt and can accumulate past that on a long invoice.

```rust
// crates/pos-fiscal/src/totals.rs                               [2.7.3]
pub struct FiscalTotalsPolicy { pub tolerance_minor: i64 }   // 1 fil = 0.001 JOD

pub fn check(doc: &FiscalDocument, sale: &PersistedSale, p: &FiscalTotalsPolicy)
    -> Result<(), TotalsDrift>;
```

It recomputes every line and the invoice total in `rust_decimal`, unrounded, from unit price × quantity − discount, applies the tax rate, and compares against the fils the document carries. `|delta| >= tolerance` → `Err`.

**The receipt never moves.** The fils the customer paid are the truth. The check exists to convert a remote rejection into a local, debuggable, pre-flight failure with the offending line named.

### 4.5 Credit notes

A refund is its own fiscal document (`381`) referencing the original invoice's UUID, number, and full amount, plus a return reason. It goes through the identical pipeline.

**Dependency ordering (E.26):** a credit note for a not-yet-cleared invoice must wait. `fiscal_queue.depends_on` points at the parent row, and the drain loop skips any row whose dependency is not `cleared`. Property `prop_credit_note_never_precedes_its_invoice`.

---

## 5 · The queue

```rust
// crates/pos-fiscal/src/queue.rs                                [2.7.4]
pub enum QueueState { Queued, Sending, Cleared, Rejected, Dead, Skipped }

pub struct RetryPolicy {
    pub base: Duration,          // 5 s
    pub factor: f64,             // 2.0
    pub max_backoff: Duration,   // 30 min
    pub max_attempts: u32,       // 12 → ≈ 6 h of trying
    pub jitter: f64,             // 0.2 — many registers must not retry in lockstep
}
```

**Drain loop** — a background task, never in the checkout path:

1. Select `state IN ('queued')` AND `next_attempt_at <= now` AND (`depends_on IS NULL` OR parent is `cleared`), oldest ICV first.
2. Mark `sending`. Submit.
3. **`200` + cleared** → persist `fiscal_result` (UUID, QR payload, raw response), state `cleared`, decrement the health counter, mark the sale's receipt reprintable-with-QR.
4. **Validation rejection** → state `rejected`, write `fiscal_dead_letter` with the ISTD error **verbatim**, alert back office. The local sale is never mutated (E.25) — an amount correction is a credit note plus a new invoice.
5. **"Already exists"** → not an error. Fetch and persist the existing QR (E.27). This is the payoff for idempotency by UUID.
6. **Network error / timeout** → increment attempts, compute backoff, stay `queued`. After `max_attempts` → `dead` + alert. **Never mutate the sale, never resubmit under a new UUID.**

**Ordering.** ICV must be monotonic per taxpayer, so the queue drains in ICV order and a stuck document blocks those behind it *for that store*. That is deliberate: a gap in the ICV sequence is worse than a delay. The health metric makes the block visible within seconds.

**Idempotency.** The `(uuid, icv)` pair is generated once, in the same transaction as the sale, and never regenerated. Retry after an ambiguous timeout re-sends the *identical* document (E.27).

---

## 6 · Testing without a sandbox

### 6.1 The conformance harness — `crates/pos-fiscal/src/conformance.rs`  [2.7.6]

Every rule known about ISTD validation, encoded as an assertion over the built document, run in CI on every commit against a golden fixture set.

```rust
pub struct Rule { pub id: &'static str, pub description: &'static str,
                  pub check: fn(&FiscalDocument, &PersistedSale) -> Result<(), String> }

pub fn run_all(doc: &FiscalDocument, sale: &PersistedSale) -> ConformanceReport;
```

Rules at minimum:

| id | Rule |
|---|---|
| `F-001` | XML validates against the UBL 2.1 invoice schema |
| `F-002` | Invoice type code is `388` or `381` |
| `F-003` | Category matches the store tax profile |
| `F-004` | Payment method is `012` or `022` |
| `F-005` | ICV present, positive, strictly greater than the previous cleared ICV |
| `F-006` | Income source sequence present and non-empty |
| `F-007` | Seller TIN present and well-formed |
| `F-008` | Issue date is `dd-mm-yyyy` |
| `F-009` | UUID is v4-shaped |
| `F-010` | **No line amount is negative** (E.19) |
| `F-011` | **No global/document-level discount element exists** (C-2) |
| `F-012` | Every line discount carries a percentage, and it re-derives to the stated amount (C-2) |
| `F-013` | Σ line nets == document tax-exclusive total, exactly |
| `F-014` | Σ line taxes == document tax total, exactly |
| `F-015` | Tax-exclusive + tax total == tax-inclusive total, exactly |
| `F-016` | High-precision recomputation drifts < 0.001 JOD (C-3) |
| `F-017` | Every tax category code is in the ISTD code list |
| `F-018` | Every unit-of-measure code is in the ISTD code list |
| `F-019` | Buyer block present iff total ≥ JOD 10,000 or a TIN was captured |
| `F-020` | Credit note references original UUID, number, and amount |
| `F-021` | Training-mode sales produce no document at all |
| `F-022` | Every string is valid UTF-8 and Arabic text is not mangled (E.41) |

Rules `F-017`/`F-018` need the official code lists — until 5.2.1 they run against the reconstruction and are marked `provisional` in the report so nobody mistakes a green harness for certification.

### 6.2 The mock ISTD server — `crates/pos-fiscal/tests/mock_istd.rs`  [2.7.7]

An HTTP server implementing the documented contract, with fault injection driven by a header the test sets:

| Fault | Asserted behaviour |
|---|---|
| happy path | `cleared`; QR persisted; health counter decremented |
| slow (30 s) | request times out; row stays `queued`; **no duplicate submission** |
| connection refused | backoff applied with jitter; sale untouched |
| `400` validation error | `rejected`; dead letter with the verbatim body; sale untouched |
| `409` already exists | existing QR fetched and persisted; treated as success (E.27) |
| malformed JSON response | `queued` with a parse error recorded, not a panic |
| wrong TIN in response | **alarm** (E.28); state `rejected`; loud UI banner |
| `500` then `200` | second attempt sends the identical bytes; exactly one `fiscal_result` |

### 6.3 The golden set — `crates/pos-fiscal/tests/golden/`  [2.7.8]

Five fixture sales, each producing a byte-stable XML golden file reviewed on every change:

1. **Plain cash sale** — 3 lines, one tax rate, no discount.
2. **Discounted sale** — a basket discount prorated to 3 lines, each carrying a percentage.
3. **Multi-rate sale** — standard 16% + a zero-rated item + an exempt item on one receipt.
4. **Weighed sale** — a price-embedded barcode line with a fractional quantity.
5. **Credit note** — a partial refund of fixture 2, referencing it.

These five replace the master plan's "four sandbox documents" gate. They are stronger: they run forever, on every commit, and a change to any byte is visible in a diff.

### 6.4 What Phase 2 exit actually means

> **The five golden documents pass all 22 conformance rules, survive every mock fault without duplicating or losing a document, and reprint byte-identically after a restart.**

Explicitly *not* claimed: that ISTD accepts them. That claim requires §7 and nothing else can produce it.

---

## 7 · Fiscal Certification — Phase 5, milestone 5.2

The only place the real endpoint is contacted. A written procedure, executed once with the first merchant, checked off item by item.

| Step | Action | Gate |
|---|---|---|
| 5.2.1 | Obtain the **official ISTD technical specification, XSD, and code lists** through the merchant's JoFotara portal account | Documents in hand |
| 5.2.2 | Diff the official spec against the reconstruction in §3. Every difference becomes a harness correction | Harness updated; goldens regenerated and reviewed |
| 5.2.3 | Obtain production `Client-Id` / `Secret-Key`; store in the OS keyring, never in a file or the database | `fiscal_credentials_ref` populated; secret never touches disk |
| 5.2.4 | Confirm the UUID version question (§3 warning) against the spec | Answered in writing |
| 5.2.5 | Merchant's tax advisor answers the offline-clearance question (§2.1) in writing | Recorded in `merchant-decisions.md` §11 with name and date |
| 5.2.6 | Submit golden document 1 as a **live, low-value invoice**; verify the returned QR with the Sanad app | QR verifies |
| 5.2.7 | Immediately credit-note it (golden 5's path). Verify both appear in the merchant's ISTD portal | Both visible, netting to zero |
| 5.2.8 | Repeat for goldens 2, 3, 4 | All clear |
| 5.2.9 | Run the reconciliation report: local sales ↔ cleared invoices | Zero unmatched on both sides |
| 5.2.10 | Kill-the-network drill: sell offline for an hour, restore, confirm the queue drains in ICV order with no gaps | Sequence intact |
| 5.2.11 | Environment guard: confirm the app refuses to start with mock credentials in a production build and vice versa (E.28) | Both directions refuse |

**Do not attempt any of this without the merchant's informed consent in writing.** Every submission is a real fiscal document against their real tax record, and step 5.2.6 puts a live invoice on it.

---

## 8 · Reconciliation and health

**Health metrics**, on the register status strip and in back-office device health:

- `uncleared_count` — `state IN ('queued','sending')`. Non-zero is normal; growing is not.
- `oldest_uncleared_age` — the number that actually matters. An alarm threshold is a merchant decision, defaulting to 4 hours.
- `dead_letter_count` — should be zero. Anything else is a person's job today.
- `rejection_rate_24h` — a rising rate means a mapping broke, usually after an ISTD change.

**Reconciliation report** (`fiscal_reconciliation`, microstep 3.6.4), for a date range:

| Row class | Meaning |
|---|---|
| Matched | local sale ↔ `fiscal_result` ↔ ISTD portal record |
| Local, uncleared | a sale with no result — the queue's backlog |
| Local, rejected | dead-lettered; the verbatim ISTD error shown |
| Cleared, no local sale | **alarm** — a document exists at ISTD that this system did not produce |
| Training excluded | count only, proving they were correctly skipped |

The fourth row is why the report exists. It is the only way to notice a duplicate submission that succeeded under a UUID the local database lost.

---

## 9 · Crate layout

```
crates/pos-fiscal/
├── src/
│   ├── lib.rs            FiscalProfile, the enable/disable switch
│   ├── model.rs          FiscalDocument, SubmitEnvelope, ClearanceResult
│   ├── builder.rs        PersistedSale → UBL 2.1 XML          [2.7.2]
│   ├── totals.rs         FiscalTotalsPolicy (correction C-3)  [2.7.3]
│   ├── codes.rs          ISTD code lists + mapping tables     [2.7.1]
│   ├── queue.rs          durable queue, retry, dependencies   [2.7.4]
│   ├── client.rs         HTTP client, auth, idempotency       [2.7.5]
│   ├── conformance.rs    the 22 rules                         [2.7.6]
│   └── qr.rs             QR payload → raster for the receipt  [2.7.9]
└── tests/
    ├── mock_istd.rs      fault-injecting server               [2.7.7]
    ├── golden/           five documents, byte-stable          [2.7.8]
    └── queue_chaos.rs    crash/duplicate/reorder scenarios    [2.7.10]
```

`codes.rs` is deliberately a separate module of plain tables. When 5.2.2 diffs the official spec against the reconstruction, the corrections land there and nowhere else.

**`pos-fiscal` depends on `pos-domain` and nothing that can surprise it.** The builder takes persisted rows as plain structs; the client is the only module touching the network, and it is behind a trait so the mock and the real endpoint are interchangeable:

```rust
pub trait ClearanceClient: Send + Sync {
    fn submit(&self, env: &SubmitEnvelope) -> Result<ClearanceResult, ClearanceError>;
    fn fetch_existing(&self, uuid: Uuid) -> Result<Option<ClearanceResult>, ClearanceError>;
}
```

---

## 10 · Edge cases owned by this component

| # | Case | Behaviour |
|---|---|---|
| E.24 | API down at sale time | queue + receipt marked pending; health counter rises; auto-retry with backoff |
| E.25 | Validation rejection | dead letter with the verbatim ISTD error; local sale never mutated; fix mapping, requeue |
| E.26 | Refund of a not-yet-cleared sale | credit note held via `depends_on` until the invoice clears |
| E.27 | Duplicate submission after ambiguous timeout | idempotent by UUID; on "already exists", fetch and persist the existing QR |
| E.28 | Mock credentials in production, or vice versa | environment banner + hard config check at startup; mismatched TIN in a response is an alarm |
| E.29 | Merchant not in a mandatory wave / unregistered | `fiscal_profile = 'disabled'` per store; receipts print without QR; enabling later backfills nothing — **document this to the merchant** |
| E.46 | Paper out mid-receipt | QR payload is persisted, so a reprint carries the identical QR |
| E.47 | Reprint days later, another register | clearance results sync down, so any register reprints from facts + stored QR |

---

## 11 · Sources

- [Odoo 19.0 — Jordan fiscal localization](https://www.odoo.com/documentation/19.0/applications/finance/fiscal_localizations/jordan.html) — no sandbox; per-line percentage discounts; 9-decimal recomputation and < 0.001 JOD tolerance; taxpayer-type classification; credentials triple
- [`jafar-albadarneh/jofotara` PHP SDK](https://packagist.org/packages/jafar-albadarneh/jofotara) — field inventory, payment codes, invoice categories, ICV, income source sequence, buyer ID types, date format, v4 UUID shape
- [Mozon — JoFotara guide 2026](https://mozon-tech.com/en/blog/the-ultimate-guide-to-jofotara/) — endpoint, base64-in-JSON envelope
- [`sedhha/automation-script-jordan-tax-dept`](https://github.com/sedhha/automation-script-jordan-tax-dept) — endpoint confirmation
- [ClearTax — Jordan e-invoicing](https://www.cleartax.com/jo/jordan-e-invoicing) — type codes 388/381, cash & A/R sub-types
- [OrchidaTax — Jordan compliance guide 2026](https://orchidatax.com/countries-compliance/jordan-e-invoicing-compliance/) — CTC model, mandate dates
- [Flick Network — JoFotara rules](https://www.flick.network/en-jo/e-invoicing-jordan-jofotara) — JOD 10,000 buyer-identification threshold
- [VATupdate — Jordan e-invoicing briefing, Mar 2026](https://www.vatupdate.com/2026/03/20/briefing-document-podcast-e-invoicing-e-reporting-in-jordan/)

*Every value in §3 is reconstructed. §7 replaces reconstruction with fact. Until then, treat a green harness as "we did our homework," never as "we are compliant."*
