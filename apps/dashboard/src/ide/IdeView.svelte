<!--
  The full-screen IDE (milestone M7.1, CP2): a dock layout of panes, each one
  hosting a module that already exists. The first pane content is the Terminal.

  The view owns the layout and nothing else owns any of it: `ide/layout.ts`
  decides what a dock, a move or a close does to the tree, `ide/dock.ts` turns
  pointer positions into dock targets, and this file does the two things only a
  component can — mount the panes, and measure the screen.

  Three decisions worth knowing before changing anything here:

  * **The view is hidden, never unmounted.** `App.svelte` keeps it mounted once
    it has been opened and this component hides itself with `visibility` and
    `inert`. Unmounting would destroy every pane, and a terminal pane closes
    its PTY session on destroy — going back to the dashboard for a moment would
    kill whatever was running in every shell. `inert` is what keeps a hidden
    pane out of the tab order and away from the keyboard.
  * **Escape does not close the IDE.** It belongs to whatever is inside the
    pane — a terminal running vim needs it far more than the view needs a
    shortcut. The way out is the button, deliberately.
  * **Drag geometry is measured once, when the drag starts.** The snapshot is
    exactly the approach `core/kanban.ts` takes for card drags; re-measuring on
    every pointer move would read a layout the drag highlight is already
    changing.

  CP3 adds projects: this layout is per-project and persists into the
  `layout_json` the CP0 crate already stores. Until then it starts fresh with
  one terminal and lives only as long as the app does.
