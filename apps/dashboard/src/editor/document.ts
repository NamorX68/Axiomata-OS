/**
 * One open document: its text, its selection, and its undo history
 * (`docs/plans/editor.md`, ED1, F1/F4/F11).
 *
 * Every change goes through `EditorDocument.edit`, which applies a list of
 * replacements and records them as one undo step. Undo is linear (D19) and
 * unbounded for as long as the file is open. Consecutive steps of the same kind
 * merge the way a person expects them to:
 *
 * * typing stays one step until a 1 s pause, a cursor jump, or a switch between
 *   typing and deleting;
 * * everything else — paste, cut, indent, moving a line — is a step of its own;
 * * undo and redo restore the selection the step started or ended with.
 *
 * "Unsaved" is not a flag that could drift: the document remembers which undo
 * step was on top when it was last saved, and is dirty whenever that is not the
 * step on top now — so undoing back to the save point is clean again, while
 * undoing past it and typing something else is dirty until the next save.
 */

import { clampPos, endOfText } from "./buffer";
import { bodyForStore, detectIndent, detectShape, joinForSave, type FileShape, type Indent } from "./detect";
import { cursor, pos, posEqual, range, type Pos, type Range, type Selection } from "./position";
import { RopeStore } from "./rope";

/** How a step may merge with the one before it. */
export type EditKind = "typing" | "deleting" | "other";

/** Longest pause that still continues a typing (or deleting) step. */
export const MERGE_WINDOW_MS = 1000;

/**
 * One replacement as it hit the text, in both coordinate systems a syntax tree
 * needs (`tree-sitter`'s `Edit`): positions and offsets, before and after.
 */
export interface TextChange {
  start: Pos;
  oldEnd: Pos;
  newEnd: Pos;
  startIndex: number;
  oldEndIndex: number;
  newEndIndex: number;
}

/** One replacement, in the coordinates of the text at the moment it is applied. */
export interface Change {
  range: Range;
  text: string;
}

/** A change as it happened, with what it removed — enough to undo it. */
interface AppliedChange {
  start: Pos;
  removed: string;
  inserted: string;
}

interface Step {
  changes: AppliedChange[];
  before: Selection;
  after: Selection;
  kind: EditKind;
  time: number;
}

export interface DocumentOptions {
  /** Indentation to use when the file itself does not show one. */
  indentFallback: Indent;
}

/** The position after `text` when it is inserted at `start`. */
export function endAfter(start: Pos, text: string): Pos {
  const lines = text.split("\n");
  const last = lines.length - 1;
  return pos(start.line + last, (last === 0 ? start.col : 0) + lines[last].length);
}

export class EditorDocument {
  readonly store: RopeStore;
  shape: FileShape;
  indent: Indent;
  /** `true` if the indentation came from the file, `false` if from the setting. */
  indentDetected: boolean;
  selection: Selection = cursor(pos(0, 0));
  /**
   * The display column ↑/↓ try to keep, set by the first vertical move and
   * cleared by anything else — so the cursor returns to column 40 after
   * passing a short line.
   */
  goalColumn: number | null = null;
  /** Grows with every change, undo and redo; lets a view know when to redraw. */
  revision = 0;

  private undoStack: Step[] = [];
  private redoStack: Step[] = [];
  /** The step on top when the document was last saved (`null` = none). */
  private savedTop: Step | null = null;
  /** Set when the next step must not merge into the current top. */
  private sealed = true;
  /**
   * An open undo group: every edit until `endUndoGroup` joins one step,
   * whatever its kind and however long it takes. Vi is the first user (ED3,
   * V5: `cw` plus the typing after it is one change). `null` while none is
   * open; holds the step once the group's first edit made one.
   */
  private group: { step: Step | null } | null = null;
  /** Told about every replacement — edits, undo, redo, reloads alike. */
  private listeners = new Set<(change: TextChange) => void>();

  constructor(fileText: string, options: DocumentOptions) {
    this.shape = detectShape(fileText);
    const detected = detectIndent(fileText);
    this.indent = detected ?? options.indentFallback;
    this.indentDetected = detected !== null;
    this.store = new RopeStore(bodyForStore(fileText, this.shape));
  }

  /** The text to save, in the file's own line endings. */
  textForSave(): string {
    return joinForSave(this.store.text(), this.shape);
  }

  get dirty(): boolean {
    return this.top() !== this.savedTop;
  }

  get canUndo(): boolean {
    return this.undoStack.length > 0;
  }

  get canRedo(): boolean {
    return this.redoStack.length > 0;
  }

  /** Marks the current text as saved, and seals the step on top against merging. */
  markSaved(): void {
    this.savedTop = this.top();
    this.sealed = true;
  }

  /**
   * Moves the selection. A move that is not simply "the cursor stays where the
   * last edit left it" seals the current step, so the next typing starts a new
   * one (F4: a cursor jump ends a typing step).
   */
  setSelection(sel: Selection, keepGoalColumn = false): void {
    const next = { anchor: clampPos(this.store, sel.anchor), head: clampPos(this.store, sel.head) };
    const top = this.top();
    if (!top || !posEqual(top.after.head, next.head) || !posEqual(top.after.anchor, next.anchor)) {
      this.sealed = true;
    }
    this.selection = next;
    if (!keepGoalColumn) this.goalColumn = null;
  }

