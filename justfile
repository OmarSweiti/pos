set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]
python := if os_family() == "windows" { "py -3" } else { "python3" }

# List recipes
default:
    just --list

# One-time / after pulling
setup:
    just hooks
    just identity
    just setup-tools-check
    just gitleaks-check
    just policy-tools-check
    pnpm install --frozen-lockfile
    cargo fetch --locked

# Fail before downloading anything, after the fail-closed hooks are installed.
# These are the tools used by those hooks and by the advertised local test gate.
[unix]
setup-tools-check:
    @command -v python3 >/dev/null 2>&1 || { echo "Python 3.11+ is required: https://www.python.org/downloads/" >&2; exit 1; }
    @python3 -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else "Python 3.11+ is required")'
    @command -v node >/dev/null 2>&1 || { echo "Node.js 22 is required: https://nodejs.org/" >&2; exit 1; }
    @node -e 'const major=Number(process.versions.node.split(".")[0]); if (major !== 22) { console.error(`Node.js 22 is required; found ${process.versions.node}`); process.exit(1); }'
    @command -v cargo >/dev/null 2>&1 || { echo "Rust/cargo is required: https://rustup.rs" >&2; exit 1; }
    @cargo nextest --version >/dev/null 2>&1 || { echo "cargo-nextest is required: https://nexte.st/docs/installation/pre-built-binaries/" >&2; exit 1; }
    @command -v pnpm >/dev/null 2>&1 || { echo "pnpm is required: https://pnpm.io/installation" >&2; exit 1; }

[windows]
setup-tools-check:
    if (-not (Get-Command bash -ErrorAction SilentlyContinue)) { Write-Error "Git Bash is required for the committed shell hooks: https://gitforwindows.org/"; exit 1 }
    if (-not (Get-Command py -ErrorAction SilentlyContinue)) { Write-Error "Python 3.11+ with the standard py launcher is required: https://www.python.org/downloads/"; exit 1 }
    py -3 -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else 'Python 3.11+ is required')"
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Write-Error "Node.js 22 is required: https://nodejs.org/"; exit 1 }
    node -e "const major=Number(process.versions.node.split('.')[0]); if (major !== 22) { console.error('Node.js 22 is required; found ' + process.versions.node); process.exit(1); }"
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Write-Error "Rust/cargo is required: https://rustup.rs"; exit 1 }
    cargo nextest --version | Out-Null
    if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) { Write-Error "pnpm is required: https://pnpm.io/installation"; exit 1 }

# Branch protection is not available on a private repo on the GitHub Free plan.
# These hooks are the first local safety net; a machine that has not run this can
# push straight to main, and `--no-verify` can bypass them.
# Point git at the committed hooks (commit-msg, pre-commit, pre-push)
hooks:
    git config core.hooksPath .githooks
    @echo "core.hooksPath = .githooks  (commit-msg, pre-commit, pre-push)"

# This is a PERSONAL project, and it is authored under a personal address — not
# whatever a work laptop happens to carry in its global git config.
#
# `git config --local` writes to .git/config, which is not tracked, so a fresh
# clone silently inherits the global identity instead. Same failure mode as the
# hooks above, same fix: `just setup` sets it per repository, every clone.
identity:
    git config --local user.email omarswaty4@gmail.com
    @echo "user.email = omarswaty4@gmail.com  (this repository only)"

# Content scanning is fail-closed in pre-commit. Prove the required scanner is
# present during setup, when the installation error is actionable.
[unix]
gitleaks-check:
    @command -v gitleaks >/dev/null 2>&1 || { echo "gitleaks is required: https://github.com/gitleaks/gitleaks#installing" >&2; exit 1; }
    @gitleaks git --help >/dev/null

[windows]
gitleaks-check:
    if (-not (Get-Command gitleaks -ErrorAction SilentlyContinue)) { Write-Error "gitleaks is required: https://github.com/gitleaks/gitleaks#installing"; exit 1 }
    gitleaks git --help | Out-Null

# Workflow policy is parsed semantically rather than with regex. Ruby's bundled
# Psych parser is therefore a fail-closed local prerequisite, checked before any
# dependency download just like Gitleaks.
[unix]
policy-tools-check:
    @command -v ruby >/dev/null 2>&1 || { echo "ruby with the bundled Psych YAML parser is required" >&2; exit 1; }
    @ruby -rpsych -e 'abort "Psych YAML parser is unavailable" unless defined?(Psych)'

[windows]
policy-tools-check:
    if (-not (Get-Command ruby -ErrorAction SilentlyContinue)) { Write-Error "ruby with the bundled Psych YAML parser is required"; exit 1 }
    ruby -rpsych -e "abort 'Psych YAML parser is unavailable' unless defined?(Psych)"

