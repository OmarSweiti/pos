#!/usr/bin/env bash
# Negative test for docs-links-on-write.sh. CLAUDE.md claims every guard in this
# repository is negative-tested; until this file existed, that claim was false
# for exactly one row of its table.
#
# Each case feeds a synthetic PostToolUse payload and asserts the exit code:
#   2 = reported back to Claude   0 = nothing to say
#
# The failing cases run against a throwaway fixture repository, never this one,
# so proving the hook fires does not require breaking a real cross-reference.
set -uo pipefail
cd "$(dirname "$0")" || exit 1
HOOK=./docs-links-on-write.sh
REAL_ROOT=$(git rev-parse --show-toplevel)

pass=0; fail=0

expect() {           # expect <want-code> <label> <payload-json>
  local want=$1 label=$2 payload=$3 got
  printf '%s' "$payload" | "$HOOK" >/dev/null 2>&1
  got=$?
  if [ "$got" -eq "$want" ]; then
    printf '  ok    %s\n' "$label"; pass=$((pass+1))
  else
    printf '  FAIL  %s  (wanted exit %s, got %s)\n' "$label" "$want" "$got"; fail=$((fail+1))
  fi
}

payload() {          # payload <cwd> <file_path>
  printf '{"tool_name":"Edit","cwd":"%s","tool_input":{"file_path":"%s"}}' "$1" "$2"
}

# A fixture repository whose only documentation link is deliberately dangling.
FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT
mkdir -p "$FIXTURE/scripts" "$FIXTURE/docs"
cp "$REAL_ROOT/scripts/check-doc-links.sh" "$FIXTURE/scripts/"
git -C "$FIXTURE" init -q
printf 'See [the other one](nowhere.md).\n' > "$FIXTURE/docs/broken.md"

# A second fixture, identical but with the link resolving, to prove the hook is
# reading the link checker's verdict rather than just the path pattern.
WHOLE=$(mktemp -d)
trap 'rm -rf "$FIXTURE" "$WHOLE"' EXIT
mkdir -p "$WHOLE/scripts" "$WHOLE/docs"
cp "$REAL_ROOT/scripts/check-doc-links.sh" "$WHOLE/scripts/"
git -C "$WHOLE" init -q
printf 'See [the other one](there.md).\n' > "$WHOLE/docs/ok.md"
printf 'Here.\n' > "$WHOLE/docs/there.md"

echo "docs-links-on-write.sh — reports a link this write broke"
expect 2 "a dangling link under docs/"        "$(payload "$FIXTURE" "$FIXTURE/docs/broken.md")"
expect 2 "a relative path under docs/"        "$(payload "$FIXTURE" "docs/broken.md")"
# The Windows form of the same payload. Without the backslash alternation this
# passed silently — the one platform whose paths differ got no checking at all.
expect 2 "a Windows-style path separator"     "$(printf '{"tool_name":"Edit","cwd":"%s","tool_input":{"file_path":"docs\\\\broken.md"}}' "$FIXTURE")"

echo "docs-links-on-write.sh — the checked tree is the payload's, not the script's"
# This is the worktree bug: the hook lives in THIS repository, whose links all
# resolve. If it derived the root from its own location it would pass here.
expect 2 "a broken tree reached via cwd"      "$(payload "$FIXTURE" "docs/broken.md")"
expect 0 "an intact tree reached via cwd"     "$(payload "$WHOLE" "docs/ok.md")"

echo "docs-links-on-write.sh — stays out of the way otherwise"
expect 0 "a write outside docs/"              "$(payload "$FIXTURE" "crates/pos-domain/src/money.rs")"
expect 0 "a non-markdown file under docs/"    "$(payload "$FIXTURE" "docs/plan/diagram.png")"
expect 0 "a markdown file outside docs/"      "$(payload "$FIXTURE" "README.md")"
expect 0 "this repository's own links"        "$(payload "$REAL_ROOT" "docs/implementation/README.md")"
expect 0 "a tree with no link checker"        "$(payload "$(mktemp -d)" "docs/x.md")"
expect 0 "malformed payload does nothing"     'not json at all'

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
