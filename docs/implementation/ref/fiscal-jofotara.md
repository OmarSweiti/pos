# Fiscal — JoFotara (Jordan e-invoicing)

The highest-risk component in the product. It touches a government API with no public sandbox, on a schedule the product does not control, and it sits beside checkout without being allowed to stop a sale.

**Read [`plan-validation.md`](plan-validation.md) §1 first.** The current corrections to the immutable source plans live in [`00-master-plan.md`](../00-master-plan.md) §4a (Errata and concordance); this reference is the buildable fiscal contract.

---

## 1 · What it is, in one paragraph

Jordan's national e-invoicing system (*الفوترة الوطني*) is run by the Income & Sales Tax Department. The technical flow builds a UBL document, submits it, and receives a fiscal response that includes the data needed for the customer document. The legal issuance event, the status of a document handed over during an outage, and the merchant's obligation or exemption are evidence-backed onboarding facts; they are not inferred from GST registration, assortment, or a general commencement date. `store.tax_profile` controls tax calculation. `store.fiscal_profile` and the evidenced fiscal taxpayer category control JoFotara issuance. A merchant may be unregistered for GST and still require an `income` fiscal document.

---

## 2 · The three hard problems, and the answers

### 2.1 Clearance vs. offline-first

The sale must remain possible through an outage; that does not decide when the customer-facing artifact may legally be called a tax invoice. The durable queue is therefore unconditional, while the artifact shown or printed before clearance is selected by the written ISTD outage ruling obtained in 2.7.0. A successful result is appended as `fiscal_result`; the completed sale is never updated. Receipt reconstruction reads that result when it exists.

Until the ruling exists, an offline sale produces a clearly marked **non-fiscal payment acknowledgement**, not a pending tax invoice. The queued fiscal document is issued only through the approved procedure after connectivity returns. `uncleared_count` and `oldest_uncleared_age` are first-class health metrics and must never silently grow.

> ⚠️ **OPEN — blocks 2.7.0.** Does ISTD permit asynchronous reporting during an outage, what artifact may be handed to the customer, when is the legal issuance event, what is the submission deadline, and how are backdating and later rejection handled? Default until answered: complete the sale, print only a non-fiscal payment acknowledgement, and issue the fiscal invoice only through the approved clearance path.
> Owner: 2.7.0. Source that settles it: the official ISTD outage procedure or a written ruling from the ISTD E-Invoicing Directorate.

### 2.2 There is no sandbox — correction **C-1**

No public ISTD sandbox contract is documented. Credentialed submissions are treated as live fiscal activity unless the official package or a written ISTD instruction obtained in 2.7.0 says otherwise. Vendor staging and a local mock are not ISTD environments.

**The answer is two local stand-ins**, built in Phase 2 (§6) from the pinned official package, plus a **certification milestone** in Phase 5 that performs the real hop with an informed merchant under a written procedure. The stand-ins prove deterministic local behaviour; they never prove acceptance by ISTD.

### 2.3 You cannot get the authoritative spec

This heading is retained for link compatibility; its premise is stale. ISTD publicly lists a Technical Integration Guide. The authoritative package must be obtained **before** fiscal implementation, not during Phase 5.

**Microstep 2.7.0 — Obtain and pin the official ISTD specification** is a precondition of every other 2.7.x step. It obtains the current guide, XSD, business rules, and code lists; records the retrieval date and package/version identifier; computes a SHA-256 digest for every source artifact; records whether each artifact may be vendored; and commits a manifest that makes the exact inputs reproducible. It then diffs every provisional row in §3 and closes or preserves each `OPEN` block in this reference.

No `codes.rs` table, builder, conformance rule derived from ISTD, or fiscal golden may be frozen before 2.7.0 passes. Phase 5 still owns credentialed live certification; it no longer owns discovery of the contract.

---

## 3 · The contract, as currently known

Every row labelled `PROVISIONAL` is a reconstruction and must be replaced or confirmed by the 2.7.0 manifest before implementation. A secondary implementation agreeing with another secondary implementation does not promote a row to authoritative.

| Item | Current build input | Status |
|---|---|---|
| Endpoint | `https://backend.jofotara.gov.jo/core/invoices/` | **PROVISIONAL — confirm in 2.7.0** |
| Method | `POST` | **PROVISIONAL — confirm in 2.7.0** |
| Authentication | headers currently reconstructed as `Client-Id`, `Secret-Key` | **PROVISIONAL — confirm names and scope in 2.7.0** |
| Envelope | JSON carrying base64-encoded UBL XML | **PROVISIONAL — confirm field names in 2.7.0** |
| Document standard | UBL 2.1 plus the ISTD profile/XSD pinned by 2.7.0 | official artifacts required before build |
| `cbc:ProfileID` | reconstructed candidate `reporting:1.0` | **PROVISIONAL — confirm in 2.7.0** |
| `cbc:ID` | immutable register-prefixed invoice number; never UUID or ICV | **PROVISIONAL mapping — confirm in 2.7.0** |
| `cbc:InvoiceTypeCode` value | reconstructed candidate `388` invoice · `381` credit note | **PROVISIONAL — confirm in 2.7.0** |
| `cbc:InvoiceTypeCode@name` | three-digit composite: scope + settlement + fiscal taxpayer type | component digits **PROVISIONAL — confirm in 2.7.0** |
| Fiscal taxpayer type | reconstructed candidates `income` · `general_sales` · `special_sales` | **PROVISIONAL — evidence and code list required** |
| ICV | allocated once at first submission from the default store scope | scope **OPEN** below; never allocated at checkout |
| Seller activity | income source sequence | **PROVISIONAL — confirm field and format in 2.7.0** |
| Buyer schemes | reconstructed candidates `TN` · `NIN` · `PN` | **PROVISIONAL — confirm matrix and tokens in 2.7.0** |
| Buyer rules | receivable documents require the pinned buyer fields at any value; cash documents follow the pinned threshold/identifier matrix | **PROVISIONAL — confirm exact fields and threshold in 2.7.0** |
| `cbc:IssueDate` | `YYYY-MM-DD` (`xs:date`) | fixed by D5; validate through the real XSD in `F-001` |
| Fiscal UUID | separate locally generated identity; reconstructed implementations use v4 | version **PROVISIONAL — confirm in 2.7.0** |
| Response | no response fields or duplicate status are assumed until pinned | **PROVISIONAL — blocks 2.7.5** |
| Discounts | exact line allowance amounts plus a document recap equal to their sum | fixed local contract; XML placement confirmed in 2.7.0 |
| Totals | exact identities over carried values plus a half-fil per-line preflight | fixed local contract; regulator tolerance **OPEN** below |

