/**
 * Walking the text character by character across lines, the way Vi's word,
 * paragraph and bracket motions do (`docs/plans/editor.md`, ED3).
 *
 * Positions here may sit at a line's end (`col === length`) — the line break,
 * which Vi counts as a blank between words. An empty line is a word of its own
 * for `w` and `b`, as in Vim.
 */

import type { TextStore } from "../buffer";
import { pos, type Pos } from "../position";
import { nextGrapheme, prevGrapheme } from "../text";

/** What a character is to a word motion. `word` letters and digits; `punct` the rest. */
export type CharClass = "empty" | "blank" | "word" | "punct";

/** Vim's keyword characters (`iskeyword`): letters, digits, marks and `_`. */
const KEYWORD = /[\p{L}\p{N}\p{M}_]/u;

/**
 * The class of the character at `p`. With `big` (Vi's WORD: `W B E`) every
 * non-blank is one class.
 */
export function classAt(store: TextStore, p: Pos, big = false): CharClass {
  const line = store.line(p.line);
  if (line.length === 0) return "empty";
  if (p.col >= line.length) return "blank";
  const ch = String.fromCodePoint(line.codePointAt(p.col) ?? 32);
  if (ch === " " || ch === "\t") return "blank";
  if (big) return "word";
  return KEYWORD.test(ch) ? "word" : "punct";
}

/** The next position, crossing into the next line after the line break; `null` at the end. */
export function nextPos(store: TextStore, p: Pos): Pos | null {
  const line = store.line(p.line);
  if (p.col < line.length) return pos(p.line, nextGrapheme(line, p.col));
  return p.line + 1 < store.lineCount() ? pos(p.line + 1, 0) : null;
}

/** The previous position, landing on the line break of the line before; `null` at the start. */
export function prevPos(store: TextStore, p: Pos): Pos | null {
  if (p.col > 0) return pos(p.line, prevGrapheme(store.line(p.line), p.col));
  return p.line > 0 ? pos(p.line - 1, store.line(p.line - 1).length) : null;
}

/** The character at `p`, or `"\n"` at a line's end. */
export function charAt(store: TextStore, p: Pos): string {
  const line = store.line(p.line);
  return p.col < line.length ? String.fromCodePoint(line.codePointAt(p.col) ?? 10) : "\n";
}

/** The start of the last grapheme of line `line` — Normal mode's rightmost cursor column. */
export function lastCharCol(store: TextStore, line: number): number {
  const text = store.line(line);
  return text.length === 0 ? 0 : prevGrapheme(text, text.length);
}

/** `p` as Normal mode allows it: on a character, never past the last one. */
export function clampNormal(store: TextStore, p: Pos): Pos {
  const line = Math.max(0, Math.min(p.line, store.lineCount() - 1));
  return pos(line, Math.max(0, Math.min(p.col, lastCharCol(store, line))));
}

/** The first non-blank column of `line` (its length if it is all blank). */
export function firstNonBlank(store: TextStore, line: number): number {
  const text = store.line(line);
  const m = /[^ \t]/.exec(text);
  return m ? m.index : text.length;
}

/** Whether `line` is empty — Vi's paragraph boundary (a line of spaces is not one). */
export function isEmptyLine(store: TextStore, line: number): boolean {
  return store.line(line).length === 0;
}
