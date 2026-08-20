# Phase 0 — remaining setup (Parts 12 → 13)

Everything through **Part 11** is built, verified, and committed. Parts 12 and 13 are
**documented here, not executed** — they touch GitHub (workflows, remote, tags), which is
yours to trigger.

Repo state at the time of writing: branch `master`, **no remote configured**, `.github/workflows/`
exists but is empty (git does not track empty directories, so it is not in any commit).

---

## What is already done

| Commit | Part |
| --- | --- |
| `70254dc` | chore: monorepo skeleton (cargo + pnpm workspaces) |
| `e62f21f` | feat(domain): Money with exact split + property tests |
| `7dda4f4` | feat(db): SQLCipher store, key provider, migration runner, schema v1 subset |
| `1301a70` | chore(sync): wire pos-sync workspace deps |
| `37c1b47` | chore(server): wire axum/tokio/sqlx workspace deps |
| `319ff11` | feat(hardware): printer trait + simulator (blueprint §5) |
| `fe0944d` | feat(terminal): IPC split_tender wired to pos-domain; tailwind+zustand+query+vitest |
| `77d9618` | chore(backoffice): scaffold with tailwind |
| `4136ad7` | feat(server): axum health endpoints, pg via compose, first sqlx migration |
| `f5606bb` | feat(sync): protocol types skeleton |
| `9c5f105` | chore: biome + justfile quality gates |

Verified locally: `just fmt` → clean, `just lint` → clean (`cargo fmt --check`, `clippy -D warnings`,
`biome ci`), `just test` → 6 Rust tests + 2 Vitest tests pass, Postgres healthy with migration
`20260819200319_init` applied, and `GET /health` / `GET /health/db` both `200 ok`.

---

## Part 12 — CI on GitHub Actions

### 12.1 `.github/workflows/ci.yml`

```yaml
name: ci
on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # Tauri's Linux system deps (the src-tauri crate is a workspace member)
      - name: Install system dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
            libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@nextest

      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo nextest run --workspace

  web:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4          # reads "packageManager" from package.json
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm biome ci .
      - run: pnpm -r --if-present test
      - run: pnpm -r --if-present build
```

The `rust` job runs exactly the three commands in the `lint` and `test` recipes of the
[justfile](../justfile), so a local `just lint && just test` predicts CI.

### 12.2 `.github/workflows/release.yml`

Draft now, activate at the Phase 0 exit ("a signed installer on all 3 OSes").

```yaml
name: release
on:
  push:
    tags: ["v*"]

jobs:
  build:
    permissions:
      contents: write
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: macos-latest
            args: "--target universal-apple-darwin"
          - platform: ubuntu-22.04      # build on oldest supported glibc
            args: ""
          - platform: windows-latest
            args: ""
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4
      - name: Linux system deps
        if: matrix.platform == 'ubuntu-22.04'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
            libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.platform == 'macos-latest' && 'aarch64-apple-darwin,x86_64-apple-darwin' || '' }}
      - uses: Swatinem/rust-cache@v2
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with: { node-version: 22, cache: pnpm }
      - run: pnpm install --frozen-lockfile

      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # Updater artifact signing (generate once: `pnpm --filter terminal tauri signer generate`)
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
          # OS code signing (Windows Authenticode cert, Apple Developer ID +
          # notarization creds) gets added here before any external pilot — §7.
        with:
          projectPath: apps/terminal
          tagName: ${{ github.ref_name }}
          releaseName: "POS Terminal ${{ github.ref_name }}"
          releaseDraft: true
          args: ${{ matrix.args }}
```

### 12.3 Push

```bash
git add -A && git commit -m "ci: rust + web pipelines; draft release workflow"
git branch -M main
git remote add origin git@github.com:<you>/pos.git   # create the repo on GitHub first
git push -u origin main
```

Note the ordering: `ci.yml` triggers on pushes to `main`, and this repo is still on `master` —
rename before the first push or the `push` trigger never fires (pull requests would still run).

### 12.4 Things CI will need that only exist locally right now

- **`apps/server/.env`** is git-ignored (only `.env.example` is committed). Nothing in CI reads
  `DATABASE_URL` yet, because the server has no tests. When server tests arrive, add a
  `services: postgres:` block to the `rust` job.
- **SQLx offline mode** is not needed yet — the queries are runtime strings. Once you start using
  the compile-time-checked `query!` macros, run `cargo sqlx prepare --workspace` and commit the
  `.sqlx/` folder so CI builds without a live database.
- **Tauri bundle identifier** is still the scaffold default in
  `apps/terminal/src-tauri/tauri.conf.json`; `tauri-action` will refuse to bundle until it is real.

---

## Part 13 — The exit gate

Seven checks. Five are verified locally already; two need a screen or a remote.

| # | Command | Status |
| --- | --- | --- |
| 1 | `just lint` | ✅ clean (fmt + clippy `-D warnings` + biome) |
| 2 | `just test` | ✅ 6 Rust tests, 2 Vitest tests |
| 3 | `just db-up && just migrate` | ✅ container healthy, `20260819200319_init` applied |
| 4 | `just dev-server` + `curl :8080/health`, `:8080/health/db` | ✅ `{"status":"ok",…}` and `{"db":"ok"}` |
| 5 | `pnpm dev:terminal` — window opens, "Split via Rust" returns values over IPC | ⬜ needs a desktop session; the Rust side (`cargo check -p terminal`) and the frontend build both pass |
| 6 | `pnpm dev:backoffice` — placeholder renders with tailwind styles | ⬜ dev server not launched; `pnpm --filter backoffice build` emits 6.67 kB of Tailwind CSS |
| 7 | `git push` → both GitHub Actions jobs green | ⬜ blocked on Part 12 |