### Taxpayer type → invoice category

Three independent axes feed fiscal composition. They must never be collapsed into `store.tax_profile`:

| Axis | Stored evidence | What it controls |
|---|---|---|
| GST calculation profile | `store.tax_profile` (`standard`, `asez`, `development_area`, `unregistered`) | local tax rules and jurisdiction pack |
| JoFotara obligation | `store.fiscal_profile` plus dated merchant evidence | whether a sale queues a fiscal document |
| Fiscal taxpayer type | evidence-backed `income`, `general_sales`, or `special_sales` classification | the taxpayer component of `InvoiceTypeCode@name` |

An `unregistered` GST profile does **not** imply `fiscal_profile = 'disabled'`. Enabling or disabling fiscal issuance requires merchant-specific ISTD obligation or exemption evidence. Zone profiles fail closed unless their complete jurisdiction and fiscal component mapping is present; they never fall through to the standard local combination.

`codes.rs` owns three component tables and one composition function:

| Component | Rust values that must be covered | Digit source |
|---|---|---|
| Scope | local and every supported zone/supply scope required by `standard`, `asez`, `development_area`, free-zone, and export contexts | pinned official table from 2.7.0 |
| Settlement | cash · receivable | pinned official table from 2.7.0 |
| Fiscal taxpayer type | income · general sales · special sales | pinned official table from 2.7.0 |

```rust
// crates/pos-fiscal/src/codes.rs                              [2.7.1]
pub fn compose_invoice_type_name(
    scope: InvoiceScope,
    settlement: SettlementMethod,
    taxpayer: FiscalTaxpayerType,
) -> Result<InvoiceTypeName, FiscalCodeError>; // exactly three ASCII digits
```

The function concatenates one pinned digit from each component table. It returns `UnsupportedInvoiceTypeCombination` for any absent combination; it never substitutes the standard profile. Tests `code_tables_match_the_pinned_manifest`, `compose_invoice_type_name_covers_every_supported_store_profile`, and `an_unsupported_combination_is_refused_not_approximated` exhaust the supported store profiles, including GST-unregistered profiles, both settlement methods, and every fiscal taxpayer type.

> ⚠️ **OPEN — blocks 2.7.1.** What exact digits and allowed combinations define the scope, settlement, and fiscal-taxpayer components of `InvoiceTypeCode@name`, including ASEZ, development-area, free-zone, and export cases? Default until answered: no reconstructed digit is frozen, every zone combination is unsupported, and the builder refuses rather than falling back to a standard profile.
> Owner: 2.7.0. Source that settles it: the official ISTD Technical Integration Guide and code lists pinned by version and SHA-256.

> ⚠️ **OPEN — blocks 2.7.0.** Is the authoritative ICV namespace per register, store/income source, or one TIN across stores? Default until answered: allocate from one store-scoped counter keyed as `('store', store_id, 'fiscal_icv')`; Phase 2 uses the single register's in-process allocator, Phase 3 uses a server-issued one-value lease, and no register advances an independent register-scoped ICV counter.
> Owner: 2.7.0. Source that settles it: the official ISTD business rules or a written ISTD E-Invoicing Directorate ruling.

The fiscal UUID is always separate from the sale's UUIDv7 identity, generated locally when the sale queues, and never regenerated. Its version remains a 2.7.0 code-table input rather than an assumption hidden in the builder.

---

## 4 · The builder — turning a finalized sale into a document

### 4.1 The cardinal rule

**The fiscal document is built from the same `pos-domain` math as the receipt, never repriced.** Totals reconcile over the document's carried values; high-precision arithmetic is a per-line preflight, not a second invoice calculator. Two invoice-level calculations that round at different points create false drift on ordinary multi-line baskets.

Concretely: the builder consumes the **persisted** `sale`, `sale_line`, `sale_line_tax`, `sale_line_discount`, and `sale_tax_summary` rows. It reads no product, no price list, no live tax rule. Those rows are the receipt's numbers, so the document carries the receipt's numbers.

### 4.2 Build order

