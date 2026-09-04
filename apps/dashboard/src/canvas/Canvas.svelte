<!--
  The module board: a fixed, relative-positioned surface (no pan/zoom in M5)
  rendering one Tile per instance in the store. `#particle-slot` is reserved
  behind the tiles for a future background module.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { get } from "svelte/store";

  import { getModule } from "../core/registry";
  import { canvasSize, guides, instances, showGrid, updateInstance } from "../core/stores";
  import BackgroundHost from "./BackgroundHost.svelte";
  import { anchorFor } from "./snap";
  import Tile from "./Tile.svelte";

  const isBackground = (type: string) => getModule(type)?.background === true;
  const backgrounds = $derived($instances.filter((i) => isBackground(i.type)));
  const tiles = $derived($instances.filter((i) => !isBackground(i.type)));

  let section = $state<HTMLElement | null>(null);

  // Publish the canvas box; every Tile derives its displayed position from
  // it (anchored edge + clamp), so nothing is persisted on resize and growing
  // the window brings tiles back to their committed spots.
  onMount(() => {
    if (!section) return;
    const publish = () => {
      const size = { w: section!.clientWidth, h: section!.clientHeight };
      canvasSize.set(size);
      backfillAnchors(size);
    };
    publish();
    const ro = new ResizeObserver(publish);
    ro.observe(section);
    return () => ro.disconnect();
  });

  // Layouts saved before anchors existed (or hand-edited ones) get theirs
  // from the first real canvas size they are shown at — otherwise they would
  // only ever be clamped, never follow their edge.
  function backfillAnchors(size: { w: number; h: number }) {
    if (size.w <= 0 || size.h <= 0) return;
    for (const i of get(instances)) {
      if (i.anchor || isBackground(i.type)) continue;
      updateInstance(i.id, { anchor: anchorFor({ x: i.x, y: i.y, w: i.w, h: i.h }, size) });
    }
  }

  // Instances that arrive later (persistence finishing after mount, picker,
  // /add, the agent) get an anchor as soon as they show up.
  $effect(() => {
    void $instances;
    backfillAnchors($canvasSize);
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
  /* Snap alignment guides: a thin accent line, drawn ABOVE the dragged tile
     (+1) so the whole line reads across the canvas — discreet, just a faint
     halo so a 1px line stays visible. */
  .guide {
    position: absolute;
    z-index: calc(var(--ax-z-tile-drag) + 1);
    pointer-events: none;
    background: var(--ax-accent);
    opacity: 0.65;
    box-shadow: 0 0 3px color-mix(in srgb, var(--ax-accent) 40%, transparent);
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
