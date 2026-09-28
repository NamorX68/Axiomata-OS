/**
 * What the editor draws around the text (`docs/plans/editor.md`, D8, G7): the
 * indentation guides and the cursor's glide. Pure and DOM-free like the rest of
 * the engine; the surface only turns the numbers into pixels.
 */

import type { TextStore } from "./buffer";
import { displayColumn, leadingWhitespace } from "./text";

/** Lines looked at beyond the visible range to indent a run of blank lines. */
const BLANK_LOOKAROUND = 200;

function leadCells(text: string, tabSize: number): number {
  const lead = leadingWhitespace(text);
  return displayColumn(lead, lead.length, tabSize);
}

/**
 * Indentation guides for lines `first..last`: the cells (display columns) at
 * which a guide is drawn, one per indentation level inside the line's leading
 * whitespace. A blank line takes the smaller indentation of the nearest
 * non-blank lines around it, so the guides of a block do not break at an
 * empty line in its middle.
 */
export function indentGuides(
  store: TextStore,
  first: number,
  last: number,
  indentSize: number,
  tabSize: number,
): Map<number, number[]> {
  const size = Math.max(1, indentSize);
  const lastLine = Math.min(last, store.lineCount() - 1);
  const guides = new Map<number, number[]>();
  const nearest = (from: number, step: -1 | 1): number => {
    for (let i = from, n = 0; i >= 0 && i < store.lineCount() && n < BLANK_LOOKAROUND; i += step, n++) {
      const text = store.line(i);
      if (text.trim() !== "") return leadCells(text, tabSize);
    }
    return 0;
  };
  for (let line = first; line <= lastLine; line++) {
    const text = store.line(line);
    const cells = text.trim() === "" ? Math.min(nearest(line - 1, -1), nearest(line + 1, 1)) : leadCells(text, tabSize);
    const levels: number[] = [];
    for (let cell = 0; cell < cells; cell += size) levels.push(cell);
    if (levels.length > 0) guides.set(line, levels);
  }
  return guides;
}

/**
 * Where the drawn cursor is (`x`, `y`, the head) and where its smear ends
 * (`tail`): the tail follows the head more slowly, so a glide draws a streak
 * that shrinks into the target as it lands (`docs/plans/editor-look.md`, K8).
 */
export interface CursorMotion {
  x: number;
  y: number;
  tail: { x: number; y: number };
}

export interface MotionOptions {
  /** Roughly how long the head takes to arrive. */
  durationMs: number;
  /** How much slower the tail follows: 1 keeps it on the head (no smear). */
  tailLag: number;
}

/** How strongly the cursor glides (K8): `subtle` is short and without a smear. */
export type GlideStrength = "subtle" | "strong";

/** The shortest glide (a character) and the longest (half a screen and further), per strength. */
const GLIDE_RANGE: Record<GlideStrength, { min: number; max: number; tailLag: number }> = {
  subtle: { min: 70, max: 130, tailLag: 1 },
  strong: { min: 80, max: 240, tailLag: 2.4 },
};
/** A jump this long (pixels) or longer takes the longest glide. */
const FAR_PX = 700;

/**
 * How a glide over `distancePx` runs (K8): the farther, the longer — a
 * character's step stays quick, a click across the screen travels visibly —
 * rising steeply at first, then flattening.
 */
export function glideMotion(distancePx: number, strength: GlideStrength): MotionOptions {
  const { min, max, tailLag } = GLIDE_RANGE[strength];
  const t = Math.min(1, Math.max(0, distancePx) / FAR_PX);
  return { durationMs: min + (max - min) * Math.sqrt(t), tailLag };
}

/** Closer than this (pixels) counts as arrived. */
const ARRIVED_PX = 0.5;

/**
 * One animation frame of the cursor's glide towards `target`: the head and the
 * tail each approach it exponentially — the tail `tailLag` times slower —
 * taking about `durationMs` whatever the frame rate. Returns the next state and
 * whether both have arrived.
 */
export function stepCursor(
  motion: CursorMotion,
  target: { x: number; y: number },
  dtMs: number,
  options: MotionOptions,
): { motion: CursorMotion; done: boolean } {
  const dt = Math.max(0, dtMs);
  const tau = Math.max(1, options.durationMs / 4);
  const approach = (from: { x: number; y: number }, lag: number) => {
    const k = 1 - Math.exp(-dt / (tau * lag));
    return { x: from.x + (target.x - from.x) * k, y: from.y + (target.y - from.y) * k };
  };
  const head = approach(motion, 1);
  const tail = options.tailLag <= 1 ? head : approach(motion.tail, options.tailLag);
  const near = (p: { x: number; y: number }) => Math.hypot(target.x - p.x, target.y - p.y) < ARRIVED_PX;
  if (near(head) && near(tail)) return { motion: { x: target.x, y: target.y, tail: { ...target } }, done: true };
  return { motion: { ...head, tail }, done: false };
}

/** A marked stretch of one line (a changed word, H4), `[from, to)` in UTF-16 columns. */
export interface LineMark {
  from: number;
  to: number;
  /** Drawn with the class `mk-<kind>`. */
  kind: string;
}

/** A button drawn on a line (unfold a gap, H11). */
export interface LineAction {
  id: string;
  label: string;
  title?: string;
}

/**
 * What a surface draws for one line beyond its text (`docs/plans/git-layer.md`,
 * H3): the diff view's colours, gutter and fold rows today, the Git-Gutter later.
 */
export interface LineDecoration {
  /** Drawn with the class `ln-<kind>` across the whole row, gutter included. */
  kind?: string;
  /** Replaces the line number. */
  gutter?: string;
  marks?: readonly LineMark[];
  /** Text drawn on a line that has none of its own (a fold, a note). */
  label?: string;
  actions?: readonly LineAction[];
}

/** Decorations for a whole document; with them, the gutter shows their labels, not line numbers. */
export interface LineDecorations {
  /** Width of the gutter, in cells. */
  gutterCells: number;
  line(line: number): LineDecoration | undefined;
}
