#!/usr/bin/env bash
# A guard nobody has seen fail is a guard nobody should trust.
# Negative-tests every git hook in this directory. Run after touching one:
#   just guards
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
HOOKS="$PWD/.githooks"
PYTHON_RUNNER="$PWD/scripts/run-python.sh"
pass=0; fail=0
ok()   { printf '  ok      %s\n' "$1"; pass=$((pass+1)); }
bad()  { printf '  FAILED  %s\n' "$1"; fail=$((fail+1)); }

# expect_msg <expected-exit> <label> <subject>
expect_msg() {
  local want="$1" label="$2" subject="$3" f
  f=$(mktemp); printf '%s\n' "$subject" > "$f"
  "$HOOKS/commit-msg" "$f" >/dev/null 2>&1
  local got=$?
  rm -f "$f"
  [ "$got" -eq "$want" ] && ok "$label" || bad "$label (wanted exit $want, got $got)"
}

echo "commit-msg — accepts what conventions §8 describes"
expect_msg 0 "a microstep commit"          'feat(domain): tax engine, inclusive extraction   [1.3.4]'
expect_msg 0 "an em-dash step tag"         'docs(impl): phase 2 harness              [—]'
expect_msg 0 "a range step tag"            'chore(repo): phase-0 close-out                [0.1.1–0.4.3]'
expect_msg 0 "repository automation uses the same grammar" \
                                              'chore(repo): bump the dependency group   [—]'
expect_msg 0 "a merge commit passes through" 'Merge branch development into staging'
expect_msg 0 "a revert passes through"     'Revert "feat(domain): tax engine   [1.3.4]"'
expect_msg 0 "a fixup passes through"      'fixup! feat(domain): tax engine'

echo "commit-msg — refuses what it must"
expect_msg 1 "no type/scope prefix"        'added the tax engine   [1.3.4]'
expect_msg 1 "a type outside the list"     'build(domain): tax engine   [1.3.4]'
expect_msg 1 "a scope outside the list"    'feat(claude): agent rules   [—]'
expect_msg 1 "a missing step tag"          'feat(domain): tax engine'
expect_msg 1 "an empty step tag"           'feat(domain): tax engine   []'
expect_msg 1 "an arbitrary step tag"       'feat(domain): tax engine   [banana]'
expect_msg 1 "an incomplete numeric tag"   'feat(domain): tax engine   [1.3]'
expect_msg 1 "an ASCII-hyphen range"       'feat(domain): tax engine   [1.3.4-1.3.5]'
expect_msg 1 "a reversed step range"       'feat(domain): tax engine   [2.1.1–1.9.9]'
expect_msg 1 "a trailing period"           'feat(domain): tax engine.   [1.3.4]'
expect_msg 1 "an empty summary"            'feat(domain): '
expect_msg 1 "a summary over 72 characters" \
  'feat(domain): a summary written at such length that it stops being a summary and becomes prose   [1.3.4]'

# expect_body <expected-exit> <label> <full multi-line message>
expect_body() {
  local want="$1" label="$2" body="$3" f got
  f=$(mktemp); printf '%s\n' "$body" > "$f"
  "$HOOKS/commit-msg" "$f" >/dev/null 2>&1
  got=$?
  rm -f "$f"
  [ "$got" -eq "$want" ] && ok "$label" || bad "$label (wanted exit $want, got $got)"
}

echo "commit-msg — no agent attribution reaches this history"
expect_body 1 "a Claude co-author trailer" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>'
expect_body 1 "lower-cased, short form" \
  'fix(db): qty to milli-units   [1.1.7]

Co-authored-by: Claude <noreply@anthropic.com>'
expect_body 1 "an anthropic address alone" \
  'fix(db): qty to milli-units   [1.1.7]

Co-Authored-By: Assistant <noreply@anthropic.com>'
expect_body 1 "a generated-with line" \
  'chore(repo): tooling   [—]

🤖 Generated with Claude Code'
expect_body 1 "attribution on a MERGE commit too — the rule is absolute" \
  'Merge branch development into staging

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>'
expect_body 0 "a real human co-author is still welcome" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Jane Cashier <jane@example.com>'
expect_body 0 "an ordinary body with a why" \
  'feat(domain): tax engine   [1.3.4]

