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
  node (launches/focuses it instead) or a "file" node sitting on the inner
  ring (`onOrbit` — a recent file with its own icon slot, not just a cloud
  point; opens it directly in the staged viewer via `openStaged`, the same
  one-click-to-content experience `core/mail.ts`'s `openMailSummary` already
  gives a mail item), or a context menu is open, which the click just
  closes. A "file" hit inside the general point cloud (every file, dense,
  no individual icon slot) still falls through to Second Brain — a single
  click there isn't precise enough to trust for opening the right file
  directly. This module's own wrapper div
  is `inset: 0`
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

  import {
    addToGroup,
    appGroups,
    createGroup,
    menuActionsFor,
    removeFromGroup,
    renameGroup,
    setGroupGlyph,
  } from "../core/appGroups";
  import { listBuiltinApps, removeUserApp, setUserAppGlyph, userApps } from "../core/apps";
  import type { WorkspaceGraph } from "../core/backend";
  import { createInstance } from "../core/lifecycle";
  import { openStaged } from "../core/staging";
  import { bringToFront, instances } from "../core/stores";
  import { toast } from "../core/toast";
  import type { ModuleContext } from "../core/types";
  import { APP_RING, layoutAppRing, layoutExpandedGroup, layoutOrbit } from "../graph/layout";
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
  /** Which App-Ring group's secondary ring is currently expanded — an id,
   *  not a node reference, because `rebuild()` creates brand-new `GraphNode`
   *  objects every time (workspace refresh, theme change, or a `userApps`/
   *  `appGroups` edit — see the subscriptions in `onMount`), so any state
   *  that has to survive a rebuild must be keyed by id, not object identity.
   *  Only one group open at a time (owner decision), so a single nullable
   *  id is enough — expanding a different group, or collapsing, replaces
   *  it rather than tracking a set. */
  let expandedGroupId = $state<string | null>(null);

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
    // every `rebuild()` (workspace refresh, theme change, or a `userApps`/
    // `appGroups` edit, see the subscriptions in `onMount`) carries it too.
    const appNodes = buildAppNodes(listBuiltinApps(), get(userApps), get(appGroups), expandedGroupId, palette);
    // Expanded-ring members are laid out separately, below — they must not
    // reach `layoutAppRing`, which would otherwise place them as if they
    // were solo ring-slot nodes.
    const ringNodes = appNodes.filter((n) => !n.onExpandedRing);
    const builtinNodes = ringNodes.filter((n) => !n.userApp);
    const userAppNodes = ringNodes.filter((n) => n.userApp);
    layoutAppRing(builtinNodes, userAppNodes);
    const expandedGroupNode = ringNodes.find((n) => n.isGroup && n.groupId === expandedGroupId);
    if (expandedGroupNode) {
      const members = appNodes.filter((n) => n.onExpandedRing);
      layoutExpandedGroup(members, Math.atan2(expandedGroupNode.y, expandedGroupNode.x));
    }
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

  /** The App-Ring-group member id for a solo/member "app" node — its
   *  registry `type` on the builtin side, its filesystem `path` on the
   *  user-app side (matching `AppGroup.members`' own identity convention,
   *  `core/appGroups.ts`). `undefined` for a group's own ring-slot node
   *  (`isGroup`), which has neither. */
  function memberIdOf(n: GraphNode): string | undefined {
    return n.userApp ? n.appPath : n.appType;
  }

  function menuSide(n: GraphNode): "builtin" | "user" {
    return n.userApp ? "user" : "builtin";
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
   *  whatever's still hovered underneath); a hit on a group's own ring-slot
   *  circle toggles its expanded secondary ring (only one group open at a
   *  time — expanding a different one, or this same click, replaces
   *  whatever was open); any other click first collapses an open expanded
   *  ring, if there is one (owner decision: opening a different group or
   *  clicking elsewhere always closes the current one); an "app" node hit
   *  (a solo node, or a member on the just-collapsed expanded ring — it
   *  behaves exactly the same either way) routes to `handleAppClick`
   *  instead of the normal open-Second-Brain behaviour; a "file" node hit
   *  on the inner ring (`onOrbit` — a recent file with its own
   *  individually addressable icon slot, per `layoutOrbit`) opens the file
   *  directly in the staged viewer — the same one-click-to-content
   *  experience `openMailSummary` already gives a mail item. A "file" node
   *  hit inside the general 3-D point cloud (every file, dense and with
   *  real gaps — `onOrbit` false) is *not* precise enough to trust a single
   *  click on: it falls through to `open`, landing in the full Second Brain
   *  graph to browse/select from instead of blindly opening whatever point
   *  the cursor happened to land nearest to. A click that only closed an
   *  expanded ring (hub/skill/routine/background otherwise) is swallowed —
   *  it must not also open Second Brain, same reasoning as the `menu`
   *  dismiss above. */
  function onClick(): void {
    if (menu) {
      menu = null;
      return;
    }
    if (hover?.kind === "app" && hover.isGroup) {
      expandedGroupId = expandedGroupId === hover.groupId ? null : (hover.groupId ?? null);
      rebuild();
      return;
    }
    const wasExpanded = expandedGroupId !== null;
    if (wasExpanded) {
      expandedGroupId = null;
      rebuild();
    }
    if (hover?.kind === "app") {
      handleAppClick(hover);
      return;
    }
    if (hover?.kind === "file" && hover.path && hover.onOrbit) {
      openStaged("md-file", { path: hover.path, mode: "read" });
      return;
    }
    if (wasExpanded) return;
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

  /** Right-click on any "app" node (solo, a group's own circle, or a
   *  member on an expanded ring) opens `AppContextMenu`, whose actual
   *  content is decided by `menuActionsFor` (`core/appGroups.ts`) — a
   *  builtin gets only grouping actions there, never "Entfernen"; anywhere
   *  else, a right-click just closes one that's already open.
   *  `preventDefault` is unconditional — this canvas has no legitimate use
   *  for the OS/browser's own context menu anywhere on it, "just close an
   *  open menu" included; without it, a right-click on ordinary background
   *  could pop the native menu on top of (or instead of) the dismiss. */
  function onContextMenu(e: MouseEvent): void {
    e.preventDefault();
    menu = hover?.kind === "app" ? { node: hover, ...clampMenuPos(e.clientX, e.clientY) } : null;
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
   *  `AppContextMenu`'s own action buttons, which already set `menu = null`
   *  themselves) — so this only ever does anything on a genuine outside
   *  click.
   *
   *  Reads `e.composedPath()`, not `e.target.closest(...)`: a click inside
   *  `AppContextMenu` that also changes its `stage` (e.g. "Entfernen" →
   *  the confirm step) removes the clicked button from the DOM as part of
   *  that same re-render — by the time this bubble-phase listener runs,
   *  `e.target` can already be a detached node whose `closest()` walk has
   *  nothing left to climb, so it wrongly fails to find `.app-context-menu`
   *  and closes the whole menu right as it was about to show the next
   *  stage. `composedPath()` instead returns the path as it was at dispatch
   *  time, unaffected by any DOM mutation the click's own handlers made. */
  $effect(() => {
    if (!menu) return;
    const onWindowClick = (e: MouseEvent) => {
      const inMenu = e.composedPath().some((t) => t instanceof Element && t.classList.contains("app-context-menu"));
      if (inMenu) return;
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
    // App-Ring group CRUD (create/add/remove/rename/change icon) all go
    // through `appGroups` — this alone is enough to pick up every one of
    // them automatically, no call site needs to remember to `rebuild()`
    // itself. Toggling `expandedGroupId` is *not* a store change, so
    // `onClick` below calls `rebuild()` directly for that.
    const unsubAppGroups = appGroups.subscribe(() => rebuild());
    return () => {
      cancelAnimationFrame(raf);
      clearInterval(refresh);
      ro.disconnect();
      mo.disconnect();
      unsubUserApps();
      unsubAppGroups();
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
    {@const node = menu.node}
    <AppContextMenu
      x={menu.x}
      y={menu.y}
      label={node.label}
      glyph={node.glyph ?? "folder"}
      actions={menuActionsFor(node, $appGroups)}
      onRemove={() => {
        if (node.appPath) removeUserApp(node.appPath);
        menu = null;
      }}
      onAddToNewGroup={() => {
        const id = memberIdOf(node);
        if (id) createGroup(menuSide(node), id);
        menu = null;
      }}
      onAddToGroup={(groupId) => {
        const id = memberIdOf(node);
        if (id) addToGroup(groupId, id);
        menu = null;
      }}
      onRemoveFromGroup={() => {
        const id = memberIdOf(node);
        if (node.groupId && id) removeFromGroup(node.groupId, id);
        menu = null;
      }}
      onRename={(name) => {
        if (node.groupId) renameGroup(node.groupId, name);
        menu = null;
      }}
      onChangeIcon={(glyph) => {
        // Offered for two, mutually exclusive node shapes (menuActionsFor):
        // a group's own circle (`groupId` set, its own id) or an ungrouped
        // solo Mac app (`appPath` set, no `groupId`) — never both.
        if (node.groupId) setGroupGlyph(node.groupId, glyph);
        else if (node.appPath) setUserAppGlyph(node.appPath, glyph);
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
