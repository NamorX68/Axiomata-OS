<!--
  Hosts a `background` module instance full-size inside `#particle-slot`:
  no tile chrome, one context for its lifetime, a small corner button that
  pops the module's settings face, and × to remove it.
-->
<script lang="ts">
  import { brainView } from "../core/brainView";
  import { getModule, makeContext } from "../core/registry";
  import { removeInstance } from "../core/stores";
  import type { CanvasInstance } from "../core/types";

  let { inst }: { inst: CanvasInstance } = $props();
  // `type` / `id` never change for a mounted instance (see Tile.svelte).
  // svelte-ignore state_referenced_locally
  const def = getModule(inst.type);
  // svelte-ignore state_referenced_locally
  const ctx = makeContext(inst);

  let settingsOpen = $state(false);
</script>

<div class="host" data-background={inst.id}>
  {#if def}
    <def.component {ctx} />
  {/if}
  <div class="corner" data-no-drag>
    <button
      type="button"
      aria-label="Bewegung"
      aria-pressed={$brainView.motion}
      title={$brainView.motion ? "Bewegung anhalten" : "Bewegung starten"}
      onclick={() => brainView.update((v) => ({ ...v, motion: !v.motion }))}>{$brainView.motion ? "⏸" : "▶"}</button
    >
    {#if def?.settings}
      <button type="button" aria-label="Background settings" title={def.title} onclick={() => (settingsOpen = !settingsOpen)}>⚙</button>
    {/if}
    <button type="button" aria-label="Remove background" title="Remove" onclick={() => removeInstance(inst.id)}>×</button>
    {#if settingsOpen && def?.settings}
      <div class="popover">
        <def.settings {ctx} />
      </div>
    {/if}
  </div>
</div>

<style>
  /* A size container: what is hosted here (the Orbit's cloud, the Second Brain's disc) is sized in `cqmin` of this one box. */
  .host {
    position: absolute;
    inset: 0;
    container-type: size;
  }
  .corner {
    position: absolute;
    left: var(--ax-space-3);
    bottom: var(--ax-space-3);
    display: flex;
    gap: var(--ax-space-1);
  }
  .corner button {
    width: calc(24px * var(--ax-ui-scale));
    height: calc(24px * var(--ax-ui-scale));
    padding: 0;
    line-height: 1;
    font-size: var(--ax-font-size-sm);
    background: var(--ax-surface-2);
    color: var(--ax-text-muted);
    opacity: 0.6;
  }
  .corner button:hover {
    opacity: 1;
    color: var(--ax-text);
  }
  .popover {
    position: absolute;
    left: 0;
    bottom: calc(30px * var(--ax-ui-scale));
    min-width: calc(180px * var(--ax-ui-scale));
    /* Same glass/hairline/elevated-shadow language as Window.svelte and
       every other panel in the app. */
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border: none;
    border-bottom: 2px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-drag);
  }
</style>
