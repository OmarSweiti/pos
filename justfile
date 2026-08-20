set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# List recipes
default:
    just --list

# One-time / after pulling
setup:
    pnpm install
    cargo fetch

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

# Documentation cross-references must resolve (CI runs this too)
docs-links:
    ./scripts/check-doc-links.sh

# pos-domain's module graph must stay acyclic (ref/domain-api.md §15)
acyclic:
    ./scripts/check-domain-acyclic.py

# ref/schema.md must be executable SQLite and obey conventions §2.
# Not yet in `just lint`: migration 0006 elides its FTS trigger bodies, so this
# fails today. Wire it in the moment that block is written.
verify-schema:
    ./scripts/verify-schema.py

# ── quality gates (CI runs exactly these) ─────────────────
test:
    cargo nextest run --workspace
    pnpm -r --if-present test

lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    ./scripts/check-domain-acyclic.py
    pnpm biome ci --error-on-warnings .
    ./scripts/check-doc-links.sh

fmt:
    cargo fmt --all
    pnpm biome format --write .
