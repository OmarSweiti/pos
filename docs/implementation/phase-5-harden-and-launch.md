# Phase 5 — Harden and launch

> **Exit:** the product can be sold to someone who is not you, with a signed compliance story you could defend to an auditor, a QSA, and a tax advisor in the same week.

**Effort:** 9–13 weeks, and the fiscal milestone (5.2) is gated on a merchant, not on you.
**Scope:** blueprint §10 hardening, PDPL registration walkthrough, PCI SAQ with a QSA, **Fiscal Certification**, restore drills, the update service, packaging and onboarding, and **milestone 5.0 — commercial and legal readiness**, without which "the product can be sold to someone who is not you" is not a claim anyone can make.

Phases 0–4 built a product. This phase turns it into something a merchant can buy and a regulator can inspect. Several milestones cannot be completed alone — they require a real merchant, a real accountant, a QSA, and counsel. **Start recruiting and contracting at Phase 1 for the long-lead items and at Phase 3 for the merchants**, per [`00-master-plan.md`](00-master-plan.md) §6a.

**Some of this milestone has already happened.** The Phase-4 pre-pilot gate (4.9.0) pulls fiscal certification, the PDPL determination, the breach runbook, the SAQ determination, the retention matrix and the independent security assessment **in front of the pilot**, because three real shops with real cards may not trade behind them. What remains here is completing, re-testing at launch scope, and writing down.

---

## Milestone 5.0 — Commercial and legal readiness

*This product computes a merchant's tax filings, holds their customers' personal data as a processor, and moves their money — and the plan contained no legal entity, no processing agreement, no terms, no liability position, no support commitment, and no path from "merchant pays" to "register is entitled". The Phase-5 exit gate is "it can be sold to someone who is not you"; every item below is load-bearing for that sentence, and several of them block other milestones outright.*

### 5.0.1 — The legal entity
The root dependency. Apple Developer ID and Windows Authenticode are issued to an identified organisation; an acquirer relationship needs a business; ISTD credentials belong to a registered taxpayer; a processing agreement needs two parties. Ordered in Phase 1 (§6a) precisely because 5.5.1 and 2.1.1 stall without it.
**Done when:** the entity exists and is the counterparty on every agreement below.

### 5.0.2 — The processing agreement and the sub-processor list
**Files:** `docs/legal/dpa.md` (new)
Use the interim merchant-controller/vendor-processor model only to draft this agreement; the dated 3.4.1 determination confirms or replaces it before signing. Write down the confirmed allocation per data flow, the sub-processors by name and region — hosting, telemetry, any email or SMS delivery — the transfer basis from 3.1.6, the security measures, the assistance obligations, the audit rights, and what happens to the data at termination. Support access is inside this agreement: permissioned, rate-limited, audited into the merchant's own trail, and bounded by a retention period for whatever was retrieved. *A processor that cannot say who accessed what is a processor that cannot answer its controller.*
**Done when:** a merchant's lawyer can read it, and the sub-processor list matches what the deployment actually calls.

### 5.0.3 — Terms, warranty, liability, and the support commitment
**Files:** `docs/legal/terms.md` (new), `docs/legal/support.md` (new)
Terms of service; the warranty and liability position; the patch SLA as a **number** — critical fixed and released within 7 days of confirmation, high within 30, per [`ref/security-compliance.md`](ref/security-compliance.md) §8a; and the support model: intake channel, hours, severity definitions, target response times, escalation, and what the merchant does meanwhile for each of the top five failures — printer, terminal, no sync, licence, register will not start. An implied 24/7 is an implied 24/7 that breaks on the first Saturday.
**Done when:** every one of those is a written number or a written sentence, and the on-call expectation from 3.10.4 agrees with it.

### 5.0.4 — The unit of sale, and the path from payment to entitlement
**Depends on:** decided **before** microstep 3.8.1, because it determines what an entitlement asserts.
**Files:** `docs/legal/commercial.md` (new)
Per register, per store, or subscription; the price; the term; the renewal; the grace buffer as a number; and what happens commercially at non-payment — which, per §7 of the security reference, is **asking**, not stopping a merchant's till. Then the operational path: merchant pays → back-office issuance (3.8.2) → the register verifies. The onboarding wizard's step list includes it (5.6.1).
**Done when:** a person who is not you can take money from a merchant and end with an entitled register, following written steps.