```rust
// crates/pos-fiscal/src/builder.rs                              [2.7.2]
pub fn build_document(
    sale: &PersistedSale,           // sale + lines + taxes + discounts + tenders
    store: &StoreFiscalConfig,      // seller evidence + explicit fiscal classification
    identity: FiscalSubmissionIdentity,
) -> Result<FiscalDocument, FiscalBuildError>;

pub struct FiscalSubmissionIdentity {
    pub fiscal_uuid: Uuid,          // generated once when the sale queues
    pub icv: u64,                   // allocated once at first submission, never at checkout
}

pub struct FiscalDocument {
    pub xml: String,
    pub xml_base64: String,
    pub envelope: SubmitEnvelope,   // the JSON body
    pub fiscal_uuid: Uuid,
    pub icv: u64,
    pub xml_sha256: String,         // persisted request identity + reconciliation
}
```

Steps, in order — each one is a checkable stage, and each is individually unit-tested:

1. **Eligibility.** `sale.is_training` → state `skipped`, never submitted. A dated `store.fiscal_profile = 'disabled'` decision → no queue row. GST registration alone never disables fiscal issuance.
2. **Header.** Emit the pinned `cbc:ProfileID`, immutable receipt/invoice number as `cbc:ID`, pinned invoice/credit-note code, composed `InvoiceTypeCode@name`, ICV, fiscal UUID, and `cbc:IssueDate` as `YYYY-MM-DD`.
3. **Seller.** Legal name, TIN, income source sequence, and every other field required by the pinned seller matrix.
4. **Buyer.** Use the pinned `TN`/`NIN`/`PN` scheme and field matrix. Receivable documents carry the required buyer identity at any value; cash documents follow the pinned threshold and captured-identifier rules. Never derive a buyer from today's customer row during a reprint or credit note.
5. **Lines.** One per `sale_line`: item name (Arabic), quantity, unit price, unit-of-measure code, tax category code, tax amount — all read from the persisted rows.
6. **Discounts** — §4.3.
7. **Totals** — tax-exclusive, tax-inclusive, total discount, total tax, payable.
8. **Credit-note lineage** — §4.5 when applicable.
9. **Self-check** — §4.4. Failure transitions the queue row to `BuildFailed`; it is never reported as an ISTD rejection.
10. **Serialize and validate.** XML → real XSD validation (`F-001`) → base64 → the pinned JSON envelope. Persist the exact XML, envelope bytes, and SHA-256 before network I/O.

### 4.3 Discounts — correction **C-2**

The fiscal truth is the **allowance amount**, not a percentage round-trip. A basket discount is allocated to exact line amounts once; the XML carries each line allowance and a document-level recap equal to their sum:

```
basket discount, in fils
  ├─ 1. prorate to lines by line value       Money::split_proportional
  │       largest-remainder → Σ line allowance amounts == basket discount, EXACTLY
  ├─ 2. emit each persisted line allowance amount
  ├─ 3. emit one document recap amount
  └─ 4. assert recap == Σ emitted line allowance amounts, EXACTLY
```

An entered percentage may be stored as provenance. It never decides fiscal eligibility, never changes the allocated fils, and is never re-derived to prove an amount. If the pinned official package requires a percentage element, `codes.rs` declares the single constant `DISCOUNT_PERCENT_DECIMALS` and the builder formats provenance to that precision; no other module chooses precision.

The domain applies each line allowance to its taxable base exactly once, before tax. The builder reads that carried base and emits the persisted allowance; it never subtracts the allowance again. Applying it twice would understate both the customer's tax and the merchant's output-tax liability.

Largest-remainder ties use an immutable canonical line key (`tax_component_signature`, `product_id`, `unit_price_minor`, `qty_milli`, `line_gross_minor`), never scan or vector position. Lines with the same key are an indistinguishable multiset. This prevents a reordered multi-rate basket from moving the remainder fil into a different tax treatment and changing the customer's tax.

Tests `prop_document_allowance_recap_equals_sum_of_line_allowances`, `prop_basket_discount_prorates_to_the_fil`, `prop_price_cart_is_invariant_under_line_reordering`, and `a_twenty_line_basket_is_submitted_not_dead_lettered` pin the contract. The recap property also proves every line allowance reduces its taxable base exactly once. The twenty-line fixture includes a high-value line whose allowance cannot round-trip through a percentage and remains valid because the amount is authoritative.

> ⚠️ **OPEN — blocks 2.7.0.** How many decimal places may the current JoFotara discount percentage carry when the pinned profile requires one? Default until answered: exact line allowance amounts and their exact document recap are authoritative; an entered percentage is provenance only, `DISCOUNT_PERCENT_DECIMALS` is the single emission constant, and percentage round-trip never gates fiscal eligibility.
> Owner: 2.7.0. Source that settles it: the pinned official ISTD Technical Integration Guide, XSD, business rules and accepted boundary vectors.

### 4.4 The totals self-check — correction **C-3**

No authoritative ISTD totals tolerance has been pinned. The local preflight must therefore reject only an internally inconsistent document, never an arithmetically correct invoice whose per-line rounding accumulated normally.

```rust
// crates/pos-fiscal/src/totals.rs                               [2.7.3]
pub struct FiscalTotalsPolicy {
    pub per_line_tolerance_minor: Decimal, // exactly Decimal::new(5, 1): half a fil
}

pub fn check(doc: &FiscalDocument, sale: &PersistedSale, p: &FiscalTotalsPolicy)
    -> Result<(), FiscalTotalsFailure>;
```

