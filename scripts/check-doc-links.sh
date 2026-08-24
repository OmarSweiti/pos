#!/usr/bin/env bash
# Portable entry point for the dependency-free Markdown link checker. Python
# owns parsing so the canonical gate and both agent hooks cannot drift onto
# different subsets of valid Markdown syntax.
set -euo pipefail
cd "$(dirname "$0")/.."
exec ./scripts/run-python.sh ./scripts/check-doc-links.py "$@"
