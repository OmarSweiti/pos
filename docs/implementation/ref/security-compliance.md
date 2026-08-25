# Security and compliance

Blueprint §7, master plan B.3 / B.4, plus the gaps this plan closes: the audit-chain specification (G-7), the permission-enforcement mechanism (G-6), and proven PII scrubbing (G-8).

> ⚠️ These are engineering positions, not legal advice. Before launch, validate PDPL with a lawyer, GST and record retention with the merchant's accountant, and PCI scope with a QSA. **Never claim a validation you have not completed.**

---

## 1 · The posture in one page

| Control | Position |
|---|---|
| Local database | SQLCipher; key in the OS credential store via `keyring`, **never a file**, never an env var in release |
| Cashier auth | PINs hashed with **Argon2id**; auto-lock on idle; manager escalation for voids, refunds over threshold, overrides, drawer opens |
| Authorisation | **enforced in Rust command handlers** via a proof-carrying token — hiding a button is UX, the check is security |
| Audit | **hash-chained**, append-only, covering logins, voids, refunds, overrides, drawer events, settings changes, sync anomalies |
| Card data | **PAN, track and CVV never exist in this process, database, or logs.** Only `psp_ref`, masked PAN, and scheme are stored |
| Transport | TLS everywhere, certificate validation on; pinning considered for the sync API |
| Updates | signed — Windows Authenticode, macOS notarization, Tauri signed manifests. An unsigned or tampered update must not install |
| Personal data | PDPL-grade: minimal collection, recorded consent, export, erasure as anonymisation, documented retention, 24-hour breach clock |
| Licensing | Ed25519-signed entitlements, generous offline grace, **degrade to read-only, never lock out** |
| Secrets | no card data, PINs, tokens or keys in logs; Sentry events scrubbed; `.env` never committed |

---

## 2 · PDPL — Personal Data Protection Law No. 24 of 2023

Jordan's first comprehensive data-protection law. Published 17 Sep 2023, in force 17 Mar 2024, grace period ended Mar 2025, and it applies **retroactively** to data collected before it existed. GDPR-like in structure.

As of Aug 2026 the national authority is still standing up and the electronic controller/processor registry is not yet activated (manual registration with the Personal Data Protection Directorate is the interim path). **Build to the law, not to the enforcement lag.**

### What it requires, and what this product does about it

| Requirement | Implementation | Where |
|---|---|---|
| Explicit informed consent | consent is a **record**, not a boolean: kind, **wording version**, timestamp, who captured it, which channel | `consent` table; microstep 3.4.2 |
| Purpose limitation | consent kinds are distinct — `loyalty_terms`, `marketing`, `data_processing` — and each feature checks its own | `prop`-tested per feature |
| Minimal collection | name, phone, email, consent flags. **No ID numbers** unless a real requirement emerges | schema `0010` |
| Right of access | *export my data*: profile + consents + purchase history + loyalty ledger, one file | 3.4.5 |
| Right of correction | back-office edit with a full audit trail | 3.6 |
| Right of erasure | **anonymisation** — null the person, keep the immutable financial facts against the anonymised id | 3.4.4 |
| Marketing consent honoured | any messaging feature and any back-office export filters on it | 3.4.2 |
| Restricted cross-border transfer | sync over TLS with server-side access control; hosting region is a merchant decision | 3.1.6 |
| **24-hour breach notification** | SQLCipher at rest + keyring + **no PII in logs**, all three tested; plus a written runbook | 1.6.8, 5.3.2 |
| Documented retention | retention periods in settings, enforced by a job that **can never delete a financial fact** | 5.3.4 |

### Why the consent record shape matters

A boolean says *"they agreed."* A regulator asks *"to what, exactly, and when?"* Storing the **wording version** means the answer is retrievable years later, after the terms have been rewritten twice. This costs one column and is the difference between a defensible position and a story.

### Erasure without destroying the books

Deleting a customer must not delete their sales — those are tax records with a statutory retention period, and the merchant is legally required to keep them. Erasure is therefore **anonymisation**: PII nulled, `is_anonymized` set, ledger and sale rows preserved against the now-anonymous id.

**Tests:** `anonymize_nulls_pii_and_keeps_ledger_rows` · `sales_survive_anonymization_with_totals_intact` · `anonymized_customer_is_not_findable_by_phone`.

