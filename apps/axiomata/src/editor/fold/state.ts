/**
 * Which ranges are folded in one open document (`docs/plans/editor.md`, ED5,
 * T7, T18), and what that hides.
 *
 * * **Closed folds are plain line ranges**, kept apart from where the text
 *   *could* fold (`ranges.ts`): once closed, a fold stays as it was even while
 *   the grammar re-reads the text around it.
 * * **They follow the text** (`follow`): lines added or taken away above a
 *   fold move it, edits among its hidden lines stretch or shrink it, and an
 *   edit that reaches across its header or its last line opens it — the fold
 *   no longer describes what is there. Typing on the header line keeps it.
 * * **Nested folds keep their own state**: opening an outer fold shows an
 *   inner one still closed, as in Vim and VS Code.
 * * **Hidden lines are never where the cursor is**: the view calls `reveal`
 *   after every command, which opens whatever hides the cursor — a search
 *   match, an undo, a jump to a line.
 *
 * Several surfaces may share one state (a split diff's two panes, T17): it
 * needs no document when the text never changes.
 */

import type { EditorDocument, TextChange } from "../document";
import type { FoldRange } from "./ranges";

/** A stretch of hidden lines, `from` to `to` inclusive. */
export interface HiddenLines {
  from: number;
  to: number;
}

/** What Vi and the commands ask about folds (`ViContext.folds`). */
export interface FoldLookup {
  /** The outermost closed fold `line` is the header or a hidden line of, or `null`. */
  closedAround(line: number): FoldRange | null;
}

export class FoldState implements FoldLookup {
  /** Closed folds, sorted by start line; never two with the same start. */
  private list: FoldRange[] = [];
  private hiddenCache: HiddenLines[] | null = null;
  /** Grows with every change of what is folded (not with the text shifting it). */
  version = 0;
  private readonly unsubscribe: () => void;

  constructor(doc: EditorDocument | null = null) {
    this.unsubscribe = doc ? doc.onTextChange((change) => this.follow(change)) : () => {};
  }

  dispose(): void {
    this.unsubscribe();
  }

  /** The closed folds, by start line. */
  get closed(): readonly FoldRange[] {
    return this.list;
  }

  /** The closed fold whose header is `line`, or `null`. */
  closedAt(line: number): FoldRange | null {
    return this.list.find((r) => r.start === line) ?? null;
  }

  closedAround(line: number): FoldRange | null {
    // Every hidden stretch begins right under a shown header (`hidden` merges nested folds into it).
    const hidden = this.hiddenAt(line);
    if (hidden) return { start: hidden.from - 1, end: hidden.to };
    if (!this.closedAt(line)) return null;
    const under = this.hiddenAt(line + 1);
    return under ? { start: line, end: under.to } : null;
  }

  /** Closes `range` (replacing a closed fold with the same header). */
  close(range: FoldRange): void {
    if (range.end <= range.start) return;
    this.list = [...this.list.filter((r) => r.start !== range.start), { ...range }].sort((a, b) => a.start - b.start);
    this.touched();
  }

  /** Opens the fold whose header is `line`; whether there was one. */
  open(line: number): boolean {
    const before = this.list.length;
    this.list = this.list.filter((r) => r.start !== line);
    if (this.list.length === before) return false;
    this.touched();
    return true;
  }

  /** Closes every range in `ranges` (fold all). */
  closeAll(ranges: readonly FoldRange[]): void {
    const byStart = new Map(this.list.map((r) => [r.start, r]));
    for (const r of ranges) if (r.end > r.start) byStart.set(r.start, { ...r });
    this.list = [...byStart.values()].sort((a, b) => a.start - b.start);
    this.touched();
  }

  openAll(): void {
    if (this.list.length === 0) return;
    this.list = [];
    this.touched();
  }

  /** Opens every fold that hides `line`; whether anything opened. */
  reveal(line: number): boolean {
    const before = this.list.length;
    this.list = this.list.filter((r) => !(r.start < line && line <= r.end));
    if (this.list.length === before) return false;
    this.touched();
    return true;
  }

