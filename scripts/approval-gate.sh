#!/usr/bin/env bash
# Name the one legal way a pull request may merge under the code-owner-review
# ruleset, or refuse.
#
# code-owner-review requires one approving review from a code owner on every
# flow branch, and GitHub never lets an author approve their own pull request.
# So there are exactly two outcomes worth naming, and this script says which:
#
#   approved  the code owner approved this pull request on GitHub. A recipe may
#             merge it normally — the approval already exists.
#   owner     the authenticated user opened it, so GitHub can never record an
#             approval. The only way through is the logged administrator bypass,
#             and that click is the owner's approval. NO RECIPE AND NO AGENT
#             MAKES IT: every pull request an agent opens is the owner's own to
#             GitHub, so a recipe that bypassed here would let an agent merge its
#             own work unapproved. The calling recipe verifies everything it can,
#             then stops and tells the owner how to merge it on GitHub.
#
# Anything else is refused: someone else's pull request without the owner's
# approval, changes requested, an unresolved review thread, or a branch where
# GitHub reports no review requirement at all — the last is ruleset drift, and a
# merge recipe must not be the thing that quietly works around it. There is no
# override argument. The owner's approval is given on GitHub, not passed here.
#
#   approval-gate.sh <author> <viewer> <review-decision> <unresolved-threads>
#   approval-gate.sh --unresolved-threads <owner/repository> <pr-number>
#   approval-gate.sh --self-test
set -euo pipefail

refuse() {
  printf 'approval-gate: REFUSED — %s\n' "$1" >&2
  shift
  local line
  for line in "$@"; do printf '  %s\n' "$line" >&2; done
  return 1
}

decide() {
  local author="$1" viewer="$2" decision="$3" unresolved="$4"
  if [ -z "$author" ] || [ -z "$viewer" ]; then
    refuse "the pull request's author or the authenticated user is unknown."
    return 1
  fi
  if ! [[ "$unresolved" =~ ^[0-9]+$ ]]; then
    refuse "the unresolved review-thread count '$unresolved' is not a number."
    return 1
  fi
  if [ "$unresolved" -ne 0 ]; then
    refuse "$unresolved review thread(s) are unresolved." \
      "code-owner-review refuses the merge until every thread is resolved on GitHub."
    return 1
  fi
  case "$decision" in
    APPROVED)
      echo approved ;;
    REVIEW_REQUIRED)
      if [ "$author" = "$viewer" ]; then
        echo owner
      else
        refuse "this pull request needs the code owner's approving review." \
          "Read the diff and its evidence comment, approve it on GitHub, then rerun."
        return 1
      fi ;;
    CHANGES_REQUESTED)
      refuse "changes were requested on this pull request." \
        "Address them, then have the review re-approved or dismissed on GitHub."
      return 1 ;;
    "")
      refuse "GitHub reports no review requirement here, but code-owner-review requires one." \
        "That is ruleset drift: diff live against .github/rulesets/ before merging anything."
      return 1 ;;
    *)
      refuse "unknown review decision '$decision'."
      return 1 ;;
  esac
}

# Review threads are not in `gh pr view --json`, so GraphQL counts them. More
# than one page is refused rather than half-read: a count that might be wrong
# is not evidence that nothing is unresolved.
unresolved_threads() {
  local repository="$1" number="$2" owner name result
  [[ "$repository" =~ ^[^/[:space:]]+/[^/[:space:]]+$ ]] \
    || { refuse "'$repository' is not an owner/repository name."; return 1; }
  [[ "$number" =~ ^[0-9]+$ ]] \
    || { refuse "'$number' is not a pull-request number."; return 1; }
  owner=${repository%%/*}
  name=${repository#*/}
  # shellcheck disable=SC2016 # GraphQL variables, not shell expansions.
  result=$(gh api graphql \
    -f query='query($owner: String!, $name: String!, $number: Int!) {
      repository(owner: $owner, name: $name) {
        pullRequest(number: $number) {
          reviewThreads(first: 100) { totalCount nodes { isResolved } }
        }
      }
    }' \
    -F owner="$owner" -F name="$name" -F number="$number" \
    --jq '.data.repository.pullRequest.reviewThreads
          | if .totalCount > 100 then "too-many"
            else ([.nodes[] | select(.isResolved | not)] | length) end') \
    || { refuse "GitHub did not answer the review-thread query."; return 1; }
  [ "$result" != too-many ] \
    || { refuse "more than 100 review threads; resolve them on GitHub and merge there."; return 1; }
  printf '%s\n' "$result"
}

self_test() {
  local passed=0 failed=0 expected author viewer decision unresolved label got
  while IFS='|' read -r expected author viewer decision unresolved label; do
    [ -n "$expected" ] || continue
    if got=$(decide "$author" "$viewer" "$decision" "$unresolved" 2>/dev/null); then
      :
    else
      got=refused
    fi
    if [ "$got" = "$expected" ]; then
      printf '  ok    %s\n' "$label"
      passed=$((passed + 1))
    else
      printf '  FAIL  %s (expected %s, got %s)\n' "$label" "$expected" "$got"
      failed=$((failed + 1))
    fi
  done <<'CASES'
approved|dependabot[bot]|OmarSweiti|APPROVED|0|an approved Dependabot pull request merges normally
owner|OmarSweiti|OmarSweiti|REVIEW_REQUIRED|0|the owner's own pull request merges as the owner's approval
refused|dependabot[bot]|OmarSweiti|REVIEW_REQUIRED|0|an unapproved Dependabot pull request is refused
refused|contributor|OmarSweiti|REVIEW_REQUIRED|0|an unapproved contributor pull request is refused
refused|OmarSweiti|OmarSweiti|REVIEW_REQUIRED|1|an unresolved thread blocks even the owner
refused|dependabot[bot]|OmarSweiti|APPROVED|2|an unresolved thread blocks an approved pull request
refused|OmarSweiti|OmarSweiti|CHANGES_REQUESTED|0|requested changes are refused
refused|OmarSweiti|OmarSweiti||0|a missing review requirement is ruleset drift, refused
refused|OmarSweiti|OmarSweiti|SOMETHING_NEW|0|an unknown review decision is refused
refused||OmarSweiti|APPROVED|0|an unknown author is refused
refused|OmarSweiti||REVIEW_REQUIRED|0|an unknown viewer is refused
refused|OmarSweiti|OmarSweiti|REVIEW_REQUIRED|one|a non-numeric thread count is refused
refused|omarsweiti|OmarSweiti|REVIEW_REQUIRED|0|a near-match login is not the owner
CASES
  printf '\napproval-gate self-test: %d passed, %d failed\n' "$passed" "$failed"
  [ "$failed" -eq 0 ]
}

case "${1:-}" in
  --self-test)
    [ "$#" -eq 1 ] || { echo "usage: $0 --self-test" >&2; exit 2; }
    self_test ;;
  --unresolved-threads)
    [ "$#" -eq 3 ] || { echo "usage: $0 --unresolved-threads <owner/repository> <pr-number>" >&2; exit 2; }
    unresolved_threads "$2" "$3" ;;
  *)
    [ "$#" -eq 4 ] || {
      echo "usage: $0 <author> <viewer> <review-decision> <unresolved-threads>" >&2
      echo "       $0 --unresolved-threads <owner/repository> <pr-number>" >&2
      echo "       $0 --self-test" >&2
      exit 2
    }
    decide "$1" "$2" "$3" "$4" ;;
esac