For each line, recompute the high-precision value from the immutable carried inputs and require the carried line value to be within **half a fil**, inclusive. Then assert exact integer identities over the document's own carried values:

- Σ line allowance amounts = document allowance recap;
- Σ line nets = tax-exclusive total;
- Σ line taxes = tax total;
- tax-exclusive total + tax total = tax-inclusive total;
- the payable identity balances using only carried document fields.

Never compare an unrounded invoice-level recomputation against the carried invoice total. Three or more independently rounded lines can exceed a one-fil invoice delta while every line and every carried identity is correct.

**The receipt never moves.** A failure transitions to `QueueState::BuildFailed`, records the offending line or identity without PII, and appears as `Local, build failed` in reconciliation. The audited operator command `fiscal_rebuild_failed` is available only after a mapping, configuration, or builder correction; it requires `fiscal.remediate` and a one-use approval bound to the queue row and reason, rebuilds from the immutable sale, preserves `fiscal_uuid` and any allocated ICV, and requeues only after all checks pass.

> ⚠️ **OPEN — blocks 2.7.0.** What tolerance, if any, does the current ISTD validator apply to transmitted line and document equations? Default until answered: enforce the half-fil per-line projection check and exact identities over the document's own carried values; do not implement an invoice-level tolerance or claim an ISTD tolerance.
> Owner: 2.7.0. Source that settles it: the pinned official ISTD business rules and Schematron/XSD package, plus credentialed accepted boundary vectors.

### 4.5 Credit notes

A refund is its own credit-note document using the pinned code. It references the original fiscal UUID, immutable `cbc:ID`, original amount, and return reason, and copies the original buyer block and original line facts from the immutable sale: line identity, name, unit price, tax components and rates, discounts, sold quantity, previously credited quantity, and `remaining_qty_milli`. Current catalog and customer records are never consulted.

Partial and repeated credits must not exceed the original quantity less prior credit notes. Tests and byte-stable cases cover `partial_credit_note_copies_original_facts`, `repeated_credit_note_respects_remaining_qty_milli`, `credit_note_survives_catalog_change`, and `credit_note_survives_customer_change`.

**Dependency ordering (E.26):** a credit note for a not-yet-cleared invoice must wait. `fiscal_queue.depends_on` points at the parent row, and the drain loop skips any row whose dependency is not `cleared`. Property `prop_credit_note_never_precedes_its_invoice`.

A credit note also preserves the original invoice filing period and its own issue/filing period. A later-period credit never silently rewrites the original period; the filing workpaper reports both period identifiers and an explicit disposition state that stays unresolved until 4.7.2 closes.

> ⚠️ **OPEN — blocks 4.7.2.** Which return period and box must receive a credit note issued after the original invoice's filed period for each supported return type and jurisdiction? Default until answered: show the credit as a negative in sales reconciliation on the credit-note date, preserve the original and credit periods, and leave statutory `box_disposition` unresolved rather than auto-populating a return.
> Owner: `4.7.2`. Source that settles it: the current official ISTD credit-note return instructions for General Tax, Special Tax, and each enabled zone profile or a written ISTD ruling; the merchant's accountant confirms how that authority applies to the merchant.

---

## 5 · The queue

```rust
// crates/pos-fiscal/src/queue.rs                                [2.7.4]
pub enum QueueState {
    Queued,
    Sending,
    BuildFailed,
    Cleared,
    Rejected,
    Dead,
    Skipped,
}

pub struct RetryPolicy {
    pub base: Duration,
    pub factor: f64,
    pub max_backoff: Duration,
    pub attempts_before_alarm: u32,
    pub jitter: f64,             // transport timing only; never money
}
```

`fiscal_queue.fiscal_uuid` is generated locally in the sale transaction. `fiscal_queue.icv` is nullable and is **not** allocated in that transaction. A register that cannot reach the applicable allocator commits the sale and queue row with `icv IS NULL`; selling never waits for fiscal allocation.

The default counter is `doc_sequence PRIMARY KEY (scope_kind, scope_id, kind)` with `scope_kind TEXT CHECK (scope_kind IN ('register','store'))`. Receipts and Z reports use `scope_kind = 'register'`; fiscal ICV uses `scope_kind = 'store'`. One allocator serializes the selected fiscal scope.

**Phase 2 — local allocator, owned by 2.7.4.** There is no server and the phase supports one register
per store. The submission worker locks that register database's store-scoped `doc_sequence` row in
process, allocates after preflight, and writes the allocating register id to `allocator_ref` in the
same transaction as `fiscal_payload_event`. This is a real allocator, not a mock service; it is safe
for the single-register Phase-2 topology and does not claim multi-register coordination.

**Phase 3 onward — server allocator and lease, owned by 3.1.7.** The server owns the store-scoped
row. At first submission the register requests a one-value lease bound to `fiscal_uuid` through the
endpoint in [`sync-protocol.md`](sync-protocol.md) §3. The server returns the same lease on replay,
and its `lease_id` becomes `allocator_ref`. A register with no lease leaves `icv IS NULL`, raises the
age/depth health signal, and continues selling.

**Drain loop** — a background task, never in the checkout path:

1. Reclaim every expired `Sending` lease. A claim carries `claimed_at`, `lease_owner`, and `lease_expires_at`; startup and every drain cycle make an expired claim eligible again.
2. Select eligible `Queued` rows whose dependency is cleared and run every identity-independent builder and totals preflight against the immutable sale. A failure becomes `BuildFailed` with `icv IS NULL`; it consumes no counter value and does not call ISTD.
3. After preflight passes, require the issue-date source permitted by the open item in API reference §3.2. `Trusted`/`Stale` documents already carry their sale-time date; a `Suspect`/`Untrusted` row remains queued until a new authenticated time anchor exists. Reaching the clearance endpoint is transport success, not time authentication. Only then obtain the phase-appropriate allocation: Phase 2 locks and increments the local store row; Phase 3 obtains the idempotent server lease. One local transaction records the ICV and `allocator_ref`, stamps any deferred `issue_date` from that anchor, finalizes the document with the existing `fiscal_uuid`, validates it against the pinned XSD, and freezes the exact XML, envelope bytes, and SHA-256. Any construction or schema failure rolls back the Phase-2 counter with the payload; in Phase 3 an already-issued lease remains bound to the UUID and is reused when the build is retried, so no second ICV is allocated.
4. Among rows with non-null ICV for the same confirmed scope, atomically claim the lowest ICV whose retry time has arrived. Later allocated ICVs wait behind any earlier non-terminal row.
5. Submit the already persisted envelope. A successful pinned-contract response appends `fiscal_result`, then changes the queue row to `Cleared`; the completed sale remains untouched.
6. A confirmed ISTD validation rejection becomes `Rejected`, retains the regulator error as protected fiscal evidence, and raises a sanitized alert. An amount correction is a credit note plus a new invoice, never a sale update.
7. A network error or timeout releases or expires the lease, increments the attempt count, applies backoff, and returns to `Queued` with the same bytes, UUID, and ICV. Crossing `attempts_before_alarm` raises the alarm but remains durably retryable. `Dead` is used only for a retry-exhausted error class explicitly permitted by the pinned official procedure; an ordinary transport outage never becomes terminal by default.
8. An ambiguous remote outcome follows the duplicate/reconciliation procedure pinned by 2.7.0. No status code, response field, or fetch endpoint is invented locally.

Rows with `icv IS NULL` are awaiting allocation and do not participate in ICV ordering. Once a row receives an ICV, it never changes, and its position is fixed. Phase 2 fixture `single_register_local_allocator_assigns_store_scoped_icv_at_first_submission` proves local allocation, allocating-register evidence and replay identity. Phase 3 fixture `two_registers_offline_then_reconnect_allocate_distinct_icvs` queues sales on two disconnected registers, reconnects both, and proves server-leased unique monotonic allocation, identical replay identity, and no lost sale.

Crash coverage is explicit: `build_failure_does_not_consume_icv`, `crash_before_claim_leaves_row_queued`, `crash_after_claim_reclaims_expired_lease`, `crash_after_remote_commit_preserves_submission_identity`, and `crash_before_result_persist_reconciles_without_new_uuid_or_icv`. These inject failure before identity freeze, before claim, after claim, after the mock commits, and before the local result transaction.

**Idempotency.** `fiscal_uuid` exists from sale time; ICV and request bytes are added once at first submission. A retry never creates a new UUID, ICV, XML document, or envelope. The external duplicate outcome remains an official-contract question, not a mock-server invention.

> ⚠️ **OPEN — blocks 2.7.5.** What exact operation recovers an ambiguous timeout or duplicate fiscal UUID: idempotent resubmission, a documented lookup, portal reconciliation, or a manual procedure? Default until answered: persist the exact request bytes and identity, keep the row recoverable, assume neither HTTP `409` nor a `fetch_existing` endpoint, and do not mark the document cleared without authoritative response evidence.
> Owner: 2.7.0. Source that settles it: the authenticated current ISTD API specification or a controlled credentialed certification case.

> ⚠️ **OPEN — blocks 2.7.0.** Are JoFotara credentials scoped to a taxpayer, income source, store, or register, and what rotation and revocation operations does ISTD support? Default until answered: do not copy one taxpayer secret to every register and do not enable the live client; keep only versioned credential references in the register credential store and choose per-register credentials or server-side KMS custody after the scope is confirmed.
> Owner: 2.7.0. Source that settles it: authenticated JoFotara portal/API documentation or a written ISTD E-Invoicing Directorate answer.

Whichever credential topology 2.7.0 selects must define authenticated provisioning, versioned key references, least-privilege storage, rotation and revocation, queued-document cutover without changing fiscal identity, compromise alerts, and a live rotation drill. Credential values never enter the database, logs, diagnostics, or fixtures.

---

## 6 · Testing without a sandbox

### 6.1 The conformance harness — `crates/pos-fiscal/src/conformance.rs`  [2.7.6]

Every rule known about ISTD validation is encoded as an assertion over an explicit conformance case. Microstep 2.7.6 owns the CI wiring that will run the pinned fixture matrix on every commit; no current workflow is credited with a fiscal conformance lane.

```rust
pub struct ConformanceContext<'a> {
    pub manifest: &'a PinnedSpecManifest,
    pub store: &'a StoreFiscalConfig,
    pub scope_history: &'a [FiscalSubmissionIdentity],
    pub original_document: Option<&'a PersistedFiscalDocument>,
}

pub struct ConformanceCase<'a> {
    pub sale: &'a PersistedSale,
    pub document: Option<&'a FiscalDocument>, // `None` is required for F-021
    pub context: ConformanceContext<'a>,
}

pub struct Rule { pub id: &'static str, pub description: &'static str,
                  pub check: fn(&ConformanceCase<'_>) -> RuleOutcome }

pub fn run_all(case: &ConformanceCase<'_>) -> ConformanceReport;
```