# ── inner loop ────────────────────────────────────────────
# Compile + borrow-check everything, tests and benches included.
# The fastest signal that says "this would build" — seconds, not minutes.
check:
    cargo check --locked --workspace --all-targets

# ── dev loops ─────────────────────────────────────────────
dev-terminal:
    pnpm --filter terminal tauri dev

dev-backoffice:
    pnpm --filter backoffice dev

dev-server:
    cd apps/server; cargo run -p pos-server

db-up:
    docker compose -f infra/docker-compose.yml up -d

db-down:
    docker compose -f infra/docker-compose.yml down

migrate:
    cd apps/server; sqlx migrate run

# Development only — see docs/implementation/02-development-workflow.md §0.
# Drop the dev Postgres *and its volume*, bring it back empty, re-migrate.
db-reset:
    docker compose -f infra/docker-compose.yml down -v
    docker compose -f infra/docker-compose.yml up -d --wait
    cd apps/server; sqlx migrate run

# Named for the exact bundle identifier so it cannot reach anything else.
# Wipe THIS machine's register database — rebuilt, empty, on next launch.
[unix]
db-local-reset:
    rm -rf "$HOME/Library/Application Support/com.perfectcoders.pos"
    rm -rf "$HOME/.local/share/com.perfectcoders.pos"

[windows]
db-local-reset:
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue "$env:APPDATA\com.perfectcoders.pos"

# Documentation cross-references must resolve (CI runs this too)
docs-links:
    bash ./scripts/check-doc-links.sh

# Conventions §10: RTL is the default, so every layout uses CSS logical
# properties. biome's recommended preset does not know Tailwind or CSS sides.
logical-css:
    bash ./scripts/check-logical-css.sh

# Property tests are prop_<invariant>; microstep 1.1.5 uses that filter, so a
# dropped prefix can omit one property while the remaining filtered tests pass.
prop-names:
    {{ python }} ./scripts/check-prop-test-names.py

# Both clippy invocations pass no lint flags of their own, so the workspace lint
# table is the entire lint scope of every gate — and it is inert in any member
# that does not opt in with `[lints] workspace = true`.
workspace-lints:
    {{ python }} ./scripts/check-workspace-lints.py

# pos-domain's module graph must stay acyclic (ref/domain-api.md §15)
acyclic:
    {{ python }} ./scripts/check-domain-acyclic.py

# Time and IDs are arguments to pos-domain. Keep UUID generation/RNG features
# out of its normal dependency graph and direct clock/random calls out of source.
domain-purity:
    {{ python }} ./scripts/check-domain-purity.py

# ref/schema.md must be executable SQLite and obey conventions §2, and the
# SHIPPED migrations are audited on their own in the same run.
verify-schema:
    {{ python }} ./scripts/verify-schema.py

# The Postgres mirror: every file declares which SQLite migration it mirrors, and
# every migration applies to a real server. Uses $DATABASE_URL if set, else a
# throwaway Docker container, else audits the mapping and says it skipped.
verify-pg:
    {{ python }} ./scripts/verify-pg-migrations.py

# Advisories, licences and banned/duplicate crates. Same gate CI runs.
#   cargo install cargo-deny --locked
#
# Deliberately NOT part of `pre-push`: both halves reach the network and depend on
# the state of the advisory databases, so this gate can fail on a push that
# changed nothing. CI's `supply-chain` job runs it on every PR, which is where a
# time-varying check belongs. Run it by hand before a release.
audit:
    cargo deny check
    pnpm audit --audit-level high

# Scan the complete reachable Git history, not just the current worktree.
secrets:
    bash ./scripts/scan-secrets.sh --history

# ── deterministic local quality gates (mirrored by CI) ───────────────────
test:
    cargo nextest run --locked --workspace
    pnpm -r --if-present test

lint:
    cargo fmt --all --check
    cargo clippy --locked --workspace --all-targets -- -D warnings
    {{ python }} ./scripts/check-workspace-lints.py
    {{ python }} ./scripts/check-domain-acyclic.py
    {{ python }} ./scripts/check-domain-purity.py
    {{ python }} ./scripts/verify-schema.py
    {{ python }} ./scripts/verify-pg-migrations.py --mapping-only
    bash ./scripts/check-logical-css.sh
    {{ python }} ./scripts/check-prop-test-names.py
    pnpm biome ci --error-on-warnings .
    bash ./scripts/check-doc-links.sh

fmt:
    cargo fmt --all
    pnpm biome format --write .

