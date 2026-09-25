/**
 * What Vi's operators and editing commands do to the text (`docs/plans/editor.md`,
 * ED3, V2, V11): delete, yank, put, shift, change case, re-indent, join, comment
 * and surround — on a `Region` of characters, whole lines, or a column block.
 *
 * Every function makes its changes with one `doc.edit`, so each is one undo
 * step on its own (or joins the open group of a `c` or an Insert session).
 * Changes are listed bottom to top where there are several, so each stays in
 * the coordinates the one before it left.
 */

import { endOfText, type TextStore } from "../buffer";
import { run, type CommandContext } from "../commands";
import type { Change, EditorDocument } from "../document";
import { cursor, pos, range, type Pos } from "../position";
import { colForDisplayColumn, displayColumn, leadingWhitespace, nextGrapheme } from "../text";
import type { RegisterContent } from "./registers";
import { clampNormal, firstNonBlank, lastCharCol } from "./scan";

/** A stretch of text an operator works on. */
export type Region =
  /** Characters from `start` to `end` (exclusive). */
  | { kind: "char"; start: Pos; end: Pos }
  /** Whole lines `first..last`. */
  | { kind: "line"; first: number; last: number }
  /** A block: lines `first..last`, display columns `left..right` (inclusive; `right` Infinity after `$`). */
  | { kind: "block"; first: number; last: number; left: number; right: number };

/** The text a region covers, as a register would hold it. */
export function regionContent(store: TextStore, region: Region, tabSize: number): RegisterContent {
  if (region.kind === "char") return { text: store.slice(range(region.start, region.end)), kind: "char" };
  if (region.kind === "line") {
    const lines: string[] = [];
    for (let l = region.first; l <= region.last; l++) lines.push(store.line(l));
    return { text: `${lines.join("\n")}\n`, kind: "line" };
  }
  const parts: string[] = [];
  for (let l = region.first; l <= region.last; l++) {
    const [s, e] = blockCols(store.line(l), region, tabSize);
    parts.push(store.line(l).slice(s, e));
  }
  return { text: parts.join("\n"), kind: "block" };
}

/** The UTF-16 columns a block covers on one line (`[s, e)`, empty when the line is too short). */
export function blockCols(line: string, region: { left: number; right: number }, tabSize: number): [number, number] {
  const s = colForDisplayColumn(line, region.left, tabSize);
  if (displayColumn(line, s, tabSize) < region.left && s >= line.length) return [line.length, line.length];
  if (region.right === Infinity) return [s, line.length];
  let e = colForDisplayColumn(line, region.right + 1, tabSize);
  if (displayColumn(line, e, tabSize) <= region.right && e < line.length) e = nextGrapheme(line, e);
  return [s, Math.max(s, e)];
}

/**
 * Deletes a region. Lines go with their line break (the last line of the text
 * takes the break before it); the cursor lands where Vim puts it — the start,
 * or the first non-blank of the line that took the deleted lines' place.
 */
export function deleteRegion(doc: EditorDocument, region: Region, ctx: CommandContext): Pos {
  const store = doc.store;
  if (region.kind === "char") {
    doc.edit([{ range: range(region.start, region.end), text: "" }], cursor(region.start), "other", ctx.now);
    return clampNormal(store, region.start);
  }
  if (region.kind === "line") {
    const lastLine = store.lineCount() - 1;
    let r;
    if (region.last < lastLine) r = range(pos(region.first, 0), pos(region.last + 1, 0));
    else if (region.first > 0) r = range(pos(region.first - 1, store.line(region.first - 1).length), endOfText(store));
    else r = range(pos(0, 0), endOfText(store));
    const landingLine = Math.min(region.first, Math.max(0, lastLine - (region.last - region.first + 1)));
    doc.edit([{ range: r, text: "" }], cursor(pos(landingLine, 0)), "other", ctx.now);
    const at = pos(landingLine, firstNonBlank(store, landingLine));
    doc.setSelection(cursor(clampNormal(store, at)));
    return clampNormal(store, at);
  }
  const changes: Change[] = [];
  for (let l = region.last; l >= region.first; l--) {
    const [s, e] = blockCols(store.line(l), region, ctx.tabSize);
    if (e > s) changes.push({ range: range(pos(l, s), pos(l, e)), text: "" });
  }
  const topStart = blockCols(store.line(region.first), region, ctx.tabSize)[0];
  const at = pos(region.first, topStart);
  doc.edit(changes, cursor(at), "other", ctx.now);
  return clampNormal(store, at);
}

