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

  import { invokeBackend, type RunSummary, type WorkspaceFile, type WorkspaceGraph } from "../core/backend";
  import { absoluteTime, formatBytes, relativeTime, untilTime } from "../core/format";
  import { excerpt, excerptHtml } from "../core/markdown";
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
  import Legend from "../graph/Legend.svelte";
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
    help?: boolean;
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
  let helpOpen = $state(prefs.help !== false);
  let areaFilter = $state("");
  let preview = $state<{ path: string; text: string } | null>(null);
  let previewState = $state<"idle" | "loading" | "none" | "error">("idle");
  const previewCache = new Map<string, string>();

  const rotationLabel = $derived(spin === 0 ? "off" : spin < 0.03 ? "slow" : spin < 0.07 ? "medium" : "fast");

  const HELP = [
    ["Rings", "Notizen liegen auf Bögen innerhalb ihres Bereichs-Segments; Skills innen, Bereiche auf dem nächsten Ring, Routinen außen. Zeigt die Größe je Bereich."],
    ["Circle", "Alle Notizen auf einem Ring, nach Bereich sortiert. Flacher, am besten um Verbindungen zwischen Bereichen zu sehen."],
    ["Areas", "Ein Segment je oberstem Vault-Ordner."],
    ["Folders", "Ein Segment je tiefstem Ordner, z. B. Learning/Rust/lessons. Feinere Aufteilung großer Bereiche."],
    ["Rotation", "Drehgeschwindigkeit des ganzen Graphen; greift sofort, ganz links steht er still. Bei kleinen Werten sieht man die Drehung erst über Sekunden."],
    ["File names", "Zeigt jeden Notiztitel dauerhaft; sonst erscheinen Titel bei Hover, Suche und Auswahl."],
  ] as const;
  let busy = $state(false);
  let error = $state("");
  let drag: { x: number; y: number; vx: number; vy: number } | null = null;

  // Real links only — hub / area spokes are structure, shown in the meta rows.
  const links = $derived(
    model && selected
      ? neighbours(model, selected.id).filter((l) => l.node.kind !== "hub" && l.node.kind !== "area" && selected!.kind !== "area")
      : [],
  );
  const linksOut = $derived(links.filter((l) => l.out));
  const linksIn = $derived(links.filter((l) => !l.out));
  const areaFiles = $derived(
    model && selected?.kind === "area"
      ? model.nodes.filter((n) => n.kind === "file" && n.area === selected!.area).sort((a, b) => (a.path ?? "").localeCompare(b.path ?? ""))
      : [],
  );
  /** Area files grouped by their immediate subfolder (relative to the area). */
  const areaGroups = $derived.by(() => {
    const q = areaFilter.trim().toLowerCase();
    const groups = new Map<string, GraphNode[]>();
    for (const n of areaFiles) {
      if (q && !`${n.label} ${n.path}`.toLowerCase().includes(q)) continue;
      const rel = (n.path ?? "").slice((selected?.area?.length ?? 0) + 1);
      const sub = rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "";
      groups.set(sub, [...(groups.get(sub) ?? []), n]);
    }
    return [...groups.entries()].sort(([a], [b]) => a.localeCompare(b));
  });
  const areaNode = $derived(model && selected?.area ? (model.byId.get(`area:${selected.area}`) ?? null) : null);
  const folderOf = $derived(selected?.path?.includes("/") ? selected.path.slice(0, selected.path.lastIndexOf("/")) : "");
  const hits = $derived(model && query.trim() ? searchNodes(model, query) : null);

  function rebuild() {
    if (!renderer || !graph) return;
    const palette = readPalette();
    const m = buildModel(regroup(graph, grouping), palette);
    applyLayout(m, layout);
    renderer.setColors(palette.text, palette.muted, palette.border, palette.invert, palette.invert);
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
    const next: Prefs = { layout, grouping, spin, fileNames, help: helpOpen };
    if (prefsReady) setSetting("secondBrain", next);
    prefsReady = true;
  });

  // Content preview for the selected file / hub.
  $effect(() => {
    const node = selected;
    const path = node && (node.kind === "file" || node.kind === "hub") ? node.path : undefined;
    if (!path) {
      preview = null;
      previewState = "idle";
      return;
    }
    const isHtml = /\.html?$/i.test(path);
    if (!(node?.isMarkdown ?? node?.kind === "hub") && !isHtml) {
      preview = null;
      previewState = "none";
      return;
    }
    const cached = previewCache.get(path);
    if (cached !== undefined) {
      preview = { path, text: cached };
      previewState = "idle";
      return;
    }
    previewState = "loading";
    void invokeBackend<WorkspaceFile>("read_workspace_file", { rel: path })
      .then((f) => {
        const text = isHtml ? excerptHtml(f.content) : excerpt(f.content);
        previewCache.set(path, text);
        if (previewCache.size > 50) previewCache.delete(previewCache.keys().next().value!);
        if (selected?.path === path) {
          preview = { path, text };
          previewState = "idle";
        }
      })
      .catch(() => {
        if (selected?.path === path) previewState = "error";
      });
  });

  function openFolder() {
    if (!folderOf) return;
    grouping = "folders";
    // The regroup happens in the layout effect; select the folder node after it.
    queueMicrotask(() => {
      const n = model?.byId.get(`area:${folderOf}`);
      if (n) select(n, true);
    });
  }
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
    <div class="controls-head">
      <input
        type="search"
        placeholder={model ? `Search ${model.nodes.length} nodes…` : "Search…"}
        aria-label="Search nodes"
        title="Titel, Pfad oder Bereich; Treffer bleiben hell, der Rest wird gedimmt"
        bind:value={query}
      />
      <button type="button" class="help-btn" class:on={helpOpen} title="Was bedeuten die Optionen?" aria-label="Help" onclick={() => (helpOpen = !helpOpen)}>?</button>
    </div>
    <div class="group">
      <span class="label">Layout</span>
      <div class="seg">
        <button type="button" class:on={layout === "rings"} title={HELP[0][1]} aria-describedby="help-rings" onclick={() => (layout = "rings")}>Rings</button>
        <button type="button" class:on={layout === "circle"} title={HELP[1][1]} aria-describedby="help-circle" onclick={() => (layout = "circle")}>Circle</button>
      </div>
    </div>
    <div class="group">
      <span class="label">Group by</span>
      <div class="seg">
        <button type="button" class:on={grouping === "areas"} title={HELP[2][1]} aria-describedby="help-areas" onclick={() => (grouping = "areas")}>Areas</button>
        <button type="button" class:on={grouping === "folders"} title={HELP[3][1]} aria-describedby="help-folders" onclick={() => (grouping = "folders")}>Folders</button>
      </div>
    </div>
    <label class="row" title={HELP[4][1]}>
      <span class="label">Rotation</span>
      <input type="range" min="0" max="0.12" step="0.005" bind:value={spin} aria-describedby="help-rotation" />
      <span class="readout">{rotationLabel}</span>
    </label>
    <label class="row check" title={HELP[5][1]}><input type="checkbox" bind:checked={fileNames} aria-describedby="help-file-names" /> File names</label>
    <div class="row">
      <button type="button" title="Zoom und Verschiebung zurücksetzen" onclick={resetView}>Reset view</button>
      <button type="button" title="Graph neu aus dem Workspace laden" onclick={() => void load()}>Reload</button>
    </div>
    {#if model}
      <p class="stats">{model.totalFiles} notes · {model.areas.length} {grouping} · {model.edges.length} links{model.truncated ? " · truncated" : ""}</p>
    {/if}
    {#if helpOpen}
      <dl class="help">
        {#each HELP as [term, text] (term)}
          <dt id="help-{term.toLowerCase().replace(' ', '-')}">{term}</dt>
          <dd>{text}</dd>
        {/each}
      </dl>
    {/if}
  </aside>

  <div class="legend-slot"><Legend /></div>

  {#if selected}
    <aside class="detail" transition:fade={{ duration: 120 }}>
      <header>
        <div class="eyebrow">
          <span class="tag kind">{selected.kind}</span>
          {#if selected.area && selected.kind !== "area"}
            <button type="button" class="tag area" style:--chip={selected.color} onclick={() => areaNode && select(areaNode, true)}>{selected.area}</button>
          {/if}
        </div>
        <h2>{selected.label}</h2>
        <button type="button" class="close" aria-label="Deselect" onclick={() => select(null)}>×</button>
      </header>

      {#if selected.kind === "file" || selected.kind === "hub"}
        <dl class="meta">
          {#if folderOf && folderOf !== selected.area}
            <dt>Folder</dt><dd><button type="button" class="linkish" onclick={openFolder}>{folderOf}</button></dd>
          {/if}
          <dt>Size</dt><dd>{formatBytes(selected.bytes)}</dd>
          <dt>Modified</dt><dd>{relativeTime(selected.modified)} <span class="dim">· {absoluteTime(selected.modified)}</span></dd>
          <dt>Path</dt><dd class="mono">{selected.path}</dd>
          <dt>Links</dt><dd>{linksOut.length} out · {linksIn.length} in</dd>
        </dl>
        <div class="preview" class:empty={previewState !== "idle" || !preview}>
          {#if previewState === "loading"}
            <span class="dim">Loading preview…</span>
          {:else if previewState === "none"}
            <span class="dim">No preview for this file type.</span>
          {:else if previewState === "error"}
            <span class="dim">Preview unavailable.</span>
          {:else if preview}
            {preview.text || "(empty file)"}
          {/if}
        </div>
        <div class="actions">
          <button type="button" class="primary" onclick={() => viewFile(selected!.path!)}>Open</button>
          <button type="button" onclick={() => copyPath(selected!.path!)}>Copy path</button>
          <button type="button" onclick={() => flyTo(selected!)}>Fly to</button>
        </div>
      {:else if selected.kind === "area"}
        <dl class="meta">
          <dt>Notes</dt><dd>{areaFiles.length}</dd>
          <dt>Folders</dt><dd>{areaGroups.length}</dd>
        </dl>
        <div class="actions">
          <button type="button" class="primary" onclick={() => flyTo(selected!)}>Fly to</button>
        </div>
        <h3>Notes in this area ({areaFiles.length})</h3>
        {#if areaFiles.length > 30}
          <input type="search" class="filter" placeholder="Filter…" aria-label="Filter notes" bind:value={areaFilter} />
        {/if}
        <div class="area-list">
          {#each areaGroups as [sub, nodes] (sub)}
            {#if sub}<h4>{sub} <span class="dim">({nodes.length})</span></h4>{/if}
            <ul class="links">
              {#each nodes as f (f.id)}
                <li>
                  <button type="button" class="link" onclick={() => select(f, true)}>
                    <span class="dot" style:background={f.color}></span>
                    <span class="txt">{f.label}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/each}
        </div>
      {:else if selected.kind === "skill"}
        <p class="body">{graph?.skills.find((s) => `/${s.name}` === selected!.label)?.description ?? ""}</p>
        <div class="actions">
          <button type="button" class="primary" disabled={busy} onclick={() => runSkill(selected!.label.slice(1))}>▶ Run</button>
          <button type="button" onclick={() => flyTo(selected!)}>Fly to</button>
        </div>
      {:else if selected.kind === "routine"}
        {@const r = graph?.routines.find((x) => `routine:${x.id}` === selected!.id)}
        {#if r}
          <dl class="meta">
            <dt>Cron</dt><dd class="mono">{r.cron_expr}</dd>
            <dt>Target</dt><dd>{r.target.type}: {r.target.value}</dd>
            <dt>Next</dt><dd>{r.enabled ? untilTime(r.next_fire_at) : "—"}</dd>
            <dt>Last</dt><dd>{relativeTime(r.last_fired_at)}</dd>
          </dl>
        {/if}
        <div class="actions">
          <button type="button" class="primary" disabled={busy} onclick={() => toggleRoutine(selected!)}>{selected.enabled ? "Disable" : "Enable"}</button>
          <button type="button" onclick={() => flyTo(selected!)}>Fly to</button>
        </div>
      {/if}

      {#if selected.kind !== "area"}
        <h3>Links to ({linksOut.length})</h3>
        {#if linksOut.length === 0}<p class="dim small">No links yet.</p>{/if}
        <ul class="links">
          {#each linksOut as l (l.node.id)}
            <li><button type="button" class="link" onclick={() => select(l.node, true)}><span class="dot" style:background={l.node.color}></span><span class="txt">{l.node.label}</span></button></li>
          {/each}
        </ul>
        <h3>Linked from ({linksIn.length})</h3>
        {#if linksIn.length === 0}<p class="dim small">No links yet.</p>{/if}
        <ul class="links">
          {#each linksIn as l (l.node.id)}
            <li><button type="button" class="link" onclick={() => select(l.node, true)}><span class="dot" style:background={l.node.color}></span><span class="txt">{l.node.label}</span></button></li>
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
    width: 270px;
    max-height: calc(100vh - 100px);
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
    padding: var(--ax-space-3);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-pop);
    font-size: var(--ax-font-size-sm);
  }
  .controls-head {
    display: flex;
    gap: var(--ax-space-2);
  }
  .controls-head input {
    flex: 1 1 auto;
    min-width: 0;
  }
  .help-btn {
    width: 30px;
    padding: 0;
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
  }
  .help-btn.on {
    color: var(--ax-accent);
    border-color: var(--ax-accent);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .label {
    font-size: var(--ax-font-size-xs);
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
  .readout {
    min-width: 44px;
    text-align: right;
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }
  .row button {
    flex: 1 1 0;
    font-size: var(--ax-font-size-sm);
  }
  .stats {
    margin: 0;
    color: var(--ax-text-muted);
  }
  .help {
    margin: 0;
    padding-top: var(--ax-space-2);
    border-top: 1px solid var(--ax-border);
    display: grid;
    grid-template-columns: 5.5em 1fr;
    gap: var(--ax-space-1) var(--ax-space-2);
    font-size: var(--ax-font-size-xs);
    line-height: 1.5;
  }
  .help dt {
    color: var(--ax-accent);
    font-weight: 600;
  }
  .help dd {
    margin: 0;
    color: var(--ax-text-muted);
  }

  .legend-slot {
    position: absolute;
    z-index: 2;
    left: var(--ax-space-5);
    bottom: 80px;
  }

  /* ---- detail panel ---- */
  .detail {
    position: absolute;
    z-index: 2;
    top: 72px;
    left: var(--ax-space-5);
    width: 360px;
    max-height: calc(100vh - 100px);
    overflow: auto;
    padding: var(--ax-space-4) var(--ax-space-5) var(--ax-space-5);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-pop);
    font-size: var(--ax-font-size-base);
    line-height: 1.6;
  }
  .detail header {
    position: relative;
    padding-right: var(--ax-space-6);
    margin-bottom: var(--ax-space-3);
  }
  .eyebrow {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
    margin-bottom: var(--ax-space-2);
  }
  .detail h2 {
    font-size: var(--ax-font-size-xl);
    font-family: var(--ax-font-display);
    line-height: 1.25;
    word-break: break-word;
  }
  .close {
    position: absolute;
    top: 0;
    right: 0;
    width: 26px;
    height: 26px;
    padding: 0;
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-lg);
  }
  .tag {
    padding: 1px var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    border: 1px solid var(--ax-accent);
    color: var(--ax-accent);
    font-size: var(--ax-font-size-xs);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .tag.kind {
    background: var(--ax-accent);
    color: var(--ax-text-invert);
  }
  .tag.area {
    --chip: var(--ax-accent);
    border-color: var(--chip);
    color: var(--chip);
    background: color-mix(in srgb, var(--chip) 18%, transparent);
    text-transform: none;
    letter-spacing: 0;
    cursor: pointer;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--ax-space-2) var(--ax-space-4);
    margin: 0 0 var(--ax-space-4);
    font-size: var(--ax-font-size-sm);
  }
  .meta dt {
    color: var(--ax-text-muted);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    font-size: var(--ax-font-size-xs);
    padding-top: 2px;
  }
  .meta dd {
    margin: 0;
    min-width: 0;
    word-break: break-word;
  }
  .mono {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
  }
  .dim {
    color: var(--ax-text-muted);
  }
  .small {
    font-size: var(--ax-font-size-sm);
    margin: 0 0 var(--ax-space-2);
  }
  .linkish {
    padding: 0;
    background: transparent;
    border: none;
    color: var(--ax-accent);
    font: inherit;
    cursor: pointer;
  }

  .preview {
    max-height: 190px;
    overflow: hidden;
    margin: 0 0 var(--ax-space-4);
    padding: var(--ax-space-3) var(--ax-space-4);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    font-size: var(--ax-font-size-sm);
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
    mask-image: linear-gradient(to bottom, #000 78%, transparent);
  }
  .preview.empty {
    mask-image: none;
    color: var(--ax-text-muted);
  }
  .body {
    margin: 0 0 var(--ax-space-3);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-2);
    margin-bottom: var(--ax-space-4);
  }
  .actions button {
    padding: var(--ax-space-1) var(--ax-space-3);
    border-radius: var(--ax-radius-pill);
  }
  .actions .primary {
    background: var(--ax-accent);
    border-color: var(--ax-accent);
    color: var(--ax-text-invert);
    font-weight: 600;
  }
  .actions .primary:hover:not(:disabled) {
    background: var(--ax-accent-hover);
  }

  .detail h3 {
    margin: var(--ax-space-3) 0 var(--ax-space-1);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }
  .detail h4 {
    margin: var(--ax-space-2) 0 var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    color: var(--ax-text);
  }
  .filter {
    width: 100%;
    margin-bottom: var(--ax-space-2);
  }
  .area-list {
    max-height: 40vh;
    overflow: auto;
  }
  .links {
    list-style: none;
    margin: 0;
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
    padding: var(--ax-space-1) var(--ax-space-2);
    border-radius: var(--ax-radius-sm);
  }
  .link:hover {
    background: var(--ax-surface-3);
  }
  .link .txt {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: 0 0 auto;
  }
</style>