  /** The hidden lines as disjoint stretches, in order (touching ones merged). */
  hidden(): readonly HiddenLines[] {
    if (this.hiddenCache) return this.hiddenCache;
    const out: HiddenLines[] = [];
    for (const r of this.list) {
      const last = out[out.length - 1];
      if (last && r.start + 1 <= last.to + 1) last.to = Math.max(last.to, r.end);
      else out.push({ from: r.start + 1, to: r.end });
    }
    this.hiddenCache = out;
    return out;
  }

  /** The hidden stretch `line` is in, or `null` if it is shown. */
  hiddenAt(line: number): HiddenLines | null {
    const stretches = this.hidden();
    let lo = 0;
    let hi = stretches.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const s = stretches[mid];
      if (line < s.from) hi = mid - 1;
      else if (line > s.to) lo = mid + 1;
      else return s;
    }
    return null;
  }

  /** How many lines between `a` and `b` (either order, both shown) are folded away. */
  hiddenBetween(a: number, b: number): number {
    const lo = Math.min(a, b);
    const hi = Math.max(a, b);
    let n = 0;
    for (const s of this.hidden()) {
      if (s.from > hi) break;
      n += Math.max(0, Math.min(s.to, hi) - Math.max(s.from, lo) + 1);
    }
    return n;
  }

  /**
   * The nearest shown line at `line` or past it in `dir`: a hidden line
   * going up is its fold's header; going down, the line after the fold
   * (`null` when the fold reaches the end of the text of `lineCount` lines).
   */
  visibleLine(line: number, dir: -1 | 1, lineCount: number): number | null {
    const s = this.hiddenAt(line);
    if (!s) return line;
    if (dir < 0) return s.from - 1;
    return s.to + 1 < lineCount ? s.to + 1 : null;
  }

  /** Moves, stretches or opens folds after one replacement of the text. */
  follow(change: TextChange): void {
    const delta = change.newEnd.line - change.oldEnd.line;
    const first = change.start.line;
    const last = change.oldEnd.line;
    // Whole lines put in or taken out right above a header (`O`, `dd` on the line before).
    const aboveHeader = (r: FoldRange) => last === r.start && change.oldEnd.col === 0 && change.newEnd.col === 0;
    const next: FoldRange[] = [];
    let changed = false;
    for (const r of this.list) {
      const onHeader = first === r.start && last === r.start && delta === 0;
      const inside = first > r.start && last <= r.end && r.end + delta > r.start;
      if (first > r.end || onHeader) next.push(r);
      else if (last < r.start || aboveHeader(r)) next.push({ start: r.start + delta, end: r.end + delta });
      else if (inside) next.push({ start: r.start, end: r.end + delta });
      else changed = true;
    }
    this.list = next;
    this.hiddenCache = null;
    if (changed) this.version++;
  }

  /** The closed folds as `[start, end]` pairs, to keep across restarts. */
  serialize(): [number, number][] {
    return this.list.map((r) => [r.start, r.end]);
  }

  /** Closed folds from `serialize`, dropping any that no longer fit `lineCount` lines. */
  restore(saved: unknown, lineCount: number): void {
    const list: FoldRange[] = [];
    if (Array.isArray(saved)) {
      for (const pair of saved) {
        if (!Array.isArray(pair) || pair.length !== 2) continue;
        const [start, end] = pair as unknown[];
        if (!Number.isInteger(start) || !Number.isInteger(end)) continue;
        const s = start as number;
        const e = end as number;
        if (s >= 0 && e > s && e < lineCount && !list.some((r) => r.start === s)) list.push({ start: s, end: e });
      }
    }
    this.list = list.sort((a, b) => a.start - b.start);
    this.touched();
  }

  private touched(): void {
    this.hiddenCache = null;
    this.version++;
  }
}
