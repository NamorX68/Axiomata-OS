/**
 * Pure Canvas-2D rendering for the terminal grid (Checkpoint 3 of
 * docs/plans/terminal.md, mouse selection added in Checkpoint 4). No
 * Svelte, no Tauri — this file only knows how to turn a `Cell` grid into
 * pixels (plus, now, a selection range into plain text); `terminal.svelte`
 * owns the canvas element, the animation/blink loop, mouse event listeners,
 * and the IPC wiring. Kept separate so the colour-resolution and selection
 * logic (the parts with actual room for bugs) are plain-function testable
 * without a real `CanvasRenderingContext2D`. Theme *data* (the named
 * 16-colour ANSI palettes) lives in `terminalThemes.ts`, not here — this
 * file only consumes a palette, it doesn't own the data (architecture
 * review, Checkpoint 5b).
 */

import { DEFAULT_THEME, THEMES } from "./terminalThemes";

/** Mirrors `axiomata-terminal::screen::Color` as it comes over the wire
 *  (`src-tauri/src/terminal.rs`'s `TerminalEvent::Screen`). Field names are
 *  snake_case, matching the Rust struct's own serde output exactly — this
 *  rides over a raw `Channel` payload, not an `invoke()` argument list, so
 *  none of Tauri's usual camelCase<->snake_case bridging applies here. */
export type TermColor =
  | { type: "default" }
  | { type: "indexed"; index: number }
  | { type: "rgb"; r: number; g: number; b: number };

/** Mirrors `axiomata-terminal::screen::Cell`. */
export interface TermCell {
  ch: string;
  fg: TermColor;
  bg: TermColor;
  bold: boolean;
  underline: boolean;
}

/** The xterm 256-colour cube's 6 possible levels per channel — indices
 *  16-231 encode r/g/b each 0-5 against this table (the standard xterm
 *  algorithm, not invented here); 232-255 is a 24-step greyscale ramp
 *  instead (`8 + 10*n`, not part of the cube). */
const CUBE_LEVELS = [0, 95, 135, 175, 215, 255] as const;

function indexedToCss(index: number, palette: readonly string[]): string {
  if (index < 16) return palette[index] ?? palette[0];
  if (index < 232) {
    const i = index - 16;
    const r = CUBE_LEVELS[Math.floor(i / 36) % 6];
    const g = CUBE_LEVELS[Math.floor(i / 6) % 6];
    const b = CUBE_LEVELS[i % 6];
    return `rgb(${r}, ${g}, ${b})`;
  }
  const level = 8 + (index - 232) * 10;
  return `rgb(${level}, ${level}, ${level})`;
}

/** Options for `resolveColor` beyond the color itself and its fallback —
 *  both Checkpoint 5b additions, both optional so every pre-existing caller
 *  (this file's own `draw`'s background-colour resolution included, plus
 *  every test written before this checkpoint) keeps working unchanged. */
export interface ResolveColorOptions {
  /** The 16-colour table to resolve indices 0-15 against — one of
   *  `THEMES`' values, defaulting to `THEMES.xterm` (the original `ANSI_16`
   *  table) when omitted. Resolving a `config.theme` *name* (e.g.
   *  `"nord"`) to this array, including falling back to `"xterm"` for an
   *  unrecognized/stale name, is the caller's job (`terminal.svelte`) —
   *  this function only ever sees the already-resolved table. */
  palette?: readonly string[];
  /** Classic terminal "bold text in a bright colour" convention
   *  (Checkpoint 5b): when set, an indexed colour in the low 8 (0-7) is
   *  resolved against its +8 "bright" counterpart instead. Only meaningful
   *  for `indexed` colours — a `"default"`/`"rgb"` colour ignores it
   *  entirely (there's no "bright" variant of an already-explicit RGB
   *  value, and `"default"` isn't a palette entry at all). */
  bright?: boolean;
}

/** Resolves a `TermColor` to a CSS colour string. `"default"` defers to
 *  `fallback` (the caller's own already-theme-resolved base fg/bg) rather
 *  than hardcoding one here — the screen model deliberately doesn't know
 *  about themes (see `Color`'s own doc comment in `screen.rs`). */
export function resolveColor(color: TermColor, fallback: string, options?: ResolveColorOptions): string {
  switch (color.type) {
    case "default":
      return fallback;
    case "indexed": {
      const palette = options?.palette ?? THEMES[DEFAULT_THEME];
      const index = options?.bright && color.index < 8 ? color.index + 8 : color.index;
      return indexedToCss(index, palette);
    }
    case "rgb":
      return `rgb(${color.r}, ${color.g}, ${color.b})`;
  }
}