Inclusive extraction must be exact before the cart machine can price
anything. I-1: one rounding, at the boundary.'
expect_body 0 "a commented-out trailer from a template" \
  'feat(domain): tax engine   [1.3.4]

# Co-Authored-By: Claude <noreply@anthropic.com>'
expect_body 1 "Copilot" \
  'feat(domain): tax engine   [1.3.4]

Co-authored-by: Copilot <copilot@github.com>'
expect_body 1 "a github-actions bot account" \
  'chore(repo): bump   [—]

Co-authored-by: github-actions[bot] <41898282+github-actions[bot]@users.noreply.github.com>'
expect_body 1 "a dependabot account" \
  'chore(repo): bump serde   [—]

Co-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>'
expect_body 1 "a model identity behind a human-looking address" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Claude Opus 5 <someone@example.com>'
expect_body 1 "OpenAI Codex behind a human-looking address" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: OpenAI Codex <someone@example.com>'
expect_body 1 "GitHub Copilot behind a human-looking address" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: GitHub Copilot <someone@example.com>'
# The instruction is "no agent attribution", not "no Claude attribution", so the
# address list covers other coding agents too. It is an ENUMERATION and it lags:
# an agent whose domain is not in .githooks/commit-msg passes. That is a known
# hole, not an oversight — there is no generic signal, because the obvious one
# (`noreply@`) is what GitHub gives real humans who keep their address private.
# Add a domain here and in the hook when a new agent shows up.
expect_body 1 "Cursor" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Cursor <hi@cursor.com>'
expect_body 1 "Devin" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Devin <devin@cognition.ai>'
expect_body 1 "a malformed trailer with no angle brackets" \
  'chore(repo): bump   [—]

Co-Authored-By: github-actions[bot]@users.noreply.github.com'

# The allow-list is as load-bearing as the deny-list. A guard that refuses a real
# contributor is discovered by the one person it affects, and by then it has
# already been rude to them. Claude is a common French given name.
echo "commit-msg — a human co-author is never blocked, whatever their name"
expect_body 0 "a human given-named Claude" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Claude Dubois <claude.dubois@example.fr>'
expect_body 0 "a human surnamed Botha" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Robert Botha <r.botha@example.co.za>'
expect_body 0 "a human surnamed Anthropov" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Anthony Anthropov <a.anthropov@example.com>'
# Matched on `devin@`, not `devin`, precisely so this person is not refused.
expect_body 0 "a human given-named Devin" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Devin Ellis <devin.ellis@example.com>'
expect_body 0 "a human given-named Cody" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Cody Fisher <cody.fisher@example.com>'
# GitHub's own privacy address for real people. Refusing `noreply@` generically
# would block them, which is why the hook enumerates instead.
expect_body 0 "a human using GitHub's private address" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Jane Smith <jane@users.noreply.github.com>'

# expect_push <expected-exit> <label> <stdin line> [env] [remote]
expect_push() {
  local want="$1" label="$2" line="$3" envset="${4:-}" remote="${5:-origin}"
  local got
  if [ -n "$envset" ]; then
    got=$(env "$envset" bash -c "printf '%s\n' '$line' | '$HOOKS/pre-push' '$remote' git@x >/dev/null 2>&1; echo \$?")
  else
    got=$(printf '%s\n' "$line" | "$HOOKS/pre-push" "$remote" git@x >/dev/null 2>&1; echo $?)
  fi
  [ "$got" -eq "$want" ] && ok "$label" || bad "$label (wanted exit $want, got $got)"
}

head_sha=$(git rev-parse HEAD)
zero=0000000000000000000000000000000000000000

echo "pre-push — the protected branches take pull requests, not pushes"
expect_push 1 "direct push to main"        "refs/heads/main $head_sha refs/heads/main $head_sha"
expect_push 1 "direct push to staging"     "refs/heads/staging $head_sha refs/heads/staging $head_sha"
expect_push 1 "direct push to development" "refs/heads/development $head_sha refs/heads/development $head_sha"
expect_push 1 "deleting main"              "refs/heads/main $zero refs/heads/main $head_sha"

