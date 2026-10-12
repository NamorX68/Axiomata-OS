/**
 * An agent's trail in an open file (`docs/plans/ui-ux-round.md` UX6c, G8): when the file changes on disk and the
 * editor takes the new text, the lines that came out different glow for a moment, and the agent's flag stands at the
 * last of them. Honest, not acted: the lines are a diff of the text before and after — nothing is typed out. Pure and
 * DOM-free like the rest of the engine; the surface draws.
 */

import { diffSequences } from "./diff/myers";

/** What changed between two texts, in the lines of the new one. */
export interface Trail {
  /** The new text's lines that are new or different (0-based). */
  lines: number[];
  /** Where the flag stands: the last changed line — or, after a pure deletion, the line where text went missing. */
  last: number | null;
}

/** The new lines of `after` against `before`; a deletion marks the line that now stands in its place. */
export function trailOf(before: string, after: string): Trail {
  const a = before.split("\n");
  const b = after.split("\n");
  const lines: number[] = [];
  let at = 0;
  let last: number | null = null;
  for (const op of diffSequences(a, b)) {
    if (op.kind === "equal") {
      at += op.count;
    } else if (op.kind === "insert") {
      for (let i = 0; i < op.count; i++) lines.push(at + i);
      at += op.count;
      last = at - 1;
    } else {
      // Removed lines leave no line of their own: the gap is marked where it is now.
      const gap = Math.min(at, b.length - 1);
      if (gap >= 0) {
        if (lines[lines.length - 1] !== gap) lines.push(gap);
        last = gap;
      }
    }
  }
  return { lines: [...new Set(lines)].sort((x, y) => x - y), last };
}
