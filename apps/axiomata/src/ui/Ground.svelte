<!--
  The ground the glass shows (docs/plans/glass.md, G15–G16): the theme's texture or the owner's image, blurred once as a
  still layer by `--ax-ground-blur`. Every translucent surface over it reads as frosted without a blur of its own, which
  would be computed again on every frame something inside it scrolls. Lies behind its stacking context's content
  (`z-index: -1`); `fixed` for the app itself, under the Orbit and the canvas, the default for a view of its own such
  as the Studio, which covers the app.
-->
<script lang="ts">
  import { wallpaper } from "../core/wallpaper";

  let { fixed = false }: { fixed?: boolean } = $props();
</script>

<div class="ground" class:fixed class:image={$wallpaper !== null} aria-hidden="true"></div>

<style>
  .ground {
    position: absolute;
    inset: 0;
    z-index: -1;
    overflow: hidden;
    pointer-events: none;
  }

  .ground.fixed {
    position: fixed;
  }

  /* The theme's texture three times over: each theme draws it at a whisper (about 3 %) for the canvas, which behind
     clear glass would be no ground at all; stacked, it is about 9 %, in every theme at once. Painted once, like the
     blur, so the extra layers cost nothing while working. The blurred layer overhangs the box, so its edge does not
     fade in from the window's sides. */
  .ground::before {
    content: "";
    position: absolute;
    inset: calc(-2 * var(--ax-ground-blur));
    background: var(--ax-texture-url), var(--ax-texture-url), var(--ax-texture-url), var(--ax-glass-backdrop);
    filter: blur(var(--ax-ground-blur));
  }

  .ground.image::before {
    background: var(--ax-wallpaper) center / cover no-repeat, var(--ax-bg);
  }

  /* Over a picture, a veil of the theme's ground: text stays readable whatever the image is. */
  .ground.image::after {
    content: "";
    position: absolute;
    inset: 0;
    background: var(--ax-ground-scrim);
  }
</style>
