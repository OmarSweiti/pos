# Plan validation — auditing the master plan before building on it

**Audited:** `pos-business-functional-master-plan.md` (Parts A–J) against independent sources, August 2026.
**Method:** every load-bearing factual claim — tax rates, thresholds, e-invoicing mandate, data-protection law, PCI scoping, currency mechanics, library versions — was checked against a primary or authoritative secondary source. Architectural claims were checked against the shipped Phase-0 code in `crates/`.

**Verdict: build on it.** The master plan is materially better than the specifications most commercial POS products are built from. It gets right the four things that cheap POS software gets wrong, and those four are the ones you cannot retrofit:

1. **Sales are immutable facts; corrections are new documents.** This single rule eliminates the hardest class of sync conflict and the hardest class of audit dispute.
2. **Price and name are captured onto the sale line at sale time.** Reports and refunds read the line, never today's catalog. Half of all historical-data bugs in POS systems are this rule missing.
3. **Stock is a ledger, not a column.** On-hand is a derived cache. Append-only rows merge across offline registers without a lock.
4. **Tax rates are data with effective dates, not code.** Jordan changes reduced rates by Cabinet decree; a hardcoded 16% is a re-release every time.

Add to that: correct instinct on semi-integrated payments (PCI scope collapse), correct instinct on treating card timeouts as *unknown* rather than *failed*, correct instinct on rasterising Arabic receipts instead of fighting printer codepages, and a genuinely rare willingness to enumerate 72 edge cases before writing code.

Four claims are wrong. One of them invalidates a phase gate. They are corrected below and repeated inline at the exact microstep they affect.

---

## 1. Corrections

### C-1 — There is no JoFotara sandbox (severity: **blocks a phase gate**)

> **Master plan B.2:** "credentials: Client ID + Secret; separate sandbox and production"
> **Master plan Part G, Phase 2 exit:** "the four sandbox fiscal docs clear (C.11)"
> **Master plan C.11:** "Gate Phase 2 exit on sandbox clearance of: plain sale, discounted sale (prorated), multi-rate sale, refund credit note."

Odoo's official Jordan fiscal localization — a first-party implementer with a production JoFotara integration — states plainly that **no sandbox environment is available**. Testing requires credentials issued to a real ISTD-registered entity, and every submission is a live fiscal document against that entity's real tax record.

Vendor marketing pages claim "sandbox verification available"; they mean *their own* staging environment, not ISTD's. Do not plan against them.

**Consequence.** The Phase-2 exit gate as written cannot be executed by anyone, merchant or not. And with no merchant yet on this project, there are no credentials at all.

**What replaces it** (see [`fiscal-jofotara.md`](fiscal-jofotara.md) §6 and `phase-2-money-grade.md` group 2.7):

- A **conformance harness** — every documented ISTD validation rule encoded as an assertion over the built document, run in CI on a golden set of fixture sales.
- A **mock ISTD server** implementing the documented request/response contract with fault injection: timeout, validation rejection, duplicate UUID, "already exists", malformed response, slow response.
- The four golden documents (plain, discounted, multi-rate, credit note) must clear the harness and survive every mock fault, byte-stable.
- The **real network hop becomes a separate gated milestone** in Phase 5 — *Fiscal Certification* — executed once with the first merchant, against production, using low-value invoices immediately reversed by credit note.

This is strictly better than the original gate even if a sandbox existed: the harness runs on every commit forever; a sandbox run is a one-off.

---

### C-2 — Order-level discounts are rejected outright, not merely un-prorated (severity: high)

> **Master plan B.2:** "order-level discounts must be **prorated across lines** (negative or unbalanced lines get rejected — your largest-remainder `Money::split_evenly` is exactly the right tool)"

The instinct is right and the tool is right, but the requirement is stricter than stated. Odoo's Jordan localization documents that **global discounts are unsupported** and that discounts "must be applied per invoice line **as a percentage**."

So proration alone is insufficient. A prorated *absolute* fils amount on a line is still not what ISTD's validator expects.

