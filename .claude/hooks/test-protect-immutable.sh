#!/usr/bin/env bash
# Negative test for protect-immutable.py. A guard nobody has seen fail is a
# guard nobody should trust (phase-0-closeout.md, on the two Phase-0 guards).
#
# Each case feeds a synthetic PreToolUse payload and asserts the exit code:
#   2 = denied   0 = allowed
set -uo pipefail
cd "$(dirname "$0")" || exit 1
ROOT=$(git rev-parse --show-toplevel)
PYTHON="$ROOT/scripts/run-python.sh"
GUARD=./protect-immutable.py

pass=0; fail=0

expect() {           # expect <want-code> <label> <payload-json>
  local want=$1 label=$2 payload=$3 got
  printf '%s' "$payload" | "$PYTHON" "$GUARD" >/dev/null 2>&1
  got=$?
  if [ "$got" -eq "$want" ]; then
    printf '  ok    %s\n' "$label"; pass=$((pass+1))
  else
    printf '  FAIL  %s  (wanted exit %s, got %s)\n' "$label" "$want" "$got"; fail=$((fail+1))
  fi
}

expect_warning() {   # expect_warning <label> <payload-json>
  local label=$1 payload=$2 got output json_ok
  output=$(printf '%s' "$payload" | "$PYTHON" "$GUARD" 2>/dev/null)
  got=$?
  printf '%s' "$output" | "$PYTHON" -c \
    'import json,sys; value=json.load(sys.stdin); assert value.get("systemMessage")' \
    >/dev/null 2>&1
  json_ok=$?
  if [ "$got" -eq 0 ] && [ "$json_ok" -eq 0 ]; then
    printf '  ok    %s\n' "$label"; pass=$((pass+1))
  else
    printf '  FAIL  %s  (exit %s, structured warning %s)\n' \
      "$label" "$got" "$json_ok"; fail=$((fail+1))
  fi
}