# The write guards are not advisory. Prove they still refuse (CLAUDE.md).
guards:
    {{ python }} ./.claude/hooks/test-settings.py
    bash ./.claude/hooks/test-protect-immutable.sh
    bash ./.claude/hooks/test-docs-links.sh
    bash ./.codex/hooks/test-hooks.sh
    {{ python }} ./.codex/test-policy.py
    {{ python }} ./.agents/test-skills.py
    bash ./.githooks/test-hooks.sh
    bash ./scripts/check-protected-paths.sh --self-test
    {{ python }} ./scripts/verify-schema.py --self-test
    {{ python }} ./scripts/verify-pg-migrations.py --self-test
    bash ./scripts/check-logical-css.sh --self-test
    {{ python }} ./scripts/check-prop-test-names.py --self-test
    {{ python }} ./scripts/check-domain-purity.py --self-test
    {{ python }} ./scripts/check-workspace-lints.py --self-test
    {{ python }} ./scripts/check-justfile-policy.py
    bash ./scripts/watch-pr-checks.sh --self-test
    bash ./scripts/validate-branch-flow.sh --self-test
    bash ./scripts/validate-change-title.sh --self-test
    {{ python }} ./scripts/check-automation-attribution.py --self-test
    bash ./scripts/scan-secrets.sh --self-test
    ruby ./scripts/check-branch-workflow-policy.rb --candidate-root .
    ruby ./scripts/check-branch-workflow-policy.rb --self-test
    bash ./scripts/gh-actions-policy.sh --check
    bash ./scripts/gh-actions-policy.sh --self-test
    bash ./scripts/test-gh-setup.sh
    bash ./scripts/pr-type-label.sh --self-test

# `lint` is biome (style) and `test` is vitest, so a TypeScript type error passes
# both and fails CI's `web` job instead. Mirrors that job's build step.
# The only place `tsc` runs. Part of `pre-push` for exactly that reason.
build-web:
    pnpm -r --if-present build

# Deterministic local equivalents of the core CI gates. Remote topology,
# cross-platform packaging, and time-varying advisory checks still run in CI.
pre-push: lint test build-web guards secrets

# ── GitHub ────────────────────────────────────────────────────────────────
# docs/implementation/03-github-workflow.md is the reference for all of these.

# Labels, milestones, merge behaviour, default branch. Idempotent.
gh-bootstrap:
    bash ./scripts/gh-bootstrap.sh

gh-bootstrap-dry:
    bash ./scripts/gh-bootstrap.sh --dry-run

# Audit the committed workflow action references without touching GitHub.
gh-actions-policy-check:
    bash ./scripts/gh-actions-policy.sh --check

# Preflight the live SHA-only Actions setting without changing it.
gh-actions-policy-dry:
    bash ./scripts/gh-actions-policy.sh --dry-run

# Post-merge activation: enables and verifies GitHub's SHA-only Actions policy.
gh-actions-policy:
    bash ./scripts/gh-actions-policy.sh

# Branch protection. Refuses politely on the Free plan — see the script's header.
gh-protect:
    bash ./scripts/gh-protect.sh

# The delivery project and its fields. Needs: gh auth refresh -s project,read:project
gh-project:
    bash ./scripts/gh-project.sh

# Start a group. `just branch phase-1/group-3-tax`. The `$` exports the recipe
# parameter as data; it is never interpolated into shell source.
branch $name:
    #!/usr/bin/env bash
    set -euo pipefail
    bash ./scripts/validate-branch-flow.sh "$name" development local local
    git switch development
    git pull --ff-only
    git switch -c "$name"

