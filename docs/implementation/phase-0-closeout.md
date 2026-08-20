# Phase 0 — close-out

**Goal:** turn a working scaffold into a foundation you can build on for two years without tripping over it.
**Effort:** 1–2 days.
**Exit:** CI green on a remote, the terminal crate belongs to the workspace, the app is identifiably *this* product, and every quality gate that Phase 1 depends on is enforced rather than aspirational.

`docs/phase-0-remaining-setup.md` describes what Parts 12–13 *should* be. This file is what the repository actually needs, verified against the tree on 20 Aug 2026. Where the two differ, this one was checked.

---

## Execution status — 20 Aug 2026

| Step | | Note |
|---|---|---|
| 0.1.1 terminal crate adopted | ✅ | edition 2024, workspace inheritance, `tauri-plugin-opener` dropped |
| 0.1.2 panic lints denied | ✅ | **found 3 real `unwrap()`s in production code** — see below |
| 0.1.3 scaffold deleted | ✅ | `greet`, `App.css`, opener plugin + capability + lockfile |
| 0.1.4 pnpm config reconciled | ✅ | no warning on install |
| 0.1.5 cycle guard | ✅ | **cargo-modules can't do this** — custom module-graph checker instead |
| 0.2.1 `ci.yml` + toolchain pin | ✅ | pinned **1.97.1** (the installed toolchain), not 1.91 |
| 0.2.2 `release.yml` drafted | ✅ | inert until a `v*` tag |
| 0.2.3 branch renamed to `main` | ✅ | **remote + push are yours** — no GitHub repo exists yet |
| 0.3.1 `tauri.conf.json` | ✅ | POS Terminal, 1366×768, min-size guard, CSP locked down |
| 0.3.2 updater keys | ⬜ | needs an interactive password + your GitHub secrets |
| 0.3.3 biome migrated | ✅ | `rules.recommended` → `rules.preset` |
| 0.4.1 plans consolidated | ◐ | `docs/plan/` populated; **deleting the root duplicates is yours** |
| 0.4.2 `CLAUDE.md` | ✅ | the nine invariants on one screen |
| 0.4.3 doc link check | ✅ | `scripts/check-doc-links.sh`, folded into `just lint`, negative-tested |

**Local exit gate: 6 of 7 green.** Check 7 (CI green on a remote) is blocked on you creating the GitHub repository.

Two things reality corrected during execution:

- **The installed toolchain is 1.97.1**, not the 1.91 this document originally assumed. `rust-toolchain.toml` pins the real one.
- **`just dev-server` never loaded `apps/server/.env`**, so `/health/db` reported `unconfigured` — the setup guide's claim of `{"db":"ok"}` only held if you exported `DATABASE_URL` by hand. Fixed properly: `dotenvy` added, and `dev-server` now runs from the crate directory exactly as `migrate` already did.

And two things worth keeping:

- Enabling the panic lints (0.1.2) immediately found **three `unwrap()` calls on `Mutex::lock()` in `SimulatedPrinter`** — production code, not tests. They were replaced with poison-recovering `lock()` rather than silenced with an allow. That is the lint earning its place on day one.
- The cycle guard (0.1.5) was specified as `cargo modules --acyclic`, which **cannot work** — see that step. Both new guards (`check-domain-acyclic.py`, `check-doc-links.sh`) were **negative-tested**: each was shown failing on a deliberately introduced fault before being trusted. A guard nobody has seen fail is a guard nobody should trust.

---

## Verified current state
*(as it was before close-out — the table this plan was written against)*

