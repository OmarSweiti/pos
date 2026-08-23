#!/usr/bin/env bash
# Free, content-based secret scanning for the staged index and CI commit ranges.
# Findings are always fully redacted. Gitleaks is required and operational
# failures refuse closed; silently skipping a missing scanner is not a gate.
set -euo pipefail

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
config="$script_dir/../.gitleaks.toml"

die() {
  echo "secret-scan: ERROR — $1" >&2
  exit 2
}

if ! command -v gitleaks >/dev/null 2>&1; then
  die "gitleaks is required. Install it from https://github.com/gitleaks/gitleaks and retry."
fi
[ -r "$config" ] || die "cannot read $config"

common=(
  --config "$config"
  --redact=100
  --no-banner
  --no-color
  --log-level error
  --ignore-gitleaks-allow
)

scan_staged() {
  local repo="$1"
  git -C "$repo" rev-parse --git-dir >/dev/null 2>&1 \
    || die "$repo is not a readable Git repository"
  gitleaks git "${common[@]}" --staged "$repo"
}

scan_range() {
  local base="$1" head="$2" repo="$3" base_commit head_commit
  base_commit=$(git -C "$repo" rev-parse --verify "$base^{commit}") \
    || die "cannot resolve base commit $base"
  head_commit=$(git -C "$repo" rev-parse --verify "$head^{commit}") \
    || die "cannot resolve head commit $head"
  gitleaks git "${common[@]}" --log-opts="$base_commit..$head_commit" "$repo"
}

scan_history() {
  local repo="$1"
  git -C "$repo" rev-parse --git-dir >/dev/null 2>&1 \
    || die "$repo is not a readable Git repository"
  gitleaks git "${common[@]}" "$repo"
}

self_test() {
  local tmp pass=0 fail=0 got token base head
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/pos-scan-secrets.XXXXXX")
  trap 'rm -rf "$tmp"' RETURN

  git -C "$tmp" init -q
  git -C "$tmp" config user.name Test
  git -C "$tmp" config user.email test@example.com
  printf 'ordinary configuration\n' > "$tmp/config.txt"
  git -C "$tmp" add config.txt
  git -C "$tmp" commit -q -m seed

  expect() {
    local want="$1" label="$2"
    shift 2
    got=0
    "$@" >/dev/null 2>&1 || got=$?
    if [ "$got" -eq "$want" ]; then
      printf '  ok      %s\n' "$label"
      pass=$((pass + 1))
    else
      printf '  FAILED  %s (wanted exit %s, got %s)\n' "$label" "$want" "$got"
      fail=$((fail + 1))
    fi
  }

  echo "secret scan — staged content, not filenames"
  printf 'api_url = "https://example.invalid"\nsha256 = "%064d"\n' 0 > "$tmp/config.txt"
  git -C "$tmp" add config.txt
  expect 0 "ordinary configuration is clean" scan_staged "$tmp"

  # Assemble the fixture so this test script does not itself contain a token
  # signature when the repository's staged diff is scanned.
  token="AK""IAA1B2C3D4E5F6G7H8"
  printf 'service_token = "%s"\n' "$token" > "$tmp/config.txt"
  git -C "$tmp" add config.txt
  expect 1 "a token in an ordinary filename is refused" scan_staged "$tmp"

  printf 'service_token = "not-a-secret-placeholder"\n' > "$tmp/config.txt"
  git -C "$tmp" add config.txt
  expect 0 "an explicit placeholder is not a false positive" scan_staged "$tmp"

  git -C "$tmp" commit -q -m clean
  base=$(git -C "$tmp" rev-parse HEAD)
  token="AK""IAZ9Y8X7W6V5U4T3S2"
  printf 'service_token = "%s"\n' "$token" > "$tmp/config.txt"
  git -C "$tmp" add config.txt
  git -C "$tmp" commit -q -m leaked-fixture
  head=$(git -C "$tmp" rev-parse HEAD)
  expect 1 "a CI commit range containing a token is refused" scan_range "$base" "$head" "$tmp"

  printf '\n%s passed, %s failed\n' "$pass" "$fail"
  [ "$fail" -eq 0 ]
}

case "${1:---staged}" in
  --staged)
    [ "$#" -le 2 ] || die "usage: $0 --staged [repository]"
    scan_staged "${2:-.}"
    ;;
  --range)
    [ "$#" -ge 3 ] && [ "$#" -le 4 ] || die "usage: $0 --range BASE HEAD [repository]"
    scan_range "$2" "$3" "${4:-.}"
    ;;
  --history)
    [ "$#" -le 2 ] || die "usage: $0 --history [repository]"
    scan_history "${2:-.}"
    ;;
  --self-test)
    [ "$#" -eq 1 ] || die "usage: $0 --self-test"
    self_test
    ;;
  *)
    die "usage: $0 --staged [repository] | --range BASE HEAD [repository] | --history [repository] | --self-test"
    ;;
esac
