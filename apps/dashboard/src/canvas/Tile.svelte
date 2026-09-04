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
  import { get } from "svelte/store";

  import { getModule, makeContext } from "../core/registry";
  import { bringToFront, canvasSize, guides, instances, removeInstance, snapEdges, updateInstance } from "../core/stores";
  import type { CanvasInstance } from "../core/types";
  import { draggable, type DragDelta } from "./drag";
  import { resizable, type ResizeDelta, type ResizeDir } from "./resize";
  import { anchorFor, displayRect, magnetMove, magnetResize, resolveOverlap, type Rect } from "./snap";

  const FALLBACK_MIN = { w: 160, h: 100 };

  function bounds() {
    const b = get(canvasSize);
    return b.w > 0 && b.h > 0 ? b : undefined;
  }
  /** Displayed rect of any instance for the current canvas size. */
  function shown(i: CanvasInstance): Rect {
    const b = get(canvasSize);
    return displayRect({ x: i.x, y: i.y, w: i.w, h: i.h }, i.anchor, b, getModule(i.type)?.minSize ?? FALLBACK_MIN);
  }
  /** The other tiles on the canvas (no background modules, not this one). */
  function others(): Rect[] {
    return get(instances)
      .filter((i) => i.id !== inst.id && getModule(i.type)?.background !== true)
      .map(shown);
  }
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

  /** Snapped drag offset: the tile "sticks" to grid / neighbour edges live. */
  function snappedDrag(d: DragDelta): DragDelta {
    const base = shown(inst);
    const r = magnetMove({ x: base.x + d.dx, y: base.y + d.dy, w: base.w, h: base.h }, others(), {
      edges: get(snapEdges),
    });
    guides.set(r.guides);
    return { dx: r.x - base.x, dy: r.y - base.y };
  }
  function snappedResize(d: ResizeDelta, dir: ResizeDir): ResizeDelta {
    const base = shown(inst);
    const r = magnetResize({ x: base.x, y: base.y, w: base.w + d.dw, h: base.h + d.dh }, others(), dir, min, {
      edges: get(snapEdges),
    });
    guides.set(r.guides);
    return { dw: r.w - base.w, dh: r.h - base.h };
  }

  /** Persist a displayed rect as the new committed position + anchor. */
  function commit(rect: Rect) {
    const b = bounds();
    updateInstance(inst.id, {
      x: rect.x,
      y: rect.y,
      w: rect.w,
      h: rect.h,
      anchor: b ? anchorFor(rect, b) : undefined,
    });
  }
  // Once true the settings component stays mounted across flips.
  let backMounted = $state(false);

  /** Where this tile sits right now (anchor-shifted, clamped). */
  const disp = $derived(displayRect({ x: inst.x, y: inst.y, w: inst.w, h: inst.h }, inst.anchor, $canvasSize, min));
  const liveW = $derived(Math.max(min.w, disp.w + (resize?.dw ?? 0)));
  const liveH = $derived(Math.max(min.h, disp.h + (resize?.dh ?? 0)));

  function onDragEnd(d: DragDelta) {
    const base = shown(inst);
    const sd = snappedDrag(d);
    drag = null;
    guides.set([]);
    commit(resolveOverlap({ x: base.x + sd.dx, y: base.y + sd.dy, w: base.w, h: base.h }, others(), bounds()));
  }

  let resizeDir: ResizeDir = "se";
  function onResizeEnd() {
    const base = shown(inst);
    const sr = snappedResize(resize ?? { dw: 0, dh: 0 }, resizeDir);
    resize = null;
    guides.set([]);
    commit(resolveOverlap({ x: base.x, y: base.y, w: base.w + sr.dw, h: base.h + sr.dh }, others(), bounds()));
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
  style:left="{disp.x}px"
  style:top="{disp.y}px"
  style:width="{liveW}px"
  style:height="{liveH}px"
  style:z-index={drag ? "var(--ax-z-tile-drag)" : `calc(var(--ax-z-tile-base) + ${inst.z})`}
  style:transform={drag ? `translate3d(${drag.dx}px, ${drag.dy}px, 0)` : undefined}
  onpointerdowncapture={() => bringToFront(inst.id)}
  use:draggable={{
    handle: ".tile-drag",
    onStart: () => (drag = { dx: 0, dy: 0 }),
    onMove: (d) => (drag = snappedDrag(d)),
    onEnd: onDragEnd,
  }}
>
  <div class="tile-inner" class:flipped={inst.flipped}>
    <div class="face front">
      <header class="tile-head front-head tile-drag">
        <span class="tile-icon" aria-hidden="true">{@html def?.icon ?? ""}</span>
        <h2 class="tile-title">{def?.title ?? inst.type}</h2>
        <span class="tile-grip" aria-hidden="true">⠿</span>
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
      <header class="tile-head tile-drag">
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
        onStart: () => {
          resizeDir = dir;
          resize = { dw: 0, dh: 0 };
        },
        onMove: (d) => (resize = snappedResize(d, dir)),
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
    border-radius: var(--ax-radius-lg);
    transition:
      box-shadow var(--ax-dur-fast) var(--ax-ease),
      background var(--ax-dur-fast) var(--ax-ease);
  }
  /* Front: almost no chrome at rest — the module's content sits straight on
     the canvas, like the reference dashboard's edge panels. The only mark is
     a faint hairline along the top and bottom edge (a discreet delimiter, no
     side borders, no fill, no shadow). A faint fill appears on hover so you
     can see what you are about to grab / resize, and the tile "materialises"
     fully while it is being moved. */
  .face.front {
    background: transparent;
    box-shadow: none;
    border-radius: 0;
    border-top: 2px solid var(--ax-border-strong);
    border-bottom: 2px solid var(--ax-border-strong);
  }
  .tile:hover .face.front {
    background: color-mix(in srgb, var(--ax-tile-glass-bg) 45%, transparent);
  }
  .tile.dragging .face.front,
  .tile.resizing .face.front {
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    box-shadow: var(--ax-shadow-drag);
  }
  /* Back: the settings side keeps the solid framed-window look, so it reads
     as a distinct surface you are configuring. */
  .face.back {
    transform: rotateY(180deg);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    box-shadow: var(--ax-shadow-tile);
  }
  .tile.dragging .face.back {
    border-color: var(--ax-border-strong);
    box-shadow: var(--ax-shadow-drag);
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
  .tile.dragging .tile-head {
    cursor: grabbing;
  }

  /* The front header is just a section label + hover chrome — no bar, no
     divider, no background. It is the only drag handle now (`.tile-drag`);
     the body no longer initiates a drag. */
  .front-head {
    padding: var(--ax-space-2) var(--ax-space-2) var(--ax-space-1);
    border-bottom: none;
  }
  .front-head .tile-title {
    font-size: var(--ax-font-size-lg);
    color: var(--ax-text-muted);
  }
  .front-head .tile-btn {
    opacity: 0;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }
  .tile:hover .front-head .tile-btn,
  .front-head:focus-within .tile-btn {
    opacity: 1;
  }

  .tile-grip {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
    line-height: 1;
    opacity: 0;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }
  .tile:hover .tile-grip,
  .tile.dragging .tile-grip {
    opacity: 0.5;
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
