# Security and compliance

Blueprint §7, master plan B.3 / B.4, plus the gaps this plan closes: the audit-chain specification (G-7), the permission-enforcement mechanism (G-6), and proven PII scrubbing (G-8).

> ⚠️ These are engineering positions, not legal advice. Before launch, validate PDPL with a lawyer, GST and record retention with the merchant's accountant, and PCI scope with a QSA. **Never claim a validation you have not completed.**

Several claims in an earlier revision of this document were stronger than the mechanism behind them. The mechanisms were kept and the claims corrected: a mechanism worth having is worth describing accurately, and an overstated control is worse than a missing one because nobody goes looking for it.

---

## 1 · The posture in one page

| Control | Position |
|---|---|
| Local database | SQLCipher; key in the OS credential store via `keyring`. The only copy that leaves the credential store is **wrapped under the merchant's recovery code** (§6a) — never a bare file, and `POS_DB_KEY` is ignored in release builds |
| Backups | SQLCipher-encrypted, with the wrapped key envelope stored beside every backup, on **two** destinations, verified, and the age of each reported to device health |
| Cashier auth | PINs hashed with **Argon2id** at named parameter floors; persisted attempt limiting and lockout; auto-lock on idle; manager escalation for voids, refunds over threshold, overrides, drawer opens |
| Authorisation | **enforced in Rust command handlers** — a compile-time capability proof (`Authorized<C>`) plus a one-use runtime `ApprovalHandle` bound to the exact operation. Hiding a button is UX; the check is security |
| Audit | **hash-chained**, append-only, covering logins, voids, refunds, overrides, drawer events, settings changes, sync anomalies. Anchored outside the database at every Z close and verified backup, and on the server from Phase 3 — without an anchor a chain detects modification and reordering, not tail deletion (§4) |
| Card data | This application never requests, parses into a stored field, or persists a PAN. Only `psp_ref`, masked PAN, and scheme are stored. **A driver that returns a full PAN is an integration rejection**, not data to discard |
| Transport | TLS everywhere, certificate validation on; pinning considered for the sync API |
| Server identity | Device tokens bound to a non-exportable device key for registers; OIDC with phishing-resistant MFA for humans; every HTTP route deny-by-default against a capability (§5a) |
| Tenancy | one shared multi-tenant service, `org_id` on every merchant-owned table, forced row-level security — recorded as a decision for sign-off (§5b) |
| Updates | signed — Windows Authenticode, macOS notarization, Tauri signed manifests. An unsigned or tampered update must not install. Signing happens on a step that compiles **no** third-party code (§6b) |
| Personal data | PDPL-grade: minimal collection, consent as an append-only event ledger against an immutable notice, export, erasure as anonymisation, documented retention, breach clocks written down and drilled |
| Licensing | Ed25519-signed entitlements with a `kid`; expiry blocks enrollment and updates, **never a sale on an entitled register** (§7) |
| Secrets | no card data, PINs, tokens or keys in logs; Sentry events scrubbed; `.env` never committed; the sensitive-field list is generated from one source (§6) |
| Incident response | a named security contact, a disclosure path, severity-based acknowledgement targets, and a patch SLA (§8a) |

---

## 2 · PDPL — Personal Data Protection Law No. 24 of 2023

Jordan's first comprehensive data-protection law. Published 17 Sep 2023, in force 17 Mar 2024, grace period ended Mar 2025, and it applies **retroactively** to data collected before it existed. GDPR-like in structure.

An earlier revision of this document said the electronic controller/processor registry "is not yet activated" and named manual registration as the interim path. That was a snapshot of a moving institution, presented as a standing fact, and a launch gate cannot be built on it. **Build to the law, not to the enforcement lag** — and establish the filing position from the regulator rather than from this document.

> ⚠️ **OPEN — blocks 3.4.1.** For this deployment, which entity is controller, which is processor, who is a recipient, is a DPO required, and is the Personal Data Processing Register entry required and complete? Default until answered: the schema may migrate, but customer capture, consent collection and customer-PII sync remain disabled.
> Owner: 3.4.1. Source that settles it: the current MoDEE Personal Data Processing Register instructions and dated Jordanian counsel advice for the deployed roles.

### What it requires, and what this product does about it

| Requirement | Implementation | Where |
|---|---|---|
| Explicit informed consent | consent is an **append-only event**, not a boolean and not a mutable row: kind, the immutable notice it references, timestamp, who captured it, which channel | `consent_event`; microstep 3.4.2 |
| Purpose limitation | consent kinds are distinct — `loyalty_terms`, `marketing`, `data_processing` — and each feature checks its own | `prop`-tested per feature |
| Minimal collection | name, phone, email, consent events. **No ID numbers** unless a real requirement emerges | schema `0011` |
| Right of access | *export my data*: profile + consent events + purchase history + loyalty ledger, one file, complete against the inventory below | 3.4.5 |
| Right of correction | back-office edit with a full audit trail | 3.6 |
| Right of erasure | **anonymisation** — null the person, keep the immutable financial facts against the anonymised id; server-authoritative and monotonic, so a restore cannot resurrect the identity | 3.4.4 |
| Marketing consent honoured | any messaging feature and any back-office export filters on the **server's** effective consent state, never a register's | 3.4.2 |
| Objection, restriction, portability, complaint | a timed request case log, not an ad-hoc support conversation | 3.4.5, and see below |
| Pre-processing privacy notice | a versioned notice, retained verbatim, that every consent event references | 3.4.2 |
| Restricted cross-border transfer | a stated hosting jurisdiction, a recorded transfer basis, and named sub-processors. TLS is transport security and is **not** a transfer basis | 3.1.6, and see below |
| Breach notification | SQLCipher at rest + credential store + **no PII in logs**, all three tested; plus a runbook written against **both** statutory clocks and a containment procedure | 1.6.8, 5.3.2, §8a |
| Documented retention | retention periods in settings, enforced by a job that **can never delete a financial fact** | 5.3.4, §8 |

### Controller, processor, and the agreements that assign them

Nothing in the plan said who is the controller and who is the processor for any data flow, so both parties could reasonably assume the other carries the duty. The table below is the interim deployment model used to draft controls; it is not a legal determination. Microstep 3.4.1 must confirm or replace it before any customer PII is processed, and the resulting allocation is contractual:

| Flow | Controller | Processor |
|---|---|---|
| Customer profile, consent, loyalty | the merchant | the vendor (hosting, sync, back office) |
| Staff accounts, PINs, audit trail | the merchant | the vendor |
| Crash and error telemetry | the vendor (own product-improvement purpose) | the telemetry provider, as sub-processor |
| Support log retrieval and the diagnostic bundle | the merchant | the vendor, acting on instruction and audited into the merchant's own trail |

The instruments that make the table binding — a data-processing agreement, a named sub-processor list with a change-notice term, and the merchant-facing privacy commitments — are commercial documents, not code. They are a launch prerequisite and they do not exist yet; recording that here is the honest position, and drafting them is a Phase-5 milestone.

### Cross-border transfer needs a basis, not a cipher

An earlier revision answered "restricted cross-border transfer" with "sync over TLS with server-side access control; hosting region is a merchant decision". That was wrong twice. TLS protects data in motion and says nothing about whether the data may lawfully leave Jordan. And on one shared multi-tenant service (§5b) the hosting region is structurally the **vendor's** decision — a merchant cannot answer a question about a machine they do not choose, so a questionnaire row asking them to is a row that stays blank.