`RuleOutcome` is `Pass`, `Fail`, or `NotApplicable`. Applicability is explicit and the fixture matrix must exercise every rule at least once; `NotApplicable` can never be counted as a pass. The manifest supplies XSD and code lists to `F-001`, `F-003`, `F-017`, and `F-018`; scope history supplies the cross-document evidence for `F-005`; the original fiscal document supplies the immutable credit context for `F-020`; and the no-document training case makes `F-021` testable.

Rules at minimum:

| id | Rule |
|---|---|
| `F-001` | A real XML Schema validator accepts the serialized XML against the exact UBL/ISTD XSD set pinned by 2.7.0; a string or date-pattern check is not validation |
| `F-002` | The `InvoiceTypeCode` value is an allowed invoice or credit-note code from the pinned table |
| `F-003` | `InvoiceTypeCode@name` is exactly the three approved scope + settlement + fiscal-taxpayer digits for this store/supply context; unsupported combinations fail |
| `F-004` | The settlement component matches the persisted tender/receivable facts; it is not a hard-coded `012`/`022` payment-code lookup |
| `F-005` | ICV is present and positive at submission, unique and monotonic in the confirmed scope, and unchanged on every replay |
| `F-006` | Income source sequence present and non-empty |
| `F-007` | Seller TIN present and well-formed |
| `F-008` | `cbc:IssueDate` is `YYYY-MM-DD` and is accepted as `xs:date` by `F-001` |
| `F-009` | Fiscal UUID is separate from the sale UUID, generated once, and matches the version pinned by 2.7.0 |
| `F-010` | **No line amount is negative** (E.19) |
| `F-011` | Every line discount reduces its carried taxable base exactly once, carries its exact allowance amount, and contributes once to a document recap equal to Σ line allowance amounts (C-2) |
| `F-012` | Discount percentage is optional provenance unless the pinned profile requires it; when emitted it uses `DISCOUNT_PERCENT_DECIMALS` and never gates the amount (C-2) |
| `F-013` | Σ line nets == document tax-exclusive total, exactly |
| `F-014` | Σ line taxes == document tax total, exactly |
| `F-015` | Tax-exclusive + tax total == tax-inclusive total, exactly |
| `F-016` | Every carried line is within half a fil of its high-precision value; no unrounded invoice-level total is compared with a tolerance (C-3) |
| `F-017` | Every tax category code is in the ISTD code list |
| `F-018` | Every unit-of-measure code is in the ISTD code list |
| `F-019` | Buyer block, name, scheme and value match the pinned cash/receivable matrix; receivable documents are checked at every value |
| `F-020` | Credit note references the original UUID/`cbc:ID`/amount, copies the original buyer and line facts, and cannot exceed `remaining_qty_milli` |
| `F-021` | A training-mode conformance case has `document IS NULL`; constructing any fiscal document fails the rule |
| `F-022` | Every string is valid UTF-8 and Arabic text is not mangled (E.41) |

Every report records the 2.7.0 manifest version and hashes that supplied its XSD and code tables. `F-001` loads those actual schema files and fails if an import is missing or a different hash is supplied. If the official package needed by any rule is unavailable, group 2.7 remains blocked; a reconstructed fallback is not reported as a pass.

### 6.2 The mock ISTD server — `crates/pos-fiscal/tests/mock_istd.rs`  [2.7.7]

An HTTP server implementing the contract pinned in 2.7.0, with fault injection driven by a header the test sets. It does not invent a duplicate endpoint, status code, or response field:

| Fault | Asserted behaviour |
|---|---|
| happy path | `cleared`; QR persisted; health counter decremented |
| slow (30 s) before remote commit | request times out; lease expires; row stays durably retryable with identical bytes |
| ambiguous timeout after remote commit | local identity and bytes remain fixed; recovery follows the pinned duplicate procedure |
| connection refused | backoff applied with jitter; sale untouched |
| pinned validation-error response | `rejected`; protected evidence retained; sanitized alert; sale untouched |
| malformed JSON response | `queued` with a parse error recorded, not a panic |
| expired `Sending` lease | reclaimed on restart and on the next drain cycle; same UUID, ICV and bytes |
| wrong seller identity in the pinned response | **alarm** (E.28); no result is attached to the local sale |
| pinned transient-server response, then success | second attempt sends the identical bytes; exactly one `fiscal_result` |

The response status values above are bound to the pinned contract at 2.7.0; the mock fixture cannot promote a reconstructed status to an official API guarantee.

### 6.3 The golden set — `crates/pos-fiscal/tests/golden/`  [2.7.8]

Five fixture sales, each producing a byte-stable XML golden file reviewed on every change. The files cannot be frozen until 2.7.0 pins the XSD and code tables:

1. **Plain cash sale** — 3 lines, one tax rate, no discount.
2. **Discounted sale** — a basket discount prorated to 3 exact line allowance amounts plus their equal document recap.
3. **Multi-rate sale** — standard 16% + a zero-rated item + an exempt item on one receipt.
4. **Weighed sale** — a price-embedded barcode line with a fractional quantity.
5. **Credit note** — a partial refund of fixture 2, carrying the immutable original buyer and line facts plus remaining quantity.