# Open the PR for the branch you are on. Always into development.
# Gates, push, open the PR into development, watch CI.
#
# `--fill-first` takes the FIRST commit's message as the PR title, and a squash
# merge commits PR_TITLE (gh-bootstrap.sh sets squash_merge_commit_title). So on a
# branch carrying several microsteps, filling from the first commit lands a commit
# on `development` that describes one microstep and stands for all of them. Pass
# the title yourself there.
#
# The title is checked by .githooks/commit-msg — the same hook that checks a commit
# subject — because that is exactly what it becomes. Better to be told before the
# push than by the branch-flow check afterwards.
#
#   just pr                                          # one commit: fill from it
#   just pr 'feat(domain): tax engine   [1.3.4]'     # several: say it once
#   just pr 'chore(repo): harden the guards   [—]' notes/pr-body.md
#   just pr '…' '' 'Phase 2 — money-grade'           # override the milestone
#
# With a title and no body file, the body is the list of microstep subjects, which
# is where 03-github-workflow.md §5 says they belong.
#
# THE MILESTONE is derived from the branch name, because nothing else was setting
# it and six phase-gate milestones sat permanently at 0 issues while eleven PRs
# shipped. `phase-<0-5>/...` earns the milestone whose title starts `Phase <n> `,
# looked up from GitHub so the titles live only in gh-bootstrap.sh and cannot
# drift into this file. A branch with no phase in its name earns none, and that is
# correct rather than a gap: §5 says milestones are the six phase gates and nothing
# else, and a `chore/` PR is not something a gate waits on. Pass the third
# argument for the exception — a `fix/` that does block a gate.
#
# Exported recipe parameters cross into this script as environment values. They
# are never substituted into its source, so quotes, `$()`, and backticks remain
# inert data and are still passed to every command as quoted arguments.
pr $title='' $body='' $milestone='':
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -n "$title" ]; then
      msg=$(mktemp)
      printf '%s\n' "$title" > "$msg"
      if ! .githooks/commit-msg "$msg"; then
        rm -f "$msg"
        echo "pr: REFUSED — that title is not a legal squash commit subject (conventions §8)."
        exit 1
      fi
      rm -f "$msg"
    fi

    # Derive the milestone before the push, so a lookup failure is reported while
    # nothing has happened yet.
    resolve_milestone() {
      local mode="$1" needle="$2" pages resolved
      if ! pages=$(gh api --paginate --slurp \
          "repos/{owner}/{repo}/milestones?state=all&per_page=100"); then
        echo "pr: milestone list failed — check GitHub authentication and network access." >&2
        return 1
      fi
      if ! resolved=$(printf '%s' "$pages" | ./scripts/run-python.sh -c '
    import json
    import sys

    mode, needle = sys.argv[1:]
    pages = json.load(sys.stdin)
    if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
        raise SystemExit("GitHub returned an unexpected milestone-list shape")
    titles = [
        item.get("title")
        for page in pages
        for item in page
        if isinstance(item, dict) and isinstance(item.get("title"), str)
    ]
    matches = [
        title for title in titles
        if (mode == "exact" and title == needle)
        or (mode == "prefix" and title.startswith(needle))
    ]
    if len(matches) != 1:
        raise SystemExit(
            f"expected exactly one milestone matching {needle!r}; found {len(matches)}"
        )
    print(matches[0])
    ' "$mode" "$needle"); then
        echo "pr: milestone lookup was ambiguous or incomplete; no push or PR was attempted." >&2
        return 1
      fi
      printf '%s\n' "$resolved"
    }

    branch=$(git branch --show-current)
    if [ -z "$milestone" ]; then
      case "$branch" in
        phase-[0-5]/*)
          phase=${branch#phase-}; phase=${phase%%/*}
          milestone=$(resolve_milestone prefix "Phase $phase ") || {
            echo "pr: phase $phase requires one milestone; run just gh-bootstrap, then retry." >&2
            exit 1
          } ;;
        *)
          echo "pr: $branch names no phase, so no milestone (03-github-workflow.md §5)." ;;
      esac
    else
      requested_milestone=$milestone
      milestone=$(resolve_milestone exact "$requested_milestone") || {
        echo "pr: requested milestone '$requested_milestone' does not resolve uniquely." >&2
        exit 1
      }
    fi
    [ -n "$milestone" ] && echo "pr: milestone -> $milestone"

    just pre-push
    git fetch --quiet origin development
    git push -u origin HEAD

    ms=()
    [ -n "$milestone" ] && ms=(--milestone "$milestone")

    if [ -n "$body" ]; then
      gh pr create --base development --title "$title" --body-file "$body" "${ms[@]+"${ms[@]}"}"
    elif [ -n "$title" ]; then
      base=$(git merge-base origin/development HEAD)
      gh pr create --base development --title "$title" \
        --body "$(git log --reverse --format='- %s' "$base"..HEAD)" "${ms[@]+"${ms[@]}"}"
    else
      gh pr create --base development --fill-first "${ms[@]+"${ms[@]}"}"
    fi

    # Independent workflows register at different times. Derive and wait for
    # the exact workflow-qualified set for this PR route and changed paths.
    created_pr=$(gh pr view --json url --jq .url)
    bash ./scripts/watch-pr-checks.sh "$created_pr"

# development → staging, as a release candidate. Merge with a MERGE COMMIT.
promote-staging:
    gh pr create --base staging --head development \
      --title "promote development to staging" \
      --body-file .github/PULL_REQUEST_TEMPLATE/promotion.md

# staging → main, for production. Merge with a MERGE COMMIT.
promote-main:
    gh pr create --base main --head staging \
      --title "promote staging to main" \
      --body-file .github/PULL_REQUEST_TEMPLATE/promotion.md

# What is between the branches right now — read this before promoting.
flow:
    @echo "── on development, not yet in staging ──"
    @git log --oneline origin/staging..origin/development || true
    @echo "── on staging, not yet in main ──"
    @git log --oneline origin/main..origin/staging || true
