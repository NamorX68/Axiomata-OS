/**
 * Insert and Replace mode (`docs/plans/editor.md`, ED3, V5): the Mac keymap's typing
 * commands, plus Vi's `<C-o>` (one Normal command, then back), `<C-r>{register}`,
 * `<C-w>`/`<C-u>`, and Replace's `<BS>` that restores what it overwrote.
 *
 * A session remembers the keys typed (for a count's repeat, `.` and `".`), and — for
 * Visual-block `I`/`A`/`c` — the rectangle to copy them onto once Esc ends it. One
 * session is one undo step, including a count's repeat and the block copy.
 *
 * `ViMachine` (`machine.ts`) still owns the mode field, the change being recorded and
 * `<C-o>`'s one-shot Normal command; {@link InsertHost} is the small seam between them.
 */

import type { Command } from "../commands";
import { run } from "../commands";
import type { EditorDocument } from "../document";
import { cursor, pos, range, type Pos } from "../position";
import { displayColumn, nextGrapheme, prevGrapheme } from "../text";
import { isEscape, type ViKey } from "./keys";
import type { Marks } from "./marks";
import type { ViContext } from "./motions";
import type { ViMode } from "./machine";
import { blockCols, shiftLines } from "./ops";
import type { Registers } from "./registers";
import { firstNonBlank } from "./scan";

/** What Insert/Replace needs from `ViMachine`. */
export interface InsertHost {
  readonly doc: EditorDocument;
  readonly marks: Marks;
  readonly registers: Registers;
  ctx(): ViContext;
  readOnly(): boolean;
  bell(): void;
  /** Places the cursor and forgets the vertical-motion goal column, as Normal mode does. */
  setCursor(p: Pos): void;
  setMode(mode: ViMode): void;
  /** The keys typed this session, for the change being recorded (only matters if one is). */
  noteInsertChange(keys: ViKey[]): void;
  /** The change this session started is complete — a no-op if none is being recorded. */
  finishChange(): void;
  /** `<C-o>`: one Normal command, then back to Insert. */
  enableOneShot(): void;
}

/** Visual-block `I`/`A`/`c`: the rectangle to copy the session's typing onto. */
export interface BlockInsertPlan {
  first: number;
  last: number;
  column: number;
  append: boolean;
}

/** An Insert session: where it began and what was typed, for counts, `.`, `".` and blocks. */
interface InsertSession {
  count: number;
  keys: ViKey[];
  replace: boolean;
  /** An arrow key moved the cursor: counts and block copies no longer apply (as in Vim). */
  broken: boolean;
  block: BlockInsertPlan | null;
  /** The characters Replace mode overwrote, for `<BS>`. */
  replaced: (string | null)[];
}

/** Arrow keys in Insert mode, as the normal key map's motions. */
const INSERT_ARROWS: Record<string, Extract<Command, { type: "move" }>["motion"]> = {
  "<Left>": "charLeft",
  "<Right>": "charRight",
  "<Up>": "up",
  "<Down>": "down",
  "<Home>": "lineStart",
  "<End>": "lineEnd",
};

export class InsertMode {
  private session: InsertSession | null = null;
  private ctrlR = false;

  constructor(private readonly host: InsertHost) {}

  /** Whether a session is running (Insert or Replace mode). */
  get active(): boolean {
    return this.session !== null;
  }

  /** Visual-block `I`/`A`/`c`: what the just-started session types goes onto this rectangle too. */
  setBlock(block: BlockInsertPlan): void {
    if (this.session) this.session.block = block;
  }

  /** `3o`: the new line's own `<CR>` repeats along with what is typed after it. */
  prependKey(key: ViKey): void {
    this.session?.keys.unshift(key);
  }

  /** Starts a session at `at` — Insert, or Replace when `replace` is set. */
  enter(at: Pos, count: number, replace: boolean): void {
    if (this.host.readOnly()) return this.host.bell();
    // The change that started the session may have opened the group already (`c`, `o`).
    if (!this.host.doc.inUndoGroup) this.host.doc.beginUndoGroup();
    this.host.setMode(replace ? "replace" : "insert");
    this.session = { count, keys: [], replace, broken: false, block: null, replaced: [] };
    this.host.doc.setSelection(cursor(at), false);
    this.host.marks.set("^", at);
  }

