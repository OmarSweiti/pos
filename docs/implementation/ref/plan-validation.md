# Plan validation — auditing the master plan before building on it

**Audited:** [`docs/plan/business-functional-master-plan.md`](../../plan/business-functional-master-plan.md)
(Parts A–J) against independent sources on `2026-08-20`.
**Revised:** `2026-08-25` after the independent seven-lens audit.
**Method:** primary government texts, official forms, standards, and regulator material settle a
claim where they are public. Implementer documentation and open-source integrations are
corroborating evidence only; a Jordan-profile value that the official package or a named advisor
must settle remains provisional. Architectural claims were checked against the shipped Phase-0
code in `crates/`.

> **Revision note (`2026-08-25`).** The original audit found four corrections, but later review
> found that C-2 and C-3 were themselves wrong and that C-1 and C-4 needed narrower premises. The
> controlling correction ledger is [`00-master-plan.md`](../00-master-plan.md) §4a, "Errata and
> concordance". This audit-of-record marks each correction's current status in place; the source
> plan remains an immutable historical input.

**Verdict (revised `2026-08-25`): build from the remediated implementation set, not from the source
plan as written.** The master plan's foundations remain stronger than those of most commercial POS
specifications, especially the four decisions that are expensive to retrofit:

1. **Sales are immutable facts; corrections are new documents.** This single rule eliminates the hardest class of sync conflict and the hardest class of audit dispute.
2. **Price and name are captured onto the sale line at sale time.** Reports and refunds read the line, never today's catalog. Half of all historical-data bugs in POS systems are this rule missing.
3. **Stock is a ledger, not a column.** On-hand is a derived cache. Append-only rows merge across offline registers without a lock.
4. **Tax rates are data with effective dates, not code.** Jordan changes reduced rates by Cabinet decree; a hardcoded 16% is a re-release every time.

Add to that: correct instinct on keeping card data outside the POS process, correct instinct on
treating card timeouts as *unknown* rather than *failed*, correct instinct on rasterising Arabic
receipts instead of fighting printer codepages, and a genuinely rare willingness to enumerate 72
edge cases before writing code.

The original four corrections now have different dispositions. Their current status follows; every
source-plan correction and superseded mapping is also recorded in the concordance named above.

---

## 1. Corrections

### C-1 — There is no JoFotara sandbox (severity: **blocks a phase gate**)

**Current status (`2026-08-25`): conclusion retained; specification premise revised.**

**Evidence confidence:** high that no public ISTD sandbox contract is documented; the authenticated
environment rules remain provisional until `2.7.0` pins the official package or a written ISTD answer.

> **Master plan B.2:** "credentials: Client ID + Secret; separate sandbox and production"
> **Master plan Part G, Phase 2 exit:** "the four sandbox fiscal docs clear (C.11)"
> **Master plan C.11:** "Gate Phase 2 exit on sandbox clearance of: plain sale, discounted sale (prorated), multi-rate sale, refund credit note."

Current implementer evidence still supports the conclusion that ISTD provides no public sandbox.
Testing against the real endpoint therefore requires credentials issued to an ISTD-registered
entity, and a submission affects that entity's real fiscal record.

Vendor marketing pages claim "sandbox verification available"; they mean *their own* staging environment, not ISTD's. Do not plan against them.

The original audit incorrectly coupled that sandbox conclusion to a second premise: that the
authoritative technical material could not be obtained before Phase 5. ISTD now publicly lists its
Technical Integration Guide. Microstep `2.7.0` obtains the official guide, XSD, business rules, and
code lists, records the package version and hash, and diffs the reconstruction **before**
`codes.rs`, the builder, or any fiscal golden is frozen.

**Consequence.** The Phase-2 exit gate as written cannot be executed by anyone, merchant or not. And with no merchant yet on this project, there are no credentials at all.

**What replaces it** (see [`fiscal-jofotara.md`](fiscal-jofotara.md) §6 and
[`phase-2-money-grade.md`](../phase-2-money-grade.md) group 2.7):

- `2.7.0` pins the official package and settles every provisional mapping before construction starts.
- A **conformance harness** encodes the pinned XSD and business rules as assertions over the built
  document and runs in CI on fixture sales.
- A **mock ISTD transport** exercises only the request, response, retry, and ambiguous-timeout
  behaviour documented by that pinned contract; the mock must not invent a `409`, fetch-existing,
  or duplicate-recovery operation.
- Five golden documents — plain, discounted, multi-rate, weighed, and credit note — are frozen only
  after `2.7.0`, pass every applicable pinned rule, and remain byte-stable; the companion training
  case proves absence and the suite exercises all 22 rules.
- Credentialed production submission remains a separate live-certification gate. The harness is
  continuous internal evidence; it is not ISTD certification and must never be described as such.