> ⚠️ **OPEN — blocks 3.1.6.** In which country and legal entity will the shared service and each subprocessor host merchant and customer data, and what cross-border basis applies? Default until answered: no customer PII may sync or enter telemetry outside Jordan; only non-PII fixtures may use a development host.
> Owner: 3.1.6. Source that settles it: the signed hosting/subprocessor contract, Jordan PDPL transfer assessment and counsel's written conclusion.

The same question governs telemetry, which is the one outbound personal-data-adjacent flow the vendor controls unilaterally. Until it is answered, telemetry ships **off by default** with a merchant-visible setting, and the provider's region is part of the answer rather than a detail of the transport.

### The breach clocks, and the containment nobody wrote down

The plan carried one deadline — notify affected individuals within 24 hours — and built a runbook around it. Pending the OPEN item below, the interim runbook tracks two candidate clocks separately: notice to affected individuals and a report to the supervisory unit. Collapsing them before counsel confirms their values, content and channels would let the drill exercise one path while silently omitting the other.

> ⚠️ **OPEN — blocks 5.3.2.** What are the two statutory deadlines, what must the regulator's report contain, and through which channel is it filed? Default until answered: individuals within **24 hours** and the supervisory unit within **72 hours**, with the report carrying the source, the mechanism, the affected population and everything known at filing time. Owner: 5.3.2. Source that settles it: PDPL Article 20 and the supervisory unit's filing instructions, confirmed by counsel.

The runbook is written against both defaults, and the tabletop times both independently from the moment of discovery. It also covers what the previous version omitted entirely — the technical response, not just the paperwork:

1. **Contain.** Revoke the affected device tokens and back-office sessions; halt the release channel; disable the support console's log retrieval if that is the vector.
2. **Preserve.** Snapshot logs, the audit chain head, and the server state *before* remediation, because remediation destroys evidence.
3. **Scope.** Which orgs, which stores, which data classes, which time window — answerable only because §5b makes `org_id` a column rather than a filter someone remembered.
4. **Rotate.** Database keys, device keys, entitlement and updater signing material as the vector requires, each with the procedure in §6a.
5. **Notify**, on both clocks, from the templates.
6. **Recover and review**, with the finding entering the test catalog as a numbered case.

### Why the consent record shape matters

A boolean says *"they agreed."* A regulator asks *"to what, exactly, and when?"* Storing the **wording version** means the answer is retrievable years later, after the terms have been rewritten twice. This costs one column and is the difference between a defensible position and a story.

Two things make it actually retrievable, and a bare `text_version` string is neither of them:

- **The notice is an immutable record, referenced by the event** — full Arabic and English text, locale, the purposes offered, the data classes, the recipients, effective dates, and a content digest. A version label with no retained content proves that something was called v2, not what the customer read. Content under an existing version never changes; a change is a new version.
- **The event ledger, not the current row, is the evidence.** A grant is an event; a withdrawal is an event referencing the one it supersedes. The effective state is the latest event the server accepted — see [`sync-protocol.md`](sync-protocol.md) §1, where field-level last-write-wins on consent is what allowed a stale grant to overwrite a withdrawal.

### Erasure without destroying the books

Deleting a customer must not delete their sales — those are tax records with a statutory retention period, and the merchant is legally required to keep them. Erasure is therefore **anonymisation**: PII nulled, `is_anonymized` set, ledger and sale rows preserved against the now-anonymous id.

**Anonymisation and the hash chain would otherwise contradict each other.** The chain covers `audit_log`, which refuses `UPDATE` unconditionally, and the customer rows anonymisation nulls sit outside it — so the two only collide if audit payloads carry PII. They must not, and `audit_log.payload` already says so in its DDL. The rule is therefore stated as an invariant rather than discovered later:

- **No PII in any hash-chained payload.** An audit entry names a customer by id, never by name, phone or email. `no_pii_in_a_full_sale_trace` walks the canonical field registry (§6) across every emitted log and audit payload, so an audit-only duplicate test cannot drift from the sink-wide assertion.
- **Anonymisation never rewrites an audit row.** It appends one — `customer.anonymized`, with the actor, the request reference, and the id — so the erasure itself is evidence.
- Any *other* table anonymisation touches is reference or mutable-shared data, not a fact, and is not chained.

**Tests:** `anonymize_nulls_pii_and_keeps_ledger_rows` · `sales_survive_anonymization_with_totals_intact` · `anonymized_customer_is_not_findable_by_phone` · `no_pii_in_a_full_sale_trace`. The anonymization fixture also asserts one appended audit row and no audit-row mutation.

### The PII estate — every place personal data actually lands

Export and erasure were designed against the customer row. Personal data is in more places than that, and a right that is honoured in one of them is not honoured. The inventory is the specification for both the export and the erasure job, and for the breach-scoping step above:

| Location | Contains | Erasure action |
|---|---|---|
| `customer` | name, phone, email | null, set `is_anonymized` |
| `consent_event`, and the notice it references | who consented, when, to which wording | retained — it is the evidence that the processing was lawful; the identity becomes the anonymised id |
| `sale.buyer_name`, `sale.buyer_tin` | the B2B buyer on a fiscal document | **retained under a lawful-retention exception** — it is on an issued tax invoice |
| `sync_outbox.payload` | a JSON copy of any fact carrying the above | pruned on acknowledgement (`sync-protocol.md` §5); the copy must not outlive the retention decision on its source |
| `receipt_artifact.content_bytes` | the rendered bytes, including any buyer block | retained with the sale; it is the document that was handed over |
| `fiscal_queue.payload_xml`, `fiscal_result.raw_response` | buyer identity as transmitted to and echoed by ISTD | **retained under a lawful-retention exception** |
| `fiscal_reconciliation_issue.error_body` | whatever the remote echoed back, verbatim | scrubbed on write against the field registry; retained otherwise |
| Server replicas and their backups | everything above | a `privacy_tombstone` propagates down and is re-applied after any restore ([`sync-protocol.md`](sync-protocol.md) §3) |
| Telemetry sink | nothing, by design — proven by test, not asserted | n/a |
| The support diagnostic bundle | whatever it collects | the bundle is generated through the scrubber, never from raw files, and its retention is bounded |

The two rows marked as retention exceptions are the honest hard part: an issued tax invoice carries buyer identity and the merchant is required to keep it. That is not a gap to be closed by code; it is an exception to be **written down and defended**, and it is exactly what the export file must disclose to the data subject rather than silently omit.

**Test:** `a_canary_identity_is_absent_from_every_store_in_the_inventory` — insert a unique canary value as a customer's name, run a sale, a sync, a backup, a diagnostic bundle and a panic, then search every location above and assert the canary appears only where this table says it may.

### Data-subject rights beyond access and erasure

Consent, access, correction, anonymisation and marketing consent were implemented; objection, restriction, portability and complaint handling were not, and a right the product cannot receive is a right the merchant cannot honour. All of them route through one mechanism rather than four:

- **A request case log** — subject, right invoked, channel, who verified their identity, what was done, when, by whom. Authenticated, because an unverified erasure request is an attack.
- **A response clock with an alarm** before it expires. The exact period is part of the counsel question above; the default is the shortest published figure.
- **Portability** is the export file in a machine-readable format, which the access right already produces.
- **Complaint handling** is a named contact and a route to the regulator, published in the privacy notice.

---

## 3 · PCI DSS — and the claim you may actually make

**The architecture:** semi-integrated, certified terminals only. The amount and a reference go to the terminal; a result and a reference come back. Card data is captured and encrypted by the terminal and travels to the PSP without passing through this application.

