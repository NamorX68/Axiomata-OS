/**
 * One file's diff as rows to draw (`docs/plans/git-layer.md`, H1, H2, H4, H11):
 * the hunks, the unchanged stretches between them folded away, and each
 * stretch unfoldable in steps when the whole text of a side is known.
 *
 * * **The hunks decide what changed** — git's, or `hunksFromTexts`' — never a
 *   second diff over the full texts: the view must agree with `+n −m`, and a
 *   hunk shown is exactly the hunk `discard_hunk` will take back (H6).
 * * **The full texts only fill the gaps** between hunks (unchanged, so either
 *   side will do) and tell how long the file is after the last hunk.
 * * **Two layouts from one model**: unified (one column, removed above added)
 *   and split (base left, change right, blank rows where a side has nothing).
 *
 * Row line numbers are zero-based indices into their side.
 */

import { parseHunkHeader, type DiffHunk, type HunkRange } from "./hunks";
import { wordChanges, type WordRange } from "./words";

export interface DiffSource {
  hunks: readonly DiffHunk[];
  /** The whole base side, when known (H2); `null` for a file the base lacks. */
  oldLines?: readonly string[] | null;
  /** The whole changed side, when known; `null` for a deleted file. */
  newLines?: readonly string[] | null;
  /** The diff was cut off: nothing is claimed about the file after the last hunk. */
  truncated?: boolean;
}

export interface LineRow {
  type: "line";
  kind: "context" | "add" | "remove";
  /** Index on the base side; `null` for an added line. */
  oldLine: number | null;
  /** Index on the changed side; `null` for a removed line. */
  newLine: number | null;
  text: string;
  /** The changed words (H4); empty when the line is not marked word by word. */
  words: readonly WordRange[];
  /** The hunk this line belongs to; `null` for context unfolded from a gap. */
  hunk: number | null;
}

export type FoldAction = "up" | "down" | "all";

/** Unchanged lines folded away. */
export interface FoldRow {
  type: "fold";
  gap: number;
  hidden: number;
  /** The enclosing function or heading git names in the next hunk's header. */
  label: string;
  /** What unfolding can do here; empty when the text of the gap is not known. */
  actions: readonly FoldAction[];
}

/** A remark between lines: a missing final newline, a cut-off diff. */
export interface NoteRow {
  type: "note";
  text: string;
}

export type UnifiedRow = LineRow | FoldRow | NoteRow;

/** One row of the split layout: the base's line left, the change's right. */
export interface PairRow {
  type: "pair";
  left: LineRow | null;
  right: LineRow | null;
}

export type SplitRow = PairRow | FoldRow | NoteRow;

/** Lines unfolded per click (H11). */
export const UNFOLD_STEP = 20;
/** Change blocks with more line pairs than this are not marked word by word. */
const MAX_WORD_PAIRS = 200;

export const NO_NEWLINE_NOTE = "No newline at end of file";
export const TRUNCATED_NOTE = "Diff cut off — the file is too large to show all of it";