---

## 3 · PCI DSS — and the claim you may actually make

**The architecture:** semi-integrated, certified terminals only. The amount and a reference go to the terminal; a result and a reference come back. Card data is captured and encrypted by the terminal and travels to the PSP without passing through this application.

**What that buys:** the cardholder data environment shrinks to the terminal, so the merchant completes a short self-assessment questionnaire instead of a full audit.

**The nuance the master plan understates — and it changes the claim:**

> **SAQ P2PE applies only if the terminal is part of a PCI-listed, validated P2PE solution.** "Semi-integrated" and "P2PE-validated" are different properties, and a terminal can be the first without being the second. If the acquirer supplies an internet-connected terminal that is not on the PCI SSC's validated P2PE list, the merchant lands on **SAQ B-IP or SAQ C** — substantially longer, pulling the store network and supporting infrastructure into scope.

**The concrete action** (Phase 2, microstep 2.1.1): when evaluating each Jordanian acquirer, ask for the candidate terminal's **PCI P2PE listing number** and record it in `merchant-decisions.md`. It costs one question and determines which questionnaire the merchant fills in.

The engineering does not change either way. The claim does. **Determine the SAQ with a QSA (milestone 5.3.3) and never claim one before it is completed.**

### Non-negotiables restated

1. Store `psp_ref` on every card tender — reconciliation and refunds both depend on it.
2. Store masked PAN and scheme **for the receipt only**. Nothing else from the card, ever.
3. Treat a timeout as **unknown** → status-query before any retry, or you will double-charge.
4. Support partial approval and split tender from day one; bolting them on later deforms checkout.
5. Card refunds go through the PSP against the original reference, never as a fresh charge.

**Tests:** `card_tender_persists_only_the_three_allowed_fields` · `full_pan_never_reaches_the_database` — feed a driver response containing a full PAN and assert its absence everywhere afterwards.

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

### The chain

```
hash₀ = BLAKE3(GENESIS ‖ canonical_bytes(entry₀))
hashₙ = BLAKE3(hashₙ₋₁ ‖ canonical_bytes(entryₙ))
GENESIS = [0u8; 32]
```

BLAKE3 rather than SHA-256: faster on the low-end CPUs registers actually run, and 32 bytes either way.

### Coverage

Logins and logouts · user switches · **every void** · **every refund** · **every price override with its reason** · every discount above the cashier cap · drawer opens including no-sale · cash movements · shift open, close, and force-close · Z generation · settings changes · PIN resets · training-mode toggles · sync anomalies · fiscal rejections · audit-chain breaks themselves.

### Behaviour on a break

**The register does not stop selling.** It raises an alarm, records the break with its sequence number, and surfaces it in back-office device health.

A tamper-evidence mechanism that halts trade converts a forensic signal into an outage — and the most likely cause of a break is a bug in your own serialization, not a thief with a hex editor.

### The verifier

`crates/pos-db/src/bin/verify-audit.rs` (microstep 5.4.4) — a CLI that walks a register's chain and reports the first break. The forensic tool you hope never to need and cannot build under pressure.

**Tests:** `prop_chain_detects_any_single_entry_mutation` · `prop_chain_detects_deletion` · `prop_chain_detects_reordering` · `chain_survives_process_restart`.

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

### The exhaustiveness test

Every IPC command registers `(name, capability, audited, escalatable)`. `ipc_commands_all_declare_a_capability` walks `tauri::generate_handler!`'s list and fails on any command absent from the registry.

**Verify it works** by adding a command without a registry entry, watching CI go red, and reverting. A guard nobody has seen fail is a guard nobody should trust.

### Escalation

- The approver's id is a **different column** from the operator's.
- A setting can require them to differ (`ban_self_approval`, default on) — E.52.
- Approval tokens name the capability they authorise, so an approval for a refund cannot be replayed as an approval for a price override.
- Offline registers honour a **max-offline-auth window** for permission changes (E.55) — a real limit of offline-first, and one to disclose rather than hide.

---

## 6 · Secrets and PII in logs (gap G-8)

PDPL's 24-hour breach clock makes "no PII in logs" a legal position. A legal position needs a test.

