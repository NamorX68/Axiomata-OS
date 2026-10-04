/**
 * Which words inside a changed line changed (`docs/plans/git-layer.md`, H4): a
 * removed line and the added line that replaced it are split into words and
 * compared, and the parts that differ are marked on each side.
 *
 * Only worth it when the two lines are recognisably the same line edited. Two
 * unrelated lines would come out marked almost everywhere, which reads worse
 * than no marks at all, so below a share of common text nothing is marked.
 */

import { diffSequences } from "./myers";

/** A changed stretch of one line, `[from, to)` in UTF-16 columns. */
export interface WordRange {
  from: number;
  to: number;
}

export interface WordChanges {
  old: WordRange[];
  new: WordRange[];
}

/** Longer lines are not compared word by word (a minified file's one line). */
export const MAX_WORD_DIFF_LINE = 1000;
/** Less than this share of the longer line in common: the lines are unrelated. */
const MIN_COMMON_SHARE = 0.3;

/** Runs of letters/digits/underscore, runs of whitespace, and every other character alone. */
const TOKEN = /[\p{L}\p{N}_]+|\s+|[^\p{L}\p{N}_\s]/gu;

export function tokenize(line: string): string[] {
  return line.match(TOKEN) ?? [];
}

/** Adds `[from, to)`, merging with the range before it when they touch. */
function add(ranges: WordRange[], from: number, to: number): void {
  if (to <= from) return;
  const last = ranges[ranges.length - 1];
  if (last && last.to === from) last.to = to;
  else ranges.push({ from, to });
}

/**
 * The changed words of `oldLine` → `newLine`, or `null` when the lines are too
 * long or too different to mark sensibly. Identical lines give no ranges.
 */
export function wordChanges(oldLine: string, newLine: string): WordChanges | null {
  if (oldLine.length > MAX_WORD_DIFF_LINE || newLine.length > MAX_WORD_DIFF_LINE) return null;
  const a = tokenize(oldLine);
  const b = tokenize(newLine);
  const out: WordChanges = { old: [], new: [] };
  let colA = 0;
  let colB = 0;
  let ia = 0;
  let ib = 0;
  let common = 0;
  for (const op of diffSequences(a, b)) {
    for (let n = 0; n < op.count; n++) {
      if (op.kind === "equal") {
        const len = a[ia].length;
        if (a[ia].trim() !== "") common += len;
        colA += len;
        colB += len;
        ia++;
        ib++;
      } else if (op.kind === "delete") {
        add(out.old, colA, colA + a[ia].length);
        colA += a[ia++].length;
      } else {
        add(out.new, colB, colB + b[ib].length);
        colB += b[ib++].length;
      }
    }
  }
  const longer = Math.max(oldLine.trim().length, newLine.trim().length);
  if (longer > 0 && common / longer < MIN_COMMON_SHARE) return null;
  return out;
}
