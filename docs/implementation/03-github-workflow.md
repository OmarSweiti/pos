# GitHub workflow — branches, issues, the board, and releases

Companion to [`02-development-workflow.md`](02-development-workflow.md). That file is how a
change gets *made*; this one is how it gets *tracked, reviewed, promoted, and shipped*.

The distinction matters because they fail differently. A bad inner loop wastes an afternoon. A
bad delivery process ships a wrong total to a merchant and cannot tell you which build did it.

---

## 1 · The four branches

```
   feature branch          development            staging              main
   ─────────────           ───────────            ───────              ────
   phase-1/group-3-tax  →  integration      →     release candidate →  production
   fix/rounding-drift        always green          v0.2.0-rc.1          v0.2.0
   chore/deps                default branch        pilot installers     merchant installers
```

| Branch | Holds | Receives | Produces |
|---|---|---|---|
| `main` | what a merchant is running | a promotion PR from `staging`, or a `hotfix/*` | `v0.2.0` — a production draft release |
| `staging` | the candidate being validated | a promotion PR from `development` | `v0.2.0-rc.1` — a pre-release, pilot channel |
| `development` | everything merged and green | a squash-merged PR from a work branch | nothing; it is the integration surface |
| work branches | one group of microsteps | your commits | one squashed commit on `development` |

**Why four and not one.** The trunk-based single-`main` model this project started with is the
right default for a web service, where "revert and redeploy" is four minutes. This ships
**installers**. A wrong build on a merchant's register is not reverted by a deploy; it is
reverted by a phone call, a site visit, and a database that has already recorded sales against
the wrong version. `staging` buys the thing a web service does not need to buy: a version that
is finished, tagged, installed, and used on hardware for a while before any merchant sees it.

**Why not full GitFlow.** No `release/*` branches, and no `develop`-plus-`release` double
integration. Those exist to let several teams stabilise several releases at once. There is one
developer here; a `release/*` branch per version would be ceremony with nothing on the other
side of it. `staging` *is* the release branch, permanently.

### Naming

| Kind | Pattern | Base | Merges with |
|---|---|---|---|
| A group of microsteps | `phase-<n>/group-<m>-<slug>` | `development` | squash |
| A fix outside a group | `fix/<slug>` | `development` | squash |
| Tooling, deps, docs | `chore/<slug>`, `docs/<slug>` | `development` | squash |
| Shape, not behaviour | `refactor/<slug>`, `perf/<slug>`, `test/<slug>` | `development` | squash |
| Production is broken now | `hotfix/<slug>` | **`main`** | merge commit, then back-merge |
| Promotion | `development` / `staging` themselves | `staging` / `main` | **merge commit** |

The `branch-flow` check refuses anything outside this table, and refuses a work branch whose
base is not `development`.

---

## 2 · The daily loop, as commands

```bash
just branch phase-1/group-3-tax    # fresh development, then branch from it
# ... microstep, gates, commit. One microstep, one commit. Repeat. ...
just pr                            # gates → push → PR into development → watch CI
gh pr merge --squash --delete-branch
```

Promotion, when `development` has a coherent set of groups and the smoke passes:

```bash
just flow                          # what is between the branches right now
just promote-staging               # PR: development → staging
# fill in the promotion template's evidence section, honestly
gh pr merge --merge                # MERGE COMMIT. Not squash. See §6
git switch staging && git pull --ff-only
# bump the version in Cargo.toml and tauri.conf.json, commit, then:
git tag -a v0.2.0-rc.1 -m "Phase 1 groups 1–4" && git push origin staging --follow-tags
```

Then, after the candidate has actually been used:

```bash
just promote-main
gh pr merge --merge
git switch main && git pull --ff-only
git tag -a v0.2.0 -m "Phase 1" && git push origin main --follow-tags
gh release view v0.2.0             # a DRAFT — check the artefacts before publishing
```

---

## 3 · What GitHub enforces here, and what it refuses to

This repository is **private, on the GitHub Free plan**. That is not a detail; it decides which
of these rules are laws and which are merely written down.

```
$ gh api repos/OmarSweiti/pos/branches/main/protection
403  Upgrade to GitHub Pro or make this repository public to enable this feature.
```

Branch protection **and** rulesets are both gated. Neither is available. So:

