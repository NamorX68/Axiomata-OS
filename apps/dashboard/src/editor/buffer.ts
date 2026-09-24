/**
 * The text store (`docs/plans/editor.md`, ED1, F1).
 *
 * Everything else in the editor talks to text through `TextStore` only — five
 * operations, nothing about how lines are kept. `LineStore` keeps them as a plain
 * array of strings, which is easy to reason about and fast enough for the 2 MiB
 * the editor edits (and the 16 MiB it shows read-only). A rope replaces it as the
 * first checkpoint of ED5, behind this same interface; `buffer.test.ts` is written
 * against the interface so it can run against both.
 *
 * Lines never contain a line break. The document's line ending (LF or CRLF) is
 * metadata of the document (`detect.ts`), applied only when the text is joined
 * for saving; inside the store every break is a line boundary.
 */

import { comparePos, pos, type Pos, type Range } from "./position";

export interface TextStore {
  /** Always at least 1: an empty document is one empty line. */
  lineCount(): number;
  /** The text of line `i`, without its line break. */
  line(i: number): string;
  /** The text in `r`, lines joined with `\n`. */
  slice(r: Range): string;
  /**
   * Replaces the text in `r` with `text` (which may contain `\n`) and returns the
   * position right after the inserted text.
   */
  replace(r: Range, text: string): Pos;
  /** The whole text, lines joined with `\n`. */
  text(): string;
  /** `p` as an offset into `text()` (UTF-16 units, each line break one unit). */
  offsetAt(p: Pos): number;
}

/** Splits text into lines on LF, CRLF or a lone CR. */
export function splitLines(text: string): string[] {
  return text.split(/\r\n|\r|\n/);
}

/** A `TextStore` over an array of lines. */
export class LineStore implements TextStore {
  private lines: string[];

  constructor(text = "") {
    this.lines = splitLines(text);
  }

  lineCount(): number {
    return this.lines.length;
  }

  line(i: number): string {
    const line = this.lines[i];
    if (line === undefined) throw new RangeError(`line ${i} out of 0..${this.lines.length - 1}`);
    return line;
  }

  slice(r: Range): string {
    const { start, end } = this.checked(r);
    if (start.line === end.line) return this.lines[start.line].slice(start.col, end.col);
    const parts = [this.lines[start.line].slice(start.col)];
    for (let i = start.line + 1; i < end.line; i++) parts.push(this.lines[i]);
    parts.push(this.lines[end.line].slice(0, end.col));
    return parts.join("\n");
  }

  replace(r: Range, text: string): Pos {
    const { start, end } = this.checked(r);
    const before = this.lines[start.line].slice(0, start.col);
    const after = this.lines[end.line].slice(end.col);
    const inserted = splitLines(text);
    const last = inserted.length - 1;
    const endCol = (last === 0 ? before.length : 0) + inserted[last].length;
    inserted[0] = before + inserted[0];
    inserted[last] += after;
    this.lines.splice(start.line, end.line - start.line + 1, ...inserted);
    return pos(start.line + last, endCol);
  }

  text(): string {
    return this.lines.join("\n");
  }

  /** Linear in the line number — a rope (ED5) answers this in log time. */
  offsetAt(p: Pos): number {
    let offset = 0;
    for (let i = 0; i < p.line; i++) offset += this.lines[i].length + 1;
    return offset + p.col;
  }

  /** Throws on a range that is reversed or points outside the text. */
  private checked(r: Range): Range {
    if (comparePos(r.start, r.end) > 0) throw new RangeError("reversed range");
    for (const p of [r.start, r.end]) {
      const line = this.lines[p.line];
      if (line === undefined || p.col < 0 || p.col > line.length) {
        throw new RangeError(`position ${p.line}:${p.col} outside the text`);
      }
    }
    return r;
  }
}

/** The position after the last character. */
export function endOfText(store: TextStore): Pos {
  const last = store.lineCount() - 1;
  return pos(last, store.line(last).length);
}

/** `p` moved into the text: line and column clamped to what exists. */
export function clampPos(store: TextStore, p: Pos): Pos {
  const line = Math.min(Math.max(p.line, 0), store.lineCount() - 1);
  const col = Math.min(Math.max(p.col, 0), store.line(line).length);
  return pos(line, col);
}
