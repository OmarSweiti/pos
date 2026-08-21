#!/usr/bin/env bash
# Conventions §10: the register is RTL by default — "Arabic is not a translation of
# this product. It is the product; English is the toggle." So every layout uses CSS
# LOGICAL properties, and §10 already calls a physical one "a lint failure".
#
# Nothing enforced that until this script. biome.json runs the `recommended` preset,
# which knows nothing about Tailwind utilities or CSS sides, so `pl-4` and
# `margin-left` passed every gate in the repository.
#
# This is not a style preference. A physical side reads correctly in English and
# lays out backwards in Arabic — the product's DEFAULT direction — so no amount of
# reviewing the English build will catch it. It is a correctness check.
#
# Escape hatch, for the rare thing that really is physical (a raster coordinate, a
# hardware offset): put `physical-ok:` and a reason on the line.
#
#   ./scripts/check-logical-css.sh
#   ./scripts/check-logical-css.sh --self-test   # prove the checks still fire
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

# Tailwind utilities with a logical counterpart:
#   pl-/pr-/ml-/mr-  -> ps-/pe-/ms-/me-      left-/right-  -> start-/end-
#   text-left/right  -> text-start/end       border-l/r    -> border-s/e
#   rounded-l/r      -> rounded-s/e          float-left/right -> float-start/end
#
# Anchored on the VALUE, not just the prefix. `\bright-[a-z]` also matches the
# phrase "right-to-left", which is all over a codebase whose whole subject is
# writing direction — three such false positives on the first run. A Tailwind
# inset value is a number, a fraction, `auto`/`full`/`px`/`screen`, or `[...]`.
TWV='(\[|auto\b|full\b|px\b|screen\b|[0-9])'
TW="\b(p|m)[lr]-$TWV|\b(left|right)-$TWV|\btext-(left|right)\b"
TW="$TW"'|\bborder-[lr](-(\[|[0-9])|\b)|\brounded-[lr](-|\b)|\bfloat-(left|right)\b'

# CSS declarations with a logical counterpart. `(-[a-z]+)*` is what catches the
# longhands — `border-left-color` is as physical as `border-left`. The leading
# guard on the bare form stops `--my-left-var` and keeps `inline-start` out.
CSS='(margin|padding|border|inset|scroll-margin|scroll-padding)-(left|right)(-[a-z]+)*[[:space:]]*:'
CSS="$CSS"'|(^|[^-[:alnum:]])(left|right)[[:space:]]*:'
CSS="$CSS"'|\btext-align[[:space:]]*:[[:space:]]*(left|right)\b'
CSS="$CSS"'|\bfloat[[:space:]]*:[[:space:]]*(left|right)\b'
CSS="$CSS"'|\bborder-(top|bottom)-(left|right)-radius'

scan() {                    # scan <root-dir>  -> prints violations, returns count
  local root="$1" found=0 f
  while IFS= read -r f; do
    while IFS= read -r hit; do
      # A documented physical case is allowed; an undocumented one is not.
      case "$hit" in *physical-ok:*) continue ;; esac
      printf 'PHYSICAL  %s:%s\n' "$f" "$hit"
      found=$((found + 1))
    done < <(grep -nE "$TW|$CSS" "$f" 2>/dev/null)
  done < <(find "$root" \
             -path '*/node_modules' -prune -o \
             -path '*/dist' -prune -o \
             -path '*/src-tauri' -prune -o \
             \( -name '*.tsx' -o -name '*.ts' -o -name '*.css' -o -name '*.html' \) -print 2>/dev/null)
  return "$found"
}

self_test() {
  local tmp pass=0 fail=0
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' RETURN
  mkdir -p "$tmp/src"

  case_is() {             # case_is <want: dirty|clean> <label> <file-body>
    local want="$1" label="$2" body="$3" n
    printf '%s\n' "$body" > "$tmp/src/probe.tsx"
    scan "$tmp/src" >/dev/null 2>&1
    n=$?
    if { [ "$want" = dirty ] && [ "$n" -gt 0 ]; } || { [ "$want" = clean ] && [ "$n" -eq 0 ]; }; then
      printf '  ok    %s\n' "$label"; pass=$((pass + 1))
    else
      printf '  FAIL  %s  (wanted %s, found %s hits)\n' "$label" "$want" "$n"; fail=$((fail + 1))
    fi
  }

  echo "check-logical-css.sh — catches a physical side"
  case_is dirty "Tailwind pl-"            '<div className="pl-4" />'
  case_is dirty "Tailwind mr- with a bp"  '<div className="md:mr-2" />'
  case_is dirty "Tailwind left-0"         '<div className="absolute left-0" />'
  case_is dirty "Tailwind text-right"     '<div className="text-right" />'
  case_is dirty "Tailwind border-l-2"     '<div className="border-l-2" />'
  case_is dirty "CSS margin-left"         '.a { margin-left: 4px; }'
  case_is dirty "CSS bare right:"         '.a { position: absolute; right: 0; }'
  case_is dirty "CSS text-align: left"    '.a { text-align: left; }'
  case_is dirty "CSS border-left-color"   '.a { border-left-color: red; }'
  case_is dirty "CSS corner radius"       '.a { border-top-left-radius: 2px; }'

  echo "check-logical-css.sh — leaves the logical form alone"
  case_is clean "Tailwind ps-/pe-"        '<div className="ps-4 pe-2" />'
  case_is clean "Tailwind start-/end-"    '<div className="absolute start-0 end-2" />'
  case_is clean "Tailwind text-start"     '<div className="text-start" />'
  case_is clean "Tailwind border-s-2"     '<div className="border-s-2" />'
  case_is clean "CSS margin-inline-start" '.a { margin-inline-start: 4px; }'
  case_is clean "CSS inset-inline-end"    '.a { inset-inline-end: 0; }'
  case_is clean "CSS text-align: start"   '.a { text-align: start; }'
  # The words themselves are not the problem — only a side used as a layout axis.
  case_is clean "the word in prose"       '// the copyright notice is on the right of the receipt'
  # Three real false positives from the first run, in a codebase whose subject IS
  # writing direction. `\bright-[a-z]` matched every one of them.
  case_is clean "the phrase right-to-left"   '/** the register is right-to-left unless asked */'
  case_is clean "the phrase left-to-right"   '// renders Arabic text left-to-right, or the reverse'
  case_is clean "rounded-lg is not rounded-l" '<div className="rounded-lg" />'
  case_is clean "a documented exception"  '.a { left: 0; /* physical-ok: raster origin */ }'

  printf '\n%s passed, %s failed\n' "$pass" "$fail"
  [ "$fail" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit $?
fi

total=0
for root in apps packages; do
  [ -d "$root" ] || continue
  scan "$root"
  total=$((total + $?))
done

if [ "$total" -ne 0 ]; then
  echo
  echo "$total physical CSS side(s). Conventions §10: the register is RTL by default, so"
  echo "these lay out backwards in Arabic and correctly in English — which is why review"
  echo "does not catch them."
  echo "  pl-/pr-  -> ps-/pe-      ml-/mr-      -> ms-/me-"
  echo "  left-/right- -> start-/end-           text-left/right -> text-start/end"
  echo "  margin-left -> margin-inline-start    right: -> inset-inline-end:"
  echo "If a case really is physical, say so on the line: physical-ok: <reason>"
  exit 1
fi
echo "CSS logical properties only (conventions §10)"