**What that buys:** the cardholder data environment shrinks to the terminal, so the merchant completes a short self-assessment questionnaire instead of a full audit.

**The nuance the master plan understates — and it changes the claim:**

> **SAQ P2PE applies only if the terminal is part of a PCI-listed, validated P2PE solution.** "Semi-integrated" and "P2PE-validated" are different properties, and a terminal can be the first without being the second. If the acquirer supplies an internet-connected terminal that is not on the PCI SSC's validated P2PE list, the merchant lands on **SAQ B-IP or SAQ C** — substantially longer, pulling the store network and supporting infrastructure into scope.

**"The engineering does not change either way" was wrong.** It is true that PAN never enters this process whichever SAQ applies, and that is the point of the architecture. It is not true that the answer is engineering-neutral. SAQ B-IP carries eligibility and isolation conditions on how the terminal reaches the acquirer. SAQ C pulls in network segmentation, configuration standards, patching, access control, monitoring, testing and written policy — obligations that land on the store network, on the register's operating system, and on how this vendor supports it remotely. Discovering that in Phase 5, after three stores have taken cards, is a network and operations redesign rather than a form.

So the SAQ is not a label chosen at the end; it is an input to the store deployment baseline, and the question is asked at the beginning.

> ⚠️ **OPEN — blocks 2.1.1.** Which exact PCI SAQ applies to the selected acquirer, terminal model and firmware, PTS/P2PE listing, integration protocol, store network and support model? Default until answered: design and operate to the SAQ C baseline, reject any integration that exposes a full PAN to this process, and make no P2PE-eligibility claim anywhere.
> Owner: `2.1.1` collects the evidence; `5.3.3` determines the SAQ. Source that settles it: the acquirer's written responsibility matrix and a QSA determination against the current PCI SSC eligibility criteria.

### The acquirer conversation, in full

One question was specified — the P2PE listing number. It is not enough to determine scope, and the conversation happens once. Microstep 2.1.1 asks for all of it, in writing, and records it in [`merchant-decisions.md`](merchant-decisions.md):

| Ask | Because |
|---|---|
| Which SAQ do you expect this merchant to complete? | Their answer is the starting point and may not match yours |
| The terminal's **PTS approval** and **P2PE listing** numbers, or an explicit "neither" | The listing, not the marketing, decides |
| Exact terminal **model and firmware version** | A listing applies to versions, not to product families |
| How does the terminal reach you — store LAN, cellular, or the register? | This is the difference between B-IP and C, and between an isolated device and one on the merchant's network |
| A **responsibility matrix**: which controls are yours, which are the merchant's, which are the POS vendor's | Every party assuming another holds a control is how the control ends up unheld |
| The revalidation cadence, and what invalidates the answer | A firmware update can move the merchant to another SAQ silently |
| Is a **status query** by original reference supported? | Without it there is no safe timeout recovery — choose a different acquirer (risk register, 2.1.1) |

### Non-negotiables restated

1. Store `psp_ref` on every card tender — reconciliation and refunds both depend on it.
2. Store masked PAN and scheme **for the receipt only**. Nothing else from the card, ever.
3. Treat a timeout as **unknown** → status-query before any retry, or you will double-charge.
4. Support partial approval and split tender from day one; bolting them on later deforms checkout.
5. Card refunds go through the PSP against the original reference, never as a fresh charge.
6. **A driver response carrying a full PAN is an integration rejection.** The old test fed one in and asserted it was discarded — which quietly conceded that PAN can enter the process, and a PAN in memory is a PAN in a crash dump. The correct behaviour is to refuse the response with a named error, persist nothing, and alarm; a terminal that returns one is not integrated until it stops.

**Tests:** `card_tender_persists_only_the_three_allowed_fields` · `full_pan_never_reaches_the_database` — feed a driver response containing a full PAN and assert the parser refuses it with a named error, nothing is persisted anywhere, and the alarm fires.

---

## 4 · The audit hash chain (gap G-7)

`prev_hash` and `hash` columns are specified in the blueprint. The specification of *what is hashed* was not — and an unverifiable hash chain is decoration.

### Canonical serialization

```
canonical_bytes(entry) = JSON with:
    keys sorted lexicographically at every level
    no whitespace
    UTF-8, no BOM
    integers as integers, never floats
    timestamps as ISO-8601 UTC with milliseconds
```

Pinned by a **golden test** (`golden_canonical_bytes_are_stable`), not left to `serde_json`'s field ordering. A serde version bump that reorders fields would otherwise silently invalidate every historical hash.

**What is hashed is the persisted entry, not the intent.** The first design hashed `AuditIntent`, which carries the actor, the action and the payload — and omits `audit_log.seq`, `audit_log.id` and `audit_log.register_id`. Those three are exactly the columns an insider would change: move a refund to another register, renumber it, or re-point its identity, and the chain still verifies because none of it was ever hashed. The canonical entry binds every immutable persisted field:

```
domain tag        "pos.audit"            — so a hash cannot be replayed into another chain
chain version     1                      — so a future serialization change is explicit
register_id, id, seq
actor, approver, action, entity, entity_id, reason, payload, at
```

The type lives in [`domain-api.md`](domain-api.md) §9. Adding a field to it is a chain-version bump, not a silent edit.

### The chain

```
hash₀ = BLAKE3(GENESIS ‖ canonical_bytes(entry₀))
hashₙ = BLAKE3(hashₙ₋₁ ‖ canonical_bytes(entryₙ))
GENESIS = [0u8; 32]
```

BLAKE3 rather than SHA-256: faster on the low-end CPUs registers actually run, and 32 bytes either way.

### Coverage

Logins and logouts · user switches · **every void** · **every refund** · **every price override with its reason** · every discount above the cashier cap · drawer opens including no-sale · cash movements · shift open, close, and force-close · Z generation · settings changes · PIN resets · training-mode toggles · sync anomalies · fiscal rejections · every approval handle issued and consumed · every support access to this merchant's data · audit-chain breaks themselves.

### What the chain proves, and what it does not

This is the part that was overclaimed, and the mechanism is worth keeping precisely because the honest version is still valuable.

BLAKE3 is unkeyed and the chain lives in a database whose key the merchant holds. Anyone with that key can edit a row and recompute every hash after it, or delete the newest entries and leave a shorter chain that verifies perfectly. The schema already admitted the second half — "the hash chain detects a modified row but cannot detect a deleted tail" — while `prop_chain_detects_deletion` and the §9 row "tampered ledger → hash chain + verifier" were stated without qualification. A keyed hash would not help: the key would have to live on the same machine, in the same custody.

**Without an external anchor, the chain proves:**

- that no row was changed, reordered or removed **by anything that does not recompute the chain** — which covers every bug, every crash, every partial write, and every casual edit through a SQL console;
- that the `UPDATE`/`DELETE` triggers were not bypassed, since bypassing them requires dropping them, which is itself visible;
- **where** the first inconsistency is, which is what a forensic investigation needs to start.

**It does not prove:** that a determined holder of the database key did not rewrite history. Only an anchor outside their control can do that, and that is the next section. Say it this way to a merchant, and to a court; the alternative is a claim that collapses the first time it is tested.

### The anchored checkpoint

The chain head is exported to somewhere outside the database, and `audit_checkpoint` records each export:

```
(register_id, last_seq, last_hash, source_kind, anchor_ref, anchored_at)
source_kind ∈ { z_report, verified_backup, server }
```

