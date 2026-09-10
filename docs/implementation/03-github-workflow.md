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
Because no configured branch protection or ruleset requires green checks, the human operator
records both tips before the watcher and re-reads both immediately before every merge. Any mismatch
discards the evidence and requires another watcher run. `--match-head-commit` atomically guards only
the head SHA during the merge; the documented flow has no equivalent atomic target-base lock.
Serialize maintainer merges—or temporarily freeze the target branch—during this final window. The
immediate base recheck narrows but cannot eliminate that residual race. The operator then watches
the merge result's exact branch-push CI SHA and only then creates the tag.

Release tags use `git tag -s`. The workflow requires an annotated tag object, a cryptographic
signature, and GitHub's verified signature status; unsigned annotated and lightweight tags are
both refused. Configure and verify the signing identity before attempting the first release.

---

## 3 · What GitHub and the repository enforce here

This repository has been **public since 30 August 2026**. Rulesets arrived on `development`,
`staging` and `refs/tags/v*` on **9 September 2026**, and on `main` on **10 September 2026**:

```
$ gh api repos/OmarSweiti/pos/rulesets --jq '.[]|"\(.name) \(.target) \(.enforcement)"'
development-flow     branch  active
main-append-only     branch  active
staging-promotion    branch  active
tags-v-append-only   tag     active

$ gh api repos/OmarSweiti/pos/branches/main/protection
404  Branch not protected
```

That 404 is **not** evidence that `main` is unprotected. It is the *legacy* branch-protection
API, a separate surface from rulesets; `main` is covered by `main-append-only` and this endpoint
cannot see it. Read `rulesets`, not `branches/*/protection`, when answering what is enforced.

`development` and `staging` require a pull request and six passing checks — `rust`, `guards`,
`web`, `supply-chain`, `protected-paths`, `topology` — block force pushes and deletions, and
constrain the merge method: squash or merge on `development`, merge commit only into `staging`.
`refs/tags/v*` is append-only with no bypass actor. All four definitions are checked in under
[`../../.github/rulesets/`](../../.github/rulesets/), which is also where the diff and restore
commands live.

`development-flow` carries `strict_required_status_checks_policy: true` — GitHub's "require
branches to be up to date before merging" — and `staging-promotion` deliberately does **not**. Both
were `false`, for no reason anybody had written down. The argument for `development` is not textual
conflict, which a merge-base check already covers, but the semantic kind it cannot see: two pull
requests can each pass schema parity or the catalog arithmetic against their own snapshot of the
tree and collide only once both are in — a race that needs concurrent pull requests, which is what
`development` takes.

`staging` must stay `false`, and the reason is structural rather than a preference. A promotion
merges `development` into `staging` with a merge commit that lives only on `staging` and is never
merged back, so `staging` is permanently ahead of `development` by one commit per prior promotion.
Strict compares tips rather than trees, so it can never be satisfied by a promotion — PR #148 was
reported `BEHIND` and refused. Turning it on there buys nothing, because `staging` has one inbound
route taken one promotion at a time and no race to lose, and costs a contentless back-merge into the
default branch before every promotion.
[`../../.github/rulesets/README.md`](../../.github/rulesets/README.md) carries the full reasoning
and the honest cost, and is the place to change it rather than here.

Alongside `required_approving_review_count: 0`, both branch rulesets also carry
`require_extra_approval_for_unattributed_changes: true`. §11 records the binding constraint that
makes the zero necessary — a sole developer cannot approve their own pull request — and this flag is
the one way that zero can stop being zero. It is **inert today**: every commit in this repository is
attributed, verified by resolving the last twenty commits to the GitHub account `OmarSweiti` through
`omarswaty4@gmail.com`. It stops being inert the moment a commit arrives whose author email is not
linked to any GitHub account — a machine with a stale `user.email`, a collaborator's first patch, a
web edit under a different address — because that pull request would then demand an approval that a
single maintainer cannot give, and the only way through would be the logged administrator bypass.
That is worth one throwaway pull request to confirm, once, before either of those situations is real
rather than hypothetical; the flag is also new enough that it should be read back from a live `GET`
before these payloads are ever used to restore a ruleset.

`main` is the uneven one, and deliberately so. It carries `deletion` and `non_fast_forward` and
nothing else: those two rules need no status checks, so they could be applied while the reason for
having no *required checks* still holds — main's `ci.yml` predates four of the six required jobs,
so requiring them would leave a `hotfix/*` branch cut from `main` waiting on checks that never
report. Force-pushing or deleting `main` is therefore refused server-side today, while a pull
request is still not required there and no check is a merge wall. Both arrive when a promotion
carries the current `ci.yml` onto `main`.

