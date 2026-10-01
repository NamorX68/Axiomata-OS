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
  import { agentStatus, describeStatus, type StatusView } from "./agentStatus";
  import { getDock, type NewPaneKind } from "./dockContext";
  import type { PaneTab, TabGroup } from "./layout";
  import { SLOT_ATTR } from "./paneStore";
  import { session } from "./projectSession";
  import StatusDot from "./StatusDot.svelte";
  import { languageColor } from "../core/languageColors";
  import { editorSettings } from "../fileapp/editorSettings";
  import Icon from "../ui/Icon.svelte";
  import { filePaneConfig } from "./paneKinds";

  let { group }: { group: TabGroup } = $props();

  const dock = getDock();
  const statuses = agentStatus.statuses;

  /**
   * The status dot for an agent tab, so a waiting agent is noticed even in a
   * pane nobody is looking at (CP6). `null` for every other kind of tab, and
   * for an agent tab whose profile has been deleted.
   */
  function tabStatus(tab: PaneTab): StatusView | null {
    const id = tab.kind === "agent" ? tab.config?.agentId : undefined;
    if (typeof id !== "number") return null;
    const agent = $session.agents.find((a) => a.id === id);
    return agent ? describeStatus($statuses.byAgent.get(id), agent, $statuses.checkedAt) : null;
  }

  /**
   * A tab's tint: a file tab's language colour, as in the file app (K14); an
   * agent tab's harness colour — Claude Code or Opencode at a glance (I7); none
   * for the rest, or when tints are off.
   */
  function tint(tab: PaneTab): string | undefined {
    if (!$editorSettings.tabColors) return undefined;
    const file = filePaneConfig(tab);
    if (file) return languageColor(file.rel);
    const id = tab.kind === "agent" ? tab.config?.agentId : undefined;
    const harness = typeof id === "number" ? $session.agents.find((a) => a.id === id)?.harness : undefined;
    return harness ? `var(--ax-harness-${harness})` : undefined;
  }

  /** The `+` menu, open at these viewport coordinates (W16). */
  let adding = $state<{ x: number; y: number } | null>(null);

  let addButton = $state<HTMLButtonElement | undefined>();

  function toggleAddMenu(event: MouseEvent): void {
    if (adding) {
      adding = null;
      return;
    }
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    adding = { x: rect.left, y: rect.bottom };
  }

  function add(kind: NewPaneKind): void {
    adding = null;
    dock.addPane(group.id, kind);
  }

  /** The drop highlight for this group, or `null` when the drag is elsewhere. */
  const highlight = $derived.by(() => {
    const target = dock.hint();
    return target && target.nodeId === group.id ? target.side : null;
  });
</script>

<svelte:window
  onclick={(event) => {
    // Every group listens here, so a click on another group's `+` still closes this menu.
    if (!addButton?.contains(event.target as Node)) adding = null;
  }}
  onkeydown={(event) => {
    if (adding && event.key === "Escape") adding = null;
  }}
/>

