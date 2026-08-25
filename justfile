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
    @command -v node >/dev/null 2>&1 || { echo "Node.js is required: https://nodejs.org/" >&2; exit 1; }
    {{ python }} ./scripts/check-node-version.py
    @command -v cargo >/dev/null 2>&1 || { echo "Rust/cargo is required: https://rustup.rs" >&2; exit 1; }
    @cargo nextest --version >/dev/null 2>&1 || { echo "cargo-nextest is required: https://nexte.st/docs/installation/pre-built-binaries/" >&2; exit 1; }
    @command -v pnpm >/dev/null 2>&1 || { echo "pnpm is required: https://pnpm.io/installation" >&2; exit 1; }
    @command -v ruff >/dev/null 2>&1 || { echo "Ruff is required: https://docs.astral.sh/ruff/installation/" >&2; exit 1; }
    @command -v shellcheck >/dev/null 2>&1 || { echo "ShellCheck is required: https://github.com/koalaman/shellcheck#installing" >&2; exit 1; }

[windows]
setup-tools-check:
    if (-not (Get-Command bash -ErrorAction SilentlyContinue)) { Write-Error "Git Bash is required for the committed shell hooks: https://gitforwindows.org/"; exit 1 }
    if (-not (Get-Command py -ErrorAction SilentlyContinue)) { Write-Error "Python 3.11+ with the standard py launcher is required: https://www.python.org/downloads/"; exit 1 }
    py -3 -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else 'Python 3.11+ is required')"
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Write-Error "Node.js is required: https://nodejs.org/"; exit 1 }
    {{ python }} ./scripts/check-node-version.py
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Write-Error "Rust/cargo is required: https://rustup.rs"; exit 1 }
    cargo nextest --version | Out-Null
    if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) { Write-Error "pnpm is required: https://pnpm.io/installation"; exit 1 }
    if (-not (Get-Command ruff -ErrorAction SilentlyContinue)) { Write-Error "Ruff is required: https://docs.astral.sh/ruff/installation/"; exit 1 }
    if (-not (Get-Command shellcheck -ErrorAction SilentlyContinue)) { Write-Error "ShellCheck is required: https://github.com/koalaman/shellcheck#installing"; exit 1 }

# Node's pin lives in .nvmrc and nowhere else. CI reads that same file through
# `node-version-file:`, so a runner and a developer's machine cannot disagree
# about which Node built the bundle — which is what rust-toolchain.toml already
# does for Rust, and what nine separate restatements of "22" did not.
#
# In `lint`, `test` and `build-web` rather than `setup` alone: the advertised
# local gate ran Biome, Vitest and tsc without ever checking which Node it was
# handing them to.
node-version-check:
    {{ python }} ./scripts/check-node-version.py

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

# --wait, because the documented next action is `just migrate`: without it the
# recipe returns while Postgres is still starting and the migration races it.
db-up:
    docker compose -f infra/docker-compose.yml up -d --wait --wait-timeout 120

db-down:
    docker compose -f infra/docker-compose.yml down

migrate:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v sqlx >/dev/null 2>&1 || {
      echo "migrate: sqlx-cli is required — cargo install sqlx-cli --no-default-features --features rustls,postgres" >&2
      exit 1
    }
    cd apps/server && sqlx migrate run

# Development only — see docs/implementation/02-development-workflow.md §0.
# Drop the dev Postgres *and its volume*, bring it back empty, re-migrate.
db-reset:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v sqlx >/dev/null 2>&1 || {
      echo "db-reset: sqlx-cli is required — cargo install sqlx-cli --no-default-features --features rustls,postgres" >&2
      exit 1
    }
    # Bound to the disposable Compose database by name, not by whatever the
    # environment happens to hold. sqlx reads apps/server/.env through dotenvy,
    # which does NOT override an already-exported variable — so a shell pointed
    # at staging would otherwise have this recipe migrate staging, one line after
    # `down -v`. The value is the database infra/docker-compose.yml defines.
    url="postgres://postgres:postgres@localhost:5432/pos"
    docker compose -f infra/docker-compose.yml down -v
    docker compose -f infra/docker-compose.yml up -d --wait --wait-timeout 120
    cd apps/server && DATABASE_URL="$url" sqlx migrate run

