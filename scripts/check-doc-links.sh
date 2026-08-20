#!/usr/bin/env bash
# Fails if any relative markdown link under docs/ points at a file that does
# not exist. This documentation set is only worth its cross-references.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

broken=0
for f in $(find docs -name '*.md'); do
  d=$(dirname "$f")
  # link targets ending in .md, anchors stripped; `*` skips globs used in prose
  targets=$(grep -oE '\]\([^)*[:space:]]+\.md(#[^)]*)?\)' "$f" 2>/dev/null \
            | sed -E 's/^\]\(//; s/\)$//; s/#.*$//' | sort -u) || true
  for t in $targets; do
    if [ ! -e "$d/$t" ]; then
      echo "BROKEN  $f  ->  $t"
      broken=1
    fi
  done
done

if [ "$broken" -ne 0 ]; then
  echo "documentation link check FAILED"
  exit 1
fi
echo "documentation links OK"