**Consequence.** The fiscal document builder needs a second stage after proration:

```
basket discount (fils)
  → largest-remainder proration to lines      [Money::split_evenly — already built]
  → per-line absolute fils discount
  → convert to percentage of that line's gross
  → RE-DERIVE the fils amount from that percentage
  → assert it reproduces the stored fils value
```

If the re-derivation does not reproduce the stored value, the document is **not submitted** — it goes to a local dead-letter with a loud alert. A rounding argument you lose locally is free; one you lose at ISTD costs a rejected invoice, an uncleared receipt, and a customer who cannot deduct.

Property test: `prop_discount_percentage_roundtrip_is_exact`. See [`fiscal-jofotara.md`](fiscal-jofotara.md) §4.3.

---

### C-3 — ISTD recomputes totals at 9 decimals; tolerance is < 0.001 JOD (severity: medium)

> **Master plan C.3:** "round **once per line** (default: half away from zero, at line level) to i64 fils; receipt summary is the exact sum of line taxes (no re-derivation — that's how you fail JoFotara total checks)."

The *receipt* rule is correct and stays. But the plan assumes ISTD accepts the sum-of-rounded-lines as authoritative. It does not: ISTD recomputes at nine decimal places, and Odoo documents a tolerated error margin of **less than 0.001 JOD** arising precisely from this precision mismatch (three decimals on the sender's side, nine on ISTD's).

**Consequence.** Per-line rounding to fils is fine for a three-line minimarket basket and can drift past tolerance on a long or heavily-discounted one. The fix is not to change the receipt — the receipt's fils values are the money the customer paid and must never move. The fix is a **pre-submit self-check**:

```
FiscalTotalsPolicy::check(&invoice) -> Result<(), Drift>
  recompute every line and the invoice total at rust_decimal precision, unrounded
  compare against the fils values the document carries
  |delta| >= 1 fil  →  Err(Drift)  →  local dead-letter + alert, NEVER submitted
```

This turns a class of remote rejection into a local, debuggable, pre-flight failure. See [`fiscal-jofotara.md`](fiscal-jofotara.md) §4.4.

---

### C-4 — GST registration thresholds are higher than stated (severity: low, but it's a seeded default)

> **Master plan B.1:** "**Registration thresholds** (annual): ~JOD 50,000 goods / 30,000 services"

Current thresholds, over any rolling 12-month period:

| Supply type | Threshold |
|---|---|
| Goods **not** subject to Special Sales Tax | **JOD 75,000** |
| Services | **JOD 30,000** |
| Goods **subject to** Special Sales Tax | **JOD 10,000** |

The services figure was right. The goods figure was low by JOD 25,000, and the plan missed the special-tax tier entirely — which matters, because a minimarket selling tobacco crosses at JOD 10,000, not 75,000. That is a very different conversation with a merchant about whether they must register.

**Consequence.** Documentation and the seeded default in `merchant-decisions.md` §10; no code change. The "tax-disabled merchant" configuration the plan calls for remains correct and necessary.

---

## 2. Confirmed as written

Everything in this table was checked and stands. Where the plan hedged appropriately ("verify with the merchant's advisor"), the hedge was correct.