The original Phase-2 sandbox gate remains unbuildable. Moving official-package acquisition to
`2.7.0` removes the separate and more dangerous error of building the harness from a reconstruction
that was known to be provisional.

---

### C-2 — Order-level discounts are rejected outright, not merely un-prorated (severity: high)

**Current status (`2026-08-25`): superseded; reclassified `BLOCKER` because the old eligibility
gate rejects valid fiscal documents. Do not implement the correction below as originally written.**

**Evidence confidence:** high that percentage round-trip gating is mathematically wrong; final XML
placement remains provisional until `2.7.0` pins the official package.

> **Master plan B.2:** "order-level discounts must be **prorated across lines** (negative or unbalanced lines get rejected — your largest-remainder `Money::split_evenly` is exactly the right tool)"

The original audit promoted an implementation's UI restriction into an ISTD wire rule. Available
official portal material and current implementation evidence instead support line allowance
**amounts** plus a document-level recap. Microstep `2.7.0` settles the final XML placement against
the pinned official package; percentage representability is never an eligibility gate.

The retained rule is exact amount conservation:

```
basket discount (fils)
  → largest-remainder proportional allocation to line gross values
      [Money::split_proportional]
  → exact line allowance amounts in fils
  → document allowance recap = exact sum of line allowance amounts
```

An entered percentage is provenance only. If the official profile requires a percentage element,
the builder emits it at the single precision named by `DISCOUNT_PERCENT_DECIMALS`; it does not
re-derive or reject the exact allowance amount from that percentage. The prior worked example was
also false: at integer ppm, a 3-fil discount on a 7-fil line rounds to `428_571` ppm and re-derives
to 3 fils.

The load-bearing property is
`prop_document_allowance_recap_equals_sum_of_line_allowances`. See
[`fiscal-jofotara.md`](fiscal-jofotara.md) §4.3.

> ⚠️ **OPEN — blocks 2.7.0.** How many decimal places may the current JoFotara discount percentage carry when the pinned profile requires one? Default until answered: exact line allowance amounts and their exact document recap are authoritative; an entered percentage is provenance only, `DISCOUNT_PERCENT_DECIMALS` is the single emission constant, and percentage round-trip never gates fiscal eligibility.
> Owner: 2.7.0. Source that settles it: the pinned official ISTD Technical Integration Guide, XSD, business rules and accepted boundary vectors.

---

### C-3 — ISTD recomputes totals at 9 decimals; tolerance is < 0.001 JOD (severity: medium)

**Current status (`2026-08-25`): superseded; reclassified `BLOCKER` because the old invoice-level
check rejects correct multi-line documents. The claimed ISTD tolerance is not sourced.**

**Evidence confidence:** high that the old local check is arithmetically invalid; the regulator's
actual tolerance remains unresolved pending the official rules and accepted boundary vectors.

> **Master plan C.3:** "round **once per line** (default: half away from zero, at line level) to i64 fils; receipt summary is the exact sum of line taxes (no re-derivation — that's how you fail JoFotara total checks)."

The receipt rule is correct and stays: each line rounds once, and document totals are exact sums of
the carried line values. The prior audit misread implementation commentary as an ISTD acceptance
tolerance. It also proposed a self-check that rejects correct baskets: each carried line may differ
from an unrounded projection by up to half a fil, so the accumulated difference across three or
more lines can exceed one fil without any arithmetic error.

The corrected pre-submit check cannot reject an internally correct document:

```
for each line:
  compare its fixed-scale projection with the carried value at a half-fil tolerance

for the document:
  tax_exclusive_total == sum(carried line nets)
  tax_total           == sum(carried line taxes)
  tax_inclusive_total == tax_exclusive_total + tax_total
  payable_total       == the exact identity over the document's carried values
```

A failure is `QueueState::BuildFailed`, with a `Local, build failed` reconciliation row and the
audited operator command `fiscal_rebuild_failed` after the builder or pinned configuration is corrected.
The rebuild preserves the immutable sale and `fiscal_uuid`. It is not `Rejected` — ISTD did not
reject it — and it is not `Dead`, because retry exhaustion did not occur. See
[`fiscal-jofotara.md`](fiscal-jofotara.md) §4.4.

> ⚠️ **OPEN — blocks 2.7.0.** What tolerance, if any, does the current ISTD validator apply to transmitted line and document equations? Default until answered: enforce the half-fil per-line projection check and exact identities over the document's own carried values; do not implement an invoice-level tolerance or claim an ISTD tolerance.
> Owner: 2.7.0. Source that settles it: the pinned official ISTD business rules and Schematron/XSD package, plus credentialed accepted boundary vectors.

---

### C-4 — GST registration thresholds are higher than stated (severity: low, but it's a seeded default)

