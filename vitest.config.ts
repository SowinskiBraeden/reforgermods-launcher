import { defineConfig } from "vitest/config";
import preact from "@preact/preset-vite";

export default defineConfig({
  plugins: [preact()],
  test: {
    // Most tests are pure; the shell smoke test opts into jsdom per file.
    environment: "node",
    css: false,
  },
});