# The force-push and attribution cases need a history of a particular SHAPE, and
# they used to read it out of THIS repository. That made them depend on how the
# repository happened to be cloned, and it broke exactly where it matters:
# `actions/checkout` fetches depth 1, so `git rev-parse HEAD~1` has no parent
# object to resolve. Worse, rev-parse prints the unresolved string `HEAD~1` on
# STDOUT before failing, so `$(... || echo "$head_sha")` APPENDED rather than
# replaced, and the hook was handed two malformed refspec lines. It ignored them,
# correctly — and the test read that as the hook having failed to refuse.
#
# So the fixture is built rather than borrowed: a throwaway repository with a
# history of a known shape, and the hook run inside it. Nothing below depends on
# the depth of the checkout, the current branch, or whether a local-only backup
# ref happens to exist. That also means neither case can silently skip in CI,
# and a guard test that skipped is not a guard test that passed.
fixture=$(mktemp -d)
(
  cd "$fixture" || exit 1
  # Counters restart here: this is a subshell, so its increments cannot reach
  # the parent's. They are carried over through the tally file below, and
  # starting from the parent's values would count every earlier test twice.
  pass=0; fail=0

  git init -q .
  git config user.email t@example.com
  git config user.name "Test"
  git config commit.gpgsign false
  git commit -q --allow-empty -m "feat(domain): first   [1.1.1]"
  git commit -q --allow-empty -m "feat(domain): second   [1.1.2]"
  trunk=$(git rev-parse HEAD)
  first=$(git rev-parse HEAD~1)

  echo "pre-push — a force-push to a protected branch discards published commits"
  # Pushing the FIRST commit over a remote that already has the second: the
  # remote tip is not an ancestor of what is being pushed, which is what a
  # force-push is.
  expect_push 1 "force-push to main" "refs/heads/main $first refs/heads/main $trunk"

  # `git push --all` would republish a backup ref — a plain ref under
  # refs/heads/ carrying the original trailers — as a brand new remote branch
  # that no branch rule covers. --mirror is not the only vector.
  git checkout -q -b backup-before-rewrite
  git commit -q --allow-empty -m "feat(domain): third   [1.1.3]

Co-Authored-By: Some Agent <noreply@anthropic.com>"
  attributed=$(git rev-parse HEAD)

  echo "pre-push — attribution cannot reach origin on ANY ref"
  expect_push 1 "pushing a ref that carries an agent trailer" \
    "refs/heads/backup-before-rewrite $attributed refs/heads/backup-before-rewrite $zero"

  # The same attributed commit is already published on `upstream`. A clean
  # descendant must pass when upstream is Git's supplied destination: a hook
  # hardcoded to origin scans too much history and rejects it incorrectly.
  git update-ref refs/remotes/upstream/development "$attributed"
  git commit -q --allow-empty -m "chore(repo): clean descendant   [—]"
  clean_descendant=$(git rev-parse HEAD)
  echo "pre-push — commit enumeration uses Git's supplied remote"
  expect_push 0 "history already present on upstream is not re-scanned" \
    "refs/heads/topic $clean_descendant refs/heads/topic $zero" "" upstream

  # Dependabot is repository automation, not a coding assistant. Preserve its
  # exact existing author/trailer shape when the Git metadata strings match.
  # Those strings are locally configurable, so this is a compatibility case,
  # not independent authentication of the App identity.
  git update-ref refs/remotes/upstream/development "$clean_descendant"
  git commit -q --allow-empty \
    --author='dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>' \
    -m "chore(repo): bump the dependency group   [—]

Co-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>"
  matching_bot=$(git rev-parse HEAD)
  echo "pre-push — the Dependabot metadata exception is exact and narrow"
  expect_push 0 "the exact Dependabot author/trailer pair is retained" \
    "refs/heads/dependabot/group $matching_bot refs/heads/dependabot/group $zero" "" upstream

  git commit -q --allow-empty \
    --author='dependabot[bot] <spoof@example.com>' \
    -m "chore(repo): spoof the dependency bot   [—]

Co-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>"
  spoofed_bot=$(git rev-parse HEAD)
  expect_push 1 "a spoofed Dependabot author does not earn the exception" \
    "refs/heads/spoof $spoofed_bot refs/heads/spoof $zero" "" upstream

  printf '%s %s\n' "$pass" "$fail" > "$fixture/.tally"
)
if [ -r "$fixture/.tally" ]; then
  read -r sub_pass sub_fail < "$fixture/.tally"
  pass=$((pass + sub_pass)); fail=$((fail + sub_fail))
