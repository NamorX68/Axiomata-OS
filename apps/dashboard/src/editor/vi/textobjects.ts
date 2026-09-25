/**
 * Vi's text objects (`docs/plans/editor.md`, ED3, V9): `iw aw iW aW is as ip
 * ap`, the quotes `i" a" i' a' i\` a\``, the brackets `i( a( ib ab i[ a[ i{ a{
 * iB aB i< a<` and the tags `it at`. The tree-sitter objects (`if af ic ac ia
 * aa`) come in ED3.4 and are resolved by the machine, not here.
 *
 * Each returns the stretch it covers — `start` inclusive, `end` exclusive — or
 * `null` if the cursor is not inside one.
 */

import type { TextStore } from "../buffer";
import { comparePos, pos, type Pos } from "../position";
import { matchingBracket } from "./motions";
import { charAt, classAt, isEmptyLine, nextPos, prevPos } from "./scan";

export interface ObjectRange {
  start: Pos;
  /** Exclusive. */
  end: Pos;
  linewise: boolean;
}

const BRACKETS: Record<string, [string, string]> = {
  "(": ["(", ")"],
  ")": ["(", ")"],
  b: ["(", ")"],
  "[": ["[", "]"],
  "]": ["[", "]"],
  "{": ["{", "}"],
  "}": ["{", "}"],
  B: ["{", "}"],
  "<": ["<", ">"],
  ">": ["<", ">"],
};

const QUOTES = new Set(['"', "'", "`"]);

/** The object `name` (`w`, `"`, `(` …) around `at`, `inner` or around (`a`), `count` times. */
export function textObject(
  store: TextStore,
  at: Pos,
  name: string,
  inner: boolean,
  count: number,
): ObjectRange | null {
  if (name === "w" || name === "W") return wordObject(store, at, inner, count, name === "W");
  if (name === "p") return paragraphObject(store, at, inner, count);
  if (name === "s") return sentenceObject(store, at, inner);
  if (QUOTES.has(name)) return quoteObject(store, at, name, inner);
  if (BRACKETS[name]) return bracketObject(store, at, BRACKETS[name], inner, count);
  if (name === "t") return tagObject(store, at, inner, count);
  return null;
}

/** Whether `name` is a text object this module knows (the tree-sitter ones are not). */
export function isClassicObject(name: string): boolean {
  return "wWpst".includes(name) || QUOTES.has(name) || name in BRACKETS;
}

/** The run of one character class around `col` in `line` (word, punctuation or blanks). */
function runAt(store: TextStore, line: number, col: number, big: boolean): { start: number; end: number } {
  const text = store.line(line);
  const cls = classAt(store, pos(line, col), big);
  let start = col;
  while (start > 0 && classAt(store, pos(line, start - 1), big) === cls) start--;
  let end = col;
  while (end < text.length && classAt(store, pos(line, end), big) === cls) end++;
  return { start, end };
}

/**
 * `iw`: the word (or run of blanks, or of punctuation) under the cursor, and
 * each further count the next run. `aw`: a word with the blanks after it — or,
 * when none follow, the blanks before.
 */
function wordObject(store: TextStore, at: Pos, inner: boolean, count: number, big: boolean): ObjectRange | null {
  const text = store.line(at.line);
  if (text.length === 0) return { start: at, end: at, linewise: false };
  const col = Math.min(at.col, text.length - 1);
  let run = runAt(store, at.line, col, big);
  const onBlank = classAt(store, pos(at.line, col), big) === "blank";
  const start = run.start;
  let end = run.end;
  for (let i = 0; i < count - 1 && end < text.length; i++) {
    run = runAt(store, at.line, end, big);
    end = run.end;
  }
  if (inner) return { start: pos(at.line, start), end: pos(at.line, end), linewise: false };
  if (onBlank) {
    // Blanks first, then the word after them.
    if (end < text.length) end = runAt(store, at.line, end, big).end;
    return { start: pos(at.line, start), end: pos(at.line, end), linewise: false };
  }
  if (end < text.length && classAt(store, pos(at.line, end), big) === "blank") {
    return { start: pos(at.line, start), end: pos(at.line, runAt(store, at.line, end, big).end), linewise: false };
  }
  let s = start;
  while (s > 0 && classAt(store, pos(at.line, s - 1), big) === "blank") s--;
  return { start: pos(at.line, s), end: pos(at.line, end), linewise: false };
}

