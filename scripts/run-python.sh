#!/usr/bin/env bash
# Portable Python 3.11+ launcher for Git Bash hooks and repository shell tests.
set -euo pipefail

supports_repository_python() {
  "$@" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else 1)' \
    >/dev/null 2>&1
}

if command -v python3 >/dev/null 2>&1 \
  && supports_repository_python python3; then
  exec python3 "$@"
fi
if command -v py >/dev/null 2>&1 \
  && supports_repository_python py -3; then
  exec py -3 "$@"
fi
if command -v python >/dev/null 2>&1 \
  && supports_repository_python python; then
  exec python "$@"
fi

echo "Python 3.11+ was not found (tried python3, py -3, and python)." >&2
exit 127
