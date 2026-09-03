#!/usr/bin/env bash
# Contract tests for Codex's repository-local lifecycle hooks.
# Synthetic hook payloads exercise both sides of each guard: a safeguard that
# has only been observed passing is not a safeguard we can trust.
set -uo pipefail
cd "$(dirname "$0")" || exit 1

ROOT=$(git rev-parse --show-toplevel)
PYTHON="$ROOT/scripts/run-python.sh"
CONFIG="$ROOT/.codex/hooks.json"
IMMUTABLE_GUARD=./protect-immutable.py
DOCS_HOOK=./docs-links-on-patch.py

pass=0
fail=0

record() { # record <exit-code> <label>
  local got=$1 label=$2
  if [ "$got" -eq 0 ]; then
    printf '  ok    %s\n' "$label"
    pass=$((pass + 1))
  else
    printf '  FAIL  %s\n' "$label"
    fail=$((fail + 1))
  fi
}

expect() { # expect <script> <wanted-code> <label> <payload-json>
  local script=$1 want=$2 label=$3 payload=$4 got
  printf '%s' "$payload" | "$script" >/dev/null 2>&1
  got=$?
  if [ "$got" -eq "$want" ]; then
    printf '  ok    %s\n' "$label"
    pass=$((pass + 1))
  else
    printf '  FAIL  %s  (wanted exit %s, got %s)\n' "$label" "$want" "$got"
    fail=$((fail + 1))
  fi
}

expect_warning() { # expect_warning <script> <label> <payload>
  local script=$1 label=$2 payload=$3 output got
  output=$(printf '%s' "$payload" | "$script" 2>/dev/null)
  got=$?
  if [ "$got" -eq 0 ] && printf '%s' "$output" | "$PYTHON" -c '
import json, sys
message = json.load(sys.stdin).get("systemMessage", "")
assert message.startswith("WARNING:")
'; then
    printf '  ok    %s\n' "$label"
    pass=$((pass + 1))
  else
    printf '  FAIL  %s  (expected exit 0 plus systemMessage)\n' "$label"
    fail=$((fail + 1))
  fi
}

payload() { # payload <event> <cwd> <patch>
  "$PYTHON" -c '
import json, sys
event, cwd, patch = sys.argv[1:]
body = {
    "hook_event_name": event,
    "tool_name": "apply_patch",
    "cwd": cwd,
    "tool_input": {"command": patch},
}
if event == "PostToolUse":
    body["tool_response"] = "Done!"
print(json.dumps(body))
' "$1" "$2" "$3"
}

bash_payload() { # bash_payload <command>
  "$PYTHON" -c '
import json, sys
print(json.dumps({
    "tool_name": "Bash",
    "cwd": sys.argv[1],
    "tool_input": {"command": sys.argv[2]},
}))
' "$ROOT" "$1"
}

echo "Codex hooks — configuration shape"
"$PYTHON" - "$CONFIG" <<'PY'
import json
import os
import subprocess
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    config = json.load(handle)

pre_command = 'python3 "$(git rev-parse --show-toplevel)/.codex/hooks/protect-immutable.py"'
pre_windows = "py -3 -c __import__('runpy').run_path(str(__import__('pathlib').Path(__import__('subprocess').check_output(['git','rev-parse','--show-toplevel'],text=True,encoding='utf-8').strip())/'.codex/hooks/protect-immutable.py'),run_name='__main__')"
post_command = 'python3 "$(git rev-parse --show-toplevel)/.codex/hooks/docs-links-on-patch.py"'
post_windows = "py -3 -c __import__('runpy').run_path(str(__import__('pathlib').Path(__import__('subprocess').check_output(['git','rev-parse','--show-toplevel'],text=True,encoding='utf-8').strip())/'.codex/hooks/docs-links-on-patch.py'),run_name='__main__')"
expected = {
    "description": "Repository safeguards for Codex tool calls.",
    "hooks": {
        "PreToolUse": [{
            "matcher": "^(Bash|apply_patch)$",
            "hooks": [{
                "type": "command",
                "command": pre_command,
                "commandWindows": pre_windows,
                "timeout": 15,
                "statusMessage": "Checking protected paths",
            }],
        }],
        "PostToolUse": [{
            "matcher": "^apply_patch$",
            "hooks": [{
                "type": "command",
                "command": post_command,
                "commandWindows": post_windows,
                "timeout": 30,
                "statusMessage": "Checking documentation links",
            }],
        }],
    },
}
assert config == expected, "hooks.json must retain the exact reviewed event-to-handler contract"