**Phase 1 already has two anchors**, which matters because Phase 1 has no server and is the phase in which a register holds the only copy of anything. Every Z close and every verified backup exports the head: the Z is a printed, numbered, immutable document, and the backup is a file whose own integrity is verified. Neither is beyond a determined merchant's reach, but both are outside the database and both are dated, so rewriting history now requires rewriting every artifact that ever recorded its head.

**From Phase 3 the server is the third anchor**, and the strongest one, because the merchant does not hold it. Every push carries the head, signed by the register's non-exportable device key (§6a). The server stores the highest checkpoint it has ever accepted per register and:

- **refuses a checkpoint below the last one** — a rollback attempt, alarmed;
- **refuses a checkpoint that forks** — same `last_seq`, different `last_hash`, which is either a clone or a rewrite, alarmed;
- serves the last accepted checkpoint back down, so `verify-audit` can compare against it offline.

Tail deletion above the newest anchor remains undetectable, and that residual is bounded by the interval between anchors — one shift, one backup cycle, or one sync. It is stated in the risk table of [`sync-protocol.md`](sync-protocol.md) §7 rather than left implicit.

The property name changes with the mechanism, because a property that cannot hold is worse than no property:

| Test | Asserts |
|---|---|
| `prop_chain_detects_any_single_entry_mutation` | any changed byte of any entry breaks verification at that sequence |
| `prop_chain_detects_deletion_before_the_anchor` | removing any entry protected by the retained anchor breaks verification |
| `prop_chain_detects_reordering` | swapping two entries breaks verification |
| `mutating_an_identity_column_breaks_the_chain` | `register_id`, `id` and `seq` are inside the hash, one test per column |
| `a_z_close_anchors_the_head` | Phase 1; the checkpoint row exists and matches the chain |
| `tail_deletion_is_detected_against_the_last_anchor` | deleting entries above a Z or backup anchor is caught locally |
| `tail_deletion_is_detected_against_the_server_checkpoint` | Phase 3; the same, against an anchor the merchant does not hold |
| `a_checkpoint_below_the_stored_head_is_refused` | rollback |
| `a_forked_checkpoint_is_refused_and_alarms` | clone or rewrite |
| `chain_survives_process_restart` | the head is read from the database, not from memory |

### Behaviour on a break

**The register does not stop selling.** It raises an alarm, records the break with its sequence number, and surfaces it in back-office device health.

A tamper-evidence mechanism that halts trade converts a forensic signal into an outage — and the most likely cause of a break is a bug in your own serialization, not a thief with a hex editor.

### The verifier

`crates/pos-db/src/bin/verify-audit.rs` (microstep 5.4.4) — a CLI that walks a register's chain, compares its head against the last server-anchored checkpoint when one is available, and reports the first break. The forensic tool you hope never to need and cannot build under pressure.

Reproducing a break for a drill requires suspending the append-only triggers on a **copy** of the database, because the triggers correctly refuse the edit. That is the only sanctioned way to produce a tampered state, and it is what an investigator would do too.

---

## 5 · Permission enforcement (gap G-6)

"RBAC enforced in Rust commands, not in the UI" is the right rule and needs a mechanism, or the twentieth command ships without a check and nobody notices for a year.

### The proof-carrying token

```rust
pub trait Capability { const NAME: &'static str; }

pub struct Authorized<C: Capability> {
    actor: UserId,
    approver: Option<UserId>,        // distinct on escalation
    at: Timestamp,
    _capability: PhantomData<fn() -> C>,
}
```

A **marker type**, not a const-generic string. `Authorized<const C: &'static str>`
does not compile — rustc: "`&'static str` is forbidden as the type of a const
generic parameter". The full declaration, the `capabilities!` macro that derives
`cap::ALL`, and the accessors live in [`domain-api.md`](domain-api.md) §8; this
section is about what the token *buys*, not how it is spelled.

The fields are private on purpose. Public fields make the token a struct literal
anybody can write, which proves nothing.

`authorize` is the **only** way to obtain a `&Authorized<C>`, and every domain function that reverses money or opens the drawer **requires one in its signature**. You cannot forget the check, because you cannot call the function without it.

```rust
pub fn void_sale(sale: Sale, reason: VoidReason,
                 auth: &Authorized<cap::SaleVoid>) -> Result<…>;
```

Two things the compiler enforces, both `trybuild` cases in 1.6.4: a validly
obtained token for the **wrong** capability is a type error, and a token cannot
be **forged**, because the marker field is private.

### The approval handle

`Authorized<C>` proves, at compile time, that *a* check for capability `C` happened. It cannot prove *which operation* a manager approved, because a capability is a class of operation and money is a specific one. The manager-approval flow returned "a short-lived token naming the capability it authorises", nothing consumed it, and nothing bound it to an amount — so one approval, or one shoulder-surfed manager PIN, authorised a class of refunds rather than the refund the manager was shown.

```rust
pub struct ApprovalHandle {
    id: ApprovalId,
    capability: String,
    actor: UserId,               // who will perform it
    approver: UserId,            // who approved; always distinct from actor
    entity_id: Uuid,             // this sale, this line, this shift
    amount_minor: i64,           // exact; zero is never a wildcard
    content_hash: Option<PreparedIntentHash>, // exact canonical prepared row, when any
    reason: String,
    issued_at: Timestamp,
    expires_at: Timestamp,
    nonce: [u8; 16],
}
```

The rules that make it a proof rather than a bearer note:

1. **Every privileged IPC command takes its `approval_id` explicitly.** The webview receives only
   `ApprovalRef`; the Rust handler resolves the retained `ApprovalHandle`, so there is no implicit
   session approval and no bearer proof in JavaScript. The registry makes that requirement visible
   to the exhaustiveness test.
2. **It is consumed in the same transaction as the financial effect and its audit row.** Three writes, one commit: the effect, the audit entry, and the `approval_consumption` row naming the effect it paid for. A crash between them is impossible rather than recovered.
3. **One use, ever.** Consumption is a row keyed by the handle, so a replay is refused by the primary key and the refusal survives a restart — an in-memory set would forget it exactly when a restart is the attack.
4. **Every field is checked, not just the capability.** A handle for 20.000 does not authorise 200.000; a handle for sale A does not authorise sale B; a handle issued to cashier X does not authorise cashier Y. A prepared-intent handle also binds a BLAKE3 `content_hash`; issue and commit each recompute the versioned canonical bytes loaded from the database, so keeping the row id while changing its quantity, reason or product fields is refused.
5. **It expires**, in seconds rather than minutes, because the manager is standing at the till.
6. **The handle itself is immutable and undeletable**, because it is the evidence of who approved what. Both halves are fact tables with the usual triggers.
7. **Amount and reason are always present.** A non-money operation binds `amount_minor = 0`; zero is an exact value, never a wildcard. An optional amount or reason would let a handle omit the very field the effect must match.
8. **Actor and approver always differ on a handle path.** `ban_self_approval` determines whether an operation needs escalation; it never permits a self-issued handle once that path is selected.
9. **Prepared intents freeze when approval is issued.** The database refuses `UPDATE` on a matching `product_quick_add_request` or `stock_adjustment_request` once an approval references it, while the independent hash check catches corruption or a bypassed trigger. The webview supplies the visible fields, never the hash.

**Tests:** `a_handle_used_twice_is_refused` · `an_altered_amount_is_refused` · `a_different_sale_is_refused` · `a_different_actor_is_refused` · `a_consumed_handle_is_still_consumed_after_restart` · `an_expired_handle_is_refused` · `the_effect_and_the_consumption_commit_together_or_not_at_all` · `altering_a_stock_request_after_approval_is_refused` · `altering_a_quick_add_request_after_approval_is_refused`.