Two limits remain and are the reason the table below keeps its "honest limit" column. The **admin
holds `bypass_mode: "pull_request"`** on all three branch rulesets, so a red check is still
mergeable by the maintainer — through a pull request only, never a direct push, and the bypass is
logged as an event. And the git hooks stay local and bypassable. So:

| Rule | Control | Honest limit |
|---|---|---|
| No direct, force, or deletion push to `main`/`staging`/`development` | server-side on all three (`non_fast_forward` and `deletion` everywhere; a pull request is additionally required on `development` and `staging`); [`.githooks/pre-push`](../../.githooks/pre-push) for all three, using Git's supplied destination remote | a *direct* push to `main` is not refused server-side — only a force push or a deletion is, because `main` has no `pull_request` rule — so for that case the hook is still the only control, and `--no-verify` or an unconfigured clone bypasses it |
| Existing tags never move or disappear | server-side for `refs/tags/v*` (`tags-v-append-only`: `update` and `deletion` blocked, **no bypass actor**, so it binds the maintainer too); `.githooks/pre-push` allows a new tag but refuses every update/deletion; the release workflow revalidates the remote annotated-tag object around draft mutation | the ruleset covers `v*` only — any other tag name is hook-only — and draft/tag binding is not atomic until immutable publication |
| Commit and squash title obey the exact same grammar | [`scripts/validate-change-title.sh`](../../scripts/validate-change-title.sh), called by `commit-msg` and `branch-flow` | `topology` is a required check on `development` and `staging`, so a red title check now blocks the merge button there; the admin can still bypass through a pull request, and that bypass is logged |
| Coding assistants receive no PR or history attribution; the exact Dependabot metadata/trailer combination remains visible | [`scripts/check-automation-attribution.py`](../../scripts/check-automation-attribution.py), called by Git and trusted CI for commits plus the PR title/body | Git author metadata is spoofable and local hooks are bypassable; `protected-paths` is now a required check on `development` and `staging`, so CI is a merge wall there, subject to the logged admin bypass |
| Protected source plans and committed migrations do not change | Claude/Codex hooks, staged-index policy, and `branch-flow` | `pull_request_target` loads the trusted default-branch definition, policy is checked out at its exact `github.workflow_sha`, and the verified PR head is materialized only as data; no configured server-side rule makes a red check a merge wall |
| Sensitive paths, oversized blobs, and Git inspection failures are refused | [`.githooks/pre-commit`](../../.githooks/pre-commit) with NUL-safe staged-index inspection, and [`.githooks/pre-push`](../../.githooks/pre-push) again over every commit a push introduces (`check-staged-policy.py --commits-file`) | local only, and deliberately twice: Git runs `pre-commit` for an ordinary commit but not for a clean merge, a cherry-pick, a revert or `rebase --continue`. `check-protected-paths.sh` backstops the plan and migration rules server-side; it inspects no filename class and no blob size, so these two are the refusals with no server-side equivalent at all |
| Secret-like content is detected independently of its filename | GitHub-native secret scanning and push protection are enabled; Gitleaks runs in pre-commit, pre-push, CI commit-range scanning, and the weekly security workflow | local scans can be skipped, so the native controls remain an independent backstop rather than a substitute for the repository-owned range and history gates |
| Tests, lint, domain purity, schema parity, real PostgreSQL, web build, docs, guards and supply-chain policy run | `ci.yml` | `rust`, `guards`, `web` and `supply-chain` are required checks on `development` and `staging`, so a red one blocks the merge button there; `main` has no ruleset, and the administrator keeps a logged pull-request bypass |
| The coverage matrix reconciles with the suite, the phase files, normative reference names, and its own arithmetic | [`scripts/check-test-catalog.py`](../../scripts/check-test-catalog.py): `just lint` runs the real reconciliation; `just guards` runs `--self-test` | the `rust` job runs the reconciliation and `guards` runs `--self-test`, so a push that skipped `just lint` is still caught. The reconciliation runs inside the `rust` job and `--self-test` inside `guards`, both of which are required checks on `development` and `staging` |
| The release signing key is never on a step that compiles third-party code | **still nothing for the same-step problem.** `release.yml` passes `TAURI_SIGNING_PRIVATE_KEY` and its password to the same step that builds the frontend and the Rust binary. What *is* now controlled is the ref surface: the `build` job declares `environment: release`, whose one deployment policy is `tag: v*`, so the key must be an environment secret and a run on any other ref cannot obtain it — and each run leaves a deployment record. That environment has no checked-in definition and withholds nothing yet, because no secret exists to withhold; the subsection below has the three qualifications in full | this row is a **requirement, not a control**. [`ref/security-compliance.md`](ref/security-compliance.md) §6b specifies the split — an unsigned job that compiles and reaches the network, then a signing step that receives artifact digests and holds the key with no checkout, no dependency installation and no compilation. Until it lands, any build script or proc macro in the dependency graph can read the key. It is one reason the first external release is deliberately blocked |
| Workflow syntax and Actions security are audited | `security.yml` using actionlint and zizmor | findings are annotations and check failures. `workflow-analysis` is deliberately **not** a required check: `security.yml` is path-filtered, so a PR touching nothing under `.github/**` produces no check run at all and a required-but-absent context would block it forever |
| Third-party Actions are immutable in tracked workflows | every external `uses:` is a complete commit SHA from the repository allowlist, enforced by repository policy | repository-wide Action selection and SHA settings are separate live configuration; `gh-actions-policy.sh` owns their checked post-merge activation |
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
That red result is the review signal, and on `development` and `staging` the ruleset now makes it
a merge wall as well — subject to the logged administrator bypass, which is why an explicit human
review still authorises the change rather than a green check alone. After it lands, its exact
`github.workflow_sha` becomes the policy used for later PRs. This friction prevents a green
policy-only PR from silently poisoning the next trusted run; it does not pretend that a red check
can block the administrator while no server-side rule requires it.
The first merge that installs this boundary is necessarily a reviewed bootstrap: a trusted
default-branch checker cannot protect the commit before that checker exists there.

