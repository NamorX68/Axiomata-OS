/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// @ts-expect-error process is a nodejs global
const isVitest = process.env.VITEST === "true";

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [svelte()],

  // `npm test` → vitest: unit tests for the frontend core (no Tauri; the
  // DEV mock backend answers `invoke`). jsdom gives the CSS validator and
  // the Markdown sanitizer a DOM.
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
  // Svelte 5 client builds for component imports under vitest.
  resolve: isVitest ? { conditions: ["browser"] } : undefined,

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
