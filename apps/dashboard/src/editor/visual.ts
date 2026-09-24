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
 */

import type { TextStore } from "./buffer";
import type { RowLayout } from "./commands";
import { SINGLE_ROW, wrapLine, type WrappedLine } from "./wrap";

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

  constructor(
    private readonly store: TextStore,
    options: VisualOptions,
  ) {
    this.options = { ...options };
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
    for (let i = 0; i < count; i++) offsets[i + 1] = offsets[i] + this.wrapped(i).starts.length;
    this.offsets = offsets;
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

  /** The logical line and its sub-row that visual row `row` shows (clamped). */
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