/** Text as the diff counts lines: split on line breaks, no phantom line after a final one. */
export function textLines(text: string): string[] {
  const lines = text.split(/\r?\n/);
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

interface Gap {
  /** Index of the gap's first line on each side. */
  oldFirst: number;
  newFirst: number;
  count: number;
  /** Lines shown from the top (below the hunk before) and the bottom (above the hunk after). */
  top: number;
  bottom: number;
  label: string;
  position: "leading" | "middle" | "trailing";
}

type Segment = { type: "gap"; gap: number } | { type: "hunk"; hunk: number };

/** A hunk's range from its header, or failing that from its own line numbers. */
function rangeOf(hunk: DiffHunk): HunkRange {
  const parsed = parseHunkHeader(hunk.header);
  if (parsed) return parsed;
  const olds = hunk.lines.flatMap((l) => (l.kind !== "add" && l.oldLine !== null ? [l.oldLine] : []));
  const news = hunk.lines.flatMap((l) => (l.kind !== "remove" && l.newLine !== null ? [l.newLine] : []));
  return {
    oldStart: olds[0] ?? 0,
    oldCount: olds.length,
    newStart: news[0] ?? 0,
    newCount: news.length,
    context: "",
  };
}

/** The last one-based line a side's range covers (the line before it, when empty). */
function rangeEnd(start: number, count: number): number {
  return count === 0 ? start : start + count - 1;
}

export class DiffModel {
  private readonly gaps: Gap[] = [];
  private readonly segments: Segment[] = [];
  private readonly hunkRows: UnifiedRow[][];
  private wholeFile = false;
  private cachedUnified: UnifiedRow[] | null = null;
  private cachedSplit: SplitRow[] | null = null;

  constructor(private readonly source: DiffSource) {
    let oldEnd = 0;
    let newEnd = 0;
    source.hunks.forEach((hunk, index) => {
      const r = rangeOf(hunk);
      const newFirst = r.newCount === 0 ? r.newStart + 1 : r.newStart;
      const oldFirst = r.oldCount === 0 ? r.oldStart + 1 : r.oldStart;
      const count = Math.max(0, Math.min(newFirst - 1 - newEnd, oldFirst - 1 - oldEnd));
      if (count > 0) this.pushGap({ oldFirst: oldEnd, newFirst: newEnd, count, label: r.context }, index === 0);
      this.segments.push({ type: "hunk", hunk: index });
      oldEnd = rangeEnd(r.oldStart, r.oldCount);
      newEnd = rangeEnd(r.newStart, r.newCount);
    });
    const total = source.newLines?.length ?? null;
    const oldTotal = source.oldLines?.length ?? null;
    const trailing = total !== null ? total - newEnd : oldTotal !== null ? oldTotal - oldEnd : 0;
    if (!source.truncated && trailing > 0 && source.hunks.length > 0) {
      this.pushGap({ oldFirst: oldEnd, newFirst: newEnd, count: trailing, label: "" }, false, true);
    }
    this.hunkRows = source.hunks.map((hunk, index) => hunkRows(hunk, index));
  }

  private pushGap(g: Pick<Gap, "oldFirst" | "newFirst" | "count" | "label">, leading: boolean, trailing = false): void {
    const position = leading ? "leading" : trailing ? "trailing" : "middle";
    this.segments.push({ type: "gap", gap: this.gaps.length });
    this.gaps.push({ ...g, top: 0, bottom: 0, position });
  }

  /** Whether the text of the unchanged lines is known, so gaps can unfold. */
  get canUnfold(): boolean {
    return (this.source.newLines ?? this.source.oldLines) != null;
  }

  get hunkCount(): number {
    return this.source.hunks.length;
  }

  /** Unfolds part of gap `gap` (H11): `down` below the hunk before, `up` above the hunk after. */
  unfold(gap: number, action: FoldAction): void {
    const g = this.gaps[gap];
    if (!g || !this.canUnfold) return;
    const hidden = g.count - g.top - g.bottom;
    if (action === "all") g.top = g.count - g.bottom;
    else if (action === "down") g.top += Math.min(UNFOLD_STEP, hidden);
    else g.bottom += Math.min(UNFOLD_STEP, hidden);
    this.invalidate();
  }

  /** "Whole file" (H11): every gap unfolded, or all folded back. */
  setWholeFile(on: boolean): void {
    this.wholeFile = on && this.canUnfold;
    if (!on) for (const g of this.gaps) g.top = g.bottom = 0;
    this.invalidate();
  }

  get isWholeFile(): boolean {
    return this.wholeFile;
  }

  private invalidate(): void {
    this.cachedUnified = null;
    this.cachedSplit = null;
  }

  /** The rows of the unified layout (H1). */
  unified(): UnifiedRow[] {
    if (this.cachedUnified) return this.cachedUnified;
    const rows: UnifiedRow[] = [];
    for (const seg of this.segments) {
      if (seg.type === "hunk") rows.push(...this.hunkRows[seg.hunk]);
      else rows.push(...this.gapRows(seg.gap));
    }
    if (this.source.truncated) rows.push({ type: "note", text: TRUNCATED_NOTE });
    this.cachedUnified = rows;
    return rows;
  }

  /** The rows of the split layout (H1, H12): the same rows, paired side by side. */
  split(): SplitRow[] {
    if (this.cachedSplit) return this.cachedSplit;
    const out: SplitRow[] = [];
    const rows = this.unified();
    for (let i = 0; i < rows.length; i++) {
      const row = rows[i];
      if (row.type !== "line") {
        out.push(row);
        continue;
      }
      if (row.kind === "context") {
        out.push({ type: "pair", left: row, right: row });
        continue;
      }
      // A change block: its removed lines, then its added ones, side by side.
      // A note inside it (git's "no newline" after the last removed line) goes
      // after the pairs — the same block `markWords` pairs across it.
      const block = collectChangeBlock(rows, i);
      for (let k = 0; k < Math.max(block.removed.length, block.added.length); k++) {
        out.push({ type: "pair", left: block.removed[k] ?? null, right: block.added[k] ?? null });
      }
      out.push(...block.notes);
      i = block.end - 1;
    }
    this.cachedSplit = out;
    return out;
  }

  private gapRows(index: number): UnifiedRow[] {
    const g = this.gaps[index];
    const top = this.wholeFile ? g.count : Math.min(g.top, g.count);
    const bottom = this.wholeFile ? 0 : Math.min(g.bottom, g.count - top);
    const hidden = g.count - top - bottom;
    const rows: UnifiedRow[] = [];
    for (let i = 0; i < top; i++) rows.push(this.gapLine(g, i));
    if (hidden > 0) rows.push({ type: "fold", gap: index, hidden, label: g.label, actions: this.foldActions(g, hidden) });
    for (let i = g.count - bottom; i < g.count; i++) rows.push(this.gapLine(g, i));
    return rows;
  }

  private foldActions(g: Gap, hidden: number): FoldAction[] {
    if (!this.canUnfold) return [];
    if (hidden <= UNFOLD_STEP) return ["all"];
    if (g.position === "leading") return ["up", "all"];
    if (g.position === "trailing") return ["down", "all"];
    return ["down", "up", "all"];
  }

  private gapLine(g: Gap, i: number): LineRow {
    const oldLine = g.oldFirst + i;
    const newLine = g.newFirst + i;
    const text = this.source.newLines?.[newLine] ?? this.source.oldLines?.[oldLine] ?? "";
    return { type: "line", kind: "context", oldLine, newLine, text, words: [], hunk: null };
  }
}

/** A hunk's rows, with word marks on its change blocks and notes for missing newlines. */
function hunkRows(hunk: DiffHunk, index: number): UnifiedRow[] {
  const rows: UnifiedRow[] = [];
  const lines = hunk.lines;
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    if (l.kind === "no_newline") {
      rows.push({ type: "note", text: NO_NEWLINE_NOTE });
      continue;
    }
    rows.push({
      type: "line",
      kind: l.kind,
      oldLine: l.oldLine === null ? null : l.oldLine - 1,
      newLine: l.newLine === null ? null : l.newLine - 1,
      text: l.text.replace(/\r$/, ""),
      words: [],
      hunk: index,
    });
  }
  markWords(rows);
  return rows;
}