/** Where `p`/`P` put register content, and where the cursor goes after it. */
export function putRegister(
  doc: EditorDocument,
  at: Pos,
  content: RegisterContent,
  opts: { after: boolean; count: number; cursorAfter: boolean },
  ctx: CommandContext,
): Pos {
  const store = doc.store;
  const n = Math.max(1, opts.count);
  if (content.kind === "line") {
    const body = content.text.endsWith("\n") ? content.text.slice(0, -1) : content.text;
    const block = Array.from({ length: n }, () => body).join("\n");
    const lines = block.split("\n").length;
    let firstLine: number;
    if (opts.after) {
      const end = pos(at.line, store.line(at.line).length);
      doc.edit([{ range: range(end, end), text: `\n${block}` }], cursor(end), "other", ctx.now);
      firstLine = at.line + 1;
    } else {
      const start = pos(at.line, 0);
      doc.edit([{ range: range(start, start), text: `${block}\n` }], cursor(start), "other", ctx.now);
      firstLine = at.line;
    }
    const target = opts.cursorAfter
      ? pos(Math.min(firstLine + lines, store.lineCount() - 1), 0)
      : pos(firstLine, firstNonBlank(store, firstLine));
    doc.setSelection(cursor(target));
    return target;
  }
  if (content.kind === "block") return putBlock(doc, at, content.text, opts, ctx);
  const text = content.text.repeat(n);
  const line = store.line(at.line);
  const col = opts.after && line.length > 0 ? nextGrapheme(line, at.col) : at.col;
  const where = pos(at.line, col);
  doc.edit([{ range: range(where, where), text }], cursor(where), "other", ctx.now);
  const endLines = text.split("\n");
  const endLine = at.line + endLines.length - 1;
  const endCol = (endLines.length === 1 ? col : 0) + endLines[endLines.length - 1].length;
  let target: Pos;
  if (opts.cursorAfter) target = pos(endLine, endCol);
  else if (text.includes("\n")) target = where;
  else target = pos(endLine, Math.max(col, endCol - 1));
  target = opts.cursorAfter ? target : clampNormal(store, target);
  doc.setSelection(cursor(target));
  return target;
}

/** A block put: each line of the block into the next text line at the same display column. */
function putBlock(
  doc: EditorDocument,
  at: Pos,
  text: string,
  opts: { after: boolean; count: number },
  ctx: CommandContext,
): Pos {
  const store = doc.store;
  const rows = text.split("\n");
  const width = Math.max(...rows.map((r) => displayColumn(r, r.length, ctx.tabSize)));
  const line = store.line(at.line);
  const col = opts.after && line.length > 0 ? nextGrapheme(line, at.col) : at.col;
  const column = displayColumn(line, col, ctx.tabSize);
  const changes: Change[] = [];
  const missing = at.line + rows.length - store.lineCount();
  if (missing > 0) {
    const end = endOfText(store);
    changes.push({ range: range(end, end), text: "\n".repeat(missing) });
  }
  for (let i = rows.length - 1; i >= 0; i--) {
    const l = at.line + i;
    const existing = l < store.lineCount() ? store.line(l) : "";
    const piece = rows[i].padEnd(rows[i].length + (width - displayColumn(rows[i], rows[i].length, ctx.tabSize)));
    const repeated = piece.repeat(Math.max(1, opts.count));
    const c = colForDisplayColumn(existing, column, ctx.tabSize);
    const pad = " ".repeat(Math.max(0, column - displayColumn(existing, existing.length, ctx.tabSize)));
    const insertAt = c >= existing.length ? existing.length : c;
    changes.push({ range: range(pos(l, insertAt), pos(l, insertAt)), text: pad + repeated });
  }
  // The appended line breaks (if any) come first; the rest go bottom to top.
  doc.edit(changes, cursor(pos(at.line, col)), "other", ctx.now);
  const target = pos(at.line, col);
  doc.setSelection(cursor(target));
  return target;
}

