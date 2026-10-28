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
  import { coverGround } from "../core/cover";
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
  // A modal window covers the app's ground, so what moves on it holds still beneath the glass (core/cover.ts).
  $effect(() => (modal ? coverGround() : undefined));

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
      class="window ax-glass-panel ax-glass-scope"
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
  <div class="window docked ax-glass-panel ax-glass-scope" {style} role="dialog" aria-label={title}>
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
    background: var(--ax-glass-overlay);
  }

  /* A glass panel (docs/plans/glass.md, G3; `.ax-glass-panel` in styles.css): its surfaces see-through by the knob,
     a real blur of what lies beneath — the ground holds still under it (core/cover.ts). */
  .window {
    /* Positioned for its glass layer (`.ax-glass-panel`), which would otherwise cover the whole overlay. */
    position: relative;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-radius: var(--ax-radius-lg);
  }

  /* Docked (the chat), the window covers nothing and the Orbit beside it keeps moving: dense glass, no live blur,
     which would be computed again on every frame of the Orbit's spin. */
  .window.docked::before {
    background: var(--ax-glass-menu);
    -webkit-backdrop-filter: none;
    backdrop-filter: none;
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
</style>