# Named for the exact bundle identifier so it cannot reach anything else.
# Wipe THIS machine's register database — rebuilt, empty, on next launch.
[unix]
db-local-reset:
    rm -rf "$HOME/Library/Application Support/com.perfectcoders.pos"
    rm -rf "$HOME/.local/share/com.perfectcoders.pos"

[windows]
db-local-reset:
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue "$env:APPDATA\com.perfectcoders.pos"

# Ruff, ShellCheck and ruby -c over the policy code. About eleven thousand lines
# of it decide whether a migration may be edited or a secret may be committed,
# and until this recipe existed nothing linted any of it. Missing linters are a
# setup error, never a green gate; CI installs both at reviewed pinned versions.
lint-scripts:
    bash ./scripts/lint-scripts.sh

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

# The Phase-5 coverage matrix is executable evidence, not a typed total: every
# named case must reconcile with its runner, owner microstep and phase gate.
test-catalog:
    {{ python }} ./scripts/check-test-catalog.py

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
audit: node-version-check
    cargo deny check
    {{ python }} ./scripts/check-js-licenses.py
    pnpm audit --audit-level high

# Scan the complete reachable Git history, not just the current worktree.
secrets:
    bash ./scripts/scan-secrets.sh --history

# ── deterministic local quality gates (mirrored by CI) ───────────────────
test: node-version-check
    cargo nextest run --locked --workspace
    pnpm -r --if-present test

lint: node-version-check
    cargo fmt --all --check
    cargo clippy --locked --workspace --all-targets -- -D warnings
    {{ python }} ./scripts/check-workspace-lints.py
    {{ python }} ./scripts/check-domain-acyclic.py
    {{ python }} ./scripts/check-domain-purity.py
    {{ python }} ./scripts/verify-schema.py
    {{ python }} ./scripts/verify-pg-migrations.py --mapping-only
    bash ./scripts/check-logical-css.sh
    {{ python }} ./scripts/check-prop-test-names.py
    {{ python }} ./scripts/check-test-catalog.py
    pnpm biome ci --error-on-warnings .
    bash ./scripts/check-doc-links.sh
    bash ./scripts/lint-scripts.sh

fmt:
    cargo fmt --all
    pnpm biome format --write .

# The write guards are not advisory. Prove they still refuse (CLAUDE.md).
guards:
    {{ python }} ./.claude/hooks/test-settings.py
    bash ./.claude/hooks/test-protect-immutable.sh
    bash ./.claude/hooks/test-docs-links.sh
    bash ./scripts/check-doc-links.sh --self-test
    bash ./scripts/lint-scripts.sh --self-test
    bash ./.codex/hooks/test-hooks.sh
    {{ python }} ./.codex/test-policy.py
    {{ python }} ./.agents/test-skills.py
    bash ./.githooks/test-hooks.sh
    bash ./scripts/check-protected-paths.sh --self-test
    {{ python }} ./scripts/verify-schema.py --self-test
    {{ python }} ./scripts/verify-pg-migrations.py --self-test
    bash ./scripts/check-logical-css.sh --self-test
    {{ python }} ./scripts/check-prop-test-names.py --self-test
    {{ python }} ./scripts/check-test-catalog.py --self-test
    {{ python }} ./scripts/check-domain-purity.py --self-test
    {{ python }} ./scripts/check-workspace-lints.py --self-test
    {{ python }} ./scripts/check-node-version.py --self-test
    {{ python }} ./scripts/check-web-build-coverage.py --self-test
    {{ python }} ./scripts/check-js-licenses.py --self-test
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
build-web: node-version-check
    {{ python }} ./scripts/check-web-build-coverage.py
    pnpm -r build

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

    # Preflight, before the full local gate below rather than after it. Every
    # path out of this recipe needs gh, and discovering that it is missing or
    # unauthenticated only at the end of a complete
    # lint/test/build/guards/secrets run wastes the whole gate.
    command -v gh >/dev/null 2>&1 || {
      echo "pr: the GitHub CLI is required — https://cli.github.com" >&2
      exit 1
    }
    gh auth status >/dev/null 2>&1 || {
      echo "pr: gh is not authenticated — run: gh auth login" >&2
      exit 1
    }

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

