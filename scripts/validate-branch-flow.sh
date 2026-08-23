#!/usr/bin/env bash
# One fail-closed grammar for PR branch names and head/base topology.
#
# The workflow executes this file from the exact trusted workflow revision. Keep its
# self-test in both `just guards` and CI so a proposed replacement must prove
# the malformed-name cases before it can become tomorrow's trusted policy.
set -euo pipefail

usage() {
  echo "usage: $0 <head-ref> <base-ref> <head-repository> <base-repository>" >&2
  echo "       $0 --self-test" >&2
  exit 2
}

refuse() {
  echo "branch-policy: REFUSED — $1" >&2
  return 1
}

is_kebab_slug() {
  [[ "$1" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]]
}

validate() {
  local head_ref="$1" base_ref="$2" head_repo="$3" base_repo="$4"

  [ -n "$head_ref" ] || { refuse "the head branch is empty."; return 1; }
  [ -n "$base_ref" ] || { refuse "the base branch is empty."; return 1; }
  [ -n "$head_repo" ] || { refuse "the head repository is empty."; return 1; }
  [ -n "$base_repo" ] || { refuse "the base repository is empty."; return 1; }

  case "$head_ref" in
    development)
      [ "$head_repo" = "$base_repo" ] || {
        refuse "an official development promotion must come from $base_repo, not $head_repo."
        return 1
      }
      [ "$base_ref" = "staging" ] || {
        refuse "development promotes to staging, never to '$base_ref'."
        return 1
      }
      ;;
    staging)
      [ "$head_repo" = "$base_repo" ] || {
        refuse "an official staging PR must come from $base_repo, not $head_repo."
        return 1
      }
      case "$base_ref" in
        main)        echo "staging -> main production promotion" ;;
        development) echo "staging -> development hotfix back-merge" ;;
        *) refuse "staging may promote to main or back-merge a hotfix to development, never to '$base_ref'."; return 1 ;;
      esac
      ;;
    main)
      [ "$head_repo" = "$base_repo" ] || {
        refuse "an official main back-merge must come from $base_repo, not $head_repo."
        return 1
      }
      [ "$base_ref" = "staging" ] || {
        refuse "main only back-merges a production hotfix to staging, never to '$base_ref'."
        return 1
      }
      ;;
    hotfix/*)
      local hotfix_slug="${head_ref#hotfix/}"
      is_kebab_slug "$hotfix_slug" || {
        refuse "hotfix branches use hotfix/<nonempty-kebab-slug>."
        return 1
      }
      [ "$head_repo" = "$base_repo" ] || {
        refuse "a production hotfix must come from $base_repo, not $head_repo."
        return 1
      }
      [ "$base_ref" = "main" ] || {
        refuse "a hotfix branch targets main. Ordinary fixes use fix/<slug> into development."
        return 1
      }
      ;;
    phase-*)
      [[ "$head_ref" =~ ^phase-[0-5]/group-[0-9]+-[a-z0-9]+(-[a-z0-9]+)*$ ]] || {
        refuse "group branches use phase-<0-5>/group-<m>-<nonempty-kebab-slug>."
        return 1
      }
      [ "$base_ref" = "development" ] || {
        refuse "work branches merge into development, not '$base_ref'."
        return 1
      }
      ;;
    fix/*|docs/*|refactor/*|perf/*|test/*)
      local work_slug="${head_ref#*/}"
      is_kebab_slug "$work_slug" || {
        refuse "${head_ref%%/*} branches require a nonempty lowercase kebab-case slug."
        return 1
      }
      [ "$base_ref" = "development" ] || {
        refuse "work branches merge into development, not '$base_ref'."
        return 1
      }
      ;;
    chore/*)
      local chore_slug="${head_ref#chore/}"
      if [[ ! "$chore_slug" =~ ^release-v[0-9]+\.[0-9]+\.[0-9]+$ ]] && \
         ! is_kebab_slug "$chore_slug"; then
        refuse "chore branches require a nonempty kebab-case slug or release-v<major>.<minor>.<patch>."
        return 1
      fi
      [ "$base_ref" = "development" ] || {
        refuse "work branches merge into development, not '$base_ref'."
        return 1
      }
      ;;
    dependabot/*)
      [[ "$head_ref" =~ ^dependabot/[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*$ ]] || {
        refuse "Dependabot branch components must be nonempty and path-safe."
        return 1
      }
      [ "$base_ref" = "development" ] || {
        refuse "Dependabot branches merge into development, not '$base_ref'."
        return 1
      }
      ;;
    *)
      refuse "branch '$head_ref' is outside the repository branch scheme."
      return 1
      ;;
  esac

  echo "branch topology OK: $head_repo:$head_ref -> $base_repo:$base_ref"
}

self_test() {
  local passed=0 failed=0

  case_is() {
    local want="$1" label="$2" head="$3" base="$4"
    local got=0
    validate "$head" "$base" "owner/pos" "owner/pos" >/dev/null 2>&1 || got=$?
    if [ "$got" -eq "$want" ]; then
      printf '  ok      %s\n' "$label"
      passed=$((passed + 1))
    else
      printf '  FAILED  %s (wanted exit %s, got %s)\n' "$label" "$want" "$got"
      failed=$((failed + 1))
    fi
  }

  echo "branch policy — canonical names and adjacent topology"
  case_is 0 "the Phase 0 boundary" "phase-0/group-1-bootstrap" development
  case_is 0 "a numbered implementation group" "phase-1/group-3-tax-engine" development
  case_is 0 "the Phase 5 boundary" "phase-5/group-5-launch" development
  case_is 0 "an ordinary fix" "fix/receipt-rounding" development
  case_is 0 "a release-preparation chore" "chore/release-v0.2.1" development
  case_is 0 "a nested Dependabot branch" "dependabot/npm_and_yarn/apps/web/vite-8.2.1" development
  case_is 0 "development promotes to staging" development staging
  case_is 0 "staging promotes to main" staging main
  case_is 0 "staging back-merges to development" staging development
  case_is 0 "main back-merges to staging" main staging
  case_is 0 "a production hotfix targets main" "hotfix/receipt-total" main

  echo "branch policy — malformed or misplaced names are refused"
  case_is 1 "a nonnumeric phase" "phase-x/group-1-tax" development
  case_is 1 "a missing phase number" "phase-/group-1-tax" development
  case_is 1 "a phase beyond the six maintained gates" "phase-6/group-1-tax" development
  case_is 1 "a missing group slug" "phase-1/group-2-" development
  case_is 1 "an empty ordinary slug" "fix/" development
  case_is 1 "a noncanonical ordinary slug" "fix/Receipt_Rounding" development
  case_is 1 "an obsolete release branch" "release/v0.2.1" development
  case_is 1 "a work branch targeting staging" "docs/release-guide" staging
  case_is 1 "a hotfix targeting development" "hotfix/receipt-total" development

  local got=0
  validate development staging "fork/pos" "owner/pos" >/dev/null 2>&1 || got=$?
  if [ "$got" -eq 1 ]; then
    printf '  ok      %s\n' "an official promotion cannot come from a fork"
    passed=$((passed + 1))
  else
    printf '  FAILED  %s (wanted exit 1, got %s)\n' \
      "an official promotion cannot come from a fork" "$got"
    failed=$((failed + 1))
  fi

  printf '\n%s passed, %s failed\n' "$passed" "$failed"
  [ "$failed" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  [ "$#" -eq 1 ] || usage
  self_test
  exit $?
fi

[ "$#" -eq 4 ] || usage
validate "$1" "$2" "$3" "$4"
