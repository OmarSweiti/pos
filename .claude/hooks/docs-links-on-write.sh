#!/usr/bin/env bash
# PostToolUse: after a markdown file under docs/ is written, re-check that every
# cross-reference still resolves. This documentation set is only worth its links.
#
# Pure bash + grep on purpose — this fires after every Edit/Write, so it must not
# pay an interpreter start-up to decide it has nothing to do.
#
# Negative-tested by ./test-docs-links.sh — run it after any change here.
# A guard nobody has seen fail is a guard nobody should trust.
set -uo pipefail
payload=$(cat)

# Nothing to do unless the written path is a .md under docs/. A Windows payload
# carries `docs\\file.md`, so accept either separator rather than silently
# checking nothing on the one platform whose paths look different.
printf '%s' "$payload" \
  | grep -qE '"(file_path|notebook_path)"[[:space:]]*:[[:space:]]*"[^"]*docs(/|\\\\)[^"]*\.md"' \
  || exit 0

# Check the repository the tool call ran in, not the one this script happens to
# live in. In a worktree those are different trees, and deriving the root from
# $0 validates the original checkout while reporting on the edited one.
cwd=$(printf '%s' "$payload" \
      | sed -n 's/.*"cwd"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)
root=""
[ -n "$cwd" ] && root=$(git -C "$cwd" rev-parse --show-toplevel 2>/dev/null)
[ -n "$root" ] || root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root" || exit 0

# A tree without the checker is not a failure to report — it is a tree this hook
# has nothing to say about.
[ -x ./scripts/check-doc-links.sh ] || exit 0

out=$(./scripts/check-doc-links.sh 2>&1) || {
  printf '%s\n' "$out" >&2
  echo "A documentation link no longer resolves. just lint and CI run this too — fix it now." >&2
  exit 2   # PostToolUse: the write already happened; this reports it back to Claude
}
exit 0