  /** One key, while a session is running. */
  key(key: ViKey): void {
    const session = this.session!;
    if (this.ctrlR) {
      this.ctrlR = false;
      if (typeof key === "string" && key.length === 1) {
        const content = this.host.registers.get(key);
        if (content) this.typeText(content.text);
        session.keys.push({ text: content?.text ?? "" });
      }
      return;
    }
    if (isEscape(key)) return this.leave();
    if (key === "<C-r>") {
      this.ctrlR = true;
      return;
    }
    if (key === "<C-o>") {
      this.host.enableOneShot();
      this.finish(false);
      return;
    }
    if (this.moveKey(key)) {
      session.broken = true;
      session.keys = [];
      return;
    }
    // A taken completion repeats (`.`) as its text at the cursor — not its import a second time.
    const taken = typeof key === "object" && "command" in key && key.command.type === "complete" ? key.command : null;
    session.keys.push(taken ? { command: { type: "complete", edit: { ...taken.edit, extra: [] } } } : key);
    this.applyKey(key);
  }

  /** Arrow keys and Mac cursor commands: they move, and break repetition (as in Vim). */
  private moveKey(key: ViKey): boolean {
    const doc = this.host.doc;
    if (typeof key === "string" && key in INSERT_ARROWS) {
      run(doc, { type: "move", motion: INSERT_ARROWS[key], extend: false }, this.host.ctx());
      this.restartGroup();
      return true;
    }
    if (typeof key === "object" && "command" in key && key.command.type === "move") {
      run(doc, key.command, this.host.ctx());
      this.restartGroup();
      return true;
    }
    return false;
  }

  /**
   * A cursor move inside a session starts a new undo step, as in Vim: what was
   * typed before it is one step, what comes after another.
   */
  private restartGroup(): void {
    const doc = this.host.doc;
    if (doc.inUndoGroup) doc.endUndoGroup();
    doc.beginUndoGroup();
  }

  private applyKey(key: ViKey): void {
    const doc = this.host.doc;
    const ctx = this.host.ctx();
    if (typeof key === "object") {
      if ("text" in key) this.typeText(key.text);
      else run(doc, key.command, ctx);
      return;
    }
    switch (key) {
      case "<CR>":
        if (this.session!.replace) this.typeText("\n");
        else run(doc, { type: "newline" }, ctx);
        return;
      case "<BS>":
        if (this.session!.replace) return this.replaceBackspace();
        run(doc, { type: "deleteBackward", unit: "char" }, ctx);
        return;
      case "<Del>":
        run(doc, { type: "deleteForward", unit: "char" }, ctx);
        return;
      case "<Tab>":
        run(doc, { type: "indent" }, ctx);
        return;
      case "<S-Tab>":
      case "<C-d>":
        return this.shiftCurrentLine(-1);
      case "<C-t>":
        return this.shiftCurrentLine(1);
      case "<C-w>":
        return this.deleteWordBefore();
      case "<C-u>":
        return this.deleteLineBefore();
    }
    if (key.length === 1 || !key.startsWith("<")) this.typeText(key === "<" ? "<" : key);
  }

  /** Typed text: inserted in Insert mode, written over the line in Replace mode. */
  private typeText(text: string): void {
    const doc = this.host.doc;
    const ctx = this.host.ctx();
    const session = this.session!;
    if (!session.replace) {
      run(doc, { type: "insert", text, kind: "typing" }, ctx);
      return;
    }
    for (const ch of text) {
      const at = doc.selection.head;
      const line = doc.store.line(at.line);
      if (ch === "\n" || at.col >= line.length) {
        session.replaced.push(null);
        run(doc, { type: "insert", text: ch, kind: "typing" }, ctx);
      } else {
        const end = nextGrapheme(line, at.col);
        session.replaced.push(line.slice(at.col, end));
        const after = cursor(pos(at.line, at.col + ch.length));
        doc.edit([{ range: range(at, pos(at.line, end)), text: ch }], after, "typing");
      }
    }
  }

  /** `<BS>` in Replace mode: the overwritten character comes back. */
  private replaceBackspace(): void {
    const doc = this.host.doc;
    const at = doc.selection.head;
    const original = this.session!.replaced.pop();
    if (original === undefined || at.col === 0) {
      if (at.col > 0) this.host.setCursor(pos(at.line, prevGrapheme(doc.store.line(at.line), at.col)));
      return;
    }
    const line = doc.store.line(at.line);
    const start = prevGrapheme(line, at.col);
    const back = pos(at.line, start);
    doc.edit([{ range: range(back, at), text: original ?? "" }], cursor(back), "typing");
  }

