#!/usr/bin/env bash
# POSIX compatibility entry point. Claude Code itself uses a shell-free exec
# form so the Python hook is available to both Bash and PowerShell tool calls.
set -uo pipefail
exec "$(dirname "$0")/../../scripts/run-python.sh" \
  "$(dirname "$0")/docs-links-on-write.py"
