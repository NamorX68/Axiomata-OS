<!--
  Full-screen Second Brain: the graph with pan (drag) / zoom (wheel), hover
  labels, layout Bundle / Map / Hex (the app-wide `brainView.layout`, also in the settings), grouping
  by areas or folders, spin + file-name toggles, and a detail panel for the
  selected node (file → open in the file panel / copy path / fly to / connections;
  skill → run; hub → open). "Back to the OS" closes.
  Bundle (`docs/plans/brain-views.md`, BV1) puts the notes on one ring, an arc per area, and bundles their links
  through the middle along the vault's folders. Map (UX5, G16) draws the vault as a territory map: every note a field,
  every area a country, links as roads, with a semantic zoom. Hex (the layout before the map, back as a choice) tiles
  every note as a honeycomb cell in its area's wedge. All three follow the Orbit's time travel.
  A filter field finds notes by name, path or `#tag` (`docs/plans/tags-links.md` Q5) and dims everything else; a
  note's tags are chips in its detail panel, and a chip sets the filter. `/brain ?…` and a tag chosen in the spotlight
  open the view with the filter set.
  "▶ How your vault grew" (BV2) replays the vault's life over the chosen layout in a fixed time, with its own bar
  (pause, a slider, Esc or a click ends it), and fades into the layout at its end.
  Orbit (the dashboard-centre widget's 3-D cloud) lives only
  there now, not as a full-view option here.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { get } from "svelte/store";
  import { fade } from "svelte/transition";

  import { invokeBackend, type RunSummary, type WorkspaceGraph } from "../core/backend";
  import { absoluteTime, formatBytes, formatDate, relativeTime } from "../core/format";
  import { BRAIN_DISC, BRAIN_LAYOUTS, brainView, spinOf, travelAt, type BrainLayout } from "../core/brainView";
  import { animates, motion } from "../core/motion";
  import { getSetting, setSetting } from "../core/persist";
  import { openFilePanel, staged } from "../core/staging";
  import { toast } from "../core/toast";
  import { applyLayout } from "../graph/layout";
  import {
    buildModel,
    modelTags,
    neighbours,
    readPalette,
    regroup,
    searchNodes,
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
  import { travelSpan } from "../graph/hive";
  import { FRESH_LINK_DAYS } from "../graph/bundle";
  import { countBornBy, GROWTH_FADE_MS, GROWTH_MS, growthMoment } from "../graph/growth";
  import TimeTravel from "../modules/TimeTravel.svelte";
  import { GraphRenderer, type RenderMode } from "../graph/render";
  import { registerShellView } from "../core/openViews";

  let {
    open = $bindable(false),
    focus = null,
    query = null,
  }: { open?: boolean; focus?: string | null; query?: { text: string; seq: number } | null } = $props();

  // Known as open to the chat's `close_view` (CH5); the graph loses nothing by closing — its preferences are settings.
  $effect(() =>
    open ? registerShellView("second-brain", { title: "Second Brain", close: () => (open = false) }) : undefined,
  );

  /** The view's own preferences; the layout is a setting of its own now (`brainView.layout`), read from here once. */
  interface Prefs {
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
  /** From this share of the lens's radius on the graph thins out into the Orbit (`RenderOptions.fadeFrom`); the map's
   *  skills ring sits inside it (`layout.ts` `MAP_SKILLS`). */
  const LENS_FADE = 0.9;

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
  // One setting with the app settings' "Layout" (`core/brainView.ts`), so a choice in either place shows in both.
  const layout = $derived($brainView.layout);
  const setLayout = (id: BrainLayout) => brainView.update((v) => ({ ...v, layout: id }));
  // Each layout has a renderer mode of the same name; spelled out so the mount-time options and the reactive
  // `$effect` below can't drift apart.
  const renderMode = $derived<RenderMode>(layout);
  let grouping = $state<Grouping>(prefs.grouping === "folders" ? "folders" : "areas");
  let helpOpen = $state(prefs.help === true);
  let areaFilter = $state("");
  /** The view's own filter: words and `#tag`s, every one has to match (`searchNodes`); everything else dims. */
  let filter = $state("");
  const filterHits = $derived(model && filter.trim() ? searchNodes(model, filter) : null);
  const allTags = $derived(model ? modelTags(model) : []);
  $effect(() => {
    const hits = filterHits;
    if (renderer) renderer.highlight = hits;
  });
  /** Shows the notes tagged `tag` (a chip in the detail panel). */
  function filterByTag(tag: string) {
    filter = `#${tag}`;
  }

  const HELP = [
    [
      "Bundle",
      `Every note on one ring, an arc per area, the newest first, clockwise. Links run through the middle, bundled along the folders, in the colour of the note they start from; the links of the last ${FRESH_LINK_DAYS} days glow. Point at a note or an area to see its links. Skills ring it outside.`,
    ],
    [
      "Map",
      "A map of the vault: every note a field, every area a country, the oldest notes at its heart. Links are roads (zoom in); close up every field shows its name. Skills ring the map.",
    ],
    [
      "Hex",
      "A honeycomb: every note its own hex cell, packed tight, each area in its own wedge, the newest nearest the middle. Skills and areas sit inside it.",
    ],
    ["Areas", "One segment per top-level vault folder."],
    [
      "Folders",
      "One segment per deepest folder, e.g. Learning/Rust/lessons. A finer split of large areas; in Bundle the links bundle along the folder tree.",
    ],
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
    renderer.setColors(
      palette.text,
      palette.muted,
      palette.border,
      palette.invert,
      palette.invert,
      palette.surface,
      palette.accent,
      palette.light,
      palette.glow,
    );
    renderer.model = m;
    model = m;
    // Re-point the selection at the new model's node without making the
    // callers' effects depend on `selected` (that would re-run them on
    // every select / deselect).
    const current = untrack(() => selected);
    selected = current ? (m.byId.get(current.id) ?? null) : null;
    renderer.selected = selected;
    const pointed = untrack(() => hover);
    hover = pointed ? (m.byId.get(pointed.id) ?? null) : null;
    renderer.hover = hover;
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
      if (growth) {
        endGrowth();
        return;
      }
      const p = rel(e);
      select(renderer.hitTest(p.x, p.y, 8));
    }
  }
  /** The pointer left the disc: nothing is pointed at any more (the bundle would keep the last note's links lit). */
  function onLeave() {
    drag = null;
    hover = null;
    if (renderer) renderer.hover = null;
  }
  function onWheel(e: WheelEvent) {
    if (!renderer) return;
    e.preventDefault();
    const view = renderer.view;
    const zoom = Math.min(8, Math.max(0.4, view.zoom * Math.exp(-e.deltaY * 0.0015)));
    // The point under the pointer stays put: zooming walks towards what is being looked at, as a map does.
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const px = e.clientX - rect.left - rect.width / 2;
    const py = e.clientY - rect.top - rect.height / 2;
    const k = zoom / view.zoom;
    view.x = px - (px - view.x) * k;
    view.y = py - (py - view.y) * k;
    view.zoom = zoom;
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
    if (growth) endGrowth();
    else if (selected) select(null);
    else open = false;
  }

  $effect(() => {
    if (renderer) {
      renderer.options = {
        ...renderer.options,
        spin: spinOf($brainView, $motion),
        labels: $brainView.labels,
        fileLabels: $brainView.fileNames,
        mode: renderMode,
        pulse: animates($motion),
        shine: $brainView.shine,
      };
    }
  });
  $effect(() => {
    const at = $travelAt;
    if (renderer) renderer.timeCutoff = at;
  });
  /** The vault's life for the time travel when no Orbit is there to offer it. */
  const span = $derived(graph ? travelSpan(graph.files.map((f) => f.created ?? f.modified), Date.now()) : null);

  // --- "How your vault grew" (docs/plans/brain-views.md, BV2) ---
  /**
   * The replay while it is up: how far it has played (ms of `GROWTH_MS`), whether it plays, and when it began to fade
   * into the layout (`performance.now()`), or `null` while it does not. The vault's life is fixed at its start.
   */
  interface Replay {
    span: { from: number; to: number };
    elapsed: number;
    playing: boolean;
    endedAt: number | null;
  }
  // Raw: replaced as a whole every frame, and read by the renderer thousands of times a frame.
  let growth = $state.raw<Replay | null>(null);
  /** At motion Off the replay does not play: it shows its end and only the slider to look back with (Q9). */
  const still = $derived($motion === "off");
  /** Every note's birth, sorted, for the count the bar shows. */
  const births = $derived(
    graph
      ? graph.files
          .map((f) => Date.parse(f.created ?? f.modified ?? ""))
          .filter((at) => !Number.isNaN(at))
          .sort((a, b) => a - b)
      : [],
  );
  const growthAt = $derived(growth ? growthMoment(growth.elapsed, growth.span) : 0);
  const growthLabel = $derived.by(() => {
    if (!growth || !graph) return "";
    const day = formatDate(growthAt, { day: "numeric", month: "short", year: "numeric" });
    // Notes without a time are there from the start.
    const count = countBornBy(births, growthAt) + (graph.files.length - births.length);
    return `${day} · ${count} ${count === 1 ? "note" : "notes"}`;
  });

  function startGrowth() {
    if (!span) return;
    hover = null;
    if (renderer) renderer.hover = null;
    growth = { span, elapsed: still ? GROWTH_MS : 0, playing: !still, endedAt: null };
  }

  /** Esc, a click or ×: the replay fades into the layout, or is simply gone when nothing may move. */
  function endGrowth() {
    if (!growth) return;
    if (still) growth = null;
    else if (growth.endedAt === null) growth = { ...growth, playing: false, endedAt: performance.now() };
  }

  function toggleGrowth() {
    if (!growth) return;
    // Played to its end: ▶ starts it again from the first note.
    const elapsed = growth.elapsed >= GROWTH_MS ? 0 : growth.elapsed;
    growth = { ...growth, elapsed, playing: !growth.playing, endedAt: null };
  }

  /** The slider moved: the replay holds there (▶ plays on), or the thumb would fight the pointer. */
  function scrubGrowth(value: number) {
    if (growth) growth = { ...growth, elapsed: value, playing: false, endedAt: null };
  }

  /** One frame of the replay: it plays on, at its end begins to fade, and is gone once faded; the renderer is told. */
  function stepGrowth(now: number, dt: number) {
    // Motion switched Off while it plays: straight to its end, as it would have begun.
    if (growth?.playing && still) growth = { ...growth, elapsed: GROWTH_MS, playing: false };
    if (growth?.playing) {
      const elapsed = Math.min(GROWTH_MS, growth.elapsed + dt);
      const done = elapsed >= GROWTH_MS;
      growth = { ...growth, elapsed, playing: !done, endedAt: done ? now : growth.endedAt };
    }
    let fade = 0;
    if (growth && growth.endedAt !== null) {
      // The end is stamped in an event handler, a little after this frame's `now`.
      fade = Math.max(0, Math.min(1, (now - growth.endedAt) / GROWTH_FADE_MS));
      if (fade >= 1) growth = null;
    }
    if (!renderer) return;
    renderer.growth = growth ? { span: growth.span, at: growthMoment(growth.elapsed, growth.span), fade } : null;
  }
  // Remember the view preferences in dashboard.json (settings.secondBrain).
  let prefsReady = false;
  $effect(() => {
    const next: Prefs = { grouping, help: helpOpen };
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
  // A filter handed in from outside (`/brain ?…`, a tag from the spotlight) replaces the one typed here — once per value.
  let lastQuery = 0;
  $effect(() => {
    const next = query;
    if (next && next.seq !== lastQuery) {
      lastQuery = next.seq;
      filter = next.text;
    }
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
      spin: spinOf($brainView, $motion),
      labels: $brainView.labels,
      fileLabels: $brainView.fileNames,
      fit: DISC_FIT,
      fadeFrom: LENS_FADE,
      mode: renderMode,
    };
    renderer.resize();
    const ro = new ResizeObserver(() => renderer?.resize());
    ro.observe(canvas);
    const mo = new MutationObserver(() => rebuild());
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    let raf = 0;
    let last = 0;
    const tick = (now: number) => {
      stepGrowth(now, last ? Math.min(100, now - last) : 0);
      last = now;
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
    onmouseleave={onLeave}
    onwheel={onWheel}
  >
    <canvas bind:this={canvas}></canvas>
    {#if error}<p class="error">{error}</p>{/if}
  </div>
  <!-- Over the Orbit its own time travel serves every layout; without one, the view brings its own. -->
  {#if !inOrbit && span && !growth}
    <div class="travel"><TimeTravel {span} onTravel={(at) => travelAt.set(at)} /></div>
  {/if}
  {#if growth}
    <div class="growth" class:away={$staged.length > 0} role="group" aria-label="How your vault grew" transition:fade={{ duration: 120 }}>
      {#if !still}
        <button type="button" class="play" aria-label={growth.playing ? "Pause" : "Play"} onclick={toggleGrowth}>{growth.playing ? "❚❚" : "▶"}</button>
      {/if}
      <input
        type="range"
        min="0"
        max={GROWTH_MS}
        step="10"
        value={growth.elapsed}
        aria-label="Moment of the replay"
        oninput={(e) => scrubGrowth(Number((e.currentTarget as HTMLInputElement).value))}
      />
      <span class="when">{growthLabel}</span>
      <button type="button" class="end" aria-label="End the replay" title="End (Esc)" onclick={endGrowth}>×</button>
    </div>
  {/if}

  <aside class="controls" class:away={$staged.length > 0}>
    <div class="group">
      <span class="label">Layout</span>
      <div class="seg">
        {#each BRAIN_LAYOUTS as l (l.id)}
          <button type="button" class:on={layout === l.id} title={helpText(l.label)} aria-describedby="help-{l.id}" onclick={() => setLayout(l.id)}>{l.label}</button>
        {/each}
      </div>
    </div>
    <div class="group">
      <span class="label">Group by</span>
      <div class="seg">
        <button type="button" class:on={grouping === "areas"} title={helpText("Areas")} aria-describedby="help-areas" onclick={() => (grouping = "areas")}>Areas</button>
        <button type="button" class:on={grouping === "folders"} title={helpText("Folders")} aria-describedby="help-folders" onclick={() => (grouping = "folders")}>Folders</button>
      </div>
    </div>
    <div class="group search">
      <input
        type="search"
        class="find"
        placeholder="Find notes or #tag"
        aria-label="Find notes by name or #tag"
        list="brain-tags"
        spellcheck="false"
        bind:value={filter}
      />
      <datalist id="brain-tags">
        {#each allTags as tag (tag)}<option value="#{tag}"></option>{/each}
      </datalist>
      {#if filterHits}
        <span class="label">{filterHits.size} {filterHits.size === 1 ? "match" : "matches"}</span>
      {/if}
    </div>
    <div class="row">
      <button type="button" title="Replay the vault's life from its first note, in twenty seconds" disabled={!span || growth !== null} onclick={startGrowth}>▶ How your vault grew</button>
      <button type="button" title="Reset zoom and pan" onclick={resetView}>Reset view</button>
      <button type="button" title="Reload the graph from the workspace" onclick={() => void load()}>Reload</button>
    </div>
    <button type="button" class="help-btn" class:on={helpOpen} title="What do the options mean?" aria-label="Help" onclick={() => (helpOpen = !helpOpen)}>?</button>
    {#if model}
      <p class="stats">{model.totalFiles} notes · {model.areas.length} {grouping} · {model.edges.length} links{model.truncated ? " · truncated" : ""}</p>
    {/if}
    {#if helpOpen}
      <div class="help">
        <Legend {layout} />
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
          {#if selected.tags && selected.tags.length > 0}
            <dt>Tags</dt>
            <dd class="chips">
              {#each selected.tags as tag (tag)}
                <button type="button" class="chip" title="Show the notes tagged #{tag}" onclick={() => filterByTag(tag)}>#{tag}</button>
              {/each}
            </dd>
          {/if}
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
    /* No hard edge (owner, UX5): the ground thins out into the Orbit's disc beneath over the outer rim — dense where
       the Orbit's hive lies under it, glass by the transparency knob (docs/plans/glass.md) — and the graph fades out
       with it instead of being cut off. */
    background: radial-gradient(closest-side, var(--ax-glass-orbit-disc) 86%, transparent 100%);
    cursor: grab;
  }
  /* In the Orbit's container (`container-type: size` there): its own box is the unit, the same one the cloud is sized from. */
  .travel {
    position: fixed;
    inset: auto 0 0 auto;
    z-index: 3;
    width: calc(320px * var(--ax-ui-scale));
    height: calc(120px * var(--ax-ui-scale));
    pointer-events: none;
  }
  .travel > :global(*) {
    pointer-events: auto;
  }
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

  /* The replay's bar: over the disc's top, under the app's top bar — the bottom holds the view's own bars. */
  .growth {
    position: fixed;
    z-index: calc(var(--ax-z-staging) - 2);
    top: calc(72px * var(--ax-ui-scale));
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    padding: var(--ax-space-2) var(--ax-space-3);
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border-bottom: 2px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-drag);
    font-size: var(--ax-font-size-sm);
    transition: opacity var(--ax-dur-med) var(--ax-ease);
  }
  .growth.away {
    opacity: 0;
    pointer-events: none;
  }
  .growth input {
    width: calc(320px * var(--ax-ui-scale));
    accent-color: var(--ax-glow);
  }
  .growth .when {
    min-width: calc(150px * var(--ax-ui-scale));
    color: var(--ax-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .growth .play,
  .growth .end {
    width: calc(30px * var(--ax-ui-scale));
    padding: 0;
    border-radius: var(--ax-radius-pill);
  }
  .growth .end {
    color: var(--ax-text-muted);
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
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-2);
  }
  .row button {
    font-size: var(--ax-font-size-sm);
    /* A button's label stays on one line; the row wraps instead. */
    white-space: nowrap;
  }
  .stats {
    margin: 0;
    color: var(--ax-text-muted);
  }
  .find {
    width: calc(200px * var(--ax-ui-scale));
    font-size: var(--ax-font-size-sm);
  }
  .search .label {
    white-space: nowrap;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
  }
  /* A tag is a plain chip: no accent, it is neither alive nor the one action (the accent rule). */
  .chip {
    padding: 1px var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-2);
    color: var(--ax-text);
    font: inherit;
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }
  .chip:hover,
  .chip:focus-visible {
    background: var(--ax-selected-bg);
  }
  /* The explanations open beside the bar, to its right: above the bar they covered the cloud's lower rim. They are lifted
     by the assistant bar's height so they do not run under that bar (it is centred over the same strip, z above these). */
  .help {
    position: absolute;
    left: calc(100% + var(--ax-space-3));
    bottom: calc(64px * var(--ax-ui-scale));
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
  /* After the base rule above, or it would win by order: the bar spans the width here, so there is no room beside it. */
  @media (max-width: 1320px) {
    .help {
      left: 0;
      bottom: calc(100% + var(--ax-space-2));
    }
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
