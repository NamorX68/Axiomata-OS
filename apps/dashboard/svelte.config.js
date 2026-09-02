import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
  // Lets `<script lang="ts">` and scoped `<style>` blocks go through Vite's
  // transform pipeline (esbuild for TS, PostCSS if configured).
  preprocess: vitePreprocess(),
};
