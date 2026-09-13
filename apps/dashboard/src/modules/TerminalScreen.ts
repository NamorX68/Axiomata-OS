/**
 * Pure Canvas-2D rendering for the terminal grid (Checkpoint 3 of
 * docs/plans/terminal.md). No Svelte, no Tauri — this file only knows how
 * to turn a `Cell` grid into pixels; `terminal.svelte` owns the canvas
 * element, the animation/blink loop, and the IPC wiring. Kept separate so
 * the colour-resolution logic (the part with actual room for bugs) is
 * plain-function testable without a real `CanvasRenderingContext2D`.
 */

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

/** Standard xterm 16-colour palette (SGR 30-37/90-97 map to indices 0-15) —
 *  the base ANSI colours most CLI tools assume. Not yet theme-customizable
 *  (a polished terminal often lets you override these); Checkpoint 3's own
 *  scope is drawing what the screen model already computes, not a palette
 *  picker — a reasonable follow-up for Checkpoint 5's "config" pass. */
const ANSI_16: readonly string[] = [
  "#000000",
  "#cd0000",
  "#00cd00",
  "#cdcd00",
  "#0000ee",
  "#cd00cd",
  "#00cdcd",
  "#e5e5e5",
  "#7f7f7f",
  "#ff0000",
  "#00ff00",
  "#ffff00",
  "#5c5cff",
  "#ff00ff",
  "#00ffff",
  "#ffffff",
];

/** The xterm 256-colour cube's 6 possible levels per channel — indices
 *  16-231 encode r/g/b each 0-5 against this table (the standard xterm
 *  algorithm, not invented here); 232-255 is a 24-step greyscale ramp
 *  instead (`8 + 10*n`, not part of the cube). */
const CUBE_LEVELS = [0, 95, 135, 175, 215, 255] as const;

function indexedToCss(index: number): string {
  if (index < 16) return ANSI_16[index] ?? ANSI_16[0];
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

/** Resolves a `TermColor` to a CSS colour string. `"default"` defers to
 *  `fallback` (the caller's own already-theme-resolved base fg/bg) rather
 *  than hardcoding one here — the screen model deliberately doesn't know
 *  about themes (see `Color`'s own doc comment in `screen.rs`). */
export function resolveColor(color: TermColor, fallback: string): string {
  switch (color.type) {
    case "default":
      return fallback;
    case "indexed":
      return indexedToCss(color.index);
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

export interface DrawOptions {
  rows: readonly (readonly TermCell[])[];
  /** `null` while the cursor is in its "off" blink phase, or there's no
   *  live session to show one for — `draw` just skips it, the blink timing
   *  itself is the caller's concern (`terminal.svelte`'s animation loop). */
  cursor: { row: number; col: number } | null;
  metrics: CharMetrics;
  /** Already resolved against the active theme (`--ax-text`/`--ax-surface-1`
   *  in practice) — see `resolveColor`'s own doc comment for why this model
   *  doesn't resolve "default" itself. */
  defaultFg: string;
  defaultBg: string;
  cursorColor: string;
  /** A plain CSS font shorthand with no weight, e.g. `"14px ui-monospace"` —
   *  `draw` prepends `"bold "` itself for bold cells, so a weight baked in
   *  here would double up. */
  font: string;
}

/**
 * Draws the whole grid: every cell's background rect, then (skipped for a
 * blank space — nothing to draw) its glyph, with an underline rect where
 * asked. Recomputes the whole canvas every call rather than diffing changed
 * cells — deliberately simple, matching Checkpoint 2's own "send the whole
 * snapshot" wire format choice, and cheap enough at ordinary terminal sizes;
 * diffing for very high-output cases (`yes`, a huge `cat`) is Checkpoint 5's
 * performance-tuning pass, not this one's.
 *
 * The cursor is drawn as a solid block in `cursorColor` with the
 * character re-drawn in `defaultBg` on top (classic reverse-video block
 * cursor), not as an outline — simplest correct rendering for Checkpoint 3;
 * shape options (bar/underline) are a possible Checkpoint 5 config item.
 */
export function draw(ctx: CanvasRenderingContext2D, options: DrawOptions): void {
  const { rows, cursor, metrics, defaultFg, defaultBg, cursorColor, font } = options;
  const { width: cw, height: ch, ascent } = metrics;
  ctx.textBaseline = "alphabetic";

  for (let r = 0; r < rows.length; r++) {
    const row = rows[r];
    for (let c = 0; c < row.length; c++) {
      const cell = row[c];
      const x = c * cw;
      const y = r * ch;
      const isCursor = cursor !== null && cursor.row === r && cursor.col === c;

      ctx.fillStyle = isCursor ? cursorColor : resolveColor(cell.bg, defaultBg);
      ctx.fillRect(x, y, cw, ch);

      if (cell.ch === " ") continue;
      ctx.font = cell.bold ? `bold ${font}` : font;
      ctx.fillStyle = isCursor ? defaultBg : resolveColor(cell.fg, defaultFg);
      ctx.fillText(cell.ch, x, y + ascent);
      if (cell.underline) {
        ctx.fillRect(x, y + ascent + 1, cw, 1);
      }
    }
  }
}
