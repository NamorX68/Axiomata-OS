/**
 * Everything the normal (Mac) key map can do to a document (`docs/plans/editor.md`,
 * ED1, F5/F6). No DOM: a command reads the document and asks it to change.
 *
 * Vertical motion and ⌘←/→ depend on how lines wrap on screen, which only the
 * view knows. They ask a `RowLayout` for the columns where a line's visual rows
 * begin; without wrapping every line is one row (`UNWRAPPED`), and the model
 * tests use exactly that.
 *
 * Clipboard, saving and opening are not in here — they reach outside the model —
 * but the pure halves (`copyText`, `cut`, `paste`) are, so their rules are tested
 * like everything else.
 */

import { endOfText } from "./buffer";
import { endAfter, type Change, type EditKind, type EditorDocument } from "./document";
import {
  addCursorVertical,
  addNextOccurrence,
  allSelections,
  eachCursor,
  eachLineBlock,
  removeLastCursor,
  selectAllOccurrences,
  selectionTexts,
  singleCursor,
} from "./multicursor";
import { comparePos, cursor, isCursor, pos, range, selectionRange, type Pos, type Selection } from "./position";
import {
  colForDisplayColumn,
  displayColumn,
  leadingWhitespace,
  nextGrapheme,
  nextWordEnd,
  prevGrapheme,
  prevWordStart,
} from "./text";

/** Where a line's visual rows start (UTF-16 columns); `[0]` for an unwrapped line. */
export interface RowLayout {
  rowStarts(line: number): readonly number[];
  /**
   * Folding (ED5, T7): the nearest line at `line` or past it in `dir` that is
   * not folded away — `null` past the end. Without it no line is folded.
   */
  visibleLine?(line: number, dir: -1 | 1): number | null;
}

/** `line` if shown, else the nearest shown line in `dir` (`null`: none that way). */
function shownLine(layout: RowLayout, line: number, dir: -1 | 1): number | null {
  return layout.visibleLine ? layout.visibleLine(line, dir) : line;
}

export const UNWRAPPED: RowLayout = { rowStarts: () => [0] };

export interface CommandContext {
  tabSize: number;
  layout: RowLayout;
  /** Visual rows a page holds, for Bild↑/↓. */
  pageRows: number;
  /** Line-comment prefix for ⌘/ (`//`, `#`, …), or `null` if the language has none. */
  commentPrefix: string | null;
  /** Clock for undo grouping; tests pass their own. */
  now?: number;
}

export type Motion =
  | "charLeft"
  | "charRight"
  | "wordLeft"
  | "wordRight"
  | "lineStart"
  | "lineEnd"
  | "up"
  | "down"
  | "pageUp"
  | "pageDown"
  | "docStart"
  | "docEnd";

export type Command =
  | { type: "move"; motion: Motion; extend: boolean }
  | { type: "insert"; text: string; kind?: EditKind }
  | { type: "newline" }
  | { type: "deleteBackward"; unit: "char" | "word" | "line" }
  | { type: "deleteForward"; unit: "char" | "word" }
  | { type: "selectAll" }
  | { type: "selectLine" }
  | { type: "indent" }
  | { type: "outdent" }
  | { type: "moveLines"; dir: -1 | 1 }
  | { type: "duplicateLines"; dir: -1 | 1 }
  | { type: "toggleComment" }
  | { type: "undo" }
  | { type: "redo" }
  // More cursors (ED5, T6, `multicursor.ts`).
  | { type: "addCursorVertical"; dir: -1 | 1 }
  | { type: "addNextOccurrence" }
  | { type: "removeLastCursor" }
  | { type: "selectAllOccurrences" }
  | { type: "singleCursor" };

/** What a command does to the text at each cursor, for merging into the undo step before it. */
function editKindOf(cmd: Command): EditKind | null {
  switch (cmd.type) {
    case "move":
    case "selectLine":
      return null;
    case "insert":
      return cmd.kind ?? "typing";
    case "deleteBackward":
    case "deleteForward":
      return "deleting";
    default:
      return "other";
  }
}

/**
 * Commands that act on whole lines: with several cursors on one line, the
 * line is taken once. Not ⇥ — at a bare cursor it puts indentation there.
 */
