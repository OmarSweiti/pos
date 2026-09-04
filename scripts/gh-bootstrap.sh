#!/usr/bin/env bash
# Shapes the GitHub repository to match docs/implementation/03-github-workflow.md.
# Idempotent: run it as often as you like. It creates or updates, never deletes.
#
#   ./scripts/gh-bootstrap.sh            # apply
#   ./scripts/gh-bootstrap.sh --dry-run  # print what it would do
#   ./scripts/test-gh-setup.sh            # mocked failure-path regression suite
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

case "$#:${1:-}" in
  0:) DRY=0 ;;
  1:--dry-run) DRY=1 ;;
  *)
    echo "usage: $0 [--dry-run]" >&2
    exit 2
    ;;
esac
PYTHON="./scripts/run-python.sh"

die() {
  printf 'gh-bootstrap: %s\n' "$*" >&2
  exit 1
}

if ! REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner); then
  die "could not identify the repository; check 'gh auth status' and network access"
fi
[ -n "$REPO" ] && [ "$REPO" != "null" ] \
  || die "GitHub returned an empty repository identifier"
echo "repository: $REPO"; [ "$DRY" -eq 1 ] && echo "(dry run — nothing will change)"
echo

# ── labels ────────────────────────────────────────────────────────────────
# A label set is a query language. These exist so that "every open money-path
# bug" and "everything blocked on the merchant" are one click, not a memory test.
label() {  # label <name> <colour> <description>
  if [ "$DRY" -eq 1 ]; then echo "  would label: $1"; return; fi
  if ! gh label create "$1" --color "$2" --description "$3" --force >/dev/null; then
    die "failed to create or update label '$1'"
  fi
  printf '  %s\n' "$1"
}

echo "labels — type (mirrors the commit types in conventions §8)"
label "type: feat"      "1d76db" "A new capability"
label "type: fix"       "d73a4a" "Wrong behaviour, corrected"
label "type: test"      "0e8a16" "Tests only"
label "type: docs"      "0075ca" "Documentation only"
label "type: chore"     "cfd3d7" "Tooling, deps, gates, housekeeping"
label "type: refactor"  "5319e7" "Same behaviour, better shape"
label "type: perf"      "fbca04" "Measured, not asserted"

echo "labels — area (mirrors the commit scopes)"
label "area: domain"     "c2e0c6" "crates/pos-domain — the pure core"
label "area: db"         "c2e0c6" "crates/pos-db — schema, migrations, repositories"
label "area: sync"       "c2e0c6" "crates/pos-sync — outbox/cursor protocol"
label "area: hardware"   "c2e0c6" "crates/pos-hardware — printer, scanner, terminal"
label "area: fiscal"     "c2e0c6" "JoFotara e-invoicing"
label "area: terminal"   "bfd4f2" "apps/terminal — the register"
label "area: server"     "bfd4f2" "apps/server — sync, auth, reporting"
label "area: backoffice" "bfd4f2" "apps/backoffice — React admin"
label "area: repo"       "ededed" "Workspace, CI, gates, tooling"
label "area: impl"       "ededed" "The implementation doc set"

echo "labels — phase (the exit gate this belongs to)"
label "phase: 0" "eeeeee" "Close-out — make the repository a foundation"
label "phase: 1" "e0d4ff" "Sellable MVP — cash, tax, Arabic receipts"
label "phase: 2" "c9b3ff" "Money-grade — cards, refunds, fiscal"
label "phase: 3" "b092ff" "Connected — sync, back office, CRM"
label "phase: 4" "9771ff" "Depth — promos, supply, reports"
label "phase: 5" "7e50ff" "Harden and launch"

echo "labels — priority"
label "priority: P0" "b60205" "Wrong money, lost sale, corrupted data, or a compliance breach"
label "priority: P1" "ff9f1c" "A cashier cannot complete a normal task"
label "priority: P2" "fef2c0" "Wrong, but there is a workaround"

