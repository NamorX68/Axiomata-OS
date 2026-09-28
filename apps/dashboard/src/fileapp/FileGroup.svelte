<!--
  One group of the file app's tabs (`docs/plans/editor-look.md`, LK2): its tab
  bar, the slots its editors are moved into, and the drop highlight while a tab
  is dragged onto it.

  The editors are not rendered here: `FileAppView` renders every tab's editor
  once, flat, and moves it into its slot after each layout change
  (`ide/paneStore.ts`), so dragging a tab to another group keeps its cursor,
  undo history and unsaved text. The `data-ide-*` attributes are what the drag
  geometry (`ide/dock.ts`) is measured from.
-->
<script lang="ts">
  import { languageColor } from "../core/languageColors";
  import { SLOT_ATTR } from "../ide/paneStore";
  import Icon from "../ui/Icon.svelte";
  import { editorSettings } from "./editorSettings";
  import type { TabGroup } from "../ide/layout";
  import { tabOf } from "./fileDock";
  import { getFileDock } from "./fileDockContext";

  let { group }: { group: TabGroup } = $props();

  const view = getFileDock();
  const tabs = $derived(group.tabs.map(tabOf));
  const focused = $derived(view.focusedGroup() === group.id);

  /** A tab's tint (K14): its language's colour, or none when that is off or it is a new note. */
  function tint(file: { rel: string } | null): string | undefined {
    return $editorSettings.tabColors && file ? languageColor(file.rel) : undefined;
  }

  /** The drop highlight for this group, or `null` when the drag is elsewhere. */
  const highlight = $derived.by(() => {
    const target = view.hint();
    return target && target.nodeId === group.id ? target.side : null;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="group" class:focused data-ide-group={group.id} onpointerdowncapture={() => view.focusGroup(group.id)}>
  <div class="tabbar" data-ide-tabbar role="tablist" aria-label="Open files">
    {#each tabs as tab (tab.id)}
      <div
        class="tab"
        class:active={tab.id === group.active}
        class:preview={tab.preview}
        class:dragging={view.draggingTab() === tab.id}
        style:--lang={tint(tab.file)}
        data-ide-tab={tab.id}
      >
        <button
          type="button"
          role="tab"
          class="tab-title"
          aria-selected={tab.id === group.active}
          title={view.where(tab)}
          onpointerdown={(event) => view.startTabDrag(tab.id, event)}
          onclick={() => view.focusTab(tab.id)}
          ondblclick={() => view.pin(tab.id)}
        >
          {view.title(tab)}
          {#if view.dirty(tab.id)}<span class="dirty" aria-label="Unsaved changes">●</span>{/if}
        </button>
        <button
          type="button"
          class="tab-close"
          aria-label="Close {view.title(tab)}"
          onpointerdown={(event) => event.stopPropagation()}
          onclick={() => view.requestClose(tab.id)}><Icon name="x" size="sm" /></button
        >
      </div>
    {/each}
  </div>

  <div class="body">
    {#each tabs as tab (tab.id)}
      <div class="slot" class:hidden={tab.id !== group.active} inert={tab.id !== group.active} {...{ [SLOT_ATTR]: tab.id }}></div>
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
    min-width: 0;
    min-height: 0;
  }

  /* The tab bar (K7): tabs as floating pills, each tinted with its language (K14). */
  .tabbar {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1) var(--ax-space-2);
    overflow-x: auto;
    scrollbar-width: none;
    background: var(--ax-surface-1);
    border-bottom: 1px solid var(--ax-border);
  }

  .tab {
    /* The tab's colour; no tint (a new note, or tints off) falls back to the plain surface. */
    --tint: var(--lang, transparent);
    display: flex;
    align-items: center;
    flex-shrink: 0;
    max-width: calc(240px * var(--ax-ui-scale));
    border: 1px solid transparent;
    border-radius: var(--ax-radius-pill);
    background: color-mix(in srgb, var(--tint) 20%, var(--ax-surface-2));
    transition:
      background var(--ax-dur-fast) var(--ax-ease),
      border-color var(--ax-dur-fast) var(--ax-ease);
    /* A tab is a drag handle: a fast drag must not start selecting its title (as in the IDE). */
    user-select: none;
  }

  .tab:hover {
    background: color-mix(in srgb, var(--tint) 28%, var(--ax-surface-3));
  }

  .tab.active {
    background: color-mix(in srgb, var(--tint) 36%, var(--ax-surface-3));
    border-color: color-mix(in srgb, var(--tint) 60%, var(--ax-border));
  }

  /* The focused group's visible tab carries the accent: the one the keys are about. */
  .focused .tab.active {
    border-color: var(--ax-accent);
  }

  .tab.dragging {
    opacity: 0.5;
  }

  .tab-title,
  .tab-close {
    display: flex;
    align-items: center;
    background: none;
    border: 0;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .tab-title {
    overflow: hidden;
    padding: var(--ax-space-1) var(--ax-space-1) var(--ax-space-1) var(--ax-space-3);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tab.active .tab-title {
    color: var(--ax-text);
  }

  /* The preview tab (W7): italic until it becomes a tab of its own. */
  .tab.preview .tab-title {
    font-style: italic;
  }

  /* The ×: quiet until the tab is hovered or visible, then a small round button. */
  .tab-close {
    margin-right: var(--ax-space-1);
    padding: 2px;
    border-radius: var(--ax-radius-pill);
    opacity: 0;
  }

  .tab:hover .tab-close,
  .tab.active .tab-close {
    opacity: 1;
  }

  .tab-close:hover {
    background: var(--ax-surface-3);
    color: var(--ax-text);
  }

  .dirty {
    margin-left: var(--ax-space-1);
    color: var(--ax-accent);
  }

  .body {
    position: relative;
    flex: 1;
    min-height: 0;
  }

  .slot {
    position: absolute;
    inset: 0;
    display: flex;
  }

  /* Not `display: none`: a hidden editor keeps its measured size. */
  .slot.hidden {
    visibility: hidden;
    pointer-events: none;
  }

  .highlight {
    position: absolute;
    z-index: 10;
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