### The exhaustiveness test

Every IPC command registers `(name, capability, audited, approval_requirement)`, where
`approval_requirement` is `Never`, `Always { binding }`, or
`Conditional { predicate, binding }`. The binding names an entity, an exact amount source and a
reason source; a non-money effect uses exact zero. Microstep 1.6.7 makes
`ipc_commands_all_declare_a_capability`, `every_privileged_command_binds_its_approval`, and
`conditional_privilege_cannot_cross_threshold_without_approval` walk the registry and generated
handler list. Until 1.6.7 lands, this is a required contract, not a current CI claim.

Two things it does **not** cover, each with its own guard:

- **Plugin commands.** The webview's capability file is where an updater or filesystem permission would be granted, and `generate_handler!` never sees it. `webview_cannot_invoke_the_updater_plugin` audits that file against an allowlist, so a default configuration cannot expose install-an-update to JavaScript.
- **HTTP routes.** The registry protects IPC, and the back office is Axum. See §5a.

**Verify it at 1.6.7** with the committed negative registry fixtures; they add a command without a
spec and an `Always` entry without a binding and require the named test command to fail. A guard
whose refusal path has not run is not evidence.

### Escalation

- The approver's id is a **different column** from the operator's.
- For anything that takes an `ApprovalHandle`, actor and approver are distinct **by construction** — `approval_handle` carries the `CHECK` — so self-approval on those paths is not a policy that can be switched off. `ban_self_approval` (default on) governs whether an operation requires escalation at all (E.52); it never relaxes that `CHECK`.
- Approval is bound to the exact operation by the handle above, so it can be neither replayed nor re-aimed.
- Offline registers honour a **max-offline-auth window** for permission changes (E.55) — a real limit of offline-first, and one to disclose rather than hide. What makes the window enforceable is the lease below.

### PIN strength, rate limiting, and lockout

A four-digit PIN has 10,000 candidates. At the 250 ms verification the plan tunes for, exhausting them takes about **42 minutes**, and half that on average. "Not a 4-digit brute force in an afternoon" was arithmetically wrong; it is a brute force over a lunch break, on a device the staff hold all day, against a database an insider can copy.

Argon2id is still right. What was missing is everything around it:

| Control | Requirement |
|---|---|
| Parameters | named `m_cost`, `t_cost` and `p_cost` floors, versioned, with a rehash-on-login path when the floor rises. Asserted by parsing them back out of the PHC string, not by timing a test |
| Attempt limiting | per user **and** per register, **persisted** in `auth_attempt_state`, with escalating delay then lockout. In memory it resets on reboot, and rebooting is free |
| Lockout release | manager action, audited — not a timeout alone for a manager account |
| Alerting | repeated failures are an audit entry and a device-health signal, because a brute force looks like nothing until it succeeds |
| Manager credentials | longer than four digits for any account holding `refund.*`, `settings.edit`, `user.admin` or `zreport.run` |
| High-value operations | a second factor, not a longer PIN, for cash-for-card refunds, user administration, key recovery and restore |

The second factor is a hardware and cost decision, not a code decision, and it is the merchant's to make.

> ⚠️ **OPEN — blocks 1.6.2.** What second factor is available on a Jordanian minimarket counter for the high-value operations above — a manager badge, a security key, or a back-office confirmation over the network? Default until answered: a six-digit-minimum manager PIN plus a mandatory audited reason, and the operation is listed in the daily exception report. Owner: 1.6.2, with the choice recorded in `merchant-decisions.md`. Source that settles it: the merchant, on their own hardware budget.

**Tests:** `argon2_parameters_match_the_reviewed_profile` · `failed_attempt_state_survives_restart` · `manager_reset_retires_old_hash_and_audits`. The reset fixture also refuses a short manager PIN and proves that only the audited manager path clears lockout.

### The offline authorization lease

The plan promised a 72-hour bound on offline authority and gave it no state and no algorithm. A naïve implementation compares an expiry against the device clock, which a cashier can change, and forgets everything on reboot.

- The server issues a **signed `authorization_lease`** binding the org, the user, the capability, the store scope, the issue time and a hard expiry. A register never mints one, so it cannot extend its own authority.
- `trusted_time_state` persists the **highest timestamp the register has ever seen from an authenticated server**, alongside boot-monotonic elapsed time and a confidence value. That pair, not the wall clock, decides whether a lease is live.
- A backward jump, an unexplained reboot, or elapsed time that cannot be established marks time **suspect**.
- With time suspect or the lease expired, **privileged capabilities fail closed** — refunds, overrides, settings, user administration — and `sale.create` does not. Selling never fails closed; authority does.

**Tests:** `offline_auth_window_expires_and_says_why` · `a_clock_rollback_marks_time_suspect_and_fails_privileged_grants_closed` · `an_expiry_during_an_open_shift_never_blocks_a_sale` · `repeated_offline_reboots_do_not_reset_the_window`.

### Two things nobody may do, ever

Written down because "no command exists" is only obvious to whoever remembers deciding it:

- **A closed shift is never reopened.** A correction is a cash movement on the next shift, with a reason.
- **A Z report is never voided or re-run under its own number.** Re-running produces a new numbered document; the old one stands.

Both are enforced by the absence of a command and by the append-only triggers, and both belong in the manager guide, because the first person to ask is a manager.

---

## 5a · Server identities and authorization

The only server authentication the plan defined was a device-scoped register token. Nothing said how an owner, a manager or a support operator signs in — and the back office is where customer exports, user administration, reports, device control and fiscal reconciliation live. The IPC capability registry protects Tauri commands; it has no opinion about an Axum route.

**Two kinds of principal, and they are not interchangeable.**

```rust
pub struct DevicePrincipal   { org_id: OrgId, store_id: StoreId, register_id: RegisterId }
pub struct BackOfficePrincipal {
    subject: SubjectId,          // from the identity provider, never a local password
    org_id: OrgId,
    store_grants: Vec<StoreId>,  // empty ⇒ org-wide, as in `user_role.store_id IS NULL`
    capabilities: GrantSet,
}
```

| Concern | Position |
|---|---|
| Human authentication | OIDC Authorization Code with PKCE against a managed provider. This product does not store back-office passwords |
| MFA | phishing-resistant (passkey or security key) for any principal holding `user.admin`, `settings.edit`, `customer.lookup` or `reports.all` |
| Sessions | server-side, short-lived, revocable from the back office and revoked on role change; `Secure`, `HttpOnly`, `SameSite`; CSRF protection on every state-changing route |
| Authorization | **deny by default.** Every route declares its required capability in a registry, exactly as IPC commands do |
| Scope | `org_id` and the store grants come from the principal, never from a path or query parameter (see [`sync-protocol.md`](sync-protocol.md) §5 rule 4) |
| Support operators | a distinct principal class, per-action, rate-limited, and **audited into the merchant's own trail** so the merchant can see who read what and when (§8a) |

**The exhaustiveness test mirrors the IPC one**, because the failure mode is identical: `http_routes_all_declare_a_capability` walks the router and fails on any route with no registry entry. The OpenAPI document emits the security requirement per operation from the same registry, so the published contract cannot disagree with the check.

**Tests:** `http_routes_all_declare_a_capability` · `an_unauthenticated_request_is_refused_on_every_route` · `a_session_revoked_mid_flight_is_refused_on_the_next_request` · `a_principal_without_a_store_grant_cannot_read_that_stores_sales` · `mfa_is_required_for_every_privileged_capability` · `a_support_access_writes_an_audit_row_the_merchant_can_see`.

