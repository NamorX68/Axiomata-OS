<!--
  One placed module instance: an absolutely positioned flip card bound to the
  instance's x/y/w/h/z/flipped.

  - Drag: the whole card is the handle (form controls, `[data-no-drag]` and
    the resize handles excluded); a press anywhere brings it to the front.
    During a drag the card follows via `translate3d`; the position is
    committed on release.
  - Resize: `e` / `s` / `se` handles; live size while dragging, clamped to
    the module's `minSize` and committed on release.
  - Flip: `.tile-inner` rotates 180° on Y (CSS 3-D). The back face mounts
    `def.settings` lazily on the first flip and keeps it mounted after.
-->
<script lang="ts">
  import { getModule, makeContext } from "../core/registry";
  import { bringToFront, removeInstance, updateInstance } from "../core/stores";
  import type { CanvasInstance } from "../core/types";
  import { draggable, type DragDelta } from "./drag";
  import { resizable, type ResizeDelta, type ResizeDir } from "./resize";

  const FALLBACK_MIN = { w: 160, h: 100 };
  /** Drag / resize commit to this grid (matches `--ax-grid`). */
  const GRID = 16;
  const snap = (v: number) => Math.round(v / GRID) * GRID;
  const HANDLES: ResizeDir[] = ["e", "s", "se"];

  let { inst }: { inst: CanvasInstance } = $props();

  // `type` and `id` never change for a mounted instance, so capturing the
  // initial value is intended; the tile is keyed by id in Canvas.svelte.
  // svelte-ignore state_referenced_locally
  const def = getModule(inst.type);
  // One context per mounted instance — the module holds on to it.
  // svelte-ignore state_referenced_locally
  const ctx = makeContext(inst);
  const min = def?.minSize ?? FALLBACK_MIN;

  let drag = $state<DragDelta | null>(null);
  let resize = $state<ResizeDelta | null>(null);
  // Once true the settings component stays mounted across flips.
  let backMounted = $state(false);

  const liveW = $derived(Math.max(min.w, inst.w + (resize?.dw ?? 0)));
  const liveH = $derived(Math.max(min.h, inst.h + (resize?.dh ?? 0)));

  function onDragEnd(d: DragDelta) {
    drag = null;
    updateInstance(inst.id, {
      x: Math.max(0, snap(inst.x + d.dx)),
      y: Math.max(0, snap(inst.y + d.dy)),
    });
  }

  function onResizeEnd() {
    const w = Math.max(min.w, snap(liveW));
    const h = Math.max(min.h, snap(liveH));
    resize = null;
    updateInstance(inst.id, { w, h });
  }

  function flip() {
    if (!inst.flipped) backMounted = true;
    updateInstance(inst.id, { flipped: !inst.flipped });
  }

  $effect(() => {
    if (inst.flipped) backMounted = true;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<article
  class="tile"
  class:dragging={drag !== null}
  class:resizing={resize !== null}
  data-instance={inst.id}
  style:left="{inst.x}px"
  style:top="{inst.y}px"
  style:width="{liveW}px"
  style:height="{liveH}px"
  style:z-index={drag ? "var(--ax-z-tile-drag)" : `calc(var(--ax-z-tile-base) + ${inst.z})`}
  style:transform={drag ? `translate3d(${drag.dx}px, ${drag.dy}px, 0)` : undefined}
  onpointerdowncapture={() => bringToFront(inst.id)}
  use:draggable={{
    onStart: () => (drag = { dx: 0, dy: 0 }),
    onMove: (d) => (drag = d),
    onEnd: onDragEnd,
  }}
>
  <div class="tile-inner" class:flipped={inst.flipped}>
    <div class="face front">
      <header class="tile-head">
        <span class="tile-icon" aria-hidden="true">{@html def?.icon ?? ""}</span>
        <h2 class="tile-title">{def?.title ?? inst.type}</h2>
        {#if def?.settings}
          <button
            type="button"
            class="tile-btn"
            title="Settings"
            aria-label="Flip to settings"
            onclick={flip}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M2.5 8a5.5 5.5 0 019.4-3.9M13.5 8a5.5 5.5 0 01-9.4 3.9M11.5 1.5v3h-3M4.5 14.5v-3h3"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>
        {/if}
        <button
          type="button"
          class="tile-btn"
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

    <div class="face back" aria-hidden={!inst.flipped}>
      <header class="tile-head">
        <h2 class="tile-title">{def?.title ?? inst.type} · Settings</h2>
        <button
          type="button"
          class="tile-btn"
          title="Back"
          aria-label="Flip back"
          onclick={flip}
        >
          ×
        </button>
      </header>
      <div class="tile-body">
        {#if backMounted && def?.settings}
          <def.settings {ctx} />
        {/if}
      </div>
    </div>
  </div>

  {#each HANDLES as dir (dir)}
    <div
      class="resize resize-{dir}"
      use:resizable={{
        dir,
        onStart: () => (resize = { dw: 0, dh: 0 }),
        onMove: (d) => (resize = d),
        onEnd: onResizeEnd,
      }}
    ></div>
  {/each}
</article>

<style>
  .tile {
    position: absolute;
    touch-action: none;
    perspective: 1200px;
  }

  .tile.dragging,
  .tile.resizing {
    user-select: none;
  }
  .tile.dragging {
    cursor: grabbing;
  }

  .tile-inner {
    position: relative;
    width: 100%;
    height: 100%;
    transform-style: preserve-3d;
    transition: transform var(--ax-dur-med) var(--ax-ease);
  }
  .tile-inner.flipped {
    transform: rotateY(180deg);
  }

  .face {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    backface-visibility: hidden;
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-tile);
    transition: box-shadow var(--ax-dur-fast) var(--ax-ease);
  }
  .face.back {
    transform: rotateY(180deg);
    background: var(--ax-surface-2);
  }
  .tile.dragging .face {
    box-shadow: var(--ax-shadow-drag);
    border-color: var(--ax-border-strong);
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

  .tile-btn {
    width: 22px;
    height: 22px;
    padding: 0;
    display: grid;
    place-items: center;
    line-height: 1;
    font-size: var(--ax-font-size-lg);
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
  }
  .tile-btn:hover:not(:disabled) {
    color: var(--ax-text);
    background: var(--ax-surface-3);
  }
  .tile-btn svg {
    width: 14px;
    height: 14px;
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

  /* ---- resize handles (outside the 3-D inner so they never flip) ---- */
  .resize {
    position: absolute;
    z-index: 1;
  }
  .resize-e {
    top: var(--ax-space-3);
    bottom: var(--ax-space-3);
    right: -3px;
    width: 7px;
    cursor: ew-resize;
  }
  .resize-s {
    left: var(--ax-space-3);
    right: var(--ax-space-3);
    bottom: -3px;
    height: 7px;
    cursor: ns-resize;
  }
  .resize-se {
    right: -2px;
    bottom: -2px;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    border-right: 2px solid var(--ax-border-strong);
    border-bottom: 2px solid var(--ax-border-strong);
    border-bottom-right-radius: var(--ax-radius-lg);
    opacity: 0;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }
  .tile:hover .resize-se,
  .tile.resizing .resize-se {
    opacity: 1;
  }
</style>
