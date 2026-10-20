**What changes at runtime.** `rustls` 0.23.45 → 0.23.46 reaches one binary: `pos-server`, through `sqlx`'s Postgres layer (`cargo tree -i rustls`). The register app does not link it. `source-map-js` 1.2.1 → 1.2.2 is build-time for both web apps, through Tailwind's Vite plugin, and jsdom in tests.

**Smoke run on this head.** Both web apps were served through the patched toolchain with a fresh `pnpm install --frozen-lockfile`, then rendered by headless Chrome at the register's window size (1366 × 768), with a throwaway browser profile.

| # | Step | Expected | Observed |
|---|---|---|---|
| 1 | `pnpm --filter terminal dev` on :1420, then render `/` | Vite starts, and the register UI renders in Arabic-first RTL | Vite 8.3.1 ready; `GET /` 200; the Phase-0 smoke panel renders with `lang=ar dir=rtl`, the split control and the language toggle |
| 2 | `pnpm --filter backoffice dev` on :5180, then render `/` | Vite starts, and the back office renders | Vite 8.3.1 ready; the Phase-3 placeholder renders |

**Gates on this head:** `just audit`: `advisories ok, bans ok, licenses ok, sources ok`, and `pnpm audit --audit-level high` found no known vulnerabilities. `just pre-push` is all green: clippy `-D warnings` over `pos-server` with rustls 0.23.46, 416 Rust tests, the web suites, and build-web for both apps.

**Not run here, and why.**
- **`pos-server` itself was not started.** It binds the hard-coded `127.0.0.1:8080`, which on this machine is held by an unrelated ssh tunnel. Its `/health/db` connects to `:5432`, where another project's Postgres runs. Neither is this PR's to touch. The server compiles and lints clean with the new rustls, and has no tests yet (owner: 3.10.4).
- **The register as a desktop app (Tauri window) was not captured.** This terminal lacks macOS Screen Recording permission. The Tauri shell does not link rustls, so the web render above exercises everything this PR changes on the register side.
