# POS repository guidance

This file applies to the entire repository. It is the Codex entry point for the
project's established engineering standards; it does not replace the normative
project documentation.

## Start with the source of truth

- Read `CLAUDE.md` before making a change. It is the maintained cross-agent
  operational overview and routes to the relevant documentation.
- Read `docs/implementation/README.md` to locate the current implementation
  step. `docs/implementation/01-conventions.md` is the engineering law, and
  `docs/implementation/02-development-workflow.md` defines the working loop.
- `docs/implementation/` is the plan of record. Treat `docs/plan/` as immutable
  historical source material: read it for intent, never for a name. If
  implementation needs the plan to change, report the conflict instead of
  editing, deleting, or moving a plan file.
- **Read `docs/implementation/00-master-plan.md` section 4a, "Errata and
  concordance", before acting on any sentence from `docs/plan/`.** It is the
  single ledger of every superseded name, number, and rule. `rate_bp`, banker's
  rounding as the money default, `stock_movement`, `tax_group`,
  `product_barcode`, `user`, `role_perm`, a mutable `loyalty_points` column,
  migrations with `down` steps, and a Phase-3 fiscal production cutover all
  still read as current truth in the source plans and are all wrong now.
  Section 4a also carries the status of corrections C-1 to C-4 — two of the four
  corrections were themselves wrong — and the open items none of them closed.
- Do not copy phase status, migration counts, or other changing facts into agent
  guidance. Link to their maintained source instead.
- Never state an unresolved legal, tax, or regulatory question as settled. The
  reference documents carry those as greppable `⚠️ OPEN` blocks with a stated
  default and an owning microstep; keep that shape when a question is still
  open, and do not convert a default into an answer.

## Load the relevant scoped standard

Codex does not automatically path-match the files in `.claude/rules/`. They are
shared repository standards, so read the applicable file in full before editing
the corresponding area:

| Work area | Required guidance |
|---|---|
| `crates/pos-domain/**`, root dependency features, domain purity/property-test checkers, or the shared Rust lexer | `.claude/rules/rust-domain.md` |
| `crates/pos-db/**`, either migration tree, schema reference/verifiers, or the shared Rust lexer | `.claude/rules/sql-migrations.md` |
| Either app's `src/**` or `index.html`, or `packages/money/**`, `packages/ui/**`, or `packages/api-types/**` | `.claude/rules/frontend.md` |
| Authentication, authorization, secrets, payments, PII, logs, or compliance | `.claude/rules/security.md` — the only rule file with no path scope, so it applies to every change |

Read every applicable row when a change crosses boundaries. For schema changes,
use the repository's `$add-migration` skill. Use `$verify-schema` when checking
the SQLite migrations or the executable schema reference.

## Codex-specific execution boundaries

These are what Codex brings to the shared law, and where each one stops.

- `.codex/hooks.json` defines the immutable-path check before every `Bash` and
  `apply_patch` call, and the documentation-link check after every
  `apply_patch`. Codex loads them only for a trusted repository and hook
  definitions require their own review: open `/hooks`, read the exact commands,
  and trust them once. `--dangerously-bypass-hook-trust` is not part of setup.
- `.codex/rules/safety.rules` contains no `allow` rules on purpose, so trusting
  this repository never grants silent unsandboxed execution. It prompts for
  history-changing Git, pushes, mutating GitHub commands, publishing, and
  destructive database operations, and forbids `sqlx migrate revert`. Execpolicy
  is exact-prefix escalation policy, not a universal command parser; the
  PreToolUse adapter separately catches wrapped and nested spellings of that
  revert, and the Git hooks and CI remain the backstops.
- `.codex/config.toml` keeps work in a `workspace-write` sandbox with network
  access deliberately on inside that write boundary — reading the world is free,
  writing is visible — and filters credential-shaped environment variables
  including `DATABASE_URL`. That is a write boundary, not proof of host
  isolation for a command a person has approved out of the sandbox.
