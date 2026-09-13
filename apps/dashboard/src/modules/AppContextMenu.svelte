<!--
  App-Ring right-click menu: one action, "Entfernen", with a confirm step —
  no generic menu-item list, this is a single-purpose widget (see
  docs/plans/app-ring.md). Rendered by `second-brain.svelte` as a sibling of
  its `.brain` canvas div, never nested inside it — `position: fixed` makes
  the DOM nesting irrelevant for where this draws, but nesting it inside
  `.brain` would let a click on this menu's own buttons bubble up into
  `.brain`'s click handler afterwards (with `hover` still pointing at the
  node this menu was opened for), double-firing the app-click action right
  after "Entfernen"/"Abbrechen" already ran.

  "Abbrechen" (at either stage) calls `onCancel`, which the parent uses to
  close the whole menu — never a "back to the first stage" affordance, since
  a fresh right-click is cheap. `onRemove`/`onCancel` are the only ways this
  component decides to close; `second-brain.svelte` can also close it from
  the outside (a click anywhere else, `Escape`) by unsetting its own `menu`
  state. The `.app-context-menu` class name is load-bearing beyond styling:
  the parent's outside-click handler uses it to tell "a click inside this
  menu" from "a click that should dismiss it" — don't rename it without
  updating that check. `second-brain.svelte` also wraps this component in
  `{#key menu.node.id}`, so retargeting the menu to a different app (via a
  second right-click before confirming/cancelling the first) always remounts
  it fresh — this component's own `confirming` state deliberately has no
  memory of *which* node it was showing, by design, so that remount is what
  keeps a retarget from silently skipping the first "Entfernen" stage.
-->
<script lang="ts">
  let {
    x,
    y,
    label,
    onRemove,
    onCancel,
  }: {
    /** Viewport px — already clamped by the caller so the menu stays fully
     *  on screen. */
    x: number;
    y: number;
    /** The app's display name, shown in the confirm text. */
    label: string;
    onRemove: () => void;
    onCancel: () => void;
  } = $props();

  let confirming = $state(false);
</script>

<div class="app-context-menu" style:left="{x}px" style:top="{y}px" role="menu">
  {#if confirming}
    <p class="confirm">„{label}" aus dem Ring entfernen?</p>
    <div class="row">
      <button type="button" class="danger" onclick={onRemove}>Entfernen</button>
      <button type="button" onclick={onCancel}>Abbrechen</button>
    </div>
  {:else}
    <button type="button" class="item" onclick={() => (confirming = true)}>Entfernen</button>
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
       button shape at all, just the "Entfernen" action(s). */
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border: none;
    border-bottom: 2px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    box-shadow: var(--ax-shadow-drag);
  }

  .item {
    width: 100%;
    text-align: left;
    background: transparent;
    border-color: transparent;
  }
  .item:hover {
    background: var(--ax-surface-3);
  }

  .confirm {
    margin: 0 0 var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
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
