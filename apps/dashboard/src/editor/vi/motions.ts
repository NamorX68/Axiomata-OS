/**
 * Vi's motions (`docs/plans/editor.md`, ED3, D17): where the cursor goes, and
 * how an operator treats the stretch it crosses — exclusive (the end is not
 * included), inclusive, or linewise.
 *
 * A motion is a pure function of the text, the cursor and a little state
 * (`MotionState`: the last `f`/`t`, the marks, the goal column). It returns
 * `null` when it cannot move — `f` finds nothing, `k` on the first line — and
 * an operator is then cancelled, as in Vim.
 */

import { endOfText, type TextStore } from "../buffer";
import { verticalTarget, type CommandContext } from "../commands";
import type { EditorDocument } from "../document";
import type { FoldLookup } from "../fold/state";
import { comparePos, cursor, pos, type Pos } from "../position";
import { colForDisplayColumn, displayColumn, nextGrapheme, prevGrapheme } from "../text";
import {
  charAt,
  classAt,
  clampNormal,
  firstNonBlank,
  isEmptyLine,
  lastCharCol,
  nextPos,
  prevPos,
  type CharClass,
} from "./scan";

/** Everything a motion may need besides the text. */
export interface ViContext extends CommandContext {
  /** The first and last line on screen, for `H M L` and scrolling. */
  viewport: { top: number; bottom: number };
  /** Closed folds (ED5, T18): `j`/`k` step over one as a line, `dd` takes it whole. */
  folds?: FoldLookup;
}

export interface MotionSpec {
  name: string;
  /** The character of `f t F T`, the mark of `' \``. */
  char?: string;
}

export interface MotionResult {
  pos: Pos;
  linewise: boolean;
  inclusive: boolean;
  /** A jump: the position before it goes on the jump list (`''`, Ctrl-o). */
  jump?: boolean;
  /** The display column vertical motions keep; `Infinity` after `$`. */
  goal?: number;
}

/** What `;` and `,` repeat. */
export interface FindState {
  kind: "f" | "F" | "t" | "T";
  char: string;
}

export interface MotionState {
  lastFind: FindState | null;
  /** A mark's position in this document, `null` if unset. */
  mark(name: string): Pos | null;
  /** The display column `j`/`k` aim for (Vim's curswant), `null` to take the cursor's. */
  goal: number | null;
}

const LINEWISE_UP_DOWN = new Set(["j", "k", "<Up>", "<Down>", "<C-p>", "<C-n>"]);

function result(p: Pos, inclusive = false, extra: Partial<MotionResult> = {}): MotionResult {
  return { pos: p, linewise: false, inclusive, ...extra };
}

function lineResult(line: number, col: number, extra: Partial<MotionResult> = {}): MotionResult {
  return { pos: pos(line, col), linewise: true, inclusive: false, ...extra };
}

/**
 * Where `spec` takes the cursor from `from`, `count` times (`null` = no count
 * typed; some motions treat that differently from 1, like `G`).
 */