const LINE_COMMANDS = new Set<Command["type"]>(["outdent", "duplicateLines", "toggleComment"]);

/**
 * Runs `cmd` against `doc` — at every cursor when there are several (ED5,
 * T6, `multicursor.ts`). ⌘A, undo and redo act on the document as a whole.
 */
export function run(doc: EditorDocument, cmd: Command, ctx: CommandContext): void {
  switch (cmd.type) {
    case "addCursorVertical":
      return addCursorVertical(doc, cmd.dir, ctx.tabSize);
    case "addNextOccurrence":
      return addNextOccurrence(doc);
    case "removeLastCursor":
      return removeLastCursor(doc);
    case "selectAllOccurrences":
      return selectAllOccurrences(doc);
    case "singleCursor":
      singleCursor(doc);
      return;
  }
  if (doc.extra.length > 0 && cmd.type !== "undo" && cmd.type !== "redo") {
    if (cmd.type === "selectAll") return doc.setSelection({ anchor: pos(0, 0), head: endOfText(doc.store) });
    // Moving lines takes each block of touching lines as a whole; copying gives each line its own copy.
    if (cmd.type === "moveLines") return eachLineBlock(doc, () => runOne(doc, cmd, ctx));
    return eachCursor(doc, editKindOf(cmd), () => runOne(doc, cmd, ctx), LINE_COMMANDS.has(cmd.type));
  }
  runOne(doc, cmd, ctx);
}

function runOne(doc: EditorDocument, cmd: Command, ctx: CommandContext): void {
  switch (cmd.type) {
    case "move":
      return move(doc, cmd.motion, cmd.extend, ctx);
    case "insert":
      return insert(doc, cmd.text, cmd.kind ?? "typing", ctx);
    case "newline":
      return newline(doc, ctx);
    case "deleteBackward":
      return deleteBackward(doc, cmd.unit, ctx);
    case "deleteForward":
      return deleteForward(doc, cmd.unit, ctx);
    case "selectAll":
      return doc.setSelection({ anchor: pos(0, 0), head: endOfText(doc.store) });
    case "selectLine":
      return selectLine(doc);
    case "indent":
      return indent(doc, ctx);
    case "outdent":
      return outdent(doc, ctx);
    case "moveLines":
      return moveLines(doc, cmd.dir, ctx);
    case "duplicateLines":
      return duplicateLines(doc, cmd.dir, ctx);
    case "toggleComment":
      return toggleComment(doc, ctx);
    case "undo":
      doc.undo();
      return;
    case "redo":
      doc.redo();
      return;
  }
}

/** The text one indentation level inserts. */
export function indentUnit(doc: EditorDocument): string {
  return doc.indent.kind === "tabs" ? "\t" : " ".repeat(doc.indent.size);
}

// ---------------------------------------------------------------- motion

function move(doc: EditorDocument, motion: Motion, extend: boolean, ctx: CommandContext): void {
  const sel = doc.selection;
  // ←/→ on a selection without ⇧ collapse it to the side they point at.
  if (!extend && !isCursor(sel) && (motion === "charLeft" || motion === "charRight")) {
    const r = selectionRange(sel);
    doc.setSelection(cursor(motion === "charLeft" ? r.start : r.end));
    return;
  }
  const vertical = motion === "up" || motion === "down" || motion === "pageUp" || motion === "pageDown";
  const head = vertical ? verticalTarget(doc, motion, ctx) : target(doc, sel.head, motion, ctx);
  doc.setSelection({ anchor: extend ? sel.anchor : head, head }, vertical);
}

function target(doc: EditorDocument, p: Pos, motion: Motion, ctx: CommandContext): Pos {
  switch (motion) {
    case "charLeft":
      return leftWithinLine(doc, p, prevGrapheme, ctx.layout);
    case "charRight":
      return rightWithinLine(doc, p, nextGrapheme, ctx.layout);
    case "wordLeft":
      return leftWithinLine(doc, p, prevWordStart, ctx.layout);
    case "wordRight":
      return rightWithinLine(doc, p, nextWordEnd, ctx.layout);
    case "lineStart":
      return lineStartTarget(doc, p, ctx);
    case "lineEnd":
      return lineEndTarget(doc, p, ctx);
    case "docStart":
      return pos(0, 0);
    case "docEnd":
      return lastShownEnd(doc, ctx);
    default:
      return p;
  }
}