export interface CharMetrics {
  /** One monospace cell's width in CSS px. */
  width: number;
  /** One monospace cell's height in CSS px. */
  height: number;
  /** Baseline offset from a cell's top — canvas `fillText` draws from the
   *  text baseline, not the top-left corner, so this is what turns a cell's
   *  `(x, y)` into the right `fillText` position. */
  ascent: number;
}

/** Measures a monospace font's actual cell size against a real canvas
 *  context — replaces Checkpoint 1's guessed `CHAR_W`/`CHAR_H` constants.
 *  Every character in a monospace font shares one width, so a single
 *  representative glyph is enough; "M" is the traditional choice (tall,
 *  wide, present in every font actually shipped as monospace). Falls back
 *  to `actualBoundingBox*`/a fixed ratio for the (now rare) browser missing
 *  `fontBoundingBox*` support, rather than throwing. */
export function measureChar(ctx: CanvasRenderingContext2D, font: string): CharMetrics {
  ctx.font = font;
  const m = ctx.measureText("M");
  const ascent = m.fontBoundingBoxAscent ?? m.actualBoundingBoxAscent ?? m.width * 0.8;
  const descent = m.fontBoundingBoxDescent ?? m.actualBoundingBoxDescent ?? m.width * 0.2;
  return { width: m.width, height: ascent + descent, ascent };
}

/** A cell coordinate — used for the cursor and (Checkpoint 4) selection
 *  endpoints alike. */
export interface CellPos {
  row: number;
  col: number;
}

/** Normalizes a `(start, end)` pair so `a` is never after `b` in reading
 *  order — the user can drag a selection in any of the four directions,
 *  but every consumer (`isCellSelected`, `selectionText`) wants one
 *  consistent "earlier" and "later" endpoint regardless of drag direction. */
function normalizeRange(start: CellPos, end: CellPos): [CellPos, CellPos] {
  if (start.row > end.row || (start.row === end.row && start.col > end.col)) {
    return [end, start];
  }
  return [start, end];
}

/**
 * Whether `(row, col)` falls inside a linear (stream) selection from
 * `start` to `end` — "linear" meaning it spans full row width for every row
 * strictly between the two endpoints (like selecting text in a document),
 * not a rectangular block. Order-independent: dragging up-and-left selects
 * the same cells as dragging down-and-right between the same two points.
 */
export function isCellSelected(row: number, col: number, start: CellPos, end: CellPos): boolean {
  const [a, b] = normalizeRange(start, end);
  if (row < a.row || row > b.row) return false;
  if (a.row === b.row) return col >= a.col && col <= b.col;
  if (row === a.row) return col >= a.col;
  if (row === b.row) return col <= b.col;
  return true; // a full row strictly between the two endpoints
}

/**
 * Plain text for a linear selection from `start` to `end` across `rows`,
 * one line per row joined with `\n` — what actually gets copied to the
 * clipboard. Trims nothing (a row's trailing spaces are real grid content,
 * same convention `Screen::line_text` uses on the Rust side), and clamps to
 * whatever rows/columns actually exist rather than assuming the endpoints
 * are in bounds (the selection was made against whatever was on screen at
 * drag time, which a resize or new output could have since changed under
 * it — see `terminal.svelte`'s own selection-clearing rules for when that
 * can happen).
 */
export function selectionText(rows: readonly (readonly TermCell[])[], start: CellPos, end: CellPos): string {
  const [a, b] = normalizeRange(start, end);
  const lines: string[] = [];
  for (let r = a.row; r <= b.row && r < rows.length; r++) {
    const row = rows[r];
    const fromCol = r === a.row ? a.col : 0;
    const toCol = r === b.row ? b.col + 1 : row.length;
    lines.push(
      row
        .slice(Math.max(0, fromCol), Math.min(toCol, row.length))
        .map((c) => c.ch)
        .join(""),
    );
  }
  return lines.join("\n");
}

/** The four cursor shapes Checkpoint 5b's "Cursor-Stil" setting offers.
 *  `block` is the original (only) Checkpoint 3 shape — reverse-video fill
 *  with the character re-drawn in `defaultBg` on top; the other three draw
 *  the cell completely normally (its own colours, its own glyph) and then
 *  overlay a `cursorColor` marker instead of inverting anything. */
export type CursorStyle = "block" | "outline" | "underline" | "bar";

/** `DrawOptions.cursorStyle`'s default when omitted, and the fallback for an
 *  unrecognized/stale `config.cursorStyle` value — exported (architecture
 *  review, Checkpoint 5b) so `terminal.svelte`/`terminal-settings.svelte`
 *  reference this one constant instead of each re-typing the literal
 *  `"block"`. */
export const DEFAULT_CURSOR_STYLE: CursorStyle = "block";