export function motion(
  doc: EditorDocument,
  from: Pos,
  spec: MotionSpec,
  count: number | null,
  ctx: ViContext,
  state: MotionState,
): MotionResult | null {
  const store = doc.store;
  const n = count ?? 1;
  switch (spec.name) {
    case "h":
    case "<Left>":
      return from.col === 0 ? null : result(pos(from.line, stepCols(store, from, -n)));
    case "l":
    case "<Right>":
      return charRight(store, from, n);
    case "<BS>":
      return wrapStep(store, from, -n);
    case " ":
      return wrapStep(store, from, n);
    case "j":
    case "<Down>":
    case "<C-n>":
      return vertical(store, from, n, ctx, state);
    case "k":
    case "<Up>":
    case "<C-p>":
      return vertical(store, from, -n, ctx, state);
    case "gj":
    case "gk":
      return displayRows(doc, from, spec.name === "gj" ? n : -n, ctx);
    case "w":
    case "W":
      return result(repeat(n, from, (p) => wordForward(store, p, spec.name === "W")));
    case "b":
    case "B":
      return result(repeat(n, from, (p) => wordBackward(store, p, spec.name === "B")));
    case "e":
    case "E":
      return result(repeat(n, from, (p) => wordEnd(store, p, spec.name === "E")), true);
    case "ge":
    case "gE":
      return result(repeat(n, from, (p) => wordEndBackward(store, p, spec.name === "gE")), true);
    case "0":
    case "<Home>":
      return result(pos(from.line, 0));
    case "^":
      return result(pos(from.line, firstNonBlank(store, from.line)));
    case "$":
    case "<End>": {
      const line = Math.min(store.lineCount() - 1, from.line + n - 1);
      return result(pos(line, lastCharCol(store, line)), true, { goal: Infinity });
    }
    case "g_": {
      const line = Math.min(store.lineCount() - 1, from.line + n - 1);
      const text = store.line(line);
      const m = /[^ \t][ \t]*$/.exec(text);
      return result(pos(line, m ? m.index : 0), true);
    }
    case "|": {
      const text = store.line(from.line);
      return result(pos(from.line, colForDisplayColumn(text, n - 1, ctx.tabSize)));
    }
    case "gg":
      return lineResult(clampLine(store, (count ?? 1) - 1), -1, { jump: true });
    case "G":
      // A bare `G` goes to the last shown line; `12G` names a line, and opens a fold hiding it.
      if (count === null) return lineResult(shown(ctx, store.lineCount() - 1), -1, { jump: true });
      return lineResult(clampLine(store, count - 1), -1, { jump: true });
    case "+":
    case "<CR>":
    case "<C-m>":
      return from.line + n < store.lineCount() ? lineResult(from.line + n, -1) : null;
    case "-":
      return from.line - n >= 0 ? lineResult(from.line - n, -1) : null;
    case "_":
      return lineResult(clampLine(store, from.line + n - 1), -1);
    case "H":
      return lineResult(shownFrom(store, ctx, ctx.viewport.top, n - 1, ctx.viewport.bottom), -1, { jump: true });
    case "L":
      return lineResult(shownFrom(store, ctx, ctx.viewport.bottom, 1 - n, ctx.viewport.top), -1, { jump: true });
    case "M": {
      const middle = shown(ctx, clampLine(store, Math.floor((ctx.viewport.top + ctx.viewport.bottom) / 2)));
      return lineResult(middle, -1, { jump: true });
    }
    case "}":
      return result(repeat(n, from, (p) => paragraphForward(store, p)), false, { jump: true });
    case "{":
      return result(repeat(n, from, (p) => paragraphBackward(store, p)), false, { jump: true });
    case ")":
      return result(repeat(n, from, (p) => sentenceForward(store, p)), false, { jump: true });
    case "(":
      return result(repeat(n, from, (p) => sentenceBackward(store, p)), false, { jump: true });
    case "%":
      return count !== null ? percentLine(store, count) : matchBracket(store, from);
    case "f":
    case "F":
    case "t":
    case "T":
      return spec.char ? find(store, from, { kind: spec.name, char: spec.char }, n, false) : null;
    case ";":
    case ",":
      return state.lastFind ? find(store, from, repeatedFind(state.lastFind, spec.name === ","), n, true) : null;
    case "'":
    case "`": {
      const at = spec.char ? state.mark(spec.char) : null;
      if (!at) return null;
      const target = clampNormal(store, at);
      return spec.name === "'" ? lineResult(target.line, -1, { jump: true }) : result(target, false, { jump: true });
    }
    default:
      return null;
  }
}

/** Whether a motion moves by lines for an operator (`dj` deletes two lines). */
export function isLinewiseMotion(name: string): boolean {
  return LINEWISE_UP_DOWN.has(name);
}

function repeat(n: number, from: Pos, step: (p: Pos) => Pos): Pos {
  let p = from;
  for (let i = 0; i < n; i++) p = step(p);
  return p;
}

function clampLine(store: TextStore, line: number): number {
  return Math.max(0, Math.min(line, store.lineCount() - 1));
}

/** `n` graphemes left (negative) or right within the line, stopping at its edges. */
function stepCols(store: TextStore, from: Pos, n: number): number {
  const text = store.line(from.line);
  let col = from.col;
  for (let i = 0; i < Math.abs(n); i++) {
    const next = n < 0 ? prevGrapheme(text, col) : nextGrapheme(text, col);
    if (next === col) break;
    col = next;
  }
  return col;
}