| Rule | Enforced by | Can it be bypassed? |
|---|---|---|
| No direct push to `main`/`staging`/`development` | [`.githooks/pre-push`](../../.githooks/pre-push) | yes — `--no-verify`, or another clone that never ran `just setup` |
| No force-push or deletion of those branches | same hook | same |
| Commit message obeys conventions §8 | [`.githooks/commit-msg`](../../.githooks/commit-msg) | `--no-verify` |
| No key, `.env`, or database file committed | [`.githooks/pre-commit`](../../.githooks/pre-commit) | `--no-verify` |
| A committed migration cannot be edited | that hook **and** `.claude/hooks/protect-immutable.py` | needs both bypassed |
| PR base is legal for its head branch | `branch-flow` check | yes — the check goes red, the merge button still works |
| PR title is a legal squash commit | `branch-flow` check | same |
| Tests, lint, docs-links | `ci` checks | same |
| A release tag sits on the right branch | `release` workflow's `guard` job | no — the build refuses to run |
| A vulnerable dependency is reported | Dependabot alerts + automatic security updates, **enabled** | no |
| A secret cannot be pushed | **nothing.** Secret scanning needs Advanced Security; the API answers `422 not available for this repository`. `.githooks/pre-commit` is the only stand-in, and it is local and bypassable | yes, trivially |

**The honest summary:** every rule is *visible* and *logged*, and exactly one is *unbypassable*
— the release guard, because it runs server-side and gates the artefact. The rest are seatbelts,
not walls.

### The three ways to turn the seatbelts into walls

1. **GitHub Pro — $4/month, repository stays private.** Then run `just gh-protect`, which is
   already written and already tested against the 403. This is the recommendation: it is the
   cheapest line item this project will ever have, and it converts nine advisory rules into
   enforced ones.
2. **A free organisation.** Does not help. Org-owned *private* repos on the Free plan still get
   no protection; only public ones do.
3. **Make the repository public.** Not an option for a commercial product with a fiscal
   integration in it.

Until (1) happens, the practical rule is: **`just setup` on every machine, always.** A clone
that has not run it has no protection at all.

---

## 4 · Issues — what earns one

Not everything. The plan already lives in the phase files, and copying 400 microsteps into
GitHub Issues would create a second, worse copy of the plan that immediately starts drifting.

**An issue exists when there is state to track that the phase file cannot hold:** what is in
flight, what is blocked, what surprised you, what you owe the merchant.

| Open an issue for | Do **not** open an issue for |
|---|---|
| the microstep you are starting now (one at a time — WIP = 1) | every microstep in the phase, in advance |
| a bug, with numbers | a refactor you might do one day |
| a question only the merchant can answer | a design decision already written in `ref/` |
| a toolchain gap — a §17 row | a note to yourself that belongs in the PR description |
| a time-boxed spike, with the question it must answer | "improve error handling" |

Blank issues are **off**. The four forms exist because the fields are exactly the parts that get
skipped, and a bug report without a reproduction is a memory, not a task.

| Form | Demands, and will not let you skip |
|---|---|
| **Microstep** | the step number, its phase-file heading, dependencies, files, exact test names, the `Verify:` command, one checkable `Done when:` sentence, the `E.n` rows it closes, and which of the nine invariants it touches |
| **Bug** | expected, actual **with figures to the fil**, a reproduction from a *clean* test bed, a severity, the invariant broken, and the new `E.n` row |
| **Merchant decision** | the question in a merchant's words, what it blocks, the assumption running in the code meanwhile, and the cost of being wrong |
| **Toolchain gap** | the gap, what it costs to leave open, and what closes it |

The `Done when` field is the one that earns its keep. "The tax engine works" is not a done-when.
"Σ line tax == receipt tax, exactly, ∀ inputs" is — and it is also a test name.

### Labels are a query language

Six families, applied by path automatically where possible
([`.github/labeler.yml`](../../.github/labeler.yml)):

| Family | Values | Answers |
|---|---|---|
| `type:` | mirrors the commit types | what kind of change |
| `area:` | mirrors the commit scopes | which crate or app |
| `phase:` | `0`–`5` | which exit gate it belongs to |
| `priority:` | `P0` `P1` `P2` | P0 = wrong money, lost sale, corrupted data, compliance breach |
| `risk:` | `money path` `migration` `security` `compliance` `immutable` | **how it must be reviewed** |
| `needs:` | `merchant answer` `decision` `hardware` | why it is not moving |
| `meta:` | `toolchain gap` `dependencies` `flake` `spike` `accepted risk` | bookkeeping |

The `risk:` family is the one that changes behaviour rather than describing it. `risk: money path`
means the PR needs a property test, not an example test. `risk: migration` means a Postgres
mirror and a data-migration test. `risk: compliance` means a claim needs evidence before it is
written down anywhere.

Queries worth keeping:

