#!/usr/bin/env bash
# The one grammar for commit subjects and squash-merge PR titles.
#
#   <type>(<scope>): <summary>   [<step>]
#
# A step is one microstep, an inclusive microstep range written with an en dash,
# or an em dash when the work has no implementation-plan step. Dependabot and
# other repository automation use the same grammar and `[—]`; automation is not
# a reason to make repository history less searchable.
#
# A microstep's last component may carry one lowercase letter, because the plan
# splits a step whose numbered form cannot compile in numbered order: Phase 1's
# §1.1 keeps `1.1.2` as a concordance heading and requires implementation commits
# to read `[1.1.2a]` and `[1.1.2b]` "so the two independently green changes stay
# distinguishable". Phase 1 alone has 13 lettered microsteps. Refusing the letter
# forced that work under the unlettered parent, which is a heading the plan calls
# non-executable — so the tag claimed a step nobody implements, and bisecting to
# one half of a split became a body-text search. Absent sorts before `a`, so
# `1.1.2 < 1.1.2a < 1.1.2b < 1.1.3` and a range spanning a split stays ordered.
set -euo pipefail

types='feat|fix|test|docs|chore|refactor|perf'
scopes='domain|db|sync|hardware|fiscal|terminal|server|backoffice|repo|impl'
step='—|[0-9]+\.[0-9]+\.[0-9]+[a-z]?(–[0-9]+\.[0-9]+\.[0-9]+[a-z]?)?'
grammar="^($types)\\(($scopes)\\): ([^[:space:]].*[^[:space:]]|[^[:space:]])[[:space:]]+\\[($step)\\][[:space:]]*$"

usage() {
  echo "usage: $0 '<title>'" >&2
  echo "       $0 --normalize '<title>'" >&2
  echo "       $0 --self-test" >&2
  exit 2
}

fail() {
  echo "title-policy: REFUSED — $1" >&2
  echo >&2
  echo "  <type>(<scope>): <summary>   [<step>]" >&2
  echo "  type  ∈ $types" >&2
  echo "  scope ∈ $scopes" >&2
  echo "  step  = N.N.N, N.N.Nx, N.N.N–N.N.N, or —" >&2
  echo >&2
  echo "  got: ${title:-<none>}" >&2
  return 1
}

component_number() {
  # Force decimal interpretation so a zero-padded component never becomes
  # accidental octal under Bash arithmetic.
  printf '%d' "$((10#$1))"
}

suffix_ordinal() {
  # A microstep's optional lowercase letter, as a sortable ordinal: absent is 0
  # and sorts first, `a` is 1, `b` is 2. That is the order the phase files use —
  # the unlettered form is the concordance heading and the letters implement it.
  # Passing the letter through Bash arithmetic directly is what this exists to
  # avoid: `$((10#2a))` is a fatal arithmetic error, not a comparison.
  local letter="$1"
  if [ -z "$letter" ]; then
    printf '0'
    return 0
  fi
  printf '%d' "$(( $(printf '%d' "'$letter") - 96 ))"
}

split_component() {
  # Echo a last component's digits and its letter ordinal, space separated.
  local value="$1"
  if [[ "$value" =~ ^([0-9]+)([a-z]?)$ ]]; then
    printf '%s %s' "$(component_number "${BASH_REMATCH[1]}")" \
      "$(suffix_ordinal "${BASH_REMATCH[2]}")"
    return 0
  fi
  # The grammar already refused anything else; be loud rather than compare zeros.
  echo "title-policy: internal — unparseable step component '$value'" >&2
  return 1
}

range_is_ordered() {
  local value="$1" start end sa sb sc ea eb ec sc_n sc_x ec_n ec_x
  case "$value" in
    *–*) ;;
    *) return 0 ;;
  esac

  start="${value%%–*}"
  end="${value#*–}"
  IFS=. read -r sa sb sc <<< "$start"
  IFS=. read -r ea eb ec <<< "$end"
  sa=$(component_number "$sa"); sb=$(component_number "$sb")
  ea=$(component_number "$ea"); eb=$(component_number "$eb")
  read -r sc_n sc_x <<< "$(split_component "$sc")"
  read -r ec_n ec_x <<< "$(split_component "$ec")"

  (( sa < ea )) && return 0
  (( sa > ea )) && return 1
  (( sb < eb )) && return 0
  (( sb > eb )) && return 1
  (( sc_n < ec_n )) && return 0
  (( sc_n > ec_n )) && return 1
  (( sc_x <= ec_x ))
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

trim_truncated_summary() {
  local value="$1" last_word last_word_lower

  while [ -n "$value" ]; do
    while [[ "$value" =~ [[:space:]]$ || "$value" =~ [[:punct:]]$ ]]; do
      value="${value%?}"
    done
    [ -n "$value" ] || break

    last_word="${value##* }"
    last_word_lower=$(printf '%s' "$last_word" | LC_ALL=C tr '[:upper:]' '[:lower:]')
    case "$last_word_lower" in
      a|an|and|as|at|but|by|for|from|in|into|nor|of|on|onto|or|so|the|to|via|with|without|yet|across)
        if [[ "$value" == *' '* ]]; then
          value="${value% *}"
        else
          value=''
        fi
        ;;
      *) break ;;
    esac
  done

  printf '%s' "$value"
}

