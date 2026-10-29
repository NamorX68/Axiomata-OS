<!--
  The grips a floating panel is resized by: a thin strip along each of its four edges, and with `corners` a square on
  each corner that pulls both sides it joins. Rendered straight into the panel (no wrapper), so the panel must be
  positioned; each strip reaches 3px past the border, so a panel that clips (`overflow: hidden`) loses that half.

  The grips only report — `use:resizable` (`canvas/resize.ts`) turns the pointer's way into growth, and the panel
  clamps, draws and remembers the size itself (`StagingPanel.svelte`, `shell/Window.svelte`).
-->
<script lang="ts">
  import { resizable, type ResizeDelta, type ResizeDir } from "../canvas/resize";

  let {
    corners = false,
    active = false,
    onStart,
    onMove,
    onEnd,
  }: {
    /** Also grips on the corners (both sides at once). */
    corners?: boolean;
    /** A resize is under way: the grips stay lit while the pointer runs ahead of them. */
    active?: boolean;
    onStart: (dir: ResizeDir) => void;
    /** The growth since the press, in screen pixels. */
    onMove: (dir: ResizeDir, delta: ResizeDelta) => void;
    onEnd: (dir: ResizeDir, delta: ResizeDelta) => void;
  } = $props();

  const EDGES: ResizeDir[] = ["w", "e", "n", "s"];
  const CORNERS: ResizeDir[] = ["nw", "ne", "sw", "se"];
</script>

{#each corners ? [...EDGES, ...CORNERS] : EDGES as dir (dir)}
  <div
    class="resize-handle resize-{dir}"
    class:corner={dir.length === 2}
    class:active
    use:resizable={{
      dir,
      onStart: () => onStart(dir),
      onMove: (d) => onMove(dir, d),
      onEnd: (d) => onEnd(dir, d),
    }}
  ></div>
{/each}

<style>
  /* Invisible strips just outside the border, so they do not eat into the content; a little hover feedback so they
     can be found. */
  .resize-handle {
    position: absolute;
    z-index: 1;
    background: transparent;
  }
  .resize-handle:not(.corner):hover,
  .resize-handle:not(.corner).active {
    background: var(--ax-text-muted);
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

  /* A corner lies over the two strips it joins, so the corner wins there. */
  .corner {
    z-index: 2;
    width: 12px;
    height: 12px;
  }
  .resize-nw {
    top: -3px;
    left: -3px;
    cursor: nwse-resize;
  }
  .resize-se {
    bottom: -3px;
    right: -3px;
    cursor: nwse-resize;
  }
  .resize-ne {
    top: -3px;
    right: -3px;
    cursor: nesw-resize;
  }
  .resize-sw {
    bottom: -3px;
    left: -3px;
    cursor: nesw-resize;
  }
</style>