echo "labels — risk (these change how the PR is reviewed)"
label "risk: money path" "b60205" "Touches money arithmetic — I-1, I-2. Needs a property test"
label "risk: migration"  "d93f0b" "Schema change — forward-only, mirrored on Postgres, data-migration test"
label "risk: security"   "d93f0b" "Auth, secrets, permissions, or the never-log list"
label "risk: compliance" "d93f0b" "PDPL, PCI, GST, or JoFotara. Claims need evidence"
label "risk: immutable"  "5319e7" "Touches a completed-sale path — I-4"

echo "labels — blocked on something outside the code"
label "needs: merchant answer" "fbca04" "Blocked on ref/merchant-decisions.md"
label "needs: decision"        "fbca04" "Blocked on an engineering decision not yet made"
label "needs: hardware"        "fbca04" "Blocked on a physical device"

echo "labels — meta"
label "meta: toolchain gap" "ededed" "A §17 row — a command that cannot work yet"
label "meta: dependencies"  "ededed" "Raised by Dependabot"
label "meta: flake"         "b60205" "A non-deterministic test. Quarantine within the hour"
label "meta: spike"         "d4c5f9" "Time-boxed investigation. Produces a written answer, not code"
label "meta: accepted risk" "ededed" "Deliberately not fixed. The reason is written down"

