/**
 * Vi's search (`docs/plans/editor.md`, ED3, V7): a JavaScript regular
 * expression plus the handful of Vim atoms people type without thinking —
 * `\<` `\>` (word edges), `\c` `\C` (ignore / match case) — with smartcase:
 * a pattern in lower case matches either case, one with a capital matches
 * exactly.
 *
 * Matches are found line by line here; a pattern that names a line break
 * (`\n`) is searched over the whole text instead, from the worker's verdict
 * (`search/guard.ts`, ED5, T5). Searching wraps around the end of the file
 * (Vim's `wrapscan`), and says so.
 */

import type { TextStore } from "../buffer";
import { comparePos, pos, range, type Pos, type Range } from "../position";
import { lineMatches } from "../search/matches";

// Shared with the find bar and the worker (ED5, T5); re-exported for Vi's own callers.
export { lineMatches };

/** A pattern ready to search with. */
export interface Compiled {
  /** The pattern as typed, for `"/`, the history and messages. */
  source: string;
  /** Global and Unicode-aware; `lastIndex` is set by whoever uses it. */
  re: RegExp;
}

/** The last search: what `n` and `N` repeat, `:s//…/` reuses and hlsearch shows. */
export interface SearchState {
  pattern: string;
  /** `?` searched backwards; `n` keeps that direction, `N` turns it. */
  backward: boolean;
}

/** A match found by {@link findMatch}, and whether the search went past an end of the file to reach it. */
export interface Found {
  range: Range;
  wrapped: boolean;
}

/** A letter, a digit or `_`: what `\<`, `\>` and `*` count as a word. */
const WORD = "[\\p{L}\\p{N}_]";
const WORD_START = `(?<!${WORD})(?=${WORD})`;
const WORD_END = `(?<=${WORD})(?!${WORD})`;
const WORD_CHAR = new RegExp(`^${WORD}$`, "u");
/** Characters a regex gives a meaning; escaping one of them makes it literal. */
const SYNTAX = /[\\^$.*+?()[\]{}|/]/;

/**
 * Whether `ch` is a word character (letter, digit or `_`) — the same
 * definition `\<`/`\>`/`*` above use, shared with `<C-w>` on the command line
 * and `*`'s "word under the cursor" so all three agree on what a word is.
 */
export function isWordChar(ch: string | undefined): boolean {
  return ch !== undefined && WORD_CHAR.test(ch);
}

/** How case is matched: smartcase (the default), or as `:s`'s `i`/`I` flags force it. */
export type CaseMode = "smart" | "ignore" | "match";

/**
 * Compiles a Vim-flavoured pattern, or returns the error message for a bad
 * one. With `"smart"` a pattern without capitals ignores case; `\c`/`\C`
 * anywhere in the pattern override every mode.
 */
export function compilePattern(pattern: string, mode: CaseMode = "smart"): Compiled | { error: string } {
  let caseless: boolean | null = null;
  let out = "";
  let hasUpper = false;
  let inClass = false;
  for (let i = 0; i < pattern.length; i++) {
    const ch = pattern[i];
    if (ch === "\\" && i + 1 < pattern.length) {
      const next = pattern[i + 1];
      i++;
      if (!inClass && next === "<") out += WORD_START;
      else if (!inClass && next === ">") out += WORD_END;
      else if (next === "c") caseless = true;
      else if (next === "C") caseless = false;
      else if (/[A-Za-z0-9]/.test(next) || SYNTAX.test(next) || (inClass && next === "-")) out += `\\${next}`;
      // `\=`, `\@`, `\<` in a class: a Unicode regex refuses such escapes; Vim means the character.
      else out += next;
      continue;
    }
    if (ch === "[" && !inClass) inClass = true;
    else if (ch === "]" && inClass) inClass = false;
    if (ch !== ch.toLowerCase()) hasUpper = true;
    out += ch;
  }
  const insensitive = caseless ?? (mode === "ignore" || (mode === "smart" && !hasUpper));
  try {
    return { source: pattern, re: new RegExp(out, insensitive ? "giu" : "gu") };
  } catch (err) {
    return { error: err instanceof Error ? err.message : String(err) };
  }
}

/** The pattern `*`/`#` search for: the word itself, as a whole word, matching case exactly. */
export function wordPattern(word: string): string {
  const escaped = word.replace(/[\\^$.*+?()[\]{}|/]/g, "\\$&");
  return /^[\p{L}\p{N}_]/u.test(word) ? `\\<${escaped}\\>\\C` : `${escaped}\\C`;
}

/**
 * The `count`-th match after `from` (before it when `backward`), wrapping
 * around the file's ends. A match *at* `from` does not count — `n` on a match
 * goes to the next one. `null` when the pattern is nowhere in the file.
 */
export function findMatch(store: TextStore, re: RegExp, from: Pos, backward: boolean, count = 1): Found | null {
  let at = from;
  let wrapped = false;
  let found: Range | null = null;
  for (let i = 0; i < Math.max(1, count); i++) {
    const step = backward ? stepBackward(store, re, at) : stepForward(store, re, at);
    if (!step) return null;
    found = step.range;
    wrapped ||= step.wrapped;
    at = step.range.start;
  }
  return found ? { range: found, wrapped } : null;
}

function stepForward(store: TextStore, re: RegExp, from: Pos): Found | null {
  const lines = store.lineCount();
  for (let i = 0; i <= lines; i++) {
    const line = (from.line + i) % lines;
    const wrapped = from.line + i >= lines;
    for (const [s, e] of lineMatches(store.line(line), re)) {
      const start = pos(line, s);
      // The cursor's own line, first time round: only matches after the cursor.
      if (i === 0 && comparePos(start, from) <= 0) continue;
      // Back on the cursor's line after wrapping: only matches up to the cursor.
      if (i === lines && comparePos(start, from) > 0) break;
      return { range: range(start, pos(line, e)), wrapped };
    }
  }
  return null;
}

function stepBackward(store: TextStore, re: RegExp, from: Pos): Found | null {
  const lines = store.lineCount();
  for (let i = 0; i <= lines; i++) {
    const line = (((from.line - i) % lines) + lines) % lines;
    const wrapped = from.line - i < 0;
    const matches = lineMatches(store.line(line), re);
    for (let k = matches.length - 1; k >= 0; k--) {
      const start = pos(line, matches[k][0]);
      if (i === 0 && comparePos(start, from) >= 0) continue;
      if (i === lines && comparePos(start, from) < 0) break;
      return { range: range(start, pos(line, matches[k][1])), wrapped };
    }
  }
  return null;
}

/** Matches on lines `first`–`last` for hlsearch, at most `perLine` on each (a one-letter pattern, a long line). */
export function matchesIn(
  store: TextStore,
  re: RegExp,
  first: number,
  last: number,
  perLine = 200,
): Map<number, Array<[number, number]>> {
  const out = new Map<number, Array<[number, number]>>();
  const end = Math.min(last, store.lineCount() - 1);
  for (let line = Math.max(0, first); line <= end; line++) {
    const found = lineMatches(store.line(line), re, perLine).filter(([s, e]) => e > s);
    if (found.length > 0) out.set(line, found);
  }
  return out;
}
