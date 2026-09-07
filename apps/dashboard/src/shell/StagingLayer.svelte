<!--
  Fixed layer for staged panels (core/staging.ts): each slides in from the
  bottom or the right, hosts a `stageable` module with a transient context,
  and closes via its × or Escape.

  The right-side panel (the file/Document viewer) is resizable from its
  left/top/bottom edges — not the right, which is the screen edge — since
  it's the one people actually want bigger for a long note or a wide table.
  The last size is remembered across restarts (`settings.stagingPanelSize`
  in dashboard.json, via core/persist's generic getSetting/setSetting) and
  applied to whatever gets staged there next; before the first resize ever
  happens, the panel still uses its CSS defaults (35vw wide, vertically
  centred, capped at 80vh) exactly as before.
-->
<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { fly } from "svelte/transition";

  const SLIDE_MS = 560;

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

  // ---- right-panel resize (left/top/bottom edges only) ----

  const MIN_W = 480;
  const MIN_H = 240;
  const SETTING_KEY = "stagingPanelSize";

  /** `null` = no custom size yet; the panel uses its CSS defaults. Loaded
   *  once from dashboard.json and applied to whichever panel is staged on
   *  the right — this is one shared preference, not per-file. */
  let rightSize = $state<{ w: number; h: number } | null>(null);
  $effect(() => {
    const saved = getSetting<{ w: number; h: number }>(SETTING_KEY);
    if (saved && typeof saved.w === "number" && typeof saved.h === "number") {
      rightSize = saved;
    }
  });

  let rightPanelEl: HTMLElement | null = null;
  let dragBase = { w: 0, h: 0 };
  let resizing = $state<ResizeDelta | null>(null);

  /** `bind:this` can't target a conditional expression, and only the
   *  right-side panel (at most one at a time, per staging.ts) needs its
   *  element captured — a tiny action instead of splitting the template. */
  function captureIfRight(node: HTMLElement, isRight: boolean) {
    if (isRight) rightPanelEl = node;
    return {
      destroy() {
        if (rightPanelEl === node) rightPanelEl = null;
      },
    };
  }

  function clampW(w: number): number {
    return Math.min(Math.max(MIN_W, w), Math.round(window.innerWidth * 0.9));
  }
  function clampH(h: number): number {
    return Math.min(Math.max(MIN_H, h), Math.round(window.innerHeight * 0.9));
  }

  const liveW = $derived(rightSize && !resizing ? rightSize.w : clampW((rightSize?.w ?? dragBase.w) + (resizing?.dw ?? 0)));
  const liveH = $derived(rightSize && !resizing ? rightSize.h : clampH((rightSize?.h ?? dragBase.h) + (resizing?.dh ?? 0)));

  // Same start logic regardless of which edge (w/n/s) is being dragged —
  // `resize.ts` already encodes the sign correctly per direction, so the
  // base size to add the live delta to doesn't depend on which one it was.
  function startResize() {
    resizing = { dw: 0, dh: 0 };
    const rect = rightPanelEl?.getBoundingClientRect();
    dragBase = rect ? { w: rect.width, h: rect.height } : (rightSize ?? { w: MIN_W, h: MIN_H });
  }

  function endResize(delta: ResizeDelta) {
    resizing = null;
    rightSize = {
      w: clampW(dragBase.w + delta.dw),
      h: clampH(dragBase.h + delta.dh),
    };
    setSetting(SETTING_KEY, rightSize);
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#each $staged as panel (panel.id)}
  {@const def = getModule(panel.type)}
  <aside
    class="panel {panel.from}"
    class:resizing={panel.from === "right" && resizing !== null}
    data-staged={panel.id}
    transition:fly={flyParams(panel)}
    aria-label={def?.title ?? panel.type}
    use:captureIfRight={panel.from === "right"}
    style={panel.from === "right" && (rightSize || resizing) ? `width: ${liveW}px; height: ${liveH}px;` : undefined}
  >
    {#if panel.from === "right"}
      <div
        class="resize-handle resize-w"
        use:resizable={{ dir: "w", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}
      ></div>
      <div
        class="resize-handle resize-n"
        use:resizable={{ dir: "n", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}
      ></div>
      <div
        class="resize-handle resize-s"
        use:resizable={{ dir: "s", onStart: startResize, onMove: (d) => (resizing = d), onEnd: endResize }}
      ></div>
    {/if}
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
    /* Vertically centred within the space between the top gap and the
       assistant bar, capped at 80% of the window height. `top` and
       `bottom` both set (rather than anchoring to one) turns this into
       the fixed-position centring trick: with an explicit `height`
       smaller than the top/bottom band, `margin: auto 0` splits the
       leftover space evenly above and below instead of stacking it all
       at the top. Once a custom size is set (inline `style` above), that
       explicit height still centres the same way — only the *number*
       changed, not the mechanism. */
    top: var(--ax-space-4);
    bottom: 64px;
    right: 0;
    height: min(80vh, calc(100vh - 64px - var(--ax-space-4)));
    margin-top: auto;
    margin-bottom: auto;
    /* 35% of the window by default, with a floor so it stays usable when
       the window itself is narrow — no ceiling: on an ultra-wide monitor
       35% is genuinely wider than 900px, and that's the point. Overridden
       by an inline `width` once the owner has resized it once. */
    width: max(480px, 35vw);
    border-right: none;
    border-radius: var(--ax-radius-lg) 0 0 var(--ax-radius-lg);
  }
  .panel.right.resizing {
    transition: none; /* no fighting the drag with the (nonexistent, but
                          future-proofing) width/height transition */
  }
  .panel.bottom {
    left: 50%;
    bottom: 64px;
    transform: translateX(-50%);
    width: min(860px, calc(100vw - 2 * var(--ax-space-5)));
    height: min(70vh, 640px);
    border-radius: var(--ax-radius-lg);
  }

  /* Invisible resize handles on the right panel's non-screen-edge sides.
     Thin strips just inside the border so they don't eat into the visible
     content area; a bit of hover feedback so they're discoverable. */
  .resize-handle {
    position: absolute;
    z-index: 1;
    background: transparent;
  }
  .resize-handle:hover,
  .panel.right.resizing .resize-handle {
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
