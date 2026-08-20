# Phase 5 — Harden and launch

> **Exit:** the product can be sold to someone who is not you, with a signed compliance story you could defend to an auditor, a QSA, and a tax advisor in the same week.

**Effort:** 6–10 weeks, and the fiscal milestone (5.2) is gated on a merchant, not on you.
**Scope:** blueprint §10 hardening, PDPL walkthrough, PCI SAQ with a QSA, **Fiscal Certification**, restore drills, staged-rollout updater, packaging and onboarding.

Phases 0–4 built a product. This phase turns it into something a merchant can buy and a regulator can inspect. Two of its milestones (5.2, 5.3) cannot be completed alone — they require a real merchant, a real accountant, and possibly a real QSA. **Start recruiting for them at the beginning of Phase 4, not here.**

---

## Milestone 5.1 — Load, soak, and restore

### 5.1.1 — Volume soak
**Files:** `crates/pos-db/tests/soak.rs` (new)
Simulate one year of a busy minimarket: ~250 000 sales, ~800 000 lines, ~1 200 000 stock events, ~300 000 audit entries on **one** register database.
**Check:** cart recompute still < 16 ms · search still < 50 ms · cold start still < 3 s · database size and WAL growth acceptable · `VACUUM` behaviour understood.
**Done when:** every Phase-1 budget still holds at year-one volume. If any does not, that is an index or an archival strategy, and it is cheaper to find now.

### 5.1.2 — Sync soak
**Files:** `crates/pos-sync/tests/soak.rs`
Ten registers pushing concurrently for a simulated month. Watch server latency, Postgres bloat, and `change_seq` behaviour.

### 5.1.3 — The restore drill
**Files:** `docs/runbooks/restore.md` (new)
Not a test — a **drill**, performed on real hardware, timed, and written down:
1. Kill a register mid-trading with unsynced sales in its outbox.
2. Wipe the machine.
3. Reinstall, re-provision, restore from backup.
4. Confirm every unsynced sale survived and drained.
5. **Time it.** That number is the merchant's downtime promise.

Repeat for the **keychain-loss** path (E.4): wipe the OS credential store, confirm the recovery screen appears, restore, confirm no data loss.
**Done when:** both drills are documented, timed, and repeated successfully by someone who did not write the code.

### 5.1.4 — Backup verification job
**Files:** `apps/terminal/src-tauri/src/backup_verify.rs` (new)
A scheduled job that opens the newest backup, verifies it, and reports its age to device health. **An unverified backup is a rumour.**
**Tests:** `verify_job_detects_a_corrupted_backup`

---

## Milestone 5.2 — Fiscal Certification

*The only place the real ISTD endpoint is ever contacted. Requires a merchant. The full checklist is [`ref/fiscal-jofotara.md`](ref/fiscal-jofotara.md) §7; it is reproduced here as the milestone's gate because it is the highest-stakes sequence in the project.*

> ⚠️ **Every submission in this milestone is a real fiscal document against the merchant's real tax record.** Do not begin without their informed consent **in writing**.

| # | Action | Gate |
|---|---|---|
| 5.2.1 | Obtain the **official ISTD technical specification, XSD, and code lists** through the merchant's JoFotara portal account | Documents in hand |
| 5.2.2 | Diff the official spec against the reconstruction in the fiscal reference §3. Every difference becomes a `codes.rs` correction | Harness updated; goldens regenerated and reviewed byte by byte |
| 5.2.3 | Obtain production `Client-Id` / `Secret-Key`; store in the OS keyring | Secret never touches disk or the database |
| 5.2.4 | Confirm the **UUID version** question (v4 shape vs. v7 primary keys) against the spec | Answered in writing |
| 5.2.5 | Merchant's tax advisor answers the **offline-clearance** question in writing: is a pending-clearance paper receipt acceptable, and within what window must it clear? | Recorded in `merchant-decisions.md` §11 with name and date |
| 5.2.6 | Submit golden document 1 as a **live, low-value invoice**; verify the QR with the Sanad app | QR verifies |
| 5.2.7 | Immediately credit-note it; confirm both appear in the merchant's ISTD portal | Both visible, netting to zero |
| 5.2.8 | Repeat for goldens 2 (discounted), 3 (multi-rate), 4 (weighed) | All clear |
| 5.2.9 | Run the reconciliation report | Zero unmatched on both sides |
| 5.2.10 | Kill-the-network drill: sell offline for an hour, restore, confirm the queue drains in ICV order | Sequence intact, no gaps |
| 5.2.11 | Environment guard both directions (E.28) | Both refuse |

