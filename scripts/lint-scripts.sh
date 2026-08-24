#!/usr/bin/env bash
# Lint the policy code: Python with Ruff, shell with ShellCheck, Ruby with its
# own parser.
#
# About eleven thousand lines of Python, shell and Ruby decide whether a
# migration may be edited, whether a secret may be committed, and whether a pull
# request may weaken the trusted-workflow boundary. `just lint` covered Rust,
# TypeScript and CSS. None of it covered this.
#
# Shell files are found by shebang rather than by extension, because the three
# Git hooks that matter most — commit-msg, pre-commit, pre-push — have no
# extension at all and were the least likely to be checked by a `*.sh` glob.
#
# A missing linter is a setup error. `--allow-missing` exists only for an
# explicitly partial diagnostic and never prints a green-gate result. CI
# installs Ruff and ShellCheck at pinned, digest-verified versions through
# scripts/install-script-linters-ci.sh.
#
# Usage:  ./scripts/lint-scripts.sh
#         ./scripts/lint-scripts.sh --allow-missing
#         ./scripts/lint-scripts.sh --self-test   # prove the shebang scan works
# Exit:   0 clean · 1 a finding · 2 incomplete setup or invalid invocation
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

# ShellCheck severity. `warning` is where this repository already sits: every
# tracked shell file and all three Git hooks are clean at this level today, so
# the gate starts green and stays honest. `info` adds findings that are
# deliberate here (SC2016 single quotes in policy regexes, SC2015 a && b || c).
readonly SHELLCHECK_SEVERITY=warning

usage() {
  echo "usage: $0 [--allow-missing|--self-test]" >&2
  exit 2
}

supported_shell_shebang() {
  case "$1" in
    '#!/bin/sh'|'#!/bin/bash'|'#!/usr/bin/sh'|'#!/usr/bin/bash'|\
    '#!/usr/bin/env sh'|'#!/usr/bin/env bash') return 0 ;;
    *) return 1 ;;
  esac
}