truncate_summary() {
  local value="$1" limit="$2" word candidate truncated=''
  local -a words
  read -r -a words <<< "$value"

  for word in "${words[@]}"; do
    candidate="$word"
    [ -z "$truncated" ] || candidate="$truncated $word"
    [ "${#candidate}" -le "$limit" ] || break
    truncated="$candidate"
  done

  truncated=$(trim_truncated_summary "$truncated")
  [ -n "$truncated" ] || return 1
  printf '%s' "$truncated"
}

normalize() {
  local input="$1" candidate="$1" canonical_tag directory_suffix group_suffix
  local type scope summary step_value prefix normalized max_summary

  if validate "$input" >/dev/null 2>&1; then
    printf '%s\n' "$input"
    return 0
  fi

  case "$input" in
    *$'\n'*|*$'\r'*) validate "$input"; return 1 ;;
  esac

  canonical_tag="[[:space:]]+\\[($step)\\][[:space:]]*$"
  if [[ ! "$candidate" =~ $canonical_tag ]]; then
    candidate="$candidate  [—]"
  fi

  if [[ ! "$candidate" =~ $grammar ]]; then
    validate "$candidate"
    return 1
  fi
  type="${BASH_REMATCH[1]}"
  scope="${BASH_REMATCH[2]}"
  summary="${BASH_REMATCH[3]}"
  step_value="${BASH_REMATCH[4]}"
  prefix="$type($scope): "

  # Dependabot's directory suffix is generated transport detail, even when the
  # remaining subject would already fit. The group name stays unless another
  # reduction is needed.
  directory_suffix='[[:space:]]+across[[:space:]]+[0-9]+[[:space:]]+director(y|ies)([[:space:]]+with[[:space:]]+[0-9]+[[:space:]]+updates?)?$'
  if [[ "$summary" =~ $directory_suffix ]]; then
    summary="${summary%"${BASH_REMATCH[0]}"}"
  fi

  if [ "$(( ${#prefix} + ${#summary} ))" -gt 72 ]; then
    group_suffix='[[:space:]]+in[[:space:]]+the[[:space:]]+[^[:space:]]+[[:space:]]+group$'
    if [[ "$summary" =~ $group_suffix ]]; then
      summary="${summary%"${BASH_REMATCH[0]}"}"
    fi
  fi

  if [ "$(( ${#prefix} + ${#summary} ))" -gt 72 ]; then
    max_summary=$((72 - ${#prefix}))
    summary=$(truncate_summary "$summary" "$max_summary") || {
      title="$input"
      fail "the summary cannot be truncated to 72 characters at a word boundary."
      return 1
    }
  fi

  normalized="$prefix$summary  [$step_value]"
  if ! validate "$normalized"; then
    return 1
  fi
  printf '%s\n' "$normalized"
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

  normalize_is() {
    local want="$1" label="$2" candidate="$3" got='' status=0
    got=$(normalize "$candidate" 2>/dev/null) || status=$?
    if [ "$status" -eq 0 ] && [ "$got" = "$want" ] && \
      validate "$got" >/dev/null 2>&1; then
      printf '  ok      %s\n' "$label"
      pass=$((pass + 1))
    else
      printf '  FAILED  %s (wanted "%s", got "%s", exit %s)\n' \
        "$label" "$want" "$got" "$status"
      fail_count=$((fail_count + 1))
    fi
  }

  normalize_is_idempotent() {
    local label="$1" candidate="$2" once='' twice='' first_status=0 second_status=0
    once=$(normalize "$candidate" 2>/dev/null) || first_status=$?
    if [ "$first_status" -eq 0 ]; then
      twice=$(normalize "$once" 2>/dev/null) || second_status=$?
    else
      second_status="$first_status"
    fi
    if [ "$first_status" -eq 0 ] && [ "$second_status" -eq 0 ] && \
      [ "$once" = "$twice" ] && validate "$once" >/dev/null 2>&1; then
      printf '  ok      %s\n' "$label"
      pass=$((pass + 1))
    else
      printf '  FAILED  %s (first exit %s, second exit %s)\n' \
        "$label" "$first_status" "$second_status"
      fail_count=$((fail_count + 1))
    fi
  }

  normalize_refuses() {
    local label="$1" candidate="$2" got='' status=0
    got=$(normalize "$candidate" 2>/dev/null) || status=$?
    if [ "$status" -ne 0 ] && [ -z "$got" ]; then
      printf '  ok      %s\n' "$label"
      pass=$((pass + 1))
    else
      printf '  FAILED  %s (wanted refusal with no output, got exit %s and "%s")\n' \
        "$label" "$status" "$got"
      fail_count=$((fail_count + 1))
    fi
  }

  echo "change-title policy — canonical subjects"
  case_is 0 "one microstep" 'feat(domain): exact inclusive tax extraction   [1.3.4]'
  case_is 0 "an ordered range" 'chore(repo): close the phase-zero guards   [0.1.1–0.4.3]'
  case_is 0 "work without a plan step" 'docs(impl): explain release evidence   [—]'
  case_is 0 "repository automation uses the same grammar" 'chore(repo): bump the Rust patch group   [—]'
  case_is 0 "a lettered split microstep" 'feat(domain): Money carries its currency   [1.1.2a]'
  case_is 0 "the second half of a split" 'feat(domain): complete Money arithmetic   [1.1.2b]'
  case_is 0 "a range across one split" 'chore(repo): land both halves   [1.1.2a–1.1.2b]'
  case_is 0 "an unlettered start into a lettered end" 'chore(repo): land the split   [1.1.2–1.1.2b]'
  case_is 0 "a lettered start into a later number" 'chore(repo): finish the group   [1.1.2a–1.1.9]'

  echo "change-title policy — malformed tags and subjects are refused"
  case_is 1 "a missing tag" 'feat(domain): exact inclusive tax extraction'
  case_is 1 "an arbitrary tag" 'feat(domain): exact inclusive tax extraction   [banana]'
  case_is 1 "an incomplete number" 'feat(domain): exact inclusive tax extraction   [1.3]'
  case_is 1 "an ASCII-hyphen range" 'feat(domain): exact inclusive tax extraction   [1.3.4-1.3.5]'
  case_is 1 "a reversed range" 'feat(domain): exact inclusive tax extraction   [2.1.1–1.9.9]'
  case_is 1 "a reversed lettered range" 'chore(repo): land both halves   [1.1.2b–1.1.2a]'
  case_is 1 "a lettered end before an unlettered start" 'chore(repo): land it   [1.1.2a–1.1.2]'
  case_is 1 "an uppercase suffix" 'feat(domain): Money carries its currency   [1.1.2A]'
  case_is 1 "two suffix letters" 'feat(domain): Money carries its currency   [1.1.2ab]'
  case_is 1 "a digit after the suffix" 'feat(domain): Money carries its currency   [1.1.2a1]'
  case_is 1 "a suffix on a middle component" 'feat(domain): Money carries its currency   [1.1a.2]'
  case_is 1 "an empty summary" 'feat(domain):    [1.3.4]'
  case_is 1 "a trailing period" 'feat(domain): exact inclusive tax extraction.   [1.3.4]'
  case_is 1 "an unknown type" 'build(domain): exact inclusive tax extraction   [1.3.4]'
  case_is 1 "an unknown scope" 'feat(agent): exact inclusive tax extraction   [1.3.4]'
  case_is 1 "a multi-line title" $'feat(domain): exact tax   [1.3.4]\nsecond line'

  echo "change-title policy — Dependabot normalization"
  normalize_is \
    'chore(repo): bump @vitejs/plugin-react from 6.0.5 to 6.1.0  [—]' \
    "an overlong package-group title loses generated suffixes" \
    'chore(repo): bump @vitejs/plugin-react from 6.0.5 to 6.1.0 in the js-minor group across 1 directory'
  normalize_is \
    'chore(repo): bump the actions group  [—]' \
    "a directory-and-update suffix is removed" \
    'chore(repo): bump the actions group across 1 directory with 5 updates'
  normalize_is \
    'chore(repo): bump alpha beta gamma delta epsilon zeta eta theta  [—]' \
    "a group suffix is removed rather than truncated mid-name" \
    'chore(repo): bump alpha beta gamma delta epsilon zeta eta theta in the g group'
  normalize_is_idempotent \
    "normalizing twice is the same as normalizing once" \
    'chore(repo): bump @vitejs/plugin-react from 6.0.5 to 6.1.0 in the js-minor group across 1 directory'
  normalize_is \
    'chore(repo): bump the Rust patch group [—]' \
    "an already-conforming title is byte-identical" \
    'chore(repo): bump the Rust patch group [—]'
  normalize_is \
    'chore(repo): bump vite from 8.2.1 to 8.2.2 in the js-patch group  [—]' \
    "a title needing only a step tag keeps its summary" \
    'chore(repo): bump vite from 8.2.1 to 8.2.2 in the js-patch group'
  normalize_is \
    'chore(repo): bump a deliberately verbose dependency package update  [—]' \
    "truncation ends on a clean complete word" \
    'chore(repo): bump a deliberately verbose dependency package update, from 1.2.3 to 4.5.6'
  normalize_is \
    'chore(repo): keep group and directory words in the middle  [—]' \
    "suffix words in the middle are not removed" \
    'chore(repo): keep group and directory words in the middle'
  normalize_refuses \
    "normalization cannot discard a second line while truncating" \
    $'chore(repo): bump a deliberately verbose dependency package update from 1.2.3 to 4.5.6\nmalicious continuation'

  printf '\n%s passed, %s failed\n' "$pass" "$fail_count"
  [ "$fail_count" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  [ "$#" -eq 1 ] || usage
  self_test
  exit $?
fi

if [ "${1:-}" = "--normalize" ]; then
  [ "$#" -eq 2 ] || usage
  normalize "$2"
  exit $?
fi

[ "$#" -eq 1 ] || usage
validate "$1"
