/**
 * Regions as the Vi machine needs them (`docs/plans/editor.md`, ED3, V2): the
 * Visual selection turned into a region, the lines a region touches, its first
 * and last position, and its size — so `.` can repeat a Visual change on as
 * much text from the cursor.
 */

import type { TextStore } from "../buffer";
import { comparePos, pos, type Pos } from "../position";
import { displayColumn, nextGrapheme, prevGrapheme } from "../text";
import { blockCols, type Region } from "./ops";
import { lastCharCol, nextPos } from "./scan";

export type VisualMode = "visual" | "visualLine" | "visualBlock";

/** How big a Visual selection was: lines, and characters (or columns of a block). */
export interface VisualSize {
  mode: VisualMode;
  lines: number;
  width: number;
}

/**
 * The region a Visual selection from `anchor` to `head` covers. Characterwise
 * it includes the character under the head (and a line break under it); a
 * block spans display columns, to every line's end after `$` (`toEnd`).
 */
export function visualRegion(
  store: TextStore,
  mode: VisualMode,
  anchor: Pos,
  head: Pos,
  toEnd: boolean,
  tabSize: number,
): Region {
  if (mode === "visualLine") {
    return { kind: "line", first: Math.min(anchor.line, head.line), last: Math.max(anchor.line, head.line) };
  }
  if (mode === "visualBlock") {
    const column = (p: Pos) => displayColumn(store.line(p.line), p.col, tabSize);
    const lastColumn = (p: Pos) => {
      const text = store.line(p.line);
      return p.col < text.length ? displayColumn(text, nextGrapheme(text, p.col), tabSize) - 1 : column(p);
    };
    return {
      kind: "block",
      first: Math.min(anchor.line, head.line),
      last: Math.max(anchor.line, head.line),
      left: Math.min(column(anchor), column(head)),
      right: toEnd ? Infinity : Math.max(lastColumn(anchor), lastColumn(head)),
    };
  }
  const forward = comparePos(anchor, head) <= 0;
  const start = forward ? anchor : head;
  const last = forward ? head : anchor;
  const text = store.line(last.line);
  const end = last.col < text.length ? pos(last.line, nextGrapheme(text, last.col)) : (nextPos(store, last) ?? last);
  return { kind: "char", start, end };
}

/** The lines a region touches (a characterwise one that ends at a line's start leaves that line out). */
export function regionLines(region: Region): { first: number; last: number } {
  if (region.kind === "char") {
    const endsAtLineStart = region.end.col === 0 && region.end.line > region.start.line;
    return { first: region.start.line, last: endsAtLineStart ? region.end.line - 1 : region.end.line };
  }
  return { first: region.first, last: region.last };
}

/** The same lines, whole — what `X D Y C S R` in Visual mode act on. */
export function wholeLines(region: Region): Region {
  const { first, last } = regionLines(region);
  return { kind: "line", first, last };
}

export function regionStart(store: TextStore, region: Region, tabSize: number): Pos {
  if (region.kind === "char") return region.start;
  if (region.kind === "line") return pos(region.first, 0);
  return pos(region.first, blockCols(store.line(region.first), region, tabSize)[0]);
}

export function regionEnd(store: TextStore, region: Region): Pos {
  if (region.kind === "char") return region.end;
  return pos(region.last, store.line(region.last).length);
}

/** The position before `p`, across a line break onto the previous line's last character. */
export function prevChar(store: TextStore, p: Pos): Pos {
  if (p.col > 0) return pos(p.line, prevGrapheme(store.line(p.line), p.col));
  return p.line > 0 ? pos(p.line - 1, lastCharCol(store, p.line - 1)) : p;
}

export function visualSize(mode: VisualMode, region: Region): VisualSize {
  if (region.kind === "char") {
    const lines = region.end.line - region.start.line + 1;
    return { mode, lines, width: lines === 1 ? region.end.col - region.start.col : region.end.col };
  }
  const lines = region.last - region.first + 1;
  if (region.kind === "line") return { mode, lines, width: 0 };
  return { mode, lines, width: region.right === Infinity ? 0 : region.right - region.left + 1 };
}

/** Where the head of a selection of `size` from `at` lies (for `.` after a Visual change). */
export function headForSize(store: TextStore, at: Pos, size: VisualSize): Pos {
  const lastLine = Math.min(store.lineCount() - 1, at.line + size.lines - 1);
  if (size.mode === "visualLine") return pos(lastLine, at.col);
  const within = (line: number, col: number) => pos(line, Math.min(lastCharCol(store, line), col));
  if (size.mode === "visualBlock" || size.lines === 1) return within(lastLine, at.col + size.width - 1);
  return within(lastLine, size.width - 1);
}
