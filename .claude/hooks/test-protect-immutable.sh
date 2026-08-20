#!/usr/bin/env bash
# Negative test for protect-immutable.py. A guard nobody has seen fail is a
# guard nobody should trust (phase-0-closeout.md, on the two Phase-0 guards).
#
# Each case feeds a synthetic PreToolUse payload and asserts the exit code:
#   2 = denied   0 = allowed
set -uo pipefail
cd "$(dirname "$0")" || exit 1
ROOT=$(git rev-parse --show-toplevel)
GUARD=./protect-immutable.py

pass=0; fail=0

expect() {           # expect <want-code> <label> <payload-json>
  local want=$1 label=$2 payload=$3 got
  printf '%s' "$payload" | python3 "$GUARD" >/dev/null 2>&1
  got=$?
  if [ "$got" -eq "$want" ]; then
    printf '  ok    %s\n' "$label"; pass=$((pass+1))
  else
    printf '  FAIL  %s  (wanted exit %s, got %s)\n' "$label" "$want" "$got"; fail=$((fail+1))
  fi
}

edit()  { printf '{"tool_name":"Write","cwd":"%s","tool_input":{"file_path":"%s","content":"x"}}' "$ROOT" "$1"; }
bash_() { printf '{"tool_name":"Bash","cwd":"%s","tool_input":{"command":%s}}' "$ROOT" "$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$1")"; }

echo "protect-immutable.py — deny cases (the guard must fire)"
expect 2 "Write to a committed SQLite migration"   "$(edit  'crates/pos-db/migrations/0001_init.sql')"
expect 2 "Write to a committed Postgres migration" "$(edit  'apps/server/migrations/20260819200319_init.sql')"
expect 2 "Write via absolute path"                 "$(edit  "$ROOT/crates/pos-db/migrations/0001_init.sql")"
expect 2 "Write to a source plan"                  "$(edit  'docs/plan/engineering-blueprint.md')"
expect 2 "sed -i on a committed migration"         "$(bash_ "sed -i '' 's/a/b/' crates/pos-db/migrations/0001_init.sql")"
expect 2 "redirect into a source plan"             "$(bash_ 'echo hi > docs/plan/engineering-blueprint.md')"
expect 2 "rm a committed migration"                "$(bash_ 'rm crates/pos-db/migrations/0001_init.sql')"
expect 2 "tee into a committed migration"          "$(bash_ 'echo x | tee crates/pos-db/migrations/0001_init.sql')"
expect 2 "cd into migrations, then bare sed -i"    "$(bash_ "cd crates/pos-db/migrations && sed -i '' 's/a/b/' 0001_init.sql")"
expect 2 "bare committed migration name, rm"       "$(bash_ 'rm 20260819200319_init.sql')"

echo "protect-immutable.py — allow cases (the guard must stay out of the way)"
expect 0 "Write a NEW, uncommitted migration"      "$(edit  'crates/pos-db/migrations/0002_catalog.sql')"
expect 0 "Write an implementation doc"             "$(edit  'docs/implementation/01-conventions.md')"
expect 0 "Write ordinary source"                   "$(edit  'crates/pos-domain/src/money.rs')"
expect 0 "read a committed migration"              "$(bash_ 'cat crates/pos-db/migrations/0001_init.sql')"
expect 0 "read a source plan with 2>&1 present"    "$(bash_ 'cat docs/plan/engineering-blueprint.md 2>&1 | head -5')"
expect 0 "grep across the migrations directory"    "$(bash_ 'grep -n TABLE crates/pos-db/migrations/0001_init.sql')"
expect 0 "sqlite3 .read of a committed migration"  "$(bash_ 'sqlite3 :memory: \".read crates/pos-db/migrations/0001_init.sql\"')"
expect 0 "heredoc creating the NEXT migration"     "$(bash_ 'cat > crates/pos-db/migrations/0002_catalog.sql <<EOF')"
expect 0 "bare NEW migration name, sed -i"         "$(bash_ "cd crates/pos-db/migrations && sed -i '' 's/a/b/' 0002_catalog.sql")"
expect 0 "unrelated shell command (no git spawned)" "$(bash_ 'cargo nextest run --workspace')"
# Regression: a write verb quoted in one segment must not implicate a path named
# in another. This blocked a real `git commit` whose message discussed both.
expect 0 "write verb and path in DIFFERENT segments"  "$(bash_ "$(printf 'git add .\ngit commit -m \"guard covers Bash because sed -i walks around it\"\ngit log crates/pos-db/migrations/0001_init.sql')")"
expect 0 "committed migration named in a commit msg"  "$(bash_ "$(printf 'git commit -m \"verifier applies 0001_init.sql first\"')")"
expect 0 "unrelated tool"                          '{"tool_name":"Grep","cwd":"'"$ROOT"'","tool_input":{"pattern":"x"}}'
expect 0 "malformed payload fails open"            'not json at all'

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