/** `>` and `<`: `levels` indentation levels more or less on lines `first..last` (blank lines stay). */
export function shiftLines(
  doc: EditorDocument,
  first: number,
  last: number,
  dir: 1 | -1,
  levels: number,
  ctx: CommandContext,
): void {
  const store = doc.store;
  const unit = doc.indent.kind === "tabs" ? "\t" : " ".repeat(doc.indent.size);
  const changes: Change[] = [];
  for (let l = last; l >= first; l--) {
    const text = store.line(l);
    if (dir > 0) {
      if (text.trim() === "") continue;
      changes.push({ range: range(pos(l, 0), pos(l, 0)), text: unit.repeat(levels) });
    } else {
      const lead = leadingWhitespace(text);
      let remove = 0;
      for (let i = 0; i < levels && remove < lead.length; i++) {
        if (lead[remove] === "\t") remove++;
        else {
          let spaces = 0;
          while (spaces < doc.indent.size && lead[remove + spaces] === " ") spaces++;
          remove += spaces;
          if (spaces === 0) break;
        }
      }
      if (remove > 0) changes.push({ range: range(pos(l, 0), pos(l, remove)), text: "" });
    }
  }
  const at = pos(first, 0);
  doc.edit(changes, cursor(at), "other", ctx.now);
  doc.setSelection(cursor(pos(first, firstNonBlank(store, first))));
}

export type CaseChange = "toggle" | "lower" | "upper";

function changeCase(text: string, how: CaseChange): string {
  if (how === "lower") return text.toLowerCase();
  if (how === "upper") return text.toUpperCase();
  return [...text].map((ch) => (ch === ch.toUpperCase() ? ch.toLowerCase() : ch.toUpperCase())).join("");
}

/** `g~ gu gU` (and Visual `~ u U`): the case of a region. */
export function caseRegion(doc: EditorDocument, region: Region, how: CaseChange, ctx: CommandContext): Pos {
  const store = doc.store;
  const changes: Change[] = [];
  const spans = regionSpans(store, region, ctx.tabSize);
  for (const span of [...spans].reverse()) {
    const text = store.slice(span);
    const changed = changeCase(text, how);
    if (changed !== text) changes.push({ range: span, text: changed });
  }
  const start = spans[0]?.start ?? pos(0, 0);
  doc.edit(changes, cursor(start), "other", ctx.now);
  doc.setSelection(cursor(clampNormal(store, start)));
  return start;
}

/** A region as the character ranges it covers, top to bottom. */
export function regionSpans(store: TextStore, region: Region, tabSize: number) {
  if (region.kind === "char") return [range(region.start, region.end)];
  const spans = [];
  for (let l = region.first; l <= region.last; l++) {
    if (region.kind === "line") spans.push(range(pos(l, 0), pos(l, store.line(l).length)));
    else {
      const [s, e] = blockCols(store.line(l), region, tabSize);
      spans.push(range(pos(l, s), pos(l, e)));
    }
  }
  return spans;
}

/**
 * `=`: re-indents lines from the line above them — one level deeper after a
 * line that opens a bracket (or ends with `:`), one level less for a line that
 * starts by closing one. A heuristic for brace languages, not a formatter.
 */