The practical rule remains **`just setup` on every machine, always**. It installs hooks before
networked dependency setup and refuses early if Gitleaks is missing. Server workflows then repeat
the policy from trusted code. This is professional defence in depth, but it is not branch
protection: `main` has no ruleset, and on `development`/`staging` the administrator keeps a bypass,
or makes CODEOWNERS a review requirement. Nothing in this repository claims those server-side
controls were completed.

### Live configuration that no file in this repository holds

The four rulesets have checked-in definitions. Three other controls do not, and they are the ones
worth naming explicitly, because [`../../scripts/check-branch-workflow-policy.rb`](../../scripts/check-branch-workflow-policy.rb)
freezes **files** — a setting changed in the web UI is not a diff, produces no red check, and is
invisible to every gate this repository owns.

**CodeQL default setup is enabled, and it is configured entirely server-side.** There
is no analysis workflow under [`../../.github/workflows/`](../../.github/workflows/) and there
cannot be one without converting to advanced setup, so nothing in this repository — no policy
script, no pin, no review — reaches it. It runs on **two** cadences, and the second is easy to miss
because only the first appears in the live configuration: a weekly cron, *and* an analysis on every
pull request, which arrives as four `Analyze (…)` check runs per language pack. PR #146 carried
`Analyze (actions)`, `Analyze (javascript-typescript)`, `Analyze (python)` and `Analyze (ruby)`
alongside the six required contexts. None of them is a required check, so a CodeQL finding
annotates and does not block. For the weekly half,
[`02-development-workflow.md`](02-development-workflow.md) §16's recency check covers `security`,
`proptest-scheduled` and `cross-platform-canary` by filename and cannot cover this one the same
way, because there is no file in the repository to name and its cadence is GitHub's to keep or
change. Live configuration reads `state: configured`,
`query_suite: default`, `threat_model: remote`, `schedule: weekly`, with languages `actions`,
`javascript`, `javascript-typescript`, `python`, `ruby` and `typescript`. Two consequences follow
from that language list and both matter:

- The `actions` pack means CodeQL independently analyses every workflow file in the repository,
  which partly overlaps what zizmor already does in `security.yml`'s `workflow-analysis` job. The
  overlap is not waste — two engines with different query sets — but it is the reason a workflow
  finding may arrive twice, from two places, only one of which this repository controls.
- **Rust is not in the list.** Whether default setup can be made to include it was not
  established — the sweep's attempt to `PATCH` the language list was not carried out — so treat
  "add Rust" as an untested option rather than a closed door. As configured today, `pos-domain` — the
  crate that holds the money rules, and the only code in this project where an arithmetic mistake
  is a financial one — is not analysed by CodeQL at all. Its coverage story is Clippy under
  `-D warnings`, the forbidden float lint, and the property suite; not this.