/**
 * charLeft/wordLeft share the same line-crossing rule: step within the line
 * while there is room, otherwise land at the end of the previous line (or stay
 * put at the very start of the document). `withinLine` picks the grapheme or
 * word boundary that differs between the two motions. A fold is stepped over
 * as a whole: from the line after it to the end of its header.
 */
function leftWithinLine(
  doc: EditorDocument,
  p: Pos,
  withinLine: (line: string, col: number) => number,
  layout: RowLayout,
): Pos {
  if (p.col > 0) return pos(p.line, withinLine(doc.store.line(p.line), p.col));
  const prev = p.line > 0 ? shownLine(layout, p.line - 1, -1) : null;
  return prev === null ? p : lineEndPos(doc, prev);
}

/** charRight/wordRight's mirror of {@link leftWithinLine}. */
function rightWithinLine(
  doc: EditorDocument,
  p: Pos,
  withinLine: (line: string, col: number) => number,
  layout: RowLayout,
): Pos {
  const line = doc.store.line(p.line);
  if (p.col < line.length) return pos(p.line, withinLine(line, p.col));
  const next = p.line < doc.store.lineCount() - 1 ? shownLine(layout, p.line + 1, 1) : null;
  return next === null ? p : pos(next, 0);
}

function lineEndPos(doc: EditorDocument, line: number): Pos {
  return pos(line, doc.store.line(line).length);
}

/**
 * Index of the visual row that holds `col`. A column exactly at a wrap point
 * belongs to the row it starts — the cursor is drawn there, too.
 */
export function rowIndex(starts: readonly number[], col: number): number {
  let row = 0;
  for (let i = 1; i < starts.length; i++) if (starts[i] <= col) row = i;
  return row;
}

/**
 * ⌘←: first the start of the visual row, then — on the first row — the first
 * non-blank character, then column 0, toggling between the two (F6).
 */
function lineStartTarget(doc: EditorDocument, p: Pos, ctx: CommandContext): Pos {
  const starts = ctx.layout.rowStarts(p.line);
  const row = rowIndex(starts, p.col);
  if (row > 0 && p.col !== starts[row]) return pos(p.line, starts[row]);
  const indentEnd = leadingWhitespace(doc.store.line(p.line)).length;
  return pos(p.line, p.col === indentEnd ? 0 : indentEnd);
}

/** ⌘→: first the end of the visual row, then the end of the logical line (F6). */
function lineEndTarget(doc: EditorDocument, p: Pos, ctx: CommandContext): Pos {
  const starts = ctx.layout.rowStarts(p.line);
  const row = rowIndex(starts, p.col);
  const lineLength = doc.store.line(p.line).length;
  if (row < starts.length - 1) {
    const rowEnd = starts[row + 1] - 1;
    if (p.col < rowEnd) return pos(p.line, rowEnd);
  }
  return pos(p.line, lineLength);
}

/** Where one row step in `dir` lands. */
interface RowPosition {
  line: number;
  row: number;
  starts: readonly number[];
}

/**
 * One visual row step in `dir` from `at`, wrapping onto the previous/next
 * line's rows and over folded lines; `null` past the first or last row of the
 * whole document. Up and down are mirror images of each other, which is what
 * {@link verticalTarget}'s loop relies on.
 */
function stepRow(doc: EditorDocument, ctx: CommandContext, dir: -1 | 1, at: RowPosition): RowPosition | null {
  if (dir < 0) {
    if (at.row > 0) return { ...at, row: at.row - 1 };
    const line = at.line === 0 ? null : shownLine(ctx.layout, at.line - 1, -1);
    if (line === null) return null;
    const starts = ctx.layout.rowStarts(line);
    return { line, row: starts.length - 1, starts };
  }
  if (at.row < at.starts.length - 1) return { ...at, row: at.row + 1 };
  const line = at.line === doc.store.lineCount() - 1 ? null : shownLine(ctx.layout, at.line + 1, 1);
  if (line === null) return null;
  return { line, row: 0, starts: ctx.layout.rowStarts(line) };
}