| Master plan claim | Status |
|---|---|
| GST standard rate 16%, administered by ISTD | ✅ |
| Zero-rated: exports, free zones, ASEZ, development areas | ✅ |
| Exempt: bread, water < 5 L, tea, sugar, gold, currency, electricity; plus air transport, education, sewage/waste, public health, religious & social organisations | ✅ — the plan's list was goods-only; services exemptions confirmed and added |
| Special (excise) Sales Tax as an additional per-item component — cement, tobacco, wines, spirits, cars, beer, fuel, lubricants | ✅ — schema must allow > 1 tax component per item |
| Reduced rates set by Cabinet resolution ⇒ rates are time-effective data | ✅ |
| GST returns filed periodically ⇒ the tax report by rate *is* the filing input | ✅ |
| JoFotara: ISTD + MoDEE, GST Law 38/2018 Art. 23, Reg. 34/2019, Amended Reg. 2/2025 | ✅ |
| Phase 2 mandatory from **1 April 2025**, covering **B2B, B2C and B2G** — ordinary retail receipts in scope | ✅ — architect as if every receipt must clear |
| Continuous Transaction Control (clearance) model, real-time validation before issue | ✅ |
| UBL 2.1 XML transmitted as JSON | ✅ — specifically, XML base64-encoded into a JSON field |
| ISTD returns a QR that **must** appear on the customer document | ✅ |
| Two invoice types: cash (POS default) and receivable (credit) | ✅ — codes recovered, see §3 |
| Buyer identification not required below ~JOD 10,000 | ✅ — but capture-and-retain is still advised for audit |
| Penalties up to JOD 500 per violation | ✅ |
| Credit notes / refunds go through the same pipeline referencing the original | ✅ |
| PDPL Law No. 24 of 2023; published 17 Sep 2023; in force 17 Mar 2024; grace ended Mar 2025; retroactive | ✅ |
| PDPL: explicit consent, purpose limitation, subject rights, restricted cross-border transfer, 24-hour breach notification, sensitive-data category | ✅ |
| PDPL enforcement institutions still standing up ⇒ build to the law, not to enforcement lag | ✅ — as of Aug 2026 the electronic controller/processor registry is still not activated; manual registration with the Personal Data Protection Directorate is the interim path |
| Semi-integrated certified terminals ⇒ short-SAQ territory | ✅ **with a caveat** — see §4 |
| Card timeout = *unknown*, status-query before retry | ✅ — the single most important payment rule in the document |
| JOD minor-unit exponent 3 (1 JOD = 1000 fils); exponent must be per-currency data | ✅ |
| Tax-inclusive shelf pricing is the Jordanian retail norm | ✅ |
| Arabic ESC/POS via raster (`GS v 0`), not codepages | ✅ — the field consensus; codepage 1256 text mode does not shape or reorder |
| CliQ / wallet QR behaves like a terminal driver | ✅ — CliQ merchant QR reaches merchants through bank/PSP POS devices (JoPACC × Network International), so it is a `PaymentTerminal` implementation, not a new integration class |
| Consumer Protection Law No. 7 of 2017: price transparency, truthful promotion, redress for defective goods; MoITS inspects | ✅ — J.3's promotion of shelf-label printing to a compliance feature is correct |
| Blueprint stack choices (Tauri 2 + Rust core + React, SQLCipher, Axum/SQLx/Postgres, ESC/POS, outbox sync) | ✅ — all current, see §5 |

---

## 3. Fiscal constants the master plan does not carry

The plan describes the JoFotara pipeline correctly but at prose level. Building it needs concrete values. These were recovered from implementer documentation and open-source SDKs, and are recorded in [`fiscal-jofotara.md`](fiscal-jofotara.md) with their provenance:

| Item | Value |
|---|---|
| Endpoint | `https://backend.jofotara.gov.jo/core/invoices/` |
| Auth headers | `Client-Id`, `Secret-Key` |
| Body | JSON; the UBL 2.1 XML base64-encoded into a single field |
| Invoice category (by taxpayer type) | `income` (unregistered) · `general_sales` (registered, standard) · `special_sales` (registered, special tax) |
| Payment method code | `012` cash · `022` receivable |
| Required counter | ICV — a monotonically increasing per-taxpayer invoice counter |
| Required seller field | income source sequence (activity number) |
| Buyer ID types | TIN · NIN · PN |
| Issue date format | `dd-mm-yyyy` |
| Invoice UUID | rendered in **v4 shape** by every implementation seen |

> ⚠️ **The last row is a live risk against this codebase.** The blueprint mandates UUIDv7 primary keys everywhere. If ISTD's validator inspects the version nibble, a v7 UUID submitted as the fiscal UUID is rejected. **Mitigation:** the fiscal UUID is a *separate column* (`fiscal_result.uuid`) generated as v4, never the sale's v7 primary key. This costs nothing and removes the risk entirely. Confirm against the official spec during Fiscal Certification.