**Done when:** all eleven are checked, dated, and signed. Until then the product's fiscal claim is *"passes our conformance harness,"* which is honest and is **not** *"JoFotara compliant."*

---

## Milestone 5.3 — Compliance walkthroughs

### 5.3.1 — PDPL walkthrough
**Files:** `docs/compliance/pdpl.md` (new)
Demonstrate, live, to the merchant and ideally to a lawyer:
- consent captured at the register with the wording version recorded;
- consent withdrawn and honoured;
- **export my data** producing a complete file;
- **erasure as anonymisation** — the person gone, the financial facts intact;
- retention periods configured and documented;
- the 24-hour breach-notification runbook (5.3.2);
- the no-PII-in-logs test, run in front of them.

**Done when:** each bullet is a screen recording plus a passing test, filed in `docs/compliance/`.

### 5.3.2 — Breach-notification runbook
**Files:** `docs/runbooks/breach.md` (new)
PDPL requires notifying affected individuals within **24 hours** for serious breaches. A 24-hour clock is not something to design at hour one. Write down: who decides it is a breach, how affected individuals are identified from the data, the notification template in Arabic and English, and who at the merchant sends it.

### 5.3.3 — PCI SAQ with a QSA
**Files:** `docs/compliance/pci.md` (new)
Bring the QSA: the semi-integrated architecture, the terminal's **P2PE listing number** (or its absence), the "only three card fields are stored" test from Phase 2 (2.2.2), and the log-scrubbing test.
**Determine the actual SAQ.** It is SAQ P2PE **only** if the terminal is in a PCI-listed validated P2PE solution; otherwise SAQ B-IP or C — see [`ref/plan-validation.md`](ref/plan-validation.md) §4.
**Done when:** the SAQ is completed and **never claimed before it is**.

### 5.3.4 — Retention policy
**Files:** `docs/compliance/retention.md` (new), settings
Sale documents and Z reports for the statutory period (with the accountant; regionally multi-year). Audit log, customer inactivity purge, backup retention. Configured in settings, documented, and enforced by a job.
**Tests:** `retention_job_never_deletes_a_financial_fact`

---

## Milestone 5.4 — Security hardening

### 5.4.1 — Penetration test
Scope: the sync API, device tokens, the enrollment flow, the licence mechanism, and the local database at rest. External if budget allows; a structured self-review against a written threat model if not.
**Done when:** findings are triaged, fixed or accepted in writing, and retested.

### 5.4.2 — Dependency and supply chain
**Files:** `.github/workflows/ci.yml`
`cargo audit` · `cargo deny` (licences and duplicate versions) · `pnpm audit`. Failing on advisories with an explicit, dated allowlist for accepted ones.

### 5.4.3 — Secrets audit
```bash
git log -p | rg -i 'private key|secret|BEGIN.*KEY|client_id|password'
```
Plus a pre-commit hook (`gitleaks` or equivalent).
**Done when:** the repository history is clean and new secrets cannot be committed.

### 5.4.4 — Audit-chain verification tool
**Files:** `crates/pos-db/src/bin/verify-audit.rs` (new)
A CLI verifying a register's chain and reporting the first break. The forensic tool you hope never to need and cannot build under pressure.

---

## Milestone 5.5 — Release engineering

### 5.5.1 — Code signing and notarization
**Files:** `.github/workflows/release.yml`
Windows Authenticode certificate; Apple Developer ID plus notarization. Secrets into GitHub. An unsigned or tampered update must not install.
**Done when:** installers on all three platforms install without a security warning.

### 5.5.2 — Staged-rollout updater
**Files:** updater endpoint configuration
5% → 50% → 100%, with one-click rollback.
**The product rule: never apply an update while a shift is open** (E.56). Download in the background; apply at register close; a failed update rolls back.
**Tests:** `update_deferred_while_shift_open` · `failed_update_rolls_back`

### 5.5.3 — Migration safety on update
**Files:** `crates/pos-db/src/lib.rs`
The app **refuses to run on a half-migrated database** and offers the restore path (E.58). Every migration is exercised against a year-one-volume database in CI, and the timing is recorded — a migration that takes four minutes on real data needs a progress screen, not a surprise.
**Tests:** `half_migrated_db_refuses_to_open_with_a_named_error` · `all_migrations_run_against_soak_dataset_within_budget`