/** A run of change lines starting at `rows[start]`: its removed lines, then its added ones, notes set aside. */
interface ChangeBlock {
  removed: LineRow[];
  added: LineRow[];
  /** Notes (e.g. a missing final newline) found inside the block, in order. */
  notes: NoteRow[];
  /** The index just past the block — where the caller resumes. */
  end: number;
}

/**
 * Collects one change block from `start`: its removed lines, then the added
 * lines that replace them (a second run of removed lines starts a new block),
 * skipping over any notes in between. Shared by `split()` and `markWords()`,
 * which each pair the two sides of a block for a different purpose.
 */
function collectChangeBlock(rows: readonly UnifiedRow[], start: number): ChangeBlock {
  const removed: LineRow[] = [];
  const added: LineRow[] = [];
  const notes: NoteRow[] = [];
  let j = start;
  for (; j < rows.length; j++) {
    const r = rows[j];
    if (r.type === "note") {
      notes.push(r);
      continue;
    }
    if (r.type !== "line" || r.kind === "context" || (r.kind === "remove" && added.length > 0)) break;
    (r.kind === "remove" ? removed : added).push(r);
  }
  return { removed, added, notes, end: j };
}

/** Pairs each change block's removed lines with its added ones and marks their changed words (H4). */
function markWords(rows: UnifiedRow[]): void {
  let i = 0;
  while (i < rows.length) {
    const { removed, added, end } = collectChangeBlock(rows, i);
    const pairs = Math.min(removed.length, added.length);
    if (pairs > 0 && pairs <= MAX_WORD_PAIRS) {
      for (let k = 0; k < pairs; k++) {
        const changes = wordChanges(removed[k].text, added[k].text);
        if (!changes) continue;
        removed[k].words = changes.old;
        added[k].words = changes.new;
      }
    }
    i = end === i ? i + 1 : end;
  }
}