---

## 5b · Tenancy — one shared service, enforced in the database

The deployment model was never decided, and one sentence in Phase 5 — "device health across merchants" — was the only place the set admitted that one server holds several merchants' data. Everything else was written as if the question would answer itself, which for a schema it does not: a multi-tenant schema can serve a single tenant, and a single-tenant schema cannot be made multi-tenant without rewriting every query that has already shipped.

> **Decision recorded for sign-off: one shared multi-tenant service.**
> The alternative — one instance per merchant — removes cross-tenant leakage by construction and costs a deployment, a database, a backup schedule, a monitoring target and an upgrade window *per merchant*, which a solo vendor cannot operate at ten merchants. The trade-off accepted here is that isolation becomes an engineering obligation instead of a physical fact. **Overrule this in review if the first customers are large enough to pay for dedicated instances**; it is cheap to run one tenant on a multi-tenant schema and expensive to discover the reverse.

What the decision obliges, all of it in the schema rather than in application discipline:

| Obligation | Detail |
|---|---|
| `org_id NOT NULL` on every merchant-owned table | including `product`, `role`, `category` and everything else currently keyed only by its own id |
| Tenant-scoped unique keys | `(org_id, sku)`, not a global `sku`. Two merchants selling the same barcode is the normal case, not a collision |
| Composite foreign keys | a child references `(org_id, parent_id)`, so a row cannot point across a tenant boundary even if a query forgets to filter |
| Forced row-level security | `ENABLE` **and** `FORCE ROW LEVEL SECURITY`, under an application role that is neither the table owner nor `BYPASSRLS`, with default-deny policies |
| The tenant in the session, from the principal | set per transaction from `BackOfficePrincipal.org_id` or `DevicePrincipal.org_id`, never from request data |

**A cross-tenant leak is a personal-data breach caused by the vendor, not by the merchant.** It affects every merchant on the instance simultaneously, it starts both clocks in §2 for all of them, and the notification is the vendor's to make about their own defect. That is why isolation is tested adversarially rather than reviewed:

**Tests:** `prop_no_query_crosses_an_org_boundary` — two fully populated orgs, every read and every write in the API surface attempted as each, asserting nothing from the other is ever returned or modified · `rls_is_forced_on_every_merchant_owned_table` (a catalogue test over the schema, so a new table without a policy fails) · `the_application_role_is_not_the_owner_and_lacks_bypassrls` · `a_composite_foreign_key_refuses_a_cross_org_parent` · `two_orgs_may_use_the_same_sku`.

---

## 6 · Secrets and PII in logs (gap G-8)

The interim breach-response clocks make "no PII in logs" an operational claim that needs a test;
the statutory deadlines remain open at 5.3.2.

### The scrubbing layer

A `tracing` layer that redacts, at any nesting depth, fields named: `pin`, `pin_hash`, `pan`, `card_number`, `cvv`, `track`, `phone`, `email`, `customer_name`, `buyer_name`, `secret_key`, `client_id`, `db_key`, `token`, `password`, `entitlement`, `recovery_code`, `enrollment_code`, `wrapped_key` — plus any field whose name ends in `_token`, `_secret`, `_key`, `_pin` or `_hash`, or contains `password`, because `device_token` is not spelled `token` and exact-name matching is how a new field arrives unredacted.

It sits in front of **every** sink: stdout, file, and Sentry.

**This list is a registry, not prose.** It lives once, as a `pub const` in the scrubbing module, and everything that needs it derives from it: the layer, the parameterized tests, the audit-payload assertion in §2, the diagnostic bundle filter, and the entry in `.claude/rules/security.md`. It was written out by hand in three places and the Phase-1 microstep's copy was already short by five fields — `card_number`, `buyer_name`, `token`, `password`, `entitlement` — which is a scrubber that passes its own test and leaks a card number.

**Three channels the layer cannot reach**, each needing its own control:

| Channel | Why the layer misses it | Control |
|---|---|---|
| `IpcError.detail` | it is serialized to the webview, not emitted through `tracing` | production errors carry a **typed code and a static detail**; the source error goes to the separately scrubbed sink. A free-form string built from a database or PSP error is how a bind value reaches a screenshot |
| Panic payloads and `Debug` output | a message is one opaque string, so no field name exists to match | secret-bearing types implement `Debug` and `Display` as a redacted constant; SQL bind-value logging is off |
| Generic carrier keys — `error`, `body`, `message`, `detail`, `response` | the name is innocent; the value is not | treated as untrusted: never logged verbatim from a remote response, and covered by the canary test below rather than by name matching |

### The tests that make it a position rather than an intention

| Test | Asserts |
|---|---|
| `scrubber_redacts_every_known_pii_field` | every entry of the registry, generated from it rather than listed again |
| `scrubber_redacts_nested_json` | nesting does not evade it |
| `scrubber_redacts_every_suffix_rule` | `device_token`, `client_secret`, `pin_hash`-shaped names |
| `no_pii_in_a_full_sale_trace` | run a complete sale with a customer attached, capture every log line, assert the fixture's phone and name are absent |
| `no_pii_in_a_captured_panic` | the same for a panic payload reaching Sentry |
| `ipc_errors_carry_no_source_detail_in_release` | the webview receives a code, not a database message |
| `credentials_never_logged` | JoFotara `Secret-Key` and `Client-Id` |
| `full_pan_never_reaches_the_database` | a driver response containing a full PAN is refused, persists nothing, and alarms |
| `a_canary_identity_is_absent_from_every_store_in_the_inventory` | the estate test from §2, which is the one that catches the channel nobody thought of |

### Secrets hygiene

- `.env` is git-ignored; only `.env.example` is committed.
- The SQLCipher key lives in the OS credential store. `POS_DB_KEY` is honoured in debug and CI and **ignored in release**, where the credential-store lookup simply continues (conventions §12). The refusal is ignore-and-continue rather than an error on purpose: falling through is the safer outcome, and a stray variable inherited from a shell must never stop a register from opening its till.
- JoFotara credentials live in the keyring; the database stores only a pointer and a four-character hint for the diagnostics screen.
- Signing keys are **never** present on a CI step that compiles third-party code (§6b).
- Verified by `git log -p | rg 'PRIVATE KEY'`, a pre-commit hook, and a full-history secret scan in `just pre-push`.
- Cargo-deny, the reviewed JavaScript licence-metadata gate, and `pnpm audit` run in CI; accepted
  advisories require a dated, reasoned exception, and distribution still requires a real notice audit.

---

## 6a · Key custody and lifecycle

Every key in this product had a birthplace and no lifecycle. The updater key "exists, is in secrets"; the entitlement key was named; the database key was generated on first run and stored in one place. Nobody wrote down who holds it, how it rotates, or what happens when it is lost — and for two of them, loss is unrecoverable in a way that ends the product.

