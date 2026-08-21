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

json() { python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$1"; }

edit()    { printf '{"tool_name":"Write","cwd":"%s","tool_input":{"file_path":"%s","content":"x"}}' "$ROOT" "$1"; }
bash_()   { printf '{"tool_name":"Bash","cwd":"%s","tool_input":{"command":%s}}' "$ROOT" "$(json "$1")"; }
monitor() { printf '{"tool_name":"Monitor","cwd":"%s","tool_input":{"command":%s,"description":"w","timeout_ms":1000,"persistent":false}}' "$ROOT" "$(json "$1")"; }

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
expect 2 "truncate a committed migration"          "$(bash_ 'truncate -s 0 crates/pos-db/migrations/0001_init.sql')"
expect 2 "dd writes via of=, not an argument"      "$(bash_ 'dd if=/dev/null of=docs/plan/engineering-blueprint.md')"

# Both protected things are directories. Removing the directory is the same
# forbidden edit with a wider blast radius, and the guard missed it until now.
echo "protect-immutable.py — the directory, not only the files in it"
expect 2 "rm -rf the plan directory"               "$(bash_ 'rm -rf docs/plan')"
expect 2 "rm -rf the migrations directory"         "$(bash_ 'rm -rf crates/pos-db/migrations')"
expect 2 "mv the plan directory aside"             "$(bash_ 'mv docs/plan docs/plan-old')"

# `cd` moves the meaning of every later token. Without following it, `cd docs &&
# rm -rf plan` never spells a protected path in a single token.
echo "protect-immutable.py — cd is followed between segments"
expect 2 "cd docs, then rm -rf plan"               "$(bash_ 'cd docs && rm -rf plan')"
expect 2 "cd into the plan directory, then rm -rf ." "$(bash_ 'cd docs/plan && rm -rf .')"
expect 2 "cd twice, then rm the file"              "$(bash_ 'cd docs && cd plan && rm engineering-blueprint.md')"

# `cp` reads out of a protected directory far more often than it writes into
# one, so only the destination — the last path argument — is a write.
echo "protect-immutable.py — copy destinations"
expect 2 "cp over a source plan"                   "$(bash_ 'cp /tmp/x docs/plan/engineering-blueprint.md')"
expect 2 "install over a committed migration"      "$(bash_ 'install -m 644 /tmp/x crates/pos-db/migrations/0001_init.sql')"
expect 2 "git mv out of the plan directory"        "$(bash_ 'git mv docs/plan/engineering-blueprint.md docs/implementation/x.md')"

# Monitor hands its `command` to the same shell Bash does.
echo "protect-immutable.py — every shell surface, not only Bash"
expect 2 "Monitor writing into a source plan"      "$(monitor 'echo x > docs/plan/engineering-blueprint.md')"
expect 2 "Monitor rm -rf on the plan directory"    "$(monitor 'rm -rf docs/plan')"

# The register ships on Windows and `just` switches to powershell.exe there, so a
# guard that only knows POSIX verbs waves a Windows contributor straight through.
echo "protect-immutable.py — PowerShell verbs and separators"
expect 2 "Remove-Item on a committed migration"    "$(bash_ 'Remove-Item -Force crates/pos-db/migrations/0001_init.sql')"
expect 2 "Remove-Item -Recurse on the plan dir"    "$(bash_ 'Remove-Item -Recurse -Force docs/plan')"
expect 2 "Set-Content into a source plan"          "$(bash_ 'Set-Content docs/plan/engineering-blueprint.md "x"')"
expect 2 "Out-File into a committed migration"     "$(bash_ 'echo x | Out-File crates/pos-db/migrations/0001_init.sql')"
expect 2 "Copy-Item over a source plan"            "$(bash_ 'Copy-Item C:/tmp/x docs/plan/engineering-blueprint.md')"
expect 2 "a Windows separator in the path"         "$(bash_ 'Remove-Item -Force crates\pos-db\migrations\0001_init.sql')"
expect 0 "Copy-Item OUT of the plan directory"     "$(bash_ 'Copy-Item docs/plan/engineering-blueprint.md C:/tmp/x.md')"
expect 0 "Get-Content on a committed migration"    "$(bash_ 'Get-Content crates/pos-db/migrations/0001_init.sql')"

echo "protect-immutable.py — allow cases (the guard must stay out of the way)"
expect 0 "Write a NEW, uncommitted migration"      "$(edit  'crates/pos-db/migrations/0003_catalog.sql')"
expect 0 "Write an implementation doc"             "$(edit  'docs/implementation/01-conventions.md')"
expect 0 "Write ordinary source"                   "$(edit  'crates/pos-domain/src/money.rs')"
expect 0 "read a committed migration"              "$(bash_ 'cat crates/pos-db/migrations/0001_init.sql')"
expect 0 "read a source plan with 2>&1 present"    "$(bash_ 'cat docs/plan/engineering-blueprint.md 2>&1 | head -5')"
expect 0 "grep across the migrations directory"    "$(bash_ 'grep -n TABLE crates/pos-db/migrations/0001_init.sql')"
expect 0 "sqlite3 .read of a committed migration"  "$(bash_ 'sqlite3 :memory: ".read crates/pos-db/migrations/0001_init.sql"')"
expect 0 "heredoc creating the NEXT migration"     "$(bash_ 'cat > crates/pos-db/migrations/0003_catalog.sql <<EOF')"
expect 0 "bare NEW migration name, sed -i"         "$(bash_ "cd crates/pos-db/migrations && sed -i '' 's/a/b/' 0003_catalog.sql")"
expect 0 "unrelated shell command (no git spawned)" "$(bash_ 'cargo nextest run --workspace')"
expect 0 "Monitor on an unrelated command"         "$(monitor 'tail -f target/build.log | grep --line-buffered ERROR')"
# The copy cases in reverse: reading OUT of a protected directory is ordinary.
expect 0 "cp a source plan somewhere else"         "$(bash_ 'cp docs/plan/engineering-blueprint.md /tmp/x.md')"
expect 0 "rsync the plan directory to a backup"    "$(bash_ 'rsync -a docs/plan/ /tmp/backup/')"
expect 0 "tee a plan's contents elsewhere"         "$(bash_ 'cat docs/plan/engineering-blueprint.md | tee /tmp/x.md')"
# The relevance gate matches a bare "plan" so that `cd docs && rm -rf plan` is
# even examined. That widened net must not start refusing ordinary paths.
expect 0 "rm a path that merely contains 'plan'"   "$(bash_ 'rm -rf docs/implementation/plan-notes.tmp')"
expect 0 "rm the plan directory of another repo"   "$(bash_ 'rm -rf /tmp/other/docs/plan')"
# Regression: a write verb quoted in one segment must not implicate a path named
# in another. This blocked a real `git commit` whose message discussed both.
expect 0 "write verb and path in DIFFERENT segments"  "$(bash_ "$(printf 'git add .\ngit commit -m "guard covers Bash because sed -i walks around it"\ngit log crates/pos-db/migrations/0001_init.sql')")"
expect 0 "committed migration named in a commit msg"  "$(bash_ 'git commit -m "verifier applies 0001_init.sql first"')"
expect 0 "a plan path inside a quoted commit message" "$(bash_ 'git commit -m "rm docs/plan was refused, correctly"')"
expect 0 "unrelated tool"                          '{"tool_name":"Grep","cwd":"'"$ROOT"'","tool_input":{"pattern":"x"}}'
expect 0 "malformed payload fails open"            'not json at all'

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
