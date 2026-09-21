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
  import { onMount } from "svelte";
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
  import DockNode from "./DockNode.svelte";
  import {
    activateTab,
    addTab,
    closeTab,
    findNode,
    isSplit,
    moveTab,
    resizeSplit,
    setTabConfig,
    singleGroupLayout,
    type DockTarget,
    type Layout,
    type PaneTab,
    type SplitDir,
  } from "./layout";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let layout = $state<Layout>(startingLayout());
  let dockEl = $state<HTMLElement | undefined>();
  let draggingTab = $state<string | null>(null);
  let hint = $state<DockTarget | null>(null);

  /** Set on pointerdown, promoted to a drag once the pointer has moved far enough. */
  let pending: { tabId: string; pointerId: number; x: number; y: number } | null = null;
  /** Measured once per drag — see the header. */
  let snapshot: { root: Rect; groups: GroupGeometry[] } | null = null;
  let divider: { splitId: string; boundary: number; pointerId: number; rect: Rect; dir: SplitDir } | null = null;

  /** A fresh terminal pane. Its id is the module's `instanceId` for its lifetime. */
  function terminalTab(): PaneTab {
    return { id: crypto.randomUUID(), kind: "terminal", title: "Terminal" };
  }

  function startingLayout(): Layout {
    return singleGroupLayout([terminalTab()]);
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

  onMount(() => {
    window.addEventListener("blur", abandonDrags);
    return () => {
      window.removeEventListener("blur", abandonDrags);
      // Today the view is never unmounted, but its correctness must not depend
      // on a caller-side invariant it cannot enforce — HMR alone breaks it.
      abandonDrags();
    };
  });

  setDock({
    activate: (tabId) => {
      layout = activateTab(layout, tabId);
    },
    close: (tabId) => {
      layout = closeTab(layout, tabId);
    },
    addPane: (groupId) => {
      layout = addTab(layout, terminalTab(), { nodeId: groupId, side: "center" });
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
    setConfig: (tabId, config) => {
      layout = setTabConfig(layout, tabId, config);
    },
    draggingTab: () => draggingTab,
    hint: () => hint,
  });

  /** The drop highlight for a drag onto the whole layout's edge. */
  const rootHint = $derived(hint && hint.nodeId === ROOT_NODE_ID ? hint.side : null);
</script>

<section class="ide" class:hidden={!open} inert={!open} aria-label="IDE">
  <header>
    <div class="titles">
      <h1>IDE</h1>
      <p class="hint">Drag a tab to an edge to split, to a tab bar to join.</p>
    </div>
    <button class="back" type="button" onclick={() => (open = false)}>Back to the OS</button>
  </header>

  <div class="dock" class:dragging={draggingTab !== null} bind:this={dockEl}>
    <DockNode node={layout.root} />
    {#if rootHint && rootHint !== "center"}
      <div class="root-highlight {rootHint}" transition:fade={{ duration: 80 }}></div>
    {/if}
  </div>
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

  .hint {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .back {
    padding: var(--ax-space-1) var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .back:hover {
    color: var(--ax-accent);
    border-color: var(--ax-accent);
  }

  .dock {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    padding: var(--ax-space-2);
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
