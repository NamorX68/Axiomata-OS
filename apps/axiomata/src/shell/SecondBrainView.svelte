<!--
  Full-screen Second Brain: the graph with pan (drag) / zoom (wheel), hover
  labels, layout Rings / Hex, grouping
  by areas or folders, spin + file-name toggles, and a detail panel for the
  selected node (file → open in the file panel / copy path / fly to / connections;
  skill → run; hub → open). "Back to the OS" closes.
  Hex tiles every note into its own honeycomb cell, same area wedges as
  Rings. Orbit (the dashboard-centre widget's 3-D cloud) lives only
  there now, not as a full-view option here.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { get } from "svelte/store";
  import { fade } from "svelte/transition";

  import { invokeBackend, type RunSummary, type WorkspaceGraph } from "../core/backend";
  import { absoluteTime, formatBytes, relativeTime } from "../core/format";
  import { BRAIN_DISC, brainView, spinOf } from "../core/brainView";
  import { getSetting, setSetting } from "../core/persist";
  import { openFilePanel, staged } from "../core/staging";
  import { toast } from "../core/toast";
  import { applyLayout, type LayoutKind } from "../graph/layout";
  import {
    buildModel,
    neighbours,
    readPalette,
    regroup,
    type GraphModel,
    type GraphNode,
    type Grouping,
  } from "../graph/model";
  import { draggable, type DragDelta } from "../canvas/drag";
  import { resizable, type ResizeDelta } from "../canvas/resize";
  import { uiScale } from "../core/uiScale";
  import FilePeek from "../fileapp/FilePeek.svelte";
  import { clampRect, moved, readRect, resizedEdge, type Edge, type Rect } from "./floatingRect";
  import Legend from "../graph/Legend.svelte";
  import { GraphRenderer, type RenderMode } from "../graph/render";

  let {
    open = $bindable(false),
    focus = null,
  }: { open?: boolean; focus?: string | null } = $props();

  interface Prefs {
    layout?: LayoutKind;
    grouping?: Grouping;
    help?: boolean;
  }
  // The detail window: movable by its head, resizable from its four edges, remembered (B8 of `docs/plans/orbit-brain.md`).
  // Unscaled units, drawn times the UI scale.
  const DETAIL_KEY = "brainDetail";
  const DEFAULT_DETAIL: Rect = { x: 24, y: 72, w: 380, h: 520 };
  const EDGES: Edge[] = ["n", "e", "s", "w"];
  const screenSize = () => ({ w: window.innerWidth / get(uiScale), h: window.innerHeight / get(uiScale) });
  let detailRect = $state<Rect>(clampRect(readRect(getSetting<unknown>(DETAIL_KEY)) ?? DEFAULT_DETAIL, screenSize()));
  let detailBase: Rect = { ...DEFAULT_DETAIL };
  const startPull = () => (detailBase = detailRect);
  const saveDetail = () => setSetting(DETAIL_KEY, detailRect);
  function moveDetail(d: DragDelta) {
    const s = get(uiScale);
    detailRect = clampRect(moved(detailBase, d.dx / s, d.dy / s), screenSize());
  }
  function pullEdge(edge: Edge, d: ResizeDelta) {
    const grow = edge === "e" || edge === "w" ? d.dw : d.dh;
    detailRect = clampRect(resizedEdge(detailBase, edge, grow / get(uiScale)), screenSize());
  }

  /** The graph's outermost radius is the disc's edge: the renderer's `fit` is the share of the canvas (the disc's own
   *  square) that one graph unit takes. */
  const DISC_FIT = 0.5;

  /**
   * The disc lives inside the Orbit's own container (`[data-background]`), moved there on mount: it then has the cloud's box —
   * the centre and the size units are the very same, by CSS, with nothing measured that could go stale — and it stands in
   * the particle layer, under the tiles like the cloud it replaces. Without an Orbit it is centred on the window.
   */
  let inOrbit = $state(false);
  function inOrbitHost(node: HTMLElement) {
    const host = document.querySelector("[data-background]");
    if (host) {
      host.appendChild(node);
      inOrbit = true;
    }
    return { destroy: () => node.remove() };
  }
  const prefs = getSetting<Prefs>("secondBrain") ?? {};

  let canvas = $state<HTMLCanvasElement | null>(null);
  let renderer = $state.raw<GraphRenderer | null>(null);
  let graph = $state<WorkspaceGraph | null>(null);
  let model = $state<GraphModel | null>(null);
  // `$state.raw`, not `$state`: these must stay the exact same object
  // references that live in `renderer.model.nodes` — the renderer decides
  // its hover/selection glow with `n === this.hover` / `n === this.selected`,
  // and a plain `$state` would proxy-wrap whatever is assigned, breaking
  // that comparison silently (property reads like `selected.label` still
  // work fine through a proxy, which is why this went unnoticed).
  let selected = $state.raw<GraphNode | null>(null);
  let hover = $state.raw<GraphNode | null>(null);
  let layout = $state<LayoutKind>(prefs.layout === "hex" ? "hex" : "rings");
  // The one place the layout is mapped to a renderer mode, so the mount-time renderer options and the reactive
  // `$effect` below can't drift apart.
  const renderMode = $derived<RenderMode>(layout === "hex" ? "hex" : "rings");
  let grouping = $state<Grouping>(prefs.grouping === "folders" ? "folders" : "areas");
  let helpOpen = $state(prefs.help === true);
  let areaFilter = $state("");

  const HELP = [
    ["Rings", "Notes sit on arcs inside their area's segment; skills inside, areas on the next ring. Shows the size of each area."],
    [
      "Hex",
      "Honeycomb instead of arcs: every note its own hex cell, packed tight, in the same area segment as in Rings. Skills and areas stay as in Rings.",
    ],
    ["Areas", "One segment per top-level vault folder."],
    ["Folders", "One segment per deepest folder, e.g. Learning/Rust/lessons. A finer split of large areas."],
  ] as const;
  /** Looks up a HELP entry's text by its term, so the buttons below reference
   *  entries by name instead of a fragile array index. */
  function helpText(term: string): string {
    return HELP.find(([t]) => t === term)?.[1] ?? "";
  }
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

  function rebuild() {
    if (!renderer || !graph) return;
    const palette = readPalette();
    const m = buildModel(regroup(graph, grouping), palette);
    applyLayout(m, layout);
    renderer.setColors(palette.text, palette.muted, palette.border, palette.invert, palette.invert, palette.surface, palette.accent, palette.light);
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
        focusExternal(model.byId.get(focus) ?? null);
      }
    } catch (err) {
      error = String(err);
    }
  }

  /** Re-resolves `node` through the current `model.byId` before it's
   *  handed to the renderer. Search results (`searchNodes`), an area's
   *  file list, and linked-note chips are all `$derived` from `model`, but
   *  a node captured from one of those lists can still end up a stale
   *  object once `rebuild()` has since swapped in a new `model` (a fresh
   *  `GraphModel` — new node objects, same ids) — e.g. from a `layout`/
   *  `grouping` change in between. `renderer.selected` is matched against
   *  the *current* model's nodes by reference (`n === this.selected`) every
   *  frame, so a stale reference never matches anything and silently never
   *  highlights, even though its `x`/`y` still look plausible. Confirmed
   *  live: a search-result click's node failed this exact identity check
   *  against `renderer.model.byId.get(node.id)`. Resolving by id here
   *  fixes it regardless of why the two model instances diverged. */
  function resolve(node: GraphNode): GraphNode {
    return model?.byId.get(node.id) ?? node;
  }

  /** Selects `node` for an external "jump here" trigger — the dashboard's
   *  Second Brain background widget, `/brain <path>`. Always marks it (the
   *  same highlight ring any selection gets); only pans the camera if it
   *  isn't already on screen. With the graph's extent normally fitted to
   *  the canvas, that's the common case — an unconditional `centerOn` used
   *  to re-centre (and rezoom-at-current-zoom) on every one of these,
   *  which owner feedback flagged as a jarring shift for a node that was
   *  already perfectly visible. A genuinely off-screen target (a much
   *  bigger graph than fits today) still gets centred, just not zoomed. */
  function focusExternal(node: GraphNode | null) {
    const resolved = node ? resolve(node) : null;
    select(resolved);
    if (resolved && renderer && !renderer.isOnScreen(resolved)) {
      renderer.centerOn(resolved);
    }
  }

  function select(node: GraphNode | null) {
    const resolved = node ? resolve(node) : null;
    selected = resolved;
    confirmDelete = false; // never carry a pending "delete?" to another node
    if (renderer) renderer.selected = resolved;
  }

  /** Selects and animates to `node` — the "click a node reference from
   *  somewhere else in the UI" action (a search result, a file in an
   *  area's list, a linked-note chip). Unlike `focusExternal`'s instant,
   *  only-if-off-screen `centerOn` (for a jump *into* the view from outside
   *  it, where an animation would be a surprise nobody asked for), this is
   *  a navigation the owner just explicitly triggered from within an
   *  already-open view, so it gets the same eased pan/zoom + landing pulse
   *  as the "Fly to" button — and, since `select` always sets
   *  `renderer.selected` first, the same highlight ring a direct canvas
   *  click gets too. */
  function goTo(node: GraphNode) {
    const resolved = resolve(node);
    select(resolved);
    renderer?.flyTo(resolved);
  }

  function flyTo(node: GraphNode) {
    renderer?.flyTo(node);
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

  function viewFile(path: string) {
    openFilePanel(path, "read");
  }

  // --- delete (two-step, destructive) ---
  let confirmDelete = $state(false);
  let deleting = $state(false);

  async function deleteFile(path: string) {
    if (deleting) return;
    deleting = true;
    try {
      await invokeBackend("delete_workspace_file", { rel: path });
      toast("File deleted.");
      select(null); // also clears confirmDelete
      await load(); // rebuild the graph without the deleted node
    } catch (err) {
      toast(`Deleting failed: ${err}`, "warning");
    } finally {
      deleting = false;
    }
  }

  async function copyPath(path: string) {
    // `selected.path` is workspace-relative; the clipboard should carry the
    // resolvable absolute path (owner feedback: it only ever copied a bare
    // file name for root-level notes).
    const absolute = graph && path ? `${graph.workspace_root.replace(/\/+$/, "")}/${path}` : path;
    try {
      await navigator.clipboard.writeText(absolute);
      toast("Path copied.");
    } catch {
      toast(absolute);
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
    if (renderer) {
      renderer.options = {
        ...renderer.options,
        spin: spinOf($brainView),
        labels: $brainView.labels,
        fileLabels: $brainView.fileNames,
        mode: renderMode,
      };
    }
  });
  // Remember the view preferences in dashboard.json (settings.secondBrain).
  let prefsReady = false;
  $effect(() => {
    const next: Prefs = { layout, grouping, help: helpOpen };
    if (prefsReady) setSetting("secondBrain", next);
    prefsReady = true;
  });

  function openFolder() {
    if (!folderOf) return;
    grouping = "folders";
    // The regroup happens in the layout effect; select the folder node after it.
    queueMicrotask(() => {
      const n = model?.byId.get(`area:${folderOf}`);
      if (n) focusExternal(n);
    });
  }
  $effect(() => {
    void layout;
    void grouping;
    untrack(rebuild);
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
      if (node) focusExternal(node);
    }
  });

  onMount(() => {
    if (!canvas) return;
    renderer = new GraphRenderer(canvas);
    renderer.options = {
      spin: spinOf($brainView),
      labels: $brainView.labels,
      fileLabels: $brainView.fileNames,
      fit: DISC_FIT,
      mode: renderMode,
    };
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
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="stage"
    style:--disc-vmin="{BRAIN_DISC * 200}vmin"
    style:--disc-cqmin="{BRAIN_DISC * 200}cqmin"
    class:in-orbit={inOrbit}
    use:inOrbitHost
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

  <aside class="controls" class:away={$staged.length > 0}>
    <div class="group">
      <span class="label">Layout</span>
      <div class="seg">
        <button type="button" class:on={layout === "rings"} title={helpText("Rings")} aria-describedby="help-rings" onclick={() => (layout = "rings")}>Rings</button>
        <button type="button" class:on={layout === "hex"} title={helpText("Hex")} aria-describedby="help-hex" onclick={() => (layout = "hex")}>Hex</button>
      </div>
    </div>
    <div class="group">
      <span class="label">Group by</span>
      <div class="seg">
        <button type="button" class:on={grouping === "areas"} title={helpText("Areas")} aria-describedby="help-areas" onclick={() => (grouping = "areas")}>Areas</button>
        <button type="button" class:on={grouping === "folders"} title={helpText("Folders")} aria-describedby="help-folders" onclick={() => (grouping = "folders")}>Folders</button>
      </div>
    </div>
    <div class="row">
      <button type="button" title="Reset zoom and pan" onclick={resetView}>Reset view</button>
      <button type="button" title="Reload the graph from the workspace" onclick={() => void load()}>Reload</button>
    </div>
    <button type="button" class="help-btn" class:on={helpOpen} title="What do the options mean?" aria-label="Help" onclick={() => (helpOpen = !helpOpen)}>?</button>
    {#if model}
      <p class="stats">{model.totalFiles} notes · {model.areas.length} {grouping} · {model.edges.length} links{model.truncated ? " · truncated" : ""}</p>
    {/if}
    {#if helpOpen}
      <div class="help">
        <Legend hex={layout === "hex"} />
        <dl class="help-terms">
          {#each HELP as [term, text] (term)}
            <dt id="help-{term.toLowerCase().replace(' ', '-')}">{term}</dt>
            <dd>{text}</dd>
          {/each}
        </dl>
      </div>
    {/if}
  </aside>

  {#if selected}
    <aside
      class="detail"
      transition:fade={{ duration: 120 }}
      style:left="{detailRect.x * $uiScale}px"
      style:top="{detailRect.y * $uiScale}px"
      style:width="{detailRect.w * $uiScale}px"
      style:height="{detailRect.h * $uiScale}px"
      use:draggable={{ handle: ".detail-head", onStart: startPull, onMove: moveDetail, onEnd: (d) => { moveDetail(d); saveDetail(); } }}
    >
      {#each EDGES as edge (edge)}
        <div
          class="edge edge-{edge}"
          use:resizable={{ dir: edge, onStart: startPull, onMove: (d) => pullEdge(edge, d), onEnd: (d) => { pullEdge(edge, d); saveDetail(); } }}
        ></div>
      {/each}
      <div class="detail-scroll">
      <header class="detail-head">
        <div class="eyebrow">
          <span class="tag kind">{selected.kind}</span>
          {#if selected.area && selected.kind !== "area"}
            <button type="button" class="tag area" style:--chip={selected.color} onclick={() => areaNode && goTo(areaNode)}>{selected.area}</button>
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
        <div class="preview">
          {#key selected.path}
            <FilePeek root="workspace" rel={selected.path!} />
          {/key}
        </div>
        <div class="actions">
          <button type="button" class="primary" onclick={() => viewFile(selected!.path!)}>Open</button>
          <button type="button" onclick={() => copyPath(selected!.path!)}>Copy path</button>
          <button type="button" onclick={() => flyTo(selected!)}>Fly to</button>
          {#if confirmDelete}
            <button type="button" class="danger" disabled={deleting} onclick={() => deleteFile(selected!.path!)}>
              {deleting ? "Deleting…" : "Really delete"}
            </button>
            <button type="button" disabled={deleting} onclick={() => (confirmDelete = false)}>Cancel</button>
          {:else}
            <button type="button" class="danger-ghost" onclick={() => (confirmDelete = true)}>Delete</button>
          {/if}
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
                  <button type="button" class="link" onclick={() => goTo(f)}>
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
      {/if}

      {#if selected.kind !== "area"}
        <h3>Links to ({linksOut.length})</h3>
        {#if linksOut.length === 0}<p class="dim small">No links yet.</p>{/if}
        <ul class="links">
          {#each linksOut as l (l.node.id)}
            <li><button type="button" class="link" onclick={() => goTo(l.node)}><span class="dot" style:background={l.node.color}></span><span class="txt">{l.node.label}</span></button></li>
          {/each}
        </ul>
        <h3>Linked from ({linksIn.length})</h3>
        {#if linksIn.length === 0}<p class="dim small">No links yet.</p>{/if}
        <ul class="links">
          {#each linksIn as l (l.node.id)}
            <li><button type="button" class="link" onclick={() => goTo(l.node)}><span class="dot" style:background={l.node.color}></span><span class="txt">{l.node.label}</span></button></li>
          {/each}
        </ul>
      {/if}
      </div>
    </aside>
  {/if}
</section>

<style>
  /* No box of its own: the layer is the disc below and the panels, each fixed on its own, so that the tiles of the Orbit
     stand between the disc (below them) and the panels (above them). */
  .brain {
    display: contents;
    color: var(--ax-text);
  }

  /* The graph sits in a disc over the cloud it replaces, and under the tiles. Radius `BRAIN_DISC` of the shorter side:
     big enough to cover the cloud (0.36), small enough to stay inside the App Ring (0.425). Zoom and pan are clipped to it. */
  .stage {
    position: fixed;
    z-index: 2;
    left: 50%;
    top: 50%;
    width: var(--disc-vmin);
    aspect-ratio: 1;
    transform: translate(-50%, -50%);
    border-radius: 50%;
    /* `border-radius` only rounds what is painted: WebKit still hands the element's whole box the pointer events, so the
       corners of the square would swallow the clicks of the App Ring's icons lying diagonally outside the circle. The clip
       path makes the circle the element's shape for hit testing too. */
    clip-path: circle(50%);
    overflow: hidden;
    background: var(--ax-bg);
    box-shadow: 0 0 0 1px var(--ax-border);
    cursor: grab;
  }
  /* In the Orbit's container (`container-type: size` there): its own box is the unit, the same one the cloud is sized from. */
  .stage.in-orbit {
    position: absolute;
    width: var(--disc-cqmin);
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

  /* The bar bottom left, beside the Orbit's own corner buttons (motion, ×). */
  .controls {
    position: fixed;
    z-index: calc(var(--ax-z-staging) - 2);
    left: calc(var(--ax-space-3) + calc(64px * var(--ax-ui-scale)));
    bottom: var(--ax-space-3);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-3);
    /* Ends before the assistant bar, which is centred over the same strip: the two used to overlap (the bar clipped the
       note count). The row wraps instead; where there is no room left beside the bar (a narrow window), see below. */
    max-width: max(
      calc(50vw - var(--ax-assistant-width) / 2 - var(--ax-space-3) - calc(64px * var(--ax-ui-scale)) - var(--ax-space-4)),
      calc(240px * var(--ax-ui-scale))
    );
    padding: var(--ax-space-2) var(--ax-space-3);
    transition: opacity var(--ax-dur-med) var(--ax-ease);
    /* Same glass/hairline/elevated-shadow language as Window.svelte and
       every other panel in the app. */
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border: none;
    border-bottom: 2px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-drag);
    font-size: var(--ax-font-size-sm);
  }
  /* A window lies over the Orbit (the Kanban, a file, a card): these controls belong to the Orbit and step back. */
  .controls.away {
    opacity: 0;
    pointer-events: none;
  }
  /* Narrower than the assistant bar plus the controls: sit above the bar instead of beside it. */
  @media (max-width: 1320px) {
    .controls {
      bottom: calc(var(--ax-space-3) + 64px * var(--ax-ui-scale));
      max-width: calc(100vw - 2 * var(--ax-space-5) - calc(64px * var(--ax-ui-scale)));
    }
  }
  .help-btn {
    width: calc(30px * var(--ax-ui-scale));
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
    align-items: center;
    gap: var(--ax-space-2);
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
  .row button {
    font-size: var(--ax-font-size-sm);
  }
  .stats {
    margin: 0;
    color: var(--ax-text-muted);
  }
  /* The explanations open above the bar. */
  .help {
    position: absolute;
    left: 0;
    bottom: calc(100% + var(--ax-space-2));
    width: calc(380px * var(--ax-ui-scale));
    max-height: 50vh;
    overflow: auto;
    margin: 0;
    padding: var(--ax-space-3);
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border-bottom: 2px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-drag);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
  }
  .help-terms {
    margin: 0;
    display: grid;
    grid-template-columns: 5.5em 1fr;
    gap: var(--ax-space-1) var(--ax-space-2);
    font-size: var(--ax-font-size-xs);
    line-height: 1.5;
  }
  .help-terms dt {
    color: var(--ax-accent);
    font-weight: 600;
  }
  .help-terms dd {
    margin: 0;
    color: var(--ax-text-muted);
  }

  /* ---- detail panel ---- */
  .detail {
    position: fixed;
    z-index: calc(var(--ax-z-staging) - 2);
    /* Same glass/hairline/elevated-shadow language as Window.svelte and
       every other panel in the app. */
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border: none;
    border-bottom: 2px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-drag);
    font-size: var(--ax-font-size-base);
    line-height: 1.6;
  }
  /* Thin grips along the four edges. They belong to the window, not to its scrolling content (`.detail-scroll`). */
  .edge {
    position: absolute;
    z-index: 3;
  }
  .edge-n,
  .edge-s {
    left: 0;
    right: 0;
    height: 6px;
    cursor: ns-resize;
  }
  .edge-e,
  .edge-w {
    top: 0;
    bottom: 0;
    width: 6px;
    cursor: ew-resize;
  }
  .edge-n { top: 0; }
  .edge-s { bottom: 0; }
  .edge-e { right: 0; }
  .edge-w { left: 0; }
  .detail-scroll {
    height: 100%;
    overflow: auto;
    padding: var(--ax-space-4) var(--ax-space-5) var(--ax-space-5);
  }
  .detail-head {
    cursor: grab;
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
  /* Hover/focus-reveal like Window.svelte and canvas/Tile.svelte's front
     face — was permanently visible before, which is also why the panel
     read as an unchanged classic card despite the glass background below
     already being in place. */
  .close {
    position: absolute;
    top: 0;
    right: 0;
    width: calc(26px * var(--ax-ui-scale));
    height: calc(26px * var(--ax-ui-scale));
    padding: 0;
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-lg);
    opacity: 0;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }
  .detail:hover .close,
  .detail:focus-within .close {
    opacity: 1;
  }
  .close:hover:not(:disabled) {
    color: var(--ax-text);
    background: var(--ax-surface-3);
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

  /* W5: the file as the editor shows it — a fixed window onto its start, never the whole panel. */
  .preview {
    height: calc(260px * var(--ax-ui-scale));
    overflow: hidden;
    margin: 0 0 var(--ax-space-4);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
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
  .actions .danger-ghost {
    color: var(--ax-danger);
    border-color: var(--ax-danger);
  }
  .actions .danger {
    background: var(--ax-danger);
    border-color: var(--ax-danger);
    color: var(--ax-text-invert);
    font-weight: 600;
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