  /**
   * Applies `changes` in order (each in the coordinates left by the previous
   * one), records them as one step, and puts the selection at `after`.
   */
  edit(changes: Change[], after: Selection, kind: EditKind = "other", now = Date.now()): void {
    if (changes.length === 0) return;
    const before = this.selection;
    const applied: AppliedChange[] = [];
    for (const change of changes) {
      // Line breaks are `\n` inside the store; a `\r` kept here would make
      // `endAfter` disagree with what the store did, and undo miss its range.
      const inserted = change.text.replace(/\r\n?/g, "\n");
      const removed = this.store.slice(change.range);
      this.replaceText(change.range, inserted);
      applied.push({ start: change.range.start, removed, inserted });
    }
    this.redoStack = [];
    const top = this.top();
    const grouped = this.group !== null && this.group.step !== null && top === this.group.step;
    const merges =
      grouped ||
      (top !== null &&
        !this.sealed &&
        kind !== "other" &&
        top.kind === kind &&
        now - top.time <= MERGE_WINDOW_MS &&
        top !== this.savedTop);
    if (merges && top) {
      top.changes.push(...applied);
      top.after = after;
      top.time = now;
    } else {
      this.undoStack.push({ changes: applied, before, after, kind, time: now });
      if (this.group) this.group.step = this.top();
    }
    this.sealed = kind === "other";
    this.selection = after;
    this.goalColumn = null;
    this.revision++;
  }

  /**
   * Opens an undo group: the edits until {@link endUndoGroup} become one step,
   * and undoing it returns to the selection before its first edit.
   */
  beginUndoGroup(): void {
    this.group = { step: null };
    this.sealed = true;
  }

  endUndoGroup(): void {
    this.group = null;
    this.sealed = true;
  }

  /** Whether an undo group is open. */
  get inUndoGroup(): boolean {
    return this.group !== null;
  }

  undo(): boolean {
    this.group = null;
    const step = this.undoStack.pop();
    if (!step) return false;
    for (const change of [...step.changes].reverse()) {
      this.replaceText(range(change.start, endAfter(change.start, change.inserted)), change.removed);
    }
    this.redoStack.push(step);
    this.afterHistoryMove(step.before);
    return true;
  }

  redo(): boolean {
    this.group = null;
    const step = this.redoStack.pop();
    if (!step) return false;
    for (const change of step.changes) {
      this.replaceText(range(change.start, endAfter(change.start, change.removed)), change.inserted);
    }
    this.undoStack.push(step);
    this.afterHistoryMove(step.after);
    return true;
  }

  /**
   * Replaces the whole text with `fileText` (in the file's own shape) as one
   * undoable step — restoring kept unsaved work, which the owner may want to
   * take back with ⌘Z.
   */
  replaceAll(fileText: string): void {
    const whole = range(pos(0, 0), endOfText(this.store));
    this.edit([{ range: whole, text: bodyForStore(fileText, this.shape) }], cursor(pos(0, 0)), "other");
  }

  /**
   * Replaces the whole text — a reload after an external change. Clears the
   * history (there is nothing to undo into) and counts as saved; the cursor
   * stays as close to where it was as the new text allows.
   */
  reset(fileText: string): void {
    this.shape = detectShape(fileText);
    const whole = range(pos(0, 0), clampPos(this.store, pos(Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER)));
    this.replaceText(whole, bodyForStore(fileText, this.shape));
    this.undoStack = [];
    this.redoStack = [];
    this.savedTop = null;
    this.sealed = true;
    this.selection = cursor(clampPos(this.store, this.selection.head));
    this.goalColumn = null;
    this.revision++;
  }

  /**
   * Subscribes to every replacement of the text (a syntax tree follows them
   * incrementally). Returns the unsubscribe function.
   */
  onTextChange(listener: (change: TextChange) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** The one place the store is written: replaces, then tells the listeners. */
  private replaceText(r: Range, raw: string): void {
    // The store splits any line break; offsets must count what it keeps.
    const text = raw.replace(/\r\n?/g, "\n");
    if (this.listeners.size === 0) {
      this.store.replace(r, text);
      return;
    }
    const startIndex = this.store.offsetAt(r.start);
    const oldEndIndex = this.store.offsetAt(r.end);
    const newEnd = this.store.replace(r, text);
    const change: TextChange = {
      start: r.start,
      oldEnd: r.end,
      newEnd,
      startIndex,
      oldEndIndex,
      newEndIndex: startIndex + text.length,
    };
    for (const listener of this.listeners) listener(change);
  }

  private afterHistoryMove(sel: Selection): void {
    this.selection = { anchor: clampPos(this.store, sel.anchor), head: clampPos(this.store, sel.head) };
    this.sealed = true;
    this.goalColumn = null;
    this.revision++;
  }

  private top(): Step | null {
    return this.undoStack[this.undoStack.length - 1] ?? null;
  }
}