/** Where ↓ past the last row goes: the end of the text, or of the last shown line when a fold hides the end. */
function lastShownEnd(doc: EditorDocument, ctx: CommandContext): Pos {
  const last = doc.store.lineCount() - 1;
  return lineEndPos(doc, shownLine(ctx.layout, last, -1) ?? last);
}

/**
 * ↑/↓ and Bild↑/↓: walk visual rows, keeping the goal display column the first
 * vertical move set. Past the first or last row the cursor goes to the very
 * start or end of the text. Also Vi's `gj`/`gk` (ED3).
 */
export function verticalTarget(doc: EditorDocument, motion: Motion, ctx: CommandContext): Pos {
  const head = doc.selection.head;
  const dir = motion === "up" || motion === "pageUp" ? -1 : 1;
  const steps = motion === "pageUp" || motion === "pageDown" ? Math.max(1, ctx.pageRows) : 1;
  const text = doc.store.line(head.line);
  const starts = ctx.layout.rowStarts(head.line);
  let at: RowPosition = { line: head.line, starts, row: rowIndex(starts, head.col) };
  const goal =
    doc.goalColumn ?? displayColumn(text, head.col, ctx.tabSize) - displayColumn(text, at.starts[at.row], ctx.tabSize);
  for (let i = 0; i < steps; i++) {
    const next = stepRow(doc, ctx, dir, at);
    if (!next) {
      doc.goalColumn = goal;
      return dir < 0 ? pos(0, 0) : lastShownEnd(doc, ctx);
    }
    at = next;
  }
  const lineText = doc.store.line(at.line);
  const rowStart = at.starts[at.row];
  const rowEnd = at.row < at.starts.length - 1 ? at.starts[at.row + 1] - 1 : lineText.length;
  const col = rowStart + colForDisplayColumn(lineText.slice(rowStart, rowEnd), goal, ctx.tabSize);
  doc.goalColumn = goal;
  return pos(at.line, col);
}

// ---------------------------------------------------------------- editing

function insert(doc: EditorDocument, text: string, kind: EditKind, ctx: CommandContext): void {
  const r = selectionRange(doc.selection);
  const normalised = text.replace(/\r\n?/g, "\n");
  doc.edit([{ range: r, text: normalised }], cursor(endAfter(r.start, normalised)), kind, ctx.now);
}

/** ↩: a new line that keeps the current line's indentation (up to the cursor). */
function newline(doc: EditorDocument, ctx: CommandContext): void {
  const r = selectionRange(doc.selection);
  const indentText = leadingWhitespace(doc.store.line(r.start.line)).slice(0, r.start.col);
  insert(doc, "\n" + indentText, "typing", ctx);
}

function deleteRange(doc: EditorDocument, from: Pos, to: Pos, ctx: CommandContext): void {
  const r = range(from, to);
  if (comparePos(r.start, r.end) === 0) return;
  doc.edit([{ range: r, text: "" }], cursor(r.start), "deleting", ctx.now);
}

/**
 * ⌫: a selection goes as a whole. In leading spaces of a space-indented file it
 * removes back to the previous indentation stop, so one ⌫ undoes one ⇥.
 */
function deleteBackward(doc: EditorDocument, unit: "char" | "word" | "line", ctx: CommandContext): void {
  const sel = doc.selection;
  if (!isCursor(sel)) {
    const r = selectionRange(sel);
    return deleteRange(doc, r.start, r.end, ctx);
  }
  const head = sel.head;
  if (unit === "line") {
    return deleteRange(doc, head.col > 0 ? pos(head.line, 0) : target(doc, head, "charLeft", ctx), head, ctx);
  }
  if (unit === "word") return deleteRange(doc, target(doc, head, "wordLeft", ctx), head, ctx);
  const before = doc.store.line(head.line).slice(0, head.col);
  if (doc.indent.kind === "spaces" && head.col > 0 && /^ +$/.test(before)) {
    const size = doc.indent.size;
    const stop = Math.floor((head.col - 1) / size) * size;
    return deleteRange(doc, pos(head.line, stop), head, ctx);
  }
  deleteRange(doc, target(doc, head, "charLeft", ctx), head, ctx);
}