else
  bad "the pre-push fixture repository could not be built"
fi
rm -rf "$fixture"

echo "pre-push — feature branches and new tags remain available"
expect_push 0 "a feature branch"           "refs/heads/x $head_sha refs/heads/phase-1/group-3-tax $head_sha"
expect_push 1 "an environment variable cannot bypass a protected push" \
  "refs/heads/main $head_sha refs/heads/main $head_sha" "POS_ALLOW_PROTECTED_PUSH=1"
expect_push 0 "creating a new tag is allowed" \
  "refs/tags/v9.9.9 $head_sha refs/tags/v9.9.9 $zero"
expect_push 1 "moving an existing tag is refused" \
  "refs/tags/v9.9.9 $head_sha refs/tags/v9.9.9 1111111111111111111111111111111111111111"
expect_push 1 "deleting an existing tag is refused" \
  "refs/tags/v9.9.9 $zero refs/tags/v9.9.9 $head_sha"
expect_push 1 "an unreadable local commit fails closed" \
  "refs/heads/x 1111111111111111111111111111111111111111 refs/heads/x $zero"
expect_push 1 "a malformed ref update fails closed" \
  "refs/heads/x $head_sha refs/heads/x"

# expect_commit <expected-exit> <label> <path> [content]
expect_commit() {
  local want="$1" label="$2" path="$3" content="${4:-x}"
  local tmp got
  tmp=$(mktemp -d)
  (
    cd "$tmp" || exit 1
    git init -q .
    git config user.email t@t; git config user.name t
    mkdir -p "$(dirname "$path")" 2>/dev/null
    printf '%s' "$content" > "$path"
    git add -f "$path" 2>/dev/null
    "$HOOKS/pre-commit" >/dev/null 2>&1
  )
  got=$?
  rm -rf "$tmp"
  [ "$got" -eq "$want" ] && ok "$label" || bad "$label (wanted exit $want, got $got)"
}

echo "pre-commit — the never-committed list"
expect_commit 1 "an env file"              ".env"                          "KEY=x"
expect_commit 1 "a register database"      "pos.db"
expect_commit 1 "a WAL sidecar"            "pos.db-wal"
expect_commit 1 "a rollback-journal sidecar" "nested/data/pos.sqlite-journal"
expect_commit 1 "a private key"            "server.pem"
expect_commit 1 "a nested OpenSSH key"      "nested/home/id_rsa"
expect_commit 1 "a case-varied OpenSSH key" "nested/home/ID_RSA"
expect_commit 1 "local tool permissions"   ".claude/settings.local.json"   "{}"
expect_commit 1 "a generated Tauri schema" "apps/terminal/src-tauri/gen/schemas/acl.json" "{}"
expect_commit 1 "a build artefact"         "target/debug/thing"
expect_commit 1 "Python bytecode"           "scripts/__pycache__/guard.cpython-314.pyc"
# Registry credentials. Each is a file a tool writes for you, in the repository
# root, without asking — and a token in history is a rotation, not a revert.
expect_commit 1 "an npm auth token"        ".npmrc"                   "//registry.npmjs.org/:_authToken=x"
expect_commit 1 "a netrc"                  ".netrc"                   "machine github.com login x"
expect_commit 1 "a nested netrc"           "nested/home/.netrc"       "machine github.com login x"
expect_commit 1 "stored git credentials"   ".git-credentials"         "https://u:p@github.com"
expect_commit 1 "a crates.io token"        ".cargo/credentials.toml"  "[registry]"
expect_commit 1 "a docker registry auth"   ".docker/config.json"      "{}"
expect_commit 0 "an npmrc EXAMPLE"         ".npmrc.example"           "registry=https://registry.npmjs.org/"
expect_commit 0 "the exact env template" "apps/server/.env.example" \
  "DATABASE_URL=postgres://localhost/pos"
