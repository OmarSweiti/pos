/// <reference types="vitest/config" />

import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],

  // The back office renders components, so its tests need a DOM. So does the
  // terminal, since microstep 1.11.0 gave it the harness its screen tests need;
  // its own config owns the document fixture and setup file that harness adds.
  test: { environment: "jsdom" },
});
