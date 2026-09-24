/**
 * Turning highlight captures into coloured spans per line — the pure half of
 * highlighting, tested without tree-sitter.
 *
 * Captures overlap: a string contains an escape, a call contains its name, and
 * several query patterns may capture the very same node. Painting settles it
 * the way tree-sitter's own highlighter does: captures are painted outermost
 * first so an inner one overrides the part it covers, and of captures with the
 * same extent the one from the earlier pattern wins. Injected languages (a code
 * fence's Rust inside Markdown) are painted after their host and override it.
 */

import type { SyntaxToken } from "./tokens";

/** Bracket-pair colours by nesting depth (D8, G7), cycling through three. */
export type BracketToken = "bracket-1" | "bracket-2" | "bracket-3";

/** Anything a span can be coloured with. */
export type PaintToken = SyntaxToken | BracketToken;

export interface Capture {
  startRow: number;
  startCol: number;
  endRow: number;
  endCol: number;
  token: PaintToken;
  /** Pattern index in its query: lower wins among equal extents. */
  priority: number;
}

/** A coloured stretch of one line, `[from, to)` in UTF-16 columns. */
export interface Span {
  from: number;
  to: number;
  token: PaintToken;
}

function compare(a: Capture, b: Capture): number {
  // Earlier start first; of equal starts the longer (outer) one first; of equal
  // extents the higher pattern index first, so the lowest is painted last.
  return (
    a.startRow - b.startRow ||
    a.startCol - b.startCol ||
    b.endRow - a.endRow ||
    b.endCol - a.endCol ||
    b.priority - a.priority
  );
}

/**
 * Paints `layers` (host captures first, then each injection's) onto lines
 * `first..last` and returns the spans of every line that has any.
 */
export function paintLines(
  layers: readonly (readonly Capture[])[],
  first: number,
  last: number,
  lineLength: (line: number) => number,
): Map<number, Span[]> {
  const canvases = new Map<number, (PaintToken | null)[]>();
  const canvas = (line: number) => {
    let c = canvases.get(line);
    if (!c) {
      c = new Array<PaintToken | null>(lineLength(line)).fill(null);
      canvases.set(line, c);
    }
    return c;
  };
  for (const layer of layers) {
    for (const cap of [...layer].sort(compare)) {
      const from = Math.max(cap.startRow, first);
      const to = Math.min(cap.endRow, last);
      for (let line = from; line <= to; line++) {
        const cells = canvas(line);
        const start = line === cap.startRow ? cap.startCol : 0;
        const end = line === cap.endRow ? Math.min(cap.endCol, cells.length) : cells.length;
        for (let col = start; col < end; col++) cells[col] = cap.token;
      }
    }
  }
  const spans = new Map<number, Span[]>();
  for (const [line, cells] of canvases) {
    const runs = encodeRuns(cells);
    if (runs.length > 0) spans.set(line, runs);
  }
  return spans;
}

/** Run-length-encodes one line's canvas into spans, dropping the uncoloured runs. */
function encodeRuns(cells: readonly (PaintToken | null)[]): Span[] {
  const runs: Span[] = [];
  let col = 0;
  while (col < cells.length) {
    const token = cells[col];
    let end = col + 1;
    while (end < cells.length && cells[end] === token) end++;
    if (token) runs.push({ from: col, to: end, token });
    col = end;
  }
  return runs;
}

/** A piece of one visual row: its text and the token it is coloured with. */
export interface Segment {
  text: string;
  token: PaintToken | null;
}

/**
 * Cuts the part `[start, end)` of a line into coloured segments, given the
 * line's spans (sorted, non-overlapping). Uncoloured gaps become segments with
 * a `null` token, so the row's text is exactly the segments' texts joined.
 */
export function rowSegments(line: string, spans: readonly Span[] | undefined, start: number, end: number): Segment[] {
  if (!spans || spans.length === 0) return [{ text: line.slice(start, end), token: null }];
  const out: Segment[] = [];
  let at = start;
  for (const span of spans) {
    if (span.to <= start || span.from >= end) continue;
    const from = Math.max(span.from, start);
    const to = Math.min(span.to, end);
    if (from > at) out.push({ text: line.slice(at, from), token: null });
    out.push({ text: line.slice(from, to), token: span.token });
    at = to;
  }
  if (at < end) out.push({ text: line.slice(at, end), token: null });
  return out;
}
