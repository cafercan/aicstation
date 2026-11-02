/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // app.css is read as text by theme.test.ts (CSS is stubbed out in tests otherwise).
  test: { environment: "jsdom", setupFiles: ["./src/setup.ts"], css: { include: [/\/app\.css\b/] } },
});
