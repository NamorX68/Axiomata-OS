/**
 * Several cursors at once (`docs/plans/editor.md`, ED5, T6): running a
 * command at every one of them, and the ways to get more — ⌥-click, ⌥⌘↑/↓,
 * ⌘D / ⌘U, ⇧⌘L, a column drawn with ⌥-drag. No DOM, like the rest of the
 * engine.
 *
 * **One command, every cursor**: the cursors are visited from the last in the
 * text to the first, each time with the document's main selection set to that
 * cursor, so the single-cursor command runs unchanged. Every other cursor —
 * done or still waiting — follows the text through each change the visit
 * makes (`onTextChange`), and all of it is one undo step: typing keeps merging
 * into the typing step before it, as with one cursor. Cursors that end up
 * touching are merged.
 */

import type { TextStore } from "./buffer";
import type { EditKind, EditorDocument, TextChange } from "./document";
import { comparePos, cursor, isCursor, pos, posEqual, range, selectionRange, type Pos, type Selection } from "./position";
import { colForDisplayColumn, displayColumn, wordAt } from "./text";

/** Every cursor, the main one first. */
export function allSelections(doc: EditorDocument): Selection[] {
  return [doc.selection, ...doc.extra];
}

/** Whether there is more than one cursor. */
export function hasMultiple(doc: EditorDocument): boolean {
  return doc.extra.length > 0;
}

/** Where `p` is after `change`: before it unmoved, inside it at its new end, after it shifted. */
export function mapPos(p: Pos, change: TextChange): Pos {
  if (comparePos(p, change.start) <= 0) return p;
  if (comparePos(p, change.oldEnd) < 0) return change.newEnd;
  if (p.line === change.oldEnd.line) return pos(change.newEnd.line, change.newEnd.col + (p.col - change.oldEnd.col));
  return pos(p.line + change.newEnd.line - change.oldEnd.line, p.col);
}

function mapSelection(sel: Selection, change: TextChange): Selection {
  return { anchor: mapPos(sel.anchor, change), head: mapPos(sel.head, change) };
}

/**
 * `list` in text order with every pair that overlaps or meets merged into
 * one; `mainIndex` is followed to where the main cursor ended up.
 */
export function mergeSelections(list: readonly Selection[], mainIndex: number): { list: Selection[]; main: number } {
  const order = list.map((sel, i) => ({ sel, i })).sort((a, b) => comparePos(selectionRange(a.sel).start, selectionRange(b.sel).start));
  const out: Array<{ sel: Selection; main: boolean }> = [];
  for (const { sel, i } of order) {
    const prev = out[out.length - 1];
    const r = selectionRange(sel);
    if (prev) {
      const p = selectionRange(prev.sel);
      const touches = comparePos(r.start, p.end) < 0 || (comparePos(r.start, p.end) === 0 && (isCursor(sel) || isCursor(prev.sel)));
      if (touches) {
        const end = comparePos(r.end, p.end) > 0 ? r.end : p.end;
        // A merged selection keeps the direction of the one it grew from.
        const forward = comparePos(prev.sel.anchor, prev.sel.head) <= 0;
        prev.sel = forward ? { anchor: p.start, head: end } : { anchor: end, head: p.start };
        prev.main ||= i === mainIndex;
        continue;
      }
    }
    out.push({ sel, main: i === mainIndex });
  }
  const main = Math.max(0, out.findIndex((o) => o.main));
  return { list: out.map((o) => o.sel), main };
}

/** Puts `list` back as the document's cursors, `main` being the main one, the others in order. */
function putBack(doc: EditorDocument, list: Selection[], main: number, continues: boolean): void {
  const others = list.filter((_, i) => i !== main);
  doc.setSelections(list[main], others, continues);
}

/**
 * Runs `work` once per cursor (see the module header). `kind` is the edit
 * kind the work makes, so a typed character with several cursors still
 * merges into the typing step before it. `oncePerLine` skips a cursor on a
 * line an earlier visit already took (indent, move a line): it just follows.
 */
