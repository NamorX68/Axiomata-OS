<!--
  Fixed layer for the staged panel (core/staging.ts, at most one at a time):
  slides in from the bottom (`slide` transition below — a plain `fly` fights
  the panel's `translateX(-50%)` centring), hosts a `stageable` module with
  a transient context, and closes via its × or Escape.

  The panel — currently only ever the Document/file viewer — flies in from
  below but comes to rest **centred** on screen (both axes), the same
  floating-in-the-middle resting spot the old right-hand panel used — only
  the entry direction changed, not where it settles. It's resizable from all
  four edges; the last size is remembered across restarts
  (`settings.stagingPanelSize` in dashboard.json, via core/persist's generic
  getSetting/setSetting) and re-checked every time a panel is staged, not
  just once at mount — `core/persist`'s `initPersistence()` is asynchronous,
  so a check that only ran once at this component's own mount could easily
  run *before* the saved size has actually loaded, on a cold start. Before
  the first resize, it uses its CSS defaults (min(1000px, viewport−2·space-5)
  wide, capped at 80vh tall).

  Visually styled after a module tile's own frameless front face
  (`canvas/Tile.svelte`'s `.face.front`), not a classic opaque dialog window:
  the glass fill + blur Tile only shows while a tile is being dragged/
  resized is on here permanently (this panel floats above arbitrary other
  content the whole time it's open, unlike a tile normally resting flush on
  its own canvas background — a fully transparent resting state, the tile's
  own default, would be unreadable here), a hairline only along the bottom
  edge instead of a full border, and no shadow beyond the tile's own
  "elevated" one (`--ax-shadow-drag`) rather than the heavier
  `--ax-shadow-pop` a dialog would use. The close button also borrows the
  tile front-head's own behaviour: hidden until the panel is hovered/
  focused, not a permanently-visible button.
-->
<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import type { TransitionConfig } from "svelte/transition";

  const SLIDE_MS = 560;

  /** Slide-in from the bottom. A plain `fly` can't be used because the panel
   *  is horizontally centred with `transform: translateX(-50%)`, and
   *  Svelte's `fly` *replaces* the element transform for the duration —
   *  which would knock the panel off-centre mid-animation. This keeps the
   *  `-50%` and adds the vertical offset. */
  function slide(_node: Element): TransitionConfig {
    return {
      duration: SLIDE_MS,
      easing: cubicOut,
      css: (t, u) => `opacity: ${t}; transform: translate(-50%, ${u * 600}px)`,
    };
  }

  import { resizable, type ResizeDelta } from "../canvas/resize";
  import { getModule, createContext } from "../core/registry";
  import { getSetting, setSetting } from "../core/persist";
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

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !e.defaultPrevented && $staged.length > 0) {
      // Consumed: later Escape handlers (chat, Second Brain) leave it alone.
      e.preventDefault();
      closeStaged($staged[$staged.length - 1].id);
    }
  }

  // ---- resize (all four edges) ----

  const MIN_W = 480;
  const MIN_H = 240;
  const SETTING_KEY = "stagingPanelSize";

  /** `null` = no custom size yet; the panel uses its CSS defaults. Re-read
   *  from dashboard.json every time a panel gets staged (this effect's only
   *  reactive dependency is `$staged`) rather than once at mount — this
   *  component mounts as part of the app shell, well before
   *  `initPersistence()`'s async load is guaranteed to have finished, so a
   *  mount-time-only read could silently miss the saved size on a cold
   *  start and never check again. By the time the owner actually opens a
   *  file, persistence has long since loaded. This is one shared
   *  preference, not per-file. */
  let panelSize = $state<{ w: number; h: number } | null>(null);
  $effect(() => {
    if ($staged.length === 0) return;
    const saved = getSetting<{ w: number; h: number }>(SETTING_KEY);
    if (saved && typeof saved.w === "number" && typeof saved.h === "number") {
      panelSize = saved;
    }
  });

  let panelEl = $state<HTMLElement | null>(null);
  let dragBase = { w: 0, h: 0 };
  let resizing = $state<ResizeDelta | null>(null);

  function clampW(w: number): number {
    return Math.min(Math.max(MIN_W, w), Math.round(window.innerWidth * 0.9));
  }
  function clampH(h: number): number {
    return Math.min(Math.max(MIN_H, h), Math.round(window.innerHeight * 0.9));
  }

  const liveW = $derived(panelSize && !resizing ? panelSize.w : clampW((panelSize?.w ?? dragBase.w) + (resizing?.dw ?? 0)));
  const liveH = $derived(panelSize && !resizing ? panelSize.h : clampH((panelSize?.h ?? dragBase.h) + (resizing?.dh ?? 0)));

  // Same start logic regardless of which edge (w/n/s/e) is being dragged —
  // `resize.ts` already encodes the sign correctly per direction, so the
  // base size to add the live delta to doesn't depend on which one it was.
  function startResize() {
    resizing = { dw: 0, dh: 0 };
    const rect = panelEl?.getBoundingClientRect();
    dragBase = rect ? { w: rect.width, h: rect.height } : (panelSize ?? { w: MIN_W, h: MIN_H });
  }

  function endResize(delta: ResizeDelta) {
    resizing = null;
    panelSize = {
      w: clampW(dragBase.w + delta.dw),
      h: clampH(dragBase.h + delta.dh),
    };
    setSetting(SETTING_KEY, panelSize);
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#each $staged as panel (panel.id)}
  {@const def = getModule(panel.type)}
  <aside
    class="panel"
    class:resizing={resizing !== null}
    data-staged={panel.id}
    transition:slide
    aria-label={def?.title ?? panel.type}
    bind:this={panelEl}
    style={panelSize || resizing ? `width: ${liveW}px; height: ${liveH}px;` : undefined}
  >
    <div class="resize-handle resize-w" use:resizable={{ dir: "w", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}></div>
    <div class="resize-handle resize-e" use:resizable={{ dir: "e", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}></div>
    <div class="resize-handle resize-n" use:resizable={{ dir: "n", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}></div>
    <div class="resize-handle resize-s" use:resizable={{ dir: "s", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}></div>
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
    /* Both `top` and `bottom` set + `margin: auto 0` is the fixed-position
       vertical-centring trick: with an explicit `height` smaller than the
       top/bottom band, the leftover space splits evenly above and below —
       the exact technique the old right-hand panel used, so the panel
       settles in the same spot regardless of which edge it now flies in
       from. Once resized, the inline `width`/`height` (`style` above) win
       and still centre the same way. */
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
  .panel.resizing {
    transition: none; /* no fighting the drag with the (nonexistent, but
                          future-proofing) width/height transition */
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
  }
  .body > :global(*) {
    flex: 1 1 auto;
    min-height: 0;
  }
</style>
