/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // i18n.test.ts reads the Rust error codes as text; allow only the Rust source folder outside ui/.
    fs: { allow: [".", "../src-tauri/src"] },
  },
  // app.css is read as text by theme.test.ts (CSS is stubbed out in tests otherwise).
  test: {
    environment: "jsdom",
    setupFiles: ["./src/setup.ts"],
    css: { include: [/\/app\.css\b/] },
    // The first test of a file also pays for its cold start (imports, first
    // render). On a busy Windows CI runner that went past the 5 s default.
    testTimeout: 20_000,
  },
});