It is also the largest single consumer of the shared Actions cache: **81 of the 100 entries and
0.709 GB**, written by a job this repository cannot add a `save-if` to. Any future cache arithmetic
has to start from that total rather than from the workflows alone. Pruning the language list would
reclaim none of it — all 81 keys parse to exactly three distinct values, `javascript` 27,
`python` 27 and `ruby` 27 — so the unused entries in the list cost nothing and there is nothing to
trim.

**The `release` environment is the only ref-surface control over the updater signing key.**
[`../../.github/workflows/release.yml`](../../.github/workflows/release.yml)'s `build` job declares
`environment: release`, and that environment's live deployment policy is exactly one entry,
`{"name": "v*", "type": "tag"}`. So an environment secret placed there can be read by a run on a
`v*` tag and by nothing else, and every such run leaves a deployment record. Three honest
qualifications belong with that:

- It has **no checked-in record.** The four rulesets got one under
  [`../../.github/rulesets/`](../../.github/rulesets/); this did not. There is nothing to diff it
  against and nothing to restore it from.
- A flip of that policy to "All branches" in the web UI would be **invisible** to every gate here,
  for the reason at the top of this subsection: the policy checker freezes files, not server state.
- It **guards nothing today.** Both `actions/secrets` and `environments/release/secrets` report
  `total_count: 0`, and `release.yml` fails closed on an empty key, so there is currently no secret
  for the policy to withhold. It becomes load-bearing the moment microstep 5.5.0/5.5.1 provisions a
  real updater keypair — which is the point at which the missing checked-in record stops being
  bookkeeping.

**Five repository security settings: three enabled, two still off.** Secret scanning, push
protection and Dependabot security updates are enabled. `secret_scanning_non_provider_patterns` and
`secret_scanning_validity_checks` are **disabled**, and both are free on a public repository, which
makes leaving them off a pure loss. They are recorded here as an open item rather than a completed
one, because the Actions hardening sweep could not close them: a `PATCH` to
`/repos/{owner}/{repo}` carrying both fields returns `200` with the payload **unchanged**, so the
REST surface accepts the request and silently ignores those two keys. They appear to be settable
only from Settings → Advanced Security, by hand.

They are worth the two clicks. Provider patterns match recognisable third-party token shapes, which
is the easy half; non-provider patterns are what catch generic private keys and database connection
strings — precisely the class [`../../.claude/rules/security.md`](../../.claude/rules/security.md)
names as never-log, never-commit (`db_key`, `wrapped_key`, `POS_DB_KEY`), and precisely the class a
`v*` release will eventually carry real material for. Validity checks ask the provider whether a
found credential is still live, which turns a finding into a triage decision instead of a question.
Verify with `gh api repos/:owner/:repo --jq '.security_and_analysis'` and update this paragraph from
that output, not from an intention.

Independent Gitleaks remains defence in depth rather than a substitute: findings are redacted, the
scanner version and downloaded archive digest are pinned in CI, and operational errors fail closed.

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
its own output.

That edit **retriggers nothing.** GitHub fires no workflow event for a `GITHUB_TOKEN`-authored
action, so `branch-flow` never re-ran and stayed on the title in the stored event payload — which is
why every Dependabot pull request was red on `topology` and why a re-run did not help: a re-run
replays the same payload. Merged PRs #81 and #82 show it, reporting the pre-normalization title
while the merged title carried its tag. The title check therefore reads the title **live** from the
REST endpoint, and for the exact author `dependabot[bot]` it accepts a title the trusted normalizer
accepts, because the write-scoped labeler owns the canonical edit and the two workflows race. A
human's merely-normalizable title is still refused: the PR title becomes the commit subject, and
`just merge` would refuse it anyway. This is not a grammar exemption. A commit with the exact Dependabot author name/email may retain the exact
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

`POS delivery`, a Projects v2 board on the maintainer's personal account. `just gh-project` owns its
checked schema; a view's grouping and sorting are the one part no API can set, so they stay a manual
step.

```bash
gh auth refresh -s project,read:project    # once — the default login lacks this scope
just gh-project                            # creates missing fields; refuses schema drift
```

