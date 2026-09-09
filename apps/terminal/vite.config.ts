/// <reference types="vitest/config" />

import { readFileSync } from "node:fs";
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
