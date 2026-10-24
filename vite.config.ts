import { defineConfig } from "vite";

// Tauri expects a fixed dev port and handles its own console output.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // The regex also catches Windows backslash paths; watching cargo's locked build files crashes Vite (EBUSY).
    watch: { ignored: ["**/src-tauri/**", /[\\/]src-tauri[\\/]/] },
  },
});