---

## Milestone 5.1 — Load, soak, and restore

### 5.1.1 — Volume soak
**Files:** `crates/pos-db/tests/soak.rs` (new)
Run the year-one dataset generated at **2.9.6** — one year of a busy minimarket, ~250 000 sales, ~800 000 lines, ~1 200 000 stock events, ~300 000 audit entries on **one** register database. The dataset is generated two phases earlier on purpose: if the answer is an index, an archival strategy or a schema change, discovering it here means discovering it after the schema and the reports have shipped.
**Check:** cart recompute still < 16 ms · search still < 50 ms · cold start still < 3 s · outbox and database size within the byte budgets from 3.2.3 · WAL growth acceptable · `VACUUM` behaviour understood.
**Done when:** every Phase-1 and Phase-2 budget still holds at year-one volume, measured on the reference register and gated by `just bench-gate`. Extend the horizon to the merchant's selected retention period rather than stopping at one year — the retention matrix in 5.3.4 says how long the data actually has to live.

### 5.1.2 — Sync soak
**Files:** `crates/pos-sync/tests/soak.rs`
Ten registers pushing concurrently for a simulated month. Watch server latency, PostgreSQL bloat, `change_seq` behaviour, and the growth of the per-org partitions under row-level security.

### 5.1.3 — The restore drills
**Files:** `docs/runbooks/restore.md` (new), `docs/drills/` (the records)
Not tests — **drills**, performed on real hardware, timed, and written down. Three of them, and the third is the one the old design could not pass:

1. **Data loss.** Kill a register mid-trading with unsynced sales in its outbox. Wipe the machine. Reinstall, re-provision, restore from backup. Confirm every unsynced sale survived and drained. **Time it** — that number is the merchant's downtime promise.
2. **Keychain loss** (E.4). Wipe the OS credential store, confirm the recovery screen appears, restore, confirm no data loss.
3. **Recovery code only** (E.4d). Destroy the database **and** the credential-store entry, on a second machine, and open the off-machine backup with nothing but the printed recovery code. The old design encrypted the backup with the same key that had just been destroyed, so this could only ever "pass" in a debug build where `POS_DB_KEY` supplied the key a release build ignores.

Plus a **key-rotation drill**: the database key, the entitlement key and the fiscal credentials each rotate without losing a queued document.
**Done when:** every drill is documented in a dated file under `docs/drills/`, timed, and repeated successfully by someone who did not write the code.

### 5.1.4 — Backup verification job
**Files:** `apps/terminal/src-tauri/src/backup_verify.rs` (new)
A scheduled job that opens the newest backup, verifies it, and reports its age to device health. **An unverified backup is a rumour.**
**Tests:** `verify_job_detects_a_corrupted_backup`

---

## Milestone 5.2 — Fiscal Certification

