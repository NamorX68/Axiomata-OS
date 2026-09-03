<!--
  Hosts a `background` module instance full-size inside `#particle-slot`:
  no tile chrome, one context for its lifetime, a small corner button that
  pops the module's settings face, and × to remove it.
-->
<script lang="ts">
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
  .host {
    position: absolute;
    inset: 0;
  }
  .corner {
    position: absolute;
    left: var(--ax-space-3);
    bottom: var(--ax-space-3);
    display: flex;
    gap: var(--ax-space-1);
  }
  .corner button {
    width: 24px;
    height: 24px;
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
    bottom: 30px;
    min-width: 180px;
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-pop);
  }
</style>