json() { "$PYTHON" -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$1"; }

edit()    { printf '{"tool_name":"Write","cwd":"%s","tool_input":{"file_path":"%s","content":"x"}}' "$ROOT" "$1"; }
read_()   { printf '{"tool_name":"Read","cwd":"%s","tool_input":{"file_path":"%s"}}' "$ROOT" "$1"; }
read_at() { printf '{"tool_name":"Read","cwd":"%s","tool_input":{"file_path":"%s"}}' "$1" "$2"; }
grep_()   { printf '{"tool_name":"Grep","cwd":"%s","tool_input":{"pattern":"needle","path":"%s","glob":"%s"}}' "$ROOT" "$1" "$2"; }
glob_()   { printf '{"tool_name":"Glob","cwd":"%s","tool_input":{"path":"%s","pattern":"%s"}}' "$ROOT" "$1" "$2"; }
bash_()   { printf '{"tool_name":"Bash","cwd":"%s","tool_input":{"command":%s}}' "$ROOT" "$(json "$1")"; }
powershell() { printf '{"tool_name":"PowerShell","cwd":"%s","tool_input":{"command":%s}}' "$ROOT" "$(json "$1")"; }
monitor() { printf '{"tool_name":"Monitor","cwd":"%s","tool_input":{"command":%s,"description":"w","timeout_ms":1000,"persistent":false}}' "$ROOT" "$(json "$1")"; }

echo "protect-immutable.py — deny cases (the guard must fire)"
expect 2 "Write to a committed SQLite migration"   "$(edit  'crates/pos-db/migrations/0001_init.sql')"
expect 2 "Write to a committed Postgres migration" "$(edit  'apps/server/migrations/20260819200319_init.sql')"
expect 2 "Write via absolute path"                 "$(edit  "$ROOT/crates/pos-db/migrations/0001_init.sql")"
expect 2 "Write to a source plan"                  "$(edit  'docs/plan/engineering-blueprint.md')"
expect 2 "Write to a differently-cased source plan" "$(edit  'DOCS/PLAN/engineering-blueprint.md')"
expect 2 "Write to a differently-cased migration"  "$(edit  'CRATES/POS-DB/MIGRATIONS/0001_INIT.SQL')"
expect 2 "sed -i on a committed migration"         "$(bash_ "sed -i '' 's/a/b/' crates/pos-db/migrations/0001_init.sql")"
expect 2 "redirect into a source plan"             "$(bash_ 'echo hi > docs/plan/engineering-blueprint.md')"
expect 2 "rm a committed migration"                "$(bash_ 'rm crates/pos-db/migrations/0001_init.sql')"
expect 2 "rm a differently-cased migration"        "$(bash_ 'rm CRATES/POS-DB/MIGRATIONS/0001_INIT.SQL')"
expect 2 "tee into a committed migration"          "$(bash_ 'echo x | tee crates/pos-db/migrations/0001_init.sql')"
expect 2 "cd into migrations, then bare sed -i"    "$(bash_ "cd crates/pos-db/migrations && sed -i '' 's/a/b/' 0001_init.sql")"
expect 2 "bare committed migration name, rm"       "$(bash_ 'rm 20260819200319_init.sql')"
expect 2 "bare differently-cased migration, rm"    "$(bash_ 'rm 20260819200319_INIT.SQL')"
expect 2 "truncate a committed migration"          "$(bash_ 'truncate -s 0 crates/pos-db/migrations/0001_init.sql')"
expect 2 "dd writes via of=, not an argument"      "$(bash_ 'dd if=/dev/null of=docs/plan/engineering-blueprint.md')"
expect 2 "git diff writes via --output="            "$(bash_ 'git diff --output=docs/plan/engineering-blueprint.md HEAD~1')"
expect 2 "git show writes via --output argument"   "$(bash_ 'git show --output docs/plan/engineering-blueprint.md HEAD')"
expect 2 "touch creates a source plan"             "$(bash_ 'touch docs/plan/new-source.md')"

echo "protect-immutable.py — sensitive environment reads"
expect 2 "Read an arbitrary production env suffix" "$(read_ 'apps/server/.env.prod')"
expect 2 "Read a backup env suffix"                "$(read_ 'apps/server/.env.backup')"
expect 2 "Read a case-varied live env file"        "$(read_ 'apps/server/.ENV.QA')"
expect 0 "Read the tracked env example"            "$(read_ 'apps/server/.env.example')"
expect 2 "Grep a direct live env path"             "$(grep_ 'apps/server/.env.preview' '')"
expect 2 "Grep with a live env glob"               "$(grep_ 'apps/server' '.env.*')"
expect 2 "Glob for live env filenames"             "$(glob_ '.' '**/.env.*')"
expect 2 "Glob cannot disguise .env with brackets"  "$(glob_ '.' '**/[.]env*')"
expect 2 "Grep cannot disguise .env with wildcards" "$(grep_ '.' '**/.e?v*')"
expect 2 "Glob cannot disguise .env with braces"    "$(glob_ '.' '**/{.env,.env.prod}')"
expect 0 "A normal Rust discovery glob remains usable" "$(glob_ '.' '**/*.rs')"

SECRET_FIXTURE=$(mktemp -d)
mkdir -p "$SECRET_FIXTURE/apps/server"
git -C "$SECRET_FIXTURE" init -q
printf 'SECRET=not-read\n' > "$SECRET_FIXTURE/apps/server/.env.prod"
ln -s .env.prod "$SECRET_FIXTURE/apps/server/safe.txt"
expect 2 "Read an innocently named symlink to a live env" \
  "$(read_at "$SECRET_FIXTURE" 'apps/server/safe.txt')"
rm -rf "$SECRET_FIXTURE"

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
expect 2 "cp -t into the plan directory"           "$(bash_ 'cp -t docs/plan /tmp/x')"
expect 2 "cp --target-directory into the plan"     "$(bash_ 'cp --target-directory=docs/plan /tmp/x')"
expect 2 "install over a committed migration"      "$(bash_ 'install -m 644 /tmp/x crates/pos-db/migrations/0001_init.sql')"
expect 2 "git mv out of the plan directory"        "$(bash_ 'git mv docs/plan/engineering-blueprint.md docs/implementation/x.md')"

# Arbitrary interpreter source cannot be evaluated safely. A literal protected
# path is therefore denied conservatively whether the source appears to read or
# write it. Dynamically constructed paths remain a documented parser limit.
echo "protect-immutable.py — literal paths passed through interpreters"
expect 2 "Python inline write to a source plan"     "$(bash_ "python3 -c \"open('docs/plan/new.md','w').write('x')\"")"
expect 2 "Node inline write to a migration"         "$(bash_ "node -e \"require('fs').writeFileSync('crates/pos-db/migrations/0001_init.sql','x')\"")"
expect 2 "shell -c write to a source plan"          "$(bash_ "sh -c 'echo x > docs/plan/new.md'")"