# Squash-merge a work PR into development — but only once its required checks
# are actually green.
#
# This is the gap that cost this repository a day. `just pr` watches CI when it
# OPENS a pull request, and nothing watched the moment that matters: #18 was
# merged with `rust` failing, and `just lint` was red on development from that
# merge until it was repaired. Branch protection cannot close this on the Free
# plan, so the merge path has to.
#
# The required set is re-derived for THIS PR by the same script `just pr` uses,
# so a check that has not registered yet cannot be mistaken for a check that
# passed. Refusal is the whole point: there is deliberately no override flag.
# A policy PR is *expected* to be red on branch-flow/protected-paths — that red
# is the manual security review signal described in 03-github-workflow.md §3,
# and taking it is a decision to make explicitly with `gh pr merge`, having read
# the diff, not something to wave through with a flag on this recipe.
merge $pr='':
    #!/usr/bin/env bash
    set -euo pipefail
    command -v gh >/dev/null 2>&1 || {
      echo "merge: the GitHub CLI is required — https://cli.github.com" >&2
      exit 1
    }
    gh auth status >/dev/null 2>&1 || {
      echo "merge: gh is not authenticated — run: gh auth login" >&2
      exit 1
    }
    target="$pr"
    if [ -z "$target" ]; then
      target=$(gh pr view --json url --jq .url) || {
        echo "merge: no pull request for this branch; pass one: just merge 18" >&2
        exit 1
      }
    fi

    repository=$(gh repo view --json nameWithOwner --jq .nameWithOwner) || {
      echo "merge: unable to resolve the current GitHub repository." >&2
      exit 1
    }
    [[ "$repository" =~ ^[^/[:space:]]+/[^/[:space:]]+$ ]] || {
      echo "merge: GitHub returned an invalid repository name." >&2
      exit 1
    }

    # Emit NUL-separated fields so refs and repository names remain data rather
    # than being reparsed by a shell. Every field and both branch tips must be
    # present before check evidence can be collected.
    snapshot_pr() {
      gh pr view "$1" \
        --json state,url,baseRefName,baseRefOid,headRefName,headRefOid,headRepository,isDraft,title,body \
        | ./scripts/run-python.sh -c '
    import json
    import re
    import sys

    data = json.load(sys.stdin)
    repository = data.get("headRepository")
    values = [
        data.get("state"),
        data.get("url"),
        data.get("baseRefName"),
        data.get("baseRefOid"),
        data.get("headRefName"),
        data.get("headRefOid"),
        repository.get("nameWithOwner") if isinstance(repository, dict) else None,
    ]
    if not all(isinstance(value, str) and value for value in values):
        raise SystemExit("GitHub returned an incomplete merge snapshot")
    draft = data.get("isDraft")
    title = data.get("title")
    body = data.get("body")
    if not isinstance(draft, bool):
        raise SystemExit("GitHub returned an invalid draft state")
    if not isinstance(title, str) or not title:
        raise SystemExit("GitHub returned an invalid PR title")
    if body is None:
        body = ""
    if not isinstance(body, str):
        raise SystemExit("GitHub returned an invalid PR body")
    values.extend(["true" if draft else "false", title, body])
    if not re.fullmatch(r"[0-9a-f]{40}", values[3]):
        raise SystemExit("GitHub returned an invalid base OID")
    if not re.fullmatch(r"[0-9a-f]{40}", values[5]):
        raise SystemExit("GitHub returned an invalid head OID")
    for value in values:
        sys.stdout.buffer.write(value.encode("utf-8") + b"\0")
    '
    }

    snapshot_file=$(mktemp)
    trap 'rm -f "$snapshot_file"' EXIT
    if ! snapshot_pr "$target" > "$snapshot_file"; then
      echo "merge: unable to read a complete pull-request snapshot." >&2
      exit 1
    fi
    before=()
    while IFS= read -r -d '' value; do before+=("$value"); done < "$snapshot_file"
    [ "${#before[@]}" -eq 10 ] || {
      echo "merge: GitHub returned an ambiguous pull-request snapshot." >&2
      exit 1
    }
    state=${before[0]}
    pr_url=${before[1]}
    base_ref=${before[2]}
    base_oid=${before[3]}
    head_ref=${before[4]}
    head_oid=${before[5]}
    head_repository=${before[6]}
    is_draft=${before[7]}

    [ "$state" = OPEN ] || {
      echo "merge: REFUSED — $pr_url is not open." >&2
      exit 1
    }
    [ "$is_draft" = false ] || {
      echo "merge: REFUSED — $pr_url is still a draft." >&2
      exit 1
    }
    url_prefix="https://github.com/$repository/pull/"
    case "$pr_url" in
      "$url_prefix"*) pr_number=${pr_url#"$url_prefix"} ;;
      *)
        echo "merge: REFUSED — $pr_url does not belong to $repository." >&2
        exit 1 ;;
    esac
    [[ "$pr_number" =~ ^[0-9]+$ ]] || {
      echo "merge: REFUSED — GitHub returned a malformed pull-request URL." >&2
      exit 1
    }
    [ "$base_ref" = development ] || {
      echo "merge: REFUSED — only work PRs into development may be squash-merged here." >&2
      exit 1
    }
    case "$head_ref" in
      development|staging|main|hotfix/*)
        echo "merge: REFUSED — promotions, back-merges, and hotfixes require merge commits." >&2
        exit 1 ;;
    esac
    bash ./scripts/validate-branch-flow.sh \
      "$head_ref" "$base_ref" "$head_repository" "$repository" || {
      echo "merge: REFUSED — this is not a legal work-branch route." >&2
      exit 1
    }
    printf 'merge: verified work route %s@%s -> %s@%s\n' \
      "$head_ref" "$head_oid" "$base_ref" "$base_oid"

    if ! bash ./scripts/watch-pr-checks.sh "$pr_url"; then
      echo >&2
      echo "merge: REFUSED — required checks did not all pass for $pr_url." >&2
      echo "  A policy change is expected to fail branch-flow/protected-paths;" >&2
      echo "  that red IS the review (03-github-workflow.md §3). Read the diff," >&2
      echo "  then merge it deliberately with gh pr merge." >&2
      exit 1
    fi

    if ! snapshot_pr "$pr_url" > "$snapshot_file"; then
      echo "merge: REFUSED — unable to re-read the PR after watching checks." >&2
      exit 1
    fi
    after=()
    while IFS= read -r -d '' value; do after+=("$value"); done < "$snapshot_file"
    snapshot_unchanged=true
    if [ "${#after[@]}" -ne 10 ]; then
      snapshot_unchanged=false
    else
      for index in "${!before[@]}"; do
        [ "${before[$index]}" = "${after[$index]}" ] || snapshot_unchanged=false
      done
    fi
    "$snapshot_unchanged" || {
      echo "merge: REFUSED — PR metadata, state, or branch tips changed after check evidence was collected." >&2
      exit 1
    }

    # GitHub atomically matches the reviewed head. The immediately preceding
    # snapshot also closes the base-tip race as far as the API permits.
    gh pr merge "$pr_url" --match-head-commit "$head_oid" --squash --delete-branch

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
