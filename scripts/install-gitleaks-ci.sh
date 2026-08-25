#!/usr/bin/env bash
# Install the exact Gitleaks release verified by its upstream SHA-256 digest.
# CI uses this instead of assuming a changing runner image happens to include it.
set -euo pipefail

readonly VERSION=8.30.1
case "$(uname -m)" in
  x86_64|amd64)
    readonly ARCH=x64
    readonly SHA256=551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb
    ;;
  aarch64|arm64)
    readonly ARCH=arm64
    readonly SHA256=e4a487ee7ccd7d3a7f7ec08657610aa3606637dab924210b3aee62570fb4b080
    ;;
  *)
    echo "unsupported Gitleaks CI architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

runner_tmp=${RUNNER_TEMP:?RUNNER_TEMP must identify the GitHub runner temporary directory}
archive="$runner_tmp/gitleaks_${VERSION}_linux_${ARCH}.tar.gz"
install_dir="$runner_tmp/gitleaks-$VERSION"
url="https://github.com/gitleaks/gitleaks/releases/download/v${VERSION}/$(basename "$archive")"

curl --fail --location --silent --show-error \
  --retry 3 --retry-all-errors \
  --output "$archive" "$url"
printf '%s  %s\n' "$SHA256" "$archive" | sha256sum --check --status
mkdir -p "$install_dir"
tar -xzf "$archive" -C "$install_dir" gitleaks
chmod 0755 "$install_dir/gitleaks"
"$install_dir/gitleaks" version

if [ -n "${GITHUB_PATH:-}" ]; then
  printf '%s\n' "$install_dir" >> "$GITHUB_PATH"
else
  echo "GITHUB_PATH is unavailable; installer is CI-only" >&2
  exit 1
fi