> ⚠️ **The authoritative technical specification is not public.** ISTD's field-level spec, code lists (UoM, tax category, city codes), and XSD are distributed through the taxpayer's own JoFotara portal account. Every value above is reconstructed from implementers. **Obtaining the official spec is microstep 5.2.1 and is a hard prerequisite for certification.** Until then, the conformance harness encodes the reconstruction and is expected to need corrections.

---

## 4. One nuance the plan understates: which SAQ

> **Master plan B.4:** "That keeps you in short-SAQ territory (e.g., SAQ P2PE-family)"

**SAQ P2PE applies only if the terminal is part of a PCI-listed, validated P2PE solution.** "Semi-integrated" and "P2PE-validated" are different properties, and a terminal can be the first without being the second. If the merchant's acquirer supplies an internet-connected terminal that is *not* on the PCI SSC's validated P2PE list, the merchant lands on **SAQ B-IP or SAQ C** — substantially longer, pulling the store network and supporting infrastructure into scope.

**Consequence.** The engineering posture does not change — PAN never touches this process either way, and that is the point. But the *claim* changes, and merchant-facing material must not promise SAQ P2PE. The action is concrete: when evaluating Jordanian acquirers (Phase 2), ask for each candidate terminal's **PCI P2PE listing number**, and record it. See [`security-compliance.md`](security-compliance.md) §3.

---

## 5. Stack currency check

Verified against crates.io, August 2026. No stack decision in the blueprint has aged badly.

| Crate / tool | Repo pins | Latest | Action |
|---|---|---|---|
| `tauri` | 2 | 2.11.5 | none |
| `axum` | 0.8 | 0.8.9 | none |
| `sqlx` | 0.9 | 0.9.0 | none |
| `rust_decimal` | 1 | 1.42.1 | none |
| `proptest` | 1 | 1.11.0 | none |
| `keyring` | 4 | 4.1.6 | none |
| `rusqlite` | **0.39** | 0.40.2 | **stay on 0.39** — the `libsqlite3-sys` `links` collision with sqlx documented in `docs/phase-0-remaining-setup.md` is still live. Revisit when sqlx accepts `libsqlite3-sys` ≥ 0.38 |

Crates the phases will add, all healthy: `argon2` 0.5.3 · `cosmic-text` 0.19 · `rustybuzz` 0.20 · `tiny-skia` 0.12 · `image` 0.25 · `qrcode` 0.14 · `quick-xml` 0.41 · `base64` 0.23 · `reqwest` 0.13 · `jiff` 0.2 · `sentry` 0.49 · `ed25519-dalek` 3.0 · `serialport` 4.9 · `rusb` 0.9.

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
| G-8 | **Telemetry with proven PII scrubbing** | Phase 1 (fields), Phase 3 (Sentry) | PDPL's 24-hour breach clock makes "no PII in logs" a legal position, and a legal position needs a test |
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

