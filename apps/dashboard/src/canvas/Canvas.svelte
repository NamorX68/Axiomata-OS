<!--
  The module board: a fixed, relative-positioned surface (no pan/zoom in M5)
  rendering one Tile per instance in the store. `#particle-slot` is reserved
  behind the tiles for a future background module.
-->
<script lang="ts">
  import { instances } from "../core/stores";
  import Tile from "./Tile.svelte";
</script>

<section class="canvas" aria-label="Module canvas">
  <div id="particle-slot" aria-hidden="true"></div>
  {#each $instances as inst (inst.id)}
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
    pointer-events: none;
  }
</style>
