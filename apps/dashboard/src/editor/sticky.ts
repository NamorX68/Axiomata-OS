/**
 * Sticky scroll (`docs/plans/editor.md`, ED5, T9, T18): the header lines of the
 * blocks the top of the view is inside, pinned above the text — up to five,
 * outermost first.
 *
 * * **Which blocks** are the fold ranges (`fold/ranges.ts`): the syntax tree's
 *   functions, classes and bracketed blocks where there is a tree, else the
 *   indentation — one idea of "a block" for folding and for this.
 * * **A header sticks once it has scrolled away**: while its own line is still
 *   in view it is not repeated above it.
 * * **The innermost header is pushed up** by the line after its block, so a
 *   block ending under it never looks as if the next one belonged to it.
 * * **What the headers cover counts**: the text under them is not really on
 *   screen, so the cursor is kept below them and Vi's `H` starts there
 *   (`clearOfSticky`, used by the surface).
 */

import type { FoldRange } from "./fold/ranges";

/** Headers pinned at most. */
export const MAX_STICKY = 5;
/** Rounds `clearOfSticky` searches at most; each moves up by at least a pixel, the headers are at most five rows. */
const CLEAR_ROUNDS = 16;

/** What of the layout this needs (`VisualLayout` has it). */
export interface StickyLayout {
  readonly totalRows: number;
  /** The first row of `line`; for the line count itself (one past the end), `totalRows`. */
  firstRow(line: number): number;
  /** Where `line`'s rows start: one entry per visual row (a wrapped line has several). */
  rowStarts(line: number): readonly number[];
  lineAt(row: number): { line: number };
}

export interface Sticky {
  /** The blocks whose headers stick, outermost first. */
  headers: FoldRange[];
  /**
   * Pixels the innermost header is pushed up by (0 or less): the line after
   * its block is arriving under it.
   */
  push: number;
  /** Pixels the headers cover from the top of the view. */
  height: number;
}

export const NO_STICKY: Sticky = { headers: [], push: 0, height: 0 };

/** Ranges holding `line` below their header, outermost first (`ranges` sorted by start). */
export function enclosing(ranges: readonly FoldRange[], line: number): FoldRange[] {
  const out: FoldRange[] = [];
  for (const r of ranges) {
    if (r.start >= line) break;
    if (r.end >= line) out.push(r);
  }
  return out;
}

/**
 * The headers that stick with the view scrolled to `scrollTop` (pixels, rows
 * `rowH` tall). `ranges` must be sorted by start line.
 */
export function stickyAt(
  ranges: readonly FoldRange[],
  layout: StickyLayout,
  scrollTop: number,
  rowH: number,
  max = MAX_STICKY,
): Sticky {
  if (ranges.length === 0 || layout.totalRows === 0 || max <= 0) return NO_STICKY;
  const lineUnder = (y: number) => layout.lineAt(Math.floor(y / rowH)).line;
  // A header is only needed once its own rows have scrolled up past its slot — all of them: the pin shows
  // a wrapped header's first row only, and its last row still in view would show it twice.
  const scrolledPast = (r: FoldRange, slot: number) =>
    (layout.firstRow(r.start) + layout.rowStarts(r.start).length - 1) * rowH < scrollTop + slot * rowH;
  // Slot by slot: the line under slot `i` names the block of depth `i` around it — as long as the blocks
  // above it are the ones already pinned, and its header has scrolled up past the slot.
  const headers: FoldRange[] = [];
  for (let i = 0; i < max; i++) {
    const around = enclosing(ranges, lineUnder(scrollTop + i * rowH));
    const next = around[i];
    if (!next || !scrolledPast(next, i) || headers.some((h, k) => around[k] !== h)) break;
    headers.push(next);
  }
  if (headers.length === 0) return NO_STICKY;
  const n = headers.length;
  const last = headers[n - 1];
  const afterY = layout.firstRow(last.end + 1) * rowH - scrollTop;
  const push = Math.min(0, afterY - n * rowH);
  return { headers, push, height: Math.max(0, n * rowH + push) };
}

/**
 * The `scrollTop` that puts pixel row `y` (from the top of the text) just
 * below the headers that stick there — for keeping the cursor clear of them,
 * and for Vi's `zt`.
 */
export function clearOfSticky(y: number, heightAt: (scrollTop: number) => number): number {
  // The headers depend on where the view is, so the answer is searched for — only ever upwards, so it
  // ends where `y` is clear of them (the height can jump, so there is not always an exact fit).
  let top = Math.max(0, y);
  for (let i = 0; i < CLEAR_ROUNDS; i++) {
    const next = Math.max(0, Math.min(top, y - heightAt(top)));
    if (next === top) break;
    top = next;
  }
  return top;
}
