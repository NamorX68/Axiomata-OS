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
  import type { IdeAgent } from "../core/backend";
  import FileIcon from "../fileapp/FileIcon.svelte";
  import HexAvatar from "../ui/HexAvatar.svelte";
  import Icon from "../ui/Icon.svelte";
  import { agentStatus, describeStatus, type StatusView } from "./agentStatus";
  import { getDock } from "./dockContext";
  import type { PaneTab, TabGroup } from "./layout";
  import { filePaneConfig, isAreaTab, TASK_PANE } from "./paneKinds";
  import { SLOT_ATTR } from "./paneStore";
  import { session } from "./projectSession";
  import { harnessBadge, harnessLabel } from "./rosterLook";
  import StatusDot from "./StatusDot.svelte";

  let { group }: { group: TabGroup } = $props();

  const dock = getDock();
  const statuses = agentStatus.statuses;

  /** The agent behind an agent tab; `null` for every other kind of tab, and for a deleted profile. */
  function tabAgent(tab: PaneTab): IdeAgent | null {
    const id = tab.kind === "agent" ? tab.config?.agentId : undefined;
    return typeof id === "number" ? ($session.agents.find((a) => a.id === id) ?? null) : null;
  }

  /** An agent tab's status, so a waiting agent is noticed even in a pane nobody is looking at (CP6). */
  function tabStatus(agent: IdeAgent | null): StatusView | null {
    return agent ? describeStatus($statuses.byAgent.get(agent.id), agent, $statuses.checkedAt) : null;
  }

  /** A group of areas (the Flow layout's Planning, Agents, Flow view) reads as section heads, not as tabs (G4). */
  const areas = $derived(group.tabs.length > 0 && group.tabs.every(isAreaTab));

  /** The drop highlight for this group, or `null` when the drag is elsewhere. */
  const highlight = $derived.by(() => {
    const target = dock.hint();
    return target && target.nodeId === group.id ? target.side : null;
  });
</script>

<div class="group" data-ide-group={group.id}>
  <div class="tabbar" class:areas data-ide-tabbar role="tablist">
    {#each group.tabs as tab (tab.id)}
      {@const agent = tabAgent(tab)}
      {@const view = tabStatus(agent)}
      {@const file = filePaneConfig(tab)}
      {@const live = view?.tone === "working"}
      <div
        class="tab"
        class:active={tab.id === group.active}
        class:dragging={dock.draggingTab() === tab.id}
        class:preview={file?.preview === true}
        class:pinned={tab.pinned}
        class:live
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
        {#if agent}
          <HexAvatar
            text={harnessBadge(agent.harness)}
            {live}
            ring="var(--ax-harness-{agent.harness})"
            size="sm"
            label={harnessLabel(agent.harness)}
          />
          {#if view && view.tone === "waiting"}<StatusDot {view} />{/if}
        {:else if file}
          <FileIcon name={file.rel} />
        {:else if tab.kind === "terminal" || tab.kind === TASK_PANE}
          <span class="kind-icon"><Icon name="terminal" size="sm" /></span>
        {/if}
        <span class="title">{tab.title}</span>
        {#if !tab.pinned}
          <button
            class="close"
            type="button"
            aria-label="Close {tab.title}"
            onpointerdown={(event) => event.stopPropagation()}
            onclick={() => dock.requestClose(tab.id)}><Icon name="x" size="sm" /></button
          >
        {/if}
      </div>
    {/each}
  </div>

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

  /* The tab bar (`docs/plans/ui-ux-round.md` G4, replacing editor-look K7/K14): a calm strip a step below the pane;
     the active tab is the pane's own surface, as if the text it shows grew out of it. The language sits in the file
     icon only, an agent's tab carries its hex avatar. */
  .tabbar {
    display: flex;
    align-items: flex-end;
    gap: calc(2px * var(--ax-ui-scale));
    padding: var(--ax-space-1) var(--ax-space-2) 0;
    background: color-mix(in srgb, var(--ax-surface-1) 55%, var(--ax-bg));
    border-bottom: 1px solid var(--ax-border);
    /* The bar is a drop target of its own; it must not shrink away. */
    flex: 0 0 auto;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-1) var(--ax-space-1) var(--ax-space-3);
    min-height: var(--ax-hit-min);
    border-radius: var(--ax-radius-md) var(--ax-radius-md) 0 0;
    color: var(--ax-text-muted);
    font: var(--ax-type-body);
    font-weight: 500;
    white-space: nowrap;
    cursor: grab;
    user-select: none;
    transition:
      background var(--ax-dur-fast) var(--ax-ease),
      color var(--ax-dur-fast) var(--ax-ease);
  }

  /* No close button to leave room for. */
  .tab.pinned {
    padding-right: var(--ax-space-3);
  }

  .tab:hover {
    color: var(--ax-text);
  }

  .tab.active {
    margin-bottom: -1px;
    padding-bottom: calc(var(--ax-space-1) + 1px);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    box-shadow: inset 0 1px 0 var(--ax-edge-light);
  }

  /* A quiet line along the top of the active tab: where the eye finds it, without the accent (selected ≠ alive). */
  .tab.active::before {
    content: "";
    position: absolute;
    top: 0;
    left: var(--ax-space-3);
    right: var(--ax-space-3);
    height: calc(2px * var(--ax-ui-scale));
    border-radius: calc(2px * var(--ax-ui-scale));
    background: var(--ax-text);
    opacity: 0.45;
  }

  /* An agent at work: its name takes the glow's colour (its avatar glows beside it). */
  .tab.live .title {
    color: var(--ax-glow);
  }

  .kind-icon {
    display: inline-flex;
    color: var(--ax-text-muted);
  }

  .tab.dragging {
    opacity: var(--ax-tile-glass-opacity);
    cursor: grabbing;
  }

  .tab:focus-visible {
    outline: 2px solid var(--ax-focus-ring);
    outline-offset: -2px;
  }

  /* The preview tab (W7): italic until it becomes a tab of its own. */
  .tab.preview .title {
    font-style: italic;
  }

  /* Areas (G4): the Flow layout's Planning, Agents and Flow view are section heads in the label role, not tabs. */
  .tabbar.areas {
    gap: var(--ax-space-4);
    padding: 0 var(--ax-space-4);
    background: var(--ax-surface-1);
  }

  .areas .tab {
    padding: 0;
    border-radius: 0;
    font: var(--ax-type-label);
    letter-spacing: var(--ax-tracking-label);
    text-transform: uppercase;
  }

  .areas .tab.active {
    margin-bottom: 0;
    padding-bottom: 0;
    background: none;
    box-shadow: none;
  }

  .areas .tab.active::before {
    top: auto;
    bottom: 0;
    left: 0;
    right: 0;
  }

  .close {
    display: flex;
    align-items: center;
    padding: calc(2px * var(--ax-ui-scale));
    background: none;
    border: none;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    cursor: pointer;
    /* Quiet until the tab is active, hovered or focused: a calm bar is not a row of crosses. */
    opacity: 0;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }

  .tab.active .close,
  .tab:hover .close,
  .close:focus-visible {
    opacity: 1;
  }

  .close:hover {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
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