  private shiftCurrentLine(dir: 1 | -1): void {
    const doc = this.host.doc;
    const at = doc.selection.head;
    const before = doc.store.line(at.line).length;
    shiftLines(doc, at.line, at.line, dir, 1, this.host.ctx());
    const delta = doc.store.line(at.line).length - before;
    doc.setSelection(cursor(pos(at.line, Math.max(0, at.col + delta))));
  }

  /** Ctrl-w: back over blanks, then over one word (or run of punctuation); at a line's start, the break. */
  private deleteWordBefore(): void {
    const doc = this.host.doc;
    const at = doc.selection.head;
    if (at.col === 0) {
      run(doc, { type: "deleteBackward", unit: "char" }, this.host.ctx());
      return;
    }
    const line = doc.store.line(at.line);
    let col = at.col;
    while (col > 0 && /[ \t]/.test(line[col - 1])) col--;
    const isWord = (ch: string) => /[\p{L}\p{N}\p{M}_]/u.test(ch);
    if (col > 0) {
      const word = isWord(line[col - 1]);
      while (col > 0 && !/[ \t]/.test(line[col - 1]) && isWord(line[col - 1]) === word) col--;
    }
    doc.edit([{ range: range(pos(at.line, col), at), text: "" }], cursor(pos(at.line, col)), "deleting");
  }

  /** Ctrl-u: back to the indentation (or the line's start, when already there). */
  private deleteLineBefore(): void {
    const doc = this.host.doc;
    const at = doc.selection.head;
    const indent = firstNonBlank(doc.store, at.line);
    const col = at.col > indent ? indent : 0;
    if (col === at.col) return;
    doc.edit([{ range: range(pos(at.line, col), at), text: "" }], cursor(pos(at.line, col)), "deleting");
  }

  /** Esc: a count's repeat and a block copy are applied, then the session ends. */
  private leave(): void {
    const session = this.session!;
    if (!session.broken) {
      for (let i = 1; i < session.count; i++) for (const key of session.keys) this.applyKey(key);
      if (session.block) this.copyBlock(session.block, session.keys);
    }
    this.finish(true);
  }

  private finish(toNormal: boolean): void {
    const session = this.session!;
    this.host.registers.readOnly["."] = () => insertedText(session.keys);
    const doc = this.host.doc;
    const at = doc.selection.head;
    this.host.marks.set("^", at);
    this.host.noteInsertChange([...session.keys, "<Esc>"]);
    if (toNormal) this.host.finishChange();
    this.session = null;
    this.host.setMode("normal");
    // Whether for good (Esc) or for one command (Ctrl-o), what was typed so far
    // is one undo step; the one command and what follows it are steps of their own.
    if (doc.inUndoGroup) doc.endUndoGroup();
    const line = doc.store.line(at.line);
    this.host.setCursor(toNormal && at.col > 0 ? pos(at.line, prevGrapheme(line, at.col)) : at);
  }

  /** Visual-block `I`/`A`/`c`: what was typed on the first line goes to the others too. */
  private copyBlock(block: BlockInsertPlan, keys: ViKey[]): void {
    const text = insertedText(keys);
    if (!text || text.includes("\n")) return;
    const doc = this.host.doc;
    const ctx = this.host.ctx();
    const store = doc.store;
    const changes = [];
    for (let l = block.last; l > block.first; l--) {
      const line = store.line(l);
      const width = displayColumn(line, line.length, ctx.tabSize);
      if (!block.append && width < block.column) continue;
      const pad = block.append && block.column !== Infinity ? " ".repeat(Math.max(0, block.column - width)) : "";
      const col =
        block.append && block.column === Infinity
          ? line.length
          : blockCols(line, { left: block.column, right: block.column }, ctx.tabSize)[0];
      const at = pos(l, Math.min(col, line.length));
      changes.push({ range: range(at, at), text: pad + text });
    }
    if (changes.length > 0) doc.edit(changes, doc.selection, "other");
  }
}

/** The text typed in a session, as `".` holds it. */
function insertedText(keys: readonly ViKey[]): string {
  let text = "";
  for (const key of keys) {
    if (typeof key === "object") {
      if ("text" in key) text += key.text;
    } else if (key === "<CR>") text += "\n";
    else if (key === "<BS>") text = text.slice(0, -1);
    else if (key.length === 1 || !key.startsWith("<")) text += key;
  }
  return text;
}
