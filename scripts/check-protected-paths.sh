#!/usr/bin/env bash
# Refuse a change set that edits a source plan or a migration that was already
# committed in the base revision.
#
#   ./scripts/check-protected-paths.sh <base-rev> <head-rev>
#
# This is the CI backstop for the two rules the local guards enforce
# (.claude/hooks/protect-immutable.py and .githooks/pre-commit). Both of those run
# on a developer's machine, which means both can be skipped: the Claude hook fails
# open by design, and `git commit --no-verify` is one flag. This check runs on the
# merge candidate, where nothing local can reach it.
#
# "Already committed" is asked of the MERGE BASE of the base and head revisions,
# not of HEAD and not of the base tip. Asking it of the merge base is what makes
# adding a new migration legitimate and editing an old one not; what keeps a
# promotion PR (development → staging) passing, since a migration added on
# development after the last promotion is new at the branch point too; and what
# stops a branch that has simply fallen behind from being accused of deleting a
# migration the base merged in the meantime.
#
# `--self-test` builds a throwaway repository and asserts each verdict. A guard
# nobody has seen fail is a guard nobody should trust.
set -uo pipefail

SELF=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")

self_test() {
  local pass=0 fail=0 tmp
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/pos-check-protected-paths.XXXXXX")
  trap 'rm -rf "$tmp"' RETURN

  git -C "$tmp" init -q
  git -C "$tmp" config user.email t@t
  git -C "$tmp" config user.name t
  mkdir -p "$tmp/docs/plan" "$tmp/crates/pos-db/migrations" "$tmp/apps/server/migrations"
  printf 'the plan\n'                  > "$tmp/docs/plan/blueprint.md"
  printf 'CREATE TABLE a (id BLOB);\n' > "$tmp/crates/pos-db/migrations/0001_init.sql"
  printf 'fn main() {}\n'              > "$tmp/main.rs"
  git -C "$tmp" add -A >/dev/null
  git -C "$tmp" commit -q --no-verify -m base >/dev/null
  local base
  base=$(git -C "$tmp" rev-parse HEAD)

  case_is() {          # case_is <want-exit> <label> <action>
    local want=$1 label=$2 action=$3 got
    git -C "$tmp" reset -q --hard "$base"
    ( cd "$tmp" && eval "$action" && git add -A && git commit -q --no-verify -m head ) >/dev/null 2>&1
    ( cd "$tmp" && "$SELF" "$base" HEAD ) >/dev/null 2>&1
    got=$?
    if [ "$got" -eq "$want" ]; then
      printf '  ok    %s\n' "$label"; pass=$((pass+1))
    else
      printf '  FAIL  %s  (wanted exit %s, got %s)\n' "$label" "$want" "$got"; fail=$((fail+1))
    fi
  }

  command_is() {       # command_is <want-exit> <label> <command...>
    local want=$1 label=$2 got=0
    shift 2
    "$@" >/dev/null 2>&1 || got=$?
    if [ "$got" -eq "$want" ]; then
      printf '  ok    %s\n' "$label"; pass=$((pass+1))
    else
      printf '  FAIL  %s  (wanted exit %s, got %s)\n' "$label" "$want" "$got"; fail=$((fail+1))
    fi
  }

  echo "check-protected-paths.sh — refuses what the local guards refuse"
  case_is 1 "editing a source plan"          'printf "edited\n" >> docs/plan/blueprint.md'
  case_is 1 "deleting a source plan"         'git rm -q docs/plan/blueprint.md'
  case_is 1 "adding a NEW source plan"       'printf "new\n" > docs/plan/extra.md'
  case_is 1 "adding a case-varied source plan" 'mkdir -p DOCS/PLAN; printf "new\n" > DOCS/PLAN/extra.md'
  case_is 1 "adding a newline-bearing source plan" 'path=$(printf "docs/plan/line\nbreak.md"); printf "new\n" > "$path"'
  case_is 1 "editing a committed migration"  'printf "ALTER TABLE a ADD b TEXT;\n" >> crates/pos-db/migrations/0001_init.sql'
  case_is 1 "deleting a committed migration" 'git rm -q crates/pos-db/migrations/0001_init.sql'
  case_is 1 "renaming a committed migration" 'git mv crates/pos-db/migrations/0001_init.sql crates/pos-db/migrations/0001_initial.sql'
  case_is 1 "adding a case-varied migration path" 'mkdir -p CRATES/POS-DB/MIGRATIONS; printf "SELECT 1;\n" > CRATES/POS-DB/MIGRATIONS/0002_NEXT.SQL'

  echo "check-protected-paths.sh — allows the work that must go through"
  case_is 0 "adding the NEXT migration"      'printf "CREATE TABLE b (id BLOB);\n" > crates/pos-db/migrations/0002_next.sql'
  case_is 0 "adding a Postgres mirror"       'printf "CREATE TABLE b (id UUID);\n" > apps/server/migrations/20260101000000_next.sql'
  case_is 0 "ordinary source changes"        'printf "// edit\n" >> main.rs'

  # The real revisions remain valid while a PATH shim makes only `git diff`
  # fail. This proves an operational failure cannot be misreported as an empty
  # protected-path change set.
  local real_git shim
  real_git=$(command -v git)
  shim="$tmp/fail-diff-bin"
  mkdir -p "$shim"
  printf '%s\n' \
    '#!/bin/sh' \
    'if [ "$1" = "diff" ]; then exit 42; fi' \
    'exec "$REAL_GIT" "$@"' \
    > "$shim/git"
  chmod 0700 "$shim/git"
  command_is 2 "a git diff failure refuses closed" \
    env REAL_GIT="$real_git" PATH="$shim:$PATH" "$SELF" "$base" HEAD

  # Divergence is the case the two-dot comparison got wrong. Every case above has
  # head as a direct child of base, where the base tip IS the merge base, so none
  # of them can distinguish the two questions. These build a real fork: the base
  # side merges a migration the head side has never seen.
  diverged_is() {      # diverged_is <want-exit> <label> <base-action> <head-action>
    local want=$1 label=$2 base_action=$3 head_action=$4 got base_tip head_tip
    git -C "$tmp" checkout -q -B base_side "$base" >/dev/null 2>&1
    git -C "$tmp" reset -q --hard "$base"
    ( cd "$tmp" && eval "$base_action" && git add -A && git commit -q --no-verify -m base-side ) >/dev/null 2>&1
    base_tip=$(git -C "$tmp" rev-parse HEAD)
    git -C "$tmp" checkout -q -B head_side "$base" >/dev/null 2>&1
    git -C "$tmp" reset -q --hard "$base"
    ( cd "$tmp" && eval "$head_action" && git add -A && git commit -q --no-verify -m head-side ) >/dev/null 2>&1
    head_tip=$(git -C "$tmp" rev-parse HEAD)
    ( cd "$tmp" && "$SELF" "$base_tip" "$head_tip" ) >/dev/null 2>&1
    got=$?
    if [ "$got" -eq "$want" ]; then
      printf '  ok    %s\n' "$label"; pass=$((pass+1))
    else
      printf '  FAIL  %s  (wanted exit %s, got %s)\n' "$label" "$want" "$got"; fail=$((fail+1))
    fi
  }

  echo "check-protected-paths.sh — a branch behind its base is judged on what it changed"
  diverged_is 0 "a stale branch is not accused of deleting the base's newer migration" \
    'printf "CREATE TABLE c (id BLOB);\n" > crates/pos-db/migrations/0003_later.sql' \
    'printf "// edit\n" >> main.rs'
  diverged_is 0 "a stale branch may still add the next migration" \
    'printf "CREATE TABLE c (id BLOB);\n" > crates/pos-db/migrations/0003_later.sql' \
    'printf "CREATE TABLE b (id BLOB);\n" > crates/pos-db/migrations/0002_next.sql'
  diverged_is 1 "a stale branch editing a migration from the branch point is still refused" \
    'printf "CREATE TABLE c (id BLOB);\n" > crates/pos-db/migrations/0003_later.sql' \
    'printf "ALTER TABLE a ADD b TEXT;\n" >> crates/pos-db/migrations/0001_init.sql'
  diverged_is 1 "a stale branch editing a source plan is still refused" \
    'printf "CREATE TABLE c (id BLOB);\n" > crates/pos-db/migrations/0003_later.sql' \
    'printf "edited\n" >> docs/plan/blueprint.md'

  # An unresolvable merge base must refuse closed rather than read as "nothing
  # protected changed". Two root commits share no ancestor at all.
  git -C "$tmp" checkout -q --orphan unrelated >/dev/null 2>&1
  git -C "$tmp" rm -rq --cached . >/dev/null 2>&1
  printf 'unrelated\n' > "$tmp/unrelated.txt"
  ( cd "$tmp" && git add unrelated.txt && git commit -q --no-verify -m unrelated ) >/dev/null 2>&1
  local unrelated_tip
  unrelated_tip=$(git -C "$tmp" rev-parse HEAD)
  command_is 2 "an unresolvable merge base refuses closed" \
    env -C "$tmp" "$SELF" "$base" "$unrelated_tip"

  printf '\n%s passed, %s failed\n' "$pass" "$fail"
  [ "$fail" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit $?
fi

BASE="${1:-}"
HEAD="${2:-}"
if [ -z "$BASE" ] || [ -z "$HEAD" ]; then
  echo "usage: $0 <base-rev> <head-rev>" >&2
  echo "       $0 --self-test" >&2
  exit 2
fi

# GitHub renders `::error::` as an annotation on the run; plain shells just see it.
fail() { echo "::error::$1"; refuse=1; }
refuse=0

# Resolve caller input to commits before it reaches `git diff`: the workflow
# supplies SHAs, but the standalone command must not interpret a revision that
# starts with `-` as another option. Any resolution failure is an operational
# error, never evidence that protected paths stayed unchanged.
base_commit=$(git rev-parse --verify --end-of-options "$BASE^{commit}") || {
  echo "::error::protected paths: cannot resolve base revision" >&2
  exit 2
}
head_commit=$(git rev-parse --verify --end-of-options "$HEAD^{commit}") || {
  echo "::error::protected paths: cannot resolve head revision" >&2
  exit 2
}

# Immutability asks what THIS branch did to files that already existed when it
# started — not how its tree differs from today's base tip. A two-dot
# `git diff base head` answers the second question, so every migration the base
# has merged since the branch point reads as a deletion: the file exists at the
# base and not at the head, and the loop below then refuses the PR for removing a
# committed migration it never opened. "You deleted a committed migration" is the
# least credible refusal this checker can emit, and unlike the frozen-policy red
# there is no documented "this red is expected" story to dismiss it by. So resolve
# the merge base and compare against that. A migration that existed at the merge
# base and is edited by the branch is still refused, which is the rule that
# matters; a duplicate migration NUMBER introduced by a stale branch is a
# different failure, caught by verify-schema.py's exact ordered parity.
#
# A merge base that cannot be resolved is an operational error, never evidence
# that protected paths stayed unchanged.
merge_base=$(git merge-base "$base_commit" "$head_commit") || {
  echo "::error::protected paths: cannot resolve the merge base of the base and head revisions; refusing closed" >&2
  exit 2
}
if [ -z "$merge_base" ]; then
  echo "::error::protected paths: empty merge base; refusing closed" >&2
  exit 2
fi

# --no-renames is load-bearing. With rename detection on, `git mv` of a committed
# migration reports only the NEW name, which does not exist at the base and so
# reads as a legitimate addition — the deletion half never appears. Turning it off
# makes a rename a delete plus an add, and the delete is what must be refused.
#
# NUL delimiters are equally load-bearing: Git permits tabs and newlines in a
# filename. Default quoted/line-oriented output can turn `docs/plan/<newline>`
# into a string that no longer begins with the protected prefix.
changed_file=$(mktemp "${TMPDIR:-/tmp}/pos-check-protected-paths.XXXXXX") || {
  echo "::error::protected paths: cannot create a temporary path list" >&2
  exit 2
}
trap 'rm -f "$changed_file"' EXIT
if ! git diff --name-only --no-renames -z "$merge_base" "$head_commit" -- > "$changed_file"; then
  echo "::error::protected paths: git diff failed; refusing closed" >&2
  exit 2
fi

if [ ! -s "$changed_file" ]; then
  echo "protected paths: nothing under docs/plan/ or either migrations directory changed"
  exit 0
fi

while IFS= read -r -d '' f; do
  [ -z "$f" ] && continue
  folded=$(printf '%s' "$f" | LC_ALL=C tr '[:upper:]' '[:lower:]')
  printf -v shown '%q' "$f"
  case "$folded" in
    docs/plan|docs/plan/*)
      fail "$shown is a source document. docs/plan/** are inputs to the implementation set, never working documents (CLAUDE.md). Record the correction in docs/implementation/." ;;
    crates/pos-db/migrations|crates/pos-db/migrations/*|apps/server/migrations|apps/server/migrations/*)
      if [ "$f" != "$folded" ]; then
        fail "$shown uses non-canonical casing for a migration path. Use the exact lowercase repository directory and filename convention."
        continue
      fi
      base_entry=''
      if ! base_entry=$(git ls-tree --full-tree "$merge_base" -- "$f"); then
        echo "::error::protected paths: cannot inspect $shown at the branch point; refusing closed" >&2
        exit 2
      fi
      if [ -n "$base_entry" ]; then
        fail "$shown existed at the branch point and this changes or removes it. Migrations are forward-only (01-conventions.md §9) — write the next one."
      else
        echo "  ok  $shown is a new migration"
      fi ;;
  esac
done < "$changed_file"

if [ "$refuse" -ne 0 ]; then
  echo
  echo "The local guards refuse these too. If one let this through, say so — that is"
  echo "a guard to fix, not a check to override."
  exit 1
fi
echo "protected paths OK"
