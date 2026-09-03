#!/usr/bin/env bash
# The `type:` label a PR title earns, printed on stdout. Nothing printed means no
# label applies.
#
#   ./scripts/pr-type-label.sh --label 'feat(domain): tax engine   [1.3.4]'   ->  type: feat
#   ./scripts/pr-type-label.sh --self-test
#
# WHY THIS EXISTS. `.github/labeler.yml` derived `type: docs` from a path glob on
# `docs/**`. In this repository that is nearly every PR, because §4.13 *requires*
# the docs a change contradicted to be fixed in the same commit. So the label was
# applied almost always and meant almost nothing: PR #9 was a `chore(repo):` and
# PR #15 a `fix(domain):`, and both were labelled `type: docs`.
#
# The type is not a property of the files touched. It is the first word of the PR
# title — a closed list, already validated by the `branch-flow` check because the
# title becomes the squash commit. So the correct label was sitting in the title
# all along, needing only to be read rather than guessed.
#
# `area:` and `risk:` stay path-derived, where a glob genuinely is the right
# evidence: touching `crates/pos-domain/src/money*` IS the money path, whatever
# the title says.
set -uo pipefail

# Conventions §8. Closed list, and the same one `.githooks/commit-msg` and
# `branch-flow.yml` enforce — if this drifts from those, the label lies.
TYPES='feat|fix|test|docs|chore|refactor|perf'

usage() {
  echo "usage: $0 --label '<pr title>'" >&2
  echo "       $0 --self-test" >&2
  exit 2
}

label_for() {                       # label_for <title>
  local title="$1"
  # `<type>(<scope>): …` — the scope is not checked here; branch-flow owns that.
  if [[ "$title" =~ ^($TYPES)\( ]]; then
    printf 'type: %s\n' "${BASH_REMATCH[1]}"
  fi
  # A promotion or hotfix PR title is free text by design (§6), so it earns
  # nothing rather than being forced into a category.
}

self_test() {
  local pass=0 fail=0
  case_is() {                       # case_is <expected|--none> <title>
    local want="$1" title="$2" got
    got=$(label_for "$title")
    [ "$want" = "--none" ] && want=""
    if [ "$got" = "$want" ]; then
      printf '  ok    %-14s <- %s\n' "${want:-(none)}" "${title:0:52}"; pass=$((pass+1))
    else
      printf '  FAIL  wanted %-12s got %-12s <- %s\n' "${want:-(none)}" "${got:-(none)}" "$title"; fail=$((fail+1))
    fi
  }

  cli_case_is() {                   # cli_case_is <status> <output|--none> <label> <args...>
    local want_status="$1" want="$2" label="$3" got='' status=0
    shift 3
    got=$("$0" "$@" 2>/dev/null) || status=$?
    [ "$want" = "--none" ] && want=""
    if [ "$status" -eq "$want_status" ] && [ "$got" = "$want" ]; then
      printf '  ok    %s\n' "$label"; pass=$((pass+1))
    else
      printf '  FAIL  %s (wanted exit %s/output %q, got exit %s/output %q)\n' \
        "$label" "$want_status" "$want" "$status" "$got"; fail=$((fail+1))
    fi
  }

  echo "pr-type-label.sh — every type in conventions §8"
  case_is "type: feat"     'feat(domain): tax engine, inclusive extraction   [1.3.4]'
  case_is "type: fix"      'fix(db): sale_line qty to milli-units   [1.1.7]'
  case_is "type: test"     'test(fiscal): discount percentage round-trip property   [2.7.6]'
  case_is "type: docs"     'docs(impl): phase 2 fiscal conformance harness   [—]'
  case_is "type: chore"    'chore(repo): harden the agent guards   [—]'
  case_is "type: refactor" 'refactor(sync): split the cursor loop   [—]'
  case_is "type: perf"     'perf(domain): tax over a 200-line basket   [—]'

  echo "pr-type-label.sh — the real regressions this replaces"
  # Both of these were labelled `type: docs` by the path glob. Neither was docs.
  case_is "type: chore"    'chore(repo): enforce §10 logical CSS, and refuse registry credentials   [—]'
  case_is "type: fix"      'fix(domain): restore the prop_ prefix its verify filter depends on   [1.1.1]'

  echo "pr-type-label.sh — earns nothing rather than guessing"
  case_is --none 'promote development to staging'
  case_is --none 'promote staging to main'
  case_is --none 'Revert "chore(repo): bump vite from 7.3.6 to 8.2.1 (#4)"'
  case_is --none 'added the tax engine'
  # A type outside the closed list is not a label. commit-msg refuses the title
  # separately; this must not invent `type: build` to keep it company.
  case_is --none 'build(domain): tax engine   [1.3.4]'
  # A bare type with no scope is not a legal subject either.
  case_is --none 'feat: tax engine   [1.3.4]'

  echo "pr-type-label.sh — explicit mode selection"
  cli_case_is 0 "type: feat" "explicit labeling emits the correct label" \
    --label 'feat(domain): tax engine, inclusive extraction   [1.3.4]'
  cli_case_is 0 --none "a title cannot select the self-test mode" \
    --label '--self-test'
  cli_case_is 0 --none "a title cannot select the label mode" \
    --label '--label'
  cli_case_is 2 --none "a bare title cannot select labeling" \
    'feat(domain): tax engine, inclusive extraction   [1.3.4]'
  cli_case_is 2 --none "the self-test mode refuses extra arguments" \
    --self-test unexpected
  cli_case_is 2 --none "the label mode requires one title" --label

  printf '\n%s passed, %s failed\n' "$pass" "$fail"
  [ "$fail" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  [ "$#" -eq 1 ] || usage
  self_test
  exit $?
fi

if [ "${1:-}" = "--label" ]; then
  [ "$#" -eq 2 ] || usage
  label_for "$2"
  exit $?
fi

usage
