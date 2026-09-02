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
| A group of microsteps | `phase-<0-5>/group-<m>-<slug>` | `development` | squash |
| A fix outside a group | `fix/<slug>` | `development` | squash |
| Tooling, deps, docs | `chore/<slug>`, `docs/<slug>` | `development` | squash |
| Shape, not behaviour | `refactor/<slug>`, `perf/<slug>`, `test/<slug>` | `development` | squash |
| Release version preparation | `chore/release-v<major>.<minor>.<patch>` | `development` | squash |
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
work_pr=$(gh pr view --json url --jq .url)
IFS=$'\t' read -r work_base work_head < <(
  gh pr view "$work_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
bash ./scripts/watch-pr-checks.sh "$work_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$work_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$work_base" ] || [ "$current_head" != "$work_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$work_pr" --match-head-commit "$work_head" --squash --delete-branch
```

`just merge [pr]` performs that complete sequence for ordinary work PRs: it
canonicalises the PR, refuses promotions/hotfixes/closed PRs, validates the
head/base route, watches the exact required checks, re-reads both tips and PR
state, and passes the reviewed head to `--match-head-commit`. It deliberately
cannot merge a promotion or hotfix because those require merge commits. The
expanded commands above document the safety contract rather than a second path
that may omit one of those checks.

Promotion, when `development` has a coherent set of groups and the smoke passes. Put the version
bump through the normal PR path first; never create an unreviewed commit directly on `staging`:

```bash
set -euo pipefail

# Wait until GitHub registers one exact push-triggered workflow run, then print its database id.
# The SHA and ref checks are deliberately repeated even though gh receives both as filters.
exact_push_run() {
  local workflow=$1 ref=$2 sha=$3 run_id
  for _ in $(seq 1 60); do
    run_id=$(gh run list --workflow "$workflow" --event push --branch "$ref" \
      --commit "$sha" --limit 20 --json databaseId,headBranch,headSha \
      --jq "map(select(.headSha == \"$sha\" and .headBranch == \"$ref\")) | first | .databaseId // empty")
    if [ -n "$run_id" ]; then
      printf '%s\n' "$run_id"
      return 0
    fi
    sleep 2
  done
  echo "no $workflow push run appeared for $ref@$sha" >&2
  return 1
}

just branch chore/release-v0.2.0
# update Cargo.toml, apps/terminal/src-tauri/tauri.conf.json, and apps/terminal/package.json
git commit -m "chore(repo): set version 0.2.0   [—]"
just pre-push
git push -u origin HEAD
version_pr=$(gh pr create --base development \
  --title "chore(repo): set version 0.2.0   [—]" \
  --body "Synchronize every release-version source before promotion.")
IFS=$'\t' read -r version_base version_head < <(
  gh pr view "$version_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
bash ./scripts/watch-pr-checks.sh "$version_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$version_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$version_base" ] || [ "$current_head" != "$version_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$version_pr" --match-head-commit "$version_head" --squash --delete-branch

git switch development && git pull --ff-only
development_sha=$(git rev-parse HEAD)
development_ci=$(exact_push_run ci.yml development "$development_sha")
gh run watch "$development_ci" --exit-status

just flow                          # what is between the branches right now
staging_pr=$(gh pr create --base staging --head development \
  --title "promote development to staging" \
  --body-file .github/PULL_REQUEST_TEMPLATE/promotion.md)
IFS=$'\t' read -r staging_base staging_head < <(
  gh pr view "$staging_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
# Copy the template to notes/promotion-staging.md and fill in both SHAs plus evidence.
gh pr edit "$staging_pr" --body-file notes/promotion-staging.md
bash ./scripts/watch-pr-checks.sh "$staging_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$staging_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$staging_base" ] || [ "$current_head" != "$staging_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$staging_pr" --match-head-commit "$staging_head" --merge   # MERGE COMMIT. Not squash. See §6
git switch staging && git pull --ff-only
staging_sha=$(git rev-parse HEAD)
staging_ci=$(exact_push_run ci.yml staging "$staging_sha")
gh run watch "$staging_ci" --exit-status
rc_tag=v0.2.0-rc.1
git tag -s "$rc_tag" -m "Phase 1 groups 1–4"
git push origin "refs/tags/$rc_tag"
rc_release=$(exact_push_run release.yml "$rc_tag" "$staging_sha")
gh run watch "$rc_release" --exit-status
```

Then, after the candidate has actually been used:

```bash
main_pr=$(gh pr create --base main --head staging \
  --title "promote staging to main" \
  --body-file .github/PULL_REQUEST_TEMPLATE/promotion.md)
IFS=$'\t' read -r main_base main_head < <(
  gh pr view "$main_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
# Copy the template to notes/promotion-main.md and fill in both SHAs plus evidence.
gh pr edit "$main_pr" --body-file notes/promotion-main.md
bash ./scripts/watch-pr-checks.sh "$main_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$main_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$main_base" ] || [ "$current_head" != "$main_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$main_pr" --match-head-commit "$main_head" --merge
git switch main && git pull --ff-only
main_sha=$(git rev-parse HEAD)
main_ci=$(exact_push_run ci.yml main "$main_sha")
gh run watch "$main_ci" --exit-status
final_tag=v0.2.0
git tag -s "$final_tag" -m "Phase 1"
git push origin "refs/tags/$final_tag"
final_release=$(exact_push_run release.yml "$final_tag" "$main_sha")
gh run watch "$final_release" --exit-status
gh release view "$final_tag"       # a DRAFT — inspect every artifact before publishing
```

`notes/` is local scratch for the filled-in promotion bodies. It is not in `.gitignore`, so it will
show in `git status` — do not `git add` it. The evidence that matters is in the pull request, not in
a file on your disk.

The explicit identifiers are load-bearing. `gh run watch` without an id may attach to an unrelated
run, and `gh pr merge` without a PR argument may act on whichever branch the CLI happens to infer.
The shared PR watcher derives the exact workflow/job set from the PR's base, head, and complete
changed-path list. It resolves every check link back to the canonical workflow file, event, and
workflow name, so an attacker-controlled workflow with the same display names cannot satisfy the
set. Its immutable snapshot includes both branch-tip SHAs plus a non-revealing fingerprint of the
title/body, so a base advance, synchronization, retarget, or attribution-bearing edit invalidates
old evidence. It waits for all core jobs, conditionally requires
`security / workflow-analysis`, requires all three cross-platform jobs for a `staging` or `main`
target, requires the promotion notice for official-branch and hotfix heads, and refuses if that
snapshot changes while it waits.
Because the current plan cannot require green checks, the human operator records both tips before
the watcher and re-reads both immediately before every merge. Any mismatch discards the evidence
and requires another watcher run. `--match-head-commit` atomically guards only the head SHA during
the merge; this plan/API has no equivalent atomic target-base lock. Serialize maintainer merges—or
temporarily freeze the target branch—during this final window. The immediate base recheck narrows
but cannot eliminate that residual race. The operator then watches the merge result's exact
branch-push CI SHA and only then creates the tag.

Release tags use `git tag -s`. The workflow requires an annotated tag object, a cryptographic
signature, and GitHub's verified signature status; unsigned annotated and lightweight tags are
both refused. Configure and verify the signing identity before attempting the first release.

---

## 3 · What GitHub enforces here, and what the current plan cannot enforce

This repository is **private, on the GitHub Free plan**. That is not a detail; it decides which
of these rules are laws and which are merely written down.

```
$ gh api repos/OmarSweiti/pos/branches/main/protection
403  Upgrade to GitHub Pro or make this repository public to enable this feature.
```

Branch protection **and** rulesets are both gated. Neither is available. So:

| Rule | Control | Honest limit |
|---|---|---|
| No direct, force, or deletion push to `main`/`staging`/`development` | [`.githooks/pre-push`](../../.githooks/pre-push), using Git's supplied destination remote | local; `--no-verify` or an unconfigured clone bypasses it |
| Existing tags never move or disappear | `.githooks/pre-push` allows a new tag but refuses every update/deletion; the release workflow revalidates the remote annotated-tag object around draft mutation | the hook is local, and draft/tag binding is not atomic until immutable publication |
| Commit and squash title obey the exact same grammar | [`scripts/validate-change-title.sh`](../../scripts/validate-change-title.sh), called by `commit-msg` and `branch-flow` | the server check can be merged while red without protection |
| Coding assistants receive no PR or history attribution; the exact Dependabot metadata/trailer combination remains visible | [`scripts/check-automation-attribution.py`](../../scripts/check-automation-attribution.py), called by Git and trusted CI for commits plus the PR title/body | Git author metadata is spoofable, local hooks are bypassable, and CI is evidence rather than a merge wall |
| Protected source plans and committed migrations do not change | Claude/Codex hooks, staged-index policy, and `branch-flow` | `pull_request_target` loads the trusted default-branch definition, policy is checked out at its exact `github.workflow_sha`, and the verified PR head is materialized only as data; a red check still cannot block the administrator |
| Sensitive paths, oversized staged blobs, and Git inspection failures are refused | [`.githooks/pre-commit`](../../.githooks/pre-commit) with NUL-safe staged-index inspection | local only |
| Secret-like content is detected independently of its filename | Gitleaks in pre-commit, pre-push, CI commit-range scanning, and the weekly security workflow | GitHub-native scanning/push protection remains unavailable; local scans can be skipped |
| Tests, lint, domain purity, schema parity, real PostgreSQL, web build, docs, guards and supply-chain policy run | `ci.yml` | visible and logged, but not a required-check wall on this plan |
| The coverage matrix reconciles with the suite, the phase files, normative reference names, and its own arithmetic | [`scripts/check-test-catalog.py`](../../scripts/check-test-catalog.py): `just lint` runs the real reconciliation; `just guards` runs `--self-test` | **local only.** Neither invocation has a step in `ci.yml`, and adding one is a change to the frozen workflow surface that needs its own reviewed edit. It is the only checker in the local lint gate with no CI step, so a push that skipped `just lint` is not caught |
| The release signing key is never on a step that compiles third-party code | **nothing yet.** `release.yml` passes `TAURI_SIGNING_PRIVATE_KEY` and its password to the same step that builds the frontend and the Rust binary | this row is a **requirement, not a control**. [`ref/security-compliance.md`](ref/security-compliance.md) §6b specifies the split — an unsigned job that compiles and reaches the network, then a signing step that receives artifact digests and holds the key with no checkout, no dependency installation and no compilation. Until it lands, any build script or proc macro in the dependency graph can read the key. It is one reason the first external release is deliberately blocked |
| Workflow syntax and Actions security are audited | `security.yml` using actionlint and zizmor | findings are annotations/check failures, not protected-branch requirements |
| Third-party Actions are immutable | every `uses:` is a complete commit SHA; the post-merge policy script enables repository SHA pinning only after a local allowlist/default-head preflight | exact selected-action allow patterns are unavailable for this private user-owned repository, so `allowed_actions` remains the repository's existing mode |
| A release identifies the exact validated branch tip | `release.yml` validates SemVer/RC grammar, annotated tag object, branch tip, versions and successful CI for the same SHA | release signing secrets and OS signing still have to be provisioned before an external release |

`branch-flow.yml` checks its candidate replacement with the trusted revision of
`scripts/check-branch-workflow-policy.rb`. It compares the Git blob and executable mode of every
workflow definition, the labeler and Dependabot configuration, the secret-scanning configuration,
the guarded Just/GitHub setup surface, and the trusted CI gate plus branch/title/attribution/
Actions/label/readiness helpers and their local execution dependencies. It also freezes the exact
tracked set, blob type, content, and executable mode of every Git hook; every repository-wide or
nested `AGENTS.md`, `AGENTS.override.md`, `CLAUDE.md`, and `CLAUDE.local.md`; nested Claude and Codex
skill trees including their support files; every file under any root or subtree `.claude/` policy
tree (including rules, namespaced commands, agents, hooks, output styles, agent memory, and tracked
local settings); every root or subtree `.codex/` policy tree; and the optional root `.mcp.json` and
`.worktreeinclude`. The root `.gitattributes` line-ending/shebang contract is frozen with that
surface. Additions, removals, symlinks (including policy-directory
symlinks), mode changes, and any added local Action are refused, as is adding or removing a
workflow. Ordinary application and test implementation remains outside this exact-byte boundary.
A change to the future policy surface is therefore intentionally red
under the current trusted revision and requires an explicit manual security review before merge.
That red result is the review signal and expected escape hatch on the Free plan; after the reviewed
change lands, its exact `github.workflow_sha` becomes the policy used for later PRs. This friction
prevents a green policy-only PR from silently poisoning the next trusted run; it does not pretend
that a red check can block the administrator on this plan.
The first merge that installs this boundary is necessarily a reviewed bootstrap: a trusted
default-branch checker cannot protect the commit before that checker exists there.

The practical rule remains **`just setup` on every machine, always**. It installs hooks before
networked dependency setup and refuses early if Gitleaks is missing. Server workflows then repeat
the policy from trusted code. This is professional defence in depth, but it is not branch
protection: the current private Free-plan repository cannot require checks, prevent an
administrator push, or activate CODEOWNERS review enforcement. Nothing in this repository claims
that those paid-plan controls were completed.

GitHub-native secret scanning and push protection are also unavailable here. The independent
Gitleaks implementation closes the content-scanning gap without pretending to be the native
product: findings are redacted, the scanner version and downloaded archive digest are pinned in
CI, and operational errors fail closed.

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

Six families. `area:` and `risk:` are applied **by path**
([`.github/labeler.yml`](../../.github/labeler.yml)); `type:` is applied **from the PR title**
([`scripts/pr-type-label.sh`](../../scripts/pr-type-label.sh)).

| Family | Values | Answers |
|---|---|---|
| `type:` | mirrors the commit types | what kind of change — read from the title, not the paths |
| `area:` | mirrors the commit scopes | which crate or app |
| `phase:` | `0`–`5` | which exit gate it belongs to |
| `priority:` | `P0` `P1` `P2` | P0 = wrong money, lost sale, corrupted data, compliance breach |
| `risk:` | `money path` `migration` `security` `compliance` `immutable` | **how it must be reviewed** |
| `needs:` | `merchant answer` `decision` `hardware` | why it is not moving |
| `meta:` | `toolchain gap` `dependencies` `flake` `spike` `accepted risk` | bookkeeping |

**Why `type:` is not path-derived.** It was, from a glob on `docs/**`, and in this repository
that is nearly every PR — §4.13 *requires* the docs a change contradicted to be fixed in the same
commit. So the label was applied almost always and meant almost nothing: PR #9 was a
`chore(repo):` and PR #15 a `fix(domain):`, and both were labelled `type: docs`. The type is the
first word of the title, from a closed list that `commit-msg` and the `branch-flow` check already
enforce — so it was sitting there to be read rather than guessed. `area:` and `risk:` stay
path-derived, where a glob genuinely is the better evidence: touching
`crates/pos-domain/src/money*` **is** the money path, whatever the title says.

Dependabot cannot be configured to add the repository's step suffix. The trusted-base labeler
therefore passes a conforming title unchanged and sends any other title through the shared
validator's tested normalizer. That mode adds `[—]` when no canonical tag is present, removes an
anchored generated directory suffix, then removes an anchored generated group suffix only if the
subject is still overlength. As a last resort it truncates at a clean word boundary, and it validates
its own output before the resulting edit retriggers title validation and type labeling. This is not
a grammar exemption. A commit with the exact Dependabot author name/email may retain the exact
GitHub-generated trailer, but that locally configurable metadata is a compatibility signal, not
authenticated App provenance. Coding assistants are tools and never receive co-author or
generated-by attribution.

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

Milestones collect the tracked delivery items for one of the six phase gates, and nothing else;
their burndown is not a phase-completion percentage. A planned microstep earns its issue only when
work starts (the rule above), while one work PR covers a whole group (§6 below), so unstarted work is absent
from the denominator and tracked items have unequal scope. A milestone bar can therefore read
near-complete while most of a phase is still unbuilt, and the two numbers this paragraph used to
quote had both gone stale before anyone noticed. Read phase progress from the checked frontier
region in [`README.md`](README.md), which
[`scripts/check-implementation-frontier.py`](../../scripts/check-implementation-frontier.py)
reconciles against the phase files that define it; use the milestone only to read the delivery
items GitHub actually tracked.

**`just pr` sets it, from the branch name.** A `phase-<0-5>/...` branch earns the milestone whose
title starts `Phase <n> `, looked up from GitHub so the six titles live only in
[`gh-bootstrap.sh`](../../scripts/gh-bootstrap.sh) and cannot drift into the justfile. The lookup
paginates the complete milestone list and requires exactly one match, whether the milestone came
from the phase name or the explicit third argument. An API failure, zero matches, or duplicate
matches aborts before `just pre-push`; nothing is pushed and no PR is opened. This is not a nicety:
nothing was setting milestones, so all six appeared empty while delivery shipped; even work GitHub
should have tracked was missing.

A branch naming no phase — `chore/`, `docs/`, `refactor/` — intentionally performs no milestone
lookup and earns none; that is correct rather than a gap because a tooling PR is not something a
phase gate waits on. For the exception, a `fix/` that genuinely blocks a gate, pass it:
`just pr '<title>' '' 'Phase 1 — sellable MVP'`.

**Closing and reopening are manual.** Before changing state, inspect every issue and PR assigned to
the milestone, then run the owning phase file's exit gate in full — every command and numbered
demonstration — and complete the per-phase evidence review in
[`02-development-workflow.md`](02-development-workflow.md) §16. Only then attest or withdraw the
gate:

```bash
phase_repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
gh api -X PATCH "repos/$phase_repo/milestones/<n>" -f state=closed
```

If later evidence invalidates that attestation, reopen it:

```bash
phase_repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
gh api -X PATCH "repos/$phase_repo/milestones/<n>" -f state=open
```

[`gh-bootstrap.sh`](../../scripts/gh-bootstrap.sh) must never auto-close a milestone: closure is an
attestation that the exit gate passed, not something item counts can prove. Phase 0 is the worked
example: its milestone is already closed with an adoption note recording closure by transfer.

---

## 5 · The board — one project, four views

`POS delivery`, a Projects v2 board. Free on a personal account, works on private repositories,
and it is the one piece of GitHub's project machinery that is fully available on this plan.

```bash
gh auth refresh -s project,read:project    # once — the default login lacks this scope
just gh-project                            # creates missing fields; refuses schema drift
```

**Live verification note — 27 August 2026:** `just gh-project` created project **#4 `POS delivery`**
on the personal account, then stopped because field inspection also queried a non-existent
organisation. The seven custom fields still await the reviewed re-run; the four views remain a
manual step.

The bootstrap validates exact field types, duplicate names, and every single-select option before
it calls the board ready. A same-named but incompatible field is a blocking manual correction, not
permission to create a duplicate or silently accept a misleading view.

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

`CODEOWNERS` records intended ownership and is ready for a future eligible repository, but it does
not activate automatic review assignment for this private repository on the current plan. Treat it
as maintained governance metadata, not an active control or a review requirement.

---

## 7 · Releases — two channels from one workflow

| Tag | On | Result |
|---|---|---|
| `v0.2.0-rc.1` | `staging` | a **pre-release** draft — the pilot channel |
| `v0.2.0` | `main` | a **production** draft release |

The guard refuses before expensive platform builds unless all of these are true:

- the name is exactly `vX.Y.Z`, `vX.Y.Z-rc.N`, or `vX.Y.Z-beta.N`, without leading zeroes;
- the reference is a signed, annotated tag and GitHub reports the signature as verified;
- a final tag identifies the current `main` tip, or a candidate identifies the current `staging`
  tip—not merely an older ancestor;
- the tag version matches every maintained workspace/application version;
- `ci.yml` completed successfully for that exact SHA as a push on the expected branch.

Each platform build/sign job has only `contents: read` plus the signing secrets it needs. A
separate publisher has the minimal `contents: write` token and no signing secrets. The release
stays draft while the workflow attaches an SPDX JSON SBOM and a SHA-256 manifest over every
application asset and the SBOM.

**Build and sign are still the same step, and that is the one release control this file cannot yet
call done.** A step that compiles a Cargo and pnpm dependency graph runs third-party build scripts
and proc macros by design, and any one of them can read the environment holding the updater key.
The required shape is in §3 and specified in [`ref/security-compliance.md`](ref/security-compliance.md)
§6b.

GitHub release immutability is enabled on the live repository. Once a draft is published, its tag
and assets cannot be silently replaced; a bad build is a **new patch**, never a moved tag.
Drafts do not have that atomic tag/asset binding. The workflow rechecks the exact annotated tag
object and target commit both before and after draft mutations, and the human publication runbook
in [workflow §15](02-development-workflow.md) repeats the check immediately before publishing.
There is still an unavoidable final instruction-sized race until immutable publication completes;
a failed workflow or recheck means the draft must not be published.
Release failures must use **Re-run all jobs**, never a failed-job or single-job rerun: artifact
names isolate `github.run_attempt`, and the publisher refuses to mix platform/SBOM artifacts from
different attempts. The exact command and publication gate are in
[workflow §15](02-development-workflow.md).

The pipeline is intentionally not ready to publish an external installer yet. It remains blocked
until a human configures verified tag signing, the repository updater-signing secrets and updater
public configuration, OS code signing/notarisation (5.5.1), the signing/build split above, and a
restore path that has actually been exercised. See [workflow §15](02-development-workflow.md) and
[`../../SECURITY.md`](../../SECURITY.md).

### A hotfix

The `exact_push_run` helper is defined in §2. A hotfix changes the patch version in the same PR;
otherwise the signed tag cannot pass the release workflow's synchronized-version check.

```bash
set -euo pipefail

git switch main && git pull --ff-only
git switch -c hotfix/receipt-total-drift
# Fix and test the defect, then set 0.2.1 in Cargo.toml,
# apps/terminal/src-tauri/tauri.conf.json, and apps/terminal/package.json.
git commit -m "fix(terminal): correct receipt total and set version 0.2.1   [—]"
just pre-push
git push -u origin HEAD
hotfix_pr=$(gh pr create --base main --head hotfix/receipt-total-drift \
  --title "Hotfix receipt total drift in v0.2.1" \
  --body "Corrects the production defect and synchronizes every release-version source.")
IFS=$'\t' read -r hotfix_base hotfix_head < <(
  gh pr view "$hotfix_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
bash ./scripts/watch-pr-checks.sh "$hotfix_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$hotfix_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$hotfix_base" ] || [ "$current_head" != "$hotfix_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$hotfix_pr" --match-head-commit "$hotfix_head" --merge   # merge commit

git switch main && git pull --ff-only
main_sha=$(git rev-parse HEAD)
main_ci=$(exact_push_run ci.yml main "$main_sha")
gh run watch "$main_ci" --exit-status
hotfix_tag=v0.2.1
git tag -s "$hotfix_tag" -m "receipt total drift"
git push origin "refs/tags/$hotfix_tag"
hotfix_release=$(exact_push_run release.yml "$hotfix_tag" "$main_sha")
gh run watch "$hotfix_release" --exit-status

# then, immediately, so every long-lived branch receives the same fix:
staging_backmerge_pr=$(gh pr create --base staging --head main \
  --title "back-merge v0.2.1 to staging" \
  --body "Carry the verified production hotfix back to staging.")
IFS=$'\t' read -r staging_backmerge_base staging_backmerge_head < <(
  gh pr view "$staging_backmerge_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
bash ./scripts/watch-pr-checks.sh "$staging_backmerge_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$staging_backmerge_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$staging_backmerge_base" ] || [ "$current_head" != "$staging_backmerge_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$staging_backmerge_pr" \
  --match-head-commit "$staging_backmerge_head" --merge

git switch staging && git pull --ff-only
staging_sha=$(git rev-parse HEAD)
staging_ci=$(exact_push_run ci.yml staging "$staging_sha")
gh run watch "$staging_ci" --exit-status

development_backmerge_pr=$(gh pr create --base development --head staging \
  --title "back-merge v0.2.1 to development" \
  --body "Carry the verified production hotfix through staging to development.")
IFS=$'\t' read -r development_backmerge_base development_backmerge_head < <(
  gh pr view "$development_backmerge_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
bash ./scripts/watch-pr-checks.sh "$development_backmerge_pr"
IFS=$'\t' read -r current_base current_head < <(
  gh pr view "$development_backmerge_pr" --json baseRefOid,headRefOid \
    --jq '[.baseRefOid, .headRefOid] | @tsv'
)
if [ "$current_base" != "$development_backmerge_base" ] || [ "$current_head" != "$development_backmerge_head" ]; then
  echo "PR base/head changed; discard the evidence and re-run the watcher" >&2
  exit 1
fi
gh pr merge "$development_backmerge_pr" \
  --match-head-commit "$development_backmerge_head" --merge

git switch development && git pull --ff-only
development_sha=$(git rev-parse HEAD)
development_ci=$(exact_push_run ci.yml development "$development_sha")
gh run watch "$development_ci" --exit-status
```

The back-merge follows the same adjacent-branch path in reverse: `main → staging → development`.
Skipping either leg means the next promotion can silently revert the hotfix. `branch-flow` checks
both the branch name and head repository for these official paths.

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
- every external Action is pinned to a full commit SHA, and checkouts that do not push disable
  persisted credentials;
- explicit token permissions and timeouts on every workflow/job;
- caching on build jobs;
- real Linux/macOS/Windows Tauri builds on promotion PRs and long-lived release branches, so a tag is not
  the first cross-platform package attempt;
- a weekly security workflow, so actionlint/zizmor and advisory/secret-history checks do not rely
  only on a developer remembering to push;
- Dependabot set to **monthly and grouped**, not daily and per-crate. A daily stream of single-crate
  bumps is both a minutes bill and a review load nobody sustains — and an unread dependency bump
  is how a supply-chain problem arrives politely.

What to watch: tag deliberately. `-rc` tags are for candidates that will actually be installed,
not for every merge to `staging`.

Whether that is *enough* is not known, and estimating it from Linux timings is worthless when one
platform costs ten times another:

> ⚠️ **OPEN — blocks `5.5.1`.** Does the promotion-and-release cadence in this document fit inside
> this plan's monthly Actions allowance once every release runs three real platform builds? Nothing
> here has measured a full release, and the figure above is GitHub's published Free-plan allowance
> rather than an observed bill. Default until answered: the cadence in this section — `-rc` tags
> only for candidates that will actually be installed, `concurrency` cancellation on work branches
> but never on `staging` or `main`, and the release `guard` job ahead of every platform build.
> Owner: `5.5.1`, the first microstep that must ship signed installers on a schedule.
> Source that settles it: this repository's own Actions usage report for the first month that runs
> a complete three-platform release, read against the plan's current included-minutes figure.

The consequence if it does not fit is not a broken build, it is a **stalled release in the last week
of a month** — which is exactly when a merchant-facing fix wants to ship. Read the usage page after
the first three-platform promotion, before planning the second.

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

What is enforced here: [`scripts/check-doc-links.sh`](../../scripts/check-doc-links.sh) is the CI
tree gate for broken cross-references. The agent `PostToolUse` hook runs only after a side effect,
so it cannot refuse a write that already happened; it reports the broken-link state and prevents a
successful agent continuation until the link is corrected. A doc set is only worth its
cross-references.

If a shareable, browsable page is genuinely needed — for a partner or an investor —
[`status-page.html`](status-page.html) is the checked-in local view to share. No publication
workflow or stable hosted URL is configured. It is a *view* of the doc set, never the source: when
the spine changes, the page is corrected from [`00-master-plan.md`](00-master-plan.md), never the
other way round.

---

## 11 · What is deliberately not set up

Honest list, same spirit as [workflow §17](02-development-workflow.md).

| Not set up | Why, and what closes it |
|---|---|
| Branch protection / rulesets | unavailable for this private repository on its current Free plan. Local hooks and checks do not impersonate it; `gh-protect.sh` is retained for a future eligibility change and was not applied |
| Required checks / reviewers | consequences of the same plan limitation. `CODEOWNERS` is maintained metadata, not active automatic assignment or enforcement |
| GitHub Discussions | off. With one developer it is a second inbox. Turn it on when there are pilot merchants with questions |
| Wiki / Pages | unavailable on this plan, and the wrong home for engineering docs anyway — §10 |
| A staging deployment of `apps/server` | there is no hosted environment yet. `staging` currently means "a tagged candidate", not "a running system" |
| Jira | free and connectable, deliberately deferred until someone outside engineering needs it — §9 |
| Protected release environment | excluded with the other paid-plan controls. Release jobs instead separate read-only signing from the minimal write-only publisher |
| Release signing material | verified signed tags, updater secrets/public configuration, and platform signing/notarisation must be configured before the intentionally blocked first external release |
| The signing/build split | the updater key currently reaches the step that compiles third-party code. [`ref/security-compliance.md`](ref/security-compliance.md) §6b specifies the two-job shape that fixes it; it is a workflow change with its own reviewed edit, and it lands before any external release — §3 |
| Signed ordinary commits | optional before external contributors; release tags are a separate required policy |
| GitHub-native secret scanning / push protection | unavailable. Independent Gitleaks scanning runs staged, pre-push, in CI, and weekly; it does not claim to be the native product |
| Exact selected-Action allowlisting | unavailable for this private non-enterprise repository. Full-SHA references and the local repository allowlist apply now; the live SHA-only setting remains pending the required post-merge activation |
| Auto-merge | without required checks, automatically merging would remove the deliberate human green-check decision, so it remains disabled |

Immutable releases **are** configured live. Repository-wide Actions SHA enforcement is different:
the checked-in workflows must first merge to the default branch, then
`./scripts/gh-actions-policy.sh` performs a clean/default-head and local allowlist preflight before
enabling it. Until that post-merge step runs, full-SHA pinning is enforced by the workflow files
and policy checks, not claimed as an already-active GitHub repository setting.

---

## 12 · The setup checklist

For a new machine, or a new developer:

```bash
gh auth login                              # ssh, and the `repo` scope
gh auth refresh -s project,read:project    # once, for the board
git clone git@github.com:OmarSweiti/pos.git && cd pos
gitleaks version                           # install a current v8 release if missing
just setup                                 # hooks/identity/scanner FIRST, then locked deps
just guards                                # prove every guard still refuses
just pre-push                              # prove the machine can go green
```

For the repository itself — idempotent, run again whenever this document changes:

```bash
just gh-bootstrap-dry     # read it first
just gh-bootstrap         # labels, milestones, merge behaviour, default branch
just gh-project           # the board and its fields; then the four views, by hand
./scripts/gh-actions-policy.sh --dry-run  # preflight now; no live mutation
# after this hardened setup is merged on the default branch:
./scripts/gh-actions-policy.sh            # enable and verify GitHub SHA-only Actions
```

Do not run `just gh-protect` on the current plan; branch protection is outside this implementation
and the script is retained only for a future repository eligibility change.

---

*Companion to [`01-conventions.md`](01-conventions.md) and
[`02-development-workflow.md`](02-development-workflow.md). Maintained with the repository: when
the plan, flow, live limitation, or gate changes, this file changes with it.*