export interface DrawOptions {
  rows: readonly (readonly TermCell[])[];
  /** `null` while the cursor is in its "off" blink phase, or there's no
   *  live session to show one for — `draw` just skips it, the blink timing
   *  itself is the caller's concern (`terminal.svelte`'s animation loop). */
  cursor: CellPos | null;
  /** `null` when nothing is selected. Drawn as a translucent overlay, not a
   *  colour swap, so the cell's own colours stay legible underneath it. */
  selection: { start: CellPos; end: CellPos } | null;
  metrics: CharMetrics;
  /** Already resolved against the active theme (`--ax-text`/`--ax-surface-1`
   *  in practice) — see `resolveColor`'s own doc comment for why this model
   *  doesn't resolve "default" itself. */
  defaultFg: string;
  defaultBg: string;
  cursorColor: string;
  /** The selection overlay's fill colour — expected to already carry some
   *  transparency (e.g. the theme's `--ax-accent-muted` token) so it reads
   *  as a highlight, not an opaque colour swap. */
  selectionColor: string;
  /** A plain CSS font shorthand with no weight, e.g. `"14px ui-monospace"` —
   *  `draw` prepends `"bold "` itself for bold cells, so a weight baked in
   *  here would double up. */
  font: string;
  /** Checkpoint 5b's "Cursor-Stil" setting. Defaults to `"block"` — the
   *  original, only Checkpoint 3-5a shape — when omitted. */
  cursorStyle?: CursorStyle;
  /** Checkpoint 5b's 16-colour palette override for indexed foreground/
   *  background colours (see `THEMES`). Defaults to `THEMES.xterm` — the
   *  original palette — when omitted. Only affects `TermColor.indexed`
   *  values under 16; the 256-colour cube/greyscale ramp and truecolor
   *  `rgb` values are unaffected by theme, same as a real terminal. */
  palette?: readonly string[];
  /** Checkpoint 5b's "Bold-Text in heller Farbe" setting: when true, a bold
   *  cell whose *foreground* is one of the low 8 indexed colours (0-7) is
   *  drawn in its +8 "bright" counterpart instead — classic terminal
   *  convention. Never affects backgrounds, `"default"`, or truecolor `rgb`
   *  foregrounds (see `ResolveColorOptions.bright`'s own doc comment).
   *  Defaults to `false` when omitted; `terminal.svelte` defaults its own
   *  `config.boldIsBright` to `true` (the plan's stated "on by default"),
   *  so this parameter default only matters for a caller — tests included —
   *  that doesn't pass it explicitly. */
  boldIsBright?: boolean;
  /** Checkpoint 5b's "Transparenz/Deckkraft" setting, `0`-`1` (already
   *  converted from the settings page's `0`-`100` UI range). Applied via
   *  `ctx.globalAlpha` only while filling a cell's background *and only
   *  when that cell has no explicit background colour of its own*
   *  (`cell.bg.type === "default"`, i.e. it's about to be filled with
   *  `defaultBg`) — a program's own explicit background (e.g. a syntax
   *  highlighter's line, `less -R` colours) stays fully opaque, matching
   *  the plan's "NUR der Terminal-Hintergrund" wording. Text, the cursor,
   *  and the selection overlay are never affected regardless of this
   *  value — see the plan's own reasoning (dark-on-dark-transparent
   *  unreadability). Defaults to `1` (fully opaque, today's behaviour)
   *  when omitted. */
  backgroundOpacity?: number;
}

/** How thick a stroke/bar-style cursor's line is, in device-independent
 *  canvas units — thin enough to read as a marker, not a second block. */
const CURSOR_LINE_WIDTH = 2;

/**
 * Draws the whole grid: every cell's background rect, then (skipped for a
 * blank space — nothing to draw) its glyph, with an underline rect where
 * asked. Recomputes the whole canvas every call rather than diffing changed
 * cells — deliberately simple, matching Checkpoint 2's own "send the whole
 * snapshot" wire format choice, and cheap enough at ordinary terminal sizes;
 * diffing for very high-output cases (`yes`, a huge `cat`) is a possible
 * future performance-tuning pass, not this one's.
 *
 * The `block` cursor style is drawn as a solid fill in `cursorColor` with
 * the character re-drawn in `defaultBg` on top (classic reverse-video block
 * cursor) — the only shape before Checkpoint 5b. The other three styles
 * (`outline`/`underline`/`bar`) draw the cell completely normally first
 * (its own background, own glyph, own colours) and then overlay a thin
 * `cursorColor` marker on top, so the character underneath stays legible
 * rather than being inverted.
 */