function deleteForward(doc: EditorDocument, unit: "char" | "word", ctx: CommandContext): void {
  const sel = doc.selection;
  if (!isCursor(sel)) {
    const r = selectionRange(sel);
    return deleteRange(doc, r.start, r.end, ctx);
  }
  deleteRange(doc, sel.head, target(doc, sel.head, unit === "word" ? "wordRight" : "charRight", ctx), ctx);
}

/**
 * The lines a line command acts on: every line the selection touches, except a
 * last line the selection merely reaches at column 0 (a ⇧↓ selection of three
 * lines ends at the start of the fourth, which is not meant).
 */
function selectedLines(sel: Selection): { first: number; last: number } {
  const r = selectionRange(sel);
  const last = r.end.line > r.start.line && r.end.col === 0 ? r.end.line - 1 : r.end.line;
  return { first: r.start.line, last };
}

/** ⌘L: the whole line; again, the next line too. */
function selectLine(doc: EditorDocument): void {
  const { first, last } = selectedLines(doc.selection);
  const r = selectionRange(doc.selection);
  const alreadyWhole = r.start.col === 0 && !isCursor(doc.selection) && r.end.col === 0 && r.end.line === last + 1;
  const end = alreadyWhole ? last + 1 : last;
  const head = end + 1 < doc.store.lineCount() ? pos(end + 1, 0) : lineEndPos(doc, end);
  doc.setSelection({ anchor: pos(first, 0), head });
}

/** Moves a position on `line` by `delta` columns if it sits at or after `from`. */
function shiftCol(p: Pos, line: number, from: number, delta: number): Pos {
  return p.line === line && p.col >= from ? pos(line, Math.max(from, p.col + delta)) : p;
}

function shiftSelection(sel: Selection, line: number, from: number, delta: number): Selection {
  return { anchor: shiftCol(sel.anchor, line, from, delta), head: shiftCol(sel.head, line, from, delta) };
}

/**
 * ⇥: with a plain cursor, indentation up to the next stop at the cursor; with a
 * selection, one level more on every selected non-empty line.
 */
function indent(doc: EditorDocument, ctx: CommandContext): void {
  const sel = doc.selection;
  if (isCursor(sel)) {
    const text = doc.store.line(sel.head.line);
    const unit =
      doc.indent.kind === "tabs"
        ? "\t"
        : " ".repeat(doc.indent.size - (displayColumn(text, sel.head.col, ctx.tabSize) % doc.indent.size));
    return insert(doc, unit, "other", ctx);
  }
  const unit = indentUnit(doc);
  const { first, last } = selectedLines(sel);
  const changes: Change[] = [];
  let after = sel;
  for (let line = first; line <= last; line++) {
    if (doc.store.line(line) === "") continue;
    changes.push({ range: range(pos(line, 0), pos(line, 0)), text: unit });
    after = shiftSelection(after, line, 0, unit.length);
  }
  doc.edit(changes, after, "other", ctx.now);
}

/** ⇧⇥: at most one level less on every selected line. */
function outdent(doc: EditorDocument, ctx: CommandContext): void {
  const sel = doc.selection;
  const { first, last } = selectedLines(sel);
  const changes: Change[] = [];
  let after = sel;
  for (let line = first; line <= last; line++) {
    const lead = leadingWhitespace(doc.store.line(line));
    const spaces = /^ */.exec(lead)?.[0].length ?? 0;
    const remove = lead.startsWith("\t") ? 1 : Math.min(spaces, doc.indent.size);
    if (remove === 0) continue;
    changes.push({ range: range(pos(line, 0), pos(line, remove)), text: "" });
    after = shiftSelection(after, line, 0, -remove);
  }
  doc.edit(changes, after, "other", ctx.now);
}

function shiftLines(sel: Selection, delta: number): Selection {
  return { anchor: pos(sel.anchor.line + delta, sel.anchor.col), head: pos(sel.head.line + delta, sel.head.col) };
}

function linesText(doc: EditorDocument, first: number, last: number): string {
  return doc.store.slice(range(pos(first, 0), lineEndPos(doc, last)));
}