### 5.5.4 — Packaging polish
Installer branding, a first-run experience, desktop and start-menu entries, kiosk/fullscreen mode, and the multi-monitor guard (E.60).

---

## Milestone 5.6 — Onboarding a merchant

### 5.6.1 — The onboarding wizard
**Files:** `apps/backoffice/src/pages/onboarding/`
Org and store details · TIN · tax profile · currency and display decimals · cash-rounding step · **the merchant-decisions questionnaire** ([`ref/merchant-decisions.md`](ref/merchant-decisions.md)) · users and roles · catalogue import (CSV) · register enrollment · printer setup · test sale.
**Done when:** a merchant reaches a first real sale without you on site.

### 5.6.2 — Catalogue import
**Files:** `apps/server/src/import/catalog.rs` (new)
CSV with Arabic names, barcodes, prices, tax categories. **Validating and reporting per-row errors rather than failing the file** — a 2 000-row import that fails on row 1 987 with no detail is an import nobody completes.
**Tests:** `import_reports_per_row_errors_and_imports_the_rest`

### 5.6.3 — Documentation set
**Files:** `docs/manual/` (new)
- **Cashier guide** — Arabic, illustrated, one page per screen.
- **Manager guide** — shifts, Z, refunds, escalations, exceptions.
- **Owner guide** — catalogue, prices, promotions, reports, the tax filing report.
- **Support runbook** — the diagnostics screen, the metrics, common failures, escalation paths.

The cashier guide **is in Arabic first**. A product whose UI is Arabic-first and whose manual is English-first is not an Arabic-first product.

### 5.6.4 — Support tooling
**Files:** `apps/backoffice/src/pages/support/`
Device health across merchants · remote log retrieval (scrubbed) · fiscal dead-letter queue across merchants · version distribution.

---

## Milestone 5.7 — Pilot merchants

### 5.7.1 — Two pilot merchants, live
Different verticals if possible (a minimarket and a small chain). Real trading, real money, real customers, daily check-ins for the first two weeks.

### 5.7.2 — The compliance story
**Files:** `docs/compliance/README.md`
One document a prospective merchant's accountant can read:
- GST handling, inclusive pricing, rate resolution, the filing report;
- JoFotara: what is certified, when, what the offline procedure is, and what the merchant is responsible for;
- PDPL: consent, rights, retention, breach process;
- PCI: the architecture, the SAQ actually completed, and the terminal's listing status;
- data protection at rest and in transit, backups, and the restore-time promise.

**Every claim in it links to a passing test, a signed checklist, or a named advisor.** Nothing in it is aspirational.

---

## Exit gate

1. **Signed installers** on Windows, macOS and Linux install without warnings.
2. **Staged rollout proven** end to end, including a rollback, and no update ever applies during an open shift.
3. **Fiscal Certification (5.2) complete and signed** — all eleven items dated.
4. **PDPL walkthrough** recorded; **PCI SAQ** completed with a QSA; **retention policy** agreed with the accountant.
5. **Restore drill** performed on real hardware by someone who did not write the code, twice — data-loss path and keychain-loss path — and timed.
6. **Year-one soak** passes every performance budget.
7. **Two pilot merchants trading**, with device health green and no unexplained variance in any Z report.
8. **The compliance story exists** and every claim in it is backed.
9. **`ref/test-catalog.md` is complete**: every one of E.1–E.72 is a passing test, an accepted risk with a written rationale, or explicitly out of scope with one.
10. **Someone who is not you** installed, provisioned, configured and sold — using only the documentation.

Number 10 is the real gate. The rest is evidence.

---

## After launch — the standing work

Not a phase; the permanent background.

- **Re-run [`ref/plan-validation.md`](ref/plan-validation.md) quarterly.** Jordanian rates move by Cabinet decree, JoFotara adds waves and changes validation, and the PDPL authority is still standing up. A compliance claim has a shelf life.
- **Watch the fiscal rejection rate.** A rise means ISTD changed something, and you will see it before the announcement does.
- **Keep the deferred list honest.** Everything marked 🧩 in the master plan's J.1 has a named architectural hook. When a merchant asks for gift cards, layaway, e-recharge, or serialized items, the answer is a phase, not a rewrite — but only if the hook is still there. Check it when you touch the surrounding code.
- **The hardware-lab checklist runs before every release.** A golden file proves bytes; only paper proves a receipt.
