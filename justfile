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
    cargo run -p pos-server

db-up:
    docker compose -f infra/docker-compose.yml up -d

db-down:
    docker compose -f infra/docker-compose.yml down

migrate:
    cd apps/server; sqlx migrate run

# ── quality gates (CI runs exactly these) ─────────────────
test:
    cargo nextest run --workspace
    pnpm -r --if-present test

lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    pnpm biome ci .

fmt:
    cargo fmt --all
    pnpm biome format --write .
