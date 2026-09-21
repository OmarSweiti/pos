/// <reference types="vitest/config" />

import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

// vitest never loads index.html, so a bare jsdom root has `documentElement.dir
// === ""`. Read the real file rather than retyping `<html lang="ar" dir="rtl">`
// here: a hand-copied literal is the same circle as writing the attribute in a
// setup file, and it leaves `sale_screen_renders_in_rtl_by_default` green when
// index.html itself regresses to `ltr`. Measured both ways.
//
// The product-side RTL default — applying the locale at boot rather than
// inheriting it from the served document — is 1.11.1's deliverable, not this
// fixture's.
const documentFixture = readFileSync(
  fileURLToPath(new URL("./index.html", import.meta.url)),
  "utf8",
);

/**
 * Everything `ui_and_rasterizer_resolve_the_same_embedded_font` needs to read,
 * read here because the test cannot read it itself.
 *
 * `apps/terminal/tsconfig.app.json` carries no `"types": ["node"]`, so an
 * `import { readFileSync } from "node:fs"` anywhere under `src/` fails
 * `tsc -b` with TS2591 — and `just build-web` is the only thing that
 * typechecks a test. Adding node types to the app project would hand every
 * production component the filesystem to fix a test, so the I/O happens in
 * this file, which `tsconfig.node.json` already types, and travels to the
 * suite through vitest's `provide`.
 *
 * Only bytes and base URLs cross that boundary. Which paths count as "the same
 * font", how a `url()` or an `include_bytes!` argument is extracted and what
 * agreement means are all decided in the test — a fixture that pre-computed
 * the answer would be asserting against itself.
 */
const FONT_DIR = new URL("../../assets/fonts/", import.meta.url);
const RASTERISER_SOURCE = new URL(
  "../../crates/pos-hardware/src/font.rs",
  import.meta.url,
);
const SCREEN_SOURCE = new URL("./src/styles/font.css", import.meta.url);

const fontResolution = {
  screenCss: readFileSync(fileURLToPath(SCREEN_SOURCE), "utf8"),
  screenCssHref: SCREEN_SOURCE.href,
  rasteriserRust: readFileSync(fileURLToPath(RASTERISER_SOURCE), "utf8"),
  rasteriserRustHref: RASTERISER_SOURCE.href,
  // What is actually on disk, so the test can refuse a path that resolves
  // tidily and names nothing. Sorted: `readdirSync` order is the filesystem's.
  presentHrefs: readdirSync(fileURLToPath(FONT_DIR))
    .sort()
    .map((name) => new URL(name, FONT_DIR).href),
};

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  test: {
    environment: "jsdom",
    environmentOptions: { jsdom: { html: documentFixture } },
    // Unconditional, not left to each test file's import graph: the cleanup and
    // matcher registration in setup.ts are what make a second rendering test in
    // one file honest, and a file that forgot to import the harness would fail
    // as a bad selector rather than a missing hook.
    setupFiles: ["./src/test/setup.ts"],
    provide: { fontResolution },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