export function draw(ctx: CanvasRenderingContext2D, options: DrawOptions): void {
  const {
    rows,
    cursor,
    selection,
    metrics,
    defaultFg,
    defaultBg,
    cursorColor,
    selectionColor,
    font,
    cursorStyle = DEFAULT_CURSOR_STYLE,
    palette = THEMES[DEFAULT_THEME],
    boldIsBright = false,
    backgroundOpacity = 1,
  } = options;
  const { width: cw, height: ch, ascent } = metrics;
  ctx.textBaseline = "alphabetic";

  // Explicit full clear, transform-independent — bug fix (owner-reported,
  // live-tested: overlapping/"ghosted" double text after a font-size
  // change, and `clear` not actually clearing). Every cell's background is
  // already painted opaquely below, which *should* make an explicit clear
  // redundant in the steady state — but that only holds if this frame's
  // grid covers exactly the same pixels the previous frame's did. It
  // doesn't always: at least one concrete way this can drift — the exact
  // root cause of the live-tested symptom wasn't fully confirmed, dev-mode
  // HMR may also play a role — is that the caller (`terminal.svelte`'s
  // `measureAndSize`, invoked from its `ResizeObserver` callback and its
  // `fontSizePx` effect, not from `tick()` itself, which only ever draws)
  // resizes the canvas's own backing store (which clears it, per the HTML5
  // canvas spec) independently of when a resized `rows` grid actually
  // arrives from the engine (a separate, debounced IPC round trip) — a
  // frame can land with new cell metrics (from a just-changed font size)
  // over old grid dimensions, or vice versa, leaving stale pixels outside
  // whatever this frame actually painted. Robust regardless of the exact
  // cause: `save`/`restore` around an identity transform so this clears the
  // *whole* backing store regardless of the DPR scale transform the caller
  // already applied — using CSS-pixel coordinates here (under that
  // transform) would only clear the visible viewport at the *current* DPR,
  // not necessarily the whole backing store if it changed since.
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
  ctx.restore();

  for (let r = 0; r < rows.length; r++) {
    const row = rows[r];
    for (let c = 0; c < row.length; c++) {
      const cell = row[c];
      const x = c * cw;
      const y = r * ch;
      const isCursor = cursor !== null && cursor.row === r && cursor.col === c;
      const isBlockCursor = isCursor && cursorStyle === "block";

      if (isBlockCursor) {
        ctx.fillStyle = cursorColor; // the cursor itself is never affected by backgroundOpacity
        ctx.fillRect(x, y, cw, ch);
      } else {
        // `backgroundOpacity` only applies to a cell with no explicit
        // background of its own (about to be filled with `defaultBg`) — see
        // `DrawOptions.backgroundOpacity`'s own doc comment for why an
        // explicit per-cell background (e.g. a colored prompt) stays opaque.
        ctx.globalAlpha = cell.bg.type === "default" ? backgroundOpacity : 1;
        ctx.fillStyle = resolveColor(cell.bg, defaultBg);
        ctx.fillRect(x, y, cw, ch);
        ctx.globalAlpha = 1;
      }
      // Drawn as a translucent overlay on top of the cell's own background
      // (not instead of it), so selected text keeps reading with its
      // normal colours underneath the highlight. Always fully opaque
      // regardless of `backgroundOpacity` (see that option's doc comment).
      if (selection && isCellSelected(r, c, selection.start, selection.end)) {
        ctx.fillStyle = selectionColor;
        ctx.fillRect(x, y, cw, ch);
      }

      if (cell.ch !== " ") {
        ctx.font = cell.bold ? `bold ${font}` : font;
        ctx.fillStyle = isBlockCursor
          ? defaultBg
          : resolveColor(cell.fg, defaultFg, { palette, bright: cell.bold && boldIsBright });
        ctx.fillText(cell.ch, x, y + ascent);
        if (cell.underline) {
          ctx.fillRect(x, y + ascent + 1, cw, 1);
        }
      }

      if (isCursor && cursorStyle !== "block") {
        ctx.fillStyle = cursorColor;
        switch (cursorStyle) {
          case "outline":
            ctx.strokeStyle = cursorColor;
            ctx.lineWidth = 1;
            // Inset by half a device pixel so the 1px stroke lands crisply
            // on the pixel grid rather than straddling (and blurring
            // across) two rows/columns.
            ctx.strokeRect(x + 0.5, y + 0.5, cw - 1, ch - 1);
            break;
          case "underline":
            ctx.fillRect(x, y + ch - CURSOR_LINE_WIDTH, cw, CURSOR_LINE_WIDTH);
            break;
          case "bar":
            ctx.fillRect(x, y, CURSOR_LINE_WIDTH, ch);
            break;
        }
      }
    }
  }
}