| Fact | Evidence |
|---|---|
| 12 commits, branch `master`, **no remote** | `git log`, `git remote -v` |
| `.github/workflows/` exists but is **empty** — no CI has ever run | directory present, no files |
| Rust: 576 lines across four crates plus the server | `wc -l` |
| `pos-domain` contains **only** `money.rs` — no tax, no cart, no ids | `src/lib.rs` |
| `pos-db` migration `0001_init` only; `sale_line.qty` is `INTEGER` units | `migrations/` |
| `apps/terminal/src-tauri/Cargo.toml` is `edition = "2021"`, `authors = ["you"]`, `description = "A Tauri App"`, no workspace inheritance | file contents |
| `tauri.conf.json`: `productName: "terminal"`, 800×600 window, `csp: null`, no updater. Identifier `com.perfectcoders.pos` **is** already real | file contents |
| The scaffold `greet` command is still registered | `src-tauri/src/lib.rs` |
| `pnpm-workspace.yaml` sets `allowBuilds: { esbuild: false }` **and** `onlyBuiltDependencies: [esbuild, …]` — contradictory | file contents |
| Root `package.json` carries a `pnpm.onlyBuiltDependencies` block pnpm 11 no longer reads | file contents |
| `apps/terminal/src/App.css` is orphaned | no importer |
| `biome.json` uses `linter.rules.recommended`, deprecated in Biome 3 | file contents |

---

## Group 0.1 — Workspace hygiene

### 0.1.1 — Adopt the terminal crate into the workspace

The terminal crate is a workspace member that ignores every workspace convention. Left alone, it drifts: a different edition means different `unsafe` rules and different lint defaults from the crates it links.

**Files:** `apps/terminal/src-tauri/Cargo.toml`

```toml
[package]
name = "terminal"
version.workspace = true
edition.workspace = true          # 2024, matching every other crate
rust-version.workspace = true
description = "POS register terminal"
authors = ["Omar Sweiti"]
```

Keep the `[lib]` block exactly as it is — the `_lib` suffix and `crate-type` are load-bearing on Windows.

**Verify:** `cargo check -p terminal` · **Done when:** `cargo metadata | grep '"edition"'` shows `2024` for every member.

### 0.1.2 — Deny panics at the workspace level

`unwrap()` in a register is a lost sale and an angry queue. Ban it now, while there are two occurrences, rather than in Phase 2 with four hundred.

**Files:** `Cargo.toml` (root)

```toml
[workspace.lints.clippy]
unwrap_used         = "deny"
expect_used         = "deny"
panic               = "deny"
indexing_slicing    = "warn"
float_arithmetic    = "deny"     # conventions I-1: no float touches money
todo                = "warn"
dbg_macro           = "deny"
```

Then `[lints] workspace = true` in every member's `Cargo.toml`. Tests and `main()` opt out with `#![cfg_attr(test, allow(clippy::unwrap_used))]` — the existing `#[test]` blocks and `apps/server/src/main.rs` need this.

**Verify:** `cargo clippy --workspace --all-targets -- -D warnings` · **Done when:** clean, with every exemption explicit.

> `float_arithmetic = "deny"` is aggressive and correct. `pos-domain` should never see a float. Where one is genuinely needed (retry jitter in `pos-fiscal`, layout maths in the receipt rasteriser), the allow is local, one line, and carries a comment saying why it is not money.

### 0.1.3 — Delete the scaffold

**Files:** `apps/terminal/src-tauri/src/lib.rs` (remove `greet` and its handler entry) · `apps/terminal/src/App.css` (delete) · `Cargo.toml` root (`tauri-plugin-opener` stays only if something opens a URL; nothing does yet — drop it and its capability entry).

Keep `split_tender`. It is the working proof of the UI → IPC → domain boundary and Phase 1 grows from it.

**Verify:** `just lint && just test` · **Done when:** no reference to `greet` survives; `rg -i 'greet' --glob '!target'` is empty.

### 0.1.4 — Reconcile the pnpm build-script config

`pnpm-workspace.yaml` says `allowBuilds: { esbuild: false }` while `onlyBuiltDependencies` lists `esbuild`. One of them is wrong and pnpm prints a warning on every command.

**Files:** `pnpm-workspace.yaml` (drop the `allowBuilds` block) · `package.json` (drop the `pnpm` block — pnpm 11 reads it from the workspace file).

**Verify:** `pnpm install` · **Done when:** no warning printed.

### 0.1.5 — Cycle guard on the domain crate

`pos-domain`'s module graph ([`ref/domain-api.md`](ref/domain-api.md) §15) is acyclic by design. Make that mechanical before there are sixteen modules.

**Files:** `scripts/check-domain-acyclic.py`, `justfile`, `.github/workflows/ci.yml`

