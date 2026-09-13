<!-- App-Ring group icon picker ("Symbol ändern" in AppContextMenu.svelte):
     a compact grid of the same drawGlyph vector glyphs the ring itself
     draws, painted with Legend.svelte's exact one-canvas-per-glyph
     technique (copied for the painting technique, not the styling — this
     needs to fit inside a context-menu-sized panel, so a grid instead of a
     vertical list). `APP_GROUP_GLYPHS` is never hardcoded to a count here,
     so a future expansion stage (more glyphs) is just a longer list. -->
<script lang="ts">
  import { onMount } from "svelte";

  import { readPalette } from "./model";
  import { APP_GROUP_GLYPHS, drawGlyph } from "./render";

  let { selected, onPick }: { selected: string; onPick: (glyph: string) => void } = $props();

  /** Canvas side length in CSS px — owner feedback: the original 22px
   *  (matching Legend.svelte's small swatches), then 28px, both still read
   *  too small once this became something the owner clicks to pick, not
   *  just a passive legend. */
  const SIZE = 36;

  let canvases: HTMLCanvasElement[] = $state([]);

  function paint() {
    const p = readPalette();
    APP_GROUP_GLYPHS.forEach((g, i) => {
      const c = canvases[i];
      if (!c) return;
      const dpr = window.devicePixelRatio || 1;
      c.width = SIZE * dpr;
      c.height = SIZE * dpr;
      const ctx = c.getContext("2d");
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, SIZE, SIZE);
      drawGlyph(ctx, g, SIZE / 2, SIZE / 2, SIZE * 0.46, g === selected ? p.accent : p.text);
    });
  }

  onMount(() => {
    const mo = new MutationObserver(paint);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => mo.disconnect();
  });

  // Repaint whenever the canvases mount or `selected` changes, so the
  // highlighted glyph updates immediately after a pick.
  $effect(() => {
    selected;
    paint();
  });
</script>

<div class="picker" role="listbox" aria-label="Symbol wählen">
  {#each APP_GROUP_GLYPHS as g, i (g)}
    <button
      type="button"
      class="cell"
      class:selected={g === selected}
      role="option"
      aria-label={g}
      aria-selected={g === selected}
      onclick={() => onPick(g)}
    >
      <canvas bind:this={canvases[i]} width={SIZE} height={SIZE}></canvas>
    </button>
  {/each}
</div>

<style>
  .picker {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--ax-space-1);
  }
  .cell {
    display: grid;
    place-items: center;
    width: 50px;
    height: 50px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--ax-radius-sm);
  }
  .cell:hover {
    background: var(--ax-surface-3);
  }
  .cell.selected {
    border-color: var(--ax-accent);
    background: var(--ax-surface-3);
  }
</style>
