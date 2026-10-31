<!-- Node-kind legend for the graph views: glyph + name, drawn with the same
     canvas glyphs as the graph so they match. -->
<script lang="ts">
  import { onMount } from "svelte";

  import type { BrainLayout } from "../core/brainView";
  import { uiScale } from "../core/uiScale";
  import { FRESH_LINK_DAYS } from "./bundle";
  import { readPalette, type NodeKind } from "./model";
  import { drawGlyph } from "./render";
  import { tBrain } from "../i18n/brain";

  /** `app` nodes only exist in orbit mode (the dashboard background's App
   *  Ring) — this legend covers the Second Brain's own bundle/map/hex
   *  views, which never show one. Excluding it from the type (rather than
   *  giving `NodeKind`'s full `Record` a phantom, never-read `app` entry)
   *  keeps the exhaustiveness check meaningful: a kind this legend actually
   *  needs but forgets still fails to compile. */
  type LegendKind = Exclude<NodeKind, "app">;

  /** The "file" swatch is what a note is drawn as in the shown layout: the
   *  map's pointy-top field, the honeycomb's flat-top cell, or the bundle's dot. */
  let { layout = "bundle" }: { layout?: BrainLayout } = $props();

  /** A legend row: a node kind, or (Bundle) a link — calm in its note's area colour, or new in the glow. */
  type Entry = { kind: LegendKind; label: () => string } | { kind: "link" | "fresh-link"; label: () => string };

  const ALL_KINDS: Entry[] = [
    { kind: "hub", label: () => tBrain("legendHub") },
    { kind: "area", label: () => tBrain("legendArea") },
    { kind: "file", label: () => tBrain("legendNote") },
    { kind: "skill", label: () => tBrain("legendSkill") },
  ];
  /** The bundle draws no hub (its middle is where the links run) but its links say something of their own. */
  const BUNDLE_KINDS: Entry[] = [
    ...ALL_KINDS.filter((k) => k.kind !== "hub"),
    { kind: "link", label: () => tBrain("legendLink") },
    { kind: "fresh-link", label: () => tBrain("legendFreshLink", { days: FRESH_LINK_DAYS }) },
  ];
  const KINDS = $derived(layout === "bundle" ? BUNDLE_KINDS : ALL_KINDS);

  let canvases: HTMLCanvasElement[] = $state([]);

  function paint() {
    const p = readPalette();
    const colors: Record<Entry["kind"], string> = {
      hub: p.text,
      area: p.accent,
      file: p.muted,
      skill: p.accent,
      link: p.muted,
      "fresh-link": p.glow,
    };
    KINDS.forEach(({ kind }, i) => {
      const c = canvases[i];
      if (!c) return;
      // Drawn in a 22-unit box; the box on screen grows with the UI scale, so the bitmap does too (crisp).
      const dpr = (window.devicePixelRatio || 1) * $uiScale;
      c.width = Math.round(22 * dpr);
      c.height = Math.round(22 * dpr);
      const ctx = c.getContext("2d");
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, 22, 22);
      if (kind === "link" || kind === "fresh-link") {
        // A bowed stroke, as a bundled link runs through the ring's middle.
        ctx.strokeStyle = colors[kind];
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.moveTo(3, 17);
        ctx.quadraticCurveTo(11, 1, 19, 17);
        ctx.stroke();
        return;
      }
      const r = kind === "file" ? 3 : 7;
      ctx.fillStyle = colors[kind];
      ctx.beginPath();
      if (kind === "file" && layout !== "bundle") {
        const hr = 6;
        // Pointy-top on the map, flat-top in the honeycomb, as each draws its cells.
        const turn = layout === "map" ? -Math.PI / 2 : 0;
        for (let k = 0; k < 6; k++) {
          const a = (Math.PI / 3) * k + turn;
          const px = 11 + Math.cos(a) * hr;
          const py = 11 + Math.sin(a) * hr;
          if (k === 0) ctx.moveTo(px, py);
          else ctx.lineTo(px, py);
        }
        ctx.closePath();
      } else {
        ctx.arc(11, 11, r, 0, Math.PI * 2);
      }
      ctx.fill();
      if (kind !== "file") {
        ctx.strokeStyle = colors[kind];
        ctx.lineWidth = 1.2;
        ctx.beginPath();
        ctx.arc(11, 11, r + 3, 0, Math.PI * 2);
        ctx.stroke();
        // The legend shows the generic default; each area actually picks
        // its own icon from `glyphForArea` (see graph/model.ts).
        drawGlyph(ctx, kind === "area" ? "folder" : kind, 11, 11, r, p.invert);
      }
    });
  }

  onMount(() => {
    const mo = new MutationObserver(paint);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => mo.disconnect();
  });

  // Repaint whenever the canvases mount, the layout switches or the UI scale changes.
  $effect(() => {
    void KINDS;
    void $uiScale;
    paint();
  });
</script>

<ul class="legend">
  {#each KINDS as k, i (k.kind)}
    <li><canvas bind:this={canvases[i]} width="22" height="22"></canvas>{k.label()}</li>
  {/each}
</ul>

<style>
  .legend {
    list-style: none;
    margin: 0;
    padding: var(--ax-space-2) var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    /* Same glass/hairline/elevated-shadow language as Window.svelte and
       every other panel in the app. */
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border: none;
    border-bottom: 2px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-drag);
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  canvas {
    width: calc(22px * var(--ax-ui-scale));
    height: calc(22px * var(--ax-ui-scale));
  }
</style>
