#!/usr/bin/env bash
# Shapes the GitHub repository to match docs/implementation/03-github-workflow.md.
# Idempotent: run it as often as you like. It creates or updates, never deletes.
#
#   ./scripts/gh-bootstrap.sh            # apply
#   ./scripts/gh-bootstrap.sh --dry-run  # print what it would do
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

DRY=0; [ "${1:-}" = "--dry-run" ] && DRY=1
REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner) || {
  echo "gh is not authenticated. Run: gh auth login"; exit 1; }
echo "repository: $REPO"; [ "$DRY" -eq 1 ] && echo "(dry run — nothing will change)"
echo

run() { if [ "$DRY" -eq 1 ]; then echo "  would: $*"; else "$@" >/dev/null 2>&1; fi }

# ── labels ────────────────────────────────────────────────────────────────
# A label set is a query language. These exist so that "every open money-path
# bug" and "everything blocked on the merchant" are one click, not a memory test.
label() {  # label <name> <colour> <description>
  if [ "$DRY" -eq 1 ]; then echo "  would label: $1"; return; fi
  gh label create "$1" --color "$2" --description "$3" --force >/dev/null 2>&1 \
    && printf '  %s\n' "$1" || printf '  FAILED %s\n' "$1"
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
  existing=$(gh api "repos/$REPO/milestones?state=all" --jq \
             ".[] | select(.title==\"$1\") | .number" 2>/dev/null | head -1)
  if [ -n "$existing" ]; then
    gh api -X PATCH "repos/$REPO/milestones/$existing" -f description="$2" >/dev/null 2>&1 \
      && printf '  updated %s\n' "$1"
  else
    gh api -X POST "repos/$REPO/milestones" -f title="$1" -f description="$2" >/dev/null 2>&1 \
      && printf '  created %s\n' "$1"
  fi
}
milestone "Phase 0 — close-out"      "Finish what is started; make the repository a foundation. 1–2 days."
milestone "Phase 1 — sellable MVP"   "Cash, tax, Arabic receipts. 8–12 weeks."
milestone "Phase 2 — money-grade"    "Cards, refunds, fiscal. 8–10 weeks. Gate blocked by C-1: JoFotara has no sandbox."
milestone "Phase 3 — connected"      "Sync, back office, CRM. 8–10 weeks."
milestone "Phase 4 — depth"          "Promotions, supply, reports. 8–10 weeks."
milestone "Phase 5 — harden & launch" "Certification, signing, launch. 6–10 weeks."

# ── merge behaviour ───────────────────────────────────────────────────────
# Squash for work branches: one commit per group on development, microsteps in
# the body. Merge commits STAY ENABLED because a promotion PR must not be
# squashed — squashing development → staging forks the branches permanently.
# Rebase-merge is off: it is the one button that silently produces a history
# nobody chose.
echo
echo "merge behaviour"
if [ "$DRY" -eq 1 ]; then
  echo "  would: squash ✓  merge-commit ✓  rebase ✗  delete-branch-on-merge ✓  auto-merge ✓"
else
  gh api -X PATCH "repos/$REPO" \
    -F allow_squash_merge=true \
    -F allow_merge_commit=true \
    -F allow_rebase_merge=false \
    -F delete_branch_on_merge=true \
    -F allow_update_branch=true \
    -f squash_merge_commit_title=PR_TITLE \
    -f squash_merge_commit_message=PR_BODY \
    -f merge_commit_title=PR_TITLE \
    -f merge_commit_message=PR_BODY \
    >/dev/null 2>&1 && echo "  squash ✓  merge-commit ✓  rebase ✗  delete-branch ✓" \
                    || echo "  FAILED — check the token has 'repo' scope"
  # allow_auto_merge is deliberately NOT set. The API accepts the field, returns
  # 200, and leaves it false: auto-merge needs required status checks to gate on,
  # and those need branch protection, which this plan does not sell.
fi

# ── Dependabot ────────────────────────────────────────────────────────────
# Alerts and automatic security updates ARE free on a private repository.
# Secret scanning is NOT — it needs Advanced Security and answers 422 here, so
# the only stand-in is .githooks/pre-commit, which is local and bypassable. That
# gap is stated in SECURITY.md rather than papered over.
echo
echo "Dependabot"
if [ "$DRY" -eq 1 ]; then
  echo "  would enable: alerts, automatic security updates"
else
  gh api -X PUT "repos/$REPO/vulnerability-alerts"     >/dev/null 2>&1 && echo "  alerts ✓"
  gh api -X PUT "repos/$REPO/automated-security-fixes" >/dev/null 2>&1 && echo "  automatic security updates ✓"
fi

# ── default branch ────────────────────────────────────────────────────────
echo
echo "default branch"
current=$(gh api "repos/$REPO" --jq .default_branch)
if [ "$current" = "development" ]; then
  echo "  already development"
elif [ "$DRY" -eq 1 ]; then
  echo "  would move the default branch from $current to development"
else
  gh api -X PATCH "repos/$REPO" -f default_branch=development >/dev/null 2>&1 \
    && echo "  moved from $current to development" \
    || echo "  FAILED — does the development branch exist on origin?"
fi

echo
echo "Done. Branch protection is a separate script — and a separate bill:"
echo "  ./scripts/gh-protect.sh"