/**
 * `l`: right within the line. In Normal mode it stops on the last character;
 * an operator may reach the line's end (`dl` on the last character), so the
 * result can be the line length and the caller clamps it for a plain move.
 */
function charRight(store: TextStore, from: Pos, n: number): MotionResult | null {
  const text = store.line(from.line);
  if (from.col >= text.length) return null;
  return result(pos(from.line, stepCols(store, from, n)));
}

/** `<BS>` and `<Space>`: like `h`/`l`, but on into the neighbouring line. */
function wrapStep(store: TextStore, from: Pos, n: number): MotionResult | null {
  let p = from;
  for (let i = 0; i < Math.abs(n); i++) {
    const next = n < 0 ? prevPos(store, p) : nextPos(store, p);
    if (!next) break;
    p = next;
    // The line break itself is not a place the cursor stops on.
    const len = store.line(p.line).length;
    if (p.col >= len && len > 0) p = n < 0 ? pos(p.line, lastCharCol(store, p.line)) : (nextPos(store, p) ?? p);
  }
  return comparePos(p, from) === 0 ? null : result(p);
}

/** `j`/`k`: whole lines, keeping the goal display column (`$` keeps the end). */
function vertical(store: TextStore, from: Pos, n: number, ctx: ViContext, state: MotionState): MotionResult | null {
  const line = ctx.folds ? foldedStep(store, from.line, n, ctx.folds) : stepLines(store, from.line, n);
  if (line === null) return null;
  const goal = state.goal ?? displayColumn(store.line(from.line), from.col, ctx.tabSize);
  const text = store.line(line);
  const col = goal === Infinity ? lastCharCol(store, line) : colForDisplayColumn(text, goal, ctx.tabSize);
  return { pos: pos(line, Math.min(col, lastCharCol(store, line))), linewise: true, inclusive: false, goal };
}

/** `line`, or the header of the closed fold hiding it (ED5, T18): a motion never lands in a fold. */
export function shown(ctx: ViContext, line: number): number {
  return ctx.folds?.closedAround(line)?.start ?? line;
}

/** `H`/`L`: `n` shown lines from the screen edge `from` (a fold counts as one), not past `limit`. */
function shownFrom(store: TextStore, ctx: ViContext, from: number, n: number, limit: number): number {
  const start = shown(ctx, clampLine(store, from));
  if (n === 0) return start;
  const line = ctx.folds ? (foldedStep(store, start, n, ctx.folds) ?? start) : clampLine(store, start + n);
  return n > 0 ? Math.min(line, shown(ctx, limit)) : Math.max(line, limit);
}

/** `n` lines from `from` (clamped with a count; `null` when a single step would leave the text, as in Vim). */
function stepLines(store: TextStore, from: number, n: number): number | null {
  const target = from + n;
  if ((target < 0 || target >= store.lineCount()) && Math.abs(n) === 1) return null;
  return clampLine(store, target);
}

/** {@link stepLines}, with a closed fold counting as one line — landing on its header (T18). */
function foldedStep(store: TextStore, from: number, n: number, folds: FoldLookup): number | null {
  const dir = n < 0 ? -1 : 1;
  let line = folds.closedAround(from)?.start ?? from;
  for (let i = 0; i < Math.abs(n); i++) {
    const edge = dir > 0 ? (folds.closedAround(line)?.end ?? line) : line;
    const next = edge + dir;
    if (next < 0 || next >= store.lineCount()) {
      if (Math.abs(n) === 1) return null;
      break;
    }
    line = folds.closedAround(next)?.start ?? next;
  }
  return line;
}

/**
 * `gj`/`gk`: screen rows of wrapped lines, through the normal key map's row
 * walk — which moves the document's selection and goal column, so both are put
 * back afterwards; the machine sets the cursor itself.
 */
