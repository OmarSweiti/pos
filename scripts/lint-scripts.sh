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
# A missing linter is reported as skipped rather than silently passed, the way
# verify-pg-migrations.py reports a missing Postgres. CI installs both at pinned,
# digest-verified versions (scripts/install-script-linters-ci.sh) and therefore
# never skips.
#
# Usage:  ./scripts/lint-scripts.sh
#         ./scripts/lint-scripts.sh --self-test   # prove the shebang scan works
# Exit:   0 clean or cleanly skipped · 1 a finding · 2 could not run at all
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

# ShellCheck severity. `warning` is where this repository already sits: every
# tracked shell file and all three Git hooks are clean at this level today, so
# the gate starts green and stays honest. `info` adds findings that are
# deliberate here (SC2016 single quotes in policy regexes, SC2015 a && b || c).
readonly SHELLCHECK_SEVERITY=warning

shell_files() {
  local file
  while IFS= read -r -d '' file; do
    # Read only the first line; a shebang is the only thing that makes a
    # tracked file a shell script here.
    case "$(head -n 1 "$file" 2>/dev/null)" in
      '#!'*[\ /]sh|'#!'*[\ /]bash|'#!'*bash*|'#!'*/env\ sh) printf '%s\n' "$file" ;;
    esac
  done < <(git ls-files -z)
}

if [ "${1:-}" = "--self-test" ]; then
  # Scanned once into a variable, not re-piped per assertion: `grep -q` exits on
  # its first match, the producer takes SIGPIPE, and under `pipefail` that reads
  # as a failed pipeline. Every assertion below would have reported a miss on a
  # scan that had in fact found the file.
  discovered=$(shell_files)
  found=$(printf '%s\n' "$discovered" | grep -c . )
  failures=0
  for required in .githooks/commit-msg .githooks/pre-commit .githooks/pre-push \
                  scripts/check-doc-links.sh scripts/lint-scripts.sh; do
    if printf '%s\n' "$discovered" | grep -qx "$required"; then
      printf '  ok    shebang scan finds %s\n' "$required"
    else
      printf '  FAIL  shebang scan missed %s\n' "$required"
      failures=$((failures + 1))
    fi
  done
  # A Rust or Markdown file must never be handed to ShellCheck.
  if printf '%s\n' "$discovered" | grep -qE '\.(rs|md|toml|json)$'; then
    printf '  FAIL  shebang scan picked up a non-shell file\n'
    failures=$((failures + 1))
  else
    printf '  ok    shebang scan excludes non-shell files\n'
  fi
  printf '\nlint-scripts self-test: %s shell file(s) discovered, %s failure(s)\n' \
    "$found" "$failures"
  [ "$failures" -eq 0 ] || exit 1
  exit 0
fi

status=0
skipped=()

# ── Python ────────────────────────────────────────────────────────────────
if command -v ruff >/dev/null 2>&1; then
  echo "ruff $(ruff --version | awk '{print $2}')"
  ruff check . || status=1
  ruff format --check . >/dev/null 2>&1 || true   # formatting is not a gate here
else
  skipped+=("ruff — https://docs.astral.sh/ruff/installation/")
fi

# ── Shell ─────────────────────────────────────────────────────────────────
if command -v shellcheck >/dev/null 2>&1; then
  files=()
  while IFS= read -r file; do files+=("$file"); done < <(shell_files)
  if [ "${#files[@]}" -eq 0 ]; then
    echo "lint-scripts: no shell files found — the shebang scan is broken" >&2
    exit 2
  fi
  echo "shellcheck $(shellcheck --version | sed -n 's/^version: //p') — ${#files[@]} file(s), severity=$SHELLCHECK_SEVERITY"
  shellcheck --severity="$SHELLCHECK_SEVERITY" ${files[@]+"${files[@]}"} || status=1
else
  skipped+=("shellcheck — https://github.com/koalaman/shellcheck#installing")
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
fi

for tool in ${skipped[@]+"${skipped[@]}"}; do
  echo "lint-scripts: SKIPPED $tool"
done
if [ "${#skipped[@]}" -ne 0 ]; then
  echo "lint-scripts: the skipped linters above run in CI at pinned versions."
fi

[ "$status" -eq 0 ] || { echo "lint-scripts FAILED"; exit 1; }
echo "policy scripts lint clean"