<div class="group" data-ide-group={group.id}>
  <div class="tabbar" data-ide-tabbar role="tablist">
    {#each group.tabs as tab (tab.id)}
      {@const view = tabStatus(tab)}
      <div
        class="tab"
        class:active={tab.id === group.active}
        class:dragging={dock.draggingTab() === tab.id}
        class:preview={filePaneConfig(tab)?.preview === true}
        style:--lang={tint(tab)}
        data-ide-tab={tab.id}
        id="ide-tab-{tab.id}"
        role="tab"
        tabindex="0"
        aria-selected={tab.id === group.active}
        aria-controls="ide-pane-{tab.id}"
        onpointerdown={(event) => dock.startTabDrag(tab.id, event)}
        ondblclick={() => dock.pin(tab.id)}
        onkeydown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            dock.activate(tab.id);
          }
        }}
      >
        {#if view}<StatusDot {view} />{/if}
        <span class="title">{tab.title}</span>
        <button
          class="close"
          type="button"
          aria-label="Close {tab.title}"
          onpointerdown={(event) => event.stopPropagation()}
          onclick={() => dock.requestClose(tab.id)}><Icon name="x" size="sm" /></button
        >
      </div>
    {/each}
    <button
      class="add"
      type="button"
      aria-label="New pane in this group"
      title="New pane in this group"
      aria-haspopup="menu"
      bind:this={addButton}
      aria-expanded={adding !== null}
      onclick={toggleAddMenu}><Icon name="plus" size="sm" /></button
    >
  </div>

  {#if adding}
    <!-- Fixed, not inside the tab bar: the bar scrolls sideways and would clip it. -->
    <div class="add-menu" role="menu" style:left="{adding.x}px" style:top="{adding.y}px">
      <button type="button" role="menuitem" onclick={() => add("terminal")}>Terminal</button>
      <button type="button" role="menuitem" onclick={() => add("files")}>Files</button>
      <button type="button" role="menuitem" onclick={() => add("search")}>Search</button>
      <button type="button" role="menuitem" onclick={() => add("git")}>Git</button>
    </div>
  {/if}

  <div class="body">
    <!-- Empty on purpose. The pane itself is rendered once, flat, in the
         view's pane store and moved in here after every layout change
         (`ide/paneStore.ts`) — rendering it in this `{#each}` is what used to
         destroy and rebuild it, and with it the agent's PTY, on every drag.

         `inert` as well as hidden: a mounted-but-invisible terminal calls
         `focus()` on itself when it starts, and would otherwise take the
         keyboard away from the pane the user is actually looking at. -->
    {#each group.tabs as tab (tab.id)}
      <div
        class="slot"
        class:hidden={tab.id !== group.active}
        inert={tab.id !== group.active}
        id="ide-pane-{tab.id}"
        role="tabpanel"
        aria-labelledby="ide-tab-{tab.id}"
        {...{ [SLOT_ATTR]: tab.id }}
      ></div>
    {/each}
  </div>

  {#if highlight}
    <div class="highlight {highlight}"></div>
  {/if}
</div>

<style>
  .group {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    min-height: 0;
    background: var(--ax-surface-1);
    overflow: hidden;
  }

  /* The tab bar (editor-look K7): tabs as floating pills; a file tab tinted with its language (K14). */
  .tabbar {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-1);
    border-bottom: 1px solid var(--ax-border);
    /* The bar is a drop target of its own; it must not shrink away. */
    flex: 0 0 auto;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab {
    --tint: var(--lang, transparent);
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-1) var(--ax-space-1) var(--ax-space-3);
    border: 1px solid transparent;
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--tint) 20%, var(--ax-surface-2));
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    white-space: nowrap;
    cursor: grab;
    user-select: none;
    transition:
      background var(--ax-dur-fast) var(--ax-ease),
      border-color var(--ax-dur-fast) var(--ax-ease);
  }

  .tab:hover {
    color: var(--ax-text);
    background: color-mix(in srgb, var(--tint) 28%, var(--ax-surface-3));
  }

  .tab.active {
    color: var(--ax-text);
    background: color-mix(in srgb, var(--tint) 36%, var(--ax-surface-3));
    border-color: color-mix(in srgb, var(--tint) 60%, var(--ax-accent));
  }

  .tab.dragging {
    opacity: var(--ax-tile-glass-opacity);
    cursor: grabbing;
  }

  .tab:focus-visible {
    outline: var(--ax-focus-ring);
  }

  /* The preview tab (W7): italic until it becomes a tab of its own. */
  .tab.preview .title {
    font-style: italic;
  }

  .close,
  .add {
    display: flex;
    align-items: center;
    padding: 2px;
    background: none;
    border: none;
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text-muted);
    cursor: pointer;
  }

  .close:hover {
    background: var(--ax-surface-3);
    color: var(--ax-danger);
  }

  .add:hover {
    background: var(--ax-surface-3);
    color: var(--ax-accent);
  }

  .add-menu {
    position: fixed;
    z-index: calc(var(--ax-z-staging) + 1);
    display: flex;
    flex-direction: column;
    padding: var(--ax-space-1);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    box-shadow: var(--ax-shadow-pop);
  }

  .add-menu button {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: none;
    border: none;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .add-menu button:hover,
  .add-menu button:focus-visible {
    background: var(--ax-surface-3);
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
