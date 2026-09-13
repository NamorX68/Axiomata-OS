<!--
  Shared "window" chrome for every floating dialog/panel in the app: the
  glass background + bottom-hairline + `--ax-shadow-drag` look
  `StagingLayer.svelte` (the file-viewer panel) and `canvas/Tile.svelte`'s
  frameless front face established, extracted here so every dialog
  (`ModulePicker`, `Settings`, `AppAddDialog`, `ChatPanel`, …) shares one
  definition instead of retyping the same CSS in each file.

  This component owns *how a window looks* (background, border, shadow,
  header, close-button behaviour) — never *where it sits or how big it is*.
  `modal={true}` (default) centres it inside a full-screen scrim, the
  classic dialog shape, closing on Escape or a click on the scrim.
  `modal={false}` renders just the card with no scrim, for an
  always-visible docked panel like `ChatPanel`; the caller positions and
  sizes it via the `style` prop (a raw inline-style string, not a `class` —
  a class passed in as a prop would carry *this* component's own Svelte
  scoping attribute, not the caller's, so the caller's own `<style>` block
  could never actually target it; a plain inline style has no such
  scoping problem).

  The close button is hidden until the window is hovered/focused, matching
  `canvas/Tile.svelte`'s front-head `.tile-btn` behaviour — not a
  permanently-visible button.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    icon,
    onClose,
    modal = true,
    style = "",
    headerExtra,
    children,
  }: {
    title: string;
    /** Raw SVG markup, same convention as `ModuleDefinition.icon`. */
    icon?: string;
    onClose: () => void;
    modal?: boolean;
    /** Positioning/sizing, applied as a literal inline style on the window
     *  card — see the component doc comment for why this is a `style`
     *  string and not a `class`. */
    style?: string;
    /** Extra header content between the title and the close button (e.g.
     *  `ChatPanel`'s session id + "New session" button) — the title/icon/
     *  close-button trio stays the one fixed part of the header every
     *  window shares, this is the escape hatch for the rare one that needs
     *  more than that. */
    headerExtra?: Snippet;
    children: Snippet;
  } = $props();

  // `!e.defaultPrevented` + calling `preventDefault()` on the way out is
  // the coordination convention `StagingLayer`/`ChatPanel`/`SecondBrainView`
  // already use for their own global Escape listeners, so that with two+
  // windows open at once, one Escape press closes only the topmost rather
  // than cascading through all of them in the same keystroke.
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if modal}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="overlay" onclick={onClose}>
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
    <div
      class="window"
      {style}
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
    >
      <header>
        {#if icon}<span class="icon" aria-hidden="true">{@html icon}</span>{/if}
        <h2>{title}</h2>
      {#if headerExtra}{@render headerExtra()}{/if}
        <button type="button" class="close" aria-label="Close" onclick={onClose}>×</button>
      </header>
      <div class="body">
        {@render children()}
      </div>
    </div>
  </div>
{:else}
  <div class="window" {style} role="dialog" aria-label={title}>
    <header>
      {#if icon}<span class="icon" aria-hidden="true">{@html icon}</span>{/if}
      <h2>{title}</h2>
      {#if headerExtra}{@render headerExtra()}{/if}
      <button type="button" class="close" aria-label="Close" onclick={onClose}>×</button>
    </header>
    <div class="body">
      {@render children()}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--ax-z-dialog);
    display: grid;
    place-items: center;
    background: var(--ax-overlay);
  }

  .window {
    display: flex;
    flex-direction: column;
    border-radius: var(--ax-radius-lg);
    /* Permanently-on glass, unlike Tile's own frameless front face (which
       is fully transparent at rest, glass only while dragging/resizing) —
       a window floats above arbitrary other content the whole time it's
       open, so the fully transparent resting state that look is built on
       would be unreadable here. */
    background: var(--ax-tile-glass-bg);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    border: none;
    border-bottom: 2px solid var(--ax-border-strong);
    box-shadow: var(--ax-shadow-drag);
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    flex: 0 0 auto;
  }
  .icon {
    display: inline-flex;
    width: 16px;
    height: 16px;
    color: var(--ax-accent);
  }
  .icon :global(svg) {
    width: 100%;
    height: 100%;
  }
  h2 {
    flex: 1 1 auto;
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }
  /* Matches canvas/Tile.svelte's .tile-btn exactly. */
  .close {
    width: 22px;
    height: 22px;
    padding: 0;
    display: grid;
    place-items: center;
    line-height: 1;
    font-size: var(--ax-font-size-lg);
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
    opacity: 0;
    transition: opacity var(--ax-dur-fast) var(--ax-ease);
  }
  .window:hover .close,
  .window:focus-within .close {
    opacity: 1;
  }
  .close:hover:not(:disabled) {
    color: var(--ax-text);
    background: var(--ax-surface-3);
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
</style>
