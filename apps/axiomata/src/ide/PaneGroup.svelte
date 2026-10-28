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

  let tabbarEl = $state<HTMLElement | undefined>();
  let bodyEl = $state<HTMLElement | undefined>();
  /** Where the active tab stands over the plate, in px from the plate's left edge: the plate's top edge is cut there,
   *  so tab and plate read as one outline (docs/plans/glass.md). `null` for a group of areas or no active tab. */
  let notch = $state<{ x: number; w: number } | null>(null);

  function measureNotch(): void {
    const tab = areas ? null : tabbarEl?.querySelector<HTMLElement>(".tab.active");
    if (!tab || !bodyEl) {
      notch = null;
      return;
    }
    const t = tab.getBoundingClientRect();
    const b = bodyEl.getBoundingClientRect();
    notch = { x: t.left - b.left, w: t.width };
  }

  // Again whenever the active tab, the tabs or the sizes change, and while the bar scrolls sideways.
  $effect(() => {
    void group.active;
    void group.tabs.length;
    const bar = tabbarEl;
    const body = bodyEl;
    if (!bar || !body) return;
    let frame = requestAnimationFrame(measureNotch);
    const again = (): void => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measureNotch);
    };
    const ro = new ResizeObserver(again);
    ro.observe(bar);
    ro.observe(body);
    bar.addEventListener("scroll", again, { passive: true });
    return () => {
      cancelAnimationFrame(frame);
      ro.disconnect();
      bar.removeEventListener("scroll", again);
    };
  });

  /** The drop highlight for this group, or `null` when the drag is elsewhere. */
  const highlight = $derived.by(() => {
    const target = dock.hint();
    return target && target.nodeId === group.id ? target.side : null;
  });
</script>

<div class="group" data-ide-group={group.id}>
  <div class="tabbar" class:areas data-ide-tabbar role="tablist" bind:this={tabbarEl}>
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

  <div
    class="body ax-glass-pane"
    class:notched={notch !== null}
    style:--notch-x={notch ? `${notch.x}px` : undefined}
    style:--notch-w={notch ? `${notch.w}px` : undefined}
    bind:this={bodyEl}
  >
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
    overflow: hidden;
  }

  /* The tab bar (`docs/plans/ui-ux-round.md` G4, replacing editor-look K7/K14). On glass (docs/plans/glass.md, owner
     2026-10-28) the tabs stand on the ground above the plate (`.body`): no strip, no line. The active tab grows out of
     the plate as a folder tab — one outline with it, curved shoulders where it meets the plate's top edge, which is
     cut beneath it (`.notched`); the others are their icon and name on the ground. The language sits in the file icon
     only, an agent's tab carries its hex avatar. Still the drop target of its group, only unseen. The bar starts past
     the plate's rounded corner, so a shoulder never meets the curve. */
  .tabbar {
    --shoulder: var(--ax-radius-md);
    display: flex;
    align-items: flex-end;
    gap: calc(2px * var(--ax-ui-scale));
    padding: var(--ax-glass-gap) var(--ax-glass-gap) 0 calc(var(--ax-glass-gap) + var(--ax-glass-radius));
    background: transparent;
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
    padding: var(--ax-space-2) var(--ax-space-2) var(--ax-space-2) var(--ax-space-4);
    min-height: calc(var(--ax-hit-min) + 6px * var(--ax-ui-scale));
    border: 1px solid transparent;
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
    background: var(--ax-surface-1);
    color: var(--ax-text);
  }

  /* The active tab: the plate's own glass and edge, open at the bottom and a pixel over the plate's cut edge, so the
     two are one surface — selected, without the accent (selected ≠ alive). */
  .tabbar:not(.areas) .tab.active {
    z-index: 1;
    margin-bottom: -1px;
    border-color: var(--ax-glass-edge);
    border-bottom-color: transparent;
    background: var(--ax-glass-pane);
    color: var(--ax-text);
    box-shadow: var(--ax-glass-light);
  }

  /* Its shoulders: a concave curve on each side, from the tab's side down into the plate's top edge — a quarter
     circle cut out of a square of the plate's glass, its rim the edge. */
  .tabbar:not(.areas) .tab.active::before,
  .tabbar:not(.areas) .tab.active::after {
    content: "";
    position: absolute;
    bottom: 0;
    width: var(--shoulder);
    height: var(--shoulder);
    pointer-events: none;
  }
  .tabbar:not(.areas) .tab.active::before {
    left: calc(-1 * var(--shoulder) - 1px);
    background: radial-gradient(
      circle at 0 0,
      transparent calc(var(--shoulder) - 1px),
      var(--ax-glass-edge) calc(var(--shoulder) - 1px),
      var(--ax-glass-edge) var(--shoulder),
      var(--ax-glass-pane) calc(var(--shoulder) + 0.5px)
    );
  }
  .tabbar:not(.areas) .tab.active::after {
    right: calc(-1 * var(--shoulder) - 1px);
    background: radial-gradient(
      circle at 100% 0,
      transparent calc(var(--shoulder) - 1px),
      var(--ax-glass-edge) calc(var(--shoulder) - 1px),
      var(--ax-glass-edge) var(--shoulder),
      var(--ax-glass-pane) calc(var(--shoulder) + 0.5px)
    );
  }

  /* An agent at work: its name takes the glow's colour (its avatar glows beside it). */
  .tab.live .title {
    color: var(--ax-glow);
  }

  .kind-icon {
    display: inline-flex;
    color: var(--ax-text-muted);
  }

  /* No fade from the glass knob: it is a multiplier on alphas (up to 2.2), no element opacity — at its default of 1
     the tab never faded anyway. */
  .tab.dragging {
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
  }

  .areas .tab {
    padding: 0;
    border: 0;
    border-radius: 0;
    font: var(--ax-type-label);
  }

  .areas .tab:hover,
  .areas .tab.active {
    background: none;
    box-shadow: none;
  }

  /* An area head's mark: a quiet line under the active one. */
  .areas .tab.active::before {
    content: "";
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: calc(2px * var(--ax-ui-scale));
    border-radius: calc(2px * var(--ax-ui-scale));
    background: var(--ax-text);
    opacity: 0.45;
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

  /* The glass plate (`.ax-glass-pane`), under the tabs: the content is cut to its rounded corners. It sits right
     under the tab bar, and its edge is drawn on its glass layer instead of as a border, so the layer can be cut
     where the active tab stands. */
  .body {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    margin-top: 0;
    overflow: hidden;
    border: 0;
  }
  .body::before {
    box-shadow:
      inset 0 0 0 1px var(--ax-glass-edge),
      var(--ax-glass-light);
  }

  /* Under the active tab and its shoulders the plate's top row is cut out of its glass layer: no edge runs through
     the folder tab, which covers that row with the same glass. */
  .body.notched::before {
    --cut-from: calc(var(--notch-x) - var(--ax-radius-md) - 1px);
    --cut-to: calc(var(--notch-x) + var(--notch-w) + var(--ax-radius-md) + 1px);
    --cut-mask:
      linear-gradient(#000 0 0),
      linear-gradient(
          to right,
          transparent var(--cut-from),
          #000 var(--cut-from),
          #000 var(--cut-to),
          transparent var(--cut-to)
        )
        0 0 / 100% 1px no-repeat;
    -webkit-mask: var(--cut-mask);
    mask: var(--cut-mask);
    -webkit-mask-composite: xor;
    mask-composite: exclude;
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