export function eachCursor(
  doc: EditorDocument,
  kind: EditKind | null,
  work: (index: number) => void,
  oncePerLine = false,
): void {
  const start = allSelections(doc);
  // Every cursor keeps its own goal column for ↑/↓.
  const goals = [doc.goalColumn, ...start.slice(1).map((_, i) => doc.extraGoals[i] ?? null)];
  // Visited last-in-text first, so a change never moves a cursor still to come.
  const order = start.map((_, i) => i).sort((a, b) => comparePos(selectionRange(start[b]).start, selectionRange(start[a]).start));
  const current = [...start];
  const unsubscribe = doc.onTextChange((change) => {
    for (let i = 0; i < current.length; i++) current[i] = mapSelection(current[i], change);
  });
  const grouped = !doc.inUndoGroup;
  if (grouped) doc.beginUndoGroup(kind);
  const takenLines = new Set<number>();
  const revision = doc.revision;
  try {
    for (const i of order) {
      const r = selectionRange(current[i]);
      if (oncePerLine) {
        let taken = false;
        for (let l = r.start.line; l <= r.end.line; l++) if (takenLines.has(l)) taken = true;
        if (taken) continue;
        for (let l = r.start.line; l <= r.end.line; l++) takenLines.add(l);
      }
      doc.focusCursor(current[i], goals[i]);
      work(i);
      current[i] = doc.selection;
      goals[i] = doc.goalColumn;
    }
  } finally {
    unsubscribe();
    if (grouped) doc.endUndoGroup();
  }
  const merged = mergeSelections(current, 0);
  // Only a visit that changed the text has a step of its own to put the cursors on.
  putBack(doc, merged.list, merged.main, kind !== null && doc.revision !== revision);
  // Goal columns go with the cursors that came through unmerged; a merged one starts afresh.
  // By place in the merged list: `putBack` stores copies, not these objects.
  const goalOf = new Map(current.map((s, i) => [s, goals[i]] as const));
  const listGoals = merged.list.map((s) => goalOf.get(s) ?? null);
  doc.goalColumn = listGoals[merged.main];
  doc.extraGoals = listGoals.filter((_, i) => i !== merged.main);
}

/**
 * Runs a command that moves whole lines (⌥↑/↓) once per block of cursors
 * on touching lines — the block as one selection, as if
 * those lines were selected — and puts each cursor back at its place in the
 * block wherever the command took it. Blocks are visited from the bottom up,
 * so a block's change never reaches the lines of one still to come (they are
 * not touching, so at least one line lies between them).
 */
export function eachLineBlock(doc: EditorDocument, work: () => void): void {
  const all = allSelections(doc);
  const lines = (s: Selection) => {
    const r = selectionRange(s);
    // A selection ending at a line's start does not take that line (as for one cursor).
    const last = r.end.line > r.start.line && r.end.col === 0 ? r.end.line - 1 : r.end.line;
    return { first: r.start.line, last };
  };
  const order = all.map((s, i) => ({ i, ...lines(s) })).sort((a, b) => a.first - b.first);
  const blocks: Array<{ first: number; last: number; members: number[] }> = [];
  for (const c of order) {
    const prev = blocks[blocks.length - 1];
    if (prev && c.first <= prev.last + 1) {
      prev.last = Math.max(prev.last, c.last);
      prev.members.push(c.i);
    } else blocks.push({ first: c.first, last: c.last, members: [c.i] });
  }
  const result = [...all];
  const grouped = !doc.inUndoGroup;
  if (grouped) doc.beginUndoGroup();
  const revision = doc.revision;
  try {
    for (const block of blocks.reverse()) {
      const end = pos(block.last, doc.store.line(block.last).length);
      doc.focusCursor({ anchor: pos(block.first, 0), head: end });
      work();
      const shift = selectionRange(doc.selection).start.line - block.first;
      for (const i of block.members) {
        const s = all[i];
        result[i] = { anchor: pos(s.anchor.line + shift, s.anchor.col), head: pos(s.head.line + shift, s.head.col) };
      }
    }
  } finally {
    if (grouped) doc.endUndoGroup();
  }
  const merged = mergeSelections(result, 0);
  putBack(doc, merged.list, merged.main, doc.revision !== revision);
}

