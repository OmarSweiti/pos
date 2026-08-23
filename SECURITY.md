# Security

This is a point-of-sale system. It handles money, tax filings, and personal data, in a
jurisdiction with a data-protection law. The security posture is written down rather than
assumed, and this file says only what is true today.

## Status — read this before quoting anything

**No compliance validation has been completed.** Not PCI DSS, not a SAQ, not JoFotara
certification, not a PDPL registration. The project is pre-pilot: no merchant runs it, no
register is installed, no real sale exists.

Saying otherwise — in code, a comment, a commit message, UI copy, a proposal, or a sales
conversation — is forbidden by [`.claude/rules/security.md`](.claude/rules/security.md) and by
[`docs/implementation/ref/security-compliance.md`](docs/implementation/ref/security-compliance.md) §3.
That file is the only place the current status is authoritative. If you need a claim for a
customer, read it first; the answer today is "not yet".

## Reporting a vulnerability

The repository is private and has one maintainer.

- **Do not open a public issue**, and do not describe the flaw in a pull request title.
- Email **omarswaty4@gmail.com** with a description, the affected version or commit, and
  the smallest reproduction you have.
- If a real cardholder value, PIN, or customer record is involved, say that in the subject
  line and **do not paste the value** into the report.

Expect an acknowledgement within a few working days. There is no bounty programme.

## What this system deliberately never stores

From a card, only three things are kept: the PSP reference, the masked PAN the payment
terminal returns for the receipt, and the card scheme. Never the PAN in full, never track
data, never a CVV, never a PIN or a PIN hash.

Never logged, anywhere — not through `tracing`, `IpcError.detail`, crash reporting, or a test
fixture that prints — a value under any canonical sensitive field name: `pin`, `pin_hash`, `pan`,
`card_number`, `cvv`, `track`, `phone`, `email`, `customer_name`, `buyer_name`, `secret_key`,
`client_id`, `db_key`, `token`, `password`, or `entitlement`. Fiscal credentials and signing
material remain sensitive under any provider-specific name. The canonical list lives in
[`docs/implementation/ref/security-compliance.md`](docs/implementation/ref/security-compliance.md)
§5 and must be updated as one contract.

The register's database key lives in the OS credential store. Never in a file, never in an
environment variable in a release build. `POS_DB_KEY` exists for development and CI only, and
the release build must refuse to honour it.

## What is enforced by a machine, not by good intentions

| Enforced | By |
|---|---|
| No float in a money path | `clippy::float_arithmetic = "deny"`, workspace-wide |
| No `unwrap` / `expect` outside tests and `main()` | `clippy::unwrap_used`, `expect_used` = deny |
| A committed migration cannot be edited | `.claude/hooks/protect-immutable.py` · `.githooks/pre-commit` |
| Sensitive filenames, committed plans, oversized staged blobs, and changes to committed migrations are refused | `.githooks/pre-commit`, using the staged index and failing closed on Git errors |
| Secret-like content in an ordinary filename is refused | Gitleaks in `pre-commit`, `pre-push`, `just secrets`, and CI |
| `pos-domain` has no runtime RNG/UUID-generation capability or direct clock/random calls | `scripts/check-domain-purity.py`, in `just lint` and CI |
| Dependabot alerts, and automatic security updates | GitHub, enabled on this repository |
| Dependency version bumps, grouped and monthly | [`.github/dependabot.yml`](.github/dependabot.yml) |
| Advisories, licences, banned crates and registries | [`deny.toml`](deny.toml), via the `supply-chain` CI job and `just audit` |

GitHub's **native** secret scanning and push protection are _not_ available on this private
repository's current plan. The repository therefore supplies an independent, content-based
Gitleaks gate: `pre-commit` scans the staged index, `pre-push` scans reachable history, and CI
scans the proposed commit range with fully redacted output. The local checks remain bypassable
with `--no-verify` or in a clone that skipped `just setup`; CI is server-side evidence but cannot
block an administrator merge while branch protection is unavailable. A finding means rotate the
credential first, then handle history as a separate, explicitly authorised operation.

`just pre-push` runs the deterministic local gates plus a full-history secret scan. CI repeats
those checks and runs the network-dependent supply-chain audit separately.

"Permissions are checked in Rust, in the command handler" remains an architectural requirement,
not a blanket machine-verification claim while the authenticated IPC surface is still being
built. Hiding a button is UX, never authorization; each command must gain its Rust check and
contract coverage with the feature.

Claude Code intentionally runs without its OS sandbox so permitted package-manager, Git/SSH,
GitHub, and other networked shell commands can use the host normally. The checked-in policy keeps
the normal manual permission flow, disables bypass-permissions mode, retains the exact project
Read/Edit denies, and keeps the PreToolUse and ConfigChange launchers fail closed. Those denies
govern Claude tools, not subprocesses: a permitted shell command has ambient host filesystem,
network, environment, and credential access. The repository does not claim subprocess credential
scrubbing, metadata-endpoint denial, or OS containment under this policy. This is an explicit
developer-convenience tradeoff, not an application-security or secret-exfiltration boundary.
Git hooks and CI remain cross-platform backstops and visible signals, but CI cannot block an
administrator merge on this plan.

## Known gaps, stated plainly

- **No installer signing of any kind.** Updater signing is microstep 0.3.2; OS code signing
  (Windows Authenticode, Apple Developer ID and notarisation) is milestone 5.5.1. Until both
  exist, an installer warns loudly on every machine, and nothing should be distributed to a
  device the maintainer does not own. The release workflow deliberately refuses unsigned or
  unverified tags and missing updater keys, separates signing from publishing, and prepares an
  SBOM/checksum manifest; those controls do not substitute for absent signing material.
- **No PII-scrubber test on the logger.** The "no PII in logs" position is currently an
  intention rather than a passing test. Closed by microstep 1.6.8, with the audit work in Phase 1.
- **JoFotara has no sandbox** (correction C-1), so fiscal submission cannot be tested against
  a real validator before production. This is the highest-risk component in the system.

The full treatment, including the audit chain and the secrets model, is
[`docs/implementation/ref/security-compliance.md`](docs/implementation/ref/security-compliance.md).