- The Windows hook commands are Python-only and native Windows dispatch is not
  verified in this repository. Do not present hook coverage as an
  operating-system enforcement boundary on any platform.

## Non-negotiable invariants

1. Money is signed `i64` minor units end to end. Never use floating point for
   money. Use `rust_decimal` for intermediate math, round once, and return to
   minor units.
2. Currency exponent is data. JOD has exponent 3; never assume a divisor of 100.
3. Quantities are signed `i64` milli-units, where one unit is 1000.
4. Completed sales are immutable. Corrections are new documents. Tender
   settlement and shift close are not exceptions: they append
   `tender_status_event` and `shift_close_event` facts, and the current state is
   a rebuildable projection. The server revokes `UPDATE` on fact tables, so a
   register-side mutation leaves central reconciliation permanently stale.
5. Capture product name and price on the sale line; historical reads do not use
   the current catalog.
6. Stock is a ledger. On-hand is a sum of quantity deltas, cached in
   `stock_cache`, and the cache is rebuildable by a command CI runs. Ledger
   append, cache projection, and the watermark commit together.
7. Ordering comes from owned sequences, not a device clock. Pull order is the
   server's `version`; push order is `(register_id, sync_outbox.seq)`. UUIDv7
   supplies identity and index locality, never causal order.
8. `pos-domain` is pure: no I/O, database, framework, clock, or randomness. Pass
   time and IDs in as arguments.
9. A business transaction commits its whole fact graph and its delivery envelope
   together: the facts, one `sync_commit`, the complete `fact_commit_member`
   manifest, and the `sync_outbox` delivery rows, in one transaction. A partial
   manifest lets the server accept a header without its lines or tenders.

Two further rules are enforced as strictly as the nine. No base sale command
accepts a price: `cart_add_line` has no `unit_price_minor`, a price-embedded
label arrives as a typed `ScanLookup::PriceEmbedded`, and price-bearing IPC
arguments exist only on audited `cart_override_price`, capped audited
`cart_add_department_sale`, and inert content-hashed `product_quick_add_prepare`. Every privileged
command consumes a one-use `ApprovalHandle` in the same transaction as its
financial effect and audit row. `docs/implementation/01-conventions.md` sections
12 and 13 own both.

## Protected and sensitive material

- Never edit, delete, rename, or replace a migration already present in `HEAD`.
  Add the next forward-only migration and preserve both SQLite/PostgreSQL
  semantics. Deleting or renaming is an edit. Never run `sqlx migrate revert`;
  correct an applied schema with the next migration.
- Every migration entry must be a repository-owned regular SQL file. Symlinks,
  gitlinks, devices, and other filesystem indirection are forbidden in both
  migration trees.
- PostgreSQL/sqlx migration names use a unique, strictly increasing 14-digit UTC
  timestamp and lower-snake suffix; every mirror also declares its SQLite
  counterpart (or a reasoned server-only exception) in the header.
- PostgreSQL migrations are transactional by default. Use SQLx's case-sensitive
  byte-zero `-- no-transaction` marker only for a statement PostgreSQL forbids
  inside a transaction, and account explicitly for partial-failure recovery.
- Do not bypass or weaken repository hooks, guards, linters, or CI checks to make
  a change pass. Fix the cause, or report why the documented rule must change.
- `scripts/check-branch-workflow-policy.rb` freezes the exact bytes of every
  workflow, Git hook, `.claude/` and `.codex/` policy tree, and both agent entry
  points — this file and `CLAUDE.md` included. Editing any of them is
  deliberately red in CI until a human reads the diff; that red is the review
  signal, not a defect to route around. Ordinary application and test code is
  not byte-pinned.
- Do not inspect or expose `.env` files, existing database files, production or
  customer data, private keys, credential stores, package-manager credentials,
  or similar secret-bearing material. In-memory fixtures, throwaway databases,
  and an explicitly scoped development database are allowed when a verification
  command requires them. Work from examples, schemas, and redacted metadata. If
  a secret is discovered, stop and report it without reproducing the value.
