#!/usr/bin/env bash
# `pnpm -r --if-present test` skips a package with no test script and exits 0, so
# a package with zero tests is indistinguishable in the log from one that passed.
# This names both groups. It does not fail the build — deciding that a package
# needs tests is a judgement call, and a gate that fires on every new scaffold
# gets disabled. Making the gap visible is the point.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

tested=() untested=()
while IFS= read -r pkg; do
  [ -f "$pkg/package.json" ] || continue
  name=$(node -p "require('./$pkg/package.json').name" 2>/dev/null) || continue
  if node -e "process.exit(require('./$pkg/package.json').scripts?.test ? 0 : 1)" 2>/dev/null; then
    tested+=("$name")
  else
    untested+=("$name")
  fi
done < <(find apps packages -maxdepth 2 -name package.json -not -path '*/node_modules/*' \
         -exec dirname {} \; | sort)

printf 'packages WITH a test script (%d):\n' "${#tested[@]}"
printf '  ✓ %s\n' "${tested[@]:-(none)}"

if [ "${#untested[@]}" -gt 0 ]; then
  printf '\npackages with NO test script (%d) — `--if-present` skipped these:\n' "${#untested[@]}"
  printf '  · %s\n' "${untested[@]}"
  printf '\n::notice::%d workspace package(s) ran no tests.\n' "${#untested[@]}"
fi