/** ⌥↑/↓: the selected lines swap places with the line above or below. */
function moveLines(doc: EditorDocument, dir: -1 | 1, ctx: CommandContext): void {
  const { first, last } = selectedLines(doc.selection);
  const block = linesText(doc, first, last);
  if (dir < 0) {
    if (first === 0) return;
    const above = doc.store.line(first - 1);
    doc.edit(
      [{ range: range(pos(first - 1, 0), lineEndPos(doc, last)), text: `${block}\n${above}` }],
      shiftLines(doc.selection, -1),
      "other",
      ctx.now,
    );
  } else {
    if (last === doc.store.lineCount() - 1) return;
    const below = doc.store.line(last + 1);
    doc.edit(
      [{ range: range(pos(first, 0), lineEndPos(doc, last + 1)), text: `${below}\n${block}` }],
      shiftLines(doc.selection, 1),
      "other",
      ctx.now,
    );
  }
}

/** ⇧⌥↑/↓: a copy of the selected lines above or below; the cursor goes with the copy. */
function duplicateLines(doc: EditorDocument, dir: -1 | 1, ctx: CommandContext): void {
  const { first, last } = selectedLines(doc.selection);
  const block = linesText(doc, first, last);
  const end = lineEndPos(doc, last);
  const after = dir < 0 ? doc.selection : shiftLines(doc.selection, last - first + 1);
  doc.edit([{ range: range(end, end), text: `\n${block}` }], after, "other", ctx.now);
}

/**
 * ⌘/: comments the selected lines out, or back in if every non-blank one is
 * commented. The prefix goes at the smallest indentation among them, so a
 * commented block keeps its shape.
 */
function toggleComment(doc: EditorDocument, ctx: CommandContext): void {
  const prefix = ctx.commentPrefix;
  if (!prefix) return;
  const { first, last } = selectedLines(doc.selection);
  const lines: { line: number; text: string; lead: number }[] = [];
  for (let line = first; line <= last; line++) {
    const text = doc.store.line(line);
    if (text.trim() !== "") lines.push({ line, text, lead: leadingWhitespace(text).length });
  }
  if (lines.length === 0) return;
  const allCommented = lines.every((l) => l.text.slice(l.lead).startsWith(prefix));
  const changes: Change[] = [];
  let after = doc.selection;
  if (allCommented) {
    for (const l of lines) {
      const rest = l.text.slice(l.lead + prefix.length);
      const len = prefix.length + (rest.startsWith(" ") ? 1 : 0);
      changes.push({ range: range(pos(l.line, l.lead), pos(l.line, l.lead + len)), text: "" });
      after = shiftSelection(after, l.line, l.lead, -len);
    }
  } else {
    const at = Math.min(...lines.map((l) => l.lead));
    for (const l of lines) {
      changes.push({ range: range(pos(l.line, at), pos(l.line, at)), text: `${prefix} ` });
      after = shiftSelection(after, l.line, at, prefix.length + 1);
    }
  }
  doc.edit(changes, after, "other", ctx.now);
}

// ---------------------------------------------------------------- clipboard

export interface ClipboardText {
  text: string;
  /** Copied without a selection: the whole line, pasted back as a line. */
  wholeLine: boolean;
  /**
   * Copied from several cursors (ED5, T6): each one's text, in text order —
   * pasted back one per cursor when the count matches.
   */
  parts?: string[];
}

/** ⌘C: the selection, or without one the whole current line (with its break). */
export function copyText(doc: EditorDocument): ClipboardText {
  if (doc.extra.length > 0) {
    const parts = selectionTexts(doc);
    if (parts.some((t) => t !== "")) return { text: parts.join("\n"), wholeLine: false, parts };
    // Only bare cursors: their lines, as one whole-line copy each.
    const lines = [...new Set(allSelections(doc).map((s) => s.head.line))].sort((a, b) => a - b);
    return { text: lines.map((l) => `${doc.store.line(l)}\n`).join(""), wholeLine: true };
  }
  const sel = doc.selection;
  if (!isCursor(sel)) return { text: doc.store.slice(selectionRange(sel)), wholeLine: false };
  return { text: `${doc.store.line(sel.head.line)}\n`, wholeLine: true };
}

/** ⌘X: `copyText`, and then the selection — or the whole line — is gone. */
export function cut(doc: EditorDocument, ctx: CommandContext): ClipboardText {
  const copied = copyText(doc);
  if (doc.extra.length > 0) {
    eachCursor(doc, "other", () => cutOne(doc, copied.wholeLine, ctx), copied.wholeLine);
    return copied;
  }
  cutOne(doc, copied.wholeLine, ctx);
  return copied;
}

