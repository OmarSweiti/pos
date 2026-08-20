# POS — read this before you touch anything

Cross-platform, offline-first point of sale for the Jordanian market.
Tauri 2 + Rust core + React UI; SQLite/SQLCipher on the register, Axum/Postgres in the cloud.

## The plan

| Document | Answers |
|---|---|
| [`docs/plan/business-functional-master-plan.md`](docs/plan/business-functional-master-plan.md) | **what** to build, and why Jordanian law demands it |
| [`docs/plan/engineering-blueprint.md`](docs/plan/engineering-blueprint.md) | **how** — stack, architecture, standards |
| [`docs/implementation/`](docs/implementation/) | **what to type**, in what order, and how you know it worked |

Start at [`docs/implementation/README.md`](docs/implementation/README.md).
The engineering law is [`docs/implementation/01-conventions.md`](docs/implementation/01-conventions.md) — read it once, keep it open.

## The nine invariants

Not style preferences. Each one, violated, produces a class of bug that costs money.

1. **Money is `i64` minor units. Always.** No float touches money in Rust, TypeScript, SQL, or JSON.
   Intermediate math uses `rust_decimal`, rounds **once**, returns to `i64`.
   `clippy::float_arithmetic` is **denied** workspace-wide.
2. **The minor-unit exponent is per-currency data.** JOD = 3 (1 dinar = 1000 fils). Never `100`.
3. **Quantities are `i64` milli-units.** `1 unit = 1000`. Weighed and discrete share one representation.
4. **Completed sales are immutable.** No `UPDATE` on a complete sale, ever. Corrections are new documents.
5. **Price and name are copied onto the sale line** at capture time. Reports and refunds read the line, never today's catalog.
6. **Stock is a ledger.** On-hand is `SUM(qty_delta)`, cached and rebuildable.
7. **Ordering comes from server versions and UUIDv7**, never a device clock.
8. **`pos-domain` is pure.** No I/O, no SQLite, no Tauri, no network, no clock, no randomness — time and IDs are *arguments*.
9. **Every fact write and its outbox row commit in one transaction.**

## Quality gates

```bash
just lint     # fmt --check · clippy -D warnings · acyclic · biome ci · doc-links
just test     # cargo nextest --workspace · pnpm -r test
just setup    # after pulling
```

CI runs exactly these, so a local `just lint && just test` predicts it.
`unwrap()` and `expect()` are **denied** outside tests and `main()`.

## What is enforced, not merely written down

`.claude/rules/` holds the standards, split by the paths they govern — they load
when a matching file is read, so a Rust rule costs nothing while you edit React.
`.claude/hooks/` holds the guards, which are not advisory:

| Guard | Refuses |
|---|---|
| `protect-immutable.py` | writing a **committed migration**, or anything in `docs/plan/` |
| `docs-links-on-write.sh` | leaving a broken cross-reference in `docs/**.md` |

Both are negative-tested — `./.claude/hooks/test-protect-immutable.sh`. Run it after
touching either. A guard nobody has seen fail is a guard nobody should trust.

## Where things live

```
crates/pos-domain/     pure rules: Money, tax, cart machine   ← the crown jewel, keep it pure
crates/pos-db/         SQLite schema, migrations, repositories
crates/pos-sync/       outbox/cursor protocol (client + server)
crates/pos-hardware/   printer/scanner/terminal traits + simulator
apps/terminal/         the register (Tauri 2): src/ = React, src-tauri/ = Rust shell
apps/server/           Axum: sync, auth, reporting
apps/backoffice/       React admin
```

Migrations are **forward-only** and are **never edited once committed**.