# ── milestones — one per phase gate ───────────────────────────────────────
echo
echo "milestones"
milestone() {  # milestone <title> <description>
  if [ "$DRY" -eq 1 ]; then echo "  would milestone: $1"; return; fi
  local pages existing
  if ! pages=$(gh api --paginate --slurp \
      "repos/$REPO/milestones?state=all&per_page=100"); then
    die "failed to list milestones; refusing to create a possible duplicate"
  fi
  if ! existing=$(printf '%s' "$pages" | "$PYTHON" -c '
import json
import sys

title = sys.argv[1]
pages = json.load(sys.stdin)
matches = [str(item["number"]) for page in pages for item in page if item.get("title") == title]
if len(matches) > 1:
    raise SystemExit(f"duplicate milestones named {title!r}; resolve them before rerunning")
print(matches[0] if matches else "")
' "$1"); then
    die "could not interpret the milestone list; no milestone was changed"
  fi
  if [ -n "$existing" ]; then
    if ! gh api -X PATCH "repos/$REPO/milestones/$existing" \
        -f description="$2" >/dev/null; then
      die "failed to update milestone '$1'"
    fi
    printf '  updated %s\n' "$1"
  else
    if ! gh api -X POST "repos/$REPO/milestones" \
        -f title="$1" -f description="$2" >/dev/null; then
      die "failed to create milestone '$1'"
    fi
    printf '  created %s\n' "$1"
  fi
}
# Descriptions carry no week range on purpose. An estimate written here is a
# second copy that drifts silently: every one of these understated its phase
# file by up to eight weeks before anything compared them. The phase file owns
# the estimate; this text owns the exit, and points at the file.
#
# A milestone's item count is not a phase-completion percentage — an unstarted
# microstep has no issue, and one PR covers a whole group. See
# docs/implementation/03-github-workflow.md §4. Closing is a human attestation
# that the exit gate passed; this script must never set `state`, and its PATCH
# deliberately sends `description` alone.
milestone "Phase 0 — close-out"      "Historical phase, completed before milestone tracking existed; no retrospective burndown exists. Closed by transfer: updater signing (0.3.2) is owned by microstep 5.5.0. Record: docs/implementation/phase-0-closeout.md."
milestone "Phase 1 — sellable MVP"   "Exit: sell for cash, all day, fully offline, in Arabic, with correct GST and a printed receipt — ten demonstrations with the cable unplugged. Plan and estimate: docs/implementation/phase-1-sellable-mvp.md."
milestone "Phase 2 — money-grade"    "Exit: cards that reconcile, returns that resist fraud, a shift that balances — fourteen demonstrations plus drills. Gate constrained by C-1: JoFotara has no sandbox, so 2.7.0 pins the ISTD specification first. Plan and estimate: docs/implementation/phase-2-money-grade.md."
milestone "Phase 3 — connected"      "Exit: many registers and a back office converge on one truth, and a conflict resolves without a human. Plan and estimate: docs/implementation/phase-3-connected.md."
milestone "Phase 4 — depth"          "Exit: promotions, purchasing and stock counts that survive a real assortment, with reports a merchant trusts. Plan and estimate: docs/implementation/phase-4-depth.md."
milestone "Phase 5 — harden & launch" "Exit: signed installers, a restored backup, a staged rollout with rollback, and certification evidence on file. Plan and estimate: docs/implementation/phase-5-harden-and-launch.md."

# ── merge behaviour ───────────────────────────────────────────────────────
# Squash for work branches: one commit per group on development, microsteps in
# the body. Merge commits STAY ENABLED because a promotion PR must not be
# squashed — squashing development → staging forks the branches permanently.
# Rebase-merge is off: it is the one button that silently produces a history
# nobody chose.
echo
echo "merge behaviour"
if [ "$DRY" -eq 1 ]; then
  echo "  would: squash ✓  merge-commit ✓  rebase ✗  delete-branch-on-merge ✓"
else
  if ! gh api -X PATCH "repos/$REPO" \
      -F allow_squash_merge=true \
      -F allow_merge_commit=true \
      -F allow_rebase_merge=false \
      -F delete_branch_on_merge=true \
      -F allow_update_branch=true \
      -f squash_merge_commit_title=PR_TITLE \
      -f squash_merge_commit_message=PR_BODY \
      -f merge_commit_title=PR_TITLE \
      -f merge_commit_message=PR_BODY \
      >/dev/null; then
    die "failed to configure merge behaviour; check the token has 'repo' scope"
  fi
  echo "  squash ✓  merge-commit ✓  rebase ✗  delete-branch ✓"
  # allow_auto_merge is deliberately NOT set. Auto-merge needs required status
  # checks to gate on, and no ruleset or branch protection is configured, so
  # there is nothing for it to wait for. It stays off by choice rather than by
  # plan limit — 03-github-workflow.md §3 records the decision.
fi

# ── Dependabot ────────────────────────────────────────────────────────────
# Alerts and automatic security updates are enabled below. GitHub-native secret
# scanning and push protection are enabled live on this public repository and are
# not shaped here — SECURITY.md §"Reporting" records that. The independent
# Gitleaks checks over staged changes, pushes, CI and a weekly full-history scan
# remain defence in depth beside the native product, not a substitute for it.
echo
echo "Dependabot"
if [ "$DRY" -eq 1 ]; then
  echo "  would enable: alerts, automatic security updates"
else
  if ! gh api -X PUT "repos/$REPO/vulnerability-alerts" >/dev/null; then
    die "failed to enable Dependabot vulnerability alerts"
  fi
  echo "  alerts ✓"
  if ! gh api -X PUT "repos/$REPO/automated-security-fixes" >/dev/null; then
    die "failed to enable Dependabot automatic security updates"
  fi
  echo "  automatic security updates ✓"
fi

# ── default branch ────────────────────────────────────────────────────────
echo
echo "default branch"
if ! current=$(gh api "repos/$REPO" --jq .default_branch); then
  die "failed to read the current default branch"
fi
[ -n "$current" ] && [ "$current" != "null" ] \
  || die "GitHub returned an empty default branch"
if [ "$current" = "development" ]; then
  echo "  already development"
elif [ "$DRY" -eq 1 ]; then
  echo "  would move the default branch from $current to development"
else
  if ! gh api -X PATCH "repos/$REPO" -f default_branch=development >/dev/null; then
    die "failed to move the default branch; does 'development' exist on origin?"
  fi
  echo "  moved from $current to development"
fi

echo
echo "Done. Branch protection and rulesets are available on this public"
echo "repository and none is configured. ./scripts/gh-protect.sh deliberately"
echo "refuses: it was written against a 403 a public repository no longer"
echo "returns, so its PUT would now apply an incomplete required-check list."
echo "The replacement is a reviewed ruleset — docs/implementation/03-github-workflow.md §3."