function displayRows(doc: EditorDocument, from: Pos, n: number, ctx: ViContext): MotionResult {
  const saved = doc.selection;
  const savedGoal = doc.goalColumn;
  doc.setSelection(cursor(from), true);
  let p = from;
  for (let i = 0; i < Math.abs(n); i++) {
    p = verticalTarget(doc, n < 0 ? "up" : "down", ctx);
    doc.setSelection(cursor(p), true);
  }
  doc.setSelection(saved, true);
  doc.goalColumn = savedGoal;
  return result(clampNormal(doc.store, p));
}

/** `w`: the start of the next word (an empty line counts as one). */
export function wordForward(store: TextStore, p: Pos, big: boolean): Pos {
  const start = classAt(store, p, big);
  let q: Pos | null = p;
  if (start === "word" || start === "punct") {
    while (q && classAt(store, q, big) === start) q = nextPos(store, q);
  } else if (start === "empty") {
    q = nextPos(store, q);
  }
  while (q && classAt(store, q, big) === "blank") q = nextPos(store, q);
  return q ?? endOfText(store);
}

/** `b`: the start of this or the previous word. */
export function wordBackward(store: TextStore, p: Pos, big: boolean): Pos {
  let q = prevPos(store, p);
  while (q && classAt(store, q, big) === "blank") q = prevPos(store, q);
  if (!q) return pos(0, 0);
  const cls = classAt(store, q, big);
  if (cls === "empty") return q;
  let prev = prevPos(store, q);
  while (prev && prev.line === q.line && classAt(store, prev, big) === cls) {
    q = prev;
    prev = prevPos(store, q);
  }
  return q;
}

/** `e`: the end of this or the next word. */
export function wordEnd(store: TextStore, p: Pos, big: boolean): Pos {
  let q = nextPos(store, p);
  while (q && isBlankOrEmpty(classAt(store, q, big))) q = nextPos(store, q);
  if (!q) return clampNormal(store, endOfText(store));
  const cls = classAt(store, q, big);
  let next = nextPos(store, q);
  while (next && next.line === q.line && classAt(store, next, big) === cls) {
    q = next;
    next = nextPos(store, q);
  }
  return q;
}

/** `ge`: the end of the previous word. */
export function wordEndBackward(store: TextStore, p: Pos, big: boolean): Pos {
  const cls = classAt(store, p, big);
  let q: Pos | null = p;
  if (cls === "word" || cls === "punct") {
    while (q && q.line === p.line && classAt(store, q, big) === cls) q = prevPos(store, q);
  } else {
    q = prevPos(store, q);
  }
  while (q && classAt(store, q, big) === "blank") q = prevPos(store, q);
  return q ?? pos(0, 0);
}

function isBlankOrEmpty(cls: CharClass): boolean {
  return cls === "blank" || cls === "empty";
}

/** `}`: the next empty line after this paragraph, or the end of the text. */
function paragraphForward(store: TextStore, p: Pos): Pos {
  let line = p.line;
  while (line < store.lineCount() - 1 && isEmptyLine(store, line)) line++;
  while (line < store.lineCount() - 1 && !isEmptyLine(store, line)) line++;
  return isEmptyLine(store, line) ? pos(line, 0) : endOfText(store);
}

/** `{`: the empty line before this paragraph, or the start of the text. */
function paragraphBackward(store: TextStore, p: Pos): Pos {
  let line = p.line;
  while (line > 0 && isEmptyLine(store, line)) line--;
  while (line > 0 && !isEmptyLine(store, line)) line--;
  return pos(line, 0);
}

/** Whether a sentence starts at `p`: after `. ! ?` (and closing quotes/brackets) and blanks, or a paragraph. */
function isSentenceStart(store: TextStore, p: Pos): boolean {
  const cls = classAt(store, p);
  if (cls === "empty") return true;
  if (cls === "blank") return false;
  let q = prevPos(store, p);
  if (!q) return true;
  let sawBlank = false;
  while (q && (classAt(store, q) === "blank" || classAt(store, q) === "empty")) {
    if (classAt(store, q) === "empty") return true;
    sawBlank = true;
    q = prevPos(store, q);
  }
  if (!q) return true;
  if (!sawBlank) return false;
  let ch = charAt(store, q);
  while (q && /[)\]"']/.test(ch)) {
    q = prevPos(store, q);
    ch = q ? charAt(store, q) : "";
  }
  return /[.!?]/.test(ch);
}