> ⚠️ **Not `cargo modules dependencies --acyclic`.** That was the obvious answer and it does not work: cargo-modules builds an **item-level** graph, so `Money::from_minor → Money` — any constructor returning `Self` — is reported as a circular dependency. Filtering with `--no-fns --no-types --no-traits` does not help; cycle detection runs before the filters. The tool cannot express "modules must be acyclic".

`scripts/check-domain-acyclic.py` parses `use crate::…` across `crates/pos-domain/src/*.rs`, builds the module graph, and DFS-colours it for cycles. It runs from `just lint` (as `just acyclic`) and from the `rust` CI job.

**Done when:** **negative-tested** — drop two modules that `use crate::` each other into `pos-domain/src/`, watch it print the cycle path and exit 1, delete them, watch it pass.

---

## Group 0.2 — CI, for real

The existing setup doc contains good workflow YAML that has never run. Committing it is the step.

### 0.2.1 — `ci.yml`

**Files:** `.github/workflows/ci.yml` (new)

Take the YAML from `docs/phase-0-remaining-setup.md` §12.1 with four changes:

1. `branches: [main]` — and actually rename the branch (0.2.3), or the push trigger never fires.
2. Add the `cargo-modules` step from 0.1.5.
3. Add a `services: postgres:` block to the `rust` job now, not when server tests arrive — the first server test should not also be a CI debugging session.
4. Pin `rust-toolchain.toml` to a specific version rather than `stable`, so a Rust release never turns a green build red on a day you are shipping.

**Files:** `rust-toolchain.toml` (new)

```toml
[toolchain]
channel = "1.97.1"          # the installed toolchain; bump as a commit, with CI proving it
components = ["rustfmt", "clippy"]
```

**Verify:** push, watch both jobs · **Done when:** `rust` and `web` are green.

### 0.2.2 — `release.yml`, drafted

**Files:** `.github/workflows/release.yml` (new)

Use §12.2 of the setup doc verbatim. It stays inert until a `v*` tag exists. Code-signing secrets are added before the first external pilot (Phase 5), not now — but the workflow being present means signing is a secrets change rather than a new build system under deadline.

**Done when:** the file is committed and GitHub lists it as a workflow.

### 0.2.3 — Remote, branch, protection

```bash
git branch -M main
git remote add origin git@github.com:<you>/pos.git
git push -u origin main
```

Then, in repository settings: protect `main`, require the `rust` and `web` checks, disallow force-push.

**Done when:** a PR cannot merge with a red check. Verify by opening one that fails.

---

## Group 0.3 — Product identity

Small, boring, and permanently annoying if skipped — every one of these leaks into a filename, a window title, or an installer a merchant sees.

### 0.3.1 — `tauri.conf.json`

**Files:** `apps/terminal/src-tauri/tauri.conf.json`

```jsonc
{
  "productName": "POS Terminal",
  "version": "0.1.0",
  "identifier": "com.perfectcoders.pos",   // already correct — leave it
  "app": {
    "windows": [{
      "title": "POS Terminal",
      "width": 1366, "height": 768,        // a real register screen
      "minWidth": 1024, "minHeight": 640,  // sale-screen min-size guard (E.60)
      "resizable": true,
      "fullscreen": false
    }],
    "security": {
      // csp: null lets the webview load anything. Lock it now; a POS webview
      // has no business making outbound requests — the Rust core does that.
      "csp": "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost"
    }
  }
}
```

**Verify:** `pnpm dev:terminal` · **Done when:** the window is titled POS Terminal, 1366×768, will not shrink below 1024×640, and the devtools console shows no CSP violation.

### 0.3.2 — Updater keys, generated but unused

```bash
pnpm --filter terminal tauri signer generate -w ~/.tauri/pos-updater.key
```

Public key into `tauri.conf.json` `plugins.updater.pubkey`; private key and password into GitHub secrets. The updater **endpoint stays empty** — auto-update is a Phase 5 feature and the product rule is *never apply an update while a shift is open* (E.56).

**Done when:** keys exist, are in secrets, and the private key is **not** in the repository. Confirm with `git log -p | rg 'PRIVATE KEY'` returning nothing.

### 0.3.3 — Biome deprecation