/** One cursor's part of ⌘X: its selection, or its whole line. */
function cutOne(doc: EditorDocument, wholeLine: boolean, ctx: CommandContext): void {
  const sel = doc.selection;
  if (!wholeLine) {
    const r = selectionRange(sel);
    if (!isCursor(sel)) doc.edit([{ range: r, text: "" }], cursor(r.start), "other", ctx.now);
    return;
  }
  // The line goes with its break; the cursor lands in the line that takes its
  // place (the next one, or for the last line the one before), same column.
  const line = sel.head.line;
  const isLast = line === doc.store.lineCount() - 1;
  let r = range(pos(line, 0), pos(line + 1, 0));
  let landing = pos(line, Math.min(sel.head.col, isLast ? 0 : doc.store.line(line + 1).length));
  if (isLast) {
    r = line > 0 ? range(lineEndPos(doc, line - 1), lineEndPos(doc, line)) : range(pos(0, 0), lineEndPos(doc, 0));
    landing = line > 0 ? pos(line - 1, Math.min(sel.head.col, doc.store.line(line - 1).length)) : pos(0, 0);
  }
  doc.edit([{ range: r, text: "" }], cursor(landing), "other", ctx.now);
}

/**
 * ⌘V: a whole line copied without a selection goes in above the current line,
 * the cursor keeping its column; anything else replaces the selection.
 */
export function paste(doc: EditorDocument, clip: ClipboardText, ctx: CommandContext): void {
  if (doc.extra.length > 0) return pasteAtEach(doc, clip, ctx);
  pasteOne(doc, clip, ctx);
}

/**
 * ⌘V with several cursors (T6): one piece per cursor, in text order, when
 * the copy had as many pieces (or the text as many lines) as there are
 * cursors; otherwise the whole text at every one.
 */
function pasteAtEach(doc: EditorDocument, clip: ClipboardText, ctx: CommandContext): void {
  const all = allSelections(doc);
  const lines = clip.text.replace(/\n$/, "").split("\n");
  const pieces = clip.parts?.length === all.length ? clip.parts : !clip.wholeLine && lines.length === all.length ? lines : null;
  // Which piece each cursor gets: by its place in the text, not in the adding order.
  const rank = all
    .map((s, i) => ({ i, at: selectionRange(s).start }))
    .sort((a, b) => comparePos(a.at, b.at))
    .reduce((m, { i }, k) => m.set(i, k), new Map<number, number>());
  eachCursor(doc, "other", (i) => {
    const piece = pieces ? pieces[rank.get(i) ?? 0] : null;
    pasteOne(doc, piece === null ? clip : { text: piece, wholeLine: false }, ctx);
  });
}

function pasteOne(doc: EditorDocument, clip: ClipboardText, ctx: CommandContext): void {
  const sel = doc.selection;
  if (clip.wholeLine && isCursor(sel)) {
    const at = pos(sel.head.line, 0);
    const text = clip.text.endsWith("\n") ? clip.text : `${clip.text}\n`;
    const lines = text.split("\n").length - 1;
    doc.edit([{ range: range(at, at), text }], cursor(pos(sel.head.line + lines, sel.head.col)), "other", ctx.now);
    return;
  }
  insert(doc, clip.text, "other", ctx);
}

/** Line-comment prefixes by file extension, for ⌘/. */
const COMMENT_PREFIXES: Record<string, string> = {
  rs: "//",
  ts: "//",
  tsx: "//",
  js: "//",
  jsx: "//",
  mjs: "//",
  cjs: "//",
  svelte: "//",
  c: "//",
  h: "//",
  cpp: "//",
  hpp: "//",
  java: "//",
  kt: "//",
  go: "//",
  swift: "//",
  scss: "//",
  py: "#",
  sh: "#",
  zsh: "#",
  bash: "#",
  toml: "#",
  yaml: "#",
  yml: "#",
  rb: "#",
  sql: "--",
  lua: "--",
};

export function commentPrefixFor(fileName: string): string | null {
  const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
  return COMMENT_PREFIXES[ext] ?? null;
}