# Monitor hands its `command` to the same shell Bash does.
echo "protect-immutable.py — every shell surface, not only Bash"
expect 2 "Monitor writing into a source plan"      "$(monitor 'echo x > docs/plan/engineering-blueprint.md')"
expect 2 "Monitor rm -rf on the plan directory"    "$(monitor 'rm -rf docs/plan')"

# These are real PowerShell tool payloads, not PowerShell text mislabeled as a
# Bash call. They test matcher routing and parsing on this host; native Windows
# process dispatch is a separate platform check and is not claimed here.
echo "protect-immutable.py — PowerShell tool, verbs, and separators"
expect 2 "PowerShell Remove-Item migration"         "$(powershell 'Remove-Item -Force crates/pos-db/migrations/0001_init.sql')"
expect 2 "PowerShell Remove-Item plan directory"    "$(powershell 'Remove-Item -Recurse -Force docs/plan')"
expect 2 "PowerShell Set-Content source plan"       "$(powershell 'Set-Content -Path docs/plan/engineering-blueprint.md -Value "x"')"
expect 2 "PowerShell Out-File migration"           "$(powershell 'echo x | Out-File crates/pos-db/migrations/0001_init.sql')"
expect 2 "PowerShell Copy-Item destination"         "$(powershell 'Copy-Item C:/tmp/x -Destination docs/plan/engineering-blueprint.md')"
expect 2 "PowerShell Windows path separators"      "$(powershell 'Remove-Item -Force crates\pos-db\migrations\0001_init.sql')"
expect 2 "PowerShell New-Item source plan"          "$(powershell 'New-Item -ItemType File -Path docs/plan/new.md')"
expect 0 "PowerShell Copy-Item out of plan"         "$(powershell 'Copy-Item docs/plan/engineering-blueprint.md -Destination C:/tmp/x.md')"
expect 0 "PowerShell Get-Content migration"         "$(powershell 'Get-Content crates/pos-db/migrations/0001_init.sql')"

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
# A dynamically assembled interpreter path is not provable from the command
# line. The OS denyWrite boundary protects docs/plan on supported platforms;
# Git and CI remain the cross-platform backstops.
expect 0 "dynamic interpreter path is outside parser proof" "$(bash_ "python3 -c \"p='docs/'+'plan/new.md'; open(p,'w')\"")"
expect 0 "unrelated tool"                          '{"tool_name":"Grep","cwd":"'"$ROOT"'","tool_input":{"pattern":"x"}}'

echo "protect-immutable.py — fail-open errors stay visible"
expect_warning "malformed payload emits systemMessage" 'not json at all'
NON_REPO=$(mktemp -d)
EMPTY_REPO=$(mktemp -d)
trap 'rm -rf "$NON_REPO" "$EMPTY_REPO"' EXIT
git -C "$EMPTY_REPO" init -q
EMPTY_REPO=$(cd "$EMPTY_REPO" && pwd -P)
expect_warning "repository discovery failure emits systemMessage" \
  "$(printf '{"tool_name":"Bash","cwd":"%s","tool_input":{"command":"rm crates/pos-db/migrations/0001_init.sql"}}' "$NON_REPO")"
expect_warning "HEAD enumeration failure emits systemMessage" \
  "$(printf '{"tool_name":"Bash","cwd":"%s","tool_input":{"command":"rm crates/pos-db/migrations/0001_init.sql"}}' "$EMPTY_REPO")"

printf '\n%s passed, %s failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
