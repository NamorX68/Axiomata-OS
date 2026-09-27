/**
 * The document as rows on screen (`docs/plans/editor.md`, ED1, F6): which logical
 * line a visual row belongs to and back, for scrolling, hit-testing and the
 * `RowLayout` the commands walk ↑/↓ with.
 *
 * Wrapping a line is pure (`wrap.ts`), so results are cached by the line's text:
 * after a keystroke only the edited line is wrapped again, the other fifty
 * thousand are map lookups. The row offsets (a prefix sum over rows per line)
 * are rebuilt by `refresh`, which the view calls once per document revision or
 * width change — linear, but a few milliseconds even for large files.
 *
 * Folded lines (ED5, T7) have no rows at all: the fold's header keeps its own,
 * and the row after it belongs to the first line past the fold.
 */

import type { TextStore } from "./buffer";
import type { RowLayout } from "./commands";
import type { HiddenLines } from "./fold/state";
import { SINGLE_ROW, wrapLine, type WrappedLine } from "./wrap";

/** What the layout needs to know about folds (a `FoldState`). */
export interface HiddenSource {
  hidden(): readonly HiddenLines[];
  visibleLine(line: number, dir: -1 | 1, lineCount: number): number | null;
}

export interface VisualOptions {
  wrap: boolean;
  /** Cells per row when wrapping. */
  width: number;
  tabSize: number;
}

export class VisualLayout implements RowLayout {
  private cache = new Map<string, WrappedLine>();
  /** `offsets[i]` is the first visual row of line `i`; one extra entry at the end. */
  private offsets: number[] = [0];
  private options: VisualOptions;
  private folds: HiddenSource | null = null;

  constructor(
    private readonly store: TextStore,
    options: VisualOptions,
  ) {
    this.options = { ...options };
    this.refresh();
  }

  /** Lets `folds` hide lines from now on (`null`: none hidden). */
  setFolds(folds: HiddenSource | null): void {
    this.folds = folds;
    this.refresh();
  }

  /** Applies new options; the cache survives unless wrapping itself changed. */
  setOptions(options: VisualOptions): void {
    const o = this.options;
    if (o.wrap !== options.wrap || o.width !== options.width || o.tabSize !== options.tabSize) {
      this.cache.clear();
    }
    this.options = { ...options };
    this.refresh();
  }

  /** Recomputes the row offsets after the text changed. */
  refresh(): void {
    const count = this.store.lineCount();
    // Edits leave stale entries behind; drop them all once the map has grown
    // far past the document rather than tracking which ones are gone.
    if (this.cache.size > count * 4 + 1024) this.cache.clear();
    const offsets = new Array<number>(count + 1);
    offsets[0] = 0;
    const hidden = this.folds?.hidden() ?? [];
    let next = 0;
    for (let i = 0; i < count; i++) {
      while (next < hidden.length && hidden[next].to < i) next++;
      const folded = next < hidden.length && hidden[next].from <= i;
      offsets[i + 1] = offsets[i] + (folded ? 0 : this.wrapped(i).starts.length);
    }
    this.offsets = offsets;
  }

  /**
   * The nearest line at `line` or past it in `dir` that is not folded away
   * (`RowLayout`): going up a folded line is its fold's header, going down
   * the line after the fold — `null` if the fold reaches the end of the text.
   */
  visibleLine(line: number, dir: -1 | 1): number | null {
    return this.folds ? this.folds.visibleLine(line, dir, this.store.lineCount()) : line;
  }

  wrapped(line: number): WrappedLine {
    if (!this.options.wrap) return SINGLE_ROW;
    const text = this.store.line(line);
    let hit = this.cache.get(text);
    if (!hit) {
      hit = wrapLine(text, Math.max(8, this.options.width), this.options.tabSize);
      this.cache.set(text, hit);
    }
    return hit;
  }

  rowStarts(line: number): readonly number[] {
    return this.wrapped(line).starts;
  }

  get totalRows(): number {
    return this.offsets[this.offsets.length - 1];
  }

  /** The first visual row of `line`. */
  firstRow(line: number): number {
    return this.offsets[line];
  }

  /**
   * The logical line and its sub-row that visual row `row` shows (clamped).
   * Folded lines share their offset with the line after them, so the search
   * — which takes the last line starting at or before the row — never lands
   * on one.
   */
  lineAt(row: number): { line: number; sub: number } {
    const last = this.offsets.length - 2;
    const target = Math.min(Math.max(row, 0), this.totalRows - 1);
    let lo = 0;
    let hi = last;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (this.offsets[mid] <= target) lo = mid;
      else hi = mid - 1;
    }
    return { line: lo, sub: target - this.offsets[lo] };
  }
}
