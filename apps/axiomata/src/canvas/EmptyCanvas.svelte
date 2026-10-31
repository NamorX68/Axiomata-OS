<!--
  The empty dashboard (`docs/plans/ui-ux-round.md` UX3, "generative empty canvas"): instead of nothing, a honeycomb
  grows ring by ring around a glowing centre cell, with the one thing to do — add a module. Shown only while the
  canvas has neither tiles nor a background module.

  At motion Full the comb grows and the centre breathes; at Calm and Off it is simply there.
-->
<script lang="ts">
  import { emit } from "../core/bus";
  import { animates, motion } from "../core/motion";
  import { tCanvas } from "../i18n/canvas";
  import { hexPoints, honeycomb } from "./honeycomb";

  /** How many rings the comb has around its centre. */
  const RINGS = 3;
  /** The cells' circumradius in the SVG's own units. */
  const R = 10;
  /** The gap between cells, in the same units. */
  const GAP = 1.6;
  /** How long a ring waits for the one inside it. */
  const RING_DELAY_MS = 260;

  const cells = honeycomb(RINGS);
  const extent = (RINGS * Math.sqrt(3) + Math.sqrt(3) / 2) * (R + GAP / 2) + 2;
</script>

<div class="empty" class:grow={animates($motion)}>
  <svg viewBox="{-extent} {-extent} {extent * 2} {extent * 2}" aria-hidden="true">
    {#each cells as cell, i (i)}
      <polygon
        class="cell ring-{cell.ring}"
        class:core={cell.ring === 0}
        points={hexPoints(cell.x * (R + GAP / 2), cell.y * (R + GAP / 2), R)}
        style:animation-delay={cell.ring === 0 ? undefined : `${cell.ring * RING_DELAY_MS + (i % 6) * 40}ms`}
      />
    {/each}
  </svg>
  <p class="title">{tCanvas("emptyTitle")}</p>
  <p class="text">{tCanvas("emptyText")}</p>
  <button type="button" class="ax-btn primary" onclick={() => emit("shell:add-module")}>{tCanvas("addAModule")}</button>
</div>

<style>
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: var(--ax-space-2);
    text-align: center;
    pointer-events: none;
  }

  .empty button {
    pointer-events: auto;
    margin-top: var(--ax-space-2);
  }

  svg {
    width: calc(220px * var(--ax-ui-scale));
    height: calc(220px * var(--ax-ui-scale));
    margin-bottom: var(--ax-space-3);
    overflow: visible;
  }

  .cell {
    fill: var(--ax-elev-1);
    stroke: var(--ax-border-strong);
    stroke-width: 0.6;
    transform-box: fill-box;
    transform-origin: center;
  }

  /* The outer rings fade into the ground: the comb is a beginning, not a frame. */
  .ring-2 {
    opacity: 0.6;
  }

  .ring-3 {
    opacity: 0.3;
  }

  .cell.core {
    fill: var(--ax-glow);
    stroke: none;
    filter: var(--ax-glow-drop);
  }

  .grow .cell {
    animation: comb-grow var(--ax-dur-slow) var(--ax-spring) both;
  }

  /* The core grows in first, then breathes: started together, the breath's opacity would cover the fade-in. */
  .grow .cell.core {
    animation:
      comb-grow var(--ax-dur-slow) var(--ax-spring) both,
      var(--ax-anim-breathe);
    animation-delay: 0ms, var(--ax-dur-slow);
  }

  @keyframes comb-grow {
    from {
      opacity: 0;
      transform: scale(0.4);
    }
  }

  .title {
    margin: 0;
    font: var(--ax-type-heading);
    color: var(--ax-text);
  }

  .text {
    margin: 0;
    max-width: calc(380px * var(--ax-ui-scale));
    font: var(--ax-type-body);
    color: var(--ax-text-muted);
  }
</style>
