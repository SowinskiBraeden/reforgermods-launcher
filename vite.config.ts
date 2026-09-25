import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

// Tauri serves the dev build from a fixed port and copies `dist/` into the
// bundle for release builds.
export default defineConfig({
  plugins: [preact()],
  // Fail loudly rather than silently picking another port: Tauri's devUrl is fixed.
  server: { port: 1420, strictPort: true },
  clearScreen: false,
  build: {
    // Matches the WebView2 / WKWebView baseline Tauri 2 targets, so no
    // unnecessary transpilation bloat ships.
    target: "es2022",
    sourcemap: false,
    // The whole UI is small; one chunk avoids extra requests at startup.
    chunkSizeWarningLimit: 300,
  },
});
