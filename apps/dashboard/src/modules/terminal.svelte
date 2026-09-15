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

  Checkpoint 5d moved every setting mentioned below from `ctx.config` (this
  tile's own per-instance config, `dashboard.json`) to `terminalSettings`
  (`./terminalSettings.ts`) — one global preferences store shared by every
  placed Terminal tile, persisted in its own `terminal-settings.json`.
  Owner feedback that motivated the move: closing/removing a tile used to
  discard every setting on it. `spawn()` `await`s
  `ensureTerminalSettingsLoaded()` before reading any of them — the fetch
  is async, and reading empty defaults because it hadn't resolved yet would
  silently ignore real saved preferences on the very first spawn after app
  boot. Everywhere else below just reads `$terminalSettings` directly
  (Svelte's store auto-subscription), same as it read `$config` before.

  Checkpoint 5b, Block A added `terminalSettings.cwd`/`terminalSettings.env`/`terminalSettings.scrollbackLimit`
  — a starting working directory, extra environment variables (parsed from a
  `KEY=value`-per-line textarea by `terminalEnv.parseEnvLines`), and a
  scrollback-size override. Read once in `spawn()`, same as `terminalSettings.shell`:
  none of the four can be applied to an already-running session, only to
  the next one spawned.

  Checkpoint 5b, Block B added six purely visual settings, all read fresh
  every `tick()`/`currentFont()` call (like `terminalSettings.fontSizePx` already
  was) rather than needing their own `$effect`, since nothing about them
  requires a resize or a fresh spawn to take effect: cursor style
  (`terminalSettings.cursorStyle` — `currentCursorStyle()`) and blink on/off
  (`terminalSettings.cursorBlink`), a named 16-colour theme (`terminalSettings.theme` —
  `currentPalette()`, see `terminalThemes.THEMES`), bold-as-bright-colour
  (`terminalSettings.boldIsBright`, on by default), a custom font family
  (`terminalSettings.fontFamily`, folded into `currentFont()` alongside the existing
  `fontSizePx`), and background opacity (`terminalSettings.opacity`, converted to
  `DrawOptions.backgroundOpacity` — applies only to a cell with no explicit
  background of its own, see that option's own doc comment in
  `TerminalScreen.ts`). The visual bell (`bell` on `TerminalEvent::Screen`,
  Checkpoint 5b) is the one exception that isn't purely visual on the wire —
  it flashes `.bell-flash` for `BELL_FLASH_MS` via `triggerBellFlash`,
  togglable off with `terminalSettings.bellEnabled`.

  Row/column count is genuinely measured (`TerminalScreen.measureChar`
  against the canvas's own resolved `--ax-font-mono`/`--ax-font-size-sm`, or
  `terminalSettings.fontSizePx` in its place once Checkpoint 5's settings side sets
  one — see `currentFont`). Both a tile drag (`ResizeObserver`) and a live
  `terminalSettings.fontSizePx` change (its own `$effect`) route through one
  `scheduleResize` — debounced, so `terminal_resize` (which resizes the PTY,
  `SIGWINCH` for the shell, and the screen model together) only actually
  fires once things settle, and so the two triggers can't race each other
  into applying a stale size (architecture review: they used to debounce
  independently, and a resize timer already in flight could fire *after*,
  and silently undo, an immediate font-size resize). `terminalSettings.shell` (also
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
  import { ensureTerminalSettingsLoaded, terminalSettings } from "./terminalSettings";
  import { DEFAULT_THEME, THEME_DEFAULT_COLORS, THEMES } from "./terminalThemes";
  import { parseEnvLines } from "./terminalEnv";

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
  /** Set in `onDestroy` — checked by the `document.fonts.load()` callback in
   *  `measureAndSize` (architecture review, Checkpoint 5j) so a font that's
   *  still loading when the tile is removed doesn't act on a stale
   *  component after the fact, matching every other lingering-callback
   *  guard already in this file (`focusRafIds`, the observers' own
   *  `.disconnect()`). */
  let destroyed = false;
  let resizeObserver: ResizeObserver | undefined;
  let themeObserver: MutationObserver | undefined;
  let resizeDebounce: ReturnType<typeof setTimeout> | undefined;
  /** How long a drag-resize has to settle before `terminal_resize` actually
   *  fires — see the `ResizeObserver` callback's own comment for why this
   *  can't just call it on every observer tick. */
  const RESIZE_DEBOUNCE_MS = 120;

  // Checkpoint 5b's visual bell: `true` for `BELL_FLASH_MS` after a
  // `TerminalEvent::Screen.bell` arrives, then auto-clears — see
  // `triggerBellFlash`. Togglable off via `terminalSettings.bellEnabled`.
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

  /** Appended as the last fallback to whatever font family is actually
   *  configured, so glyphs the primary family doesn't have — Powerline
   *  separators, devicon/git-branch icons in a Starship/Powerlevel10k
   *  prompt — still render instead of showing as blank boxes (Checkpoint
   *  5g, owner-confirmed via Ghostty-comparison screenshots in Checkpoint
   *  5e). Canvas `ctx.font` resolves a comma-separated family list the same
   *  way CSS does: per-glyph, not per-string, so normal text still comes
   *  from the primary family and only genuinely missing glyphs fall
   *  through to this one. Bundled in `main.ts`
   *  (`@azurity/pure-nerd-font/pure-nerd-font.css`) — icons only, no
   *  regular character set, deliberately layered onto any font rather than
   *  being its own patched monospace family. That import's own comment has
   *  the full caveat (architecture review, Checkpoint 5g): the bundled
   *  glyph set is pinned to an old Nerd Fonts generation, so a codepoint
   *  added upstream after that pin may still render blank. */
  const NERD_FONT_FALLBACK = '"PureNerdFont"';

  /** A plain CSS font shorthand — no weight baked in, since `draw` (via
   *  `TerminalScreen.cellFont`) prepends one itself per cell: the literal
   *  `"bold"` keyword for a bold cell, or Checkpoint 5h's configured
   *  `fontWeight` (see the `fontWeight` `$derived` below) for a non-bold
   *  one — off the canvas's own resolved
   *  `--ax-font-mono`/`--ax-font-size-sm` — or `terminalSettings.fontSizePx`/
   *  `terminalSettings.fontFamily` (Checkpoint 5/5b's settings-side overrides,
   *  `terminal-settings.svelte`) in place of the theme's own size/family
   *  when set. Read fresh each call, matching `graph/render.ts`'s own
   *  per-frame `getComputedStyle` convention for the same reason: cheap,
   *  and stays correct across a theme switch (or a `terminalSettings` change)
   *  with no extra wiring. */
  function currentFont(): string {
    if (!canvasEl) return "13px monospace";
    const cs = getComputedStyle(canvasEl);
    const size = typeof $terminalSettings.fontSizePx === "number" ? `${$terminalSettings.fontSizePx}px` : cs.fontSize;
    const family = typeof $terminalSettings.fontFamily === "string" && $terminalSettings.fontFamily.trim() ? $terminalSettings.fontFamily : cs.fontFamily;
    return `${size} ${family}, ${NERD_FONT_FALLBACK}`;
  }

  /** `terminalSettings.theme`, or `DEFAULT_THEME` when unset/not a string —
   *  the one place that resolves *which* named theme is active, shared by
   *  `currentPalette` and `currentDefaultColors` below so they can't ever
   *  disagree on it. */
  function currentThemeName(): string {
    return typeof $terminalSettings.theme === "string" ? $terminalSettings.theme : DEFAULT_THEME;
  }

  /** `terminalSettings.theme` (Checkpoint 5b's "Farbschema/Theme" setting) resolved
   *  to an actual 16-colour table — an unrecognized/stale name (or none
   *  set) falls back to `THEMES[DEFAULT_THEME]`, the original palette,
   *  rather than throwing or drawing with `undefined` colours. */
  function currentPalette(): readonly string[] {
    const name = currentThemeName();
    return THEMES[name] ?? THEMES[DEFAULT_THEME];
  }

  /** The actual background/foreground an *unwritten* cell shows — Checkpoint
   *  5k: `THEME_DEFAULT_COLORS[currentThemeName()]`'s own authentic colours
   *  when the active theme has them (every named theme except `xterm`, see
   *  that table's own doc comment), otherwise the app's chrome-theme colours
   *  `readThemeColors` already keeps in `defaultFg`/`defaultBg` — unchanged
   *  pre-5k behaviour, and still what `xterm` (this module's own "inherit
   *  the app theme" default) uses. */
  function currentDefaultColors(): { fg: string; bg: string } {
    const override = THEME_DEFAULT_COLORS[currentThemeName()];
    return { fg: override?.foreground ?? defaultFg, bg: override?.background ?? defaultBg };
  }

  /** `terminalSettings.cursorStyle` (Checkpoint 5b) narrowed to a real `CursorStyle`
   *  — anything else (unset, a stale/typo'd value) falls back to
   *  `DEFAULT_CURSOR_STYLE`, the original shape. */
  function currentCursorStyle(): CursorStyle {
    const raw = $terminalSettings.cursorStyle;
    return raw === "outline" || raw === "underline" || raw === "bar" ? raw : DEFAULT_CURSOR_STYLE;
  }

  /** `terminalSettings.opacity` (0-100) normalized to a 0-1 fraction — a
   *  real `$derived` (not computed inline in `tick()`, which is a plain
   *  function the animation loop calls, not a template expression) purely
   *  for reuse/readability, not because anything else reads it.
   *
   *  Bug fix history: the canvas's own `backgroundOpacity` alone first
   *  appeared to have no visible effect, because `.terminal`'s wrapper
   *  `<div>` painted an opaque `--ax-surface-1` behind it — the exact
   *  colour the canvas's own "default background" cells already paint, so
   *  punching a transparent hole in the canvas only ever revealed an
   *  identically-coloured div, not whatever's actually behind the tile.
   *  The first fix also made `.terminal` itself translucent at the same
   *  fraction (architecture review caught this): compositing two "over"
   *  blends at the same alpha `O` stacks to an effective `O·(2−O)`, not
   *  `O` — e.g. the slider's midpoint read as ~75% opaque, not 50%
   *  see-through, even though both endpoints (fully opaque / fully
   *  see-through) still looked correct. Fixed properly below: `.terminal`
   *  is now unconditionally `background: transparent` (see its own CSS
   *  comment) — only the canvas's per-cell alpha does any blending, so the
   *  slider is linear across its whole range, not just at its ends. */
  const backgroundOpacity = $derived(
    typeof $terminalSettings.opacity === "number" ? Math.min(100, Math.max(0, $terminalSettings.opacity)) / 100 : 1,
  );

  /** `terminalSettings.fontWeight` clamped to CSS's valid 1-1000
   *  `font-weight` range and rounded to a whole number — same "clamp
   *  untrusted persisted data on read" pattern as `backgroundOpacity`
   *  right above (architecture review, Checkpoint 5h): the settings
   *  page's own `<select>` only ever writes one of nine known-good values,
   *  but `terminalSettings` is untyped, schema-free persisted JSON
   *  (`crates/axiomata-core/src/terminal_settings.rs` only checks for a
   *  numeric `version`), so a hand-edited or foreign `terminal-settings.json`
   *  could contain anything — `NaN`, a negative number, a value outside
   *  the spec's own range. `cellFont` builds a raw CSS font shorthand
   *  string from this value with no parsing/escaping of its own; an
   *  invalid weight token there doesn't throw, it makes the whole
   *  `ctx.font` assignment silently fail per the Canvas 2D spec, which
   *  *keeps the previous font* rather than falling back to anything
   *  sensible — a much worse failure mode than clamping here ever risks. */
  const fontWeight = $derived(
    typeof $terminalSettings.fontWeight === "number" && Number.isFinite($terminalSettings.fontWeight)
      ? Math.min(1000, Math.max(1, Math.round($terminalSettings.fontWeight)))
      : undefined,
  );

  /** Measures the real character cell against the canvas (replacing
   *  Checkpoint 1's guessed average), sizes the canvas's backing store for
   *  the current device pixel ratio (same pattern as `graph/render.ts`'s
   *  `GraphRenderer.resize()`), and returns the row/column count that fits.
   *  Fetches `context2d` itself if it isn't set yet rather than requiring
   *  the caller to have already done so — used both from `onMount` (after
   *  that assignment) and from the settings-reactive `$effect` below, whose
   *  ordering relative to `onMount` isn't something worth depending on.
   *
   *  Bug found producing Checkpoint 5h/5g demo screenshots (Chromium
   *  dev-mock, not a live-tested owner report): a bundled font's `@font-face`
   *  rule is declared at app boot (`main.ts`), but browsers fetch/rasterize
   *  the actual font file lazily, only once something is actually measured
   *  or drawn with it — the *first* time a given family is picked in a
   *  session, `measureChar`'s synchronous `ctx.measureText("M")` call below
   *  can run before that fetch finishes, silently measuring against
   *  whatever fallback font was already loaded instead. That wrong
   *  `metrics` value then gets baked into the canvas's backing-store size
   *  and row/col count — and since nothing re-measures once the real font
   *  *does* finish loading a few dozen ms later, every subsequent frame's
   *  `fillText` draws the correct glyphs into a grid still laid out for the
   *  wrong font, visibly overlapping/misaligned. `document.fonts.check`
   *  confirmed this empirically: `false` for a family never used yet this
   *  session, even though its CSS was imported at boot. Fixed below by
   *  explicitly loading the font and re-measuring once it's actually
   *  ready, instead of trusting one synchronous measurement.
   *
   *  Two follow-up guards (architecture review, Checkpoint 5j): the retry
   *  bails out via `destroyed` if the tile was removed while a font was
   *  still loading — every other lingering callback in this component
   *  (`focusRafIds`, `resizeDebounce`, the observers) is already cancelled
   *  in `onDestroy`, and this one wasn't; and `attemptedFontLoads` makes
   *  the retry a one-shot per distinct font string, not an unbounded
   *  loop — a font/weight combination this app didn't actually bundle a
   *  face for (some fonts only ship Regular/Bold, see `main.ts`) could in
   *  principle keep `document.fonts.check` returning `false` forever even
   *  after `.load()` resolves, and without this guard every later
   *  `measureAndSize()` call (each settings change, each resize) would
   *  re-issue another `.load()` for the same unsatisfiable descriptor. */
  const attemptedFontLoads = new Set<string>();

  function measureAndSize(): { rows: number; cols: number } {
    if (!canvasEl || !root) return { rows: MIN_ROWS, cols: MIN_COLS };
    if (!context2d) context2d = canvasEl.getContext("2d");
    if (!context2d) return { rows: MIN_ROWS, cols: MIN_COLS };
    const rect = root.getBoundingClientRect();
    dpr = window.devicePixelRatio || 1;
    const font = currentFont();
    metrics = measureChar(context2d, font);
    canvasEl.width = Math.max(1, Math.round(rect.width * dpr));
    canvasEl.height = Math.max(1, Math.round(rect.height * dpr));
    // `document.fonts` doesn't exist in every conceivable environment (old
    // WebKit, some embeddings) — treat its absence as "already fine",
    // matching this measurement's own pre-existing fallback behaviour
    // rather than throwing.
    if (document.fonts && !document.fonts.check(font) && !attemptedFontLoads.has(font)) {
      attemptedFontLoads.add(font);
      document.fonts
        .load(font)
        .then(() => {
          if (destroyed) return; // tile removed while the font was still loading
          // The font that just finished loading might not be the one
          // configured any more (the owner could have changed it again
          // while this was in flight) — `measureAndSize`/`currentFont`
          // both always read the *current* live settings, so re-running
          // the whole measurement naturally picks up whatever's current,
          // not stale data captured in this closure.
          const resized = measureAndSize();
          scheduleResize(resized.rows, resized.cols);
        })
        .catch(() => {
          // Font failed to load (offline, corrupt file, …) — the
          // fallback-font measurement already computed above stays in
          // effect, same as it always did before this fix existed.
        });
    }
    return {
      rows: Math.max(MIN_ROWS, Math.floor(rect.height / metrics.height)),
      cols: Math.max(MIN_COLS, Math.floor(rect.width / metrics.width)),
    };
  }

  function tick(now: number): void {
    if (context2d && metrics) {
      // `terminalSettings.cursorBlink` (Checkpoint 5b) defaults to on — set to
      // `false` and the cursor stays continuously visible instead of
      // toggling with `CURSOR_BLINK_MS`.
      const blinkOn = $terminalSettings.cursorBlink === false || Math.floor(now / CURSOR_BLINK_MS) % 2 === 0;
      // No cursor while scrolled into history (nothing "live" to point at
      // there) or once the shell has ended.
      const cursor = !ended && sessionId && scrollOffset === 0 && blinkOn ? liveCursor : null;
      const selection = selStart && selEnd ? { start: selStart, end: selEnd } : null;
      const { fg: resolvedDefaultFg, bg: resolvedDefaultBg } = currentDefaultColors();
      context2d.setTransform(dpr, 0, 0, dpr, 0, 0);
      draw(context2d, {
        rows: displayRows(),
        cursor,
        selection,
        metrics,
        defaultFg: resolvedDefaultFg,
        defaultBg: resolvedDefaultBg,
        cursorColor,
        selectionColor,
        font: currentFont(),
        fontWeight,
        cursorStyle: currentCursorStyle(),
        palette: currentPalette(),
        // Owner decision (docs/plans/terminal.md, Checkpoint 5b): on by
        // default — only `terminalSettings.boldIsBright === false` turns it off.
        boldIsBright: $terminalSettings.boldIsBright !== false,
        backgroundOpacity,
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
    // Must resolve before anything below reads `$terminalSettings` — the
    // fetch is async, and reading its empty pre-load default (rather than
    // whatever's really saved) would silently spawn with wrong settings on
    // the very first terminal after app boot. Cheap after the first
    // Terminal instance: `ensureTerminalSettingsLoaded` is a no-op once
    // already loaded.
    await ensureTerminalSettingsLoaded();

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
      // `terminalSettings.bellEnabled` (Checkpoint 5b) defaults to on; only an
      // explicit `false` suppresses the flash.
      if (event.bell && $terminalSettings.bellEnabled !== false) triggerBellFlash();
    };

    // `terminalSettings.shell` (Checkpoint 5's settings-side override) only picks
    // which shell *this* spawn uses — see `terminal-settings.svelte`'s own
    // hint that a shell change needs a fresh session, not a live swap
    // under an already-running one. `terminalSettings.cwd`/`terminalSettings.env`/
    // `terminalSettings.scrollbackLimit` (Checkpoint 5b) work the same way: read once
    // here, not watched, since none of them can be applied to an
    // already-running session either.
    const shell = typeof $terminalSettings.shell === "string" && $terminalSettings.shell ? $terminalSettings.shell : null;
    const cwd = typeof $terminalSettings.cwd === "string" && $terminalSettings.cwd.trim() ? $terminalSettings.cwd.trim() : null;
    const env = typeof $terminalSettings.env === "string" ? parseEnvLines($terminalSettings.env) : [];
    const scrollbackLimit =
      typeof $terminalSettings.scrollbackLimit === "number" && $terminalSettings.scrollbackLimit >= 0 ? $terminalSettings.scrollbackLimit : null;

    try {
      sessionId = await ctx.invoke<string>("terminal_spawn", {
        rows,
        cols,
        options: { shell, cwd, env, scrollbackLimit },
        onOutput,
      });
      // Owner feedback: a freshly spawned terminal should be ready to type
      // into immediately, not require a deliberate click first — the same
      // expectation a real terminal app's newly opened window already
      // meets. `onMount`'s own earlier `focusInputSoon()` call already
      // tries this well before `spawn()` gets here; this one more attempt
      // is for whatever might have stolen focus during the round trip
      // above (see `focusInputSoon`'s own doc comment for why it retries
      // instead of a single `.focus()` call).
      focusInputSoon();
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
    const bytes = keyToBytes(e.key, e.ctrlKey, e.shiftKey);
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

  /** Live-applies a `terminalSettings` change that can move the character
   *  cell's own pixel size from the settings side (`terminal-settings.svelte`):
   *  re-measure the cell and, if that changes the row/column count, resize
   *  the session to match — the same reflow a tile drag-resize already
   *  triggers via `ResizeObserver` below, just driven by a settings change
   *  instead of a pixel-size one. Runs once on mount too (reading each of
   *  these is what makes this effect re-run on later changes), when
   *  `sessionId` is still `null` and `measureAndSize` alone is a no-op
   *  beyond sizing the canvas — harmless, and `spawn()`'s own first
   *  `measureAndSize` call already accounts for whatever `terminalSettings`
   *  held at that point anyway.
   *
   *  Bug found while producing demo screenshots (not a live-tested owner
   *  report): originally only watched `fontSizePx` — changing the font
   *  *family* (the "Bundled font"/"Font family" fields) or, from
   *  Checkpoint 5h, `fontWeight` never re-measured the cell at all, so the
   *  canvas kept drawing at the *previous* font's cached cell dimensions
   *  while `ctx.font` (via `currentFont`/`cellFont`) had already switched
   *  to the new one — visibly mismatched glyph spacing/line height, not a
   *  clear/ghosting bug (Checkpoint 5e's `clearRect` fix is unrelated and
   *  still correct; this is a stale-`metrics` bug, a different failure
   *  mode). A font's own size is far from the only thing that can change
   *  its cell footprint. */
  $effect(() => {
    void $terminalSettings.fontSizePx;
    void $terminalSettings.fontFamily;
    void $terminalSettings.fontWeight;
    if (!sessionId) return; // spawn()'s own first measureAndSize() already covers the pre-spawn case
    const { rows, cols } = measureAndSize();
    scheduleResize(rows, cols);
  });

  /** The one place that actually calls `terminal_resize` — both the
   *  `ResizeObserver` (tile drag) and the `terminalSettings.fontSizePx` effect above
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

  /** Pending `requestAnimationFrame` ids from the last `focusInputSoon()`
   *  call, so a stale chain can be cancelled — before scheduling a new one,
   *  or on unmount — instead of a late, ungated `.focus()` call landing
   *  after something newer already decided otherwise (architecture review,
   *  Checkpoint 5f). */
  let focusRafIds: number[] = [];

  function cancelPendingFocusAttempts(): void {
    for (const id of focusRafIds) cancelAnimationFrame(id);
    focusRafIds = [];
  }

  /** Repeatedly attempts to focus the hidden `.typer` input — immediately,
   *  then again on each of the next couple of animation frames — for the
   *  two *automatic* (non-click) moments a freshly placed/opened terminal
   *  should already be typeable into: right after it mounts, and once
   *  `spawn()` actually brings up a shell. Checkpoint 5e's first attempt (a
   *  single `inputEl?.focus()` right after `spawn()`'s `terminal_spawn`
   *  IPC call resolved) did not reliably work in the real app — the owner
   *  reported the same "still need to click first" symptom afterward.
   *  Checkpoint 5f: the most likely explanation is a known class of
   *  WebKit/WKWebView timing quirk (this project's own track record, see
   *  `canvas/Tile.svelte`'s `.tile-body` comment) where a `.focus()` call
   *  issued well after the triggering interaction — here, after two
   *  `await`s (`ensureTerminalSettingsLoaded`, then the `terminal_spawn`
   *  round trip itself) — silently fails to move real OS-level keyboard
   *  focus, even though the DOM's own `document.activeElement` may say
   *  otherwise. Retrying across a couple of animation frames is the
   *  standard, low-risk mitigation for exactly this timing class; calling
   *  `.focus()` on an already-focused element is a harmless no-op, so the
   *  redundancy costs nothing on a browser/engine where the very first
   *  call already worked.
   *
   *  Every attempt (including the immediate one) is gated by default on
   *  nothing else already holding meaningful focus (`document.activeElement`
   *  is `body` or unset): `Canvas.svelte` mounts every placed tile at once,
   *  not one at a time behind a lazy tab, so an ungated auto-focus would
   *  steal keyboard focus from an unrelated field (search, settings,
   *  another module) whenever a terminal tile happens to mount, and would
   *  fight with sibling terminal tiles mounting in the same pass — e.g.
   *  restoring a saved layout with several terminals at once, where
   *  whichever tile's retry chain landed last would otherwise "win"
   *  arbitrarily (architecture review, Checkpoint 5f). Pass
   *  `{ gated: false }` for a moment that is itself already unambiguous user
   *  intent — the flip-back-to-front `MutationObserver` below is the one
   *  other caller, Checkpoint 5f2/Owner-Feedback: gating there would be
   *  self-defeating, since `document.activeElement` right after clicking
   *  the tile's own "Flip back" button is that button, not `body`, so the
   *  gate would always refuse to hand focus back to the terminal. A
   *  deliberate click straight into the tile body (the `.terminal` wrapper's
   *  own `onclick` below) is intentionally `inputEl?.focus()` directly, NOT
   *  this helper at all: that click IS the user's intent, a single
   *  synchronous call already worked for it before Checkpoint 5f, and
   *  giving every click its own retry chain would add the same
   *  cross-tile stale-focus risk to a path that was never broken. */
  function focusInputSoon(opts: { gated?: boolean } = {}): void {
    const gated = opts.gated ?? true;
    cancelPendingFocusAttempts();
    const attempt = () => {
      if (gated && document.activeElement && document.activeElement !== document.body) return;
      inputEl?.focus();
    };
    attempt();
    const id1 = requestAnimationFrame(() => {
      attempt();
      const id2 = requestAnimationFrame(attempt);
      focusRafIds.push(id2);
    });
    focusRafIds.push(id1);
  }

  /** Watches this tile's own `.tile-inner` wrapper (`Tile.svelte`) for its
   *  `flipped` class — present while the settings back-face is showing,
   *  absent while this terminal front-face is — and refocuses the typer the
   *  moment it goes from present to absent, i.e. right when the owner flips
   *  back from settings to the terminal. Checkpoint 5f2 (owner feedback
   *  after Checkpoint 5f shipped): `Tile.svelte` mounts both faces at once
   *  and only rotates between them in CSS (`transform: rotateY`), so this
   *  component's own `onMount` fires exactly once, the very first time the
   *  tile is placed — there is otherwise no signal at all telling this
   *  component "you just became visible again," so a second, later flip
   *  back to the front face silently stayed unfocused until a manual click
   *  into the tile body, same underlying complaint as Checkpoint 5f's
   *  original bug, just triggered from a different UI action. Mirrors
   *  `themeObserver` right below it — a `MutationObserver` on an ancestor's
   *  `class`/attribute, the same technique this file already uses for
   *  reacting to something changing outside this component's own props. */
  let flipObserver: MutationObserver | undefined;

  function watchFlipBack(): void {
    const tileInner = root?.closest<HTMLElement>(".tile-inner");
    if (!tileInner) {
      // Today this only happens for a genuine regression — every Terminal
      // tile is mounted inside a `Tile.svelte` (see its own `.tile-inner`
      // comment) and there is no other host yet. Warn instead of failing
      // silently (architecture review, Checkpoint 5f2), dev-only so it
      // can't spam a real user's console. Once a standalone/out-of-Tile
      // Terminal use exists (`docs/plans/terminal.md`'s own noted future
      // goal), this branch stops being an error and the warning below
      // should be revisited/removed alongside whatever change makes that
      // legitimate.
      if (import.meta.env.DEV) {
        console.warn('terminal.svelte: watchFlipBack() found no ancestor ".tile-inner" — flip-back autofocus is disabled for this instance.');
      }
      return;
    }
    let wasFlipped = tileInner.classList.contains("flipped");
    flipObserver = new MutationObserver(() => {
      const flipped = tileInner.classList.contains("flipped");
      if (wasFlipped && !flipped) {
        // Just flipped back to the front face — always the direct result of
        // the owner's own "Flip back" click, so (unlike onMount/spawn())
        // there is nothing else in the app whose focus this could be
        // stealing; gating would only suppress the very refocus this exists
        // to do (see `focusInputSoon`'s own doc comment).
        focusInputSoon({ gated: false });
      }
      wasFlipped = flipped;
    });
    flipObserver.observe(tileInner, { attributes: true, attributeFilter: ["class"] });
  }

  onMount(() => {
    if (canvasEl) context2d = canvasEl.getContext("2d");
    readThemeColors();
    focusInputSoon();
    watchFlipBack();
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
    destroyed = true;
    cancelAnimationFrame(raf);
    cancelPendingFocusAttempts();
    clearTimeout(resizeDebounce);
    clearTimeout(bellFlashTimeout);
    resizeObserver?.disconnect();
    themeObserver?.disconnect();
    flipObserver?.disconnect();
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
    /* Unconditionally transparent — see `backgroundOpacity`'s own doc
     *  comment for the two-round bug-fix history. The canvas is the *only*
     *  thing that paints this tile's background: at the default
     *  `backgroundOpacity` (1, fully opaque), it paints every default-bg
     *  cell at full alpha, which looks identical to this wrapper having its
     *  own opaque fill; below 1, only the canvas's own per-cell alpha
     *  blends toward whatever is actually behind the tile, so the slider is
     *  linear across its whole range. A second, independently-alpha-scaled
     *  layer here (the first fix attempt) would compound with the canvas's
     *  own blend instead of composing cleanly (architecture review).
     *  Trade-off: the sliver of canvas beyond the exact row/col grid (a
     *  rounding remainder when the tile's pixel size isn't an exact
     *  multiple of one cell) is now genuinely transparent rather than
     *  solid-coloured — a pre-existing rounding gap that simply wasn't
     *  visible before this file's background was always opaque. */
    background: transparent;
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