**Files:** `biome.json` — run `pnpm biome migrate --write`. Confirm `linter.rules.recommended` becomes `linter.rules.preset` and that `just lint` still passes. Keep the `!**/public` exclusion; the reason (scaffold SVGs tripping `a11y/noSvgWithoutTitle`) is documented and still true.

---

## Group 0.4 — Documentation in the repo

### 0.4.1 — Tidy the duplicated plan documents  *(already half done)*

`pos-engineering-blueprint.md` lived only in `~/Downloads` — a plan the master plan cites constantly and that was not in the repository at all. It has been **imported**, along with canonical copies of the other two, so this documentation set links to real files:

```
docs/plan/
├── engineering-blueprint.md            ← imported from ~/Downloads
├── business-functional-master-plan.md  ← copy of the root file
└── phase-0-setup-guide.md              ← copy of docs/phase-0-remaining-setup.md
```

What remains is removing the now-redundant originals. These are your files, so the deletions are left to you:

```bash
rm 'pos-business-functional-master-plan.md'
rm 'pos-business-functional-master-plan(1).md'   # byte-identical duplicate of the above
git rm docs/phase-0-remaining-setup.md           # superseded by docs/plan/phase-0-setup-guide.md
git add docs/plan docs/implementation
git commit -m "docs: consolidate plans under docs/plan; add implementation set  [0.4.1]"
```

**Done when:** `docs/plan/` holds all three, the repository root holds no loose plan files, and every link in `docs/implementation/` resolves (`just docs-links`, added in 0.4.3).

### 0.4.2 — `CLAUDE.md` / `CONTRIBUTING.md`

A short file at the root pointing at [`01-conventions.md`](01-conventions.md) and stating the nine invariants in one screen. Anyone — human or agent — landing in this repository should hit the money rule before they hit a keyboard.

### 0.4.3 — A link check in CI

**Files:** `scripts/check-doc-links.sh`, `justfile`, `.github/workflows/ci.yml`

This documentation set is worth only as much as its cross-references. `scripts/check-doc-links.sh` walks every relative `.md` link under `docs/`, strips anchors, and fails on an unresolvable target. It runs from `just lint` and from the `web` CI job.

> One trap worth knowing: with `set -o pipefail`, a `grep` that finds nothing returns 1 and fails the pipeline — so every link-free file reads as broken. The script collects targets into a variable instead of piping into the loop.

**Done when:** the checker is **negative-tested** — add a link to a nonexistent file, watch it fail, remove it, watch it pass. A guard nobody has seen fail is a guard nobody should trust.

---

## Exit gate

Run in order. All seven must pass.

```bash
just lint                      # fmt + clippy -D warnings + biome
just test                      # 6 Rust + 2 Vitest, all green
just db-up && just migrate     # container healthy, sqlx migration applied
just dev-server &              # then:
curl -s localhost:8080/health      | grep -q '"status":"ok"'
curl -s localhost:8080/health/db   | grep -q '"db":"ok"'
pnpm dev:terminal              # window titled POS Terminal, 1366×768,
                               # "Split via Rust" returns values over IPC
pnpm dev:backoffice            # placeholder renders with Tailwind styles
git push && gh run watch       # both CI jobs green on the remote
```

Plus, by inspection:

- `cargo metadata` shows edition 2024 for every member.
- `rg 'unwrap\(\)' crates/ apps/ --glob '!*test*'` returns nothing outside `main()`.
- No `greet`, no orphaned `App.css`, no pnpm warning on install.
- `docs/plan/` holds the blueprint, the master plan, and the setup guide.
- A failing PR cannot merge into `main`.

---

## What Phase 0 deliberately does not do

Recorded so nobody adds it here and delays the gate:

- **No signing or notarization.** The workflow exists; the certificates are a Phase 5 purchase.
- **No `pos-fiscal` crate.** Phase 2.
- **No schema beyond `0001`.** Phase 1 opens with `0002`, and `0002` includes the `sale_line.qty` fix (gap G-12) — which is why no sale row may exist yet.
- **No UI beyond the smoke panel.** The sale screen is Phase 1, and it is built RTL-first from its first commit, not retrofitted.

→ **Next:** [`phase-1-sellable-mvp.md`](phase-1-sellable-mvp.md)
