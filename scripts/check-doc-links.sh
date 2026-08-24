#!/usr/bin/env bash
# Fails if a relative link in any tracked Markdown file points at something that
# does not exist. This documentation set is only worth its cross-references.
#
# Scope is every tracked *.md, not just docs/. The five root documents — README,
# CLAUDE.md, CONTRIBUTING.md, SECURITY.md, AGENTS.md — carry the table that sends
# a new reader into the documentation set and the links to the compliance
# reference. They were outside every link guard while `just lint` reported
# "doc links" and CI reported "Documentation links resolve", so a rename could
# orphan the entry points with both gates green.
#
# Targets are checked whatever their extension. The .md-only pattern this
# replaces had already let one break through: docs/plan/phase-0-setup-guide.md
# links `../justfile`, which resolves to docs/justfile and has never existed.
#
# Usage:  ./scripts/check-doc-links.sh
#         ./scripts/check-doc-links.sh --self-test   # prove the check fires
# Exit:   0 clean · 1 a broken link · 2 could not run at all
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

# Links that cannot be repaired where they are written. docs/plan/ is immutable
# under .claude/hooks/protect-immutable.py and scripts/check-protected-paths.sh:
# a source plan is the record of what was agreed, so a PR that edits one is
# refused. The link is wrong (`../justfile` from docs/plan/ should be
# `../../justfile`) and stays wrong until the plan is superseded rather than
# edited. Recorded here, once, with the reason — and verified below to still be
# broken, so a stale exception cannot outlive the thing it excuses.
ALLOWED_BROKEN=(
  "docs/plan/phase-0-setup-guide.md	../justfile"
)

is_allowed() {
  local entry
  for entry in "${ALLOWED_BROKEN[@]}"; do
    [ "$entry" = "$1	$2" ] && return 0
  done
  return 1
}

# Every relative link target in one file: markdown inline links, anchors
# stripped, external schemes and prose globs skipped.
targets_in() {
  grep -oE '\]\([^)[:space:]]+\)' "$1" 2>/dev/null \
    | sed -E 's/^\]\(//; s/\)$//; s/#.*$//' \
    | grep -vE '^(https?|mailto|tel):' \
    | grep -Fv '*' \
    | grep -v '^$' \
    | sort -u
}

# A leading / is repository-root-relative, the way GitHub renders it; anything
# else resolves beside the file that wrote it.
resolve() {
  case "$2" in
    /*) printf '%s' ".$2" ;;
    *)  printf '%s/%s' "$(dirname "$1")" "$2" ;;
  esac
}

scan() {                     # scan <file>...  -> prints BROKEN lines, returns 1 if any
  local broken=0 f t
  for f in "$@"; do
    while IFS= read -r t; do
      [ -n "$t" ] || continue
      if [ ! -e "$(resolve "$f" "$t")" ]; then
        is_allowed "$f" "$t" && continue
        echo "BROKEN  $f  ->  $t"
        broken=1
      fi
    done < <(targets_in "$f")
  done
  return "$broken"
}

self_test() {
  local tmp pass=0 fail=0
  tmp=$(mktemp -d) || { echo "self-test: cannot create a scratch directory" >&2; return 2; }
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" RETURN

  check() {                  # check <want> <label> <file>
    local want=$1 label=$2 got
    ( cd "$tmp" && scan "$3" >/dev/null 2>&1 ); got=$?
    if [ "$got" -eq "$want" ]; then
      printf '  ok    %s\n' "$label"; pass=$((pass+1))
    else
      printf '  FAIL  %s  (wanted %s, got %s)\n' "$label" "$want" "$got"; fail=$((fail+1))
    fi
  }

  mkdir -p "$tmp/sub"
  : > "$tmp/real.md"
  : > "$tmp/justfile"
  printf '[ok](real.md)\n'            > "$tmp/good-md.md"
  printf '[gone](missing.md)\n'       > "$tmp/bad-md.md"
  printf '[ok](justfile)\n'           > "$tmp/good-plain.md"
  printf '[gone](Makefile)\n'         > "$tmp/bad-plain.md"
  printf '[ok](real.md#a-heading)\n'  > "$tmp/good-anchor.md"
  printf '[gone](missing.md#h)\n'     > "$tmp/bad-anchor.md"
  printf '[x](https://example.com/nope.md)\n' > "$tmp/external.md"
  printf '[x](#local-heading)\n'      > "$tmp/anchor-only.md"
  printf '[x](crates/*/Cargo.toml)\n' > "$tmp/glob.md"
  printf '[up](../real.md)\n'         > "$tmp/sub/good-parent.md"
  printf '[up](../missing.md)\n'      > "$tmp/sub/bad-parent.md"

  check 0 "an existing .md target passes"          good-md.md
  check 1 "a missing .md target is reported"       bad-md.md
  check 0 "an existing non-.md target passes"      good-plain.md
  check 1 "a missing non-.md target is reported"   bad-plain.md
  check 0 "an anchor on an existing file passes"   good-anchor.md
  check 1 "an anchor on a missing file is reported" bad-anchor.md
  check 0 "an external URL is not a file link"     external.md
  check 0 "a bare heading anchor is skipped"       anchor-only.md
  check 0 "a prose glob is skipped"                glob.md
  check 0 "a parent-relative target resolves"      sub/good-parent.md
  check 1 "a broken parent-relative target fires"  sub/bad-parent.md

  # The exception table must not outlive what it excuses.
  local entry file target stale=0
  for entry in "${ALLOWED_BROKEN[@]}"; do
    file=${entry%%	*}; target=${entry##*	}
    if [ ! -e "$file" ]; then
      printf '  FAIL  allowlisted file no longer exists: %s\n' "$file"; fail=$((fail+1)); stale=1
    elif [ -e "$(resolve "$file" "$target")" ]; then
      printf '  FAIL  allowlisted link now resolves; remove it: %s -> %s\n' "$file" "$target"
      fail=$((fail+1)); stale=1
    fi
  done
  [ "$stale" -eq 0 ] && { printf '  ok    every allowlisted link is still genuinely broken\n'; pass=$((pass+1)); }

  printf '\ndoc-links self-test: %d passed, %d failed\n' "$pass" "$fail"
  [ "$fail" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit $?
fi

# NUL-delimited, and read with a plain loop rather than `mapfile`: macOS ships
# bash 3.2, where mapfile does not exist and this script would fail open.
files=()
while IFS= read -r -d '' f; do
  files+=("$f")
done < <(git ls-files -z '*.md')

if [ "${#files[@]}" -eq 0 ]; then
  echo "documentation link check: git listed no tracked Markdown files" >&2
  exit 2
fi

if ! scan ${files[@]+"${files[@]}"}; then
  echo "documentation link check FAILED"
  exit 1
fi
echo "documentation links OK (${#files[@]} tracked Markdown files)"
