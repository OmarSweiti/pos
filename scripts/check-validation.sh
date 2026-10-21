#!/usr/bin/env bash
# The `validation` check (required on development): a pull request merges only
# with evidence that its exact head commit was run and seen working
# (docs/implementation/03-github-workflow.md §6). The evidence is the comment
# `just evidence` posts, carrying
#
#   <!-- pos-validation-evidence pr=<n> head=<40-hex SHA> evidence=<40-hex SHA> -->
#
# written by the owner, or by a GitHub App the owner owns (the engineering app).
# Evidence for an earlier commit does not count: change the code, validate it
# again. A marker in anyone else's comment does not count either, so it cannot be
# forged by a stranger's reply.
#
# Exempt, and said so in the check's output: promotions and back-merges (the head
# is a flow branch of this repository), because each change they carry was
# validated on its own pull request. Dependabot is NOT exempt: a dependency bump
# changes what runs, and its evidence is a smoke run of the app.
#
#   ./scripts/check-validation.sh <owner/repo> <pr-number>
#   ./scripts/check-validation.sh --self-test
set -euo pipefail

# stdin: {"pr": <pull>, "comments": [<issue comment>…], "owner": "<login>"}
# stdout: "pass <reason>" or "fail <reason>"
verdict() {
  jq -r '
    def countable($owner):
      (.user.login == $owner and .author_association == "OWNER")
      or (.user.type == "Bot" and (.performed_via_github_app.owner.login // "") == $owner);
    def evidence($owner; $number):
      [ .comments[] | select(countable($owner)) | (.body // "")
        | capture("<!-- pos-validation-evidence pr=(?<pr>[0-9]+) head=(?<sha>[0-9a-f]{40}) "; "g")
        | select(.pr == ($number | tostring)) | .sha ];
    .owner as $owner | .pr as $pr | $pr.head.sha as $head
    | evidence($owner; $pr.number) as $seen
    | if ($pr.head.ref | IN("development", "staging", "main"))
        and $pr.head.repo.full_name == $pr.base.repo.full_name then
        "pass a promotion or back-merge: each change it carries was validated on its own pull request"
      elif ($seen | any(. == $head)) then
        "pass validation evidence for \($head[0:7])"
      elif ($seen | length) > 0 then
        "fail the evidence is for \($seen[-1][0:7]), and the pull request has changed since: validate \($head[0:7]) again"
      else
        "fail no validation evidence for \($head[0:7]): run the change against the emulators, then post it with `just evidence`"
      end'
}

check() {
  local repo=$1 number=$2 pr comments result
  pr=$(gh api "repos/$repo/pulls/$number")
  comments=$(gh api --paginate "repos/$repo/issues/$number/comments" | jq -s 'add // []')
  result=$(jq -n --argjson pr "$pr" --argjson comments "$comments" --arg owner "${repo%%/*}" \
    '{pr: $pr, comments: $comments, owner: $owner}' | verdict)
  echo "validation: ${result#* }"
  [ "${result%% *}" = pass ]
}

self_test() {
  local pass=0 fail=0 head old
  head=$(printf '%040d' 1 | tr 0 a)
  old=$(printf '%040d' 2 | tr 0 b)
  pr() { # pr <author> <head ref> [<head repository>] [<number>]
    jq -n --arg author "$1" --arg ref "$2" --arg repo "${3:-o/r}" --arg sha "$head" \
      --argjson number "${4:-7}" \
      '{number: $number, user: {login: $author}, head: {ref: $ref, sha: $sha, repo: {full_name: $repo}},
        base: {repo: {full_name: "o/r"}}}'
  }
  marker() { # marker <sha> [<pr>]
    printf 'What was run, expected and seen.\n<!-- pos-validation-evidence pr=%s head=%s evidence=%s -->\n' \
      "${2:-7}" "$1" "$(printf '%040d' 3)"
  }
  by_owner() { jq -n --arg body "$(marker "$@")" \
    '{user: {login: "o", type: "User"}, author_association: "OWNER", body: $body}'; }
  by_app() { jq -n --arg body "$(marker "$1")" --arg owner "${2:-o}" \
    '{user: {login: "pos-engineering[bot]", type: "Bot"}, author_association: "NONE",
      performed_via_github_app: {owner: {login: $owner}}, body: $body}'; }
  by_stranger() { jq -n --arg body "$(marker "$1")" \
    '{user: {login: "stranger", type: "User"}, author_association: "NONE", body: $body}'; }
  expect() { # expect <pass|fail> <label> <pr> [<comment>…]
    local want=$1 label=$2 pull=$3 got
    shift 3
    got=$(jq -n --argjson pr "$pull" --arg owner o --argjson comments "$(printf '%s\n' "$@" | jq -s .)" \
      '{pr: $pr, comments: $comments, owner: $owner}' | verdict)
    if [ "${got%% *}" = "$want" ]; then printf '  ok      %s\n' "$label"; pass=$((pass + 1))
    else printf '  FAILED  %s (wanted %s, got: %s)\n' "$label" "$want" "$got"; fail=$((fail + 1)); fi
  }
  echo "validation evidence"
  expect pass "the engineering app's evidence for the head" "$(pr 'pos-engineering[bot]' feat/x)" "$(by_app "$head")"
  expect pass "the owner's evidence for the head" "$(pr 'pos-engineering[bot]' feat/x)" "$(by_owner "$head")"
  expect fail "no evidence" "$(pr 'pos-engineering[bot]' feat/x)"
  expect fail "evidence for an earlier commit only" "$(pr 'pos-engineering[bot]' feat/x)" "$(by_app "$old")"
  expect pass "a re-validated head after an earlier one" "$(pr 'pos-engineering[bot]' feat/x)" \
    "$(by_app "$old")" "$(by_app "$head")"
  expect fail "evidence written for another pull request" "$(pr 'pos-engineering[bot]' feat/x)" \
    "$(by_owner "$head" 8)"
  expect fail "a stranger's marker" "$(pr 'pos-engineering[bot]' feat/x)" "$(by_stranger "$head")"
  expect fail "an app someone else owns" "$(pr 'pos-engineering[bot]' feat/x)" "$(by_app "$head" someone-else)"
  expect fail "a Dependabot pull request without evidence: a bump changes what runs" \
    "$(pr 'dependabot[bot]' dependabot/cargo/x)"
  expect pass "a Dependabot pull request with its smoke run" "$(pr 'dependabot[bot]' dependabot/cargo/x)" \
    "$(by_owner "$head")"
  expect pass "a promotion from development" "$(pr 'pos-engineering[bot]' development)"
  expect fail "a fork's branch named development" "$(pr stranger development stranger/r)"
  printf '\ncheck-validation self-test: %s passed, %s failed\n' "$pass" "$fail"
  [ "$fail" -eq 0 ]
}

case "${1:-}" in
  --self-test) self_test ;;
  '' | -*)
    echo "usage: $0 <owner/repo> <pr-number> | --self-test" >&2
    exit 2
    ;;
  *) check "$1" "${2:?usage: $0 <owner/repo> <pr-number>}" ;;
esac