function sentenceForward(store: TextStore, p: Pos): Pos {
  let q = nextPos(store, p);
  while (q) {
    const len = store.line(q.line).length;
    if (q.col < len || len === 0) {
      if (isSentenceStart(store, q)) return q;
    }
    q = nextPos(store, q);
  }
  return endOfText(store);
}

function sentenceBackward(store: TextStore, p: Pos): Pos {
  let q = prevPos(store, p);
  while (q) {
    const len = store.line(q.line).length;
    if ((q.col < len || len === 0) && isSentenceStart(store, q)) return q;
    q = prevPos(store, q);
  }
  return pos(0, 0);
}

/** `N%`: the line `N` percent into the text. */
function percentLine(store: TextStore, percent: number): MotionResult | null {
  if (percent > 100) return null;
  return lineResult(clampLine(store, Math.ceil((percent * store.lineCount()) / 100) - 1), -1, { jump: true });
}

const PAIRS: Record<string, { mate: string; dir: 1 | -1 }> = {
  "(": { mate: ")", dir: 1 },
  "[": { mate: "]", dir: 1 },
  "{": { mate: "}", dir: 1 },
  ")": { mate: "(", dir: -1 },
  "]": { mate: "[", dir: -1 },
  "}": { mate: "{", dir: -1 },
};

/** `%`: the first bracket at or after the cursor on this line, and its partner. */
function matchBracket(store: TextStore, from: Pos): MotionResult | null {
  const text = store.line(from.line);
  let col = from.col;
  while (col < text.length && !PAIRS[text[col]]) col++;
  if (col >= text.length) return null;
  const at = matchingBracket(store, pos(from.line, col));
  return at ? result(at, true, { jump: true }) : null;
}

/** The bracket matching the one at `p`, counting nested pairs of the same kind. */
export function matchingBracket(store: TextStore, p: Pos): Pos | null {
  const open = charAt(store, p);
  const pair = PAIRS[open];
  if (!pair) return null;
  let depth = 0;
  let q: Pos | null = p;
  while (q) {
    const ch = charAt(store, q);
    if (ch === open) depth++;
    else if (ch === pair.mate && --depth === 0) return q;
    q = pair.dir > 0 ? nextPos(store, q) : prevPos(store, q);
  }
  return null;
}

/** `;` repeats the last find; `,` repeats it the other way. */
function repeatedFind(last: FindState, reverse: boolean): FindState {
  if (!reverse) return last;
  const flip: Record<FindState["kind"], FindState["kind"]> = { f: "F", F: "f", t: "T", T: "t" };
  return { kind: flip[last.kind], char: last.char };
}

/**
 * `f t F T` within the line: the `n`th `char` forward or back; `t`/`T` stop
 * one before it. Repeated with `;`, a `t` that already stands right before its
 * character looks past it (`repeat`), or it would never move.
 */
function find(store: TextStore, from: Pos, f: FindState, n: number, repeat: boolean): MotionResult | null {
  const text = store.line(from.line);
  const forward = f.kind === "f" || f.kind === "t";
  const till = f.kind === "t" || f.kind === "T";
  let col = from.col;
  if (repeat && till) col += forward ? 1 : -1;
  for (let i = 0; i < n; i++) {
    col = forward ? text.indexOf(f.char, col + 1) : text.lastIndexOf(f.char, col - 1);
    if (col < 0) return null;
  }
  if (till) col += forward ? -1 : 1;
  if (!forward && till && col > from.col) return null;
  return result(pos(from.line, col), forward);
}

/** Whether a motion counts as a jump (for the jump list). */
export function isJump(result: MotionResult | null): boolean {
  return result?.jump === true;
}

/** The first non-blank of `line` — where linewise motions put the cursor (`-1` in a result means this). */
export function resolveLineCol(store: TextStore, p: Pos): Pos {
  return p.col < 0 ? pos(p.line, firstNonBlank(store, p.line)) : p;
}

/** The Normal-mode cursor after a motion: on a character, linewise targets at the first non-blank. */
export function landing(store: TextStore, r: MotionResult): Pos {
  return clampNormal(store, resolveLineCol(store, r.pos));
}