- Never log or place in errors card data, database keys, fiscal secrets, or
  customer name, phone, or email. Never claim PCI, PDPL, or JoFotara validation
  that the project has not actually completed.

## Working and verification agreements

- Preserve unrelated working-tree changes. Inspect the diff before and after an
  edit, and do not discard or rewrite user work.
- Use the root `just` recipes rather than inventing parallel command sequences.
  Start with the narrowest relevant check, then expand in proportion to risk.
- For substantive implementation, run `just lint` and `just test`. Before a
  requested push or release, run `just pre-push`. Run `just guards` after changing
  a guard, hook, or its tests. Report every skipped or unavailable check.
- Schema work must run `just verify-schema`; validate the PostgreSQL mirror with
  `just verify-pg` when applicable, and state clearly if the engine pass was
  skipped because no PostgreSQL service was available. Repository policy removes
  inherited `DATABASE_URL` from Codex shell commands, so Codex uses the throwaway
  Docker path or reports a mapping-only result; a human may run an explicitly
  scoped development-server pass after confirming that target is disposable.
  The verifiers require exact runtime-array/directory parity and create a unique
  scratch database for each PostgreSQL run.
- Run `just domain-purity` for `pos-domain` dependency or architecture changes;
  UUID generation/RNG features and direct clock/random calls are forbidden there.
- A change that adds, renames, retires, or lands a catalogued test must leave
  `docs/implementation/ref/test-catalog.md` reconciled. `just lint` runs
  `scripts/check-test-catalog.py` for that; the `PLANNED` allowlist may only
  shrink, and a retired name needs its tombstone. Do not raise the ceiling to
  make a check pass.
- When a change contradicts a maintained document, fix the document in the same
  change. When it supersedes something a source plan says, record the erratum in
  `docs/implementation/00-master-plan.md` section 4a.
- Do not commit, push, open or merge a pull request, publish, deploy, or reset a
  database unless the user requested that state change. Add or update a
  dependency only when it is necessary for the requested implementation; explain
  the choice and include the lockfile and supply-chain verification.

## Git and delivery

- Follow `docs/implementation/03-github-workflow.md`. Normal work branches from
  `development`, not `main`; direct pushes, force-pushes, and branch deletion are
  forbidden for `development`, `staging`, and `main`.
- Work pull requests target `development` and are squash-merged. Promotion pull
  requests (`development` to `staging`, then `staging` to `main`) use merge
  commits; never squash a promotion.
- Commit and squash titles follow
  `<type>(<scope>): <summary>  [<step>]`, where step is `N.N.N`, an ordered
  `N.N.N–N.N.N` range, or `—`. Do not skip hooks with `--no-verify`. Coding
  assistants are tools: do not add AI co-author or generated-by trailers to
  commits or pull requests. The exact Dependabot bot author/trailer combination
  is a narrow compatibility exception and follows the same title grammar; local
  Git author metadata alone is not cryptographic proof of App identity.

## Code review rules

- Treat any violation of the nine invariants, append-only migrations, domain
  purity, transaction/outbox atomicity, authorization boundaries, or secret/PII
  handling as blocking.
- Treat a price on any base-sale IPC argument, a price field outside the three
  controlled entries above, and a privileged command that does not consume its
  `ApprovalHandle` inside the effect-and-audit transaction as blocking. Each
  defeats the price or escalation controls in the plan and is cheap to add by
  accident.
- Require regression coverage for changed behavior and property tests for domain
  invariants. Flag a check that reports success while running zero relevant tests.
- Check Arabic-first RTL behavior, logical CSS, keyboard use, and generated IPC
  contracts for frontend changes.
- Prefer evidence from tests and maintained documentation. Do not approve a
  security, compliance, performance, or compatibility claim based only on prose.
- Leave mechanical formatting to the repository tooling; focus review comments
  on correctness, safety, behavior, maintainability, and missing verification.
