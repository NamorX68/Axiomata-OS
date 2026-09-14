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

  Checkpoint 5b, Block A added `config.cwd`/`config.env`/`config.scrollbackLimit`
  — a starting working directory, extra environment variables (parsed from a
  `KEY=value`-per-line textarea by `terminalEnv.parseEnvLines`), and a
  scrollback-size override. Read once in `spawn()`, same as `config.shell`:
  none of the four can be applied to an already-running session, only to
  the next one spawned.

  Checkpoint 5b, Block B added six purely visual settings, all read fresh
  every `tick()`/`currentFont()` call (like `config.fontSizePx` already
  was) rather than needing their own `$effect`, since nothing about them
  requires a resize or a fresh spawn to take effect: cursor style
  (`config.cursorStyle` — `currentCursorStyle()`) and blink on/off
  (`config.cursorBlink`), a named 16-colour theme (`config.theme` —
  `currentPalette()`, see `terminalThemes.THEMES`), bold-as-bright-colour
  (`config.boldIsBright`, on by default), a custom font family
  (`config.fontFamily`, folded into `currentFont()` alongside the existing
  `fontSizePx`), and background opacity (`config.opacity`, converted to
  `DrawOptions.backgroundOpacity` — applies only to a cell with no explicit
  background of its own, see that option's own doc comment in
  `TerminalScreen.ts`). The visual bell (`bell` on `TerminalEvent::Screen`,
  Checkpoint 5b) is the one exception that isn't purely visual on the wire —
  it flashes `.bell-flash` for `BELL_FLASH_MS` via `triggerBellFlash`,
  togglable off with `config.bellEnabled`.

  Row/column count is genuinely measured (`TerminalScreen.measureChar`
  against the canvas's own resolved `--ax-font-mono`/`--ax-font-size-sm`, or
  `config.fontSizePx` in its place once Checkpoint 5's settings side sets
  one — see `currentFont`). Both a tile drag (`ResizeObserver`) and a live
  `config.fontSizePx` change (its own `$effect`) route through one
  `scheduleResize` — debounced, so `terminal_resize` (which resizes the PTY,
  `SIGWINCH` for the shell, and the screen model together) only actually
  fires once things settle, and so the two triggers can't race each other
  into applying a stale size (architecture review: they used to debounce
  independently, and a resize timer already in flight could fire *after*,
  and silently undo, an immediate font-size resize). `config.shell` (also
  Checkpoint 5) only applies to the *next* spawned session — there's
  no way to swap a shell under an already-running process — so it's just
  read once in `spawn()`, not watched.

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
  import {
    draw,
    DEFAULT_CURSOR_STYLE,
    measureChar,
    selectionText,
    type CellPos,
    type CharMetrics,
    type CursorStyle,
    type TermCell,
  } from "./TerminalScreen";
  import { keyToBytes } from "./terminalInput";
  import { createSequenceGuard } from "./terminalScrollback";
  import { DEFAULT_THEME, THEMES } from "./terminalThemes";
  import { parseEnvLines } from "./terminalEnv";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

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
    | {
        type: "screen";
        rows: TermCell[][];
        cursor_row: number;
        cursor_col: number;
        bracketed_paste: boolean;
        bell: boolean;
      }
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

  // Checkpoint 5b's visual bell: `true` for `BELL_FLASH_MS` after a
  // `TerminalEvent::Screen.bell` arrives, then auto-clears — see
  // `triggerBellFlash`. Togglable off via `config.bellEnabled`.
  let bellFlash = $state(false);
  let bellFlashTimeout: ReturnType<typeof setTimeout> | undefined;
  const BELL_FLASH_MS = 200;

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
   *  `--ax-font-mono`/`--ax-font-size-sm` — or `config.fontSizePx`/
   *  `config.fontFamily` (Checkpoint 5/5b's settings-side overrides,
   *  `terminal-settings.svelte`) in place of the theme's own size/family
   *  when set. Read fresh each call, matching `graph/render.ts`'s own
   *  per-frame `getComputedStyle` convention for the same reason: cheap,
   *  and stays correct across a theme switch (or a config change) with no
   *  extra wiring. */
  function currentFont(): string {
    if (!canvasEl) return "13px monospace";
    const cs = getComputedStyle(canvasEl);
    const size = typeof $config.fontSizePx === "number" ? `${$config.fontSizePx}px` : cs.fontSize;
    const family = typeof $config.fontFamily === "string" && $config.fontFamily.trim() ? $config.fontFamily : cs.fontFamily;
    return `${size} ${family}`;
  }

  /** `config.theme` (Checkpoint 5b's "Farbschema/Theme" setting) resolved
   *  to an actual 16-colour table — an unrecognized/stale name (or none
   *  set) falls back to `THEMES[DEFAULT_THEME]`, the original palette,
   *  rather than throwing or drawing with `undefined` colours. */
  function currentPalette(): readonly string[] {
    const name = typeof $config.theme === "string" ? $config.theme : DEFAULT_THEME;
    return THEMES[name] ?? THEMES[DEFAULT_THEME];
  }

  /** `config.cursorStyle` (Checkpoint 5b) narrowed to a real `CursorStyle`
   *  — anything else (unset, a stale/typo'd value) falls back to
   *  `DEFAULT_CURSOR_STYLE`, the original shape. */
  function currentCursorStyle(): CursorStyle {
    const raw = $config.cursorStyle;
    return raw === "outline" || raw === "underline" || raw === "bar" ? raw : DEFAULT_CURSOR_STYLE;
  }

  /** Measures the real character cell against the canvas (replacing
   *  Checkpoint 1's guessed average), sizes the canvas's backing store for
   *  the current device pixel ratio (same pattern as `graph/render.ts`'s
   *  `GraphRenderer.resize()`), and returns the row/column count that fits.
   *  Fetches `context2d` itself if it isn't set yet rather than requiring
   *  the caller to have already done so — used both from `onMount` (after
   *  that assignment) and from the `config.fontSizePx` effect below, whose
   *  ordering relative to `onMount` isn't something worth depending on. */
  function measureAndSize(): { rows: number; cols: number } {
    if (!canvasEl || !root) return { rows: MIN_ROWS, cols: MIN_COLS };
    if (!context2d) context2d = canvasEl.getContext("2d");
    if (!context2d) return { rows: MIN_ROWS, cols: MIN_COLS };
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
      // `config.cursorBlink` (Checkpoint 5b) defaults to on — set to
      // `false` and the cursor stays continuously visible instead of
      // toggling with `CURSOR_BLINK_MS`.
      const blinkOn = $config.cursorBlink === false || Math.floor(now / CURSOR_BLINK_MS) % 2 === 0;
      // No cursor while scrolled into history (nothing "live" to point at
      // there) or once the shell has ended.
      const cursor = !ended && sessionId && scrollOffset === 0 && blinkOn ? liveCursor : null;
      const selection = selStart && selEnd ? { start: selStart, end: selEnd } : null;
      const opacityPercent = typeof $config.opacity === "number" ? Math.min(100, Math.max(0, $config.opacity)) : 100;
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
        cursorStyle: currentCursorStyle(),
        palette: currentPalette(),
        // Owner decision (docs/plans/terminal.md, Checkpoint 5b): on by
        // default — only `config.boldIsBright === false` turns it off.
        boldIsBright: $config.boldIsBright !== false,
        backgroundOpacity: opacityPercent / 100,
      });
    }
    raf = requestAnimationFrame(tick);
  }

  /** Flashes the tile's bell overlay for `BELL_FLASH_MS`, then auto-clears
   *  — Checkpoint 5b's "Visueller Bell" setting. No audio (see the plan's
   *  own reasoning: playing sound from a background Tauri process is its
   *  own can of worms, not justified for a backlog item this size). */
  function triggerBellFlash(): void {
    bellFlash = true;
    clearTimeout(bellFlashTimeout);
    bellFlashTimeout = setTimeout(() => {
      bellFlash = false;
    }, BELL_FLASH_MS);
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
      // `config.bellEnabled` (Checkpoint 5b) defaults to on; only an
      // explicit `false` suppresses the flash.
      if (event.bell && $config.bellEnabled !== false) triggerBellFlash();
    };

    // `config.shell` (Checkpoint 5's settings-side override) only picks
    // which shell *this* spawn uses — see `terminal-settings.svelte`'s own
    // hint that a shell change needs a fresh session, not a live swap
    // under an already-running one. `config.cwd`/`config.env`/
    // `config.scrollbackLimit` (Checkpoint 5b) work the same way: read once
    // here, not watched, since none of them can be applied to an
    // already-running session either.
    const shell = typeof $config.shell === "string" && $config.shell ? $config.shell : null;
    const cwd = typeof $config.cwd === "string" && $config.cwd.trim() ? $config.cwd.trim() : null;
    const env = typeof $config.env === "string" ? parseEnvLines($config.env) : [];
    const scrollbackLimit =
      typeof $config.scrollbackLimit === "number" && $config.scrollbackLimit >= 0 ? $config.scrollbackLimit : null;

    try {
      sessionId = await ctx.invoke<string>("terminal_spawn", {
        rows,
        cols,
        options: { shell, cwd, env, scrollbackLimit },
        onOutput,
      });
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

  /** Live-applies a `config.fontSizePx` change from the settings side
   *  (`terminal-settings.svelte`): re-measure the character cell at the new
   *  size and, if that changes the row/column count, resize the session to
   *  match — the same reflow a tile drag-resize already triggers via
   *  `ResizeObserver` below, just driven by a font-size change instead of a
   *  pixel-size one. Runs once on mount too (reading `$config.fontSizePx`
   *  is what makes this effect re-run on later changes), when `sessionId`
   *  is still `null` and `measureAndSize` alone is a no-op beyond sizing
   *  the canvas — harmless, and `spawn()`'s own first `measureAndSize` call
   *  already accounts for whatever the config held at that point anyway. */
  $effect(() => {
    void $config.fontSizePx;
    if (!sessionId) return; // spawn()'s own first measureAndSize() already covers the pre-spawn case
    const { rows, cols } = measureAndSize();
    scheduleResize(rows, cols);
  });

  /** The one place that actually calls `terminal_resize` — both the
   *  `ResizeObserver` (tile drag) and the `config.fontSizePx` effect above
   *  route through here instead of each debouncing/invoking independently.
   *  Found necessary in architecture review: two separate debounce-or-not
   *  paths writing the same `lastRows`/`lastCols` raced each other — a tile
   *  drag's *already-scheduled* timer could fire after a font-size change's
   *  immediate call and silently revert the terminal back to the stale
   *  pre-font-change size, since the timer's closure never re-checked
   *  anything at fire time. `clearTimeout` on every call guarantees only
   *  the *last* requested size — whichever trigger produced it — ever
   *  actually reaches `terminal_resize`. */
  function scheduleResize(rows: number, cols: number): void {
    if (!sessionId || (rows === lastRows && cols === lastCols)) return;
    lastRows = rows;
    lastCols = cols;
    // Debounced, not immediate: a continuous tile drag can cross several
    // row/col thresholds in quick succession, and each one is a real
    // `SIGWINCH` to whatever full-screen program is running (vim, htop, …)
    // — a real terminal coalesces these to the size things actually settle
    // on rather than firing on every intermediate size. A font-size change
    // is a single discrete event, not a drag, but the same debounce is
    // harmless for it too (120ms is imperceptible for a one-off change) and
    // having only one code path is worth more than either trigger's own
    // "ideal" timing.
    clearTimeout(resizeDebounce);
    resizeDebounce = setTimeout(() => {
      if (sessionId) void ctx.invoke("terminal_resize", { id: sessionId, rows, cols }).catch(() => {});
    }, RESIZE_DEBOUNCE_MS);
  }

  onMount(() => {
    if (canvasEl) context2d = canvasEl.getContext("2d");
    readThemeColors();
    void spawn();

    if (root) {
      resizeObserver = new ResizeObserver(() => {
        if (!sessionId) return; // not spawned yet, or the shell already ended
        // Resize the canvas's own backing store immediately (cheap, local
        // only) so drawing stays crisp through the drag; the actual
        // `terminal_resize` call is debounced inside `scheduleResize`.
        const { rows, cols } = measureAndSize();
        scheduleResize(rows, cols);
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
    clearTimeout(bellFlashTimeout);
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
    <!-- Checkpoint 5b's visual bell — a brief border flash, no audio. -->
    <div class="bell-flash" class:active={bellFlash}></div>
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
  /* Checkpoint 5b's visual bell: an inset border that briefly appears and
   *  fades, rather than popping instantly on and off — `bellFlash` toggles
   *  `.active` for `BELL_FLASH_MS` (see `triggerBellFlash`). */
  .bell-flash {
    position: absolute;
    inset: 0;
    pointer-events: none;
    box-shadow: inset 0 0 0 2px transparent;
    transition: box-shadow 60ms ease-out;
  }
  .bell-flash.active {
    box-shadow: inset 0 0 0 2px var(--ax-accent);
  }
</style>