hooks = config["hooks"]
for groups in hooks.values():
    for group in groups:
        for handler in group["hooks"]:
            assert handler["type"] == "command"
            assert "$(git rev-parse --show-toplevel)" in handler["command"]
            windows = handler["commandWindows"]
            assert windows.startswith("py -3 -c ")
            assert '"' not in windows
            code = windows.removeprefix("py -3 -c ")
            compile(code, "<commandWindows>", "exec")
            result = subprocess.run(
                [sys.executable, "-c", code],
                input="not json",
                capture_output=True,
                text=True,
                timeout=10,
            )
            assert result.returncode == 0, result.stderr
            assert json.loads(result.stdout)["systemMessage"].startswith("WARNING:")
            assert 0 < handler["timeout"] <= 30

root = os.path.dirname(os.path.dirname(os.path.abspath(sys.argv[1])))
blocking_payload = json.dumps({
    "hook_event_name": "PreToolUse",
    "tool_name": "apply_patch",
    "cwd": root,
    "tool_input": {
        "command": "*** Begin Patch\n*** Delete File: docs/plan/engineering-blueprint.md\n*** End Patch"
    },
})
blocked = subprocess.run(
    pre_command,
    shell=True,
    executable="/bin/sh",
    cwd=root,
    input=blocking_payload,
    capture_output=True,
    text=True,
    timeout=15,
)
assert blocked.returncode == 2, "configured PreToolUse command must execute the blocking guard"

warned = subprocess.run(
    post_command,
    shell=True,
    executable="/bin/sh",
    cwd=root,
    input="not json",
    capture_output=True,
    text=True,
    timeout=30,
)
assert warned.returncode == 0
assert json.loads(warned.stdout)["systemMessage"].startswith("WARNING:")
PY
record $? "hooks.json binds each event to the exact reviewed handler"