```bash
gh issue list --label "priority: P0"
gh issue list --label "needs: merchant answer"          # anything here for a week is a risk
gh issue list --label "risk: money path" --state all
gh issue list --milestone "Phase 1 — sellable MVP" --state open
gh issue list --label "meta: flake"                     # should always be empty
```

Milestones are the six phase gates, and nothing else. A milestone's burndown is the honest answer
to "how far is Phase 1", which a phase file cannot give you because it does not know what is done.

---

## 5 · The board — one project, four views

`POS delivery`, a Projects v2 board. Free on a personal account, works on private repositories,
and it is the one piece of GitHub's project machinery that is fully available on this plan.

```bash
gh auth refresh -s project,read:project    # once — the default login lacks this scope
just gh-project                            # creates the project and its fields
```

Custom fields, because the built-in ones cannot answer the questions this plan asks:

| Field | Type | Why |
|---|---|---|
| `Phase` | select `0`–`5` | the unit the plan is organised in |
| `Group` | text | the branch and PR unit |
| `Microstep` | text | the commit unit, and a stable reference |
| `Priority` | select `P0`–`P2` | triage |
| `Risk` | select | how it gets reviewed |
| `Blocked` | select | why it is not moving |
| `Target` | date | only where a real date exists — a fictional date is worse than none |

Four views, and no more. A board with nine views is a board nobody reads:

| View | Layout | What it is for |
|---|---|---|
| **Board — now** | board, by Status, hiding Done | the only view open while working. More than one card in progress means WIP is not 1 |
| **Phase plan** | table, grouped by Phase, sorted by Microstep | reading order. "What is left before the gate?" |
| **Blocked** | table, filtered to anything with `Blocked` set | §16's weekly question, as a saved query |
| **Money & compliance** | table, filtered to `Risk` ∈ money path, migration, compliance | the rows where a mistake costs money instead of time |

Enable the three built-in workflows (project → ⋯ → Workflows) — they are free and they remove the
step everyone forgets: *item closed* → Done, *PR merged* → Done, *auto-add* new open issues.

**The board is not the plan.** The phase files are the plan. The board holds *status*: what is in
flight, what is blocked, what is done. When the two disagree, the phase file is right and the
board is stale — never the other way around.

---

## 6 · Pull requests — two kinds, and the merge button matters

### A work PR: into `development`, squash-merged

One per group. Squashing gives `development` one commit per group with the microsteps in the
body — a history that reads as a plan, and a bisect that lands on something meaningful.

**A squash-merge commits the PR title, not your commit messages.** This surprises people once
and then costs them a broken history. Your careful `feat(domain): …  [1.3.4]` messages are
discarded; whatever is in the PR title becomes the commit on `development`. That is why the
`branch-flow` check validates the PR title against conventions §8, and why the microstep messages
end up in the squash *body*.

The template asks for six things — what, why now, invariants touched, verification, test catalog,
and what is deliberately *not* in the PR. The last one is the one reviewers thank you for.

### A promotion PR: `development → staging`, `staging → main`, merged with a MERGE COMMIT

**Never squash a promotion.** A squash rewrites the commits into a new one, so `staging` no longer
shares history with `development`. Every subsequent promotion then re-proposes the same work,
`just flow` becomes meaningless, and the only fix is to delete and recreate the branch. CI cannot
choose the merge button for you, so the `promotion-notice` job posts a warning on exactly the PRs
that are at risk of it.

Rebase-merge is **disabled** at the repository level. It is the one button that quietly produces a
history nobody chose.

### Reviewing your own PR

There is one developer, so the substitutes are the control, not a formality —
[workflow §7](02-development-workflow.md). In GitHub terms, two habits do most of the work:

- **Open the PR, then walk away.** Read the diff in the GitHub UI the next morning. The diff
  reads differently there than in your editor, which is the entire point.
- **Leave review comments on your own PR**, and resolve them. A comment you wrote and answered is
  a decision with a record. A thought you had and forgot is a bug in three weeks.

`CODEOWNERS` auto-requests the review. On this plan it cannot *require* it — that is a
protection setting — but it is the file that starts working the day a second developer arrives.

---

## 7 · Releases — two channels from one workflow

| Tag | On | Result |
|---|---|---|
| `v0.2.0-rc.1` | `staging` | a **pre-release** draft — the pilot channel |
| `v0.2.0` | `main` | a **production** draft release |

The `guard` job refuses the build before the matrix starts if:

- a final `vX.Y.Z` tag is not an ancestor of `main`, or a `-rc`/`-beta` tag is not an ancestor of
  `staging` — the wrong-branch mistake, caught in twenty seconds instead of three platform builds;
- the tag disagrees with the version in `Cargo.toml` or `tauri.conf.json`.