-->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { fade } from "svelte/transition";

  import {
    ROOT_NODE_ID,
    crossedDragThreshold,
    dividerFraction,
    dropTarget,
    splitFractionAt,
    type GroupGeometry,
    type Rect,
  } from "./dock";
  import { setDock } from "./dockContext";
  import { pendingLocations } from "../fileapp/locationList";
  import DockNode from "./DockNode.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Inspector, { type InspectorTab } from "../fileapp/Inspector.svelte";
  import { inspectorSurface } from "../fileapp/inspectorSurface.svelte";
  import {
    activateTab,
    addTab,
    allGroups,
    allTabs,
    closeTab,
    findNode,
    findTab,
    isSplit,
    moveTab,
    resizeSplit,
    setTabConfig,
    type DockTarget,
    type Layout,
    type PaneTab,
    type SplitDir,
  } from "./layout";
  import type { AgentFields, IdeAgent } from "../core/backend";
  import AgentPicker from "./AgentPicker.svelte";
  import { agentStatus } from "./agentStatus";
  import { foldKey, forgetFolds } from "../fileapp/foldMemory";
  import QuickOpen from "../fileapp/QuickOpen.svelte";
  import type { FileRef } from "../fileapp/tabs";
  import {
    FILES_PANE,
    fileTab,
    filePaneConfig,
    fileOrigin,
    filesTab,
    frontFile,
    FILE_PANE,
    isWorkPane,
    openOrFocus,
    projectRoot,
    SEARCH_PANE,
    searchTab,
    showsFile,
  } from "./paneKinds";
  import { applyProjectCwd } from "./paneCwd";
  import PaneHost from "./panes/PaneHost.svelte";
  import { focusedIn, PANE_ATTR, parkPanes, placePanes, restoreFocus } from "./paneStore";
  import ProjectPicker from "./ProjectPicker.svelte";
  import * as projectSession from "./projectSession";
  import { flushLayout } from "./projects";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  // Which project is open, and the list, live in `projectSession` — this
  // component owns the dock tree and the two drag engines, and nothing else.
  const sessionState = projectSession.session;
  const projects = $derived($sessionState.projects);
  const current = $derived($sessionState.current);
  const agents = $derived($sessionState.agents);

  let layout = $state<Layout>(projectSession.noProjectLayout());
  let dockEl = $state<HTMLElement | undefined>();
  let draggingTab = $state<string | null>(null);
  let hint = $state<DockTarget | null>(null);
  /** The right-hand inspector (editor-look I5): the editor's settings and the shortcuts, as in the file app. */
  let inspector = $state<InspectorTab | null>(null);
  const preview = inspectorSurface();
  /** ⌘P over the open project's files (W9). */
  let quickOpen = $state(false);

  /** The open project as quick open's only root. */
  const projectRoots = $derived(
    current
      ? [{ id: projectRoot(current.id), label: current.name, path: current.repo_root, kind: "project" as const }]
      : [],
  );
  /** The files open in the dock, for quick open to rank first. */
  const openFiles = $derived(
    allTabs(layout).flatMap((t): FileRef[] => {
      const c = filePaneConfig(t);
      return c ? [{ root: c.root, rel: c.rel }] : [];
    }),
  );

  /**
   * Opens a file from quick open in the dock's file group, at `line` if given —
   * or, with no file open yet, beside the Files pane, as a click in it would.
   */
  function openFromQuickOpen(file: FileRef, line: number | null): void {
    const beside = allTabs(layout).find((t) => t.kind === FILES_PANE)?.id ?? null;
    const from = fileOrigin(layout, beside, lastWorkTab);
    layout = openOrFocus(layout, fileTab(file.root, file.rel, line), (t) => showsFile(t, file.root, file.rel), from);
  }

  /**
   * The pane the user last worked in — a terminal, an agent, a file; never
   * the Files tree or the Search pane. A first file opened from those helpers
   * becomes a tab in its group (`fileOrigin`). Followed by focus and by
   * pointer, since a terminal's canvas takes clicks without moving focus —
   * and, because a pane can hold the focus without ever having been clicked
   * (a terminal focuses itself when it starts; a reload forgets this value),
   * also by asking which pane still has the focus the moment the user turns
   * to the tree or to ⌘P.
   */
  let lastWorkTab = $state<string | null>(null);

  /** The work pane `el` sits in (or whose tab it is), if any. */
  function workPaneOf(el: Element | null): string | null {
    const id =
      el?.closest(`[${PANE_ATTR}]`)?.getAttribute(PANE_ATTR) ??
      el?.closest("[data-ide-tab]")?.getAttribute("data-ide-tab") ??
      null;
    const tab = id ? allTabs(layout).find((t) => t.id === id) : undefined;
    return tab && isWorkPane(tab) ? tab.id : null;
  }

  /** Remembers the work pane that still has the focus, before it moves. */
  function noteFocusedPane(): void {
    lastWorkTab = workPaneOf(document.activeElement) ?? lastWorkTab;
  }

  function noteWorkPane(event: Event): void {
    // A pointer press comes before the focus moves: whatever held it is
    // where the user comes from, even when that pane was never clicked.
    if (event.type === "pointerdown") noteFocusedPane();
    lastWorkTab = workPaneOf(event.target as Element | null) ?? lastWorkTab;
  }

  /**
   * The view's own keys, in the capture phase so a focused terminal does not
   * get them first: ⌘P (quick open) and ⇧⌘F (the Search pane, T13).
   * Everything else belongs to the panes.
   */
  function onViewKeydown(e: KeyboardEvent): void {
    if (!e.metaKey || e.altKey || e.ctrlKey || !current) return;
    const key = e.key.toLowerCase();
    if (!e.shiftKey && key === "p") {
      e.preventDefault();
      e.stopPropagation();
      if (!quickOpen) noteFocusedPane();
      quickOpen = !quickOpen;
    } else if (e.shiftKey && (key === "f" || e.code === "KeyF")) {
      e.preventDefault();
      e.stopPropagation();
      showSearch();
    }
  }

  /**
   * ⇧⌘F: brings the Search pane forward with its field focused, or opens one
   * — in the Files pane's group, where a column for it already is.
   */
  function showSearch(focus = true): void {
    const existing = allTabs(layout).find((t) => t.kind === SEARCH_PANE);
    if (existing) {
      const config = focus ? { ...existing.config, focus: Date.now() } : existing.config;
      layout = activateTab(setTabConfig(layout, existing.id, config ?? {}), existing.id);
      return;
    }
    const files = allTabs(layout).find((t) => t.kind === FILES_PANE);
    const group = files ? findTab(layout, files.id)?.group.id : undefined;
    const target: DockTarget = group ? { nodeId: group, side: "center" } : { nodeId: layout.root.id, side: "left" };
    layout = addTab(layout, searchTab(), target);
  }

  /** Set on pointerdown, promoted to a drag once the pointer has moved far enough. */
  let pending: { tabId: string; pointerId: number; x: number; y: number } | null = null;
  /** Measured once per drag — see the header. */
  let snapshot: { root: Rect; groups: GroupGeometry[] } | null = null;
  let divider: { splitId: string; boundary: number; pointerId: number; rect: Rect; dir: SplitDir } | null = null;

  /** Opening a project replaces the tree; its old panes are unmounted, which
   *  is the one place in this view where destroying a pane is right — those
   *  terminals were running in a different project's folder. */
  async function openProjectById(id: number) {
    const next = await projectSession.open(id);
    if (next) layout = next;
  }

  async function openFolderAsProject() {
    const next = await projectSession.openFolder();
    if (next) layout = next;
  }

  async function addProject(name: string, gitInit: boolean) {
    const next = await projectSession.createFolder(name, gitInit);
    if (next) layout = next;
  }

  async function changeRoot(id: number) {
    const next = await projectSession.changeRoot(id, layout);
    if (next) layout = next;
  }

  async function removeProject(id: number) {
    if (await projectSession.remove(id)) layout = projectSession.noProjectLayout();
  }

  /**
   * Puts an agent into a pane, beside whatever is already open.
   *
   * Docked to the right of the group holding the active pane rather than as
   * another tab in it: agents are watched, not switched between — the whole
   * point of the dock is seeing more than one at a time. A tab only carries
   * the agent's *id*; the profile itself stays in one place, so editing it
   * does not mean hunting down copies in a stored layout.
   */
  function openAgent(agent: IdeAgent) {
    const tab: PaneTab = {
      id: crypto.randomUUID(),
      kind: "agent",
      title: agent.name,
      config: { agentId: agent.id },
    };
    const groups = allGroups(layout);
    const target = groups.length > 0 ? groups[groups.length - 1].id : layout.root.id;
    layout = addTab(layout, tab, { nodeId: target, side: "right" });
  }

  /**
   * Opens a terminal beside what is already there.
   *
   * The tab bar's own `+` adds one *into* a group; this one splits, because
   * the header is where you reach for a second thing to look at rather than a
   * second tab in the pane you are in. Same target as opening an agent.
   */
  function openTerminal() {
    const project = current;
    if (!project) return;
    const groups = allGroups(layout);
    const target = groups.length > 0 ? groups[groups.length - 1].id : layout.root.id;
    const added = addTab(layout, projectSession.terminalTab(), { nodeId: target, side: "right" });
    layout = applyProjectCwd(added, project.repo_root);
  }

  async function addAgent(fields: AgentFields) {
    const created = await projectSession.addAgent(fields);
    if (created) openAgent(created);
  }

  function rectOf(el: Element): Rect {
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  }

  /**
   * Reads the dock's geometry off the DOM for a drag.
   *
   * The dragged tab is left out of every tab bar, because `moveTab`'s drop
   * index counts positions among the *other* tabs.
   */
  function measure(draggedId: string): { root: Rect; groups: GroupGeometry[] } | null {
    if (!dockEl) return null;
    const groups: GroupGeometry[] = [];
    for (const el of dockEl.querySelectorAll<HTMLElement>("[data-ide-group]")) {
      const bar = el.querySelector<HTMLElement>("[data-ide-tabbar]");
      if (!bar || !el.dataset.ideGroup) continue;
      groups.push({
        nodeId: el.dataset.ideGroup,
        rect: rectOf(el),
        tabBar: rectOf(bar),
        tabs: [...bar.querySelectorAll<HTMLElement>("[data-ide-tab]")]
          .filter((tab) => tab.dataset.ideTab !== draggedId)
          .map(rectOf),
      });
    }
    return { root: rectOf(dockEl), groups };
  }

  function onTabPointerMove(event: PointerEvent) {
    if (!pending || event.pointerId !== pending.pointerId) return;
    if (!draggingTab) {
      if (!crossedDragThreshold(pending, event.clientX, event.clientY)) return;
      snapshot = measure(pending.tabId);
      if (!snapshot) return;
      draggingTab = pending.tabId;
    }
    if (snapshot) hint = dropTarget(snapshot.root, snapshot.groups, event.clientX, event.clientY);
  }

  function onTabPointerUp(event: PointerEvent) {
    if (pending && event.pointerId !== pending.pointerId) return;
    if (draggingTab && hint) {
      // `dropTarget` works from rectangles and cannot know the root's id.
      const nodeId = hint.nodeId === ROOT_NODE_ID ? layout.root.id : hint.nodeId;
      layout = moveTab(layout, draggingTab, { ...hint, nodeId });
    }
    endTabDrag();
  }

  /**
   * Ends a tab drag, dropped or abandoned.
   *
   * Every way out of a drag goes through here, including the ones nobody
   * arranged: a `pointercancel` from the system, a window that lost focus
   * mid-drag to an OS dialog or ⌘-Tab, a pointer released outside the window.
   * That matters more here than it looks: while `draggingTab` is set the panes
   * ignore the pointer, so a drag that never ends leaves the whole IDE
   * unclickable with no way back.
   */
  function endTabDrag() {
    pending = null;
    snapshot = null;
    draggingTab = null;
    hint = null;
    window.removeEventListener("pointermove", onTabPointerMove);
    window.removeEventListener("pointerup", onTabPointerUp);
    window.removeEventListener("pointercancel", endTabDrag);
  }

  function onDividerPointerMove(event: PointerEvent) {
    if (!divider || event.pointerId !== divider.pointerId) return;
    const node = findNode(layout, divider.splitId);
    if (!node || !isSplit(node)) return;
    const along = splitFractionAt(divider.rect, divider.dir, event.clientX, event.clientY);
    const fraction = dividerFraction(node.sizes, divider.boundary, along);
    layout = resizeSplit(layout, divider.splitId, divider.boundary, fraction);
  }

  function onDividerPointerUp(event: PointerEvent) {
    if (divider && event.pointerId !== divider.pointerId) return;
    endDividerDrag();
  }

  /** Ends a divider drag, however it ended. See {@link endTabDrag}. */
  function endDividerDrag() {
    divider = null;
    window.removeEventListener("pointermove", onDividerPointerMove);
    window.removeEventListener("pointerup", onDividerPointerUp);
    window.removeEventListener("pointercancel", endDividerDrag);
  }

  /** The window lost the pointer to something outside the page. */
  function abandonDrags() {
    endTabDrag();
    endDividerDrag();
  }

  /** The last arrangement must not be left in a timer when the app goes away. */
  function flushOnLeaving() {
    void flushLayout();
  }

  onMount(() => {
    // The most recently opened project comes first out of the store, so
    // reopening the IDE lands where the user left off without a stored
    // "current project" of its own to drift out of step.
    void projectSession.start().then((next) => {
      if (next) layout = next;
    });

    window.addEventListener("blur", abandonDrags);
    // `pagehide` is what `core/persist.ts` uses for the same job: a quit while
    // the IDE is still the view on screen would otherwise drop the last write.
    window.addEventListener("pagehide", flushOnLeaving);
    return () => {
      window.removeEventListener("blur", abandonDrags);
      window.removeEventListener("pagehide", flushOnLeaving);
      // Today the view is never unmounted, but its correctness must not depend
      // on a caller-side invariant it cannot enforce — HMR alone breaks it.
      abandonDrags();
    };
  });

  setDock({
    activate: (tabId) => {
      layout = activateTab(layout, tabId);
    },
    showLocations: (list) => {
      // The Search pane takes the list from here once it is there (`SearchPane.svelte`).
      pendingLocations.set(list);
      showSearch(false);
    },
    close: (tabId) => {
      // A file pane's folds are kept only while it is open (T7).
      const file = allTabs(layout).find((t) => t.id === tabId);
      const config = file ? filePaneConfig(file) : null;
      if (config) forgetFolds(foldKey(config.root, config.rel));
      layout = closeTab(layout, tabId);
    },
    addPane: (groupId, kind) => {
      const project = current;
      if (!project) return;
      const tab =
        kind === "files" ? filesTab(project.id) : kind === "search" ? searchTab() : projectSession.terminalTab();
      const added = addTab(layout, tab, { nodeId: groupId, side: "center" });
      layout = applyProjectCwd(added, project.repo_root);
    },
    startTabDrag: (tabId, event) => {
      // A second pointer must not take over a drag already under way, and a
      // right-click is not a drag at all.
      if (event.button !== 0 || pending || divider) return;
      layout = activateTab(layout, tabId);
      pending = { tabId, pointerId: event.pointerId, x: event.clientX, y: event.clientY };
      // On `window`, not the tab: the pointer spends the drag over other
      // panes, and a terminal's canvas would otherwise swallow the moves.
      window.addEventListener("pointermove", onTabPointerMove);
      window.addEventListener("pointerup", onTabPointerUp);
      window.addEventListener("pointercancel", endTabDrag);
      event.preventDefault();
    },
    startDividerDrag: (splitId, boundary, event) => {
      if (event.button !== 0 || pending || divider) return;
      const el = (event.currentTarget as HTMLElement | null)?.closest("[data-ide-split]");
      const node = findNode(layout, splitId);
      if (!el || !node || !isSplit(node)) return;
      divider = { splitId, boundary, pointerId: event.pointerId, rect: rectOf(el), dir: node.dir };
      window.addEventListener("pointermove", onDividerPointerMove);
      window.addEventListener("pointerup", onDividerPointerUp);
      window.addEventListener("pointercancel", endDividerDrag);
      event.preventDefault();
    },
    open: (tab, match, fromTabId) => {
      const from = tab.kind === FILE_PANE ? fileOrigin(layout, fromTabId, lastWorkTab) : fromTabId;
      layout = openOrFocus(layout, tab, match, from);
    },
    setConfig: (tabId, config) => {
      layout = setTabConfig(layout, tabId, config);
    },
    activeFile: () => frontFile(layout),
    draggingTab: () => draggingTab,
    hint: () => hint,
  });

  /** The drop highlight for a drag onto the whole layout's edge. */
  const rootHint = $derived(hint && hint.nodeId === ROOT_NODE_ID ? hint.side : null);

  /** Every pane in the layout, flat — the store renders exactly this list. */
  const panes = $derived(allTabs(layout));
  /** The panes on screen: the active tab of every group, while the view is open. */
  const visibleTabs = $derived(new Set(open ? allGroups(layout).map((g) => g.active) : []));

  let storeEl = $state<HTMLElement | undefined>();

  // The two halves of keeping a pane alive across a layout change. Park before
  // Svelte touches the DOM, because a slot being destroyed would take the pane
  // inside it along; place after, into whatever slots now exist. Reading
  // `layout` in both is what ties them to the change. See `ide/paneStore.ts`.
  $effect.pre(() => {
    void layout;
    // Untracked elements: see `fileapp/FileAppView.svelte` — parking again when the elements get bound
    // would undo the placement, and nothing places a second time.
    const focused = untrack(() => (dockEl ? focusedIn(dockEl) : null));
    untrack(() => {
      if (dockEl && storeEl) parkPanes(dockEl, storeEl);
    });
    // Place once the tree is rebuilt, and give the focus back (a moved pane loses it); see
    // `fileapp/FileAppView.svelte` for why the effect below is not enough.
    void tick().then(() =>
      untrack(() => {
        if (dockEl) placePanes(dockEl);
        restoreFocus(focused);
      }),
    );
  });

  $effect(() => {
    void layout;
    if (dockEl) placePanes(dockEl);
  });

  // Every change to the tree queues a write of the open project's layout. The
  // write that follows opening a project stores what was just read back, which
  // costs one statement and buys not having to track a dirty flag that could
  // be wrong in the other direction — a layout silently not saved.
  $effect(() => {
    projectSession.save(layout);
  });

  // Leaving the IDE is the last moment anyone is watching, and the app may be
  // closed from the dashboard next. Don't leave the last arrangement in a timer.
  $effect(() => {
    if (!open) void flushLayout();
  });

  // Agent statuses are polled only while somebody can see them: the IDE is on
  // screen and a project is open (CP6). Closing the view stops the tick.
  $effect(() => {
    agentStatus.watch(open ? (current?.id ?? null) : null);
  });
  onMount(() => () => agentStatus.watch(null));
