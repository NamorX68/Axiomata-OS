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

/** Where the drawn cursor is, and where it has just been. */
export interface CursorMotion {
  x: number;
  y: number;
  /** Earlier positions, oldest first, for the trail; empty when it rests. */
  trail: { x: number; y: number }[];
}

export interface MotionOptions {
  /** Roughly how long a glide takes. */
  durationMs: number;
  /** Keep a trail behind the moving cursor. */
  trail: boolean;
  /** Trail points kept. */
  trailLength?: number;
}

/** Closer than this (pixels) counts as arrived. */
const ARRIVED_PX = 0.5;

/**
 * One animation frame of the cursor's glide towards `target`: an exponential
 * approach that takes about `durationMs` whatever the frame rate. Returns the
 * next state and whether the cursor has arrived (then the trail is gone too).
 */
export function stepCursor(
  motion: CursorMotion,
  target: { x: number; y: number },
  dtMs: number,
  options: MotionOptions,
): { motion: CursorMotion; done: boolean } {
  const tau = Math.max(1, options.durationMs / 4);
  const k = 1 - Math.exp(-Math.max(0, dtMs) / tau);
  const x = motion.x + (target.x - motion.x) * k;
  const y = motion.y + (target.y - motion.y) * k;
  const arrived = Math.hypot(target.x - x, target.y - y) < ARRIVED_PX;
  if (arrived) return { motion: { x: target.x, y: target.y, trail: [] }, done: true };
  const trail = options.trail
    ? [...motion.trail, { x: motion.x, y: motion.y }].slice(-(options.trailLength ?? 8))
    : [];
  return { motion: { x, y, trail }, done: false };
}
