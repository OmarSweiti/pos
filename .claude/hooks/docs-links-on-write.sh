#!/usr/bin/env bash
# PostToolUse: after a markdown file under docs/ is written, re-check that every
# cross-reference still resolves. This documentation set is only worth its links.
#
# Pure bash + grep on purpose — this fires after every Edit/Write, so it must not
# pay an interpreter start-up to decide it has nothing to do.
set -uo pipefail
payload=$(cat)

# Nothing to do unless the written path is a .md under docs/.
printf '%s' "$payload" | grep -qE '"(file_path|notebook_path)"[[:space:]]*:[[:space:]]*"[^"]*docs/[^"]*\.md"' || exit 0

cd "$(dirname "$0")/../.." || exit 0
out=$(./scripts/check-doc-links.sh 2>&1) || {
  printf '%s\n' "$out" >&2
  echo "A documentation link no longer resolves. just lint and CI run this too — fix it now." >&2
  exit 2   # PostToolUse: the write already happened; this reports it back to Claude
}
exit 0
