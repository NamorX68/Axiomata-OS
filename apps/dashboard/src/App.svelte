<!--
  Shell composition: TopBar → IconBar → Canvas → (assistant bar, step 11).
  Until the canvas modules replace them, the pre-M5 panels sit collapsed
  below the canvas so the app stays useful.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { get } from "svelte/store";

  import Canvas from "./canvas/Canvas.svelte";
  import { on } from "./core/bus";
  import { invokeAction, manifest } from "./core/registry";
  import { addInstance, instances } from "./core/stores";
  import IconBar from "./shell/IconBar.svelte";
  import LegacyPanels from "./shell/LegacyPanels.svelte";
  import Toasts from "./shell/Toasts.svelte";
  import TopBar from "./shell/TopBar.svelte";

  // Step-2 scaffolding: prove the registry/store/context pipeline end to end.
  // Removed once the module picker (step 7) exists.
  async function devProbe() {
    const offset = 40 + get(instances).length * 32;
    const inst = addInstance({
      type: "dummy",
      x: offset,
      y: offset,
      w: 260,
      h: 160,
      config: {},
    });
    console.log("registry.manifest() →", manifest());
    console.log("invokeAction(ping) →", await invokeAction(inst.id, "ping", {}));
  }

  // Stub handlers for the icon-bar events until their dialogs exist
  // (settings: step 13, add-module: step 7, search: step 11).
  onMount(() => {
    const offs = ["shell:settings", "shell:add-module", "shell:search"].map((ev) =>
      on(ev, () => console.info(`${ev} — not wired yet`)),
    );
    return () => offs.forEach((off) => off());
  });
</script>

<TopBar />
<IconBar />
<Toasts />

{#if import.meta.env.DEV}
  <div class="dev-tools">
    <button type="button" onclick={devProbe}>dev: add dummy + log manifest</button>
  </div>
{/if}

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

  .dev-tools {
    display: flex;
    justify-content: center;
    margin-bottom: var(--ax-space-2);
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
