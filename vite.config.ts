/// <reference types="vitest/config" />
import { defineConfig } from "vite";

// Tauri expects a fixed dev port and handles its own console output.
export default defineConfig({
  clearScreen: false,
  // Chrono's window, the dashboard (Kairo's apps and settings), the small window a focus session
  // shows over a blocked app, and the command palette on macOS and Linux (Windows draws its own:
  // src-tauri/src/palette/native.rs).
  build: {
    rollupOptions: {
      input: { main: "index.html", dashboard: "dashboard.html", blocked: "blocked.html", palette: "palette.html" },
    },
  },
  // The web code's tests run in a simulated page, as Windows' web view: Kairo's code names keys and
  // settings by the system it reads from the user agent, and the tests expect the same answers
  // whichever system runs them.
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test-setup.ts"],
    environmentOptions: {
      jsdom: { userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Edg/130.0" },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    // The regex also catches Windows backslash paths; watching cargo's locked build files crashes Vite (EBUSY).
    watch: { ignored: ["**/src-tauri/**", /[\\/]src-tauri[\\/]/] },
  },
});
