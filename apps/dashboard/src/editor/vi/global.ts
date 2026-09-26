/**
 * `:g` and `:v` (`docs/plans/editor.md`, ED5, T12): which lines a pattern
 * marks, and keeping track of them while the command run on each one moves
 * the lines below it — Vim marks the lines first and then visits them, so a
 * `:g/x/d` that deletes one line does not skip the next.
 */

import type { TextStore } from "../buffer";
import type { TextChange } from "../document";
import { lineMatches, spansLines } from "../search/matches";
import type { LineRange } from "./ex";

/** The lines of `range` on which `re` matches (starts a match) — or, with `invert`, on which it does not. */
export function markedLines(store: TextStore, re: RegExp, range: LineRange, invert: boolean): number[] {
  const hit = new Set<number>();
  if (spansLines(re)) {
    // A match across lines marks the line it starts on.
    const text = store.text();
    let line = 0;
    let lineStart = 0;
    for (const [s] of lineMatches(text, re)) {
      while (true) {
        const nl = text.indexOf("\n", lineStart);
        if (nl === -1 || nl >= s) break;
        lineStart = nl + 1;
        line++;
      }
      hit.add(line);
    }
  } else {
    for (let l = range.first; l <= range.last; l++) if (lineMatches(store.line(l), re, 1).length > 0) hit.add(l);
  }
  const out: number[] = [];
  for (let l = range.first; l <= range.last; l++) if (hit.has(l) !== invert) out.push(l);
  return out;
}

/**
 * Line numbers that follow the text's changes: a change above moves them, a
 * change that removes their line drops them (`null`). Fed by the document's
 * `onTextChange`.
 */
export class LineAnchors {
  private readonly lines: Array<number | null>;

  constructor(lines: readonly number[]) {
    this.lines = [...lines];
  }

  /** Where anchor `i` is now, `null` if its line is gone. */
  at(i: number): number | null {
    return this.lines[i] ?? null;
  }

  get length(): number {
    return this.lines.length;
  }

  /**
   * Moves every anchor for one replacement of `[start, oldEnd)` by text that
   * ends at `newEnd`:
   *
   * * a line above it stays, a line below it moves by the lines gained or lost;
   * * a line strictly inside it is gone;
   * * the line it ends on survives (joined onto `newEnd`'s line) only if none
   *   of it was taken — the replacement ends at its very start;
   * * the line it starts on survives, unless the replacement took all of it
   *   from column 0 through its line break and put nothing back (`:d`).
   */
  change(c: TextChange): void {
    const spans = c.oldEnd.line > c.start.line;
    for (let i = 0; i < this.lines.length; i++) {
      const l = this.lines[i];
      if (l === null || l < c.start.line) continue;
      if (l > c.oldEnd.line) this.lines[i] = l + c.newEnd.line - c.oldEnd.line;
      else if (l === c.start.line) {
        const emptied = spans && c.start.col === 0 && c.newEnd.line === c.start.line && c.newEnd.col === 0;
        if (emptied && !(l === c.oldEnd.line)) this.lines[i] = null;
      } else if (l === c.oldEnd.line) this.lines[i] = c.oldEnd.col === 0 ? c.newEnd.line : null;
      else this.lines[i] = null;
    }
  }
}
