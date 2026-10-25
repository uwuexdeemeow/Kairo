import { defineConfig } from "vite";

// Tauri expects a fixed dev port and handles its own console output.
export default defineConfig({
  clearScreen: false,
  // The main window, the small window a focus session shows over a blocked app, and the
  // command palette.
  build: {
    rollupOptions: {
      input: { main: "index.html", blocked: "blocked.html", palette: "palette.html" },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    // The regex also catches Windows backslash paths; watching cargo's locked build files crashes Vite (EBUSY).
    watch: { ignored: ["**/src-tauri/**", /[\\/]src-tauri[\\/]/] },
  },
});
