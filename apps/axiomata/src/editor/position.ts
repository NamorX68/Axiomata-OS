/**
 * Positions, ranges and the selection of the editor model (`docs/plans/editor.md`,
 * ED1, F3).
 *
 * A position is `{ line, col }`, both zero-based; `col` counts **UTF-16 code
 * units**, JavaScript's own string unit, so `line.slice(0, col)` is always the
 * text before it. Where a person sees characters — moving, deleting, selecting a
 * word — the commands step by grapheme instead (`text.ts`), so a position never
 * lands inside an emoji or a combining sequence.
 *
 * Everything here is an immutable value; functions return new objects.
 */

export interface Pos {
  readonly line: number;
  readonly col: number;
}

/** A range between two positions, `start <= end`. */
export interface Range {
  readonly start: Pos;
  readonly end: Pos;
}

/**
 * The selection: `anchor` is where it started, `head` where the cursor is. They
 * are equal for a plain cursor. Unlike a `Range`, the head may come first — a
 * selection made with ⇧← grows to the left.
 */
export interface Selection {
  readonly anchor: Pos;
  readonly head: Pos;
}

export function pos(line: number, col: number): Pos {
  return { line, col };
}

/** Negative if `a` comes first, zero if equal, positive if `b` comes first. */
export function comparePos(a: Pos, b: Pos): number {
  return a.line - b.line || a.col - b.col;
}

export function posEqual(a: Pos, b: Pos): boolean {
  return a.line === b.line && a.col === b.col;
}

export function range(a: Pos, b: Pos): Range {
  return comparePos(a, b) <= 0 ? { start: a, end: b } : { start: b, end: a };
}

export function isEmpty(r: Range): boolean {
  return posEqual(r.start, r.end);
}

export function cursor(at: Pos): Selection {
  return { anchor: at, head: at };
}

/** The selection as a range, whichever way round it was made. */
export function selectionRange(sel: Selection): Range {
  return range(sel.anchor, sel.head);
}

export function isCursor(sel: Selection): boolean {
  return posEqual(sel.anchor, sel.head);
}
