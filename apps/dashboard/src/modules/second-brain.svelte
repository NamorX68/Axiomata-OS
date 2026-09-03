<!--
  second-brain — the particle graph behind the tiles: the workspace as
  rings (skills inner, areas as coloured segments, routines outer, CLAUDE.md
  hub). Loads `get_workspace_graph` on mount and every REFRESH_MS, redraws
  on theme change, spins slowly. Hover shows the node label; a click on a
  node or the centre opens the full Second Brain view (bus
  `open-second-brain`, step 4). Config: `spin`, `labels`.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { WorkspaceGraph } from "../core/backend";
  import type { ModuleContext } from "../core/types";
  import { layoutRings } from "../graph/layout";
  import { buildModel, readPalette, type GraphNode } from "../graph/model";
  import { GraphRenderer } from "../graph/render";

  const REFRESH_MS = 30_000;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let canvas = $state<HTMLCanvasElement | null>(null);
  let renderer = $state.raw<GraphRenderer | null>(null);
  let graph: WorkspaceGraph | null = null;
  let hover = $state<GraphNode | null>(null);
  let error = $state("");
  let summary = $state("");

  const spin = $derived($config.spin !== false);
  const labels = $derived($config.labels !== false);

  function rebuild() {
    if (!renderer || !graph) return;
    const palette = readPalette();
    const model = buildModel(graph, palette);
    layoutRings(model);
    renderer.setColors(palette.text, palette.muted, palette.border, palette.invert, palette.invert);
    renderer.model = model;
    summary = `${graph.files.length} files · ${graph.areas.length} areas · ${graph.links.length} links${graph.truncated ? " · truncated" : ""}`;
  }

  async function load() {
    try {
      graph = await ctx.invoke<WorkspaceGraph>("get_workspace_graph");
      error = "";
      rebuild();
    } catch (err) {
      error = String(err);
    }
  }

  function onMove(e: MouseEvent) {
    if (!renderer) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    hover = renderer.hitTest(e.clientX - rect.left, e.clientY - rect.top);
    renderer.hover = hover;
  }

  function onLeave() {
    hover = null;
    if (renderer) renderer.hover = null;
  }

  function open(node: GraphNode | null) {
    ctx.emit("open-second-brain", { focus: node?.id ?? null });
  }

  $effect(() => {
    // Read the reactive inputs first — an early return on a missing renderer
    // would otherwise leave the effect without dependencies.
    const next = { spin: spin ? 0.02 : 0, labels };
    if (renderer) renderer.options = { ...renderer.options, ...next };
  });

  onMount(() => {
    if (!canvas) return;
    renderer = new GraphRenderer(canvas);
    renderer.options = { spin: spin ? 0.02 : 0, labels, fileLabels: false, fit: 0.42 };
    renderer.resize();
    const ro = new ResizeObserver(() => renderer?.resize());
    ro.observe(canvas);
    const mo = new MutationObserver(() => rebuild());
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    let raf = 0;
    const tick = (now: number) => {
      renderer?.frame(now);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    void load();
    const refresh = setInterval(() => void load(), REFRESH_MS);
    return () => {
      cancelAnimationFrame(raf);
      clearInterval(refresh);
      ro.disconnect();
      mo.disconnect();
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
  class="brain"
  class:hovering={hover !== null}
  onmousemove={onMove}
  onmouseleave={onLeave}
  onclick={() => open(hover)}
>
  <canvas bind:this={canvas}></canvas>
  {#if error}
    <p class="error">{error}</p>
  {:else}
    <p class="hint">{hover ? hover.label : "CLICK TO OPEN SECOND BRAIN"}</p>
    <p class="summary">{summary}</p>
  {/if}
</div>

<style>
  .brain {
    position: absolute;
    inset: 0;
    cursor: pointer;
  }
  .brain.hovering {
    cursor: pointer;
  }
  canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
  .hint,
  .summary {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    margin: 0;
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-accent);
    pointer-events: none;
    white-space: nowrap;
  }
  .hint {
    bottom: 28px;
  }
  .summary {
    bottom: 8px;
    text-transform: none;
    letter-spacing: 0.04em;
    color: var(--ax-text-muted);
  }
  .error {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
  }
</style>
