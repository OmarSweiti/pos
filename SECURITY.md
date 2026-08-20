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

Never logged, anywhere — not through `tracing`, not in an `IpcError.detail`, not in a test
fixture that prints: any card value, any PIN or PIN hash, the database key, a JoFotara
credential, or a customer's name, phone, or email.

The register's database key lives in the OS credential store. Never in a file, never in an
environment variable in a release build. `POS_DB_KEY` exists for development and CI only, and
the release build must refuse to honour it.

## What is enforced by a machine, not by good intentions

| Enforced | By |
|---|---|
| No float in a money path | `clippy::float_arithmetic = "deny"`, workspace-wide |
| No `unwrap` / `expect` outside tests and `main()` | `clippy::unwrap_used`, `expect_used` = deny |
| A committed migration cannot be edited | `.claude/hooks/protect-immutable.py` · `.githooks/pre-commit` |
| A key, `.env`, or database file cannot be committed | `.githooks/pre-commit` |
| Permissions are checked in Rust, in the command handler | code review priority 5 — hiding a button is UX, not security |
| Dependabot alerts, and automatic security updates | GitHub, enabled on this repository |
| Dependency version bumps, grouped and monthly | [`.github/dependabot.yml`](.github/dependabot.yml) |
| Advisories, licences, banned crates and registries | [`deny.toml`](deny.toml), via the `supply-chain` CI job and `just audit` |

**Secret scanning is _not_ available on this repository.** GitHub offers it free on public
repositories only; on a private repository it needs GitHub Advanced Security, and the API answers
`422 Secret scanning is not available for this repository`. Push protection therefore does not
exist here either. The only thing standing between a key and the history is
[`.githooks/pre-commit`](.githooks/pre-commit) — a local hook, which `--no-verify` defeats and a
fresh clone does not have until `just setup` runs. Treat "do not paste a secret into a file" as a
human rule, because on this plan it is one.

`just lint && just test && just guards` runs every gate that can run locally. CI runs the same
set, so a green local run predicts a green CI run.

## Known gaps, stated plainly

- **No installer signing of any kind.** Updater signing is microstep 0.3.2; OS code signing
  (Windows Authenticode, Apple Developer ID and notarisation) is milestone 5.5.1. Until both
  exist, an installer warns loudly on every machine, and nothing should be distributed to a
  device the maintainer does not own.
- **No PII-scrubber test on the logger.** The "no PII in logs" position is currently an
  intention rather than a passing test. Closed by microstep 1.6.8, with the audit work in Phase 1.
- **JoFotara has no sandbox** (correction C-1), so fiscal submission cannot be tested against
  a real validator before production. This is the highest-risk component in the system.

The full treatment, including the audit chain and the secrets model, is
[`docs/implementation/ref/security-compliance.md`](docs/implementation/ref/security-compliance.md).