**JoFotara / e-invoicing**
- [Odoo 19.0 — Jordan fiscal localization](https://www.odoo.com/documentation/19.0/applications/finance/fiscal_localizations/jordan.html) — *primary source for C-1 (no sandbox), C-2 (per-line percentage discounts), C-3 (9-decimal recomputation, < 0.001 JOD tolerance), taxpayer-type classification*
- [VATupdate — E-Invoicing & E-Reporting in Jordan briefing (Mar 2026)](https://www.vatupdate.com/2026/03/20/briefing-document-podcast-e-invoicing-e-reporting-in-jordan/)
- [ClearTax — Jordan e-invoicing timeline & process](https://www.cleartax.com/jo/jordan-e-invoicing) — *invoice type codes 388/381, cash & A/R sub-types*
- [OrchidaTax — Jordan e-invoicing compliance guide 2026](https://orchidatax.com/countries-compliance/jordan-e-invoicing-compliance/) — *CTC clearance model, launch date, penalties*
- [Flick Network — JoFotara rules & deadlines](https://www.flick.network/en-jo/e-invoicing-jordan-jofotara) — *JOD 10,000 buyer-identification threshold*
- [Mozon — JoFotara guide 2026](https://mozon-tech.com/en/blog/the-ultimate-guide-to-jofotara/) — *endpoint, base64-in-JSON envelope*
- [`jafar-albadarneh/jofotara` PHP SDK](https://packagist.org/packages/jafar-albadarneh/jofotara) — *field inventory, payment codes 012/022, invoice categories, ICV, income source sequence, buyer ID types, v4 UUID shape*
- [`sedhha/automation-script-jordan-tax-dept`](https://github.com/sedhha/automation-script-jordan-tax-dept) — *endpoint confirmation*

**Tax**
- [PwC Worldwide Tax Summaries — Jordan, Other taxes](https://taxsummaries.pwc.com/jordan/corporate/other-taxes) *(updated 05 Jul 2026)* — *16% standard rate, zero-rated categories, exempt goods and services, excise scope*
- [Flick Network — GST in Jordan](https://www.flick.network/en-jo/gst-in-jordan) — *registration thresholds 75k / 30k / 10k (C-4)*
- [Quaderno — Jordan GST guide 2026](https://quaderno.io/guides/jordan-gst-guide/) — *bi-monthly filing*
- [BDO Jordan — VAT Navigator](https://www.bdo.com.jo/getattachment/827089db-2161-4aec-a718-03e0ce5307bb/VAT_Jordan_2024_Final3.pdf?lang=en-GB)

**Data protection**
- [Personal Data Protection Law No. 24 of 2023 — official text (MoDEE)](https://www.modee.gov.jo/ebv4.0/root_storage/en/eb_list_page/pdpl.pdf)
- [Securiti — Jordan PDPL overview](https://securiti.ai/jordan-personal-data-protection-law-of-2023/)
- [DLA Piper — Data protection laws: Jordan](https://www.dlapiperdataprotection.com/?t=law&c=JO)
- [Nsair & Partners — DPOs in Jordan](https://nsairs.com/2025/05/20/1257/) — *registry not yet activated; manual registration interim path*

**Payments & PCI**
- [Semi-integrated POS (overview)](https://en.wikipedia.org/wiki/Semi-integrated_POS)
- [episki — PCI DSS SAQ types explained](https://episki.com/frameworks/pci/saq-types-explained) — *§4: SAQ P2PE requires a PCI-listed validated P2PE solution*
- [JoPACC — CliQ system](https://www.jopacc.com/what-we-do/systems-platforms/cliq-system) and [JoPACC × Network International merchant QR](https://www.jopacc.com/En/NewsDetails/JoPACC__NI_Launch_Instant_QR_Payment_through_CliQ)

**Consumer trade**
- [Consumer Protection — UN ESCWA Jordan brief](https://www.unescwa.org/sites/default/files/inline-files/ABLF-2023-consumer-CP-Jordan-english.pdf)
- [Petra (Jordan News Agency) — MoITS consumer complaint & enforcement reporting](https://petra.gov.jo/en/index.php/en/news/ministry-resolves-81-percent-of-consumer-complaints-in-first-quarter)

**Engineering**
- [SQLite FTS5 extension](https://www.sqlite.org/fts5.html) — *tokenizer options for Arabic search*
- [Star Micronics — ESC/POS command specification](https://www.starmicronics.com/support/Mannualfolder/escpos_cm_en.pdf) — *`GS v 0` raster*
- [Tauri core releases](https://tauri.app/release/core/)
- crates.io API, queried 20 Aug 2026, for every version in §5

---

*Re-run this audit before each phase gate. Jordanian tax rates move by Cabinet decree, JoFotara is still adding waves, and the PDPL authority is still standing up. The master plan's own J.0 rule applies to this document too: anything that does not map to a row here is a gap, not an absence.*
