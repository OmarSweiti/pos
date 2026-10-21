#!/usr/bin/env bash
# Print a GitHub token for the engineering app: the GitHub App the owner owns,
# which opens the pull requests prepared for the owner so that the owner can
# approve them — GitHub refuses an approval from a pull request's own author
# (docs/implementation/03-github-workflow.md §6). The token is scoped to one
# repository and expires within the hour; it is printed, never stored.
#
# The app's id and private key live outside every repository, in
# ${POS_ENGINEERING_APP_DIR:-~/.config/pos/engineering-app}; `just engineering-app`
# creates them. The key is read, never printed.
#
#   ./scripts/engineering-token.sh <owner/repo>   exit 3: no engineering app configured
#   ./scripts/engineering-token.sh --self-test    the token's signature, offline
set -euo pipefail

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

# A ten-minute app JWT (RS256), back-dated a minute for clock skew.
jwt() { # jwt <app id> <private key file> [<now>]
  local now=${3:-$(date +%s)} header claims signature
  header=$(printf '{"alg":"RS256","typ":"JWT"}' | b64url)
  claims=$(printf '{"iat":%d,"exp":%d,"iss":"%s"}' $((now - 60)) $((now + 540)) "$1" | b64url)
  signature=$(printf '%s.%s' "$header" "$claims" | openssl dgst -sha256 -sign "$2" -binary | b64url)
  printf '%s.%s.%s' "$header" "$claims" "$signature"
}

self_test() {
  local scratch pass=0 fail=0 token header claims signature padded
  scratch=$(mktemp -d)
  trap 'rm -rf "$scratch"' RETURN
  openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$scratch/key.pem" 2>/dev/null
  openssl pkey -in "$scratch/key.pem" -pubout -out "$scratch/public.pem" 2>/dev/null
  token=$(jwt 4242 "$scratch/key.pem" 1700000000)
  IFS=. read -r header claims signature <<<"$token"
  expect() { # expect <label> <command…>
    local label=$1
    shift
    if "$@" >/dev/null 2>&1; then printf '  ok      %s\n' "$label"; pass=$((pass + 1))
    else printf '  FAILED  %s\n' "$label"; fail=$((fail + 1)); fi
  }
  decode() { # base64url, re-padded
    padded=$(printf '%s' "$1" | tr '_-' '/+')
    while [ $((${#padded} % 4)) -ne 0 ]; do padded="$padded="; done
    printf '%s' "$padded" | openssl base64 -d -A
  }
  echo "engineering token"
  expect "three base64url parts, no padding" test -n "$signature" -a "${token//=/}" = "$token"
  expect "the header says RS256" test "$(decode "$header")" = '{"alg":"RS256","typ":"JWT"}'
  expect "the claims name the app and a window of ten minutes, back-dated a minute" \
    test "$(decode "$claims")" = '{"iat":1699999940,"exp":1700000540,"iss":"4242"}'
  printf '%s.%s' "$header" "$claims" >"$scratch/signed"
  decode "$signature" >"$scratch/signature"
  expect "the signature verifies against the app's public key" \
    openssl dgst -sha256 -verify "$scratch/public.pem" -signature "$scratch/signature" "$scratch/signed"
  printf 'x' >>"$scratch/signed"
  expect "and fails for any other content" \
    bash -c '! openssl dgst -sha256 -verify "$1" -signature "$2" "$3"' _ \
    "$scratch/public.pem" "$scratch/signature" "$scratch/signed"
  POS_ENGINEERING_APP_DIR="$scratch/none" expect "no app configured exits 3" \
    bash -c '"$1" owner/repo; test $? -eq 3' _ "$0"
  printf '\nengineering-token self-test: %s passed, %s failed\n' "$pass" "$fail"
  [ "$fail" -eq 0 ]
}

case "${1:-}" in
  --self-test)
    self_test
    exit
    ;;
  '' | -*)
    echo "usage: engineering-token.sh <owner/repo> | --self-test" >&2
    exit 2
    ;;
esac

repo=$1
[[ "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || {
  echo "engineering-token: $repo is not <owner/repo>" >&2
  exit 2
}
dir=${POS_ENGINEERING_APP_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/pos/engineering-app}
if ! [ -f "$dir/app.json" ] || ! [ -f "$dir/private-key.pem" ]; then
  echo "engineering-token: no engineering app in $dir — run \`just engineering-app\`" >&2
  exit 3
fi
app_id=$(jq -r '.id // empty' "$dir/app.json")
[[ "$app_id" =~ ^[0-9]+$ ]] || {
  echo "engineering-token: $dir/app.json names no app id" >&2
  exit 1
}
token=$(jwt "$app_id" "$dir/private-key.pem")

api() { # api <method> <path> [<json body>] — as the app, with its JWT
  curl --fail --silent --show-error --retry 2 -X "$1" \
    -H "Authorization: Bearer $token" -H 'Accept: application/vnd.github+json' \
    -H 'X-GitHub-Api-Version: 2022-11-28' ${3:+--data "$3"} "https://api.github.com$2"
}

installation=$(api GET "/repos/$repo/installation" | jq -r '.id // empty') || installation=''
[[ "$installation" =~ ^[0-9]+$ ]] || {
  echo "engineering-token: the engineering app is not installed on $repo — run \`just engineering-app status\`" >&2
  exit 1
}
api POST "/app/installations/$installation/access_tokens" "{\"repositories\":[\"${repo#*/}\"]}" |
  jq -r '.token // empty'