Companion conformance cases cover a repeated partial credit, changed catalog data, changed customer data, and a training sale whose expected document is absent. These five goldens replace the master plan's "four sandbox documents" gate. The matrix runs on every commit, but it remains local evidence rather than ISTD acceptance evidence.

### 6.4 What Phase 2 exit actually means

> **Across the five golden documents and the companion training absence case, every applicable check passes against the pinned manifest and all 22 rules are exercised; the queue survives every mock fault without losing identity or a document; and each artifact reprints byte-identically after restart.**

Explicitly *not* claimed: that ISTD accepts them. That claim requires §7 and nothing else can produce it.

---

## 7 · Fiscal Certification — Phase 5, milestone 5.2

The only place the real endpoint is contacted. Contract acquisition and reconstruction diffing formerly numbered 5.2.1 and 5.2.2 now belong to 2.7.0 and must already be complete. This milestone validates the pinned implementation against the credentialed service with an informed merchant; it does not discover the contract for the first time.

| Step | Action | Gate |
|---|---|---|
| 2.7.0 prerequisite | Re-read the pinned manifest, closed `OPEN` evidence, and all five reviewed goldens | Artifact hashes match; no provisional table drives the build |
| 5.2.3 | Provision production credentials using the topology selected in 2.7.0; store only the versioned reference in the approved credential store or KMS | secret value never enters database, log, diagnostics, or fixture |
| 5.2.4 | Confirm live UUID, response, and duplicate-recovery behaviour against the pinned contract without changing an already-issued identity | observed behaviour matches the manifest or certification stops and the erratum is recorded |
| 5.2.5 | Execute the written outage procedure selected in §2.1, including the exact customer artifact and reconciliation path | every step produces dated evidence; no policy is invented during the drill |
| 5.2.6 | Submit golden document 1 as a **live, low-value invoice**; verify the returned QR with the Sanad app | QR verifies |
| 5.2.7 | Immediately credit-note it through golden 5's path, including original buyer/line facts. Verify both appear in the merchant's ISTD portal | both visible, linked, and net to zero |
| 5.2.8 | Repeat for goldens 2, 3, 4 | All clear |
| 5.2.9 | Run the reconciliation report: local sales ↔ cleared invoices | Zero unmatched on both sides |
| 5.2.10 | Two-register kill-the-network drill: queue on both registers, reconnect, allocate ICV centrally, and drain in assigned order | unique scope-correct ICVs; no new UUID/ICV on replay; selling never stopped |
| 5.2.11 | Environment guard: confirm the app refuses to start with mock credentials in a production build and vice versa (E.28) | Both directions refuse |

**Do not attempt any of this without the merchant's informed consent in writing.** Every submission is a real fiscal document against their real tax record, and step 5.2.6 puts a live invoice on it.

---

## 8 · Reconciliation and health

**Health metrics**, on the register status strip and in back-office device health:

- `uncleared_count` — `state IN ('queued','sending')`. Non-zero is normal; growing is not.
- `unallocated_icv_count` — queued rows with `icv IS NULL`; a growing value means the allocator is unreachable, not that checkout failed.
- `oldest_uncleared_age` — the number that actually matters. An alarm threshold is a merchant decision, defaulting to 4 hours.
- `build_failed_count` — local construction/preflight failures. Any non-zero value requires the catalogued `fiscal_rebuild_failed` command after a code or configuration correction.
- `expired_sending_lease_count` — should return to zero after the next drain cycle; a persistent value means reclaim is broken.
- `dead_letter_count` — confirmed `Rejected`/`Dead` rows only. It never includes `BuildFailed`.
- `rejection_rate_24h` — operational alarm default: above 2%, or any three consecutive rejections with the same pinned ISTD error code. This is a change detector, not a statutory threshold.

The dashboard is not the terminal control. Microstep `3.9.3` delivers threshold breaches to the merchant and vendor recipients configured for the deployment, records delivery and acknowledgement, and escalates an unacknowledged fiscal alarm because a queue can grow while checkout still appears healthy.

**Reconciliation report** (`fiscal_reconciliation`, microstep 3.6.4), for a date range:

| Row class | Meaning |
|---|---|
| Matched | local sale ↔ `fiscal_result` ↔ ISTD portal record |
| Local, awaiting ICV | sale is complete; queue row has `icv IS NULL` and awaits the confirmed allocator |
| Local, uncleared | row is `Queued` or has an active `Sending` lease; identity and retry age are shown |
| Local, build failed | local builder or preflight failed; ISTD did not reject it; remediation is the `fiscal_rebuild_failed` operator command |
| Local, rejected | ISTD rejected the submitted document; sanitized error and protected raw evidence are linked |
| Local, ambiguous | remote outcome is unresolved under the pinned duplicate procedure; never silently treated as cleared |
| Later-period credit | original invoice period, credit-note period, and pinned filing disposition are all shown |
| Cleared, no local sale | **alarm** — a document exists at ISTD that this system did not produce |
| Training excluded | count only, proving they were correctly skipped |

The `Cleared, no local sale` row is why the report exists. It is the only way to notice a document that succeeded under an identity the local database cannot account for.

---

## 9 · Crate layout

