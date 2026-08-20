#!/usr/bin/env bash
# A guard nobody has seen fail is a guard nobody should trust.
# Negative-tests every git hook in this directory. Run after touching one:
#   just guards
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
HOOKS="$PWD/.githooks"
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
expect_msg 0 "a merge commit passes through" 'Merge branch development into staging'
expect_msg 0 "a revert passes through"     'Revert "feat(domain): tax engine   [1.3.4]"'
expect_msg 0 "a fixup passes through"      'fixup! feat(domain): tax engine'

echo "commit-msg — refuses what it must"
expect_msg 1 "no type/scope prefix"        'added the tax engine   [1.3.4]'
expect_msg 1 "a type outside the list"     'build(domain): tax engine   [1.3.4]'
expect_msg 1 "a scope outside the list"    'feat(claude): agent rules   [—]'
expect_msg 1 "a missing step tag"          'feat(domain): tax engine'
expect_msg 1 "an empty step tag"           'feat(domain): tax engine   []'
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
# GitHub's own privacy address for real people. Refusing `noreply@` generically
# would block them, which is why the hook enumerates instead.
expect_body 0 "a human using GitHub's private address" \
  'feat(domain): tax engine   [1.3.4]

Co-Authored-By: Jane Smith <jane@users.noreply.github.com>'

# expect_push <expected-exit> <label> <stdin line> [env]
expect_push() {
  local want="$1" label="$2" line="$3" envset="${4:-}"
  local got
  if [ -n "$envset" ]; then
    got=$(env "$envset" bash -c "printf '%s\n' '$line' | '$HOOKS/pre-push' origin git@x >/dev/null 2>&1; echo \$?")
  else
    got=$(printf '%s\n' "$line" | "$HOOKS/pre-push" origin git@x >/dev/null 2>&1; echo $?)
  fi
  [ "$got" -eq "$want" ] && ok "$label" || bad "$label (wanted exit $want, got $got)"
}

head_sha=$(git rev-parse HEAD)
prev_sha=$(git rev-parse HEAD~1 2>/dev/null || echo "$head_sha")
zero=0000000000000000000000000000000000000000

echo "pre-push — the protected branches take pull requests, not pushes"
expect_push 1 "direct push to main"        "refs/heads/main $head_sha refs/heads/main $head_sha"
expect_push 1 "direct push to staging"     "refs/heads/staging $head_sha refs/heads/staging $head_sha"
expect_push 1 "direct push to development" "refs/heads/development $head_sha refs/heads/development $head_sha"
expect_push 1 "deleting main"              "refs/heads/main $zero refs/heads/main $head_sha"
if [ "$prev_sha" != "$head_sha" ]; then
  expect_push 1 "force-push to main"       "refs/heads/main $prev_sha refs/heads/main $head_sha"
fi
# `git push --all` would republish backup-before-rewrite — a plain ref under
# refs/heads/ carrying all 7 original trailers — as a new remote branch that no
# branch rule covers. Tested against the real ref, not a fixture.
if git rev-parse --verify -q backup-before-rewrite >/dev/null; then
  echo "pre-push — attribution cannot reach origin on ANY ref"
  backup_sha=$(git rev-parse backup-before-rewrite)
  expect_push 1 "pushing the pre-rewrite backup branch" \
    "refs/heads/backup-before-rewrite $backup_sha refs/heads/backup-before-rewrite $zero"
fi

echo "pre-push — everything else is none of its business"
expect_push 0 "a feature branch"           "refs/heads/x $head_sha refs/heads/phase-1/group-3-tax $head_sha"
expect_push 0 "a deliberate override"      "refs/heads/main $head_sha refs/heads/main $head_sha" \
                                           "POS_ALLOW_PROTECTED_PUSH=1"

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
expect_commit 1 "a private key"            "server.pem"
expect_commit 1 "local tool permissions"   ".claude/settings.local.json"   "{}"
expect_commit 1 "a generated Tauri schema" "apps/terminal/src-tauri/gen/schemas/acl.json" "{}"
expect_commit 1 "a build artefact"         "target/debug/thing"
expect_commit 0 "ordinary source"          "crates/pos-domain/src/tax.rs"  "fn main() {}"

echo
if [ "$fail" -ne 0 ]; then
  echo "git hooks: $pass passed, $fail FAILED"
  exit 1
fi
echo "git hooks: $pass passed — every guard still refuses what it must"
