<!--
  Full-screen Second Brain: the graph with pan (drag) / zoom (wheel), hover
  labels, search (dims non-matches), layout Rings / Circle, grouping by
  areas or folders, spin + file-name toggles, and a detail panel for the
  selected node (file → view in md-file / copy path / fly to / connections;
  skill → run; routine → toggle; hub → open). "Back to the OS" closes.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { fade } from "svelte/transition";

  import { invokeBackend, type RunSummary, type WorkspaceGraph } from "../core/backend";
  import { relativeTime, untilTime } from "../core/format";
  import { getSetting, setSetting } from "../core/persist";
  import { openStaged } from "../core/staging";
  import { toast } from "../core/toast";
  import { applyLayout, type LayoutKind } from "../graph/layout";
  import {
    buildModel,
    neighbours,
    readPalette,
    regroup,
    searchNodes,
    type GraphModel,
    type GraphNode,
    type Grouping,
  } from "../graph/model";
  import { GraphRenderer } from "../graph/render";

  let {
    open = $bindable(false),
    focus = null,
    initialQuery = "",
  }: { open?: boolean; focus?: string | null; initialQuery?: string } = $props();

  interface Prefs {
    layout?: LayoutKind;
    grouping?: Grouping;
    spin?: number;
    fileNames?: boolean;
  }
  const prefs = getSetting<Prefs>("secondBrain") ?? {};

  let canvas = $state<HTMLCanvasElement | null>(null);
  let renderer: GraphRenderer | null = null;
  let graph = $state<WorkspaceGraph | null>(null);
  let model = $state<GraphModel | null>(null);
  let selected = $state<GraphNode | null>(null);
  let hover = $state<GraphNode | null>(null);
  // svelte-ignore state_referenced_locally
  let query = $state(initialQuery);
  let layout = $state<LayoutKind>(prefs.layout === "circle" ? "circle" : "rings");
  let grouping = $state<Grouping>(prefs.grouping === "folders" ? "folders" : "areas");
  let spin = $state(typeof prefs.spin === "number" ? prefs.spin : 0.02);
  let fileNames = $state(prefs.fileNames === true);
  let busy = $state(false);
  let error = $state("");
  let drag: { x: number; y: number; vx: number; vy: number } | null = null;

  const links = $derived(model && selected ? neighbours(model, selected.id) : []);
  const hits = $derived(model && query.trim() ? searchNodes(model, query) : null);

  function rebuild() {
    if (!renderer || !graph) return;
    const palette = readPalette();
    const m = buildModel(regroup(graph, grouping), palette);
    applyLayout(m, layout);
    renderer.setColors(palette.text, palette.muted, palette.border);
    renderer.model = m;
    model = m;
    // Re-point the selection at the new model's node without making the
    // callers' effects depend on `selected` (that would re-run them on
    // every select / deselect).
    const current = untrack(() => selected);
    selected = current ? (m.byId.get(current.id) ?? null) : null;
    renderer.selected = selected;
  }

  async function load() {
    try {
      graph = await invokeBackend<WorkspaceGraph>("get_workspace_graph");
      error = "";
      rebuild();
      if (focus && model) {
        lastFocus = focus;
        select(model.byId.get(focus) ?? null, true);
      }
    } catch (err) {
      error = String(err);
    }
  }

  function select(node: GraphNode | null, fly = false) {
    selected = node;
    if (renderer) {
      renderer.selected = node;
      if (node && fly) renderer.centerOn(node);
    }
  }

  function flyTo(node: GraphNode) {
    if (!renderer) return;
    renderer.view.zoom = Math.max(renderer.view.zoom, 1.6);
    renderer.centerOn(node);
  }

  function resetView() {
    if (renderer) renderer.view = { x: 0, y: 0, zoom: 1 };
  }

  function rel(e: MouseEvent) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  function onDown(e: MouseEvent) {
    if (!renderer) return;
    drag = { x: e.clientX, y: e.clientY, vx: renderer.view.x, vy: renderer.view.y };
  }
  function onMove(e: MouseEvent) {
    if (!renderer) return;
    if (drag && e.buttons === 1) {
      renderer.view.x = drag.vx + (e.clientX - drag.x);
      renderer.view.y = drag.vy + (e.clientY - drag.y);
      return;
    }
    const p = rel(e);
    hover = renderer.hitTest(p.x, p.y, 8);
    renderer.hover = hover;
  }
  function onUp(e: MouseEvent) {
    if (!renderer) return;
    const moved = drag ? Math.hypot(e.clientX - drag.x, e.clientY - drag.y) : 0;
    drag = null;
    if (moved < 4) {
      const p = rel(e);
      select(renderer.hitTest(p.x, p.y, 8));
    }
  }
  function onWheel(e: WheelEvent) {
    if (!renderer) return;
    e.preventDefault();
    const factor = Math.exp(-e.deltaY * 0.0015);
    renderer.view.zoom = Math.min(8, Math.max(0.4, renderer.view.zoom * factor));
  }

  async function runSkill(name: string) {
    busy = true;
    try {
      const r = await invokeBackend<RunSummary>("run_skill", { name });
      toast(`/${name}: ${r.status} (${r.duration_ms} ms)`, r.status === "success" ? "info" : "warning");
    } catch (err) {
      toast(String(err), "danger");
    } finally {
      busy = false;
    }
  }

  async function toggleRoutine(node: GraphNode) {
    const id = Number(node.id.slice("routine:".length));
    busy = true;
    try {
      await invokeBackend("set_routine_enabled", { id, enabled: !node.enabled });
      await load();
    } catch (err) {
      toast(String(err), "danger");
    } finally {
      busy = false;
    }
  }

  function viewFile(path: string) {
    openStaged("md-file", { path, mode: "read" }, "right");
  }

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      toast("Path copied.");
    } catch {
      toast(path);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    // A staged panel on top consumes Escape first (StagingLayer marks it).
    if (!open || e.key !== "Escape" || e.defaultPrevented) return;
    e.preventDefault();
    if (selected) select(null);
    else open = false;
  }

  $effect(() => {
    if (renderer) renderer.options = { ...renderer.options, spin, fileLabels: fileNames };
  });
  // Remember the view preferences in dashboard.json (settings.secondBrain).
  let prefsReady = false;
  $effect(() => {
    const next: Prefs = { layout, grouping, spin, fileNames };
    if (prefsReady) setSetting("secondBrain", next);
    prefsReady = true;
  });
  $effect(() => {
    if (renderer) renderer.highlight = hits;
  });
  $effect(() => {
    void layout;
    void grouping;
    untrack(rebuild);
  });
  // A new query from `/brain ? …` or the module's search action while open.
  $effect(() => {
    if (initialQuery) query = initialQuery;
  });
  // `/brain <path>` while the view is already open re-targets the focus —
  // once per focus value, not on every model rebuild.
  let lastFocus: string | null = null;
  $effect(() => {
    const target = focus;
    const m = untrack(() => model);
    if (target && target !== lastFocus && m) {
      lastFocus = target;
      const node = m.byId.get(target) ?? null;
      if (node) select(node, true);
    }
  });

  onMount(() => {
    if (!canvas) return;
    renderer = new GraphRenderer(canvas);
    renderer.options = { spin, labels: true, fileLabels: fileNames, fit: 0.44 };
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
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      mo.disconnect();
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<section class="brain" aria-label="Second Brain">
  <header>
    <h1>
      <svg class="logo" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 2.5l8.2 4.75v9.5L12 21.5l-8.2-4.75v-9.5z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
      </svg>
      <span>Axiomata</span> <em>Second Brain</em>
    </h1>
    <button type="button" class="back" onclick={() => (open = false)}>← Back to the OS</button>
  </header>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="stage"
    class:hovering={hover !== null}
    onmousedown={onDown}
    onmousemove={onMove}
    onmouseup={onUp}
    onmouseleave={() => (drag = null)}
    onwheel={onWheel}
  >
    <canvas bind:this={canvas}></canvas>
    {#if error}<p class="error">{error}</p>{/if}
  </div>

  <aside class="controls">
    <input
      type="search"
      placeholder={model ? `Search ${model.nodes.length} nodes…` : "Search…"}
      aria-label="Search nodes"
      bind:value={query}
    />
    <div class="group">
      <span class="label">Layout</span>
      <div class="seg">
        <button type="button" class:on={layout === "rings"} onclick={() => (layout = "rings")}>Rings</button>
        <button type="button" class:on={layout === "circle"} onclick={() => (layout = "circle")}>Circle</button>
      </div>
    </div>
    <div class="group">
      <span class="label">View</span>
      <div class="seg">
        <button type="button" class:on={grouping === "areas"} onclick={() => (grouping = "areas")}>Areas</button>
        <button type="button" class:on={grouping === "folders"} onclick={() => (grouping = "folders")}>Folders</button>
      </div>
    </div>
    <label class="row"><span class="label">Ring spin</span><input type="range" min="0" max="0.12" step="0.005" bind:value={spin} /></label>
    <label class="row check"><input type="checkbox" bind:checked={fileNames} /> File names</label>
    <div class="row">
      <button type="button" onclick={resetView}>Reset view</button>
      <button type="button" onclick={() => void load()}>Reload</button>
    </div>
    {#if model}
      <p class="stats">{model.totalFiles} files · {model.areas.length} {grouping} · {model.edges.length} edges{model.truncated ? " · truncated" : ""}</p>
    {/if}
  </aside>

  {#if selected}
    <aside class="detail" transition:fade={{ duration: 120 }}>
      <header>
        <h2>{selected.label}</h2>
        <button type="button" class="close" aria-label="Deselect" onclick={() => select(null)}>×</button>
      </header>
      <div class="tags">
        <span class="tag kind">{selected.kind}</span>
        {#if selected.area}<span class="tag">{selected.area}</span>{/if}
        {#if selected.kind === "file"}<span class="tag muted">{(selected.bytes / 1024).toFixed(1)} KB</span>{/if}
      </div>
      {#if selected.path}<p class="path">{selected.path}</p>{/if}

      {#if selected.kind === "file" || selected.kind === "hub"}
        <div class="actions">
          <button type="button" onclick={() => viewFile(selected!.path!)}>View here</button>
          <button type="button" onclick={() => copyPath(selected!.path!)}>Copy path</button>
          <button type="button" onclick={() => flyTo(selected!)}>Fly to</button>
        </div>
      {:else if selected.kind === "skill"}
        <p class="muted">{graph?.skills.find((s) => `/${s.name}` === selected!.label)?.description ?? ""}</p>
        <div class="actions">
          <button type="button" disabled={busy} onclick={() => runSkill(selected!.label.slice(1))}>▶ Run</button>
          <button type="button" onclick={() => flyTo(selected!)}>Fly to</button>
        </div>
      {:else if selected.kind === "routine"}
        {@const r = graph?.routines.find((x) => `routine:${x.id}` === selected!.id)}
        {#if r}
          <p class="muted"><code>{r.cron_expr}</code> · {r.target.type}: {r.target.value}</p>
          <p class="muted">next {r.enabled ? untilTime(r.next_fire_at) : "—"} · last {relativeTime(r.last_fired_at)}</p>
        {/if}
        <div class="actions">
          <button type="button" disabled={busy} onclick={() => toggleRoutine(selected!)}>{selected.enabled ? "Disable" : "Enable"}</button>
          <button type="button" onclick={() => flyTo(selected!)}>Fly to</button>
        </div>
      {/if}

      {#if links.length > 0}
        <h3>Connections</h3>
        <ul class="links">
          {#each links as l (l.node.id + (l.out ? ">" : "<"))}
            <li>
              <button type="button" class="link" onclick={() => select(l.node, true)}>
                <span class="dot" style:background={l.node.color}></span>
                {l.node.label}
                <span class="dir">{l.out ? "→" : "←"}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>
  {/if}
</section>

<style>
  .brain {
    position: fixed;
    inset: 0;
    /* Below the staging layer so "View here" panels slide in on top. */
    z-index: calc(var(--ax-z-staging) - 1);
    background: var(--ax-bg);
    background-image: var(--ax-texture-url);
    color: var(--ax-text);
  }
  .brain > header {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    padding: var(--ax-space-4) var(--ax-space-5);
    pointer-events: none;
  }
  h1 {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: 20px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  h1 em {
    font-style: normal;
    font-weight: 400;
    color: var(--ax-text-muted);
  }
  .logo {
    width: 22px;
    height: 22px;
    color: var(--ax-accent);
  }
  .back {
    margin-left: auto;
    pointer-events: auto;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    font-size: var(--ax-font-size-sm);
    border-color: var(--ax-accent);
  }

  .stage {
    position: absolute;
    inset: 0;
    cursor: grab;
  }
  .stage.hovering {
    cursor: pointer;
  }
  .stage:active {
    cursor: grabbing;
  }
  canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
  .error {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    color: var(--ax-danger);
  }

  .controls {
    position: absolute;
    z-index: 2;
    top: 72px;
    right: var(--ax-space-5);
    width: 250px;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
    padding: var(--ax-space-3);
    background: color-mix(in srgb, var(--ax-surface-1) 88%, transparent);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-pop);
    font-size: var(--ax-font-size-sm);
  }
  .controls input[type="search"] {
    width: 100%;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .label {
    font-size: 10px;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }
  .seg {
    display: flex;
    gap: var(--ax-space-1);
  }
  .seg button {
    flex: 1 1 0;
    padding: 2px var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .seg button.on {
    background: var(--ax-accent);
    border-color: var(--ax-accent);
    color: var(--ax-text-invert);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  .row input[type="range"] {
    flex: 1 1 auto;
    accent-color: var(--ax-accent);
  }
  .row button {
    flex: 1 1 0;
    font-size: var(--ax-font-size-sm);
  }
  .stats {
    margin: 0;
    color: var(--ax-text-muted);
  }

  .detail {
    position: absolute;
    z-index: 2;
    top: 72px;
    left: var(--ax-space-5);
    width: 300px;
    max-height: calc(100vh - 100px);
    overflow: auto;
    padding: var(--ax-space-3) var(--ax-space-4);
    background: color-mix(in srgb, var(--ax-surface-1) 92%, transparent);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-pop);
    font-size: var(--ax-font-size-sm);
  }
  .detail header {
    display: flex;
    align-items: flex-start;
    gap: var(--ax-space-2);
  }
  .detail h2 {
    flex: 1 1 auto;
    font-size: var(--ax-font-size-lg);
    word-break: break-word;
  }
  .close {
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
    margin: var(--ax-space-2) 0;
  }
  .tag {
    padding: 1px var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    border: 1px solid var(--ax-accent);
    color: var(--ax-accent);
    font-size: 11px;
  }
  .tag.kind {
    background: var(--ax-accent);
    color: var(--ax-text-invert);
  }
  .tag.muted {
    border-color: var(--ax-border-strong);
    color: var(--ax-text-muted);
  }
  .path {
    margin: 0 0 var(--ax-space-2);
    font-family: var(--ax-font-mono);
    color: var(--ax-text-muted);
    word-break: break-all;
  }
  .muted {
    margin: 0 0 var(--ax-space-2);
    color: var(--ax-text-muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
  }
  .actions button {
    font-size: var(--ax-font-size-sm);
    padding: 2px var(--ax-space-2);
  }
  h3 {
    margin-top: var(--ax-space-3);
    font-size: 10px;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }
  .links {
    list-style: none;
    margin: var(--ax-space-1) 0 0;
    padding: 0;
  }
  .link {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    text-align: left;
    background: transparent;
    border-color: transparent;
    padding: 2px var(--ax-space-1);
  }
  .link:hover {
    background: var(--ax-surface-3);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: 0 0 auto;
  }
  .dir {
    margin-left: auto;
    color: var(--ax-text-muted);
  }
</style>