```
crates/pos-fiscal/
├── src/
│   ├── lib.rs            FiscalProfile, the enable/disable switch
│   ├── model.rs          FiscalDocument, SubmitEnvelope, ClearanceResult
│   ├── builder.rs        PersistedSale → UBL 2.1 XML          [2.7.2]
│   ├── totals.rs         half-fil lines + carried identities  [2.7.3]
│   ├── codes.rs          pinned component/code tables         [2.7.1]
│   ├── queue.rs          allocation, leases, dependencies     [2.7.4]
│   ├── client.rs         HTTP client, auth, idempotency       [2.7.5]
│   ├── conformance.rs    the 22 rules                         [2.7.6]
│   └── qr.rs             QR payload → raster for the receipt  [2.7.9]
├── spec/
│   └── manifest.json     package/version, retrieval date, SHA-256s [2.7.0]
└── tests/
    ├── mock_istd.rs      fault-injecting server               [2.7.7]
    ├── golden/           five documents, byte-stable          [2.7.8]
    └── queue_chaos.rs    crash/duplicate/reorder scenarios    [2.7.10]
```

`codes.rs` is deliberately a separate module of plain tables. It is generated or transcribed only from the 2.7.0 manifest, carries `DISCOUNT_PERCENT_DECIMALS` when needed, and is exhaustive over supported profile outcomes. An official code change lands there, its manifest, and the affected goldens rather than leaking conditionals through the builder.

**`pos-fiscal` depends on `pos-domain` and nothing that can surprise it.** The builder takes persisted rows as plain structs; the client is the only module touching the network, and it is behind a trait so the mock and the real endpoint are interchangeable:

```rust
pub trait ClearanceClient: Send + Sync {
    fn submit(&self, env: &SubmitEnvelope) -> Result<ClearanceResult, ClearanceError>;
}
```

There is no `fetch_existing` method until an official operation requires one. Duplicate recovery is a queue workflow selected from the pinned contract, not a speculative method on the transport trait.

---

## 10 · Edge cases owned by this component

| # | Case | Behaviour |
|---|---|---|
| E.24 | API or ICV allocator down at sale time | sale completes; queue keeps `icv IS NULL` if necessary; the approved non-fiscal acknowledgement prints; health counters rise |
| E.25 | Local build failure or ISTD validation rejection | `BuildFailed` uses `fiscal_rebuild_failed`; `Rejected` follows regulator remediation; neither mutates the sale or is confused with the other |
| E.26 | Refund of a not-yet-cleared sale | credit note held via `depends_on` until the invoice clears |
| E.27 | Duplicate submission after ambiguous timeout | same UUID, ICV and request bytes; recover exactly as the pinned official procedure says; no assumed `409` or fetch endpoint |
| E.28 | Mock credentials in production, or vice versa | environment banner + hard config check at startup; mismatched TIN in a response is an alarm |
| E.29 | Merchant not obliged or exempt | `fiscal_profile = 'disabled'` only from dated obligation evidence; GST `unregistered` alone does not disable JoFotara; enabling later backfills nothing |
| E.46 | Paper out mid-receipt | QR payload is persisted, so a reprint carries the identical QR |
| E.47 | Reprint days later, another register | a connected register needs a complete authorized `reprint_bundle`; QR sync alone is insufficient, and no facts-down path is assumed here |

---

## 11 · Sources

- [ISTD — guidance for the National E-Invoicing System](https://istd.gov.jo/AR/List/%D8%A7%D9%84%D8%A7%D8%AF%D9%84%D8%A9_%D8%A7%D9%84%D8%A7%D8%B1%D8%B4%D8%A7%D8%AF%D9%8A%D8%A9_%D9%84%D9%86%D8%B8%D8%A7%D9%85_%D8%A7%D9%84%D9%81%D9%88%D8%AA%D8%B1%D8%A9_%D8%A7%D9%84%D9%88%D8%B7%D9%86%D9%8A) — authoritative listing from which 2.7.0 obtains the current Technical Integration Guide/package
- [ISTD — procedure manual for linking to the National E-Invoicing System](https://istd.gov.jo/ebv4.0/root_storage/en/eb_list_page/procedure_manual_for_linking_to_the_jordanian_national_electronic_invoicing_system.pdf) — official transport material; it does not by itself settle outage or duplicate recovery
- [OASIS UBL 2.1 schemas](https://docs.oasis-open.org/ubl/os-UBL-2.1/xsd/) and [W3C XML Schema `date`](https://www.w3.org/TR/xmlschema11-2/#date) — `F-001` schema basis and the `YYYY-MM-DD` lexical form
- [Odoo 19.0 Jordan localization](https://github.com/odoo/odoo/tree/19.0/addons/l10n_jo_edi) — secondary implementation evidence for comparison only; never authority for ISTD tolerance or wire rules
- [`jafar-albadarneh/jofotara` PHP SDK](https://packagist.org/packages/jafar-albadarneh/jofotara), [Mozon](https://mozon-tech.com/en/blog/the-ultimate-guide-to-jofotara/), and [`sedhha/automation-script-jordan-tax-dept`](https://github.com/sedhha/automation-script-jordan-tax-dept) — reconstruction inputs whose values remain provisional until 2.7.0

*Every §3 row marked `PROVISIONAL` is blocked from implementation until 2.7.0 confirms or replaces it. A green local harness means the pinned package was implemented consistently; only the Phase-5 credentialed milestone can establish live ISTD acceptance.*
