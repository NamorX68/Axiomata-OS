<!--
  Shell composition: TopBar → IconBar → canvas area → (assistant bar, step 11).
  The canvas area hosts `Canvas.svelte` from step 4; until the canvas modules
  replace them, the pre-M5 panels render inside it so the app stays useful.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { on } from "./core/bus";
  import { invokeAction, manifest } from "./core/registry";
  import { addInstance } from "./core/stores";
  import IconBar from "./shell/IconBar.svelte";
  import LegacyPanels from "./shell/LegacyPanels.svelte";
  import TopBar from "./shell/TopBar.svelte";

  // Step-2 scaffolding: prove the registry/store/context pipeline end to end.
  // Removed once the module picker (step 7) exists.
  async function devProbe() {
    const inst = addInstance({
      type: "dummy",
      x: 40,
      y: 40,
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

<div class="canvas-area">
  <div id="particle-slot" aria-hidden="true"></div>
  {#if import.meta.env.DEV}
    <div class="dev-tools">
      <button type="button" onclick={devProbe}>dev: add dummy + log manifest</button>
    </div>
  {/if}
  <LegacyPanels />
</div>

<style>
  .canvas-area {
    position: relative;
    flex: 1 1 auto;
    overflow: auto;
  }

  #particle-slot {
    position: absolute;
    inset: 0;
    z-index: var(--ax-z-particle);
    pointer-events: none;
  }

  .dev-tools {
    display: flex;
    justify-content: center;
    margin-bottom: var(--ax-space-2);
  }
</style>