**Current status (`2026-08-25`): threshold numbers retained; merchant categories corrected;
severity reclassified high because the wrong default changes a merchant's registration advice.**

**Evidence confidence:** high; the official threshold regulation and GST Law settle the activity
classes, threshold amounts, mixed-activity rule, forecast test, and first-taxable-import trigger.

> **Master plan B.1:** "**Registration thresholds** (annual): ~JOD 50,000 goods / 30,000 services"

The official threshold categories carry these figures; applying one to a merchant still requires
their registered activity and evidence:

| Registered activity used for the threshold test | Threshold |
|---|---:|
| Ordinary goods seller/trader outside the special producer/manufacturer class | **JOD 75,000** |
| Services | **JOD 30,000** |
| Producer/manufacturer of goods subject to Special Sales Tax | **JOD 10,000** |
| Mixed activities | **the lowest threshold applicable to the merchant's registered activities** |

An ordinary minimarket does **not** enter the JOD 10,000 class merely because it resells tobacco or
another SST-bearing product. SST is generally charged at import or the designated domestic tax
point; assortment alone neither identifies that tax point nor proves producer liability.

**Consequence.** Onboarding records the merchant's registered activity, producer/manufacturer and
importer roles, any SST certificate or designated tax point, mixed activities, trailing turnover,
forward forecast, the statutory first-taxable-import test, and the dated evidence supporting the
answer. [`merchant-decisions.md`](merchant-decisions.md) owns the questionnaire; no assortment-based
default may decide registration.

GST registration and JoFotara obligation are independent axes. GST evidence selects the tax
profile and taxpayer category; separate official obligation or exemption evidence enables or
disables fiscal issuance. A merchant below a GST threshold may still require an income invoice, so
"GST unregistered" must never imply `fiscal_profile = 'disabled'`.

---

## 2. Confirmed as written

The architectural claims retained below still stand. Rows whose earlier legal or protocol wording
did not survive the `2026-08-25` review are marked **revised** rather than silently carried forward
as confirmations.

| Master plan claim | Status |
|---|---|
| GST standard rate 16%, administered by ISTD | ✅ |
| Zero-rated supplies include distinct domestic, export, free-zone, ASEZ, and development-area cases | ⚠️ **revised** — destination, reason, eligibility, and evidence can be supply-specific; a product category alone must not zero-rate every sale |
| Exempt: bread, water < 5 L, tea, sugar, gold, currency, electricity; plus air transport, education, sewage/waste, public health, religious & social organisations | ✅ — the plan's list was goods-only; services exemptions confirmed and added |
| Special Sales Tax as an additional per-item component | ⚠️ **revised** — the model must represent fixed and ad-valorem components, their unit basis and ordering, and GST calculated on the base including SST; a percentage-only hook is insufficient |
| Reduced rates set by Cabinet resolution ⇒ rates are time-effective data | ✅ |
| GST returns require periodic reporting | ⚠️ **revised** — `report_tax_by_rate` is a sales-side tax reconciliation, not a complete GST filing input; purchases, imports, input-tax deductibility/apportionment, credits, adjustments, elections, and return-box mapping arrive with the accounting work in Phase 4 |
| Tax rounding is a merchant-selectable store preference | ❌ **superseded** — line rounding is one versioned Jordan jurisdiction policy; its official scale and tie rule remain an open item owned by `2.7.0`, and cash rounding is a separate settlement policy; see [`tax-jordan.md`](tax-jordan.md) §4 |
| GST registration determines JoFotara obligation | ❌ **superseded** — registration/tax treatment and fiscal obligation/category are separate decisions backed by separate merchant evidence |
| A blanket `2025-04-01` B2B/B2C/B2G statement proves every merchant's JoFotara obligation | ❌ **superseded** — onboarding records the merchant's current official obligation or exemption evidence and never infers it from GST registration alone |
| The documented transport uses UBL XML in a base64-in-JSON envelope | ✅ for the documented transport shape; the exact Jordan profile is pinned in `2.7.0` |
| ISTD returns a QR and the customer handoff rule is settled for outages | ⚠️ **revised** — QR persistence is part of the response path; the legal status, wording, and QR-at-handoff rule for a pending document remain open pending an official ruling |
| Cash and receivable are settlement modes | ✅ — they feed one component of composite `InvoiceTypeCode@name`; `012` and `022` are not standalone payment-method codes |
| Buyer identification has one value threshold | ❌ **superseded** — receivable and cash documents have different buyer rules, and the pinned official package in `2.7.0` owns the exact field/scheme matrix |
| Credit notes / refunds go through the same pipeline referencing the original | ✅ — they also preserve original buyer and line identity, price, tax, and remaining refundable quantity because a later catalog or CRM edit must not change the fiscal correction |
| PDPL Law No. 24 of 2023; published `2023-09-17`; in force `2024-03-17`; grace ended `2025-03` | ✅; deployment-specific duties still require the open determination below |
| PDPL: explicit consent, purpose limitation, subject rights, restricted cross-border transfer, and sensitive-data controls | ⚠️ — the incident workflow keeps the affected-person `24 h` and supervisory-unit `72 h` clocks as interim drill defaults until the counsel-owned OPEN item at 5.3.2 settles both deadlines, content and filing channels |
| PDPL electronic controller/processor registration is unavailable | ❌ **superseded on `2026-08-25`** — MoDEE now publishes the electronic register; each deployment still needs a dated controller/processor/DPO and registration determination before customer PII processing |
| Semi-integrated certified terminals ⇒ short-SAQ territory | ⚠️ **revised** — semi-integration alone proves no SAQ eligibility; the selected deployment changes engineering and operations as §4 specifies |
| Card timeout = *unknown*, status-query before retry | ✅ — the single most important payment rule in the document |
| JOD minor-unit exponent 3 (1 JOD = 1000 fils); exponent must be per-currency data | ✅ |
| Tax-inclusive shelf pricing is the Jordanian retail norm | ✅ |
| Arabic ESC/POS via raster (`GS v 0`), not codepages | ✅ — the field consensus; codepage 1256 text mode does not shape or reorder |
| CliQ / wallet QR behaves like a terminal driver | ✅ for v1 only through a bank or CBJ-licensed merchant acquirer; a vendor-operated direct funds or acceptance path stays blocked until CBJ gives a written classification |
| Consumer Protection Law No. 7 of 2017: price transparency, truthful promotion, redress for defective goods; MoITS inspects | ✅ — J.3's promotion of shelf-label printing to a compliance feature is correct |
| Blueprint stack choices (Tauri 2 + Rust core + React, SQLCipher, Axum/SQLx/Postgres, ESC/POS, outbox sync) | ✅ as architecture; dependency currency and the SQLCipher/SQLite WAL safety prerequisite are separate checks in §5 |

