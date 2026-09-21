<!--
  One tab group: the tab bar, the panes stacked underneath it, and the drop
  highlight while something is being dragged onto it.

  Every tab in the group is mounted at once and all but the active one are
  hidden with `visibility` — see `panes/PaneHost.svelte` for why a hidden pane
  must keep its size and its process. The `data-ide-*` attributes are what the
  view measures its drag geometry from; they are the contract with
  `ide/dock.ts`, not decoration.
-->
<script lang="ts">
  import { getDock } from "./dockContext";
  import type { TabGroup } from "./layout";
  import PaneHost from "./panes/PaneHost.svelte";

  let { group }: { group: TabGroup } = $props();

  const dock = getDock();

  /** The drop highlight for this group, or `null` when the drag is elsewhere. */
  const highlight = $derived.by(() => {
    const target = dock.hint();
    return target && target.nodeId === group.id ? target.side : null;
  });
</script>

<div class="group" data-ide-group={group.id}>
  <div class="tabbar" data-ide-tabbar role="tablist">
    {#each group.tabs as tab (tab.id)}
      <div
        class="tab"
        class:active={tab.id === group.active}
        class:dragging={dock.draggingTab() === tab.id}
        data-ide-tab={tab.id}
        id="ide-tab-{tab.id}"
        role="tab"
        tabindex="0"
        aria-selected={tab.id === group.active}
        aria-controls="ide-pane-{tab.id}"
        onpointerdown={(event) => dock.startTabDrag(tab.id, event)}
        onkeydown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            dock.activate(tab.id);
          }
        }}
      >
        <span class="title">{tab.title}</span>
        <button
          class="close"
          type="button"
          aria-label="Close {tab.title}"
          onpointerdown={(event) => event.stopPropagation()}
          onclick={() => dock.close(tab.id)}>×</button
        >
      </div>
    {/each}
    <button class="add" type="button" aria-label="New terminal in this group" onclick={() => dock.addPane(group.id)}
      >+</button
    >
  </div>

  <div class="body">
    {#each group.tabs as tab (tab.id)}
      <!-- `inert` as well as hidden: a mounted-but-invisible terminal calls
           `focus()` on itself when it starts, and would otherwise take the
           keyboard away from the pane the user is actually looking at. -->
      <div
        class="slot"
        class:hidden={tab.id !== group.active}
        inert={tab.id !== group.active}
        id="ide-pane-{tab.id}"
        role="tabpanel"
        aria-labelledby="ide-tab-{tab.id}"
      >
        <PaneHost {tab} onConfig={(config) => dock.setConfig(tab.id, config)} />
      </div>
    {/each}
  </div>

  {#if highlight}
    <div class="highlight {highlight}"></div>
  {/if}
</div>

<style>
  .group {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    min-height: 0;
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    overflow: hidden;
  }

  .tabbar {
    display: flex;
    align-items: stretch;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1);
    background: var(--ax-surface-2);
    border-bottom: 1px solid var(--ax-border);
    /* The bar is a drop target of its own; it must not shrink away. */
    flex: 0 0 auto;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    white-space: nowrap;
    cursor: grab;
    user-select: none;
  }

  .tab:hover {
    color: var(--ax-text);
    background: var(--ax-surface-3);
  }

  .tab.active {
    color: var(--ax-text);
    background: var(--ax-surface-1);
    box-shadow: inset 0 -2px 0 var(--ax-accent);
  }

  .tab.dragging {
    opacity: var(--ax-tile-glass-opacity);
    cursor: grabbing;
  }

  .tab:focus-visible {
    outline: var(--ax-focus-ring);
  }

  .close,
  .add {
    padding: 0 var(--ax-space-1);
    background: none;
    border: none;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    line-height: 1;
    cursor: pointer;
  }

  .close:hover {
    color: var(--ax-danger);
  }

  .add:hover {
    color: var(--ax-accent);
  }

  .body {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
  }

  .slot {
    position: absolute;
    inset: 0;
  }

  /* Not `display: none`: a hidden pane keeps its measured size, so a terminal
     does not hear that it has zero rows every time a sibling tab is shown. */
  .slot.hidden {
    visibility: hidden;
    pointer-events: none;
  }

  .highlight {
    position: absolute;
    pointer-events: none;
    background: var(--ax-accent-muted);
    border: 2px solid var(--ax-accent);
    border-radius: var(--ax-radius-sm);
  }

  .highlight.center {
    inset: 0;
  }

  .highlight.left {
    top: 0;
    bottom: 0;
    left: 0;
    width: 50%;
  }

  .highlight.right {
    top: 0;
    bottom: 0;
    right: 0;
    width: 50%;
  }

  .highlight.top {
    left: 0;
    right: 0;
    top: 0;
    height: 50%;
  }

  .highlight.bottom {
    left: 0;
    right: 0;
    bottom: 0;
    height: 50%;
  }
</style>
