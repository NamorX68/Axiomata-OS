/**
 * Replacing a document's text with a new version as small changes
 * (`docs/plans/editor.md`, ED6.5): a formatter's output, or a language
 * server's edits (formatting, a rename), become the lines that actually
 * differ — so undo, folds and the cursor keep their place instead of the
 * whole text being swapped.
 *
 * Positions are lines and UTF-16 columns, as everywhere in the engine; text
 * is `\n`-separated (the store's form).
 */

import { diffSequences } from "./diff/myers";
import { endAfter, type Change } from "./document";
import { comparePos, pos, range, type Pos, type Range } from "./position";

/** One of a server's text edits: a range of the text it was given, and what goes there. */
export interface TextEdit {
  range: Range;
  text: string;
}

/** `text` with `edits` applied — all in the coordinates of `text`, as the protocol states them. */
export function applyTextEdits(text: string, edits: readonly TextEdit[]): string {
  const starts = lineStarts(text);
  const offset = (p: Pos) => {
    const line = Math.min(p.line, starts.length - 1);
    const lineEnd = line + 1 < starts.length ? starts[line + 1] - 1 : text.length;
    return p.line >= starts.length ? text.length : Math.min(starts[line] + p.col, lineEnd);
  };
  // Last first, so every earlier range is still where it was; equal starts keep their order.
  const ordered = edits
    .map((e, i) => ({ e, i, from: offset(e.range.start), to: offset(e.range.end) }))
    .sort((a, b) => b.from - a.from || b.i - a.i);
  let out = text;
  for (const { e, from, to } of ordered) out = out.slice(0, from) + e.text.replace(/\r\n?/g, "\n") + out.slice(to);
  return out;
}

function lineStarts(text: string): number[] {
  const starts = [0];
  for (let i = 0; i < text.length; i++) if (text[i] === "\n") starts.push(i + 1);
  return starts;
}

/**
 * The changes that turn `before` into `after`, one per run of differing lines,
 * each in the coordinates of `before` (they do not overlap).
 */
export function textChanges(before: string, after: string): Change[] {
  const a = before.split("\n");
  const b = after.split("\n");
  const changes: Change[] = [];
  let i = 0;
  let j = 0;
  const ops = diffSequences(a, b);
  for (let k = 0; k < ops.length; k++) {
    const op = ops[k];
    if (op.kind === "equal") {
      i += op.count;
      j += op.count;
      continue;
    }
    // A run of removals and insertions (in either order) is one change.
    let removed = 0;
    let added = 0;
    while (k < ops.length && ops[k].kind !== "equal") {
      if (ops[k].kind === "delete") removed += ops[k].count;
      else added += ops[k].count;
      k++;
    }
    k--;
    changes.push(linesChange(a, i, removed, b.slice(j, j + added)));
    i += removed;
    j += added;
  }
  return changes;
}

/** Replacing `count` lines of `lines` from `start` with `inserted`, as a change in character positions. */
function linesChange(lines: readonly string[], start: number, count: number, inserted: readonly string[]): Change {
  const last = lines.length - 1;
  const endOf = (line: number) => pos(line, lines[line].length);
  if (count > 0 && inserted.length > 0) {
    const end = start + count - 1;
    return { range: range(pos(start, 0), pos(end, lines[end].length)), text: inserted.join("\n") };
  }
  if (count > 0) {
    // Whole lines go, with one of the line breaks around them.
    if (start + count <= last) return { range: range(pos(start, 0), pos(start + count, 0)), text: "" };
    if (start > 0) return { range: range(endOf(start - 1), endOf(last)), text: "" };
    return { range: range(pos(0, 0), endOf(last)), text: "" };
  }
  if (start <= last) return { range: range(pos(start, 0), pos(start, 0)), text: `${inserted.join("\n")}\n` };
  return { range: range(endOf(last), endOf(last)), text: `\n${inserted.join("\n")}` };
}

/**
 * Where `p` is once `changes` (non-overlapping, in the coordinates `p` is in)
 * are applied. A position inside a changed stretch keeps its line within it
 * where it can, its column clamped by the caller's text.
 */
export function mapPosition(p: Pos, changes: readonly Change[]): Pos {
  let line = p.line;
  let col = p.col;
  // Nearest first: each is applied in coordinates the ones after it have not touched.
  const before = changes
    .filter((c) => comparePos(c.range.start, p) <= 0)
    .sort((x, y) => comparePos(y.range.start, x.range.start));
  for (const c of before) {
    const newEnd = endAfter(c.range.start, c.text);
    const at = pos(line, col);
    if (comparePos(c.range.end, at) <= 0) {
      if (line === c.range.end.line) {
        col = newEnd.col + (col - c.range.end.col);
        line = newEnd.line;
      } else line += newEnd.line - c.range.end.line;
    } else {
      // Inside the change: the same line of it (as far as it reaches), same column.
      line = Math.min(line, newEnd.line);
    }
  }
  return pos(line, col);
}

/**
 * A formatter's output as the text the store holds: line endings to `\n`, and
 * one final line break dropped — the store keeps that as the file's shape,
 * never as an empty last line.
 */
export function storeBody(output: string): string {
  return output.replace(/\r\n?/g, "\n").replace(/\n$/, "");
}