/** ⌥-click: a cursor at `at` added (or, if one is there, taken away). */
export function toggleCursor(doc: EditorDocument, at: Pos): void {
  const all = allSelections(doc);
  const hit = all.findIndex((sel) => isCursor(sel) && posEqual(sel.head, at));
  if (hit >= 0 && all.length > 1) {
    const rest = all.filter((_, i) => i !== hit);
    putBack(doc, rest, rest.length - 1, false);
    return;
  }
  // The newest cursor is the main one; the old main joins the others at the end.
  const merged = mergeSelections([cursor(at), ...all], 0);
  putBack(doc, merged.list, merged.main, false);
  reorderNewestLast(doc, all);
}

/**
 * Keeps `doc.extra` in the order cursors were added, the old main one last —
 * `mergeSelections` sorts by position; ⌘U needs the adding order.
 */
function reorderNewestLast(doc: EditorDocument, before: readonly Selection[]): void {
  const key = (s: Selection) => `${s.anchor.line}:${s.anchor.col}:${s.head.line}:${s.head.col}`;
  const rank = new Map<string, number>();
  // Older extras first, then the old main, as they were.
  [...before.slice(1), before[0]].forEach((s, i) => rank.set(key(s), i));
  doc.extra = [...doc.extra].sort((a, b) => (rank.get(key(a)) ?? -1) - (rank.get(key(b)) ?? -1));
  doc.extraGoals = doc.extra.map(() => null);
}

/**
 * ⌥⌘↑ / ⌥⌘↓: one more cursor a line above (below) the topmost (bottommost),
 * at the display column of the cursor this began from (the oldest) — so a
 * column passing a short line comes back out at its own column.
 */
export function addCursorVertical(doc: EditorDocument, dir: -1 | 1, tabSize: number): void {
  const all = allSelections(doc);
  const edge = all.reduce((best, sel) => (comparePos(sel.head, best.head) * dir > 0 ? sel : best), all[0]);
  const line = edge.head.line + dir;
  if (line < 0 || line >= doc.store.lineCount()) return;
  const origin = doc.extra[0] ?? doc.selection;
  const column = displayColumn(doc.store.line(origin.head.line), origin.head.col, tabSize);
  const at = pos(line, colForDisplayColumn(doc.store.line(line), column, tabSize));
  const merged = mergeSelections([cursor(at), ...all], 0);
  putBack(doc, merged.list, merged.main, false);
  reorderNewestLast(doc, all);
}

/** The word around a cursor, as ⌘D and ⇧⌘L take it when nothing is selected. */
function wordSelection(store: TextStore, at: Pos): Selection | null {
  const w = wordAt(store.line(at.line), at.col);
  return w.end > w.start ? { anchor: pos(at.line, w.start), head: pos(at.line, w.end) } : null;
}

/** Every place `needle` occurs in `store`, case and all, in text order (no overlaps). */
export function occurrences(store: TextStore, needle: string): Selection[] {
  if (!needle) return [];
  const out: Selection[] = [];
  const text = store.text();
  const lineStarts: number[] = [0];
  for (let i = text.indexOf("\n"); i !== -1; i = text.indexOf("\n", i + 1)) lineStarts.push(i + 1);
  const toPos = (offset: number): Pos => {
    let lo = 0;
    let hi = lineStarts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (lineStarts[mid] <= offset) lo = mid;
      else hi = mid - 1;
    }
    return pos(lo, offset - lineStarts[lo]);
  };
  for (let at = text.indexOf(needle); at !== -1; at = text.indexOf(needle, at + needle.length)) {
    out.push({ anchor: toPos(at), head: toPos(at + needle.length) });
  }
  return out;
}

