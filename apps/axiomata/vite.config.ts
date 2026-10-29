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
    // Vitest blanks every stylesheet by default; the theme tests read the theme files' text (`?raw`).
    css: { include: [/themes\/.*\.css/] },
    // Vitest loads `.env.local` the same way the dev server does, so a
    // developer's own dev flag would otherwise decide test outcomes — and
    // `VITE_AXIOMATA_DISABLE_AUTO_REFRESH=true` (the flag CLAUDE.md suggests
    // keeping set) made two `core/devFlags` tests fail on that machine alone.
    // Tests state the environment they want with `vi.stubEnv`; here every dev
    // flag starts unset, whatever the machine has in `.env.local`.
    env: { VITE_AXIOMATA_DISABLE_AUTO_REFRESH: "" },
  },
  // The CSP's nonce (`core/htmllink.ts`): Vite puts this placeholder on the tags it writes into `index.html`, Tauri
  // fills a fresh nonce into it on every load and allows it in the policy's `script-src`; a course page framed by the
  // file viewer takes it for its own scripts. Only for a build — the tests have no `index.html`.
  html: isVitest ? undefined : { cspNonce: "__TAURI_SCRIPT_NONCE__" },
  // Svelte 5 client builds for component imports under vitest.
  resolve: isVitest ? { conditions: ["browser"] } : undefined,

  // Fonts are never inlined as `data:` URLs (card #76): the CSP has no `font-src data:`, and the small `@fontsource`
  // subsets Vite would inline (Vietnamese, Greek, Cyrillic, symbol ranges) were blocked in the installed app only —
  // `vite dev` serves them as files. Everything else keeps Vite's default limit (`undefined`).
  build: {
    assetsInlineLimit: (file: string) => (/\.(woff2?|ttf|otf)$/i.test(file) ? false : undefined),
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    // The handbook's coverage test reads the pages where the build embeds them from (`docs/handbook/`, outside the
    // app's folder). Only under vitest: the dev server keeps Vite's default boundary.
    fs: isVitest ? { allow: ["../.."] } : undefined,
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
