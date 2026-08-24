#!/usr/bin/env bash
# Negative test for docs-links-on-write.py. CLAUDE.md claims every guard in this
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
HOOK=./docs-links-on-write.py
LAUNCHER=./run-python-hook.mjs
REAL_ROOT=$(git rev-parse --show-toplevel)
PYTHON="$REAL_ROOT/scripts/run-python.sh"

pass=0; fail=0

expect() {           # expect <want-code> <label> <payload-json>
  local want=$1 label=$2 payload=$3 got
  printf '%s' "$payload" | "$PYTHON" "$HOOK" >/dev/null 2>&1
  got=$?
  if [ "$got" -eq "$want" ]; then
    printf '  ok    %s\n' "$label"; pass=$((pass+1))
  else
    printf '  FAIL  %s  (wanted exit %s, got %s)\n' "$label" "$want" "$got"; fail=$((fail+1))
  fi
}

expect_warning() {   # expect_warning <label> <payload-json>
  local label=$1 payload=$2 got output json_ok
  output=$(printf '%s' "$payload" | "$PYTHON" "$HOOK" 2>/dev/null)
  got=$?
  printf '%s' "$output" | "$PYTHON" -c \
    'import json,sys; value=json.load(sys.stdin); assert value.get("systemMessage")' \
    >/dev/null 2>&1
  json_ok=$?
  if [ "$got" -eq 0 ] && [ "$json_ok" -eq 0 ]; then
    printf '  ok    %s\n' "$label"; pass=$((pass+1))
  else
    printf '  FAIL  %s  (exit %s, structured warning %s)\n' \
      "$label" "$got" "$json_ok"; fail=$((fail+1))
  fi
}

payload() {          # payload <cwd> <file_path>
  printf '{"tool_name":"Edit","cwd":"%s","tool_input":{"file_path":"%s"}}' "$1" "$2"
}

