<!--
  Fixed layer for staged panels (core/staging.ts): each slides in from the
  bottom or the right, hosts a `stageable` module with a transient context,
  and closes via its × or Escape.
-->
<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { fly } from "svelte/transition";

  const SLIDE_MS = 560;

  import { getModule, createContext } from "../core/registry";
  import { closeStaged, staged, type StagedPanel } from "../core/staging";
  import type { ModuleContext } from "../core/types";

  // One context per panel for its whole lifetime — modules capture
  // `ctx.config` once at mount (same invariant as Tile.svelte). Re-creating
  // it on every template re-evaluation would detach the mounted module.
  const contexts = new Map<string, ModuleContext>();

  function contextFor(panel: StagedPanel): ModuleContext {
    let ctx = contexts.get(panel.id);
    if (!ctx) {
      ctx = createContext(panel.id, panel.config, (config) => {
        panel.config = config;
      });
      contexts.set(panel.id, ctx);
    }
    return ctx;
  }

  // Drop contexts of closed panels.
  $effect(() => {
    const live = new Set($staged.map((p) => p.id));
    for (const id of [...contexts.keys()]) {
      if (!live.has(id)) contexts.delete(id);
    }
  });

  function flyParams(panel: StagedPanel) {
    return panel.from === "bottom"
      ? { y: 600, duration: SLIDE_MS, easing: cubicOut }
      : { x: 600, duration: SLIDE_MS, easing: cubicOut };
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !e.defaultPrevented && $staged.length > 0) {
      // Consumed: later Escape handlers (chat, Second Brain) leave it alone.
      e.preventDefault();
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
    bottom: 64px;
    width: min(560px, 60vw);
    border-right: none;
    border-radius: var(--ax-radius-lg) 0 0 var(--ax-radius-lg);
  }
  .panel.bottom {
    left: 50%;
    bottom: 64px;
    transform: translateX(-50%);
    width: min(860px, calc(100vw - 2 * var(--ax-space-5)));
    height: min(70vh, 640px);
    border-radius: var(--ax-radius-lg);
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
