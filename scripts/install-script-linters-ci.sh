#!/usr/bin/env bash
# Install the exact Ruff and ShellCheck releases verified by their SHA-256
# digests. CI uses this instead of assuming a changing runner image happens to
# include them, and instead of `pip install ruff` resolving to whatever is
# newest on the day — a linter that changes under you turns a green build red on
# a day you are shipping, which is the same reason rust-toolchain.toml is
# pinned. Mirrors scripts/install-gitleaks-ci.sh.
set -euo pipefail

readonly RUFF_VERSION=0.16.4
readonly SHELLCHECK_VERSION=0.11.0

case "$(uname -m)" in
  x86_64|amd64)
    readonly RUFF_TARGET=x86_64-unknown-linux-gnu
    readonly RUFF_SHA256=9cb1234804ddb0f7f57cef3f81623ce5acb990e40af7cce08dc7778c9d7ee96c
    readonly SHELLCHECK_ARCH=x86_64
    readonly SHELLCHECK_SHA256=8c3be12b05d5c177a04c29e3c78ce89ac86f1595681cab149b65b97c4e227198
    ;;
  aarch64|arm64)
    readonly RUFF_TARGET=aarch64-unknown-linux-gnu
    readonly RUFF_SHA256=6f4fe8417b8679e04cc3db046c396b74d1b0f78978145a9fba48c4af53260eef
    readonly SHELLCHECK_ARCH=aarch64
    readonly SHELLCHECK_SHA256=12b331c1d2db6b9eb13cfca64306b1b157a86eb69db83023e261eaa7e7c14588
    ;;
  *)
    echo "unsupported script-linter CI architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

runner_tmp=${RUNNER_TEMP:?RUNNER_TEMP must identify the GitHub runner temporary directory}
install_dir="$runner_tmp/script-linters"
mkdir -p "$install_dir"

fetch_verified() {                     # fetch_verified <url> <destination> <sha256>
  curl --fail --location --silent --show-error \
    --retry 3 --retry-all-errors \
    --output "$2" "$1"
  printf '%s  %s\n' "$3" "$2" | sha256sum --check --status
}

ruff_archive="$runner_tmp/ruff-${RUFF_VERSION}-${RUFF_TARGET}.tar.gz"
fetch_verified \
  "https://github.com/astral-sh/ruff/releases/download/${RUFF_VERSION}/ruff-${RUFF_TARGET}.tar.gz" \
  "$ruff_archive" "$RUFF_SHA256"
tar --extract --gzip --file "$ruff_archive" --directory "$install_dir" --strip-components 1 \
  "ruff-${RUFF_TARGET}/ruff"

shellcheck_archive="$runner_tmp/shellcheck-${SHELLCHECK_VERSION}-${SHELLCHECK_ARCH}.tar.xz"
fetch_verified \
  "https://github.com/koalaman/shellcheck/releases/download/v${SHELLCHECK_VERSION}/shellcheck-v${SHELLCHECK_VERSION}.linux.${SHELLCHECK_ARCH}.tar.xz" \
  "$shellcheck_archive" "$SHELLCHECK_SHA256"
tar --extract --xz --file "$shellcheck_archive" --directory "$install_dir" --strip-components 1 \
  "shellcheck-v${SHELLCHECK_VERSION}/shellcheck"

chmod 0755 "$install_dir/ruff" "$install_dir/shellcheck"

# Prove the pinned binaries are the ones that will actually run.
"$install_dir/ruff" --version
"$install_dir/shellcheck" --version | sed -n '2p'

if [ -n "${GITHUB_PATH:-}" ]; then
  printf '%s\n' "$install_dir" >> "$GITHUB_PATH"
else
  echo "script linters installed to $install_dir"
fi
