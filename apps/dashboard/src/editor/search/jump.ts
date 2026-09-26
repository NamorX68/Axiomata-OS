/**
 * Moving between matches the worker already found (`docs/plans/editor.md`,
 * ED5, T5): `n`, `N` and the highlights answer from a verdict's offsets by
 * binary search, without running the pattern again on the main thread.
 */

import type { Rope } from "../rope";
import { pos, range, type Pos, type Range } from "../position";

/** A match reached, and whether getting there went past an end of the text. */
export interface Jump {
  range: Range;
  wrapped: boolean;
}

/** Index of the first match starting after `offset` (`offsets` holds `[start, end)` pairs, in order). */
function firstAfter(offsets: Int32Array, offset: number): number {
  let lo = 0;
  let hi = offsets.length / 2;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (offsets[mid * 2] <= offset) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/**
 * The `count`-th match after `from` (before it when `backward`), wrapping
 * around the ends; a match *at* `from` does not count, as with Vim's `n`.
 * `null` when there is none at all.
 */
export function jumpTo(rope: Rope, offsets: Int32Array, from: Pos, backward: boolean, count = 1): Jump | null {
  const total = offsets.length / 2;
  if (total === 0) return null;
  let at = rope.offsetAt(from);
  let index = -1;
  let wrapped = false;
  for (let i = 0; i < Math.max(1, count); i++) {
    const after = firstAfter(offsets, at);
    if (backward) {
      // The last match starting before `at`.
      let k = after - 1;
      while (k >= 0 && offsets[k * 2] >= at) k--;
      if (k < 0) {
        k = total - 1;
        wrapped = true;
      }
      index = k;
    } else {
      index = after < total ? after : 0;
      if (after >= total) wrapped = true;
    }
    at = offsets[index * 2];
  }
  return { range: range(rope.posAt(offsets[index * 2]), rope.posAt(offsets[index * 2 + 1])), wrapped };
}

/** The non-empty match `at` lies in (its start included, its end not), or `null`. */
export function matchAround(rope: Rope, offsets: Int32Array, at: Pos): Range | null {
  const offset = rope.offsetAt(at);
  const k = firstAfter(offsets, offset) - 1;
  if (k < 0 || offsets[k * 2 + 1] <= offset) return null;
  return range(rope.posAt(offsets[k * 2]), rope.posAt(offsets[k * 2 + 1]));
}

/**
 * The non-empty matches touching lines `first`–`last`, cut into one
 * `[start, end)` column pair per line — a match across a line break shows on
 * both lines. At most `perLine` pairs on one line.
 */
export function matchesOnLines(
  rope: Rope,
  offsets: Int32Array,
  first: number,
  last: number,
  perLine = 200,
): Map<number, Array<[number, number]>> {
  const out = new Map<number, Array<[number, number]>>();
  const top = Math.max(0, first);
  const bottom = Math.min(last, rope.lineCount() - 1);
  if (top > bottom) return out;
  const startOffset = rope.offsetAt(pos(top, 0));
  const endOffset = rope.offsetAt(pos(bottom, rope.line(bottom).length));
  // Matches are sorted by start; one starting before the window may still reach into it.
  let k = Math.max(0, firstAfter(offsets, startOffset) - 1);
  while (k > 0 && offsets[k * 2 + 1] > startOffset) k--;
  for (; k < offsets.length / 2 && offsets[k * 2] <= endOffset; k++) {
    const s = offsets[k * 2];
    const e = offsets[k * 2 + 1];
    if (e <= s || e <= startOffset) continue;
    const a = rope.posAt(Math.max(s, startOffset));
    const b = rope.posAt(Math.min(e, endOffset));
    for (let line = a.line; line <= b.line; line++) {
      const from = line === a.line ? a.col : 0;
      const to = line === b.line ? b.col : rope.line(line).length;
      const list = out.get(line) ?? [];
      if (to <= from || list.length >= perLine) continue;
      list.push([from, to]);
      out.set(line, list);
    }
  }
  return out;
}