This is the only rule in this document that cannot be bypassed, because it runs server-side and
gates the artefact rather than the merge.

A tag is immutable. A bad build is a **new patch**, never a moved tag — a moved tag means two
different binaries claim the same version, and a merchant support call can no longer be answered.

Still missing before anything reaches a machine you do not own: updater signing keys (0.3.2), OS
code signing (5.5.1), and a restore path you have actually exercised. See
[workflow §15](02-development-workflow.md) and [`../../SECURITY.md`](../../SECURITY.md).

### A hotfix

```bash
git switch main && git pull --ff-only
git switch -c hotfix/receipt-total-drift
# fix, test, gates
gh pr create --base main --fill
gh pr merge --merge                       # merge commit
git tag -a v0.2.1 -m "receipt total drift" && git push origin main --follow-tags
# then, immediately, so the fix is not lost on the next promotion:
gh pr create --base development --head main --title "chore(repo): back-merge v0.2.1   [—]"
```

The back-merge is the step that gets skipped, and skipping it means the next promotion silently
reverts the hotfix. It is the single most common way a two-branch-and-up flow goes wrong.

---

## 8 · Actions minutes are a real budget

A private repository on the Free plan gets **2,000 Actions minutes a month**, and the multipliers
are not kind: **Linux ×1, Windows ×2, macOS ×10.** A three-platform release build can consume a
double-digit percentage of the month's allowance in one tag.

What is already done about it:

- `concurrency` groups on `ci`, so a superseded run is cancelled — but **never** on `staging` or
  `main`, where a half-cancelled build tells you nothing;
- the release `guard` job, so a mistagged commit costs twenty Linux seconds instead of three
  builds;
- `Swatinem/rust-cache` on every job;
- Dependabot set to **monthly and grouped**, not daily and per-crate. A daily stream of single-crate
  bumps is both a minutes bill and a review load nobody sustains — and an unread dependency bump
  is how a supply-chain problem arrives politely.

What to watch: tag deliberately. `-rc` tags are for candidates that will actually be installed,
not for every merge to `staging`.

---

## 9 · Jira — free, worth connecting, not worth centring

The user asked for Jira only if it is free. It is, with a real caveat about *which part* is free.

| Piece | Cost | Note |
|---|---|---|
| Jira Software **Free** plan | free forever | up to **10 users**, 2 GB storage, 100 automation runs/month, community support only |
| The **GitHub for Jira** app (Atlassian-built) | free | branches, commits, PRs, builds and deployments shown on the Jira issue |
| Advanced roadmaps, unlimited automation, audit logs | **paid** | Standard ≈ $8/user/month, Premium ≈ $15 |
| Most third-party Marketplace apps | **paid, per user** | this is where a "free" Jira usually stops being free |

### What connecting actually gives you

Install **GitHub for Jira** from the Atlassian Marketplace, authorise it against `OmarSweiti`,
and Jira's *development panel* starts showing the branch, the commits, and the PR for each issue.
The link is made by putting the issue key in the branch name or the commit message:

```
feat(domain): tax engine, inclusive extraction   [1.3.4]

POS-42
```

Put the key in the **commit body**, on its own line — not in the subject. The subject is checked
against conventions §8 by a hook and by CI, and an issue key in it will be refused. A branch named
`phase-1/group-3-tax` carries no key, so the commit body is the link, and Jira's smart commits
(`POS-42 #time 2h #comment …`) work from there too.

### The recommendation: one system of record, and it should be GitHub

Running Jira *and* GitHub Issues *and* a GitHub Project means three places to update and three
chances to be stale — and with one developer, the update that gets skipped is whichever one is
not open at the time. Pick one:

**Use GitHub Issues + Projects as the system of record** — because in this repository the work
items are already deeply coupled to the code: microstep numbers, `E.n` catalog rows, invariant
numbers, file paths, `ref/` sections. Issue forms can demand those fields; the labeler can apply
`risk: money path` from a path glob; a PR can close an issue by writing `Closes #42`. Jira knows
none of that without paid automation.

**Connect Jira as a read-only window** when a stakeholder — an investor, a partner, a client whose
PMO runs on Jira — needs to see progress in a tool they already use. The free app makes that
window real at zero cost. Create the `POS` project, mirror only **epics and phase-level
milestones**, and let the microsteps stay in GitHub. Six Jira epics mapping to the six phase gates
is a thing you can keep true by hand. Four hundred mirrored microsteps is not.

**Switch Jira to the system of record** only when a second or third person is doing non-engineering
work in it — sales, support, merchant onboarding. That is the point where Jira's strength (workflow
across roles) starts to outweigh the coupling GitHub gives you for free.

