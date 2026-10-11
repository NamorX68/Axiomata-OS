<!--
  The dock's theme choice (`docs/plans/ui-ux-round.md` UX3): a popover with every theme as a chip — its own ground,
  surface and accent as a swatch, drawn from the theme's tokens by scoping `data-theme` on the swatch — instead of a
  button that cycles blindly through thirteen. A click switches and closes; Escape or a click elsewhere closes. The
  richer preview with a real piece of the app is UX7 (G21).
-->
<script lang="ts">
  import { activeTheme } from "../core/stores";
  import { THEMES, applyTheme } from "../core/themes";

  let { onClose }: { onClose: () => void } = $props();

  let root = $state<HTMLElement | null>(null);

  function choose(id: string): void {
    applyTheme(id);
    onClose();
  }

  // In the capture phase: the picker mounts late, so the panels' own Escape listeners would hear the key first and
  // close the panel under it instead.
  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  }

  // A press outside closes it; the dock's own theme button toggles it and is left to do so.
  function onPointerDown(e: PointerEvent): void {
    const target = e.target;
    if (!(target instanceof Node) || root?.contains(target)) return;
    if (target instanceof Element && target.closest("[data-theme-toggle]")) return;
    onClose();
  }
</script>

<svelte:window onkeydowncapture={onKeydown} onpointerdown={onPointerDown} />

<div class="picker" role="dialog" aria-label="Theme" bind:this={root}>
  <ul>
    {#each THEMES as t (t.id)}
      <li>
        <button
          type="button"
          class="theme"
          class:active={$activeTheme === t.id}
          title={t.blurb}
          onclick={() => choose(t.id)}
        >
          <span class="swatch" data-theme={t.id} aria-hidden="true"><i></i><i></i><i></i></span>
          <span class="name">{t.label}</span>
        </button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .picker {
    position: absolute;
    top: calc(100% + var(--ax-space-2));
    right: 0;
    width: calc(420px * var(--ax-ui-scale));
    padding: var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-lg);
    background: var(--ax-elev-3);
    box-shadow: var(--ax-shadow-panel);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--ax-space-1);
  }

  .theme {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid transparent;
    border-radius: var(--ax-radius-md);
    background: transparent;
    color: var(--ax-text);
    font: var(--ax-type-body);
    text-align: left;
    cursor: pointer;
  }

  .theme:hover {
    background: var(--ax-selected-bg);
  }

  /* The theme in use is the selection: the accent's one allowed place here. */
  .theme.active {
    border-color: var(--ax-accent);
  }

  .swatch {
    display: inline-flex;
    gap: calc(2px * var(--ax-ui-scale));
    flex: none;
  }

  .swatch i {
    width: calc(8px * var(--ax-ui-scale));
    height: calc(18px * var(--ax-ui-scale));
    border-radius: calc(2px * var(--ax-ui-scale));
  }

  .swatch i:nth-child(1) {
    background: var(--ax-bg);
    border: 1px solid var(--ax-border-strong);
  }

  .swatch i:nth-child(2) {
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border-strong);
  }

  .swatch i:nth-child(3) {
    background: var(--ax-accent);
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