expect_commit 1 "a root env example"       ".env.example"             "KEY=x"
expect_commit 1 "a misplaced env example"  "nested/.env.example"      "KEY=x"
expect_commit 1 "another app env example"  "apps/terminal/.env.example" \
  "KEY=x"
expect_commit 1 "an env sample"            "apps/server/.env.sample"  "KEY=x"
expect_commit 1 "an env template alias"    "apps/server/.env.template" \
  "KEY=x"
expect_commit 1 "an env example backup"    "apps/server/.env.example.backup" \
  "KEY=x"
expect_commit 1 "a case-varied env example" "APPS/SERVER/.ENV.EXAMPLE" \
  "KEY=x"
expect_commit 0 "ordinary source"          "crates/pos-domain/src/tax.rs"  "fn main() {}"
expect_commit 1 "a newline-bearing path is parsed safely" $'nested/odd\nfolder/.env' "KEY=x"
expect_commit 0 "a tab-bearing source path is parsed safely" $'src/odd\tfile.rs' "fn main() {}"
expect_commit 1 "source-plan material is read-only" "docs/plan/new-source.md" "immutable source"
expect_commit 1 "case-varied source-plan material is still read-only" "DOCS/PLAN/new-source.md" "immutable source"

# expect_index_state <expected-exit> <label> <setup/action script>
#
# These cases deliberately make the index and working tree disagree. A hook
# that reads the worktree is checking something other than the commit Git is
# about to create.
expect_index_state() {
  local want="$1" label="$2" action="$3" tmp got
  tmp=$(mktemp -d)
  (
    cd "$tmp" || exit 1
    git init -q .
    git config user.email t@t; git config user.name t
    eval "$action" >/dev/null 2>&1
    "$HOOKS/pre-commit" >/dev/null 2>&1
  )
  got=$?
  rm -rf "$tmp"
  [ "$got" -eq "$want" ] && ok "$label" || bad "$label (wanted exit $want, got $got)"
}

echo "pre-commit — the staged index is authoritative"
expect_index_state 1 "a staged large blob cannot be hidden by truncating the worktree" \
  'dd if=/dev/zero of=large.bin bs=1000 count=2001 2>/dev/null; git add large.bin; : > large.bin'
expect_index_state 0 "an unstaged large worktree file does not poison a small staged blob" \
  'printf small > source.bin; git add source.bin; dd if=/dev/zero of=source.bin bs=1000 count=2001 2>/dev/null'
expect_index_state 1 "a staged token cannot be hidden by cleaning the worktree" \
  'token="AK""IAA1B2C3D4E5F6G7H8"; printf "api_key = %s\n" "$token" > config.txt; git add config.txt; printf placeholder > config.txt'
expect_index_state 1 "an inline gitleaks allow comment cannot approve its own secret" \
  'token="AK""IAA1B2C3D4E5F6G7H8"; printf "api_key = %s # gitleaks:allow\n" "$token" > config.txt; git add config.txt'

expect_precommit_failure() {
  local want="$1" label="$2" mode="$3" tmp bin got
  tmp=$(mktemp -d)
  (
    cd "$tmp" || exit 1
    git init -q .
    git config user.email t@t; git config user.name t
    printf ordinary > source.txt
    git add source.txt
    case "$mode" in
      broken-index)
        mkdir invalid-index
        GIT_INDEX_FILE="$tmp/invalid-index" "$HOOKS/pre-commit" >/dev/null 2>&1
        ;;
      missing-scanner)
        bin="$tmp/bin"; mkdir "$bin"
        ln -s "$(command -v bash)" "$bin/bash"
        ln -s "$(command -v dirname)" "$bin/dirname"
        ln -s "$(command -v git)" "$bin/git"
        python_executable=$("$PYTHON_RUNNER" -c 'import sys; print(sys.executable)')
        ln -s "$python_executable" "$bin/python3"
        PATH="$bin" "$HOOKS/pre-commit" >/dev/null 2>&1
        ;;
    esac
  )
  got=$?
  rm -rf "$tmp"
  [ "$got" -eq "$want" ] && ok "$label" || bad "$label (wanted exit $want, got $got)"
}

