<!--
  Fixed layer for staged panels (`core/staging.ts`) — a `stageable` module
  (currently only ever the Document/file viewer) opened as a floating panel
  instead of a canvas tile, how the chat and the agent hand the user a file
  to look at. More than one can be open at once (owner request); each
  panel's own look, slide-in transition, size/position state and resize/
  drag handling lives in `StagingPanel.svelte` — this layer only owns the
  list of *which* panels are staged, their module contexts, and global
  Escape handling (closes the topmost, i.e. last, entry first, so with
  several panels open one Escape press closes only one rather than
  cascading through all of them in the same keystroke).
-->
<script lang="ts">
  import { createContext } from "../core/registry";
  import { closeStaged, staged, type StagedPanel } from "../core/staging";
  import type { ModuleContext } from "../core/types";
  import StagingPanel from "./StagingPanel.svelte";

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
  <StagingPanel {panel} ctx={contextFor(panel)} onClose={() => closeStaged(panel.id)} />
{/each}