export function reindentLines(doc: EditorDocument, first: number, last: number, ctx: CommandContext): void {
  const store = doc.store;
  const unit = doc.indent.kind === "tabs" ? "\t" : " ".repeat(doc.indent.size);
  const indentWidth = (s: string) => displayColumn(s, s.length, ctx.tabSize);
  let prev = first - 1;
  while (prev >= 0 && store.line(prev).trim() === "") prev--;
  let level = prev >= 0 ? Math.round(indentWidth(leadingWhitespace(store.line(prev))) / (doc.indent.size || 1)) : 0;
  let opens = prev >= 0 && /[([{:]\s*$/.test(store.line(prev));
  const lines: string[] = [];
  for (let l = first; l <= last; l++) {
    const text = store.line(l).replace(/^[ \t]+/, "");
    if (opens) level++;
    if (/^[)\]}]/.test(text)) level = Math.max(0, level - 1);
    lines.push(text === "" ? "" : unit.repeat(level) + text);
    if (text !== "") opens = /[([{:]\s*$/.test(text);
  }
  const r = range(pos(first, 0), pos(last, store.line(last).length));
  doc.edit([{ range: r, text: lines.join("\n") }], cursor(pos(first, 0)), "other", ctx.now);
  doc.setSelection(cursor(pos(first, firstNonBlank(store, first))));
}

/** `J` (with a space, leading blanks of the next line dropped) and `gJ` (as they are): `count` lines into one. */
export function joinLines(
  doc: EditorDocument,
  first: number,
  count: number,
  spaces: boolean,
  ctx: CommandContext,
): Pos {
  const store = doc.store;
  const last = Math.min(store.lineCount() - 1, first + Math.max(2, count) - 1);
  if (last === first) return doc.selection.head;
  let text = store.line(first);
  let cursorCol = 0;
  for (let l = first + 1; l <= last; l++) {
    const next = store.line(l);
    if (!spaces) {
      cursorCol = text.length;
      text += next;
      continue;
    }
    const trimmed = next.replace(/^[ \t]+/, "");
    const base = text.replace(/[ \t]+$/, "");
    const glue = trimmed === "" || base === "" || /^\)/.test(trimmed) ? "" : " ";
    cursorCol = base.length;
    text = base + glue + trimmed;
  }
  const r = range(pos(first, 0), pos(last, store.line(last).length));
  const at = pos(first, cursorCol);
  doc.edit([{ range: r, text }], cursor(at), "other", ctx.now);
  const landing = clampNormal(store, at);
  doc.setSelection(cursor(landing));
  return landing;
}

/** `gc`: lines `first..last` commented out, or back in — the normal map's ⌘/ (V11). */
export function commentLines(doc: EditorDocument, first: number, last: number, ctx: CommandContext): void {
  const store = doc.store;
  doc.setSelection({ anchor: pos(first, 0), head: pos(last, store.line(last).length) });
  run(doc, { type: "toggleComment" }, ctx);
  doc.setSelection(cursor(pos(first, firstNonBlank(store, first))));
}

/** The two halves a surround character adds: `(` pads with spaces, `)` does not (vim-surround). */
export function surroundPair(ch: string): [string, string] {
  switch (ch) {
    case "(":
      return ["( ", " )"];
    case ")":
    case "b":
      return ["(", ")"];
    case "[":
      return ["[ ", " ]"];
    case "]":
    case "r":
      return ["[", "]"];
    case "{":
      return ["{ ", " }"];
    case "}":
    case "B":
      return ["{", "}"];
    case "<":
    case ">":
    case "a":
      return ["<", ">"];
    default:
      return [ch, ch];
  }
}

/** `ys` and Visual `S`: the region wrapped in `ch`'s pair; a linewise region gets it on lines of its own. */
export function surroundRegion(doc: EditorDocument, region: Region, ch: string, ctx: CommandContext): Pos {
  const store = doc.store;
  const [open, close] = surroundPair(ch);
  if (region.kind === "line") {
    const indent = leadingWhitespace(store.line(region.first));
    const endLine = pos(region.last, store.line(region.last).length);
    const startLine = pos(region.first, 0);
    doc.edit(
      [
        { range: range(endLine, endLine), text: `\n${indent}${close.trim()}` },
        { range: range(startLine, startLine), text: `${indent}${open.trim()}\n` },
      ],
      cursor(startLine),
      "other",
      ctx.now,
    );
    return clampNormal(store, pos(region.first, indent.length));
  }
  if (region.kind === "block") {
    const changes: Change[] = [];
    for (let l = region.last; l >= region.first; l--) {
      const [s, e] = blockCols(store.line(l), region, ctx.tabSize);
      changes.push({ range: range(pos(l, e), pos(l, e)), text: close });
      changes.push({ range: range(pos(l, s), pos(l, s)), text: open });
    }
    const at = pos(region.first, blockCols(store.line(region.first), region, ctx.tabSize)[0]);
    doc.edit(changes, cursor(at), "other", ctx.now);
    return at;
  }
  doc.edit(
    [
      { range: range(region.end, region.end), text: close },
      { range: range(region.start, region.start), text: open },
    ],
    cursor(region.start),
    "other",
    ctx.now,
  );
  doc.setSelection(cursor(region.start));
  return region.start;
}

/** `r{char}` `count` times from `at` — `false` if the line is too short, as in Vim. `\n` splits the line. */
export function replaceChars(doc: EditorDocument, at: Pos, ch: string, count: number, ctx: CommandContext): boolean {
  const store = doc.store;
  const line = store.line(at.line);
  let end = at.col;
  for (let i = 0; i < count; i++) {
    if (end >= line.length) return false;
    end = nextGrapheme(line, end);
  }
  if (ch === "\n") {
    const r = range(at, pos(at.line, end));
    doc.edit([{ range: r, text: "\n" }], cursor(pos(at.line + 1, 0)), "other", ctx.now);
    return true;
  }
  const r = range(at, pos(at.line, end));
  doc.edit([{ range: r, text: ch.repeat(count) }], cursor(at), "other", ctx.now);
  doc.setSelection(cursor(pos(at.line, at.col + ch.length * count - ch.length)));
  return true;
}

/** Visual `r{char}`: every character of the region becomes `ch` (line breaks stay). */
export function replaceRegion(doc: EditorDocument, region: Region, ch: string, ctx: CommandContext): Pos {
  const store = doc.store;
  const spans = regionSpans(store, region, ctx.tabSize);
  const changes: Change[] = [];
  for (const span of [...spans].reverse()) {
    const text = store.slice(span);
    changes.push({ range: span, text: [...text].map((c) => (c === "\n" ? c : ch)).join("") });
  }
  const start = spans[0].start;
  doc.edit(changes, cursor(start), "other", ctx.now);
  doc.setSelection(cursor(clampNormal(store, start)));
  return start;
}

/**
 * Ctrl-a / Ctrl-x: adds `delta` to the number at or after the cursor on its
 * line (decimal, `0x` hex, a leading `-`), keeping the width of a hex number.
 * The cursor lands on its last digit; `false` if the line has no number.
 */
export function incrementNumber(doc: EditorDocument, at: Pos, delta: number, ctx: CommandContext): boolean {
  const line = doc.store.line(at.line);
  const re = /0[xX][0-9a-fA-F]+|-?\d+/g;
  for (let m = re.exec(line); m; m = re.exec(line)) {
    const start = m.index;
    const end = start + m[0].length;
    if (end <= at.col) continue;
    let text: string;
    if (/^0[xX]/.test(m[0])) {
      const value = parseInt(m[0].slice(2), 16) + delta;
      const hex = (value < 0 ? 0 : value).toString(16);
      text = m[0].slice(0, 2) + hex.padStart(m[0].length - 2, "0");
    } else {
      text = String(Number(m[0]) + delta);
    }
    const r = range(pos(at.line, start), pos(at.line, end));
    doc.edit([{ range: r, text }], cursor(pos(at.line, start + text.length - 1)), "other", ctx.now);
    return true;
  }
  return false;
}

/** The line's last character column, or 0 on an empty line — re-exported for the machine. */
export { lastCharCol };