/** `ip`: the lines of this paragraph (or of this run of empty lines); `ap` adds the empty lines after. */
function paragraphObject(store: TextStore, at: Pos, inner: boolean, count: number): ObjectRange {
  const empty = isEmptyLine(store, at.line);
  let first = at.line;
  while (first > 0 && isEmptyLine(store, first - 1) === empty) first--;
  let last = at.line;
  const blocks = inner ? count : count * 2;
  let kind = empty;
  for (let i = 0; i < blocks; i++) {
    while (last + 1 < store.lineCount() && isEmptyLine(store, last + 1) === kind) last++;
    if (i < blocks - 1) {
      if (last + 1 >= store.lineCount()) break;
      last++;
      kind = !kind;
    }
  }
  if (!inner && !empty && (last + 1 >= store.lineCount() || !isEmptyLine(store, last))) {
    // No empty lines after the paragraph: take those before it instead.
    while (first > 0 && isEmptyLine(store, first - 1)) first--;
  }
  return lineRange(store, first, last);
}

/** Lines `first..last` as a linewise range. */
export function lineRange(store: TextStore, first: number, last: number): ObjectRange {
  return { start: pos(first, 0), end: pos(last, store.line(last).length), linewise: true };
}

/** `is`/`as`: the sentence around the cursor — to the next `. ! ?` and blank, within the paragraph. */
function sentenceObject(store: TextStore, at: Pos, inner: boolean): ObjectRange | null {
  const text = store.line(at.line);
  if (text.length === 0) return null;
  // Sentences inside one line are what matters in notes and comments.
  const ends: number[] = [];
  const re = /[.!?]+['")\]]*(?=\s|$)/g;
  for (let m = re.exec(text); m; m = re.exec(text)) ends.push(m.index + m[0].length);
  let start = 0;
  let end = text.length;
  for (const e of ends) {
    if (e <= at.col) start = e;
    else {
      end = e;
      break;
    }
  }
  while (start < text.length && /\s/.test(text[start])) start++;
  if (!inner) {
    while (end < text.length && /\s/.test(text[end])) end++;
  }
  return { start: pos(at.line, start), end: pos(at.line, end), linewise: false };
}

/**
 * `i"`/`a"`: the quoted text on the cursor's line. On a quote, the pair it
 * belongs to (counting from the line's start); inside a pair, that pair; before
 * the first quote, the first pair after the cursor. `a"` takes the blanks after
 * the closing quote too (or, if none, those before the opening one).
 */
function quoteObject(store: TextStore, at: Pos, quote: string, inner: boolean): ObjectRange | null {
  const text = store.line(at.line);
  const quotes: number[] = [];
  for (let i = 0; i < text.length; i++) {
    if (text[i] === quote && (i === 0 || text[i - 1] !== "\\")) quotes.push(i);
  }
  let open = -1;
  let close = -1;
  for (let i = 0; i + 1 < quotes.length; i += 2) {
    if (quotes[i] <= at.col && at.col <= quotes[i + 1]) {
      open = quotes[i];
      close = quotes[i + 1];
      break;
    }
    if (quotes[i] > at.col) {
      open = quotes[i];
      close = quotes[i + 1];
      break;
    }
  }
  if (open < 0) return null;
  if (inner) return { start: pos(at.line, open + 1), end: pos(at.line, close), linewise: false };
  let start = open;
  let end = close + 1;
  if (end < text.length && /[ \t]/.test(text[end])) {
    while (end < text.length && /[ \t]/.test(text[end])) end++;
  } else {
    while (start > 0 && /[ \t]/.test(text[start - 1])) start--;
  }
  return { start: pos(at.line, start), end: pos(at.line, end), linewise: false };
}

/** The `count`-th enclosing `open` bracket before (or at) `at`, skipping nested pairs. */
function enclosingOpen(store: TextStore, at: Pos, [open, close]: [string, string], count: number): Pos | null {
  let q: Pos | null = at;
  // On a closing bracket, its own pair is meant.
  if (charAt(store, at) === close) q = prevPos(store, at);
  else if (charAt(store, at) === open) {
    if (count === 1) return at;
    q = prevPos(store, at);
    count--;
  }
  let depth = 0;
  while (q) {
    const ch = charAt(store, q);
    if (ch === close) depth++;
    else if (ch === open) {
      if (depth === 0 && --count === 0) return q;
      if (depth > 0) depth--;
    }
    q = prevPos(store, q);
  }
  return null;
}

/**
 * `i(`/`a(` and friends, across lines. When the inside starts right after the
 * opening bracket at a line's end and the closing bracket stands alone on its
 * line, `i{` takes the lines in between, whole — as `di{` does in Vim.
 */
function bracketObject(
  store: TextStore,
  at: Pos,
  pair: [string, string],
  inner: boolean,
  count: number,
): ObjectRange | null {
  const openAt = enclosingOpen(store, at, pair, count);
  if (!openAt) return null;
  const closeAt = matchingBracket(store, openAt);
  if (!closeAt || comparePos(closeAt, at) < 0) return null;
  const afterOpen = nextPos(store, openAt)!;
  if (!inner) return { start: openAt, end: nextPos(store, closeAt) ?? closeAt, linewise: false };
  const openLine = store.line(openAt.line);
  const startsLine = afterOpen.col >= openLine.length && openAt.line < closeAt.line;
  const closeLead = store.line(closeAt.line).slice(0, closeAt.col);
  const endsLine = /^[ \t]*$/.test(closeLead) && closeAt.line > openAt.line;
  if (startsLine && endsLine) {
    if (closeAt.line - openAt.line < 2) return { start: afterOpen, end: afterOpen, linewise: false };
    return lineRange(store, openAt.line + 1, closeAt.line - 1);
  }
  return { start: startsLine ? pos(openAt.line + 1, 0) : afterOpen, end: closeAt, linewise: false };
}

/** Offsets of every position in the text, for the tag scan. */
function offsetOf(store: TextStore, p: Pos): number {
  return store.offsetAt(p);
}

function posOfOffset(store: TextStore, offset: number): Pos {
  let line = 0;
  let rest = offset;
  while (line < store.lineCount() - 1 && rest > store.line(line).length) {
    rest -= store.line(line).length + 1;
    line++;
  }
  return pos(line, rest);
}

const TAG = /<(\/?)([A-Za-z][\w:.-]*)[^<>]*?(\/?)>/g;

/** `it`/`at`: the `count`-th XML/HTML element around the cursor. */
function tagObject(store: TextStore, at: Pos, inner: boolean, count: number): ObjectRange | null {
  const text = store.text();
  const cursorAt = offsetOf(store, at);
  const stack: { name: string; start: number; end: number }[] = [];
  const pairs: { openStart: number; openEnd: number; closeStart: number; closeEnd: number }[] = [];
  TAG.lastIndex = 0;
  for (let m = TAG.exec(text); m; m = TAG.exec(text)) {
    if (m[3] === "/") continue;
    if (m[1] === "/") {
      const i = stack.map((t) => t.name).lastIndexOf(m[2]);
      if (i < 0) continue;
      const open = stack[i];
      stack.length = i;
      pairs.push({ openStart: open.start, openEnd: open.end, closeStart: m.index, closeEnd: m.index + m[0].length });
    } else {
      stack.push({ name: m[2], start: m.index, end: m.index + m[0].length });
    }
  }
  const around = pairs
    .filter((p) => p.openStart <= cursorAt && cursorAt < p.closeEnd)
    .sort((a, b) => b.openStart - a.openStart);
  const pair = around[count - 1];
  if (!pair) return null;
  const [s, e] = inner ? [pair.openEnd, pair.closeStart] : [pair.openStart, pair.closeEnd];
  return { start: posOfOffset(store, s), end: posOfOffset(store, e), linewise: false };
}