> ⚠️ **OPEN — blocks 2.7.0.** Does ISTD permit asynchronous reporting during an outage, what artifact may be handed to the customer, when is the legal issuance event, what is the submission deadline, and how are backdating and later rejection handled? Default until answered: complete the sale, print only a non-fiscal payment acknowledgement, and issue the fiscal invoice only through the approved clearance path.
> Owner: 2.7.0. Source that settles it: the official ISTD outage procedure or a written ruling from the ISTD E-Invoicing Directorate.

> ⚠️ **OPEN — blocks 3.4.1.** For this deployment, which entity is controller, which is processor, who is a recipient, is a DPO required, and is the Personal Data Processing Register entry required and complete? Default until answered: the schema may migrate, but customer capture, consent collection and customer-PII sync remain disabled.
> Owner: 3.4.1. Source that settles it: the current MoDEE Personal Data Processing Register instructions and dated Jordanian counsel advice for the deployed roles.

> ⚠️ **OPEN — blocks 3.1.6.** In which country and legal entity will the shared service and each subprocessor host merchant and customer data, and what cross-border basis applies? Default until answered: no customer PII may sync or enter telemetry outside Jordan; only non-PII fixtures may use a development host.
> Owner: 3.1.6. Source that settles it: the signed hosting/subprocessor contract, Jordan PDPL transfer assessment and counsel's written conclusion.

---

## 3. Fiscal constants the master plan does not carry

The plan describes the JoFotara pipeline at prose level. Generic UBL syntax and the documented
transport can be stated now; every Jordan-profile mapping remains provisional until `2.7.0`
downloads the official package, records its version and hash, and reconciles the table with the
pinned XSD, business rules, and code lists. Implementer code corroborates a mapping but cannot
settle a conflict with that package.

| Item | Build contract before `2.7.0` closes | Status |
|---|---|---|
| Endpoint | `https://backend.jofotara.gov.jo/core/invoices/` | documented transport default; confirm and pin |
| Auth headers | `Client-Id`, `Secret-Key` | documented transport default; credential scope/rotation remains provisional |
| Body | JSON containing base64-encoded UBL XML | documented transport default; exact field schema is pinned in `2.7.0` |
| Taxpayer/document category | `income`, `general_sales`, `special_sales`, plus supported zone profiles | provisional mapping; GST registration and JoFotara category are resolved independently |
| `InvoiceTypeCode@name` | `compose_invoice_type_name(scope, settlement, taxpayer_type)` | three component tables, exhaustive supported-profile tests, and refusal of unsupported combinations; exact digits provisional |
| ICV | `fiscal_queue.icv` is nullable; allocate once at first submission from the store-scoped default counter and never regenerate | authoritative scope provisional; selling does not wait for allocation |
| Required seller field | income source sequence / activity identifier | provisional name and scheme |
| Buyer ID schemes | `TN`, `NIN`, `PN` | provisional field matrix pending the official package |
| `cbc:IssueDate` | `YYYY-MM-DD` | fixed by UBL's `xs:date` lexical form and validated by the real XSD, never a string-pattern check |
| `cbc:ID` | immutable register-prefixed invoice number | explicit and distinct from ICV and UUID |
| Fiscal UUID | locally generated `fiscal_uuid`, distinct from the sale's UUIDv7 primary key | generated at sale time for idempotency; Jordan-profile UUID constraints provisional |

