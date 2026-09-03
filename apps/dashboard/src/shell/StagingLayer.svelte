<!--
  Fixed layer for staged panels (core/staging.ts): each slides in from the
  bottom or the right, hosts a `stageable` module with a transient context,
  and closes via its × or Escape.
-->
<script lang="ts">
  import { fly } from "svelte/transition";

  import { getModule, createContext } from "../core/registry";
  import { closeStaged, staged, type StagedPanel } from "../core/staging";

  function contextFor(panel: StagedPanel) {
    return createContext(panel.id, panel.config, (config) => {
      panel.config = config;
    });
  }

  function flyParams(panel: StagedPanel) {
    return panel.from === "bottom" ? { y: 600, duration: 260 } : { x: 600, duration: 260 };
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && $staged.length > 0) {
      closeStaged($staged[$staged.length - 1].id);
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#each $staged as panel (panel.id)}
  {@const def = getModule(panel.type)}
  <aside
    class="panel {panel.from}"
    data-staged={panel.id}
    transition:fly={flyParams(panel)}
    aria-label={def?.title ?? panel.type}
  >
    <header>
      <span class="icon" aria-hidden="true">{@html def?.icon ?? ""}</span>
      <h2>{def?.title ?? panel.type}</h2>
      <button type="button" class="close" aria-label="Close panel" onclick={() => closeStaged(panel.id)}>×</button>
    </header>
    <div class="body">
      {#if def}
        <def.component ctx={contextFor(panel)} />
      {/if}
    </div>
  </aside>
{/each}

<style>
  .panel {
    position: fixed;
    z-index: var(--ax-z-staging);
    display: flex;
    flex-direction: column;
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    box-shadow: var(--ax-shadow-pop);
  }
  .panel.right {
    top: var(--ax-space-4);
    right: 0;
    bottom: var(--ax-space-4);
    width: min(560px, 60vw);
    border-right: none;
    border-radius: var(--ax-radius-lg) 0 0 var(--ax-radius-lg);
  }
  .panel.bottom {
    left: 50%;
    bottom: 0;
    transform: translateX(-50%);
    width: min(860px, calc(100vw - 2 * var(--ax-space-5)));
    height: min(70vh, 640px);
    border-bottom: none;
    border-radius: var(--ax-radius-lg) var(--ax-radius-lg) 0 0;
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    flex: 0 0 auto;
  }
  .icon {
    display: inline-flex;
    width: 16px;
    height: 16px;
    color: var(--ax-accent);
  }
  .icon :global(svg) {
    width: 100%;
    height: 100%;
  }
  h2 {
    flex: 1 1 auto;
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }
  .close {
    width: 22px;
    height: 22px;
    padding: 0;
    line-height: 1;
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .body > :global(*) {
    flex: 1 1 auto;
    min-height: 0;
  }
</style>
