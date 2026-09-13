<!--
  second-brain — the particle graph behind the tiles: the workspace as
  rings (skills inner, areas as coloured segments, routines outer, CLAUDE.md
  hub), plus the App Ring around the outside (builtin modules left of the
  "+", externally added Mac apps right of it — see docs/plans/app-ring.md).
  Loads `get_workspace_graph` on mount and every REFRESH_MS, redraws on
  theme change or a `userApps` change, spins slowly (the App Ring itself
  does not — see `layout.ts`'s `layoutAppRing`). Hover shows the node label;
  a click on the visible cloud (a node hit, or just within its disc radius —
  the point cloud has real gaps between points) opens the full Second Brain
  view (bus `open-second-brain`, step 4) — *unless* the hit node is an "app"
  node, which instead launches/focuses it, or a context menu is open, which
  the click just closes. This module's own wrapper div is `inset: 0`
  (full-bleed behind every tile), so a click has to be checked against the
  cloud's actual footprint explicitly — otherwise any click on empty
  dashboard background would open Second Brain too (owner feedback: "egal
  wo ich auf den Hintergrund klicke ich im Brain lande"). Config: `spin`,
  `labels`.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { openPath } from "@tauri-apps/plugin-opener";

  import { listBuiltinApps, removeUserApp, userApps } from "../core/apps";
  import type { WorkspaceGraph } from "../core/backend";
  import { createInstance } from "../core/lifecycle";
  import { bringToFront, instances } from "../core/stores";
  import { toast } from "../core/toast";
  import type { ModuleContext } from "../core/types";
  import { APP_RING, layoutAppRing, layoutOrbit } from "../graph/layout";
  import { buildAppNodes, buildModel, readPalette, type GraphNode } from "../graph/model";
  import { appNodeRadiusPx, GraphRenderer } from "../graph/render";
  import AppAddDialog from "./AppAddDialog.svelte";
  import AppContextMenu from "./AppContextMenu.svelte";

  const REFRESH_MS = 30_000;
  /** Fraction of the shorter canvas side used as the orbit ring's radius —
   *  shared by both rings, since the App Ring's own radius is a multiple of
   *  this one (`APP_RING`, `graph/layout.ts`): shrinking it scales both
   *  down together instead of just the inner ring. Owner feedback: both
   *  rings read as a bit too large at the previous `0.42`. */
  const ORBIT_FIT = 0.36;
  /** Rough on-screen footprint of `AppContextMenu`, for clamping it inside
   *  the viewport — doesn't need to be exact, just big enough that the real
   *  (CSS-laid-out) menu never pokes past an edge by more than a few px. */
  const MENU_W = 180;
  const MENU_H = 110;

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let canvas = $state<HTMLCanvasElement | null>(null);
  /** Outer disc radius in px (`ORBIT_FIT` × the shorter side) — positions
   *  the hint, and (via `APP_RING`, see the "+" button below) the App Ring.
   *  This is only numerically equal to the renderer's own private
   *  `radius()` (`min(width,height) * options.fit * view.zoom`) because
   *  this uses the same `ORBIT_FIT` passed to `renderer.options` below, and
   *  `view.zoom` is never touched anywhere in this file (stays at its
   *  default `1`). Nothing enforces that; if either ever changes, this and
   *  the "+" button's position silently drift apart from the ring's actual
   *  geometry. */
  let discR = $state(200);
  let renderer = $state.raw<GraphRenderer | null>(null);
  let graph: WorkspaceGraph | null = null;
  // `$state.raw`, not `$state`: this must stay the exact same object
  // reference that lives in `renderer.model.nodes`, since the renderer
  // decides its hover glow with `n === this.hover` — a plain `$state` would
  // proxy-wrap the assigned node, breaking that comparison silently.
  let hover = $state.raw<GraphNode | null>(null);
  let error = $state("");
  let summary = $state("");

  /** Open right-click menu on a user-app node, or `null`. Positioned in
   *  viewport px (`AppContextMenu` is `position: fixed`), already clamped
   *  on open so it can never render partly off-screen. */
  let menu = $state<{ node: GraphNode; x: number; y: number } | null>(null);
  let dialogOpen = $state(false);

  const spin = $derived($config.spin !== false);
  const labels = $derived($config.labels !== false);
  /** The "+" button's diameter, in lockstep with the App Ring's own icon
   *  nodes (`appNodeRadiusPx`) — so it reads as one of the ring's slots
   *  instead of a separately-sized piece of UI chrome dropped on top of it. */
  const addAppD = $derived(appNodeRadiusPx(discR) * 2);

  function rebuild() {
    if (!renderer || !graph) return;
    const palette = readPalette();
    const model = buildModel(graph, palette);
    layoutOrbit(model);
    // The App Ring is independent of the workspace graph — attached here so
    // every `rebuild()` (workspace refresh, theme change, or a `userApps`
    // edit, see the subscription in `onMount`) carries it too.
    const appNodes = buildAppNodes(listBuiltinApps(), get(userApps), palette);
    const builtinNodes = appNodes.filter((n) => !n.userApp);
    const userAppNodes = appNodes.filter((n) => n.userApp);
    layoutAppRing(builtinNodes, userAppNodes);
    for (const n of appNodes) {
      model.nodes.push(n);
      model.byId.set(n.id, n);
    }
    renderer.setColors(palette.text, palette.muted, palette.border, palette.invert, palette.invert, palette.surface, palette.accent, palette.light);
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

  /** True once the pointer is over the visible cloud (an actual node hit,
   *  or just within the disc — the fibonacci-sphere point cloud has real
   *  gaps between points, so requiring an exact hit there would make most
   *  of the cloud's own body feel unclickable) rather than the empty
   *  background this module's `inset: 0` div otherwise covers full-bleed
   *  behind every tile. Gates both the click-to-open behaviour and the
   *  pointer cursor, so the affordance matches what's actually clickable. */
  let withinCloud = $state(false);

  function onMove(e: MouseEvent) {
    if (!renderer) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    hover = renderer.hitTest(x, y);
    renderer.hover = hover;
    withinCloud = hover !== null || Math.hypot(x - rect.width / 2, y - rect.height / 2) <= discR;
  }

  function onLeave() {
    hover = null;
    withinCloud = false;
    if (renderer) renderer.hover = null;
  }

  function open(node: GraphNode | null) {
    if (!withinCloud) return;
    ctx.emit("open-second-brain", { focus: node?.id ?? null });
  }

  /** Builtin: launch (create) it, or bring an already-placed instance to
   *  front — every builtin currently on the ring is a singleton (`apps.ts`
   *  excludes the one exception, `md-file`), so there's no "repeat click on
   *  a non-singleton" case to handle. User app: launch the `.app` bundle via
   *  the OS. Either can fail (a module rejects `createInstance`, the `.app`
   *  has moved/been deleted) — surfaced as a toast, same as `ModulePicker`'s
   *  own failed-create path. */
  function handleAppClick(n: GraphNode): void {
    if (n.userApp) {
      if (n.appPath) void openPath(n.appPath).catch((err) => toast(String(err), "warning"));
      return;
    }
    if (!n.appType) return;
    const placed = get(instances).find((i) => i.type === n.appType);
    if (placed) {
      bringToFront(placed.id);
      return;
    }
    const result = createInstance(n.appType);
    if (!result.ok) toast(result.reason, "warning");
  }

  /** Click routing: closing an open context menu takes priority over
   *  everything else (the click that dismisses it must not also act on
   *  whatever's still hovered underneath); an "app" node hit routes to
   *  `handleAppClick` instead of the normal open-Second-Brain behaviour;
   *  everything else falls through to the existing `open`. */
  function onClick(): void {
    if (menu) {
      menu = null;
      return;
    }
    if (hover?.kind === "app") {
      handleAppClick(hover);
      return;
    }
    open(hover);
  }

  /** Clamps a context-menu anchor point so `AppContextMenu` (an estimated
   *  `MENU_W × MENU_H`, `position: fixed`) always renders fully on screen,
   *  even for a right-click near an edge (relevant on a 21:9 monitor). */
  function clampMenuPos(px: number, py: number): { x: number; y: number } {
    return {
      x: Math.min(Math.max(8, px), window.innerWidth - MENU_W - 8),
      y: Math.min(Math.max(8, py), window.innerHeight - MENU_H - 8),
    };
  }

  /** Right-click on a user-app node opens the removal menu; anywhere else,
   *  a right-click just closes one that's already open (no menu ever
   *  appears for a builtin — those aren't removable, see the plan doc).
   *  `preventDefault` is unconditional — this canvas has no legitimate use
   *  for the OS/browser's own context menu anywhere on it, "just close an
   *  open menu" included; without it, a right-click on ordinary background
   *  could pop the native menu on top of (or instead of) the dismiss. */
  function onContextMenu(e: MouseEvent): void {
    e.preventDefault();
    if (hover?.kind === "app" && hover.userApp) {
      menu = { node: hover, ...clampMenuPos(e.clientX, e.clientY) };
    } else {
      menu = null;
    }
  }

  function onMenuKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") menu = null;
  }

  /** Closes an open menu on a click anywhere outside it — including a click
   *  that lands on a tile rather than the background canvas. `.brain` sits
   *  behind every tile (`#particle-slot`'s z-index), so a click on a tile
   *  never reaches `.brain`'s own onclick (which otherwise already closes
   *  the menu first); without this, the menu could be left floating
   *  indefinitely once the owner clicks anywhere but empty canvas. Right
   *  click doesn't fire a `click` event in any browser (only `contextmenu`
   *  does), so opening the menu here never immediately re-closes it. Runs
   *  on the bubble phase, after the target's own handlers (including
   *  `AppContextMenu`'s own "Entfernen"/"Abbrechen" buttons, which already
   *  set `menu = null` themselves) — so this only ever does anything on a
   *  genuine outside click. */
  $effect(() => {
    if (!menu) return;
    const onWindowClick = (e: MouseEvent) => {
      if ((e.target as HTMLElement | null)?.closest(".app-context-menu")) return;
      menu = null;
    };
    window.addEventListener("click", onWindowClick);
    return () => window.removeEventListener("click", onWindowClick);
  });

  $effect(() => {
    // Read the reactive inputs first — an early return on a missing renderer
    // would otherwise leave the effect without dependencies.
    const next = { mode: "orbit" as const, spin: spin ? 0.02 : 0, labels };
    if (renderer) renderer.options = { ...renderer.options, ...next };
  });

  onMount(() => {
    if (!canvas) return;
    renderer = new GraphRenderer(canvas);
    renderer.options = { mode: "orbit", spin: spin ? 0.02 : 0, labels, fileLabels: false, fit: ORBIT_FIT };
    renderer.resize();
    const ro = new ResizeObserver(() => {
      renderer?.resize();
      const r = canvas!.getBoundingClientRect();
      discR = Math.min(r.width, r.height) * ORBIT_FIT;
    });
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
    // The App Ring's right side tracks `userApps` independently of the
    // workspace-graph refresh cycle above — `rebuild()` already no-ops
    // until `graph` is loaded, so an immediate fire from `.subscribe()`
    // itself (the Svelte store contract) before that happens is harmless.
    const unsubUserApps = userApps.subscribe(() => rebuild());
    return () => {
      cancelAnimationFrame(raf);
      clearInterval(refresh);
      ro.disconnect();
      mo.disconnect();
      unsubUserApps();
    };
  });
</script>

<svelte:window onkeydown={menu ? onMenuKeydown : undefined} />

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
  class="brain"
  class:hovering={hover !== null}
  class:clickable={withinCloud}
  onmousemove={onMove}
  onmouseleave={onLeave}
  onclick={onClick}
  oncontextmenu={onContextMenu}
>
  <canvas bind:this={canvas}></canvas>
  {#if error}
    <p class="error">{error}</p>
  {:else}
    <p class="hint" style:top="calc(50% + {discR * 0.72}px)">{hover ? hover.label : "CLICK TO OPEN SECOND BRAIN"}</p>
    <p class="summary" style:top="calc(50% + {discR * 0.72 + 20}px)">{summary}</p>
  {/if}
  <button
    type="button"
    class="add-app"
    style:top="calc(50% - {discR * APP_RING}px)"
    style:width="{addAppD}px"
    style:height="{addAppD}px"
    style:font-size="{addAppD * 0.55}px"
    aria-label="Add app"
    title="Add app"
    onclick={(e) => {
      e.stopPropagation();
      dialogOpen = true;
    }}
  >
    +
  </button>
</div>

{#if menu}
  <!-- Keyed on the target node's id: a right-click retargeting the menu to
       a different app (without first confirming/cancelling the previous
       one) must remount `AppContextMenu` fresh, or its own `confirming`
       $state would survive the retarget and silently skip the new node's
       first "Entfernen" stage. -->
  {#key menu.node.id}
    <AppContextMenu
      x={menu.x}
      y={menu.y}
      label={menu.node.label}
      onRemove={() => {
        if (menu?.node.appPath) removeUserApp(menu.node.appPath);
        menu = null;
      }}
      onCancel={() => (menu = null)}
    />
  {/key}
{/if}

<AppAddDialog bind:open={dialogOpen} />

<style>
  .brain {
    position: absolute;
    inset: 0;
    cursor: default;
  }
  .brain.clickable {
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
    top: 50%;
  }
  .summary {
    top: 50%;
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

  /* Sits at the App Ring's 12 o'clock, between the builtin (left) and
     user-app (right) sides, sized to match the ring's own icon nodes
     (`addAppD`, from `appNodeRadiusPx`) so it reads as one of the ring's
     slots rather than separate UI chrome dropped on top of it — same
     surface fill as a canvas node, accent border like a builtin app node's
     resting stroke. */
  .add-app {
    position: absolute;
    left: 50%;
    transform: translate(-50%, -50%);
    padding: 0;
    line-height: 1;
    border-radius: 50%;
    background: var(--ax-surface-2);
    border-width: 1.5px;
    border-color: color-mix(in srgb, var(--ax-accent) 80%, transparent);
    color: var(--ax-accent);
  }
  .add-app:hover {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }
</style>