The earlier table's `012`/`022` row was mislabelled: those values are composite
`InvoiceTypeCode@name` examples, not payment-method codes. The composition combines document
scope, settlement method, and taxpayer type. `codes.rs` owns the three provisional component maps
so `2.7.0` changes one isolated module instead of scattering digits through the builder.

> ⚠️ **OPEN — blocks 2.7.0.** Is the authoritative ICV namespace per register, store/income source, or one TIN across stores? Default until answered: allocate from one store-scoped counter keyed as `('store', store_id, 'fiscal_icv')`; Phase 2 uses the single register's in-process allocator, Phase 3 uses a server-issued one-value lease, and no register advances an independent register-scoped ICV counter.
> Owner: 2.7.0. Source that settles it: the official ISTD business rules or a written ISTD E-Invoicing Directorate ruling.

The public ISTD guidance listing makes official-package acquisition a Phase-2 precondition, not a
Phase-5 discovery. No `codes.rs` table, builder contract, or fiscal golden may be frozen before
`2.7.0` records the package hash and closes or explicitly carries forward each provisional row.

---

## 4. One nuance the plan understates: which SAQ

> **Master plan B.4:** "That keeps you in short-SAQ territory (e.g., SAQ P2PE-family)"

**SAQ P2PE applies only if the terminal is part of a PCI-listed, validated P2PE solution.** "Semi-integrated" and "P2PE-validated" are different properties, and a terminal can be the first without being the second. If the merchant's acquirer supplies an internet-connected terminal that is *not* on the PCI SSC's validated P2PE list, the merchant lands on **SAQ B-IP or SAQ C** — substantially longer, pulling the store network and supporting infrastructure into scope.

**Consequence.** The engineering and operating posture **does** change with the answer. SAQ B-IP
has eligibility and network-isolation requirements. SAQ C brings the payment environment's network,
configuration, patching, access control, monitoring, testing, and policy evidence into scope. PAN
must still never enter the POS process; a driver response containing full PAN is an integration
rejection, not data the application may accept and discard. Merchant-facing material must not
promise SAQ P2PE, B-IP, C, or compliance before the acquirer and QSA determine the deployed scope.
See [`security-compliance.md`](security-compliance.md) §3.

> ⚠️ **OPEN — blocks 2.1.1.** Which exact PCI SAQ applies to the selected acquirer, terminal model and firmware, PTS/P2PE listing, integration protocol, store network and support model? Default until answered: design and operate to the SAQ C baseline, reject any integration that exposes a full PAN to this process, and make no P2PE-eligibility claim anywhere.
> Owner: `2.1.1` collects the evidence; `5.3.3` determines the SAQ. Source that settles it: the acquirer's written responsibility matrix and a QSA determination against the current PCI SSC eligibility criteria.

---

## 5. Stack currency check

Dependency versions were compared with crates.io on `2026-08-20`. Version currency is not a safety
proof for the embedded SQLCipher/SQLite runtime, and the WAL prerequisite below must close before
the Phase-1 storage layer permits concurrent source connections.

| Crate / tool | Repo pins | Latest | Action |
|---|---|---|---|
| `tauri` | 2 | 2.11.5 | none |
| `axum` | 0.8 | 0.8.9 | none |
| `sqlx` | 0.9 | 0.9.0 | none |
| `rust_decimal` | 1 | 1.42.1 | none |
| `proptest` | 1 | 1.11.0 | none |
| `keyring` | 4 | 4.1.6 | none |
| `rusqlite` | **0.39** | 0.40.2 | retain until the `libsqlite3-sys` `links` collision documented in [`docs/phase-0-remaining-setup.md`](../../phase-0-remaining-setup.md) is resolved; independently verify the compiled SQLCipher/SQLite WAL safety before storage work |

Versions recorded for crates the phases may add, to be rechecked when each dependency lands:
`argon2` 0.5.3 · `cosmic-text` 0.19 · `rustybuzz` 0.20 · `tiny-skia` 0.12 ·
`image` 0.25 · `qrcode` 0.14 · `quick-xml` 0.41 · `base64` 0.23 · `reqwest`
0.13 · `jiff` 0.2 · `sentry` 0.49 · `ed25519-dalek` 3.0 · `serialport` 4.9 · `rusb`
0.9.

