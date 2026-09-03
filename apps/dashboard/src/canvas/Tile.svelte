<!--
  One placed module instance: an absolutely positioned card bound to the
  instance's x/y/w/h/z. The whole card is the drag handle (form controls and
  anything `[data-no-drag]` excluded); a press anywhere brings it to the front.
  During a drag the card follows via `translate3d`; the new position is
  committed to the store on release. Resize and flip land in step 5.
-->
<script lang="ts">
  import { onDestroy } from "svelte";

  import { getModule, makeContext } from "../core/registry";
  import { bringToFront, removeInstance, updateInstance } from "../core/stores";
  import type { CanvasInstance } from "../core/types";
  import { draggable, type DragDelta } from "./drag";

  let { inst }: { inst: CanvasInstance } = $props();

  // `type` and `id` never change for a mounted instance, so capturing the
  // initial value is intended; the tile is keyed by id in Canvas.svelte.
  // svelte-ignore state_referenced_locally
  const def = getModule(inst.type);
  // One context per mounted instance — the module holds on to it.
  // svelte-ignore state_referenced_locally
  const ctx = makeContext(inst);

  let drag = $state<DragDelta | null>(null);

  function onDragStart() {
    drag = { dx: 0, dy: 0 };
  }

  function onDragMove(d: DragDelta) {
    drag = d;
  }

  function onDragEnd(d: DragDelta) {
    drag = null;
    updateInstance(inst.id, {
      x: Math.max(0, Math.round(inst.x + d.dx)),
      y: Math.max(0, Math.round(inst.y + d.dy)),
    });
  }

  onDestroy(() => {
    drag = null;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<article
  class="tile"
  class:dragging={drag !== null}
  data-instance={inst.id}
  style:left="{inst.x}px"
  style:top="{inst.y}px"
  style:width="{inst.w}px"
  style:height="{inst.h}px"
  style:z-index={drag ? "var(--ax-z-tile-drag)" : `calc(var(--ax-z-tile-base) + ${inst.z})`}
  style:transform={drag ? `translate3d(${drag.dx}px, ${drag.dy}px, 0)` : undefined}
  onpointerdowncapture={() => bringToFront(inst.id)}
  use:draggable={{ onStart: onDragStart, onMove: onDragMove, onEnd: onDragEnd }}
>
  <div class="tile-inner">
    <header class="tile-head">
      <span class="tile-icon" aria-hidden="true">{@html def?.icon ?? ""}</span>
      <h2 class="tile-title">{def?.title ?? inst.type}</h2>
      <button
        type="button"
        class="tile-close"
        title="Remove"
        aria-label="Remove module"
        onclick={() => removeInstance(inst.id)}
      >
        ×
      </button>
    </header>
    <div class="tile-body">
      {#if def}
        <def.component {ctx} />
      {:else}
        <p class="tile-unknown">Unknown module type “{inst.type}”.</p>
      {/if}
    </div>
  </div>
</article>

<style>
  .tile {
    position: absolute;
    touch-action: none;
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-tile);
    transition: box-shadow var(--ax-dur-fast) var(--ax-ease);
  }

  .tile.dragging {
    user-select: none;
    box-shadow: var(--ax-shadow-drag);
    border-color: var(--ax-border-strong);
    cursor: grabbing;
  }

  .tile-inner {
    display: flex;
    flex-direction: column;
    height: 100%;
    border-radius: inherit;
    overflow: hidden;
  }

  .tile-head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    cursor: grab;
    flex: 0 0 auto;
  }

  .tile-icon {
    display: inline-flex;
    width: 16px;
    height: 16px;
    color: var(--ax-accent);
  }
  .tile-icon :global(svg) {
    width: 100%;
    height: 100%;
  }

  .tile-title {
    flex: 1 1 auto;
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tile-close {
    width: 22px;
    height: 22px;
    padding: 0;
    line-height: 1;
    font-size: var(--ax-font-size-lg);
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
  }
  .tile-close:hover:not(:disabled) {
    color: var(--ax-text);
    background: var(--ax-surface-3);
  }

  .tile-body {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
  }

  .tile-unknown {
    margin: 0;
    padding: var(--ax-space-3);
    color: var(--ax-danger);
  }
</style>