### The scrubbing layer

A `tracing` layer that redacts, at any nesting depth, fields named: `pin`, `pin_hash`, `pan`, `card_number`, `cvv`, `track`, `phone`, `email`, `customer_name`, `buyer_name`, `secret_key`, `client_id`, `db_key`, `token`, `password`, `entitlement`.

It sits in front of **every** sink: stdout, file, and Sentry.

### The tests that make it a position rather than an intention

| Test | Asserts |
|---|---|
| `scrubber_redacts_every_known_pii_field` | each field name, individually |
| `scrubber_redacts_nested_json` | nesting does not evade it |
| `no_pii_in_a_full_sale_trace` | run a complete sale with a customer attached, capture every log line, assert the fixture's phone and name are absent |
| `no_pii_in_a_captured_panic` | the same for a panic payload reaching Sentry |
| `credentials_never_logged` | JoFotara `Secret-Key` and `Client-Id` |
| `full_pan_never_reaches_the_database` | a driver response containing a full PAN leaves no trace |

### Secrets hygiene

- `.env` is git-ignored; only `.env.example` is committed.
- The SQLCipher key lives in the OS credential store. `POS_DB_KEY` works in debug and is **refused in release** with a named error (microstep 1.8.5).
- JoFotara credentials live in the keyring; the database stores only a pointer and a four-character hint for the diagnostics screen.
- Updater private keys are in GitHub secrets, never the repository. Verified by `git log -p | rg 'PRIVATE KEY'` and a pre-commit hook.
- Cargo-deny, the reviewed JavaScript licence-metadata gate, and `pnpm audit` run in CI; accepted
  advisories require a dated, reasoned exception, and distribution still requires a real notice audit.

---

## 7 · Licensing

Blueprint §7. Ed25519-signed entitlement files; periodic online validation; a **generous offline grace period**; on expiry, **degrade to read-only — never lock out**.

> **A store must not die because a licence server did.** There is no code path that locks a register during an open shift (E.57). Prove it with a test that opens a shift, expires the licence, and asserts selling continues to shift close.

**Tests:** `tampered_entitlement_is_rejected` · `expired_licence_degrades_read_only_never_mid_day` · `grace_period_survives_a_long_outage`.

---

## 8 · Record retention

Master plan B.6. Sale documents and Z reports must be kept for the statutory period — regionally multi-year; confirm with the merchant's accountant (merchant decision #12).

This reinforces the architecture rather than complicating it: **financial facts are never hard-deleted**, so retention is a question of storage and archival, not of deletion policy.

The retention job enforces:

| Data | Policy |
|---|---|
| Sale documents, Z reports, audit log, stock ledger | **never deleted.** Archived after the statutory period if storage requires |
| Customer PII | anonymised after the configured inactivity period |
| Backups | rotated per the configured schedule |
| Telemetry | capped, rotated |

**Test:** `retention_job_never_deletes_a_financial_fact` — the single most important assertion in this section.

---

## 9 · Threat model, briefly

The threats worth designing against, in the order they actually occur:

| Threat | Control |
|---|---|
| **Cashier discounts for friends** | override report by user, reason strings, margin floor, audit chain (E.33) |
| **Cashier pockets cash, opens drawer without a sale** | drawer events counted on X/Z, no-sale report, blind close, over/short trend (E.35) |
| **Cashier refunds to their own card** | refunds route to the *original* card; cash-for-card is a separate capability with a threshold |
| **Serial refund abuse across stores** | connected remaining-refundable check; offline window disclosed (E.31) |
| **Terminated employee returns** | deactivation syncs down; offline auth window (E.55) |
| **Stolen register** | SQLCipher at rest; key in the OS credential store, not on the disk |
| **Tampered ledger** | hash chain + verifier |
| **Stolen backup file** | backups carry the same SQLCipher encryption |
| **Man in the middle on sync** | TLS with validation; pinning considered |
| **Malicious update** | signed manifests; an unsigned update must not install |
| **Card data theft** | it is not there to steal |

Note the ordering. The first four are **staff**, and they are what actually costs a merchant money. That is why fraud reporting, blind close, and the audit chain sit in Phases 1–2 rather than being deferred to a security phase — and why the master plan's insistence on them is one of its best instincts.