declares_shell_interpreter() {
  local declaration=$1
  [[ "$declaration" =~ ^\#![^[:space:]]*/(sh|bash|dash|ksh|zsh|fish)([[:space:]]|$) ]] ||
    [[ "$declaration" =~ ^\#!/usr/bin/env[[:space:]]+(-S[[:space:]]+)?(sh|bash|dash|ksh|zsh|fish)([[:space:]]|$) ]]
}

shell_policy_required() {
  local file=$1 first_line=$2
  case "$file" in
    *.sh|.githooks/*) return 0 ;;
  esac
  declares_shell_interpreter "$first_line"
}

missing_tools_allowed() {
  local missing_count=$1 allow_missing=$2
  [ "$missing_count" -eq 0 ] || "$allow_missing"
}

discover_shell_files() {
  local file first_line
  SHELL_FILES=()
  SHELL_FILE_COUNT=0
  SHEBANG_PROBLEMS=()
  SHEBANG_PROBLEM_COUNT=0
  while IFS= read -r -d '' file; do
    if [ -L "$file" ]; then
      case "$file" in
        *.sh|.githooks/*)
          SHEBANG_PROBLEMS+=("$file: shell policy files must be repository-owned regular files")
          SHEBANG_PROBLEM_COUNT=$((SHEBANG_PROBLEM_COUNT + 1)) ;;
      esac
      continue
    fi
    first_line=''
    IFS= read -r first_line < "$file" || true
    if supported_shell_shebang "$first_line"; then
      SHELL_FILES+=("$file")
      SHELL_FILE_COUNT=$((SHELL_FILE_COUNT + 1))
    elif shell_policy_required "$file" "$first_line"; then
      SHEBANG_PROBLEMS+=("$file: unsupported or missing shell shebang")
      SHEBANG_PROBLEM_COUNT=$((SHEBANG_PROBLEM_COUNT + 1))
    fi
  done < <(git ls-files -z)
}

if [ "${1:-}" = "--self-test" ]; then
  failures=0
  cases=(
    '#!/bin/sh'
    '#!/bin/bash'
    '#!/usr/bin/sh'
    '#!/usr/bin/bash'
    '#!/usr/bin/env sh'
    '#!/usr/bin/env bash'
  )
  for declaration in "${cases[@]}"; do
    if supported_shell_shebang "$declaration"; then
      printf '  ok    supported shebang: %s\n' "$declaration"
    else
      printf '  FAIL  rejected supported shebang: %s\n' "$declaration"
      failures=$((failures + 1))
    fi
  done
  for declaration in '#!/usr/bin/env -S bash -eu' '#!/bin/zsh' '#!/bin/bash -e'; do
    if supported_shell_shebang "$declaration"; then
      printf '  FAIL  accepted noncanonical shebang: %s\n' "$declaration"
      failures=$((failures + 1))
    elif declares_shell_interpreter "$declaration"; then
      printf '  ok    noncanonical shell declaration is discoverable: %s\n' "$declaration"
    else
      printf '  FAIL  noncanonical shell declaration escaped discovery: %s\n' "$declaration"
      failures=$((failures + 1))
    fi
  done
  if shell_policy_required scripts/missing.sh ''; then
    printf '  ok    a .sh file cannot omit its shebang\n'
  else
    printf '  FAIL  a .sh file with no shebang escaped policy\n'
    failures=$((failures + 1))
  fi
  if shell_policy_required scripts/tool '#!/usr/bin/env python3'; then
    printf '  FAIL  a non-shell executable was classified as shell\n'
    failures=$((failures + 1))
  else
    printf '  ok    non-shell shebangs are excluded\n'
  fi
  if missing_tools_allowed 1 false; then
    printf '  FAIL  strict mode accepted a missing required linter\n'
    failures=$((failures + 1))
  else
    printf '  ok    strict mode refuses a missing required linter\n'
  fi
  if missing_tools_allowed 1 true; then
    printf '  ok    explicit diagnostic mode may report a missing linter\n'
  else
    printf '  FAIL  explicit diagnostic mode did not permit a reported skip\n'
    failures=$((failures + 1))
  fi

  discover_shell_files
  for problem in ${SHEBANG_PROBLEMS[@]+"${SHEBANG_PROBLEMS[@]}"}; do
    printf '  FAIL  %s\n' "$problem"
    failures=$((failures + 1))
  done
  for required in .githooks/commit-msg .githooks/pre-commit .githooks/pre-push \
                  scripts/check-doc-links.sh scripts/lint-scripts.sh; do
    found_required=false
    for discovered in "${SHELL_FILES[@]}"; do
      [ "$discovered" != "$required" ] || found_required=true
    done
    if "$found_required"; then
      printf '  ok    shebang scan finds %s\n' "$required"
    else
      printf '  FAIL  shebang scan missed %s\n' "$required"
      failures=$((failures + 1))
    fi
  done
  # A Rust or Markdown file must never be handed to ShellCheck.
  for discovered in "${SHELL_FILES[@]}"; do
    case "$discovered" in
      *.rs|*.md|*.toml|*.json)
        printf '  FAIL  shebang scan picked up non-shell file %s\n' "$discovered"
        failures=$((failures + 1)) ;;
    esac
  done
  [ "$failures" -ne 0 ] || printf '  ok    shebang scan excludes non-shell files\n'
  printf '\nlint-scripts self-test: %s shell file(s) discovered, %s failure(s)\n' \
    "$SHELL_FILE_COUNT" "$failures"
  [ "$failures" -eq 0 ] || exit 1
  exit 0
fi

[ "$#" -le 1 ] || usage
allow_missing=false
case "${1:-}" in
  '') ;;
  --allow-missing) allow_missing=true ;;
  *) usage ;;
esac

discover_shell_files
if [ "$SHEBANG_PROBLEM_COUNT" -ne 0 ]; then
  for problem in ${SHEBANG_PROBLEMS[@]+"${SHEBANG_PROBLEMS[@]}"}; do
    echo "lint-scripts: ERROR — $problem" >&2
  done
  exit 2
fi

status=0
skipped=()
skipped_count=0

# ── Python ────────────────────────────────────────────────────────────────
if command -v ruff >/dev/null 2>&1; then
  echo "ruff $(ruff --version | awk '{print $2}')"
  ruff check . || status=1
  ruff format --check . >/dev/null 2>&1 || true   # formatting is not a gate here
else
  skipped+=("ruff — https://docs.astral.sh/ruff/installation/")
  skipped_count=$((skipped_count + 1))
fi

# ── Shell ─────────────────────────────────────────────────────────────────
if command -v shellcheck >/dev/null 2>&1; then
  if [ "$SHELL_FILE_COUNT" -eq 0 ]; then
    echo "lint-scripts: no shell files found — the shebang scan is broken" >&2
    exit 2
  fi
  echo "shellcheck $(shellcheck --version | sed -n 's/^version: //p') — $SHELL_FILE_COUNT file(s), severity=$SHELLCHECK_SEVERITY"
  shellcheck --severity="$SHELLCHECK_SEVERITY" "${SHELL_FILES[@]}" || status=1
else
  skipped+=("shellcheck — https://github.com/koalaman/shellcheck#installing")
  skipped_count=$((skipped_count + 1))
fi

# ── Ruby ──────────────────────────────────────────────────────────────────
# Only a syntax check: this repository has one Ruby file and it is already a
# fail-closed policy checker with 170 of its own self-tests. A style linter
# would be a new dependency for a single file.
if command -v ruby >/dev/null 2>&1; then
  while IFS= read -r -d '' file; do
    ruby -c "$file" >/dev/null || status=1
  done < <(git ls-files -z '*.rb')
  echo "ruby -c: syntax OK"
else
  skipped+=("ruby — needed for workflow policy anyway; see just policy-tools-check")
  skipped_count=$((skipped_count + 1))
fi

for tool in ${skipped[@]+"${skipped[@]}"}; do
  echo "lint-scripts: SKIPPED $tool"
done
if [ "$skipped_count" -ne 0 ]; then
  if ! missing_tools_allowed "$skipped_count" "$allow_missing"; then
    echo "lint-scripts: ERROR — install every required linter; this gate is incomplete." >&2
    exit 2
  fi
  echo "lint-scripts: partial diagnostic only; omitted linters were explicitly allowed."
fi

[ "$status" -eq 0 ] || { echo "lint-scripts FAILED"; exit 1; }
if "$allow_missing" && [ "$skipped_count" -ne 0 ]; then
  echo "policy-script diagnostic completed with explicit skips"
else
  echo "policy scripts lint clean"
fi