echo "Codex PreToolUse — immutable paths"
expect "$IMMUTABLE_GUARD" 2 "update a committed migration" \
  "$(payload PreToolUse "$ROOT" $'*** Begin Patch\n*** Update File: crates/pos-db/migrations/0001_init.sql\n@@\n-old\n+new\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 2 "delete a source-plan document" \
  "$(payload PreToolUse "$ROOT" $'*** Begin Patch\n*** Delete File: docs/plan/engineering-blueprint.md\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 2 "move a committed migration" \
  "$(payload PreToolUse "$ROOT" $'*** Begin Patch\n*** Update File: crates/pos-db/migrations/0001_init.sql\n*** Move to: crates/pos-db/migrations/0001_moved.sql\n@@\n-old\n+new\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 2 "add a new source-plan document" \
  "$(payload PreToolUse "$ROOT" $'*** Begin Patch\n*** Add File: docs/plan/replacement.md\n+x\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 2 "resolve a path from a session below the git root" \
  "$(payload PreToolUse "$ROOT/docs/plan" $'*** Begin Patch\n*** Update File: engineering-blueprint.md\n@@\n-old\n+new\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 2 "accept Windows separators but still protect the plan" \
  "$(payload PreToolUse "$ROOT" $'*** Begin Patch\n*** Update File: docs\\plan\\engineering-blueprint.md\n@@\n-old\n+new\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 2 "protect a differently-cased plan path" \
  "$(payload PreToolUse "$ROOT" $'*** Begin Patch\n*** Update File: DOCS/PLAN/engineering-blueprint.md\n@@\n-old\n+new\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 2 "protect a differently-cased committed migration" \
  "$(payload PreToolUse "$ROOT" $'*** Begin Patch\n*** Update File: CRATES/POS-DB/MIGRATIONS/0001_INIT.SQL\n@@\n-old\n+new\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 0 "add the next uncommitted migration" \
  "$(payload PreToolUse "$ROOT" $'*** Begin Patch\n*** Add File: crates/pos-db/migrations/9999_test.sql\n+SELECT 1;\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 0 "edit ordinary source" \
  "$(payload PreToolUse "$ROOT" $'*** Begin Patch\n*** Update File: crates/pos-domain/src/money.rs\n@@\n-old\n+new\n*** End Patch')"
expect "$IMMUTABLE_GUARD" 2 "block the same policy through a Bash call" \
  "$(bash_payload 'rm docs/plan/engineering-blueprint.md')"
expect "$IMMUTABLE_GUARD" 2 "block differently-cased paths through Bash" \
  "$(bash_payload 'rm DOCS/PLAN/engineering-blueprint.md')"
expect_warning "$IMMUTABLE_GUARD" "unrecognized relevant patch syntax warns visibly" \
  "$(payload PreToolUse "$ROOT" 'future patch syntax mentioning docs/plan')"
expect_warning "$IMMUTABLE_GUARD" "malformed input fails open with a visible warning" 'not json'

echo "Codex PreToolUse — forward-only SQLx migrations"
expect "$IMMUTABLE_GUARD" 2 "block sqlx migrate revert" \
  "$(bash_payload 'sqlx migrate revert')"
expect "$IMMUTABLE_GUARD" 2 "block a global-option spelling" \
  "$(bash_payload 'sqlx --database-url postgres://localhost/pos migrate revert')"
expect "$IMMUTABLE_GUARD" 2 "look through an environment wrapper" \
  "$(bash_payload 'env DATABASE_URL=postgres://localhost/pos sqlx migrate revert')"
expect "$IMMUTABLE_GUARD" 2 "inspect env split-string execution" \
  "$(bash_payload "env -S 'sqlx migrate revert'")"
expect "$IMMUTABLE_GUARD" 2 "look through wrapper options with values" \
  "$(bash_payload 'sudo -u postgres -- /opt/tools/sqlx migrate revert')"
expect "$IMMUTABLE_GUARD" 2 "look through command end-of-options" \
  "$(bash_payload 'command -- sqlx migrate revert')"
expect "$IMMUTABLE_GUARD" 2 "recognize an absolute executable" \
  "$(bash_payload '/usr/local/bin/sqlx migrate revert')"
expect "$IMMUTABLE_GUARD" 2 "recognize a Windows executable path" \
  "$(bash_payload 'C:\tools\sqlx.exe migrate revert')"
expect "$IMMUTABLE_GUARD" 2 "inspect a literal nested Bash command" \
  "$(bash_payload "bash -lc 'sqlx --database-url postgres://localhost/pos migrate revert'")"
expect "$IMMUTABLE_GUARD" 2 "inspect a literal nested PowerShell command" \
  "$(bash_payload "pwsh -Command 'sqlx migrate revert'")"
expect "$IMMUTABLE_GUARD" 2 "inspect a literal nested cmd command" \
  "$(bash_payload 'cmd.exe /C sqlx migrate revert')"
expect "$IMMUTABLE_GUARD" 0 "allow forward migration execution" \
  "$(bash_payload 'sqlx migrate run')"
expect "$IMMUTABLE_GUARD" 0 "ignore policy words passed to another command" \
  "$(bash_payload "printf '%s' 'sqlx migrate revert'")"
expect "$IMMUTABLE_GUARD" 0 "ignore policy words printed by a nested shell" \
  "$(bash_payload "sh -c \"printf '%s' 'sqlx migrate revert'\"")"

# Fixtures prove the post-write hook reads the edited worktree and the link
# checker's verdict. No repository file is changed by these cases.
BROKEN=$(mktemp -d "${TMPDIR:-/tmp}/pos-test-hooks.XXXXXX")
WHOLE=$(mktemp -d "${TMPDIR:-/tmp}/pos-test-hooks.XXXXXX")
trap 'rm -rf "$BROKEN" "$WHOLE"' EXIT
mkdir -p "$BROKEN/docs"
mkdir -p "$WHOLE/docs"
git -C "$BROKEN" init -q
git -C "$WHOLE" init -q
printf 'See [missing](nowhere.md).\n' > "$BROKEN/docs/broken.md"
printf 'See [present](there.md).\n' > "$WHOLE/docs/ok.md"
printf 'Present.\n' > "$WHOLE/docs/there.md"
printf 'See [external](https://example.com/spec.md).\n' > "$WHOLE/docs/external.md"

echo "Codex PostToolUse — documentation links"
expect "$DOCS_HOOK" 2 "report a broken link after a docs patch" \
  "$(payload PostToolUse "$BROKEN" $'*** Begin Patch\n*** Update File: docs/broken.md\n@@\n-old\n+new\n*** End Patch')"
expect "$DOCS_HOOK" 2 "resolve a docs path from below the git root" \
  "$(payload PostToolUse "$BROKEN/docs" $'*** Begin Patch\n*** Update File: broken.md\n@@\n-old\n+new\n*** End Patch')"
expect "$DOCS_HOOK" 2 "check a differently-cased docs path" \
  "$(payload PostToolUse "$BROKEN" $'*** Begin Patch\n*** Update File: DOCS/BROKEN.MD\n@@\n-old\n+new\n*** End Patch')"
expect "$DOCS_HOOK" 0 "accept an intact documentation tree" \
  "$(payload PostToolUse "$WHOLE" $'*** Begin Patch\n*** Update File: docs/ok.md\n@@\n-old\n+new\n*** End Patch')"
expect "$DOCS_HOOK" 0 "ignore an external URL ending in .md" \
  "$(payload PostToolUse "$WHOLE" $'*** Begin Patch\n*** Update File: docs/external.md\n@@\n-old\n+new\n*** End Patch')"
expect "$DOCS_HOOK" 0 "ignore a source-only patch in a broken tree" \
  "$(payload PostToolUse "$BROKEN" $'*** Begin Patch\n*** Update File: src/example.rs\n@@\n-old\n+new\n*** End Patch')"
expect "$DOCS_HOOK" 2 "a root Markdown patch checks the complete documentation tree" \
  "$(payload PostToolUse "$BROKEN" $'*** Begin Patch\n*** Update File: README.md\n@@\n-old\n+new\n*** End Patch')"
expect_warning "$DOCS_HOOK" "malformed post-tool input fails open with a visible warning" 'not json'

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
