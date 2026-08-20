/// <reference types="vitest/config" />

import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],

  // The back office renders components, so its tests need a DOM. The terminal's
  // suite is "node" because its logic is deliberately DOM-free (lib/direction.ts
  // takes the document root as an argument rather than reaching for it).
  test: { environment: "jsdom" },
});
