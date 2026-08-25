#!/usr/bin/env bash
# The one grammar for commit subjects and squash-merge PR titles.
#
#   <type>(<scope>): <summary>   [<step>]
#
# A step is one microstep, an inclusive microstep range written with an en dash,
# or an em dash when the work has no implementation-plan step. Dependabot and
# other repository automation use the same grammar and `[—]`; automation is not
# a reason to make repository history less searchable.
set -euo pipefail

types='feat|fix|test|docs|chore|refactor|perf'
scopes='domain|db|sync|hardware|fiscal|terminal|server|backoffice|repo|impl'
step='—|[0-9]+\.[0-9]+\.[0-9]+(–[0-9]+\.[0-9]+\.[0-9]+)?'
grammar="^($types)\\(($scopes)\\): ([^[:space:]].*[^[:space:]]|[^[:space:]])[[:space:]]+\\[($step)\\][[:space:]]*$"

usage() {
  echo "usage: $0 '<title>'" >&2
  echo "       $0 --self-test" >&2
  exit 2
}

fail() {
  echo "title-policy: REFUSED — $1" >&2
  echo >&2
  echo "  <type>(<scope>): <summary>   [<step>]" >&2
  echo "  type  ∈ $types" >&2
  echo "  scope ∈ $scopes" >&2
  echo "  step  = N.N.N, N.N.N–N.N.N, or —" >&2
  echo >&2
  echo "  got: ${title:-<none>}" >&2
  return 1
}

component_number() {
  # Force decimal interpretation so a zero-padded component never becomes
  # accidental octal under Bash arithmetic.
  printf '%d' "$((10#$1))"
}

range_is_ordered() {
  local value="$1" start end sa sb sc ea eb ec
  case "$value" in
    *–*) ;;
    *) return 0 ;;
  esac

  start="${value%%–*}"
  end="${value#*–}"
  IFS=. read -r sa sb sc <<< "$start"
  IFS=. read -r ea eb ec <<< "$end"
  sa=$(component_number "$sa"); sb=$(component_number "$sb"); sc=$(component_number "$sc")
  ea=$(component_number "$ea"); eb=$(component_number "$eb"); ec=$(component_number "$ec")

  (( sa < ea )) && return 0
  (( sa > ea )) && return 1
  (( sb < eb )) && return 0
  (( sb > eb )) && return 1
  (( sc <= ec ))
}

validate() {
  title="$1"

  case "$title" in
    *$'\n'*|*$'\r'*) fail "the title must be exactly one line."; return 1 ;;
  esac

  if [[ ! "$title" =~ $grammar ]]; then
    fail "use the complete type, scope, summary, and canonical step tag."
    return 1
  fi

  local type="${BASH_REMATCH[1]}" scope="${BASH_REMATCH[2]}"
  local summary="${BASH_REMATCH[3]}" step_value="${BASH_REMATCH[4]}"
  local subject="$type($scope): $summary"

  if [ "${#subject}" -gt 72 ]; then
    fail "the subject before the step tag is ${#subject} characters; the limit is 72."
    return 1
  fi
  case "$summary" in
    *.) fail "do not end a change summary with a period."; return 1 ;;
  esac
  if ! range_is_ordered "$step_value"; then
    fail "the end of a step range must not precede its start."
    return 1
  fi
}

self_test() {
  local pass=0 fail_count=0

  case_is() {
    local want="$1" label="$2" candidate="$3" got=0
    validate "$candidate" >/dev/null 2>&1 || got=$?
    if [ "$got" -eq "$want" ]; then
      printf '  ok      %s\n' "$label"
      pass=$((pass + 1))
    else
      printf '  FAILED  %s (wanted exit %s, got %s)\n' "$label" "$want" "$got"
      fail_count=$((fail_count + 1))
    fi
  }

  echo "change-title policy — canonical subjects"
  case_is 0 "one microstep" 'feat(domain): exact inclusive tax extraction   [1.3.4]'
  case_is 0 "an ordered range" 'chore(repo): close the phase-zero guards   [0.1.1–0.4.3]'
  case_is 0 "work without a plan step" 'docs(impl): explain release evidence   [—]'
  case_is 0 "repository automation uses the same grammar" 'chore(repo): bump the Rust patch group   [—]'

  echo "change-title policy — malformed tags and subjects are refused"
  case_is 1 "a missing tag" 'feat(domain): exact inclusive tax extraction'
  case_is 1 "an arbitrary tag" 'feat(domain): exact inclusive tax extraction   [banana]'
  case_is 1 "an incomplete number" 'feat(domain): exact inclusive tax extraction   [1.3]'
  case_is 1 "an ASCII-hyphen range" 'feat(domain): exact inclusive tax extraction   [1.3.4-1.3.5]'
  case_is 1 "a reversed range" 'feat(domain): exact inclusive tax extraction   [2.1.1–1.9.9]'
  case_is 1 "an empty summary" 'feat(domain):    [1.3.4]'
  case_is 1 "a trailing period" 'feat(domain): exact inclusive tax extraction.   [1.3.4]'
  case_is 1 "an unknown type" 'build(domain): exact inclusive tax extraction   [1.3.4]'
  case_is 1 "an unknown scope" 'feat(agent): exact inclusive tax extraction   [1.3.4]'
  case_is 1 "a multi-line title" $'feat(domain): exact tax   [1.3.4]\nsecond line'

  printf '\n%s passed, %s failed\n' "$pass" "$fail_count"
  [ "$fail_count" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  [ "$#" -eq 1 ] || usage
  self_test
  exit $?
fi

[ "$#" -eq 1 ] || usage
validate "$1"