echo "pre-commit — operational failures refuse closed"
expect_precommit_failure 2 "an unreadable index is an error, never a pass" broken-index
expect_precommit_failure 2 "a missing Gitleaks installation is an error, never a skip" missing-scanner

# expect_migration <expected-exit> <label> <action run on the fixture tree>
#
# A separate fixture because "committed" is the entire question: the hook must
# refuse a change to a migration that is already in HEAD, and stay silent while
# the next one is added beside it. Both migration directories are seeded, so the
# server mirror is covered by the same cases as the register.
expect_migration() {
  local want="$1" label="$2" action="$3" got tmp
  tmp=$(mktemp -d)
  (
    cd "$tmp" || exit 1
    git init -q .
    git config user.email t@t; git config user.name t
    mkdir -p crates/pos-db/migrations apps/server/migrations
    printf 'CREATE TABLE a (id BLOB);\n' > crates/pos-db/migrations/0001_init.sql
    printf 'CREATE TABLE a (id UUID);\n' > apps/server/migrations/20260819200319_init.sql
    git add -A >/dev/null 2>&1
    git commit -q --no-verify -m seed >/dev/null 2>&1
    eval "$action" >/dev/null 2>&1
    "$HOOKS/pre-commit" >/dev/null 2>&1
  )
  got=$?
  rm -rf "$tmp"
  [ "$got" -eq "$want" ] && ok "$label" || bad "$label (wanted exit $want, got $got)"
}

echo "pre-commit — a committed migration is forward-only (conventions §9)"
expect_migration 1 "editing a committed migration" \
  'printf "ALTER TABLE a ADD b TEXT;\n" >> crates/pos-db/migrations/0001_init.sql; git add -A'
# The deletion cases are why this block exists. `--diff-filter=ACMR` never
# reported a removal, so `git rm` on a committed migration passed this hook and
# the Claude write guard both, and the file left the tree unremarked.
expect_migration 1 "git rm on a committed migration" \
  'git rm -q crates/pos-db/migrations/0001_init.sql'
expect_migration 1 "rm then stage the deletion" \
  'rm crates/pos-db/migrations/0001_init.sql; git add -A'
expect_migration 1 "renaming a committed migration" \
  'git mv crates/pos-db/migrations/0001_init.sql crates/pos-db/migrations/0001_initial.sql'
expect_migration 1 "deleting the Postgres mirror" \
  'git rm -q apps/server/migrations/20260819200319_init.sql'
expect_migration 1 "adding a case-varied migration path" \
  'mkdir -p CRATES/POS-DB/MIGRATIONS; printf "SELECT 1;\n" > CRATES/POS-DB/MIGRATIONS/0002_NEXT.SQL; git add -A'
expect_migration 1 "adding a migration as a symlink" \
  'target=$(printf "../../../migration-target.sql" | git hash-object -w --stdin); \
   git update-index --add --cacheinfo 120000 "$target" crates/pos-db/migrations/0002_next.sql'
expect_migration 0 "adding the NEXT migration beside it" \
  'printf "CREATE TABLE b (id BLOB);\n" > crates/pos-db/migrations/0002_next.sql; git add -A'
expect_migration 0 "deleting an UNCOMMITTED migration" \
  'printf "x\n" > crates/pos-db/migrations/0002_next.sql; git add -A; rm crates/pos-db/migrations/0002_next.sql; git add -A'

echo
if [ "$fail" -ne 0 ]; then
  echo "git hooks: $pass passed, $fail FAILED"
  exit 1
fi
echo "git hooks: $pass passed — every guard still refuses what it must"
