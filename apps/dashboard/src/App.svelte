<!--
  Shell composition: TopBar → IconBar → Canvas → (assistant bar, step 11).
  Until the canvas modules replace them, the pre-M5 panels sit collapsed
  below the canvas so the app stays useful.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import Canvas from "./canvas/Canvas.svelte";
  import { on } from "./core/bus";
  import IconBar from "./shell/IconBar.svelte";
  import LegacyPanels from "./shell/LegacyPanels.svelte";
  import ModulePicker from "./shell/ModulePicker.svelte";
  import Toasts from "./shell/Toasts.svelte";
  import TopBar from "./shell/TopBar.svelte";

  let pickerOpen = $state(false);

  // Stub handlers for the icon-bar events until their dialogs exist
  // (settings: step 13, search: step 11).
  onMount(() => {
    const offs = [
      on("shell:add-module", () => (pickerOpen = true)),
      ...["shell:settings", "shell:search"].map((ev) =>
        on(ev, () => console.info(`${ev} — not wired yet`)),
      ),
    ];
    return () => offs.forEach((off) => off());
  });
</script>

<TopBar />
<IconBar />
<Toasts />
<ModulePicker bind:open={pickerOpen} />

<div class="board">
  <Canvas />
  <details class="legacy">
    <summary>Legacy panels (pre-M5)</summary>
    <LegacyPanels />
  </details>
</div>

<style>
  .board {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
  }

  .legacy {
    flex: 0 0 auto;
    border-top: 1px solid var(--ax-border);
  }
  .legacy > summary {
    padding: var(--ax-space-2) var(--ax-space-5);
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
    cursor: pointer;
  }
</style>
