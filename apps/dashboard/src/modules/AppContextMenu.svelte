<!--
  App-Ring right-click menu: content is entirely driven by `actions`
  (`core/appGroups.ts`'s `menuActionsFor` — the single source of truth for
  "which state shows which actions", see its own doc comment) rather than
  this component deciding anything about node kind/grouping itself.
  Rendered by `second-brain.svelte` as a sibling of its `.brain` canvas div,
  never nested inside it — `position: fixed` makes the DOM nesting
  irrelevant for where this draws, but nesting it inside `.brain` would let
  a click on this menu's own buttons bubble up into `.brain`'s click
  handler afterwards (with `hover` still pointing at the node this menu was
  opened for), double-firing the app-click action right after one of this
  menu's own actions already ran.

  Four stages: "menu" (one button per `actions` entry — grouping/rename/
  icon actions fire their callback immediately, no confirmation, same
  "a deliberate click on one row of a list is structurally far less likely
  to be accidental" reasoning `AppAddDialog`'s own toggle already relies on;
  only "Entfernen" needs a confirm step, since it's the one genuinely
  destructive action here), "confirmRemove" (unchanged two-button confirm),
  "rename" (a text field, pre-filled with the current name), "changeIcon"
  (embeds `GlyphPicker`, picking a glyph commits immediately — no separate
  confirm, matching the picker's own click-to-pick convention).

  "Abbrechen" (at every stage that has one) calls `onCancel`, which the
  parent uses to close the whole menu — never a "back to the menu stage"
  affordance, since a fresh right-click is cheap. Every action callback
  (including a successful rename/icon-pick) also closes the whole menu on
  the parent's side, for the same reason. `onCancel` is also reachable via
  `Escape` (wired one level up, in `second-brain.svelte`) and a click
  outside the menu, regardless of stage.

  The `.app-context-menu` class name is load-bearing beyond styling: the
  parent's outside-click handler uses it to tell "a click inside this menu"
  from "a click that should dismiss it" — don't rename it without updating
  that check. `second-brain.svelte` also wraps this component in
  `{#key menu.node.id}`, so retargeting the menu to a different node (via a
  second right-click before finishing the current one) always remounts it
  fresh — this component's own `stage`/`renameValue` state deliberately has
  no memory of *which* node it was showing, by design, so that remount is
  what keeps a retarget from silently landing mid-flow on the wrong node.
-->
<script lang="ts">
  import type { AppMenuAction } from "../core/appGroups";
  import GlyphPicker from "../graph/GlyphPicker.svelte";

  let {
    x,
    y,
    label,
    glyph,
    actions,
    onRemove,
    onAddToNewGroup,
    onAddToGroup,
    onRemoveFromGroup,
    onRename,
    onChangeIcon,
    onCancel,
  }: {
    /** Viewport px — already clamped by the caller so the menu stays fully
     *  on screen. */
    x: number;
    y: number;
    /** The node's display name, shown in the "Entfernen" confirm text and
     *  as the rename field's starting value. */
    label: string;
    /** The node's current glyph — only read by the "changeIcon" stage's
     *  picker, to show which one is already selected. */
    glyph: string;
    actions: AppMenuAction[];
    onRemove: () => void;
    onAddToNewGroup: () => void;
    onAddToGroup: (groupId: string) => void;
    onRemoveFromGroup: () => void;
    onRename: (name: string) => void;
    onChangeIcon: (glyph: string) => void;
    onCancel: () => void;
  } = $props();

  let stage = $state<"menu" | "confirmRemove" | "rename" | "changeIcon">("menu");
  // Seeds the editable field from the prop once — `label` itself never
  // changes after mount in practice (the parent remounts this whole
  // component via `{#key menu.node.id}` on retarget), so only the initial
  // value matters here, not a live binding.
  // svelte-ignore state_referenced_locally
  let renameValue = $state(label);

  function submitRename() {
    const trimmed = renameValue.trim();
    if (trimmed) onRename(trimmed);
  }
</script>

<div class="app-context-menu" style:left="{x}px" style:top="{y}px" role="menu">
  {#if stage === "menu"}
    {#each actions as action}
      {#if action.kind === "addToNewGroup"}
        <button type="button" class="item" onclick={onAddToNewGroup}>Zu neuer Gruppe hinzufügen</button>
      {:else if action.kind === "addToGroup"}
        <button type="button" class="item" onclick={() => onAddToGroup(action.groupId)}>
          Zu Gruppe „{action.groupName}" hinzufügen
        </button>
      {:else if action.kind === "removeFromGroup"}
        <button type="button" class="item" onclick={onRemoveFromGroup}>Aus Gruppe entfernen</button>
      {:else if action.kind === "rename"}
        <button type="button" class="item" onclick={() => (stage = "rename")}>Umbenennen</button>
      {:else if action.kind === "changeIcon"}
        <button type="button" class="item" onclick={() => (stage = "changeIcon")}>Symbol ändern</button>
      {:else if action.kind === "remove"}
        <button type="button" class="item" onclick={() => (stage = "confirmRemove")}>Entfernen</button>
      {/if}
    {/each}
  {:else if stage === "confirmRemove"}
    <p class="confirm">„{label}" aus dem Ring entfernen?</p>
    <div class="row">
      <button type="button" class="danger" onclick={onRemove}>Entfernen</button>
      <button type="button" onclick={onCancel}>Abbrechen</button>
    </div>
  {:else if stage === "rename"}
    <label class="field-label" for="app-context-rename">Neuer Name</label>
    <input
      id="app-context-rename"
      class="rename-input"
      type="text"
      bind:value={renameValue}
      onkeydown={(e) => e.key === "Enter" && submitRename()}
    />
    <div class="row">
      <button type="button" onclick={submitRename}>Übernehmen</button>
      <button type="button" onclick={onCancel}>Abbrechen</button>
    </div>
  {:else if stage === "changeIcon"}
    <span class="field-label">Symbol wählen</span>
    <GlyphPicker selected={glyph} onPick={onChangeIcon} />
  {/if}
</div>

<style>
  .app-context-menu {
    position: fixed;
    z-index: var(--ax-z-dialog);
    min-width: 160px;
    padding: var(--ax-space-2);
    /* Same glass/hairline/elevated-shadow language as Window.svelte and
       every other panel — a plain rgba surface + full border would read as
       the "classic dialog" look everything else just moved away from. Not
       wrapped in Window.svelte itself: this has no header/title/close-
       button shape at all, just a short action list. */
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border: none;
    border-bottom: 2px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-drag);
  }

  .item {
    display: block;
    width: 100%;
    text-align: left;
    background: transparent;
    border-color: transparent;
  }
  .item + .item {
    margin-top: 2px;
  }
  .item:hover {
    background: var(--ax-surface-3);
  }

  .confirm {
    margin: 0 0 var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
  }

  .field-label {
    display: block;
    margin-bottom: var(--ax-space-1);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }

  .rename-input {
    display: block;
    width: 100%;
    margin-bottom: var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font: inherit;
    padding: var(--ax-space-1) var(--ax-space-2);
  }

  .row {
    display: flex;
    gap: var(--ax-space-2);
  }
  .row button {
    flex: 1 1 auto;
  }

  .danger {
    background: var(--ax-danger);
    border-color: var(--ax-danger);
    color: var(--ax-text-invert);
    font-weight: 600;
  }
</style>