**Live verification note — 8 September 2026:** project **#4 `POS delivery`** exists on the personal
account with all seven custom fields at the types and select options
[`gh-project.sh`](../../scripts/gh-project.sh) declares, all four views, the repository linked, and
13 items — **10 active and 3 archived** since 9 September, when the closed cards were archived by
explicit `archiveProjectV2Item` calls. There is no standing rule doing that: the built-in
*Auto-archive items* workflow is UI-only, like the view grouping above. Enumerating the whole
GraphQL mutation root returns 260 mutations and the only one naming a workflow is
`deleteProjectV2Workflow`, so a project workflow can be deleted through the API but never created
or enabled. The 27 August run stopped because field inspection queried a non-existent organisation;
`7400a12`, landed the same day, made the script resolve the owner through `repositoryOwner(login:)`
with fragments on both `User` and `Organization`, and the field query now returns clean — the
mutating recipe itself has not been re-run, so a reviewed re-run remains unproven.

What is **not** set is each view's grouping and sorting. `createProjectV2View` accepts only
`projectId`, `name`, `layout` and `configuration`; `updateProjectV2View` adds `filter`; and
`ProjectV2ViewConfigurationInput` exposes **only** `visibleFieldIds`. There is no group-by or sort-by
input anywhere in the schema, so those two remain clicks — `Phase plan`'s in particular. Every
view's name, layout and filter already match the table below.

**Auto-add does not work, and it is the one automation that must be configured by hand.** Issues
#68–#71, #110–#115 and #119–#120 were all created and none reached the board until added explicitly
with `gh project item-add`. The other built-in workflows do work: "Item closed" moved #110 and #117
to `Done` on merge with no intervention. Until Auto-add is configured, **add every new issue by
hand**.

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

Enable the three built-in workflows (project → ⋯ → Workflows) — they remove the step everyone
forgets: *item closed* → Done, *PR merged* → Done, *auto-add* new open issues.

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

`CODEOWNERS` records intended ownership. The enforced review path does not depend on automatic
assignment or a CODEOWNERS review requirement; treat it as maintained governance metadata, not
evidence that a review occurred.

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

## 8 · Actions work is still worth controlling

GitHub-hosted standard-runner minutes are not metered for this public repository, so the former
2,000-minute monthly budget and operating-system multipliers do not govern this workflow. Runtime
still matters: redundant three-platform builds delay useful evidence and consume runner capacity.

What is already done about it:

- `concurrency` groups on `ci`, so a superseded work-branch run stops consuming runner capacity —
  but **never** on `staging` or `main`, where a half-cancelled build tells you nothing;
- the release `guard` job, so a mistagged commit is refused before three platform builds;
- every external Action is pinned to a full commit SHA, and checkouts that do not push disable
  persisted credentials. Only the SHA executes; the trailing comment is documentation, and it must
  name a **tag that resolves to that exact SHA today**. A branch name there is false as soon as the
  upstream branch moves, so zizmor's `ref-version-mismatch` audit reds the next `.github/**` pull
  request over an upstream commit rather than over anything in the diff;
- explicit token permissions and timeouts on every workflow/job;
- caching on build jobs;
- real Linux/macOS/Windows Tauri builds on promotion PRs and long-lived release branches, so a tag is not
  the first cross-platform package attempt;
- a weekly security workflow, so actionlint/zizmor and advisory/secret-history checks do not rely
  only on a developer remembering to push;
- Dependabot set to **monthly and grouped**, not daily and per-crate. A daily stream of single-crate
  bumps is both a queue and a review load nobody sustains — and an unread dependency bump is how a
  supply-chain problem arrives politely.

What to watch: tag deliberately. `-rc` tags are for candidates that will actually be installed,
not for every merge to `staging`.

The constraint to measure at `5.5.1` is elapsed release time and reliability, not an included-minutes
budget. Keep the same cadence — candidate tags only for builds that will actually be installed,
concurrency cancellation on work branches but never on `staging` or `main`, and the release `guard`
job before every platform build — then measure it against the first complete three-platform release.
A superseded run no longer threatens a monthly allowance, but a slow or flaky release still delays a
merchant-facing fix.

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

Whether GitHub Wikis or Pages are enabled is not the architectural decision here:

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
workflow is configured in this repository. It is a *view* of the doc set, never the source: when
the spine changes, the page is corrected from [`00-master-plan.md`](00-master-plan.md), never the
other way round.

---

## 11 · What is deliberately not set up

Honest list, same spirit as [workflow §17](02-development-workflow.md).

