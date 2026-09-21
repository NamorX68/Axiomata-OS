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

  Slides in from the bottom and comes to rest centred (both axes), unless
  its opener passed an `anchor` — then it settles centred on whatever opened
  it, which is what a Kanban card does so the detail appears over the tile
  the eye is already on. Without an anchor the original behaviour stands
  (owner request: "neue Fenster verhalten sich beim öffnen wie bisher").

  Resizable from all four edges and remembered across restarts. The size used
  to be one shared value for every panel, which the owner asked to keep while
  the file viewer was the only staged module. That stopped working when a
  Kanban board became openable as a panel: it inherited whatever size was last
  used for a three-line card and opened unusably small. The size is now
  remembered **per module type** (`stagingPanelSizes`), falling back to the
  old shared value and then to the module's own declared default, so nothing
  already saved is lost and a type nobody has resized yet still opens at a
  size its author thought sensible.

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
  import {
    bringToFront,
    readAnchor,
    readPanelSize,
    readSizeKey,
    type PanelSize,
    type StagedPanel,
  } from "../core/staging";
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
  /** Legacy single-size setting, still read so an existing one survives. */
  const SETTING_KEY = "stagingPanelSize";
  const SETTING_KEY_BY_TYPE = "stagingPanelSizes";

  function clampW(w: number): number {
    return Math.min(Math.max(MIN_W, w), Math.round(window.innerWidth * 0.9));
  }
  function clampH(h: number): number {
    return Math.min(Math.max(MIN_H, h), Math.round(window.innerHeight * 0.9));
  }

  /**
   * Remembered size, per module type.
   *
   * It used to be one size for every panel, which was fine while the file
   * viewer was the only one. It stopped being fine the moment a Kanban board
   * could be opened as a panel: a board inherited the size last used for a
   * three-line card and opened unusably small. A document and a board simply
   * do not want the same window.
   *
   * Falls back to what the opener asked for (`config.panelSize`, the typed
   * contract in `core/staging.ts`), then to the older shared value so a size
   * already saved is not thrown away, and then to the module's own tile
   * default — a module that states a reasonable size for itself is the best
   * guess available.
   */
  function savedSize(): PanelSize | null {
    const perType = getSetting<Record<string, unknown>>(SETTING_KEY_BY_TYPE);
    const mine = readPanelSize(perType?.[sizeKey()]);
    if (mine) return mine;

    // An opener that states its own starting size gets it.
    const asked = readPanelSize(panel.config.panelSize);
    if (asked) return { w: clampW(asked.w), h: clampH(asked.h) };

    const shared = readPanelSize(getSetting<unknown>(SETTING_KEY));
    if (shared) return shared;

    const fallback = readPanelSize(getModule(panel.type)?.defaultSize);
    return fallback ? { w: clampW(fallback.w), h: clampH(fallback.h) } : null;
  }

  function sizeKey(): string {
    return readSizeKey(panel.config, panel.type);
  }

  let panelSize = $state<PanelSize | null>(savedSize());

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
    const byType = { ...(getSetting<Record<string, unknown>>(SETTING_KEY_BY_TYPE) ?? {}) };
    byType[sizeKey()] = panelSize;
    setSetting(SETTING_KEY_BY_TYPE, byType);
  }

  // ---- move (drag by header) ----

  /**
   * Where the panel should first appear, when its opener said so.
   *
   * An anchored panel starts centred on whatever opened it (its tile or
   * panel) instead of in the middle of the screen. The size is already known
   * here, so this is computed up front rather than measured after mount —
   * that keeps `pos` set before the entrance transition reads it, so the
   * panel animates in at its final spot instead of visibly jumping there.
   */
  function anchoredPos(): { x: number; y: number } | null {
    const anchor = readAnchor(panel.config.anchor);
    if (!anchor) return null;
    const w = clampW(panelSize?.w ?? 0);
    const h = clampH(panelSize?.h ?? 0);
    const inset = 8;
    const fit = (value: number, size: number, limit: number) =>
      Math.round(Math.min(Math.max(value, inset), Math.max(inset, limit - size - inset)));
    return {
      x: fit(anchor.x + anchor.w / 2 - w / 2, w, window.innerWidth),
      y: fit(anchor.y + anchor.h / 2 - h / 2, h, window.innerHeight),
    };
  }

  /** `null` until first dragged, unless the opener anchored it — see the
   *  component doc comment for why this is local/transient rather than
   *  persisted or shared. */
  let pos = $state<{ x: number; y: number } | null>(anchoredPos());
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
