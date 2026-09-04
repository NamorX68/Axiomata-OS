<!-- Node-kind legend for the graph views: glyph + name, drawn with the same
     canvas glyphs as the graph so they match. -->
<script lang="ts">
  import { onMount } from "svelte";

  import { readPalette, type NodeKind } from "./model";
  import { drawGlyph } from "./render";

  /** In Hex mode the "file" swatch matches the honeycomb cell it stands
   *  for instead of the dot every other layout uses. */
  let { hex = false }: { hex?: boolean } = $props();

  const KINDS: { kind: NodeKind; label: string }[] = [
    { kind: "hub", label: "CLAUDE.md (hub)" },
    { kind: "area", label: "Bereich (Ordner)" },
    { kind: "file", label: "Notiz / Seite" },
    { kind: "skill", label: "Skill" },
    { kind: "routine", label: "Routine" },
  ];

  let canvases: HTMLCanvasElement[] = $state([]);

  function paint() {
    const p = readPalette();
    const colors: Record<NodeKind, string> = { hub: p.text, area: p.accent, file: p.muted, skill: p.accent, routine: p.warning };
    KINDS.forEach(({ kind }, i) => {
      const c = canvases[i];
      if (!c) return;
      const dpr = window.devicePixelRatio || 1;
      c.width = 22 * dpr;
      c.height = 22 * dpr;
      const ctx = c.getContext("2d");
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, 22, 22);
      const r = kind === "file" ? 3 : 7;
      ctx.fillStyle = colors[kind];
      ctx.beginPath();
      if (kind === "file" && hex) {
        const hr = 6;
        for (let k = 0; k < 6; k++) {
          const a = (Math.PI / 3) * k;
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

  // Repaint whenever the canvases mount or `hex` flips (layout switch).
  $effect(() => {
    hex;
    paint();
  });
</script>

<ul class="legend">
  {#each KINDS as k, i (k.kind)}
    <li><canvas bind:this={canvases[i]} width="22" height="22"></canvas>{k.label}</li>
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
    background: color-mix(in srgb, var(--ax-surface-1) 85%, transparent);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  canvas {
    width: 22px;
    height: 22px;
  }
</style>