*The only place the real ISTD endpoint is ever contacted. Requires a merchant. The full checklist is [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §7; it is reproduced here as the milestone's gate because it is the highest-stakes sequence in the project.*

> ⚠️ **Every submission in this milestone is a real fiscal document against the merchant's real tax record.** Do not begin without their informed consent **in writing**.

> **This milestone runs before the Phase-4 pilot for any store that will issue fiscal documents through this product** — it is row 2 of the pre-pilot gate (4.9.0). It is written here because it belongs with the compliance work; it is *executed* earlier, with one recruited merchant, because a week of three-store trading on an uncertified pipeline is thousands of documents nobody can defend.

**Steps 5.2.1 and 5.2.2 no longer exist here.** Obtaining the official package and diffing the reconstruction are microstep **2.7.0**, a precondition of building the fiscal pipeline at all. This milestone validates a pinned implementation against the credentialed service; it does not discover the contract for the first time, with a merchant watching. Numbers 5.2.3 to 5.2.11 keep their meanings, so nothing that cites them moves.

| # | Action | Gate |
|---|---|---|
| **prerequisite** | Re-read the `2.7.0` manifest, the closed `⚠️ OPEN` evidence, and all five reviewed goldens | Artifact hashes match; no provisional table drives the build |
| 5.2.3 | Provision production credentials using the topology `2.7.0` selected; store only the versioned reference in the approved credential store or KMS | Secret value never enters the database, a log, a diagnostic bundle or a fixture |
| 5.2.4 | Confirm live UUID, response and duplicate-recovery behaviour against the pinned contract, **without changing an already-issued identity** | Observed behaviour matches the manifest, or certification stops and the erratum is recorded |
| 5.2.5 | Execute the written outage procedure selected in the fiscal reference §2.1 — the exact customer artifact, and the reconciliation path | Every step produces dated evidence; no policy is invented during the drill |
| 5.2.6 | Submit golden document 1 as a **live, low-value invoice**; verify the QR with the Sanad app | QR verifies |
| 5.2.7 | Immediately credit-note it through golden 5's path, carrying the original's buyer and line facts; confirm both appear in the merchant's ISTD portal | Both visible, linked, netting to zero |
| 5.2.8 | Repeat for goldens 2 (discounted), 3 (multi-rate), 4 (weighed) | All clear |
| 5.2.9 | Run the reconciliation report | Zero unmatched on both sides |
| 5.2.10 | **Two-register** kill-the-network drill: queue on both registers offline, reconnect, allocate ICV centrally, drain in assigned order | Unique scope-correct ICVs; no new UUID or ICV on replay; selling never stopped |
| 5.2.11 | Environment guard both directions (E.28) | Both refuse |

**Done when:** all nine are checked, dated, and signed, with the `2.7.0` prerequisite verified first. Until then the product's fiscal claim is *"passes our conformance harness against the pinned official specification,"* which is honest and is **not** *"JoFotara compliant."*

> The offline-clearance question the old step 5.2.5 asked a private advisor is now an `⚠️ OPEN` item owned by `2.7.0` and answered by ISTD, not by an advisor: a tax advisor is the right authority for product classification and merchant-specific elections, and the wrong authority for a protocol fact like the issuance event or an outage grace period. The merchant's advisor still signs the merchant's own position, and it is recorded in [`ref/merchant-decisions.md`](ref/merchant-decisions.md) **section F, row 6.7** — there is no §11.

---

## Milestone 5.3 — Compliance walkthroughs

### 5.3.1 — PDPL walkthrough, and the registration itself
**Files:** `docs/compliance/pdpl.md` (new)
Demonstrate, live, to the merchant and ideally to a lawyer:
- the versioned privacy notice presented before processing starts;
- consent captured at the register against an **immutable notice**, and the exact historical wording rendered years later;
- consent withdrawn and honoured, including the case where a stale grant arrives afterwards;
- **export my data** producing a complete file, and objection, restriction, portability and complaint each having a route and a timed case;
- **erasure as anonymisation** — the person gone, the financial facts intact, and a canary identity absent from every store in the PII inventory;
- retention periods configured and documented, with the legal hold that no job overrides;
- both breach clocks exercised (5.3.2);
- the no-PII-in-logs test, run in front of them;
- **the electronic-register entry**, complete, with the controller, processor, DPO determination and the owner of the 15-day change updates.

That last bullet is new because the plan's premise was stale: it said the electronic controller/processor registry was not yet activated, and MoDEE now publishes it. Registration is therefore a dated determination with an owner, not a wait — and the determination itself is owed at `3.4.1`, before Phase 3 processes any customer PII.

> ⚠️ **OPEN — blocks 3.4.1.** For this deployment, which entity is controller, which is processor, who is a recipient, is a DPO required, and is the Personal Data Processing Register entry required and complete? Default until answered: the schema may migrate, but customer capture, consent collection and customer-PII sync remain disabled.
> Owner: 3.4.1. Source that settles it: the current MoDEE Personal Data Processing Register instructions and dated Jordanian counsel advice for the deployed roles.
**Done when:** each bullet is a screen recording plus a passing test, filed in `docs/compliance/`, with the registration evidence attached.

### 5.3.2 — Breach-notification runbook — **two** clocks and a containment sequence
**Files:** `docs/runbooks/breach.md` (new)
**Executed before the Phase-4 pilot** (pre-pilot gate row 5), because a breach arriving before the runbook exists is a breach nobody can answer.

Until counsel closes the OPEN item in [`ref/security-compliance.md`](ref/security-compliance.md) §2, the runbook uses the interim defaults of notice to affected individuals within **24 hours** and a separate report to the supervisory unit within **72 hours**, carrying the source, mechanism, affected population and what is known at filing time. These are drill clocks, not a claim that the statutory values or filing content have been validated.

Write down: who decides it is a breach; the discovery timestamp and both clocks tracked independently from it; how affected individuals are identified from the data; the Unit's filing channel and the required fields; the bilingual notification template; who at the merchant sends the individual notice; and the processor-to-controller escalation SLA, because on a shared service the vendor may notice first and the merchant is the controller.

And the containment sequence, which nothing wrote down: preserve evidence and snapshot before remediating · scope the affected tenants · halt the release and update channel if it is implicated · rotate credentials and keys · restore · then notify.
**Tests / evidence:** a timed tabletop producing the Unit's source/mechanism/affected-population record, filed in `docs/drills/`.

### 5.3.3 — PCI SAQ with a QSA
**Files:** `docs/compliance/pci.md` (new)
**Determined before the Phase-4 pilot** (pre-pilot gate row 6), because the answer changes the store network, not only the paperwork.

Bring the QSA: the semi-integrated architecture, the terminal's exact model and firmware with its **PTS and P2PE listing numbers** (or their absence), the acquirer's written responsibility matrix, the store network topology, the remote-support model, the "only three card fields are stored" test from Phase 2 (2.2.2), and the log-scrubbing test.

It is SAQ P2PE **only** if the terminal is part of a PCI-listed validated P2PE solution; otherwise SAQ B-IP or C. **The engineering and operating posture changes with the answer** — B-IP carries eligibility and network-isolation requirements, and C pulls network, configuration, patching, access control, monitoring, testing and policy evidence into scope. An earlier revision of [`ref/plan-validation.md`](ref/plan-validation.md) §4 said the engineering did not change; that is corrected in [`00-master-plan.md`](00-master-plan.md) §4a. A driver response carrying a full PAN is an **integration rejection**, not data to accept and discard.
**Done when:** the SAQ is completed, its annual revalidation is on a calendar, and it is **never claimed before it is completed**.

### 5.3.4 — Retention matrix, with clocks and a hold
**Files:** `docs/compliance/retention.md` (new), settings
"Regionally multi-year" supplies no clock, and a configurable period with no floor can expire evidence early. The matrix is in [`ref/security-compliance.md`](ref/security-compliance.md) §8 and is signed here with the accountant: a period **per class of record**, its trigger date, and what extends it.

Two things it must name that the old policy did not. First, the **fiscal artifact set** is retained whole — the receipt bytes as handed over, the submitted XML and its hash, the raw ISTD response, the QR, the UUID/ICV/invoice-number mapping and the credit-note links. An inspection asks for the document, not for the parts of it that were convenient to keep. Second, an **indefinite legal hold** that no job may override, for a dispute or an audit.
**Tests:** `retention_job_never_deletes_a_financial_fact` · `a_legal_hold_overrides_every_configured_period` · `the_fiscal_artifact_set_is_retained_whole`

> ⚠️ **OPEN — blocks 5.3.4.** What is the statutory retention period for each class of record — sales-tax records, income-tax records, electronic fiscal artifacts, personal data — from which trigger date, and what extends it during a dispute or an audit? Default until answered: retain every financial and fiscal artifact for **ten years** from the end of the tax period, treat the configured period as a floor rather than a ceiling, and hold indefinitely on dispute.
> Owner: `5.3.4`. Source that settles it: the merchant's accountant on the tax clocks, and counsel on the dispute hold.

---

## Milestone 5.4 — Security hardening

### 5.4.1 — Independent security assessment, then the launch-scope retest
**The first assessment happens before the Phase-4 pilot** (pre-pilot gate row 7). This microstep is the retest at launch scope, once the surface is complete.

Scope both times: the sync API, device tokens and proof of possession, the enrollment flow, **cross-tenant isolation on the shared server**, back-office authentication and session handling, the licence and entitlement mechanism, the update channel, and the local database at rest.

**External, not a self-review.** For a multi-tenant system holding customer, fiscal, licensing and fleet-update authority, a sole-author review of their own design cannot substitute for adversarial testing — and "external if budget allows" makes the cheapest option the default at exactly the moment it costs the most. It is a long-lead item ([`00-master-plan.md`](00-master-plan.md) §6a) precisely so the budget question is asked in Phase 4 rather than the week before launch.
**Done when:** findings are triaged, every critical and high is fixed or accepted **in writing** by the owner, and the fixes are retested.

### 5.4.2 — Dependency, supply chain, and provenance
**Files:** `.github/workflows/ci.yml`, `supply-chain/audits.toml` (new)
`cargo deny` (advisories, licences, bans and sources) · reviewed JavaScript licence metadata · `pnpm audit`. Fail on advisories unless an exception is explicit, reasoned, owned and **dated with an expiry**.

**Advisory scanning does not control a malicious dependency**, and the plan treated it as though it did. A freshly published, not-yet-advisory version of a transitive crate whose build script runs arbitrary code at compile time passes every one of those gates green — and Cargo build scripts and proc macros execute by design. What controls it is source review with a record:

- a checked-in audit ledger (`cargo-vet` or equivalent) that **fails on any unvetted crate version**, importing audits only from named trusted organisations;
- security-sensitive direct dependencies split out of grouped bumps, so a money, crypto, database, rendering or fiscal dependency is never upgraded inside a batch of twelve;
- release builds from the reviewed lockfile with no network resolution step;
- a source diff read for every upgrade of those dependencies.

**The SBOM inventories each shipped installer, not the checkout.** One document generated from the repository root cannot say which native libraries were packaged into the Windows bundle, the macOS app or the Linux artifact — which is exactly the question asked during a vulnerability recall. Each platform job generates an SBOM from its own staged bundle **after** packaging, and each SBOM's digest is bound into that artifact's provenance. The existing repository-level document stays, relabelled a *source dependency inventory*.

**And a provenance statement per artifact**: artifact digest, source commit, workflow revision, toolchain and runner identity, lockfile hashes, and the build invocation, with a documented command that verifies it. Checksums prove a download matches what was published; they say nothing about whether what was published came from the reviewed commit. The word "reproducible" is not used until two clean builds per platform have produced equal payload digests.
**Done when:** an unvetted crate version fails CI, each platform publishes its own SBOM, and a third party can verify an installer back to a commit with one command.

### 5.4.3 — Secrets audit
```bash
git log -p | rg -i 'private key|secret|BEGIN.*KEY|client_id|password'
```
Plus a pre-commit hook (`gitleaks` or equivalent).
**Done when:** the repository history is clean and new secrets cannot be committed.

### 5.4.4 — Audit-chain verification against the server anchor
**Files:** `crates/pos-db/src/bin/verify-audit.rs`
The CLI itself is **not** new here: it is built at microstep 1.6.6b, because the Phase-1 exit gate demonstrates tamper detection and a claim whose tool arrives four phases later is a claim nobody can check. What this microstep adds is the half that only exists once there is a server: `--anchor` accepts the last accepted `audit_checkpoint` (3.2.4) so the verifier can detect a **tail deletion** — the one tamper a local hash chain cannot see, and the one that removes exactly the newest drawer opens, refunds and overrides.

The residual risk is stated rather than engineered away: entries above the last synced checkpoint are unanchored, and the window is however long the register has been offline.
**Tests:** `an_unanchored_tail_is_reported_as_unverified_not_as_intact`
**Re-runs:** 3.2.4's `tail_deletion_is_detected_against_the_server_checkpoint`; Phase 5 consumes that test as release evidence and does not create a competing owner for it.

---

## Milestone 5.5 — Release engineering

### 5.5.0 — The updater signing keypair
**Files:** `apps/terminal/src-tauri/tauri.conf.json`, key custody per [`ref/security-compliance.md`](ref/security-compliance.md) §6a
Phase 0 closed with this item open, and it then belonged to no phase and no gate — while `release.yml` hard-fails without the public key in `tauri.conf.json`, so no release could be built at all. It is re-homed here, first in the milestone, because everything below it depends on it.

Generate the keypair on the offline signing host, not on a developer laptop and not in CI. The public key goes in `tauri.conf.json`; the private key never leaves its host. **Two independently encrypted recovery copies**, stored apart: losing this key means every installed register needs a site visit, which is the worst single-key outcome in the product.
**Done when:** `release.yml`'s guard passes on a test tag, and the recovery copies exist in two places with two custodians.

### 5.5.1 — Code signing and notarization, on a step that compiles nothing
**Files:** `.github/workflows/release.yml`
Windows Authenticode certificate; Apple Developer ID plus notarization. An unsigned or tampered update must not install.

**The signing key must not be on a step that compiles third-party code.** The release workflow currently exposes the updater signing key and its password as `env` on the same step that builds the frontend and the Rust binary — a step that executes third-party build scripts and proc macros by design, any one of which can read the environment. Split it: an **unsigned build job** that touches the network and compiles dependencies, then a **signing step that receives only artifact digests**, holds the key, and performs no checkout, no dependency installation and no compilation. Keep the updater and OS signing keys off the ordinary developer laptop.

The workflow edit is a reviewed change of its own — `scripts/check-branch-workflow-policy.rb` requires it — and the requirement is recorded in [`ref/security-compliance.md`](ref/security-compliance.md) §6b.
**Done when:** installers on all three platforms install without a security warning, and no signing secret appears in the environment of any step that runs third-party code.

### 5.5.2 — The update service
**Files:** `apps/server/src/updates/` (new), `.github/workflows/release.yml`, `apps/terminal/src-tauri/tauri.conf.json`
The plan said "5% → 50% → 100%, with one-click rollback" and specified no mechanism at all: there was no updater endpoint, no manifest generation, and no cohort assignment anywhere, while the exit gate required a staged rollout proven end to end. A static release asset cannot answer differently per register, which is what a percentage rollout means.

Three parts:

- **5.5.2a — the service.** A manifest endpoint, manifests generated by `release.yml` at publish time, and cohort assignment by `register_id` so 5% is a stable set rather than a coin flip per check. The webview gets **no** updater plugin permission; the only path is the three Rust-owned commands `update_check`, `update_download` and `update_apply`, and a machine-audited check rejects `updater:default`, `updater:allow-install` and `updater:allow-download-and-install` if one is ever added. Tauri's default permission puts install directly in reach of frontend JavaScript, and a rule enforced in Rust that the webview can route around is not enforced.
- **5.5.2b — what "rollback" means.** It cannot mean an older binary against a migrated database: migrations are forward-only and the runtime refuses with `SchemaTooNew`, which is correct behaviour and the reason the promise was empty. **Fleet-level rollback is halting the rollout.** Register-level recovery is: close the shift, retain the previous bundle, take an encrypted **pre-migration snapshot**, run migration and startup smoke tests before reopening, and then either keep the new version or restore *both* the bundle and the snapshot before any new fact is written. A comparator that accepts an arbitrary lower version is forbidden — it replays older signed vulnerable builds.
- **5.5.2c — the register that is four versions behind.** Apply the whole chain in one restart, behind a determinate progress screen, with the budget from 5.5.3.

**The product rule stands: never apply an update while a shift is open** (E.56). Download in the background; apply at register close.
**Tests:** `update_deferred_while_shift_open` · `a_failed_update_before_migration_restores_the_previous_bundle` · `a_post_migration_failure_restores_the_pre_update_snapshot_or_rolls_forward` · `webview_cannot_invoke_the_updater_plugin` · `an_arbitrary_lower_version_is_refused` · `a_register_four_versions_behind_upgrades_in_one_restart`

### 5.5.3 — Migration safety and its budget
**Files:** `crates/pos-db/src/lib.rs`
The app **refuses to run on a half-migrated database** and offers the restore path (E.58). Every migration is exercised in CI against the year-one dataset from 2.9.6, and the timing is recorded.

**The budget is a number, or the test cannot fail.** The full migration chain completes in under **60 s** against the soak dataset on the reference register. Beyond **10 s** the updater shows a determinate progress screen; beyond 60 s the migration fails this gate and needs a background or online strategy. Without a number, the merchant updates before opening, the register spends four minutes on a silent migration, the cashier force-quits it, and that produces exactly the half-migrated database E.58 exists to handle — at 07:55 on a trading day.
**Tests:** `half_migrated_db_refuses_to_open_with_a_named_error` · `all_migrations_run_against_soak_dataset_within_budget`
**Done when:** the measured duration is recorded in `docs/drills/` for the release, so the trend is visible before it crosses.

### 5.5.4 — Packaging polish
Installer branding, a first-run experience, desktop and start-menu entries, kiosk/fullscreen mode, the migration progress screen from 5.5.3, and the multi-monitor guard (E.60).

---

## Milestone 5.6 — Onboarding a merchant

### 5.6.1 — The onboarding wizard
**Files:** `apps/backoffice/src/pages/onboarding/`
Org and store details · TIN · **registered activity, producer/importer role and the obligation evidence** · tax profile · JoFotara obligation, recorded separately from GST registration · currency and display decimals · cash-rounding step · **the merchant-decisions questionnaire** ([`ref/merchant-decisions.md`](ref/merchant-decisions.md)) · users and roles · **entitlement activation** (5.0.4) · catalogue import · register enrollment · **the printed recovery code, shown once** · printer setup · test sale.

Two steps are new and both were holes. **Entitlement activation** is the missing link between "merchant pays" and "register is entitled". **The recovery code** is displayed exactly once, printed, and its custody explained — because the merchant losing it is the scenario 1.8.5b exists for, and emailing it later defeats the design.
**Done when:** a merchant reaches a first real sale without you on site, holding a printed recovery code and an entitled register.

### 5.6.2 — Catalogue import
**Files:** `apps/server/src/import/catalog.rs`
The importer itself lands at **3.6.7**, in the back office beside the catalogue pages it feeds, because the Phase-4 pilot needs three real assortments and typing 1 500 Arabic SKUs per store through a CRUD form is weeks of unbudgeted work that quietly truncates the pilot. This microstep is the onboarding wrapper around it: guided upload, a template, and the per-row error report in the wizard's own language.

CSV with Arabic names, **multiple barcodes per product including pack quantity**, PLU codes, prices, tax categories, `min_age`, `max_price_minor`, and an optional opening-stock column. **Validating and reporting per-row errors rather than failing the file** — a 2 000-row import that fails on row 1 987 with no detail is an import nobody completes.
**Tests:** `import_reports_per_row_errors_and_imports_the_rest` · `a_multipack_barcode_row_imports_its_pack_quantity`

### 5.6.3 — Documentation set
**Files:** `docs/manual/` (new)
Four documents, each with its own `Done when`, because one unsized microstep at the end of the longest phase is how the real exit gate fails on documentation rather than on the product:

| Guide | Contents | Started |
|---|---|---|
| **Cashier** — Arabic, illustrated, one page per screen | sale, tender, park/resume, returns, lock, what to do when something is red | **at the Phase-1 exit gate**, when screens 1–5 exist and are stable; its screenshots are the fixture-based ones the smoke test already produces |
| **Manager** | shifts, blind close, Z, refunds and escalations, cash movements and the safe, exception reports | Phase 2 |
| **Owner** | catalogue, prices, promotions, reports, the sales-side tax reconciliation and what it is *not* | Phase 4 |
| **Support runbook** | the diagnostics screen, the metrics and their thresholds, the top five failures, escalation paths, and the answer to *"what do I owe my suppliers?"* — which is: the goods-received export is the accountant's input, and payables live in their accounting software | Phase 3 |

The cashier guide **is in Arabic first**, and it is confirmed by a native-speaking cashier who is not you — the same discipline as the receipt goldens, named as a role rather than assumed.
**Done when:** each guide exists, is dated, and has been read end to end by someone in the role it is written for.

### 5.6.4 — Support tooling, and the boundary around it
**Files:** `apps/backoffice/src/pages/support/`
Device health across merchants · remote log retrieval (scrubbed) · fiscal dead-letter queue across merchants · version distribution.

**"Across merchants" is the vendor reaching into every merchant's estate**, and it is the one place the plan admitted the service is multi-tenant at all. So it is bounded: a distinct support-principal class, per-action and rate-limited, **audited into the merchant's own trail** so they can see who read what and when, with a retention period for whatever was retrieved. Under a processing agreement the vendor must be able to answer "who accessed this?" — and without this, it cannot.
**Tests:** `a_support_access_writes_an_audit_row_the_merchant_can_see` · `retrieved_logs_expire_on_their_own_clock`

---

## Milestone 5.7 — Pilot merchants

### 5.7.1 — Two pilot merchants, live
Different verticals if possible (a minimarket and a small chain). Real trading, real money, real customers, daily check-ins for the first two weeks.

### 5.7.2 — The compliance story
**Files:** `docs/compliance/README.md`
One document a prospective merchant's accountant can read:
- GST handling, inclusive pricing, rate resolution, and the **sales-side tax reconciliation** — described as what it is, and as what it is not;
- JoFotara: what is certified, when, against which pinned specification version and hash, what the offline procedure is, and what the merchant is responsible for;
- PDPL: the controller/processor allocation, the registration entry, consent, rights, retention, and both breach clocks;
- PCI: the architecture, the SAQ **actually completed**, and the terminal's listing status;
- the sub-processor list, the hosting region and the transfer basis;
- data protection at rest and in transit, backups on two destinations, the recovery-code custody, and the restore-time promise as a number.

**Every claim in it links to a passing test, a signed checklist, or a named advisor.** Nothing in it is aspirational, and any of the `⚠️ OPEN` items in [`00-master-plan.md`](00-master-plan.md) §4a.3 still outstanding at launch appears here as an open item with its default, not as silence.

---

## Exit gate

**Technical**

1. **Signed installers** on Windows, macOS and Linux install without warnings, and each ships its own SBOM and a provenance statement that verifies back to a commit.
2. **The update service is proven end to end**: a cohort receives an update, an update is deferred while a shift is open, a rollout is halted mid-flight, a pre-migration failure restores the previous bundle, and a post-migration failure restores the pre-update snapshot. The webview cannot invoke the updater at all.
3. **Fiscal Certification (5.2) complete and signed** — all nine credentialed items dated, on top of a `2.7.0` manifest whose hashes still match.
4. **PDPL walkthrough** recorded **and the register entry complete**; **PCI SAQ** completed with a QSA; **retention matrix** agreed with the accountant, with its legal hold.
5. **Restore drills** performed on real hardware by someone who did not write the code, and timed: data loss, keychain loss, **and recovery-code-only on a second machine**. Plus the key-rotation drill.
6. **Year-one soak** passes every performance budget through `just bench-gate`, and the full migration chain runs inside its stated 60-second budget.
7. **The packaged application** is exercised automatically on every supported OS, and the breach tabletop has been run against both clocks.
8. **`ref/test-catalog.md` is complete**: every one of the 92 numbered cases is a passing test, an accepted risk with a written rationale, an open question with a stated default, or explicitly out of scope with one — checked by `scripts/check-test-catalog.py`, not by reading the table.

**Commercial**

9. **The legal entity exists**, and the processing agreement, sub-processor list, terms, warranty and liability position, patch SLA and support model are written and signed (5.0).
10. **The path from payment to entitlement works**, performed by someone who is not you: take money, issue an entitlement, and end with an entitled register.
11. **Two pilot merchants trading**, with device health green, alerts reaching a person, and no unexplained variance in any Z report.
12. **The compliance story exists** and every claim in it is backed.

**The real one**

13. **Someone who is not you** installed, provisioned, configured and sold — using only the documentation, in Arabic where the role is Arabic-speaking.

Number 13 is the real gate. The rest is evidence.

---

## After launch — the standing work

Not a phase; the permanent background.

- **Re-run [`ref/plan-validation.md`](ref/plan-validation.md) quarterly**, and re-diff the `2.7.0` manifest against the current ISTD package in the same pass. Jordanian rates move by Cabinet decree, JoFotara adds waves and changes validation, and the PDPL register is live. A compliance claim has a shelf life.
- **Watch the fiscal rejection rate**, and treat it as the primary detector rather than the quarterly audit — a quarter is a quarter of uncleared documents. The alarm is a number: above 2% in 24 hours, or three consecutive rejections with the same pinned ISTD error code, is an ISTD change until proven otherwise, and it reaches a person through `3.9.3`.
- **Keep the deferred list honest.** Everything marked 🧩 in the master plan's J.1 has a named architectural hook. When a merchant asks for gift cards, layaway, e-recharge, or serialized items, the answer is a phase, not a rewrite — but only if the hook is still there. Check it when you touch the surrounding code.
- **The hardware-lab checklist runs before every release.** A golden file proves bytes; only paper proves a receipt.