| Not set up | Why, and what closes it |
|---|---|
| A ruleset on `main` | `development` and `staging` are configured (§3); `main` is deliberately not, because its `ci.yml` predates four of the six required contexts, so a `hotfix/*` branch cut from it would wait forever on checks that never report. Closed by a promotion carrying the current `ci.yml`, then the third payload |
| Required reviewers | no ruleset requires an approval, and none can: a sole developer cannot approve their own pull request, so `required_approving_review_count` is `0` everywhere. `CODEOWNERS` stays maintained metadata rather than a merge control. Six checks *are* required on `development` and `staging`, subject to a logged administrator bypass — §3 |
| GitHub Discussions | off. With one developer it is a second inbox. Turn it on when there are pilot merchants with questions |
| Wiki / Pages publication | no publication workflow is configured, and none is wanted: a wiki is editable with no pull request and `check-doc-links.py` would not gate it, which would put a hole in the discipline the rest of the doc set depends on. Engineering docs stay versioned and reviewed with the code — §10 |
| A checked-in definition of the three rulesets | **partly closed.** [`.github/rulesets/`](../../.github/rulesets/) now carries the applicable configuration of each live ruleset, normalized to the payload the API accepts back, so the configuration can be diffed and restored by hand — the commands are in its README, and all three matched live when they were written. What is still missing is enforcement of that agreement: no gate runs the diff, so drift is detectable rather than prevented, and the payloads have not been round-tripped. `scripts/gh-protect.sh` still refuses and exits 3 because it predates the rulesets API; rewriting it to apply these files, with the negative tests `scripts/test-gh-setup.sh` requires, is the remaining work. Note issue #115 is **closed** — it tracked "no ruleset exists", which the three rulesets settled; the diff-and-restore half it also named outlived it, which is why this row does not point at it |
| A staging deployment of `apps/server` | there is no hosted environment yet. `staging` currently means "a tagged candidate", not "a running system" |
| Jira | free and connectable, deliberately deferred until someone outside engineering needs it — §9 |
| Protected release-environment enforcement | not claimed. Release jobs instead separate read-only signing from the minimal write-only publisher |
| Release signing material | verified signed tags, updater secrets/public configuration, and platform signing/notarisation must be configured before the intentionally blocked first external release |
| The signing/build split | the updater key currently reaches the step that compiles third-party code. [`ref/security-compliance.md`](ref/security-compliance.md) §6b specifies the two-job shape that fixes it; it is a workflow change with its own reviewed edit, and it lands before any external release — §3 |
| Signed ordinary commits | optional before external contributors; release tags are a separate required policy |
| Repository-level selected-Action allowlisting | its live capability and state are not asserted here. Every tracked `uses:` reference is a full SHA and policy checks enforce that; repository-wide configuration remains the separate checked post-merge step below |
| Auto-merge | enabled 9 September 2026, once six checks became required on `development`. `gh pr merge --auto --squash` waits for the required set, which includes the `topology` and `protected-paths` walls, so it does not bypass the route, title and attribution validation `just merge` performs locally — it defers the merge to them |

Immutable releases **are** configured live. Repository-wide Actions settings are separate live
configuration: after the checked-in workflows merge to the default branch,
`./scripts/gh-actions-policy.sh` performs a clean/default-head and repository-allowlist preflight
before applying them. Independently of that live setting, full-SHA pinning is enforced by the
workflow files and policy checks.

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
just gh-project           # the board and its fields; views by hand (grouping has no API)
./scripts/gh-actions-policy.sh --dry-run  # preflight now; no live mutation
# after this hardened setup is merged on the default branch:
./scripts/gh-actions-policy.sh            # enable and verify GitHub SHA-only Actions
```

`just gh-protect` now refuses and exits 3, and that refusal is the point. The script was written
while the repository was private on GitHub Free, where the protection API answered 403 on every
call — so the 403 was doing the reviewing, and three defects went unnoticed. Going public removed
it: the `PUT` would now succeed and apply a required-check list omitting `guards`, `supply-chain`
and `protected-paths`, which makes a branch look protected while the checks that refuse an edited
migration are not required at all. It also cannot express `allowed_merge_methods`, which this flow
needs. The replacement is a **ruleset**, not a patch to that file.

---

*Companion to [`01-conventions.md`](01-conventions.md) and
[`02-development-workflow.md`](02-development-workflow.md). Maintained with the repository: when
the plan, flow, live limitation, or gate changes, this file changes with it.*
