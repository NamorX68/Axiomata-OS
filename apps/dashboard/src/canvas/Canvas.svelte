<!--
  The module board: a fixed, relative-positioned surface (no pan/zoom in M5)
  rendering one Tile per instance in the store. `#particle-slot` is reserved
  behind the tiles for a future background module.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";

  import { getModule } from "../core/registry";
  import { guides, instances, showGrid, updateInstances } from "../core/stores";
  import BackgroundHost from "./BackgroundHost.svelte";
  import { relayoutForViewport } from "./snap";
  import Tile from "./Tile.svelte";

  const RELAYOUT_DEBOUNCE_MS = 150;

  const isBackground = (type: string) => getModule(type)?.background === true;
  const backgrounds = $derived($instances.filter((i) => isBackground(i.type)));
  const tiles = $derived($instances.filter((i) => !isBackground(i.type)));

  let section = $state<HTMLElement | null>(null);

  // Keep every tile visible when the window (and so the canvas) shrinks.
  onMount(() => {
    if (!section) return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const ro = new ResizeObserver(() => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        const el = section!;
        const placed = get(instances)
          .filter((i) => !isBackground(i.type))
          .map(({ id, x, y, w, h, z }) => ({ id, x, y, w, h, z }));
        const minOf = (id: string) => {
          const inst = get(instances).find((i) => i.id === id);
          return (inst && getModule(inst.type)?.minSize) ?? { w: 160, h: 100 };
        };
        updateInstances(relayoutForViewport(placed, { w: el.clientWidth, h: el.clientHeight }, minOf));
      }, RELAYOUT_DEBOUNCE_MS);
    });
    ro.observe(section);
    return () => {
      ro.disconnect();
      if (timer) clearTimeout(timer);
    };
  });
</script>

<section class="canvas" class:grid={$showGrid} aria-label="Module canvas" bind:this={section}>
  {#each $guides as g, i (i)}
    <div class="guide {g.axis}" style:left={g.axis === "x" ? `${g.at}px` : undefined} style:top={g.axis === "y" ? `${g.at}px` : undefined}></div>
  {/each}
  <div id="particle-slot">
    {#each backgrounds as inst (inst.id)}
      <BackgroundHost {inst} />
    {/each}
  </div>
  {#each tiles as inst (inst.id)}
    <Tile {inst} />
  {/each}
</section>

<style>
  .canvas {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
  }
  /* The tile grid as faint dots — optional (settings.showGrid); tiles snap
     to --ax-grid either way. */
  .canvas.grid {
    background-image: radial-gradient(
      circle,
      color-mix(in srgb, var(--ax-text-muted) 22%, transparent) 1px,
      transparent 1.2px
    );
    background-size: var(--ax-grid) var(--ax-grid);
    background-position: 0 0;
  }
  .guide {
    position: absolute;
    z-index: var(--ax-z-tile-drag);
    pointer-events: none;
    background: var(--ax-accent);
    opacity: 0.7;
  }
  .guide.x {
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .guide.y {
    left: 0;
    right: 0;
    height: 1px;
  }

  #particle-slot {
    position: absolute;
    inset: 0;
    z-index: var(--ax-z-particle);
  }
</style>
