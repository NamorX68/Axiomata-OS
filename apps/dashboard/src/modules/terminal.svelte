<!--
  terminal — Checkpoint 3 of docs/plans/terminal.md: a real canvas renderer
  replaces the interim `<pre>` text view from Checkpoint 2. Every
  `TerminalEvent::Screen` now carries full `Cell` data (colour, bold,
  underline), and `TerminalScreen.ts`'s pure `draw()` paints it — same
  "redraw the whole grid from the model every update" approach Checkpoint 2
  established, just onto a canvas instead of joining plain-text lines. A
  blinking block cursor is drawn on top of whatever's already there,
  timed off `requestAnimationFrame`'s own clock (no extra timer) —
  the animation loop runs continuously while a session is alive, the same
  "redraw every frame regardless of whether new data arrived" choice
  `second-brain.svelte`'s own loop already makes, for the same reason: the
  cursor blinks even when nothing else on screen is changing.

  Row/column count is now genuinely measured (`TerminalScreen.measureChar`
  against the canvas's own resolved `--ax-font-mono`/`--ax-font-size-sm`),
  not Checkpoint 1's guessed average cell size, and a `ResizeObserver` keeps
  it in sync as the tile is resized — feeding `terminal_resize` (on the
  backend since Checkpoint 1, unused until now), which resizes the PTY
  (`SIGWINCH` for the shell) and the screen model together.

  Input has no local echo: keystrokes are forwarded to the shell as bytes
  (via the hidden-ish text field below, which is cleared on every native
  `input` event rather than trusted as terminal state) and what's typed only
  appears once the shell's own PTY echoes it back over `on_output` — same as
  any real terminal with local echo off. Only Enter/Backspace/Tab/Escape and
  Ctrl+<letter> get their own C0 control byte; arrow-key history navigation
  and other escape-sequence input are a later checkpoint's concern.

  Two outcomes are handled very differently (architecture review,
  Checkpoint 1): the shell ending — typing `exit`, or a write hitting a
  session the backend already tore down — is completely ordinary: the last
  real screen just freezes (no more cursor blink) and a small banner says
  so, replacing Checkpoint 1/2's inline "[process exited]" scrollback line
  now that there's no scrolling text buffer left to append it to. Only
  `terminal_spawn` itself failing (no shell could even be started) is a
  fatal error that replaces the tile's content.
-->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Channel } from "@tauri-apps/api/core";

  import type { ModuleContext } from "../core/types";
  import { draw, measureChar, type CharMetrics, type TermCell } from "./TerminalScreen";
  import { keyToBytes } from "./terminalInput";

  let { ctx }: { ctx: ModuleContext } = $props();

  const MIN_ROWS = 4;
  const MIN_COLS = 20;
  /** Classic terminal cursor blink period — on/off every half-period. */
  const CURSOR_BLINK_MS = 530;

  /** Mirrors the backend's `TerminalEvent` (`src-tauri/src/terminal.rs`).
   *  Field names are snake_case, not camelCase: this rides over a raw
   *  `Channel` payload, not a `#[tauri::command]` argument list, so none of
   *  `invoke`'s usual camelCase<->snake_case bridging applies here. */
  type TerminalEvent = { type: "screen"; rows: TermCell[][]; cursor_row: number; cursor_col: number } | { type: "exited" };

  let root = $state<HTMLDivElement>();
  let canvasEl = $state<HTMLCanvasElement>();
  let inputEl = $state<HTMLInputElement>();
  let error = $state("");
  /** The shell ended (on its own, or a write against it failed) — the last
   *  drawn screen just stays frozen; see the component doc comment. */
  let ended = $state(false);
  let sessionId: string | null = null;

  // The latest screen snapshot and cursor position, redrawn every animation
  // frame (not only when a new one arrives) so the cursor can blink.
  let screenRows: TermCell[][] = [];
  let cursorPos: { row: number; col: number } | null = null;

  // Resolved once at mount and whenever the theme changes, not read fresh
  // every animation frame — unlike the font (see `currentFont`), these
  // don't need to track a live resize, just a theme swap.
  let defaultFg = "#f2f2f5";
  let defaultBg = "#17171c";
  let cursorColor = "#ff7a1a";

  let context2d: CanvasRenderingContext2D | null = null;
  let metrics: CharMetrics | null = null;
  let dpr = 1;
  let lastRows = 0;
  let lastCols = 0;
  let raf = 0;
  let resizeObserver: ResizeObserver | undefined;
  let themeObserver: MutationObserver | undefined;
  let resizeDebounce: ReturnType<typeof setTimeout> | undefined;
  /** How long a drag-resize has to settle before `terminal_resize` actually
   *  fires — see the `ResizeObserver` callback's own comment for why this
   *  can't just call it on every observer tick. */
  const RESIZE_DEBOUNCE_MS = 120;

  const encoder = new TextEncoder();

  function readThemeColors(): void {
    const cs = getComputedStyle(document.documentElement);
    const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback;
    defaultFg = v("--ax-text", defaultFg);
    defaultBg = v("--ax-surface-1", defaultBg);
    cursorColor = v("--ax-accent", cursorColor);
  }

  /** A plain CSS font shorthand (no weight — `TerminalScreen.draw` adds
   *  `"bold "` itself per cell) off the canvas's own resolved
   *  `--ax-font-mono`/`--ax-font-size-sm`. Read fresh each call, matching
   *  `graph/render.ts`'s own per-frame `getComputedStyle` convention for
   *  the same reason: cheap, and stays correct across a theme switch with
   *  no extra wiring. */
  function currentFont(): string {
    if (!canvasEl) return "13px monospace";
    const cs = getComputedStyle(canvasEl);
    return `${cs.fontSize} ${cs.fontFamily}`;
  }

  /** Measures the real character cell against the canvas (replacing
   *  Checkpoint 1's guessed average), sizes the canvas's backing store for
   *  the current device pixel ratio (same pattern as `graph/render.ts`'s
   *  `GraphRenderer.resize()`), and returns the row/column count that fits. */
  function measureAndSize(): { rows: number; cols: number } {
    if (!canvasEl || !root || !context2d) return { rows: MIN_ROWS, cols: MIN_COLS };
    const rect = root.getBoundingClientRect();
    dpr = window.devicePixelRatio || 1;
    metrics = measureChar(context2d, currentFont());
    canvasEl.width = Math.max(1, Math.round(rect.width * dpr));
    canvasEl.height = Math.max(1, Math.round(rect.height * dpr));
    return {
      rows: Math.max(MIN_ROWS, Math.floor(rect.height / metrics.height)),
      cols: Math.max(MIN_COLS, Math.floor(rect.width / metrics.width)),
    };
  }

  function tick(now: number): void {
    if (context2d && metrics) {
      const blinkOn = Math.floor(now / CURSOR_BLINK_MS) % 2 === 0;
      const cursor = !ended && sessionId && blinkOn ? cursorPos : null;
      context2d.setTransform(dpr, 0, 0, dpr, 0, 0);
      draw(context2d, { rows: screenRows, cursor, metrics, defaultFg, defaultBg, cursorColor, font: currentFont() });
    }
    raf = requestAnimationFrame(tick);
  }

  function sendBytes(bytes: Uint8Array): void {
    if (!sessionId) return;
    void ctx.invoke("terminal_write", { id: sessionId, data: Array.from(bytes) }).catch(() => {
      sessionId = null;
      ended = true;
    });
  }

  async function spawn(): Promise<void> {
    const { rows, cols } = measureAndSize();
    lastRows = rows;
    lastCols = cols;

    const onOutput = new Channel<TerminalEvent>();
    onOutput.onmessage = (event) => {
      if (event.type === "exited") {
        sessionId = null;
        ended = true;
        return;
      }
      screenRows = event.rows;
      cursorPos = { row: event.cursor_row, col: event.cursor_col };
    };

    try {
      sessionId = await ctx.invoke<string>("terminal_spawn", { rows, cols, onOutput });
    } catch (err) {
      error = String(err);
    }
  }

  /** Keys that either produce no native `input` event or need a specific C0
   *  control byte instead of literal text — everything else (printable
   *  characters, IME composition, paste) is handled by `handleInput` below.
   *  The actual key->byte mapping is `terminalInput.keyToBytes`, a plain
   *  function pulled out of this handler (architecture review, Checkpoint
   *  3) so its Ctrl+letter arithmetic and C0 mappings get real unit test
   *  coverage instead of being reachable only through a live
   *  `KeyboardEvent`. */
  function handleKeydown(e: KeyboardEvent): void {
    const bytes = keyToBytes(e.key, e.ctrlKey);
    if (!bytes) return;
    e.preventDefault();
    sendBytes(bytes);
  }

  /** Printable text lands here, not in `handleKeydown` — covers IME
   *  composition and paste for free. The field's value is never treated as
   *  terminal state (see the component doc comment), just cleared after
   *  forwarding it. */
  function handleInput(e: Event): void {
    const target = e.target as HTMLInputElement;
    const text = target.value;
    target.value = "";
    if (text) sendBytes(encoder.encode(text));
  }

  onMount(() => {
    if (canvasEl) context2d = canvasEl.getContext("2d");
    readThemeColors();
    void spawn();

    if (root) {
      resizeObserver = new ResizeObserver(() => {
        if (!sessionId) return; // not spawned yet, or the shell already ended
        // Resize the canvas's own backing store immediately (cheap, local
        // only) so drawing stays crisp through the drag, but debounce the
        // `terminal_resize` call itself: a continuous tile drag can cross
        // several row/col thresholds in quick succession, and each one is a
        // real `SIGWINCH` to whatever full-screen program is running (vim,
        // htop, …) — a real terminal coalesces these to the size the drag
        // actually settles on instead of firing on every intermediate size.
        const { rows, cols } = measureAndSize();
        if (rows === lastRows && cols === lastCols) return;
        lastRows = rows;
        lastCols = cols;
        clearTimeout(resizeDebounce);
        resizeDebounce = setTimeout(() => {
          if (sessionId) void ctx.invoke("terminal_resize", { id: sessionId, rows, cols }).catch(() => {});
        }, RESIZE_DEBOUNCE_MS);
      });
      resizeObserver.observe(root);
    }

    themeObserver = new MutationObserver(readThemeColors);
    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });

    raf = requestAnimationFrame(tick);
  });

  onDestroy(() => {
    cancelAnimationFrame(raf);
    clearTimeout(resizeDebounce);
    resizeObserver?.disconnect();
    themeObserver?.disconnect();
    if (sessionId) void ctx.invoke("terminal_close", { id: sessionId });
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<!-- Clicking anywhere in the tile focuses the hidden `.typer` input below —
     the real keyboard target, already reachable on its own by Tab. -->
<div class="terminal" bind:this={root} onclick={() => inputEl?.focus()}>
  {#if error}
    <p class="error">{error}</p>
  {:else}
    <canvas class="screen" bind:this={canvasEl}></canvas>
    {#if ended}<div class="ended">[process exited]</div>{/if}
    <input
      class="typer"
      bind:this={inputEl}
      type="text"
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
      onkeydown={handleKeydown}
      oninput={handleInput}
    />
  {/if}
</div>

<style>
  .terminal {
    position: relative;
    height: 100%;
    background: var(--ax-surface-1);
    cursor: text;
    overflow: hidden;
  }
  .screen {
    display: block;
    width: 100%;
    height: 100%;
    /* Only the font is read out of this (`getComputedStyle` in
     *  `currentFont`/`measureAndSize`) — the canvas never shows text via
     *  CSS, just via `fillText`. */
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
  }
  .ended {
    position: absolute;
    left: var(--ax-space-2);
    bottom: var(--ax-space-2);
    padding: 2px var(--ax-space-2);
    border-radius: var(--ax-radius-sm);
    background: color-mix(in srgb, var(--ax-surface-1) 60%, transparent);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
    pointer-events: none;
  }
  /* Captures keyboard focus/input without showing its own (always-empty)
   *  text — the canvas is the only visible typing feedback, same as a real
   *  terminal with local echo off. `pointer-events: none` is deliberate:
   *  without it this full-tile overlay would swallow mouse wheel/selection
   *  underneath it (this project has a history of exactly that class of
   *  tile scroll bug) — `.terminal`'s own `onclick` still focuses it for
   *  keyboard capture regardless. */
  .typer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: transparent;
    caret-color: transparent;
    opacity: 0;
    pointer-events: none;
  }
  .error {
    padding: var(--ax-space-3);
    margin: 0;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-danger);
  }
</style>