### Immediate backlog after the gate (from the blueprint)

1. **Tax engine** in `pos-domain` (`tax_rule` with basis points, inclusive/exclusive, configurable
   rounding via `rust_decimal`) + the property "line taxes sum to receipt tax within the rounding
   rule" (§8).
2. **Cart/checkout state machine** as a Rust enum with transition functions
   (Idle → Building → Tendering → Finalizing → Complete, plus Parked/Voided) — illegal transitions
   must not compile (§8).
3. **Full schema** as `pos-db` migrations 0002+ (tax tables, shifts, cash movements, users/roles
   with Argon2id PINs, hash-chained `audit_log`, customers) (§3).
4. **Outbox writer**: every sale insert also writes `sync_outbox` in the same transaction (§4) —
   then the push/pull endpoints on the server.
5. **ESC/POS renderer** (template → bytes) with golden-file tests, behind the `ReceiptPrinter`
   trait (§5, §8).

---

## Troubleshooting

| Symptom | Cause → fix |
| --- | --- |
| `failed to select a version for libsqlite3-sys` / `links to the native library sqlite3 … conflicts` | `rusqlite` pinned too new for `sqlx` to coexist (cargo resolves sqlx's *optional* sqlite driver into the lock and enforces `links` uniqueness) → keep `rusqlite = "0.39"` in `[workspace.dependencies]` until sqlx accepts `libsqlite3-sys` 0.38+. **Already applied in `7dda4f4`.** |
| `linker 'cc' not found` / `link.exe not found` | Missing C toolchain → Part 1: `build-essential` (Linux), Xcode CLT (macOS), VS Build Tools C++ workload (Windows). |
| `webkit2gtk-4.1 not found` during `cargo check` | Linux deps missing → rerun the apt line in 1C. |
| Building `openssl-src` fails on Windows | Perl/NASM missing → 1A installs Strawberry Perl + NASM; open a new terminal so PATH refreshes. |
| `pos_db` test fails with `BadKey` unexpectedly | The key changed between opens, or an old unencrypted `.db` file exists at that path — delete it. |
| pnpm prints "Ignored build scripts: esbuild…" | pnpm ≥ 10 script blocking → `pnpm approve-builds`, approve `esbuild` + `@tailwindcss/oxide` (or keep the `onlyBuiltDependencies` list from 3.6). |
| `pnpm dev:terminal` shows a blank white window (Linux) | Webview compositing quirk on some GPUs → `WEBKIT_DISABLE_COMPOSITING_MODE=1 pnpm dev:terminal`. |
| `error: edition 2024 is unstable` | Old Rust → `rustup update` (needs ≥ 1.85). |
| Tauri build error about the bundle identifier | Default identifier kept → set a real one in `apps/terminal/src-tauri/tauri.conf.json`. |
| `sqlx migrate run: connection refused` | Postgres not up/healthy → `just db-up`, wait for the healthcheck; check nothing else owns port 5432. |
| CI `rust` job fails only on clippy | It runs `-D warnings` exactly like `just lint` — fix locally with `just lint` before pushing. |

---

## Deviations from the guide, and open drift

These were necessary to make Parts 7–11 actually pass on this machine (rustc 1.91, pnpm 11.22,
Biome 2.5.9):

1. **`biome.json` excludes `**/public`.** Biome 2.5 lints SVG files, and the three scaffold assets
   (`apps/*/public/*.svg`) trip `a11y/noSvgWithoutTitle`, which made `pnpm biome ci .` fail. The
   guide's four `includes` entries are otherwise unchanged.
2. **One Biome suppression in `apps/terminal/src/App.tsx`** — `lint/suspicious/noArrayIndexKey` on
   the `splits.map` list. A tender's position *is* its identity here, so the index is the correct
   key; the comment records that.
3. **`apps/backoffice/src/main.tsx`** — the scaffold's `document.getElementById('root')!` became
   `as HTMLElement` (matching the terminal's `main.tsx`) to clear `style/noNonNullAssertion`.
4. **`biome ci` prints a deprecation notice**: `linter.rules.recommended` will become
   `linter.rules.preset` in Biome 3. Informational only — `biome migrate` will rewrite it when you
   care to.
5. **`pnpm` warns on every command**: `"pnpm": { "onlyBuiltDependencies": [...] }` in the root
   `package.json` is no longer read by pnpm 11. The same list already lives in
   `pnpm-workspace.yaml`, so deleting the `package.json` field silences the warning with no
   behaviour change. `pnpm-workspace.yaml` also carries an `allowBuilds: { esbuild: false }` entry
   that contradicts `onlyBuiltDependencies: [esbuild]` — worth reconciling.
6. **`apps/terminal/src/App.css`** is now unimported (the new `App.tsx` dropped it) but still on
   disk; delete when convenient.