| Key | Lives | Rotation | Loss | Compromise |
|---|---|---|---|---|
| **Database key** (per register) | OS credential store; wrapped copy beside every backup | rekey via SQLCipher with the old key present; the wrapped envelope is re-issued | recovery code, below | rekey and re-issue every envelope; treat the register as breached |
| **Merchant recovery code** | issued and **displayed once** at provisioning, printed, merchant-held; from Phase 3 the wrapped envelope is also in `org_recovery_envelope` | re-issue rotates the envelope, not the data key | the merchant's problem to avoid, and the reason it is printed at provisioning rather than emailed later | re-issue immediately; every old envelope becomes worthless |
| **Device identity key** (per register) | OS or hardware keystore, **non-exportable** | re-enroll | re-enroll the register | revoke the token; the clone fails its next request (`sync-protocol.md` §5) |
| **Updater signing key** | offline host or a policy-bound signing service — **never a CI secret on a build step** | overlapping validity, with the new public key shipped in a build signed by the old one | every installed register needs a site visit. This is the worst single-key outcome in the product | revoke, rotate through the bridge above, and notify every merchant |
| **Entitlement signing key** | as above | `kid` in every entitlement, so two keys can be valid at once | no merchant can renew | revoke by `kid`; issue replacements |
| **OS code-signing identities** | vendor's platform accounts | annual, and the expiry date is on a calendar | releases stop until re-issued | revoke through the platform |
| **JoFotara credentials** | OS keyring per register; pointer only in the database | see the open item below | the merchant re-issues from their portal | revoke and re-issue; the queue must resume on the new credentials without re-submitting a cleared document |

**Two copies, two custodians, for anything whose loss is unrecoverable.** Each recovery copy is independently encrypted and stored somewhere the other is not.

### The database key and the backup that could not be opened

The backup was "SQLCipher-encrypted with the same key" as the live database, and that key existed only in the OS credential store. So the two things that fail together — a wiped machine and a wiped credential store — take the live database *and* every backup with them. The Phase-1 exit demonstration asked for exactly that scenario and could only pass in a debug build, where `POS_DB_KEY` supplies the key that a release build ignores.

**The design (microsteps 1.8.5b and 1.8.6b):**

- A random **data key** encrypts the database.
- The data key is **wrapped** under a key derived from a merchant-held **recovery code**, issued and displayed once at provisioning.
- The wrapped envelope — `org_recovery_envelope`, carrying the data-key id, the wrap and KDF algorithms and their parameters — is stored **beside every backup**, and from Phase 3 on the server too.
- `restore` accepts the recovery code, unwraps the data key, and opens the backup. No credential store required, because the credential store is the thing that was lost.
- Key generation **refuses to mint a new key when a database file already exists** at the register path. Minting one silently is how an openable database becomes an unopenable one.
- Backups go to **two** destinations, one of them off the machine, and each destination's age is a device-health metric. Theft, fire and ransomware take the machine and its local backup directory together.

**The demonstration this makes possible** — destroy the database and the credential-store entry, restore from the off-machine backup using only the printed recovery code, and find every unsynced sale — is the one that was written down. It is now the one that can pass.

### The credential store, described accurately

"Key in the OS credential store, **not on the disk**" is false and was load-bearing. Credential stores are encrypted persistent databases on the disk. What they buy is real and narrower: the key is not in a plaintext file, so a stolen powered-off machine, a copied disk image, or a backup of the filesystem does not yield it.

What they do **not** buy: protection from anything running as the cashier's OS user. The application retrieves a generic password with no user presence and no prompt, so any process with that user's session can retrieve it too. Against the "stolen register" threat, the honest posture is:

| Layer | Effect |
|---|---|
| Credential store | cold-disk protection: powered off, imaged, or backed up, the key is not readable |
| Full-disk encryption | the same, for the rest of the machine, and required |
| A dedicated locked-down kiosk account, no software installation, Secure Boot | reduces "anything running as that user" from *anything* to *what you installed* |
| App-identity or hardware-backed wrapping, where the platform offers it | narrows retrieval to this signed application |

A powered-on, logged-in, unattended register is a machine whose database key is retrievable, and no keyring API changes that. Say so, and control it with the account baseline.

### Fiscal credentials on every register

Direct submission means each register holds a live credential that can file fiscal documents in the merchant's name. Nothing said what its scope is, so nothing could say what compromising one register costs.

> ⚠️ **OPEN — blocks 2.7.0.** Are JoFotara credentials scoped to a taxpayer, income source, store, or register, and what rotation and revocation operations does ISTD support? Default until answered: do not copy one taxpayer secret to every register and do not enable the live client; keep only versioned credential references in the register credential store and choose per-register credentials or server-side KMS custody after the scope is confirmed.
> Owner: 2.7.0. Source that settles it: authenticated JoFotara portal/API documentation or a written ISTD E-Invoicing Directorate answer.

If the answer is taxpayer-wide, the safer topology is a server-side credential with registers queueing through sync — which costs offline clearance and must therefore be a decision, not a default.

---

## 6b · Supply chain and release integrity

Three claims in this area were stronger than their mechanism, and one placement is a live exposure.

**Advisory scanning does not control a malicious dependency.** `cargo deny` and `pnpm audit` find *known* advisories, licences and disallowed sources. A freshly published, not-yet-advisory version of a transitive crate whose build script runs arbitrary code at compile time passes every gate green. What controls it is source review with a record:

- a checked-in audit ledger (`cargo-vet` or equivalent) that **fails on any unvetted crate version**, importing audits only from named trusted organisations;
- security-sensitive direct dependencies split out of grouped dependency bumps, so a money, crypto, database or fiscal dependency is never upgraded inside a batch of twelve;
- release builds from the reviewed lockfile, with no network resolution step;
- every exception dated, reasoned, owned, and expiring.

**The SBOM inventories the checkout, not the shipped installer.** One document generated from the repository root cannot know which native libraries were packaged into the Windows bundle, the macOS app, or the Linux artifact — which is precisely the question asked during a vulnerability recall. Each platform job generates an SBOM from its own staged bundle after packaging, and each SBOM's digest is bound into that artifact's provenance. The existing repository-level document is fine, relabelled as a source dependency inventory.

**"Signed, reproducible from day one" is not true and should not be repeated.** Phase 0 closed unsigned; signing is a Phase-5 milestone; release builds run on mutable hosted runner images against live package repositories, and nothing compares two builds. Checksums prove that a download matches what was published, not that what was published came from the reviewed commit. The honest claim, and the work that earns it:

| Claim | Requires |
|---|---|
| "Signed" | Phase 5 signing, per platform, verified by installing without a warning |
| "Traceable" | a provenance statement per artifact: artifact digest, source commit, workflow revision, toolchain and runner identity, lockfile hashes, and the build invocation — plus a documented command that verifies it |
| "Reproducible" | two clean builds per platform producing equal payload digests. Until that has been observed, the word is not used |

**The signing key must not be on a step that compiles third-party code.** The release workflow currently exposes the updater signing key and its password to the same step that builds the frontend and the Rust binary — a step that executes third-party build scripts and proc macros by design. Any one of them can read the environment. The requirement is a split: an unsigned build job that touches the network and compiles dependencies, then a signing step that receives only artifact digests and holds the key, with no checkout, no dependency installation and no compilation. Implementing the split is a workflow change and gets its own reviewed edit.

---

## 7 · Licensing

Blueprint §7. Ed25519-signed entitlement files, each carrying a `kid`, the org, the licensed registers or stores, the features, and its validity window.

> **A store must not die because a licence server did.** There is no code path that locks a register during an open shift (E.57). Prove it with a test that opens a shift, expires the licence, and asserts selling continues to shift close.

**"Degrade to read-only, never lock out" needs restating, because for a point of sale read-only *is* the lockout.** A register that cannot complete a sale is a closed shop, and calling that a graceful degradation only changes the name. Worse, the mechanism composes badly with the vendor being a single person: entitlements that need periodic online validation, signed by a key only the vendor holds, mean an unavailable vendor eventually stops every register at every merchant at roughly the same time.

