/**
 * Where things are on screen, in cells and visual rows (`docs/plans/editor.md`,
 * ED1.2): the cursor, the selection's highlight runs, and the text position under
 * a click. The view only multiplies by the measured cell width and row height,
 * so every rule here — a continuation row's indent, the extra cell a selected
 * line break gets, which row owns a column at a wrap point — is tested without a
 * DOM.
 */

import type { TextStore } from "./buffer";
import { rowIndex } from "./commands";
import { pos, type Pos, type Range } from "./position";
import { colForDisplayColumn, displayColumn } from "./text";
import type { VisualLayout } from "./visual";

/** One visual row: which part of which line it shows. */
export interface RowSlice {
  line: number;
  sub: number;
  /** UTF-16 range of the line this row shows. */
  start: number;
  end: number;
  text: string;
  /** Cells the row is indented by (continuation rows only). */
  indent: number;
  /** The row is the line's last. */
  last: boolean;
}

export function rowSlice(layout: VisualLayout, store: TextStore, row: number): RowSlice {
  const { line, sub } = layout.lineAt(row);
  const wrapped = layout.wrapped(line);
  const text = store.line(line);
  const start = wrapped.starts[sub];
  const last = sub === wrapped.starts.length - 1;
  const end = last ? text.length : wrapped.starts[sub + 1];
  return { line, sub, start, end, text: text.slice(start, end), indent: sub > 0 ? wrapped.indent : 0, last };
}

/** Cells from the row's left edge to `col` of `line`, indent included. */
function cellInRow(
  store: TextStore,
  line: number,
  start: number,
  indent: number,
  col: number,
  tabSize: number,
): number {
  const text = store.line(line);
  return indent + displayColumn(text, col, tabSize) - displayColumn(text, start, tabSize);
}

/** The visual row and cell a position is drawn at. */
export function cursorCell(
  layout: VisualLayout,
  store: TextStore,
  p: Pos,
  tabSize: number,
): { row: number; cell: number } {
  const wrapped = layout.wrapped(p.line);
  const sub = rowIndex(wrapped.starts, p.col);
  const indent = sub > 0 ? wrapped.indent : 0;
  return {
    row: layout.firstRow(p.line) + sub,
    cell: cellInRow(store, p.line, wrapped.starts[sub], indent, p.col, tabSize),
  };
}

/** A highlighted stretch of one visual row, `[from, to)` in cells. */
export interface Run {
  row: number;
  from: number;
  to: number;
}

/**
 * The selection's highlight on rows `firstRow..lastRow` (the visible ones). A
 * selected line break shows as one extra cell at the end of the line's last row,
 * so selecting across an empty line still shows that line as selected.
 */
export function selectionRuns(
  layout: VisualLayout,
  store: TextStore,
  r: Range,
  firstRow: number,
  lastRow: number,
  tabSize: number,
): Run[] {
  const runs: Run[] = [];
  const from = Math.max(firstRow, layout.firstRow(r.start.line));
  // `rowCount`, not `rowStarts`: a line a fold hides has no rows, and its first row belongs to the line after
  // the fold — counting one made every mark on a hidden line (a diagnostic, a search match) cover that line.
  const to = Math.min(lastRow, layout.firstRow(r.end.line) + layout.rowCount(r.end.line) - 1);
  for (let row = from; row <= to; row++) {
    const slice = rowSlice(layout, store, row);
    const selStart = slice.line === r.start.line ? r.start.col : 0;
    const selEnd = slice.line === r.end.line ? r.end.col : store.line(slice.line).length;
    const a = Math.max(selStart, slice.start);
    const b = Math.min(selEnd, slice.end);
    const breakSelected = slice.last && slice.line < r.end.line;
    if (a > b || (a === b && !breakSelected)) continue;
    const left = cellInRow(store, slice.line, slice.start, slice.indent, a, tabSize);
    const right = cellInRow(store, slice.line, slice.start, slice.indent, b, tabSize) + (breakSelected ? 1 : 0);
    if (right > left) runs.push({ row, from: left, to: right });
  }
  return runs;
}

/**
 * The text position nearest to `cell` on visual row `row` — a click. Past the
 * end of a row that continues on the next one it stays on this row (just before
 * the wrap point), so a click in the empty right margin of a wrapped paragraph
 * does not jump down a row.
 */
export function posAtCell(layout: VisualLayout, store: TextStore, row: number, cell: number, tabSize: number): Pos {
  const slice = rowSlice(layout, store, Math.min(Math.max(row, 0), layout.totalRows - 1));
  const lineText = store.line(slice.line);
  const offset = displayColumn(lineText, slice.start, tabSize);
  // colForDisplayColumn works in line columns; shift the target by the cells
  // before the row so tabs keep their real stops.
  const target = Math.max(0, cell - slice.indent) + offset;
  let col = colForDisplayColumn(lineText, target, tabSize);
  col = Math.min(Math.max(col, slice.start), slice.end);
  if (!slice.last && col === slice.end && slice.end > slice.start) col = slice.end - 1;
  return pos(slice.line, col);
}

/**
 * `items` with the later of any two that share a `key` left out. The view draws its marks in a keyed
 * `{#each}`, where one repeated key stops the whole surface from updating; two marks with one key
 * are one place drawn twice, so dropping the second loses nothing.
 */
export function uniqueByKey<T extends { key: string }>(items: readonly T[]): T[] {
  const seen = new Set<string>();
  const out: T[] = [];
  for (const item of items) {
    if (seen.has(item.key)) continue;
    seen.add(item.key);
    out.push(item);
  }
  return out;
}
