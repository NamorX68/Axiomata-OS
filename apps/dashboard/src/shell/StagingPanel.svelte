<!--
  One staged panel — split out of `StagingLayer.svelte` (owner request:
  several panels open at once, each independently movable/resizable) so
  every panel gets its own component instance and therefore its own local
  drag/resize/position state, the same reason `canvas/Tile.svelte` is one
  component per tile rather than one shared state object indexed by id. With
  `StagingLayer.svelte` previously rendering every panel from *one* shared
  `resizing`/`panelSize` pair, resizing one (back when at most one was ever
  open) would have live-resized *every* open panel at once now that more
  than one can be staged — this split is what avoids that, not just a
  refactor for its own sake.

  Slides in from the bottom, comes to rest centred (both axes) — unchanged
  from before this split, and always the same regardless of how many other
  panels are already open or where they've been dragged to (owner request:
  "neue Fenster verhalten sich beim öffnen wie bisher"). Resizable from all
  four edges, remembered across restarts via one *shared* setting
  (`stagingPanelSize` in dashboard.json) — deliberately still one value for
  every panel, not per-panel, matching the pre-split behaviour the owner
  asked to keep ("das soll so bleiben").

  Movable by dragging the header once shown (owner request), via the same
  `use:draggable` action `canvas/Tile.svelte` uses for tiles. Unlike the
  size, a panel's dragged-to position is deliberately *not* persisted or
  shared with other panels — `pos` below is local, transient state that
  resets to "centred" every time a panel is freshly opened, exactly the
  "new windows behave as before" requirement. Once dragged at least once,
  `.positioned` overrides the default centring CSS with the explicit
  `left`/`top` from `pos`; the `slide` transition itself checks `pos` so a
  panel dragged away from centre flies out from wherever it actually is on
  close, instead of snapping back to the centred transform first.

  Any pointer interaction with the panel (`pointerdowncapture`, same
  convention as `Tile.svelte`'s `bringToFront`) raises it to the front of
  the stack — see `core/staging.ts`'s `bringToFront` for how "front" is
  just array order, since every panel shares one fixed `z-index`.
-->
<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import type { TransitionConfig } from "svelte/transition";

  import { draggable, type DragDelta } from "../canvas/drag";
  import { resizable, type ResizeDelta } from "../canvas/resize";
  import { getSetting, setSetting } from "../core/persist";
  import { getModule } from "../core/registry";
  import { bringToFront, type StagedPanel } from "../core/staging";
  import type { ModuleContext } from "../core/types";

  let { panel, ctx, onClose }: { panel: StagedPanel; ctx: ModuleContext; onClose: () => void } = $props();

  // `$derived`, not a plain `const`: `panel` is a reactive prop (Svelte 5
  // warns on a `const` that only ever captures its initial value) — moot in
  // practice since `StagingLayer.svelte` keys each instance on `panel.id`
  // and never swaps a live instance's `panel` to a different one, but this
  // is the correct idiom regardless.
  const def = $derived(getModule(panel.type));

  const SLIDE_MS = 560;
  const MIN_W = 480;
  const MIN_H = 240;
  const SETTING_KEY = "stagingPanelSize";

  function clampW(w: number): number {
    return Math.min(Math.max(MIN_W, w), Math.round(window.innerWidth * 0.9));
  }
  function clampH(h: number): number {
    return Math.min(Math.max(MIN_H, h), Math.round(window.innerHeight * 0.9));
  }

  // Starting size: the shared last-used size (remembered across close/
  // reopen, one preference for every panel — see the component doc
  // comment), read once per panel at its own mount, or `null` (this
  // panel's CSS defaults: min(1000px, viewport−2·space-5) wide, capped at
  // 80vh tall) if nothing has been saved yet.
  const saved = getSetting<{ w: number; h: number }>(SETTING_KEY);
  let panelSize = $state<{ w: number; h: number } | null>(
    saved && typeof saved.w === "number" && typeof saved.h === "number" ? saved : null,
  );

  let panelEl = $state<HTMLElement | null>(null);
  let resizeBase = { w: 0, h: 0 };
  let resizing = $state<ResizeDelta | null>(null);

  const liveW = $derived(
    panelSize && !resizing ? panelSize.w : clampW((panelSize?.w ?? resizeBase.w) + (resizing?.dw ?? 0)),
  );
  const liveH = $derived(
    panelSize && !resizing ? panelSize.h : clampH((panelSize?.h ?? resizeBase.h) + (resizing?.dh ?? 0)),
  );

  function startResize() {
    resizing = { dw: 0, dh: 0 };
    const rect = panelEl?.getBoundingClientRect();
    resizeBase = rect ? { w: rect.width, h: rect.height } : (panelSize ?? { w: MIN_W, h: MIN_H });
  }

  function endResize(delta: ResizeDelta) {
    resizing = null;
    panelSize = {
      w: clampW(resizeBase.w + delta.dw),
      h: clampH(resizeBase.h + delta.dh),
    };
    setSetting(SETTING_KEY, panelSize);
  }

  // ---- move (drag by header) ----

  /** `null` until first dragged — see the component doc comment for why
   *  this is local/transient rather than persisted or shared. */
  let pos = $state<{ x: number; y: number } | null>(null);
  let moving = $state<DragDelta | null>(null);
  let moveBase = { x: 0, y: 0 };

  function startMove() {
    bringToFront(panel.id);
    moving = { dx: 0, dy: 0 };
    if (pos) {
      moveBase = pos;
      return;
    }
    // First-ever drag: the panel is still positioned by the centring CSS
    // (no inline left/top yet), so its current on-screen box is the base
    // to apply the drag delta to — from here on it's `.positioned`.
    const rect = panelEl?.getBoundingClientRect();
    moveBase = rect ? { x: rect.left, y: rect.top } : { x: 0, y: 0 };
  }

  function endMove(delta: DragDelta) {
    moving = null;
    pos = { x: moveBase.x + delta.dx, y: moveBase.y + delta.dy };
  }

  const liveX = $derived(pos ? pos.x + (moving?.dx ?? 0) : null);
  const liveY = $derived(pos ? pos.y + (moving?.dy ?? 0) : null);

  /** Slide-in from the bottom, settling centred — same shape regardless of
   *  how many other panels are open (see the component doc comment). A
   *  plain `fly` can't be used for the centred case because the panel is
   *  horizontally centred with `transform: translateX(-50%)`, and Svelte's
   *  `fly` *replaces* the element transform for the duration, which would
   *  knock it off-centre mid-animation — this keeps the `-50%` and adds the
   *  vertical offset instead. Once the panel has been dragged at least once
   *  (`pos` set), its position is real `left`/`top` pixels with no `-50%`
   *  transform at rest (`.positioned` below), so closing it plays the exit
   *  from wherever it actually is instead of snapping back to the centred
   *  transform first. */
  function slide(_node: Element): TransitionConfig {
    if (pos) {
      return { duration: SLIDE_MS, easing: cubicOut, css: (t, u) => `opacity: ${t}; transform: translateY(${u * 600}px)` };
    }
    return { duration: SLIDE_MS, easing: cubicOut, css: (t, u) => `opacity: ${t}; transform: translate(-50%, ${u * 600}px)` };
  }
</script>

<aside
  class="panel"
  class:resizing={resizing !== null}
  class:moving={moving !== null}
  class:positioned={pos !== null}
  data-staged={panel.id}
  transition:slide
  aria-label={def?.title ?? panel.type}
  bind:this={panelEl}
  onpointerdowncapture={() => bringToFront(panel.id)}
  style:width="{liveW}px"
  style:height="{liveH}px"
  style:left={pos ? `${liveX}px` : undefined}
  style:top={pos ? `${liveY}px` : undefined}
  use:draggable={{ handle: ".panel-head", onStart: startMove, onMove: (d) => (moving = d), onEnd: endMove }}
>
  <div class="resize-handle resize-w" use:resizable={{ dir: "w", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}></div>
  <div class="resize-handle resize-e" use:resizable={{ dir: "e", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}></div>
  <div class="resize-handle resize-n" use:resizable={{ dir: "n", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}></div>
  <div class="resize-handle resize-s" use:resizable={{ dir: "s", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}></div>
  <header class="panel-head">
    <span class="icon" aria-hidden="true">{@html def?.icon ?? ""}</span>
    <h2>{def?.title ?? panel.type}</h2>
    <button type="button" class="close" aria-label="Close panel" onclick={onClose}>×</button>
  </header>
  <div class="body">
    {#if def}
      <def.component {ctx} />
    {/if}
  </div>
</aside>

<style>
  .panel {
    position: fixed;
    z-index: var(--ax-z-staging);
    display: flex;
    flex-direction: column;
    /* Needed so a header drag doesn't lose the race to the browser's own
       native text-selection/touch-scroll gesture on press-and-move — the
       exact same reasoning as `canvas/Tile.svelte`'s `.tile`. `.body`
       below re-enables vertical scrolling for its own subtree with
       `pan-y`, the same way `.tile-body` does. */
    touch-action: none;
    /* Both `top` and `bottom` set + `margin: auto 0` is the fixed-position
       vertical-centring trick: with an explicit `height` smaller than the
       top/bottom band, the leftover space splits evenly above and below.
       Overridden entirely by `.positioned` below once the panel has been
       dragged at least once. */
    top: var(--ax-space-4);
    bottom: 64px;
    left: 50%;
    transform: translateX(-50%);
    margin-top: auto;
    margin-bottom: auto;
    height: min(80vh, calc(100vh - 64px - var(--ax-space-4)));
    width: min(1000px, calc(100vw - 2 * var(--ax-space-5)));
    border-radius: var(--ax-radius-lg);
    /* Tile-front "elevated" look (see canvas/Tile.svelte's
       .tile.dragging/.resizing .face.front), permanently on rather than
       only while interacting — this panel is never at rest against its own
       canvas background the way a tile is, so the fully transparent resting
       state that look is built on would be unreadable here. */
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border: none;
    border-bottom: 2px solid var(--ax-border-strong);
    box-shadow: var(--ax-shadow-drag);
  }
  .panel.resizing,
  .panel.moving {
    transition: none; /* no fighting the drag/resize with any future width/height/position transition */
    user-select: none; /* a fast drag can otherwise outrun the pointer and start selecting the panel's own text */
  }
  /* Dragged at least once: real `left`/`top` pixels (set inline, see
     `style:left`/`style:top` above) replace the centring trick entirely. */
  .panel.positioned {
    bottom: auto;
    transform: none;
    margin-top: 0;
    margin-bottom: 0;
  }

  /* Invisible resize handles on the panel's non-screen-edge sides. Thin
     strips just inside the border so they don't eat into the visible
     content area; a bit of hover feedback so they're discoverable. */
  .resize-handle {
    position: absolute;
    z-index: 1;
    background: transparent;
  }
  .resize-handle:hover,
  .panel.resizing .resize-handle {
    background: var(--ax-accent);
    opacity: 0.5;
  }
  .resize-w {
    top: 0;
    left: -3px;
    width: 6px;
    height: 100%;
    cursor: ew-resize;
  }
  .resize-e {
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    cursor: ew-resize;
  }
  .resize-n {
    top: -3px;
    left: 0;
    width: 100%;
    height: 6px;
    cursor: ns-resize;
  }
  .resize-s {
    bottom: -3px;
    left: 0;
    width: 100%;
    height: 6px;
    cursor: ns-resize;
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    flex: 0 0 auto;
    cursor: grab;
    /* Permanent, not just while `.moving` (unlike `.panel.resizing`/
       `.panel.moving`'s own `user-select: none` below): a native
       press-and-drag text selection starts on `mousedown` itself, before
       `use:draggable`'s own threshold-crossing `preventDefault()` ever
       gets a chance to run — a title bar's text should never be
       selectable in the first place anyway, the same convention any
       desktop window titlebar follows. */
    user-select: none;
  }
  .panel.moving header {
    cursor: grabbing;
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
  /* Matches canvas/Tile.svelte's .tile-btn exactly, for the same close/
     settings-button look everywhere in the app — including its front-head
     behaviour of staying hidden until the pointer is over the tile (or it
     has focus), not a permanently-visible button. */
  .close {
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
    opacity: 0;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
    cursor: pointer;
  }
  .panel:hover .close,
  .panel:focus-within .close {
    opacity: 1;
  }
  .close:hover:not(:disabled) {
    color: var(--ax-text);
    background: var(--ax-surface-3);
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    /* Restores vertical scroll/selection for the module content — `.panel`'s
       own `touch-action: none` (needed for header-drag reliability) would
       otherwise also block scrolling/selecting text anywhere in this
       subtree. */
    touch-action: pan-y;
  }
  .body > :global(*) {
    flex: 1 1 auto;
    min-height: 0;
  }
</style>
