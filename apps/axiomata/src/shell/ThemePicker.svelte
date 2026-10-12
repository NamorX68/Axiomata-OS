<!--
  The dock's theme choice (`docs/plans/ui-ux-round.md` UX3): a popover with every theme as a chip — its own ground,
  surface and accent as a swatch, drawn from the theme's tokens by scoping `data-theme` on the swatch — instead of a
  button that cycles blindly through thirteen. A click switches and closes; Escape or a click elsewhere closes. Each
  chip shows a real piece of the app in its theme (UX7b, G21: `ui/ThemePreview.svelte`); hovering recolours nothing.
-->
<script lang="ts">
  import { activeTheme } from "../core/stores";
  import { THEMES, applyTheme } from "../core/themes";
  import ThemePreview from "../ui/ThemePreview.svelte";

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
          <ThemePreview theme={t.id} />
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
    width: min(calc(540px * var(--ax-ui-scale)), calc(100vw - 2 * var(--ax-space-4)));
    max-height: 70vh;
    overflow-y: auto;
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
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--ax-space-1);
  }

  .theme {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1);
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

  /* The theme in use is chosen, not alive: neutral, like every selection (the accent rule). */
  .theme.active {
    border-color: var(--ax-text-muted);
    background: var(--ax-selected-bg);
  }

  .name {
    padding: 0 var(--ax-space-1);
    font: var(--ax-type-caption);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
