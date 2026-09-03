<!--
  The module board: a fixed, relative-positioned surface (no pan/zoom in M5)
  rendering one Tile per instance in the store. `#particle-slot` is reserved
  behind the tiles for a future background module.
-->
<script lang="ts">
  import { getModule } from "../core/registry";
  import { instances } from "../core/stores";
  import BackgroundHost from "./BackgroundHost.svelte";
  import Tile from "./Tile.svelte";

  const isBackground = (type: string) => getModule(type)?.background === true;
  const backgrounds = $derived($instances.filter((i) => isBackground(i.type)));
  const tiles = $derived($instances.filter((i) => !isBackground(i.type)));
</script>

<section class="canvas" aria-label="Module canvas">
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
    /* The tile grid, as faint dots (tiles snap to --ax-grid). */
    background-image: radial-gradient(
      circle,
      color-mix(in srgb, var(--ax-text-muted) 22%, transparent) 1px,
      transparent 1.2px
    );
    background-size: var(--ax-grid) var(--ax-grid);
    background-position: 0 0;
  }

  #particle-slot {
    position: absolute;
    inset: 0;
    z-index: var(--ax-z-particle);
  }
</style>
