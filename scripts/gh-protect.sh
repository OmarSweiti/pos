#!/usr/bin/env bash
# Makes the branch flow unbypassable — on a plan that allows it.
#
# THE SITUATION TODAY: this repository is PRIVATE on the GitHub Free plan, where
# neither branch protection nor rulesets exist. The API answers:
#   403  "Upgrade to GitHub Pro or make this repository public to enable this feature."
# So this script is written, tested against that 403, and waiting. Three ways out,
# in the order they make sense for a commercial product:
#   1. GitHub Pro — $4/month, keeps the repo private. Run this script, done.
#   2. A free GitHub organisation — org-owned PRIVATE repos on the Free plan still
#      do not get protection; only public ones do. Not a way out on its own.
#   3. Make the repository public — not an option for this product.
# Until then: .githooks/pre-push and the branch-flow check are the enforcement.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner) || exit 1
echo "repository: $REPO"

# Written once, applied to each branch. Note what is deliberately NOT set:
#   required_linear_history — false, because a promotion PR merges with a MERGE
#     COMMIT on purpose. Linear history and this flow are incompatible.
#   required_approving_review_count — 0, because there is one developer. It still
#     forces the change through a pull request, which is the point; raise it to 1
#     the day a second developer arrives.
#   restrictions — null. Push restrictions are an organisation feature; a
#     user-owned repository cannot use them.
protect() {  # protect <branch> <enforce_admins> <checks-json>
  local branch="$1" admins="$2" checks="$3"
  echo
  echo "── $branch"
  local body out code
  body=$(cat <<JSON
{
  "required_status_checks": { "strict": true, "contexts": $checks },
  "enforce_admins": $admins,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": true,
    "required_approving_review_count": 0,
    "require_last_push_approval": false
  },
  "restrictions": null,
  "required_linear_history": false,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "block_creations": false,
  "required_conversation_resolution": true
}
JSON
)
  out=$(printf '%s' "$body" | gh api -X PUT "repos/$REPO/branches/$branch/protection" \
        --input - 2>&1)
  code=$?
  if [ "$code" -eq 0 ]; then
    echo "  protected · PR required · checks must pass and be up to date · no force-push · no deletion"
    return 0
  fi
  if printf '%s' "$out" | grep -q 'Upgrade to GitHub Pro'; then
    echo "  NOT APPLIED — GitHub answered 403:"
    echo "    Upgrade to GitHub Pro or make this repository public to enable this feature."
    echo "  This is the expected answer on a private Free-plan repository. Nothing is wrong"
    echo "  with the script; the plan does not sell the feature. Enforcement stays with"
    echo "  .githooks/pre-push and the branch-flow check until the plan changes."
    return 2
  fi
  echo "  FAILED: $out"
  return 1
}

# `rust` and `web` are the job names in ci.yml; `topology` is the job in
# branch-flow.yml. A check name that does not exist blocks every merge forever,
# so these three strings must match the workflows exactly.
CI_CHECKS='["rust","web","topology"]'

gated=0
protect development false "$CI_CHECKS" || gated=$?
protect staging     false "$CI_CHECKS" || gated=$?
protect main        true  "$CI_CHECKS" || gated=$?

echo
if [ "$gated" -eq 2 ]; then
  cat <<'TXT'
Nothing was applied. To close this gap for $4/month:
  1. https://github.com/settings/billing  → upgrade to Pro
  2. ./scripts/gh-protect.sh
  3. verify:  gh api repos/OmarSweiti/pos/branches/main/protection --jq .required_status_checks
TXT
  exit 0     # a known plan limit is not a script failure
fi
echo "Verify what actually applied:"
echo "  gh api repos/$REPO/branches/main/protection --jq '{checks:.required_status_checks.contexts,pr:.required_pull_request_reviews}'"