> ⚠️ **OPEN — blocks 1.8.1.** Does the resolved bundled SQLCipher/SQLite runtime contain the upstream WAL-reset corruption fix for every supported source-connection and checkpoint pattern? Default until answered: permit one source database connection only and do not start a concurrent checkpoint, backup, reporting, or sync connection.
> Owner: `1.8.1`. Source that settles it: runtime `sqlite_version()` and `cipher_version()` matched to the official SQLite and SQLCipher advisories/release notes, plus the upstream concurrency regression on the compiled build.

One thing to check rather than assume: `rusqlite` exposes **no `fts5` feature flag**. FTS5 arrives through the bundled SQLite build. With `bundled-sqlcipher-vendored-openssl` this needs *verifying*, not hoping — microstep 1.2.6 adds a startup assertion and a test that fails loudly if FTS5 is absent, rather than discovering it when product search silently returns nothing.

---

## 6. Gaps — things the master plan does not cover that the build needs

Not errors. The master plan is a business and functional specification and these are engineering concerns it deliberately leaves to the blueprint — but the blueprint does not carry them either, so they would otherwise be discovered at the keyboard. Each gets a design and microsteps in the phase files.

| # | Gap | Lands in | Why it cannot wait |
|---|---|---|---|
| G-1 | **Local encrypted backup + tested restore** | Phase 1 | A register holds unsynced facts. Between the sale and the successful push, the *only* copy of that money is one SQLCipher file. The blueprint puts backups in Phase 5; that is four phases of unprotected revenue |
| G-2 | **Sequence integrity** — receipt & Z numbers | Phase 1 | Per-register counters must be crash-safe and gap-detectable. A gap in a receipt sequence is what an auditor asks about first |
| G-3 | **Device provisioning / enrollment** | identity columns Phase 1, flow Phase 3 | E.13 names clone-collision detection but nothing says how a register acquires `register_id`, its store binding, or its first catalog |
| G-4 | **Business-date algorithm** | Phase 1 | Referenced throughout (E.7, C.12, the `sale.business_date` column that already exists) but never specified. It is a function of the shift and a store cutover time, not of wall-clock midnight |
| G-5 | **i18n mechanism** | Phase 1 | "Arabic-first RTL from the first commit" is a requirement without a mechanism. Needs: catalog format, key naming, RTL strategy, numeral policy, and a font that serves both the UI and the receipt rasterizer |
| G-6 | **Permission-enforcement pattern** | Phase 1 | "RBAC enforced in Rust command handlers" needs teeth — a guard type plus an exhaustive test that every IPC command declares a capability. Otherwise the twentieth command silently ships without a check |
| G-7 | **Audit hash-chain spec** | Phase 1 | `prev_hash`/`hash` columns are specified; the canonical serialization, coverage, verifier, and chain-break behaviour are not. An unverifiable hash chain is decoration |
| G-8 | **Telemetry with proven PII scrubbing** | Phase 1 (fields), Phase 3 (Sentry) | the interim 24-hour affected-person drill clock makes an untested "no PII in logs" claim operationally indefensible; the statutory clocks remain open at 5.3.2 |
| G-9 | **Performance budgets as CI benchmarks** | Phase 1 | Four budgets are stated. None is measured. Unmeasured budgets are decoration |
| G-10 | **Jordanian minimarket seed fixture** | Phase 1 | You cannot build or demo an Arabic-first, tax-inclusive, weighed-goods POS against `SKU-001 / Espresso`. The fixture is a development tool and the RTL/tax/rounding test corpus |
| G-11 | **`Money` is incomplete** | Phase 1, first | Today's `Money` is a bare `i64` with no currency and no exponent, so JOD's 3-decimal minor unit (B.5) is unrepresentable and USD/JOD cannot coexist. Everything else depends on this type |
| G-12 | **`sale_line.qty INTEGER`** | Phase 1, migration 0002 | `0001_init` uses integer quantity; Part F mandates milli-units for weighed goods. Fix before any sale row exists |

---

## 7. Where the plan is *deliberately* silent, and rightly so

Recorded so nobody "fixes" them later:

- **Multi-currency settlement** (B.5) — refused until a paying customer insists. Correct. It drags in FX accounting.
- **Hospitality** (J.2) — a different product with a different state machine. The domain core supports it; do not promise it.
- **Promotions engine before Phase 4** (C.9) — manual discounts genuinely cover Phases 1–3. Stacking rules are the hard part and are not worth solving before a merchant has an opinion.
- **PowerSync vs. custom sync** (blueprint §4) — prototype custom first. In a Tauri app, PowerSync's client SDKs would put the synced SQLite webview-side, on the wrong side of the boundary this architecture exists to draw.
- **Lot/expiry, serialized items, layaway, RFID** — designed-out with architectural hooks named. That is the correct treatment.