### Setting it up, when you want it

Neither half of this can be done from the repository, so it is a click-path, not a script:

1. **https://www.atlassian.com/software/jira/free** — create the site, choose the Free plan.
2. Create a **team-managed** Scrum or Kanban project, key `POS`.
3. **Apps → Explore more apps → "GitHub for Jira"** → Install → Configure → *Connect a GitHub
   account* → authorise `OmarSweiti` → select the `pos` repository.
4. Create six epics, one per phase gate, matching the milestones in this repository.
5. Confirm the link works: put `POS-1` in a commit body, push, and check the Jira issue's
   development panel.

There is nothing to configure on the GitHub side — the app reads through the authorisation, and it
needs no workflow, no secret, and no webhook of your own.

---

## 10 · Documentation lives in `docs/`, and that is the professional answer

GitHub Wikis are **not available** on a private repository on the Free plan (`has_wiki: false`,
and it cannot be turned on). GitHub Pages from a private repository also needs Pro. So the wiki
question has an easy answer here — but it would have the same answer on any plan:

**Engineering documentation belongs in the repository, not in a wiki.** In `docs/`, it is
versioned with the code that it describes, reviewed in the pull request that changes the
behaviour, checked by CI, and correct at every commit — you can check out `v0.2.0` and read the
documentation as it was when that build shipped. A wiki is a separate history with no review, no
CI, and no relationship to any version. It rots, quietly, and nobody notices until it is wrong.

What is enforced here: [`scripts/check-doc-links.sh`](../../scripts/check-doc-links.sh) fails CI on
a broken cross-reference, and a `PostToolUse` hook refuses a write that leaves one. A doc set is
only worth its cross-references.

If a shareable, browsable page is genuinely needed — for a partner, an investor — the pattern
already in this repository is the right one: `status-page.html`, published as an Artifact. It is a
*view* of the doc set, never the source.

---

## 11 · What is deliberately not set up

Honest list, same spirit as [workflow §17](02-development-workflow.md).

| Not set up | Why, and what closes it |
|---|---|
| Branch protection / rulesets | the plan does not sell them for private repos. `just gh-protect` is written and waiting. $4/month |
| Required reviewers | one developer; `required_approving_review_count` is 0 in the protection script. Raise it to 1 when a second developer arrives |
| GitHub Discussions | off. With one developer it is a second inbox. Turn it on when there are pilot merchants with questions |
| Wiki / Pages | unavailable on this plan, and the wrong home for engineering docs anyway — §10 |
| `cargo deny` / `pnpm audit` in CI | before the first external pilot. A supply-chain gate with no consumers is theatre; with a pilot merchant it is not |
| A staging deployment of `apps/server` | there is no hosted environment yet. `staging` currently means "a tagged candidate", not "a running system" |
| CODEOWNERS as a *requirement* | it auto-requests review today; requiring it is a protection setting |
| Jira | free and connectable, deliberately deferred until someone outside engineering needs it — §9 |
| Environments / deployment protection rules | an org or Pro feature for private repos, and there is nothing to deploy yet |
| Signed commits | worth doing before the first external contributor. Not a substitute for anything above |
| Secret scanning / push protection | unavailable — GitHub gives it free on **public** repos only; a private repo needs Advanced Security. The `pre-commit` hook is the stand-in, and [`../../SECURITY.md`](../../SECURITY.md) says so plainly instead of implying coverage |
| Auto-merge | the API accepts `allow_auto_merge`, returns 200, and leaves it `false`. It needs required status checks to gate on, and those need branch protection. It will start working the day `just gh-protect` does |

---

## 12 · The setup checklist

For a new machine, or a new developer:

```bash
gh auth login                              # ssh, and the `repo` scope
gh auth refresh -s project,read:project    # once, for the board
git clone git@github.com:OmarSweiti/pos.git && cd pos
just setup                                 # deps AND core.hooksPath — do not skip
just guards                                # prove every guard still refuses
just pre-push                              # prove the machine can go green
```

For the repository itself — idempotent, run again whenever this document changes:

```bash
just gh-bootstrap-dry     # read it first
just gh-bootstrap         # labels, milestones, merge behaviour, default branch
just gh-protect           # refuses politely until the plan allows it
just gh-project           # the board and its fields; then the four views, by hand
```

---

*Companion to [`01-conventions.md`](01-conventions.md) and
[`02-development-workflow.md`](02-development-workflow.md). Written against the repository as it
stood on 20 August 2026 — when the plan, the flow, or a gate changes, this file changes with it.*
