# Contributing

One developer today. This file exists so that the second one is productive on day one, and
so the first one has something to be held to.

Everything here is a pointer. The documents are the authority.

## Before your first commit

1. **[`CLAUDE.md`](CLAUDE.md)** — the nine invariants, on one screen. Each one, violated,
   produces a class of bug that costs money.
2. **[`docs/implementation/01-conventions.md`](docs/implementation/01-conventions.md)** — the
   engineering law. Read once, keep open.
3. **[`docs/implementation/02-development-workflow.md`](docs/implementation/02-development-workflow.md)**
   — every command, the thirteen-station feature lifecycle, the manual test playbook.
4. **[`docs/implementation/03-github-workflow.md`](docs/implementation/03-github-workflow.md)**
   — branches, issues, pull requests, the project board, releases.

```bash
just setup      # hooks, prerequisite checks, and locked deps — do not skip this
just check      # seconds: would it build?
just pre-push   # deterministic local gates, guards, build, and secret history
```

## The shape of a change

```
feature branch  →  development  →  staging  →  main
```

Work branches from `development`, never from `main`. One branch per **group**; one commit per
**microstep**. A pull request into `development` is squash-merged. A promotion PR
(`development → staging`, `staging → main`) is merged with a **merge commit** — squashing a
promotion forks the branches permanently.

```bash
just branch phase-1/group-3-tax     # branch from a fresh development
# ... work, one microstep per commit ...
just pr                             # gates, push, PR into development, watch CI
just merge                          # recheck the exact tips, then safely squash the work PR
```

Commit messages are checked by a hook, not by a reviewer:

```
<type>(<scope>): <summary>            [<step>]

feat(domain): tax engine, inclusive + exclusive extraction   [1.3.4]
```

`type` ∈ `feat` `fix` `test` `docs` `chore` `refactor` `perf`
`scope` ∈ `domain` `db` `sync` `hardware` `fiscal` `terminal` `server` `backoffice` `repo` `impl`

Both lists are closed. A squash-merge commits the **PR title**, so the PR title obeys the same
rule and CI checks it.

## The five things that are never allowed

Even pre-pilot, when almost everything else is cheap to change
([workflow §0](docs/implementation/02-development-workflow.md)):

1. Editing a committed migration — forward-only. Write the next one.
2. Editing `docs/plan/**` — those are source documents. Corrections land in `docs/implementation/`.
3. Committing a secret. If one is already in the tree, say so and stop.
4. Claiming a compliance validation that has not been completed. See [`SECURITY.md`](SECURITY.md).
5. A float in a money path. Fix the arithmetic; do not `#[allow]` the lint.

Guards refuse the first three automatically. The compliance claim is a mandatory human review,
and the money invariant has several automated checks but still requires design review across
boundaries. CI additionally supplies real PostgreSQL, event topology, advisory, and platform-build
evidence that `just pre-push` cannot reproduce locally. If a guard stops you, it is working.

## Opening an issue

Use a form — blank issues are turned off, because the fields are the parts people forget.

| Form | For |
|---|---|
| **Microstep** | a numbered unit of work from a phase file. The normal way work enters the repo |
| **Bug** | wrong behaviour. Money bugs are P0 by default, and need a property test |
| **Merchant decision** | a question only the merchant can answer. Never guess one in code |
| **Toolchain gap** | a command that cannot work yet — a §17 row |

## Reviewing

There is one reviewer, so the substitutes are the whole control: time, a checklist, the
`/review` and `/security-review` tools, CI, and the guards in `.claude/`. The review
priority order is [workflow §7](docs/implementation/02-development-workflow.md) — it starts with
"does any money value touch a float, or round more than once?" and it starts there for a reason.

Review the tests first. If the tests are right, the implementation has somewhere to be wrong
out loud.