---

## 8. Sources

Official statutes, regulator publications, standards, and scheme-owner material settle the open
items above. Implementer documentation and source code show compatibility evidence only; they are
not described as primary ISTD or PCI authority.

**JoFotara / e-invoicing**
- [ISTD — National E-Invoicing System guidance listing](https://istd.gov.jo/AR/List/%D8%A7%D9%84%D8%A7%D8%AF%D9%84%D8%A9_%D8%A7%D9%84%D8%A7%D8%B1%D8%B4%D8%A7%D8%AF%D9%8A%D8%A9_%D9%84%D9%86%D8%B8%D8%A7%D9%85_%D8%A7%D9%84%D9%81%D9%88%D8%AA%D8%B1%D8%A9_%D8%A7%D9%84%D9%88%D8%B7%D9%86%D9%8A) — *official source proving the Technical Integration Guide is obtainable*
- [ISTD — linking procedure](https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/procedure_manual_for_linking_to_the_jordanian_national_electronic_invoicing_system.pdf) — *official transport and response procedure; does not by itself settle outage or duplicate recovery*
- [JoFotara portal manual](https://portal.jofotara.gov.jo/85e41d44095082ee4c9c.pdf) — *official portal evidence for line discount amounts*
- [OASIS UBL 2.1 XSD](https://docs.oasis-open.org/ubl/os-UBL-2.1/xsd/common/UBL-CommonBasicComponents-2.1.xsd) and [W3C `xs:date`](https://www.w3.org/TR/xmlschema11-2/#date) — *normative `cbc:IssueDate` lexical form*
- [Odoo Jordan fiscal localization](https://www.odoo.com/documentation/19.0/applications/finance/fiscal_localizations/jordan.html) and [current Jordan UBL builder](https://github.com/odoo/odoo/blob/19.0/addons/l10n_jo_edi/models/account_edi_xml_ubl_21_jo.py) — *implementation evidence only; not authority for ISTD tolerances or code lists*

**Tax**
- [ISTD — General Sales Tax Law and amendments](https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/general_sales_tax_law_and_its_amendments_2023-1.pdf) — *rates, taxable value, registration tests, filing and SST interaction*
- [ISTD — Registration Threshold Regulation No. 81/2000 and amendments](https://istd.gov.jo/EBV4.0/Root_Storage/AR/EB_Legislation/%D9%86%D8%B8%D8%A7%D9%85_%D8%AD%D8%AF_%D8%A7%D9%84%D8%AA%D8%B3%D8%AC%D9%8A%D9%84_%D9%84%D8%BA%D8%A7%D9%8A%D8%A7%D8%AA_%D8%A7%D9%84%D8%B6%D8%B1%D9%8A%D8%A8%D8%A9_%D8%A7%D9%84%D8%B9%D8%A7%D9%85%D8%A9_%D8%B9%D9%84%D9%89_%D8%A7%D9%84%D9%85%D8%A8%D9%8A%D8%B9%D8%A7%D8%AA_%D8%B1%D9%82%D9%85_81_%D9%84%D8%B3%D9%86%D8%A9_2000_%D9%88%D8%AA%D8%B9%D8%AF%D9%8A%D9%84%D8%A7%D8%AA%D9%87.pdf) — *C-4 category and threshold authority*
- [ISTD — GST declaration](https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/gst_declaration-1.pdf) and [tax-return filing manuals](https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/tax_returns_filling_manuals.pdf) — *return inputs beyond sales*
- [ISTD — current tax-rate catalogue](https://www.istd.gov.jo/AR/List/%D8%A7%D9%84%D9%86%D8%B3%D8%A8_%D8%A7%D9%84%D8%B6%D8%B1%D9%8A%D8%A8%D9%8A%D8%A9) — *enabled reduced bands must be confirmed from the current catalogue and merchant evidence*
- [ISTD — Special Tax Regulation No. 80/2000](https://istd.gov.jo/EBV4.0/Root_Storage/AR/Regulations/KTA_Document_%2839%29.pdf) — *fixed and ad-valorem component schedules; pin the current version before enabling SST*
- [ASEZA Regulation 54/2005 as amended](https://aseza.jo/EBV4.0/Root_Storage/EN/EB_List_Page/Regulation_no_54_of_2005_for_the_Goods_and_Services_Sales_Tax_in_Aqaba_Special_Economic_Zone_%28ASEZA%29_as_amended.pdf) and [ASEZA declaration](https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/aqaba_gst_declaration-0.pdf) — *zone-specific rules and return rows; no standard-profile fallback*

**Data protection**
- [Personal Data Protection Law No. 24 of 2023 — official translation (MoDEE)](https://www.modee.gov.jo/ebv4.0/root_storage/en/eb_list_page/official_translation_of_the_personal_data_protection_law_no.24_of_2023_-_stamped-2.pdf)
- [MoDEE — electronic Personal Data Processing Register announcement](https://modee.gov.jo/En/NewsDetails/%D8%A7%D9%84%D8%A7%D9%82%D8%AA%D8%B5%D8%A7%D8%AF_%D8%A7%D9%84%D8%B1%D9%82%D9%85%D9%8A_%D8%AA%D8%B9%D9%84%D9%86_%D8%A5%D8%B7%D9%84%D8%A7%D9%82_%D8%A7%D9%84%D9%85%D9%88%D9%82%D8%B9_%D8%A7%D9%84%D8%A7%D9%84%D9%83%D8%AA%D8%B1%D9%88%D9%86%D9%8A_%D9%84%D8%B3%D8%AC%D9%84_%D9%85%D8%B3%D8%A4%D9%88%D9%84%D9%8A_%D9%88%D9%85%D8%B9%D8%A7%D9%84%D8%AC%D9%8A_%D9%88%D9%85%D8%B1%D8%A7%D9%82%D8%A8%D9%8A_%D8%AD%D9%85%D8%A7%D9%8A%D8%A9_%D8%A7%D9%84%D8%A8%D9%8A%D8%A7%D9%86%D8%A7%D8%AA_%D8%A7%D9%84%D8%B4%D8%AE%D8%B5%D9%8A%D8%A9_%D8%AA%D8%AC%D8%B1%D9%8A%D8%A8%D9%8A%D8%A7%D9%8B) and [official register instructions](https://www.modee.gov.jo/ebv4.0/root_storage/en/eb_list_page/the_register_of_dpos_processors_and_controllers_instructions_-_en.pdf) — *the prior inactive-registry claim is stale*
- [MoDEE — Security Technical and Organisational Measures Instructions 2025](https://www.modee.gov.jo/ebv4.0/root_storage/en/eb_list_page/security_technical_and_organizational_measures_instructions_2025_-_en_%282%29.pdf) — *DPIA and foreign-processing controls*

**Payments & PCI**
- [PCI SSC — SAQ eligibility guidance](https://www.pcisecuritystandards.org/faqs/1443/) and [P2PE eligibility guidance](https://www.pcisecuritystandards.org/faqs/1247/) — *scheme-owner criteria; the deployed QSA still determines the exact SAQ*
- [JoPACC — CliQ services](https://www.jopacc.com/what-we-do/systems-platforms/instant-payment-system-cliq/cliq-services) and [CBJ payment-licensing guide](https://www.cbj.gov.jo/ebv4.0/root_storage/en/eb_list_page/licensing_guideline_for_electronic_payments_and_money_transfer_activities-0.pdf) — *v1 stays behind a bank/acquirer boundary*

**Consumer trade**
- [Ministry of Industry, Trade and Supply — Consumer Protection Law No. 7 of 2017](https://www.mit.gov.jo/ebv4.0/root_storage/ar/eb_list_page/%D9%82%D8%A7%D9%86%D9%88%D9%86_%D8%AD%D9%85%D8%A7%D9%8A%D8%A9_%D8%A7%D9%84%D9%85%D8%B3%D8%AA%D9%87%D9%84%D9%83.pdf)
- [Petra (Jordan News Agency) — MoITS consumer complaint & enforcement reporting](https://petra.gov.jo/en/index.php/en/news/ministry-resolves-81-percent-of-consumer-complaints-in-first-quarter)

**Engineering**
- [SQLite FTS5 extension](https://www.sqlite.org/fts5.html) — *tokenizer options for Arabic search*
- [SQLite WAL documentation](https://www.sqlite.org/wal.html) and [SQLCipher release notes](https://www.zetetic.net/blog/) — *official sources that settle the compiled-runtime WAL prerequisite; this audit does not guess the fixed version boundary*
- [Star Micronics — ESC/POS command specification](https://www.starmicronics.com/support/Mannualfolder/escpos_cm_en.pdf) — *`GS v 0` raster*
- [Tauri core releases](https://tauri.app/release/core/)
- crates.io API, queried `2026-08-20`, for every version in §5

---

*Re-run this audit before each phase gate. Last reconciled `2026-08-25`; next scheduled review is no
later than `2026-11-25`. Jordanian rates and official fiscal packages change, and a compliance claim
has a shelf life. The master plan's own J.0 rule applies here too: anything that does not map to a
row is a gap, not an absence.*
