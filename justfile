set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# List recipes
default:
    just --list

# One-time / after pulling
setup:
    pnpm install
    cargo fetch
    just hooks
    just identity

# Branch protection is not available on a private repo on the GitHub Free plan,
# so these hooks ARE the protection: a machine that has not run this can push
# straight to main.
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

# ── inner loop ────────────────────────────────────────────
# Compile + borrow-check everything, tests and benches included.
# The fastest signal that says "this would build" — seconds, not minutes.
check:
    cargo check --workspace --all-targets

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
    ./scripts/check-doc-links.sh

# Conventions §10: RTL is the default, so every layout uses CSS logical
# properties. biome's recommended preset does not know Tailwind or CSS sides.
logical-css:
    ./scripts/check-logical-css.sh

# pos-domain's module graph must stay acyclic (ref/domain-api.md §15)
acyclic:
    ./scripts/check-domain-acyclic.py

# ref/schema.md must be executable SQLite and obey conventions §2, and the
# SHIPPED migrations are audited on their own in the same run.
verify-schema:
    ./scripts/verify-schema.py

# Advisories, licences and banned/duplicate crates. Same gate CI runs.
#   cargo install cargo-deny --locked
audit:
    cargo deny check
    pnpm audit --audit-level high

# ── quality gates (CI runs exactly these) ─────────────────
test:
    cargo nextest run --workspace
    pnpm -r --if-present test

lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    ./scripts/check-domain-acyclic.py
    ./scripts/verify-schema.py
    ./scripts/check-logical-css.sh
    pnpm biome ci --error-on-warnings .
    ./scripts/check-doc-links.sh

fmt:
    cargo fmt --all
    pnpm biome format --write .

# The write guards are not advisory. Prove they still refuse (CLAUDE.md).
guards:
    ./.claude/hooks/test-protect-immutable.sh
    ./.githooks/test-hooks.sh
    ./scripts/verify-schema.py --self-test
    ./scripts/check-logical-css.sh --self-test

# `lint` is biome (style) and `test` is vitest, so a TypeScript type error passes
# both and fails CI's `web` job instead. Mirrors that job's build step.
# The only place `tsc` runs. Part of `pre-push` for exactly that reason.
build-web:
    pnpm -r --if-present build

# Everything CI runs, plus the guards. The last thing you type before a push.
pre-push: lint test build-web guards

# ── GitHub ────────────────────────────────────────────────────────────────
# docs/implementation/03-github-workflow.md is the reference for all of these.

# Labels, milestones, merge behaviour, default branch. Idempotent.
gh-bootstrap:
    ./scripts/gh-bootstrap.sh

gh-bootstrap-dry:
    ./scripts/gh-bootstrap.sh --dry-run

# Branch protection. Refuses politely on the Free plan — see the script's header.
gh-protect:
    ./scripts/gh-protect.sh

# The delivery project and its fields. Needs: gh auth refresh -s project,read:project
gh-project:
    ./scripts/gh-project.sh

# Start a group. `just branch phase-1/group-3-tax`
branch name:
    git switch development
    git pull --ff-only
    git switch -c {{name}}

# Open the PR for the branch you are on. Always into development.
pr:
    just pre-push
    git push -u origin HEAD
    gh pr create --base development --fill-first
    gh pr checks --watch

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