> **Decision recorded for sign-off:** expiry blocks **new register enrollment and updates**. It does not block a sale on a register that was entitled when it last synced. Entitlements are issued dated to the end of the paid term plus a stated buffer, so continuing to trade requires no online validation at all. Non-payment is collected the way every other B2B vendor collects it — by asking — not by stopping a merchant's till.
> The alternative, read-only on expiry, is a stronger commercial lever and a materially worse product; overrule this in review if the commercial model demands it, and then state the grace period as a number in the contract rather than as the word "generous".

The grace period is a merchant-facing term, so it is a number in `merchant-decisions.md`, not an adjective here.

**Tests:** `tampered_entitlement_is_rejected` · `an_entitlement_for_another_org_is_rejected` · `an_unknown_kid_is_rejected` · `licence_expiry_never_prevents_a_sale_on_an_entitled_register` · `expiry_blocks_enrollment_and_updates` · `grace_period_survives_a_long_outage`.

---

## 8 · Record retention

Master plan B.6. Sale documents and Z reports must be kept for the statutory period.

This reinforces the architecture rather than complicating it: **financial facts are never hard-deleted**, so retention is a question of storage and archival, not of deletion policy. What it does need is a clock per class of record and a list of what a "sale document" actually consists of — neither of which "regionally multi-year" supplies, and a configurable period with no floor can expire evidence early.

> ⚠️ **OPEN — blocks 5.3.4.** What is the statutory retention period for each class of record — sales-tax records, income-tax records, electronic fiscal artifacts, personal data — from which trigger date, and what extends it during a dispute or an audit? Default until answered: retain every financial and fiscal artifact for **ten years** from the end of the tax period, treat the configured period as a floor rather than a ceiling, and hold indefinitely on dispute.
> Owner: `5.3.4`. Source that settles it: the merchant's accountant on the tax clocks, and counsel on the dispute hold.

The retention job enforces:

| Data | Policy |
|---|---|
| Sale documents, Z reports, audit log, stock ledger, cash movements | **never deleted.** Archived after the statutory period if storage requires |
| The fiscal artifact set — the receipt bytes as handed over, the submitted XML and its hash, the raw ISTD response, the QR, the UUID/ICV/invoice-number mapping, and credit-note links | retained with the sale, as one inseparable set. An inspection asks for the document, not for the parts of it that were convenient to keep |
| Consent events and the notices they reference | retained for as long as the processing they authorise can be questioned |
| Customer PII | anonymised after the configured inactivity period, except the buyer identity on an issued tax invoice (§2) |
| Backups | rotated per the configured schedule, on both destinations, with the wrapped key envelope retained as long as the backup it opens |
| Telemetry, diagnostic bundles, dead-letter bodies | capped and rotated on a short clock — they are the copies most likely to hold something nobody inventoried |

**Tests:** `retention_job_never_deletes_a_financial_fact` — the single most important assertion in this section · `a_legal_hold_overrides_every_configured_period` · `the_fiscal_artifact_set_is_retained_whole`.

---

## 8a · Incident response, disclosure, and the patch SLA

A breach clock measured in hours makes an incident procedure a legal instrument rather than an operational nicety, and the plan had none — no intake channel, no severity definitions, no acknowledgement target, no patch commitment, and no way for a researcher to report anything.

| Element | Position |
|---|---|
| **Security contact** | a published address that reaches a human, in `SECURITY.md` and in the compliance story |
| **Disclosure policy** | what is in scope, what a reporter may do, coordinated disclosure with a stated timeline, and a commitment not to pursue good-faith research |
| **Severity and acknowledgement** | critical acknowledged within **4 hours** during trading hours; high within one working day; the definitions written down so severity is not negotiated during the incident |
| **Patch SLA** | a number, not "soon": critical fixed and released within **7 days** of confirmation, high within **30**. It goes in the merchant agreement, because it is the merchant's exposure and not the vendor's convenience |
| **Runbook** | the containment sequence in §2, executed against the two clocks, with the decision owner named |
| **Evidence** | an incident log, and a snapshot taken before remediation |
| **Support access** | every read of a merchant's data by the vendor is permissioned, rate-limited, audited into the merchant's own trail, and bounded by a retention period for what was retrieved. A processor that cannot say who accessed what is a processor that cannot answer its controller |
| **After** | the finding becomes a numbered case in [`test-catalog.md`](test-catalog.md) with a test, an accepted risk, or an out-of-scope |

Drills produce a record — the incident tabletop, the restore drills, the key-rotation drill — and a drill nobody wrote down did not happen. The record lives in a dated file per run, not in memory.

---

## 9 · Threat model, briefly

The threats worth designing against, in the order they actually occur:

| Threat | Control |
|---|---|
| **Cashier discounts for friends** | override report by user, reason strings, margin floor, audit chain (E.33); no command sets a price without `price.override` |
| **Cashier pockets cash, opens drawer without a sale** | software-commanded drawer opens counted on X/Z, no-sale report, blind close, over/short trend (E.35) |
| **Cashier refunds to their own card** | refunds route to the *original* card; cash-for-card is a separate capability with a threshold |
| **Serial refund abuse across stores** | connected remaining-refundable check; offline window disclosed (E.31) |
| **Manager approval reused or re-aimed** | one-use `ApprovalHandle` bound to actor, entity and amount, consumed in the effect's transaction (§5) |
| **Terminated employee returns** | deactivation syncs down; signed offline authorization lease that fails closed on privileged capabilities (E.55) |
| **PIN brute force on a shared device** | Argon2id floors, persisted attempt limiting and lockout, longer manager credentials (§5) |
| **Stolen register** | SQLCipher at rest and full-disk encryption give cold-disk protection; a powered-on, logged-in register's key is retrievable by anything running as that user, so the control is the kiosk account baseline (§6a) |
| **Tampered ledger** | hash chain + verifier detects modification, reordering and internal deletion; a determined holder of the database key is caught only by an anchor outside the database — a Z, a verified backup, or the server (§4) |
| **Stolen backup file** | backups carry SQLCipher encryption; the wrapped envelope beside them is only openable with the merchant's recovery code (§6a) |
| **Cloned register image** | the device token is bound to a non-exportable key, so the clone fails its first request; same-UUID-different-payload pushes are rejected and alarmed |
| **Man in the middle on sync** | TLS with validation; pinning considered |
| **Malicious update** | signed manifests; an unsigned update must not install; signing on a step that compiles no third-party code (§6b) |
| **Malicious dependency** | source-review ledger that fails on unvetted versions; advisories alone do not cover this (§6b) |
| **Card data theft** | it is not there to steal, and a driver that returns a PAN is refused rather than trusted |
| **Cross-tenant read on the shared server** | `org_id` on every table, composite foreign keys, forced row-level security, two-org adversarial tests (§5b) |
| **Compromised back-office session** | OIDC with phishing-resistant MFA on privileged capabilities, short revocable sessions, deny-by-default routes (§5a) |
| **Vendor support access misused** | permissioned, rate-limited, and audited into the merchant's own trail (§8a) |

Note the ordering. The first seven are **staff**, and they are what actually costs a merchant money. That is why fraud reporting, blind close, and the audit chain sit in Phases 1–2 rather than being deferred to a security phase — and why the master plan's insistence on them is one of its best instincts.

The rows after them are the vendor's own attack surface, and they were absent from the first version of this table. They belong here, below the staff threats rather than above them, because a shared server, a fleet updater and a support console are all things this product acquires in Phases 3–5 — and each one turns a single vendor mistake into every merchant's incident.