shell_payload() {    # shell_payload <tool> <cwd> <command>
  local encoded
  encoded=$("$PYTHON" -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$3")
  printf '{"tool_name":"%s","cwd":"%s","tool_input":{"command":%s}}' "$1" "$2" "$encoded"
}

# A fixture repository whose only documentation link is deliberately dangling.
FIXTURE=$(mktemp -d "${TMPDIR:-/tmp}/pos-test-docs-links.XXXXXX") || {
  echo "test-docs-links: cannot create a temp directory" >&2
  exit 2
}
trap 'rm -rf "$FIXTURE"' EXIT
mkdir -p "$FIXTURE/scripts" "$FIXTURE/docs"
cp "$REAL_ROOT/scripts/check-doc-links.sh" "$FIXTURE/scripts/"
git -C "$FIXTURE" init -q
printf 'See [the other one](nowhere.md).\n' > "$FIXTURE/docs/broken.md"

# A second fixture, identical but with the link resolving, to prove the hook is
# reading the link checker's verdict rather than just the path pattern.
WHOLE=$(mktemp -d "${TMPDIR:-/tmp}/pos-test-docs-links.XXXXXX") || {
  echo "test-docs-links: cannot create a temp directory" >&2
  exit 2
}
trap 'rm -rf "$FIXTURE" "$WHOLE"' EXIT
mkdir -p "$WHOLE/scripts" "$WHOLE/docs"
cp "$REAL_ROOT/scripts/check-doc-links.sh" "$WHOLE/scripts/"
git -C "$WHOLE" init -q
printf 'See [the other one](there.md).\n' > "$WHOLE/docs/ok.md"
printf 'Here.\n' > "$WHOLE/docs/there.md"

# A third fixture with NO docs/ tree at all: its only documentation is a root
# README whose link dangles, and a root document linking a non-.md file that is
# not there. Both were invisible to this hook and to the canonical checker until
# the scope widened — the docs/-only walk never opened them, and the .md-only
# target pattern never looked at the second.
ROOT_BROKEN=$(mktemp -d "${TMPDIR:-/tmp}/pos-test-docs-links.XXXXXX") || {
  echo "test-docs-links: cannot create a temp directory" >&2
  exit 2
}
git -C "$ROOT_BROKEN" init -q
printf 'Start at [the plan](docs/plan.md).\n' > "$ROOT_BROKEN/README.md"

ROOT_PLAIN=$(mktemp -d "${TMPDIR:-/tmp}/pos-test-docs-links.XXXXXX") || {
  echo "test-docs-links: cannot create a temp directory" >&2
  exit 2
}
git -C "$ROOT_PLAIN" init -q
printf 'Every command is in [the justfile](justfile).\n' > "$ROOT_PLAIN/CONTRIBUTING.md"

NON_REPO=$(mktemp -d "${TMPDIR:-/tmp}/pos-test-docs-links.XXXXXX") || {
  echo "test-docs-links: cannot create a temp directory" >&2
  exit 2
}
NO_DOCS=$(mktemp -d "${TMPDIR:-/tmp}/pos-test-docs-links.XXXXXX") || {
  echo "test-docs-links: cannot create a temp directory" >&2
  exit 2
}
git -C "$NO_DOCS" init -q
trap 'rm -rf "$FIXTURE" "$WHOLE" "$NON_REPO" "$NO_DOCS" "$ROOT_BROKEN" "$ROOT_PLAIN"' EXIT

echo "docs-links-on-write.py — reports a link this write broke"
expect 2 "a dangling link under docs/"        "$(payload "$FIXTURE" "$FIXTURE/docs/broken.md")"
expect 2 "a relative path under docs/"        "$(payload "$FIXTURE" "docs/broken.md")"
# The Windows form of the same payload. Without the backslash alternation this
# passed silently — the one platform whose paths differ got no checking at all.
expect 2 "a Windows-style path separator"     "$(printf '{"tool_name":"Edit","cwd":"%s","tool_input":{"file_path":"docs\\\\broken.md"}}' "$FIXTURE")"
expect 2 "a Bash docs mutation"                "$(shell_payload Bash "$FIXTURE" 'touch docs/broken.md')"
expect 2 "a Bash redirect into docs"           "$(shell_payload Bash "$FIXTURE" 'echo x > docs/broken.md')"
expect 2 "a PowerShell docs mutation"          "$(shell_payload PowerShell "$FIXTURE" 'Set-Content docs/broken.md "x"')"
expect 2 "a Monitor docs mutation"             "$(shell_payload Monitor "$FIXTURE" 'touch docs/broken.md')"

echo "docs-links-on-write.py — the root documents are in scope"
expect 2 "a dangling link in a root README"   "$(payload "$ROOT_BROKEN" "README.md")"
expect 2 "a missing non-.md target"           "$(payload "$ROOT_PLAIN" "CONTRIBUTING.md")"
printf 'x\n' > "$ROOT_PLAIN/justfile"
expect 0 "the same target once it exists"     "$(payload "$ROOT_PLAIN" "CONTRIBUTING.md")"

echo "docs-links-on-write.py — the checked tree is the payload's, not the script's"
# This is the worktree bug: the hook lives in THIS repository, whose links all
# resolve. If it derived the root from its own location it would pass here.
expect 2 "a broken tree reached via cwd"      "$(payload "$FIXTURE" "docs/broken.md")"
expect 0 "an intact tree reached via cwd"     "$(payload "$WHOLE" "docs/ok.md")"

echo "docs-links-on-write.py — stays out of the way otherwise"
expect 0 "a write outside docs/"              "$(payload "$FIXTURE" "crates/pos-domain/src/money.rs")"
expect 0 "a non-markdown file under docs/"    "$(payload "$FIXTURE" "docs/plan/diagram.png")"
# Scope is every Markdown file, so a root write is checked like any other. The
# fixture's docs link is still dangling, and the hook is still expected to say so.
expect 2 "a root write is in scope too"       "$(payload "$FIXTURE" "README.md")"
expect 0 "this repository's own links"        "$(payload "$REAL_ROOT" "docs/implementation/README.md")"
expect 0 "a repository with no docs tree"      "$(payload "$NO_DOCS" "docs/x.md")"

echo "docs-links-on-write.py — fail-open errors stay visible"
expect_warning "malformed payload emits systemMessage" 'not json at all'
expect_warning "repository discovery failure emits systemMessage" \
  "$(payload "$NON_REPO" "docs/x.md")"

# Claude Code uses this exec-form launcher in settings.json. Exercise the
# launcher on the current host without claiming native Windows execution.
launcher_payload=$(payload "$FIXTURE" "$FIXTURE/docs/broken.md")
printf '%s' "$launcher_payload" | node "$LAUNCHER" "$HOOK" >/dev/null 2>&1
launcher_got=$?
if [ "$launcher_got" -eq 2 ]; then
  printf '  ok    portable launcher preserves the hook exit status\n'; pass=$((pass+1))
else
  printf '  FAIL  portable launcher exit status  (wanted 2, got %s)\n' "$launcher_got"; fail=$((fail+1))
fi

launcher_warning=$(printf '{}' | node "$LAUNCHER" 2>/dev/null)
launcher_warning_got=$?
if [ "$launcher_warning_got" -eq 0 ] \
   && printf '%s' "$launcher_warning" | "$PYTHON" -c \
      'import json,sys; assert json.load(sys.stdin)["systemMessage"]'; then
  printf '  ok    launcher fail-open warning is visible JSON\n'; pass=$((pass+1))
else
  printf '  FAIL  launcher fail-open warning is not visible JSON\n'; fail=$((fail+1))
fi

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
