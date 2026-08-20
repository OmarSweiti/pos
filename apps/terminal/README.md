# POS Terminal

The register: a Tauri 2 desktop application. React and TypeScript in `src/`, the
Rust shell in `src-tauri/`. Business rules do not live here — they live in
`crates/pos-domain`, and this application marshals between the UI and that crate
across the Tauri IPC boundary.

## Running it

From the repository root:

```bash
just dev-terminal        # or: pnpm --filter terminal tauri dev
```

The Vite dev server is pinned to port 1420 because Tauri expects it there; the
build fails rather than silently moving.

## Layout

```
src/                  React UI
  lib/direction.ts    locale and writing direction — Arabic/RTL is the default
  store/              zustand stores
src-tauri/
  src/lib.rs          the #[tauri::command] surface — the whole IPC boundary
  capabilities/       Tauri permissions, deny-by-default
  tauri.conf.json     window, bundle identifier, CSP
```

## Conventions that bite here

- **Arabic and RTL are the default**, not a translation layer (conventions §10).
  `lang` and `dir` move together; a root where they disagree renders Arabic text
  left-to-right.
- **Money is never a float and never assumes two decimal places.** Format it
  through `@pos/money`, which requires a currency — JOD has three (conventions
  §1, I-1 and I-2).
- **A command handler checks permissions in Rust.** Hiding a button is UX; the
  check is the security.
- `unwrap()` and `expect()` are denied outside tests and the entry point.

## Recommended IDE setup

[VS Code](https://code.visualstudio.com/) with
[Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
and [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).
