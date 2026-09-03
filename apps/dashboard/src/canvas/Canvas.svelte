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
  }

  #particle-slot {
    position: absolute;
    inset: 0;
    z-index: var(--ax-z-particle);
  }
</style>
