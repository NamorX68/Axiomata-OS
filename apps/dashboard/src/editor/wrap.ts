/**
 * Soft wrapping of one line (`docs/plans/editor.md`, ED1, F6).
 *
 * Widths are counted in cells, not pixels: every bundled font is monospace, so
 * the view measures one cell once and everything else is arithmetic
 * (`text.ts`'s `cellWidth` gives wide characters two cells). A line breaks after
 * the last space that still fits; a word longer than a whole row breaks hard.
 * Spaces never start a row — they hang at the end of the row they follow, the
 * way every text editor does it. Continuation rows keep the line's own
 * indentation, so a wrapped list item or code line stays visibly one block.
 */

import { cellWidth, leadingWhitespace } from "./text";

const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });

export interface WrappedLine {
  /** UTF-16 columns where each visual row starts; `[0]` if the line fits. */
  starts: number[];
  /** Cells continuation rows are indented by (0 for the first row). */
  indent: number;
}

/**
 * Wraps `text` into rows of at most `width` cells. The continuation indent is
 * dropped when it would take more than half the row — a deeply indented line
 * in a narrow pane would otherwise wrap one word per row.
 */
export function wrapLine(text: string, width: number, tabSize: number): WrappedLine {
  const lead = leadingWhitespace(text);
  let leadCells = 0;
  for (const seg of graphemes.segment(lead)) leadCells += cellWidth(seg.segment, leadCells, tabSize);
  const indent = leadCells > 0 && leadCells <= width / 2 ? leadCells : 0;

  const starts = [0];
  let column = 0; // absolute display column, for tab stops
  let rowStartColumn = 0;
  let lastBreak = -1; // UTF-16 index just after the latest space in this row
  let lastBreakColumn = 0;
  for (const seg of graphemes.segment(text)) {
    const cells = cellWidth(seg.segment, column, tabSize);
    const limit = starts.length === 1 ? width : width - indent;
    const isSpace = seg.segment === " " || seg.segment === "\t";
    const rowIsEmpty = column === rowStartColumn;
    if (!isSpace && !rowIsEmpty && column + cells - rowStartColumn > limit) {
      if (lastBreak > starts[starts.length - 1]) {
        starts.push(lastBreak);
        rowStartColumn = lastBreakColumn;
      } else {
        starts.push(seg.index);
        rowStartColumn = column;
      }
    }
    column += cells;
    if (isSpace) {
      lastBreak = seg.index + seg.segment.length;
      lastBreakColumn = column;
    }
  }
  return { starts, indent: starts.length > 1 ? indent : 0 };
}

/** The line unwrapped: one row. */
export const SINGLE_ROW: WrappedLine = { starts: [0], indent: 0 };