</script>

<section class="ide" class:hidden={!open} inert={!open} aria-label="IDE" onkeydowncapture={onViewKeydown}>
  <header>
    <div class="titles">
      <h1>IDE</h1>
      <ProjectPicker
        {projects}
        {current}
        switching={$sessionState.switching}
        onOpen={(id) => void openProjectById(id)}
        onOpenFolder={() => void openFolderAsProject()}
        onNewFolder={(name, gitInit) => void addProject(name, gitInit)}
        onSetRoot={(id) => void changeRoot(id)}
        onRemove={(id) => void removeProject(id)}
      />
      <IconButton icon="terminal" label="Open a terminal beside the others" disabled={!current} onclick={openTerminal} />
      <AgentPicker
        {agents}
        disabled={!current}
        onOpen={openAgent}
        onCreate={(fields) => void addAgent(fields)}
        onEdit={(id, fields) => void projectSession.editAgent(id, fields)}
        onRemove={(id) => void projectSession.removeAgent(id)}
      />
    </div>
    <div class="actions">
      <IconButton
        icon="sliders-horizontal"
        label="Editor settings"
        pressed={inspector === "settings"}
        onclick={() => (inspector = inspector === "settings" ? null : "settings")}
      />
      <IconButton
        icon="keyboard"
        label="Keyboard shortcuts"
        pressed={inspector === "shortcuts"}
        onclick={() => (inspector = inspector === "shortcuts" ? null : "shortcuts")}
      />
      <span class="separator" aria-hidden="true"></span>
      <IconButton icon="layout-grid" label="Back to the OS" onclick={() => (open = false)} />
    </div>
  </header>

  <div class="body">

  <div
    class="dock"
    class:dragging={draggingTab !== null}
    bind:this={dockEl}
    onfocusin={noteWorkPane}
    onpointerdowncapture={noteWorkPane}
  >
    <!-- Where panes live. They are rendered here once and only ever *moved*
         into the tree's slots, so that dragging a pane does not destroy and
         rebuild it — which closed an agent's PTY and restarted it, for every
         pane on screen, on every structural change. The store fills the dock
         and is hidden with `visibility` rather than `display: none`, so a pane
         waiting here still measures its real size. -->
    <div class="pane-store" bind:this={storeEl} aria-hidden="true">
      {#each panes as tab (tab.id)}
        <div class="pane-slot" {...{ [PANE_ATTR]: tab.id }}>
          <PaneHost
            {tab}
            visible={visibleTabs.has(tab.id)}
            onConfig={(config) => projectSession.save((layout = setTabConfig(layout, tab.id, config)))}
          />
        </div>
      {/each}
    </div>

    {#if current}
      <DockNode node={layout.root} />
      {#if rootHint && rootHint !== "center"}
        <div class="root-highlight {rootHint}" transition:fade={{ duration: 80 }}></div>
      {/if}
    {:else}
      <!-- No project, no panes: a terminal with nowhere to start is worse than
           no terminal. The menu above is the only thing to do here. -->
      <div class="empty">
        <p>Open a folder to work in, or start a new project — from the project menu above.</p>
      </div>
    {/if}
  </div>
  {#if inspector}
    <Inspector tab={inspector} surface={preview.current} onTab={(t) => (inspector = t)} onClose={() => (inspector = null)} />
  {/if}
  </div>
  {#if quickOpen && open}
    <QuickOpen
      roots={projectRoots}
      recent={openFiles}
      onOpen={(file, _preview, line) => openFromQuickOpen(file, line)}
      onClose={() => (quickOpen = false)}
    />
  {/if}
</section>

<style>
  .ide {
    position: fixed;
    inset: 0;
    z-index: calc(var(--ax-z-staging) - 1);
    display: flex;
    flex-direction: column;
    background: var(--ax-bg);
    background-image: var(--ax-texture-url);
    color: var(--ax-text);
  }

  /* Hidden, not unmounted — see the header comment. `visibility` keeps every
     pane's measured size, which a terminal needs to keep its PTY in step. */
  .ide.hidden {
    visibility: hidden;
    pointer-events: none;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-4);
    padding: var(--ax-space-3) var(--ax-space-5);
    border-bottom: 1px solid var(--ax-border);
    flex: 0 0 auto;
  }

  .titles {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-3);
  }

  h1 {
    margin: 0;
    font-family: var(--ax-font-display);
    font-size: var(--ax-font-size-lg);
    letter-spacing: var(--ax-tracking-wide);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .separator {
    width: 1px;
    height: var(--ax-icon-md);
    margin: 0 var(--ax-space-1);
    background: var(--ax-border);
  }

  /* The dock and, beside it, the inspector. */
  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
  }

  /* Groups meet edge to edge, a line between them (editor-look I1). */
  .dock {
    position: relative;
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
  }

  .pane-store {
    position: absolute;
    inset: 0;
    visibility: hidden;
    pointer-events: none;
  }

  .pane-slot {
    position: absolute;
    inset: 0;
  }

  .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  /* While a tab is in flight the panes must not react to the pointer passing
     over them — a terminal would start selecting text under the drag. */
  .dock.dragging :global(.pane-host) {
    pointer-events: none;
  }

  .root-highlight {
    position: absolute;
    pointer-events: none;
    background: var(--ax-accent-muted);
    border: 2px solid var(--ax-accent);
    border-radius: var(--ax-radius-md);
  }

  .root-highlight.left {
    top: 0;
    bottom: 0;
    left: 0;
    width: 33%;
  }

  .root-highlight.right {
    top: 0;
    bottom: 0;
    right: 0;
    width: 33%;
  }

  .root-highlight.top {
    left: 0;
    right: 0;
    top: 0;
    height: 33%;
  }

  .root-highlight.bottom {
    left: 0;
    right: 0;
    bottom: 0;
    height: 33%;
  }
</style>
