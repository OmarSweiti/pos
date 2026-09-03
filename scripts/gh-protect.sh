#!/usr/bin/env bash
# Makes the branch flow unbypassable — and REFUSES TO RUN, deliberately.
#
# THE SITUATION THIS WAS WRITTEN FOR is gone. It was authored while the repository
# was PRIVATE on GitHub Free, where the protection API answered:
#   403  "Upgrade to GitHub Pro or make this repository public to enable this feature."
# Every run was that refusal, which is why three defects below were never noticed:
# the 403 was doing the reviewing.
#
# The repository went PUBLIC on 30 August 2026. The 403 is gone, the PUT below now
# SUCCEEDS, and it would apply a configuration that is wrong in three verified ways:
#
#   1. Its check list omits `guards`, `supply-chain` and — critically —
#      `protected-paths`. Requiring an incomplete set is worse than requiring
#      none: it makes a branch look protected while the checks that refuse an
#      edited migration or a frozen-surface change are not required at all.
#   2. It sets `require_code_owner_reviews: true` with
#      `required_approving_review_count: 0` against a sole-developer CODEOWNERS,
#      and on `main` pairs that with `enforce_admins: true`.
#   3. The legacy branch-protection API cannot express `allowed_merge_methods`.
#      Only a RULESET can, and this flow needs it: `development` must allow squash
#      AND merge, while a promotion into `staging` must be a merge commit.
#
# So this script fails closed until it is rewritten against the rulesets API.
# `.githooks/pre-push` and the branch-flow check remain the enforcement meanwhile.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

cat >&2 <<'REFUSAL'
gh-protect: REFUSED — this script predates the repository going public.

  It was written against a 403 that no longer happens, so its PUT would now
  actually apply. Three verified defects make that unsafe:

    * the required-check list omits guards, supply-chain and protected-paths
    * require_code_owner_reviews with 0 required approvals, sole-developer
      CODEOWNERS, and enforce_admins on main
    * the legacy API cannot express allowed_merge_methods, which this flow
      needs: squash+merge on development, merge-commit only into staging

  The replacement is a ruleset, not a patch to this file. Until it lands,
  .githooks/pre-push and the branch-flow check are the enforcement.
REFUSAL
exit 3

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
