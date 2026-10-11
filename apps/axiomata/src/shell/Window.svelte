<!--
  Shared chrome for every floating dialog in the app (`ModulePicker`, `Settings`, `AppAddDialog`, `ChatPanel`, …):
  the panel surface of the elevation scale (`--ax-elev-2` with its light edge and `--ax-shadow-panel`) and the one
  panel head (`ui/PanelHead.svelte`) — the same look the staged panels (`StagingPanel.svelte`) wear.

  This component owns *how a window looks and closes* — never *where it sits or how big it is*. `modal={true}`
  (default) centres it inside a full-screen scrim, the classic dialog shape, closing on Escape, ⌘W or a click on the
  scrim, and grows out of whatever opened it (`ui/fromOrigin.ts`). `modal={false}` renders just the card with no
  scrim, for an always-visible docked panel like `ChatPanel`, which moves itself; the caller positions and sizes it via
  the `style` prop (a raw inline-style string, not a `class` — a class passed in as a prop would carry *this*
  component's own Svelte scoping attribute, not the caller's, so the caller's own `<style>` block could never actually
  target it; a plain inline style has no such scoping problem).
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade } from "svelte/transition";

  import { pushClose } from "../core/closeStack";
  import PanelHead from "../ui/PanelHead.svelte";
  import { fromOrigin } from "../ui/fromOrigin";

  let {
    title,
    icon,
    subtitle,
    onClose,
    modal = true,
    style = "",
    headerExtra,
    children,
  }: {
    title: string;
    /** Raw SVG markup, same convention as `ModuleDefinition.icon`. */
    icon?: string;
    /** A caption after the title (a session, a count). */
    subtitle?: string;
    onClose: () => void;
    modal?: boolean;
    /** Positioning/sizing, applied as a literal inline style on the window
     *  card — see the component doc comment for why this is a `style`
     *  string and not a `class`. */
    style?: string;
    /** The window's own controls in the head, between the title and the close button (e.g. `ChatPanel`'s "New
     *  session"). */
    headerExtra?: Snippet;
    children: Snippet;
  } = $props();

  // ⌘W closes the newest dialog (`App.svelte`); the latest `onClose` is the one called. Only a modal one: the docked
  // chat is no dialog, and ⌘W must not close it from under the file window the owner is working in.
  $effect(() => (modal ? pushClose(() => onClose()) : undefined));

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

{#snippet card()}
  <PanelHead {title} iconSvg={icon} {subtitle} actions={headerExtra} {onClose} />
  <div class="body">
    {@render children()}
  </div>
{/snippet}

{#if modal}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="overlay" onclick={onClose} transition:fade|global={{ duration: 160 }}>
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
    <div
      class="window"
      {style}
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      transition:fromOrigin|global
    >
      {@render card()}
    </div>
  </div>
{:else}
  <div class="window" {style} role="dialog" aria-label={title}>
    {@render card()}
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

  /* The panel step of the elevation scale: a tone, the light edge along the top and a shadow that carries in the
     dark — a hint of glass behind it, but never so much that the canvas shows through the text. */
  .window {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-lg);
    background: color-mix(in srgb, var(--ax-elev-2) 94%, transparent);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    box-shadow: var(--ax-shadow-panel);
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
</style>
