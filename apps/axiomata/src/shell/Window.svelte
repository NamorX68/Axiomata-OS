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

  Two opt-in exceptions to "never where or how big", off by default so every other window stays as it is:
  `movable` lets the owner drag the window by its head, and `resizable` gives it grips on its edges and corners
  (`ui/ResizeEdges.svelte`), starting at `defaultSize`. The geometry is the Second Brain detail window's
  (`floatingRect.ts`): unscaled units drawn times the UI scale, a minimum size, part of the window always on screen,
  the grabbed edge following the pointer while the opposite one stays. With a `sizeKey` the size is remembered across
  restarts (setting `windowSizes`, the way `stagingPanelSizes` files a staged panel's); the position never is — each
  opening comes up centred again, the way a staged panel does.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { get } from "svelte/store";
  import { fade } from "svelte/transition";

  import { draggable, type DragDelta } from "../canvas/drag";
  import type { ResizeDelta, ResizeDir } from "../canvas/resize";
  import { pushClose } from "../core/closeStack";
  import { coverGround } from "../core/cover";
  import { getSetting, setSetting } from "../core/persist";
  import { readPanelSize, rememberedSize, rememberingSize, type PanelSize } from "../core/staging";
  import { uiScale } from "../core/uiScale";
  import PanelHead from "../ui/PanelHead.svelte";
  import ResizeEdges from "../ui/ResizeEdges.svelte";
  import { fromOrigin } from "../ui/fromOrigin";
  import { clampRect, clampSize, moved, resizedWithin, windowStyle, type MinSize, type Rect } from "./floatingRect";

  /** The smallest a resizable window gets by default — a staged panel's minimum (`StagingPanel.svelte`). */
  const DEFAULT_MIN_SIZE: MinSize = { w: 480, h: 240 };
  /** The share of the screen a window may take when it opens, the way a staged panel's size is held to it: a freshly
   *  opened window never covers everything, though it may be pulled that big. */
  const OPEN_SHARE = 0.9;

  let {
    title,
    icon,
    subtitle,
    onClose,
    modal = true,
    style = "",
    headerExtra,
    movable = false,
    resizable = false,
    defaultSize,
    sizeKey,
    minSize = DEFAULT_MIN_SIZE,
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
    /** The head is a handle the window is dragged by. */
    movable?: boolean;
    /** Grips on the edges and corners; the window's size is then this component's, not the `style`'s. */
    resizable?: boolean;
    /** With `resizable`: the size (unscaled px) a window nobody has resized yet starts at. */
    defaultSize?: PanelSize;
    /** With `resizable`: what the size is remembered under across restarts; without one it lasts one opening. */
    sizeKey?: string;
    /** With `resizable`: the smallest the window may get (unscaled px). */
    minSize?: MinSize;
    children: Snippet;
  } = $props();

  // ⌘W closes the newest dialog (`App.svelte`); the latest `onClose` is the one called. Only a modal one: the docked
  // chat is no dialog, and ⌘W must not close it from under the file window the owner is working in.
  $effect(() => (modal ? pushClose(() => onClose()) : undefined));
  // A window covers the app's ground — the docked chat too, it lies over the Orbit — so what moves on it holds still
  // beneath the glass (core/cover.ts).
  $effect(() => coverGround());

  // ---- moving and resizing (opt-in) ----

  /** The setting the remembered sizes are filed in, by `sizeKey`. */
  const SIZES_SETTING = "windowSizes";

  // The screen in unscaled units, the space the window's geometry lives in.
  const screen = () => ({ w: window.innerWidth / get(uiScale), h: window.innerHeight / get(uiScale) });

  /** The remembered size, else the default — fitted to the screen it opens on, which may be smaller than the one it
   *  was resized on. `null`: the `style` sizes the window. */
  function initialSize(): PanelSize | null {
    if (!resizable) return null;
    const saved = sizeKey ? rememberedSize(getSetting<unknown>(SIZES_SETTING), sizeKey) : null;
    const start = saved ?? readPanelSize(defaultSize);
    const view = screen();
    return start ? clampSize(start, { w: view.w * OPEN_SHARE, h: view.h * OPEN_SHARE }, minSize) : null;
  }

  let windowEl = $state<HTMLElement | null>(null);
  let size = $state<PanelSize | null>(initialSize());
  /** Where the window was put (unscaled); `null` while it sits where it opened, centred. */
  let pos = $state<{ x: number; y: number } | null>(null);
  /** A drag or a resize under way. */
  let gesture = $state<"move" | "resize" | null>(null);
  let gestureBase: Rect = { x: 0, y: 0, w: 0, h: 0 };

  /** The window as it lies now. The first gesture takes it from its centred place to explicit `left`/`top`, so a
   *  pulled left or top edge can follow the pointer while the opposite edge stays. */
  function beginGesture(kind: "move" | "resize") {
    const scale = get(uiScale);
    const rect = windowEl?.getBoundingClientRect();
    const box = rect
      ? { x: rect.left / scale, y: rect.top / scale, w: rect.width / scale, h: rect.height / scale }
      : { x: 0, y: 0, w: minSize.w, h: minSize.h };
    gestureBase = size ? { ...box, w: size.w, h: size.h } : box;
    pos = pos ?? { x: gestureBase.x, y: gestureBase.y };
    gesture = kind;
  }

  function place(rect: Rect) {
    pos = { x: rect.x, y: rect.y };
    if (resizable) size = { w: rect.w, h: rect.h };
  }

  function dragTo(onScreen: DragDelta) {
    const scale = get(uiScale);
    place(clampRect(moved(gestureBase, onScreen.dx / scale, onScreen.dy / scale), screen(), minSize));
  }

  function pullTo(dir: ResizeDir, onScreen: ResizeDelta) {
    const scale = get(uiScale);
    const grow = { dw: onScreen.dw / scale, dh: onScreen.dh / scale };
    const view = screen();
    place(clampRect(resizedWithin(gestureBase, dir, grow, view, minSize), view, minSize));
  }

  function endPull(dir: ResizeDir, onScreen: ResizeDelta) {
    pullTo(dir, onScreen);
    gesture = null;
    if (sizeKey && size) {
      setSetting(SIZES_SETTING, rememberingSize(getSetting<unknown>(SIZES_SETTING), sizeKey, size));
    }
  }

  const dragOptions = $derived({
    handle: ".panel-head",
    onStart: () => beginGesture("move"),
    onMove: dragTo,
    onEnd: (d: DragDelta) => {
      dragTo(d);
      gesture = null;
    },
  });

  /** The caller's `style` with the gesture's geometry after it — one attribute, never `style:` directives beside it
   *  (`windowStyle` says why). */
  const inlineStyle = $derived(windowStyle(style, pos, size, $uiScale));

  /** `use:draggable` only on a movable window; an action cannot be switched on and off by itself. */
  function maybeDraggable(node: HTMLElement, options: typeof dragOptions) {
    return movable ? draggable(node, options) : undefined;
  }

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
  {#if resizable}
    <ResizeEdges
      corners
      active={gesture === "resize"}
      onStart={() => beginGesture("resize")}
      onMove={pullTo}
      onEnd={endPull}
    />
  {/if}
  <PanelHead {title} iconSvg={icon} {subtitle} actions={headerExtra} {onClose} {movable} />
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
      class:movable
      class:resizable
      class:positioned={pos !== null}
      class:gesture={gesture !== null}
      style={inlineStyle}
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      bind:this={windowEl}
      onclick={(e) => e.stopPropagation()}
      transition:fromOrigin|global
      use:maybeDraggable={dragOptions}
    >
      {@render card()}
    </div>
  </div>
{:else}
  <div
    class="window ax-glass-panel ax-glass-scope"
    class:movable
    class:resizable
    class:positioned={pos !== null}
    class:gesture={gesture !== null}
    style={inlineStyle}
    role="dialog"
    aria-label={title}
    bind:this={windowEl}
    use:maybeDraggable={dragOptions}
  >
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

  /* Dragged by its head: the browser's own press-and-move gestures (text selection, touch scrolling) must not win the
     race — `StagingPanel.svelte`'s reasoning; the body takes vertical scrolling back below. */
  .window.movable {
    touch-action: none;
  }
  .window.movable .body {
    touch-action: pan-y;
  }
  /* The grips reach a little past the border (`ui/ResizeEdges.svelte`); clipping would halve them. The glass layer
     keeps the rounded shape by itself (`border-radius: inherit`), the way a staged panel's does. */
  .window.resizable {
    overflow: visible;
  }
  /* Moved or resized once: explicit `left`/`top` (inline) replace the overlay's centring. Still positioned, as the
     glass layer needs. */
  .window.positioned {
    position: fixed;
    margin: 0;
  }
  .window.gesture {
    user-select: none; /* a fast drag could otherwise outrun the pointer and select the window's own text */
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
</style>
