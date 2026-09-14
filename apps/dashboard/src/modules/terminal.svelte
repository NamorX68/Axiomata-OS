<!--
  terminal — Checkpoint 4 of docs/plans/terminal.md added scrollback, the
  alternate screen, mouse selection/copy, and (bracketed) paste on top of
  Checkpoint 3's canvas renderer.

  Every `TerminalEvent::Screen` carries the *live* screen (full `Cell` data:
  colour, bold, underline) — `liveRows`/`liveCursor` always track it, same
  "redraw the whole grid from the model every update" approach Checkpoint 2
  established. Scrolling into history (mouse wheel) doesn't touch that live
  state at all: it's a separate, on-demand `terminal_scrollback` fetch into
  `historyRows`, drawn instead of the live rows while `scrollOffset > 0`
  (see `displayRows`) — the live channel keeps updating quietly underneath,
  so returning to the bottom (`scrollOffset` back to 0) shows whatever
  arrived while scrolled, not a stale frame. A blinking block cursor is
  drawn on top of whatever's showing, timed off `requestAnimationFrame`'s
  own clock — but only at `scrollOffset === 0`; scrolled into history,
  there's no live edit point to point at.

  Row/column count is genuinely measured (`TerminalScreen.measureChar`
  against the canvas's own resolved `--ax-font-mono`/`--ax-font-size-sm`),
  and a `ResizeObserver` keeps it in sync as the tile is resized — feeding
  `terminal_resize` (debounced; see its own comment), which resizes the PTY
  (`SIGWINCH` for the shell) and the screen model together.

  Input has no local echo: keystrokes are forwarded to the shell as bytes
  (via the hidden-ish text field below, which is cleared on every native
  `input` event rather than trusted as terminal state) and what's typed only
  appears once the shell's own PTY echoes it back over `on_output` — same as
  any real terminal with local echo off. Only Enter/Backspace/Tab/Escape and
  Ctrl+<letter> get their own C0 control byte; arrow-key history navigation
  and other escape-sequence input are a later checkpoint's concern. Pasting
  (into that same field) wraps the text in `\x1b[200~...\x1b[201~` only if
  the program running in the shell actually asked for bracketed paste
  (`bracketedPaste`, read off the live snapshot) — otherwise those marker
  bytes would show up as literal text. Any input (typed or pasted) snaps the
  view back to the live bottom if it was scrolled into history, matching
  ordinary terminal behaviour.

  Mouse selection is a linear (stream) selection, not a rectangular block —
  drag across the canvas, release to copy the covered text to the clipboard
  (`TerminalScreen.selectionText`/`isCellSelected`, both plain functions so
  the selection maths has real unit tests). It operates on whatever's
  currently drawn (`displayRows`), so selecting from scrollback history
  works the same as selecting live text — the selection just doesn't know
  or care which source its rows came from.

  Two outcomes are handled very differently (architecture review,
  Checkpoint 1): the shell ending — typing `exit`, or a write hitting a
  session the backend already tore down — is completely ordinary: the last
  real screen just freezes (no more cursor blink) and a small banner says
  so. Only `terminal_spawn` itself failing (no shell could even be started)
  is a fatal error that replaces the tile's content.
-->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Channel } from "@tauri-apps/api/core";

  import type { ModuleContext } from "../core/types";
  import { draw, measureChar, selectionText, type CellPos, type CharMetrics, type TermCell } from "./TerminalScreen";
  import { keyToBytes } from "./terminalInput";
  import { createSequenceGuard } from "./terminalScrollback";

  let { ctx }: { ctx: ModuleContext } = $props();

  const MIN_ROWS = 4;
  const MIN_COLS = 20;
  /** Classic terminal cursor blink period — on/off every half-period. */
  const CURSOR_BLINK_MS = 530;
  /** Lines per mouse-wheel "tick" scrolled into history. */
  const WHEEL_LINES = 3;

  /** Mirrors the backend's `TerminalEvent` (`src-tauri/src/terminal.rs`).
   *  Field names are snake_case, not camelCase: this rides over a raw
   *  `Channel` payload, not a `#[tauri::command]` argument list, so none of
   *  `invoke`'s usual camelCase<->snake_case bridging applies here. */
  type TerminalEvent =
    | { type: "screen"; rows: TermCell[][]; cursor_row: number; cursor_col: number; bracketed_paste: boolean }
    | { type: "exited" };

  let root = $state<HTMLDivElement>();
  let canvasEl = $state<HTMLCanvasElement>();
  let inputEl = $state<HTMLInputElement>();
  let error = $state("");
  /** The shell ended (on its own, or a write against it failed) — the last
   *  drawn screen just stays frozen; see the component doc comment. */
  let ended = $state(false);
  let sessionId: string | null = null;

  // The *live* screen — always current, regardless of `scrollOffset` (see
  // `displayRows`). Not itself redrawn from directly except through that.
  let liveRows: TermCell[][] = [];
  let liveCursor: CellPos | null = null;
  let bracketedPaste = $state(false);

  // How far up (in lines) the view is scrolled into scrollback, and the
  // on-demand fetch that fills in for the live rows while it's non-zero.
  // See the component doc comment's second paragraph.
  let scrollOffset = $state(0);
  let historyRows: TermCell[][] | null = null;
  const scrollFetchGuard = createSequenceGuard();

  // Mouse selection state (row/col in `displayRows()`'s coordinate space).
  let selecting = false;
  let dragged = false;
  let selStart = $state<CellPos | null>(null);
  let selEnd = $state<CellPos | null>(null);

  // Resolved once at mount and whenever the theme changes, not read fresh
  // every animation frame — unlike the font (see `currentFont`), these
  // don't need to track a live resize, just a theme swap.
  let defaultFg = "#f2f2f5";
  let defaultBg = "#17171c";
  let cursorColor = "#ff7a1a";
  let selectionColor = "rgba(255, 122, 26, 0.16)";

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

  /** The rows actually drawn/selected-from right now: the live screen at
   *  `scrollOffset === 0`, otherwise the last-fetched history viewport
   *  (falling back to live if nothing's been fetched yet — e.g. the very
   *  first wheel tick, before its `terminal_scrollback` call resolves). */
  function displayRows(): TermCell[][] {
    return scrollOffset === 0 ? liveRows : (historyRows ?? liveRows);
  }

  function readThemeColors(): void {
    const cs = getComputedStyle(document.documentElement);
    const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback;
    defaultFg = v("--ax-text", defaultFg);
    defaultBg = v("--ax-surface-1", defaultBg);
    cursorColor = v("--ax-accent", cursorColor);
    selectionColor = v("--ax-accent-muted", selectionColor);
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
      // No cursor while scrolled into history (nothing "live" to point at
      // there) or once the shell has ended.
      const cursor = !ended && sessionId && scrollOffset === 0 && blinkOn ? liveCursor : null;
      const selection = selStart && selEnd ? { start: selStart, end: selEnd } : null;
      context2d.setTransform(dpr, 0, 0, dpr, 0, 0);
      draw(context2d, {
        rows: displayRows(),
        cursor,
        selection,
        metrics,
        defaultFg,
        defaultBg,
        cursorColor,
        selectionColor,
        font: currentFont(),
      });
    }
    raf = requestAnimationFrame(tick);
  }

  /** Any input (typed or pasted) snaps the view back to the live bottom —
   *  the same convention real terminals use: you don't keep reading history
   *  while actively typing. */
  function snapToLive(): void {
    if (scrollOffset !== 0) {
      scrollOffset = 0;
      historyRows = null;
    }
  }

  function sendBytes(bytes: Uint8Array): void {
    if (!sessionId) return;
    snapToLive();
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
      liveRows = event.rows;
      liveCursor = { row: event.cursor_row, col: event.cursor_col };
      bracketedPaste = event.bracketed_paste;
    };

    try {
      sessionId = await ctx.invoke<string>("terminal_spawn", { rows, cols, onOutput });
    } catch (err) {
      error = String(err);
    }
  }

  /** Fetches a scrollback viewport for the current `scrollOffset` — see the
   *  component doc comment's second paragraph. `scrollFetchGuard` discards a
   *  response that resolves after a *later* request already changed
   *  `scrollOffset` again (rapid wheel ticks can easily outrace the
   *  round-trip), so an old, out-of-date viewport never overwrites a newer
   *  one that already landed — see `terminalScrollback.ts` for why that
   *  guard is its own tested unit rather than inline here. */
  async function fetchHistory(offset: number): Promise<void> {
    if (!sessionId) return;
    const seq = scrollFetchGuard.next();
    try {
      const rows = await ctx.invoke<TermCell[][]>("terminal_scrollback", { id: sessionId, offset });
      if (scrollFetchGuard.isCurrent(seq)) historyRows = rows;
    } catch {
      // The session most likely ended mid-fetch — `onOutput`'s `exited`
      // handling already covers telling the owner; nothing more to do here.
    }
  }

  function handleWheel(e: WheelEvent): void {
    if (!sessionId) return;
    e.preventDefault();
    const delta = e.deltaY > 0 ? -WHEEL_LINES : WHEEL_LINES;
    const next = Math.max(0, scrollOffset + delta);
    if (next === scrollOffset) return;
    scrollOffset = next;
    if (scrollOffset === 0) {
      // Back at the bottom — `liveRows` has been current the whole time,
      // nothing to fetch.
      historyRows = null;
      return;
    }
    void fetchHistory(scrollOffset);
  }

  /** Pixel coordinates -> a cell position, for mouse selection. `null` if
   *  the canvas hasn't been measured yet (shouldn't happen in practice —
   *  the canvas is only interactive once a session is up, by which point
   *  `measureAndSize` has already run). */
  function cellFromEvent(e: PointerEvent): CellPos | null {
    if (!canvasEl || !metrics) return null;
    const rect = canvasEl.getBoundingClientRect();
    return {
      row: Math.floor((e.clientY - rect.top) / metrics.height),
      col: Math.floor((e.clientX - rect.left) / metrics.width),
    };
  }

  function handlePointerDown(e: PointerEvent): void {
    const pos = cellFromEvent(e);
    if (!pos) return;
    canvasEl?.setPointerCapture(e.pointerId); // keep tracking past the canvas's own edge during a drag
    selecting = true;
    dragged = false;
    selStart = pos;
    selEnd = pos;
  }

  function handlePointerMove(e: PointerEvent): void {
    if (!selecting) return;
    const pos = cellFromEvent(e);
    if (!pos) return;
    if (!selEnd || pos.row !== selEnd.row || pos.col !== selEnd.col) dragged = true;
    selEnd = pos;
  }

  /** A genuine drag copies the covered text; a plain click (no drag) just
   *  clears whatever was selected before — matching how most terminals
   *  treat a click as "deselect", not "select one character". */
  function handlePointerUp(): void {
    if (!selecting) return;
    selecting = false;
    if (dragged && selStart && selEnd) {
      const text = selectionText(displayRows(), selStart, selEnd);
      if (text) void navigator.clipboard.writeText(text).catch(() => {});
    } else {
      selStart = null;
      selEnd = null;
    }
  }

  /** Wraps pasted text in bracketed-paste markers only if the program
   *  running in the shell actually asked for them (`bracketedPaste`) — see
   *  the component doc comment. The native paste event, not a keydown, so
   *  this also covers a right-click-paste or an OS-level paste gesture, not
   *  just Ctrl/Cmd+V. */
  function handlePaste(e: ClipboardEvent): void {
    e.preventDefault();
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (!text) return;
    const payload = bracketedPaste ? `\x1b[200~${text}\x1b[201~` : text;
    sendBytes(encoder.encode(payload));
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
     the real keyboard target, already reachable on its own by Tab. Wheel
     scrolling into history lives on this same wrapper (see `handleWheel`). -->
<div class="terminal" bind:this={root} onclick={() => inputEl?.focus()} onwheel={handleWheel}>
  {#if error}
    <p class="error">{error}</p>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- The mouse-selection target; `.typer` below has pointer-events:none
         precisely so this canvas is what actually receives them. -->
    <canvas
      class="screen"
      bind:this={canvasEl}
      onpointerdown={handlePointerDown}
      onpointermove={handlePointerMove}
      onpointerup={handlePointerUp}
    ></canvas>
    {#if ended}
      <div class="banner">[process exited]</div>
    {:else if scrollOffset > 0}
      <div class="banner">history — scroll down to return</div>
    {/if}
    <input
      class="typer"
      bind:this={inputEl}
      type="text"
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
      onkeydown={handleKeydown}
      oninput={handleInput}
      onpaste={handlePaste}
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
  /* Shared by the "process exited" (Checkpoint 1) and "scrolled into
   *  history" (Checkpoint 4) banners — only one is ever shown at once (see
   *  the template's `{:else if}`), so one class covers both. */
  .banner {
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