/**
 * ⌘D: with a bare cursor, selects the word it is in; with a selection, adds
 * the next place its text occurs (wrapping around), which becomes the main
 * cursor. Nothing when every occurrence is already selected.
 */
export function addNextOccurrence(doc: EditorDocument): void {
  const main = doc.selection;
  if (isCursor(main)) {
    const word = wordSelection(doc.store, main.head);
    if (word) doc.setSelections(word, doc.extra);
    return;
  }
  const needle = doc.store.slice(selectionRange(main));
  const all = allSelections(doc);
  const taken = new Set(all.map((s) => `${selectionRange(s).start.line}:${selectionRange(s).start.col}`));
  const found = occurrences(doc.store, needle);
  const after = selectionRange(main).end;
  const next =
    found.find((s) => comparePos(s.anchor, after) >= 0 && !taken.has(`${s.anchor.line}:${s.anchor.col}`)) ??
    found.find((s) => !taken.has(`${s.anchor.line}:${s.anchor.col}`));
  if (!next) return;
  doc.setSelections(next, [...doc.extra, main]);
}

/** ⌘U: takes back the cursor added last; the one before it becomes the main one. */
export function removeLastCursor(doc: EditorDocument): void {
  if (doc.extra.length === 0) return;
  const extra = [...doc.extra];
  const main = extra.pop()!;
  doc.setSelections(main, extra);
}

/** ⇧⌘L: every occurrence of the selection's text (of the word at the cursor) selected. */
export function selectAllOccurrences(doc: EditorDocument): void {
  const main = isCursor(doc.selection) ? wordSelection(doc.store, doc.selection.head) : doc.selection;
  if (!main) return;
  const found = occurrences(doc.store, doc.store.slice(selectionRange(main)));
  if (found.length === 0) return;
  const start = selectionRange(main).start;
  const index = Math.max(0, found.findIndex((s) => posEqual(s.anchor, start)));
  putBack(doc, found, index, false);
}

/** Esc with several cursors: only the main one stays. */
export function singleCursor(doc: EditorDocument): boolean {
  if (doc.extra.length === 0) return false;
  doc.setSelection(doc.selection);
  return true;
}

/**
 * ⌥-drag: a column from `anchor` to `head` — one selection per line between
 * them, spanning the same display columns (shorter lines get a cursor at
 * their end) — added to the cursors in `keep` (those there before the drag).
 * The line the drag is on holds the main cursor.
 */
export function columnSelection(
  doc: EditorDocument,
  anchor: Pos,
  head: Pos,
  tabSize: number,
  keep: readonly Selection[] = [],
): void {
  const store = doc.store;
  const fromCol = displayColumn(store.line(anchor.line), anchor.col, tabSize);
  const toCol = displayColumn(store.line(head.line), head.col, tabSize);
  const step = head.line >= anchor.line ? 1 : -1;
  const list: Selection[] = [];
  for (let line = anchor.line; ; line += step) {
    const text = store.line(line);
    list.push({ anchor: pos(line, colForDisplayColumn(text, fromCol, tabSize)), head: pos(line, colForDisplayColumn(text, toCol, tabSize)) });
    if (line === head.line) break;
  }
  const merged = mergeSelections([list[list.length - 1], ...list.slice(0, -1), ...keep], 0);
  putBack(doc, merged.list, merged.main, false);
}

/** The text of every selection, in text order — for copying several at once. */
export function selectionTexts(doc: EditorDocument): string[] {
  return allSelections(doc)
    .map((s) => selectionRange(s))
    .sort((a, b) => comparePos(a.start, b.start))
    .map((r) => doc.store.slice(range(r.start, r.end)));
}
