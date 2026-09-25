/**
 * The Vi machine (`docs/plans/editor.md`, ED3, D17, V1–V12): keys in, edits,
 * cursor moves and effects out — for one open document. Pure like the rest of
 * the engine; the view feeds it keys (`feed`) and draws what it reports (mode,
 * cursor shape, the Visual region, the pill's text).
 *
 * * **Modes**: Normal, Insert, Replace, Visual (characters, lines, block).
 * * **Commands** are parsed from the keys typed so far (`parse.ts`) and run
 *   once whole. A change — an operator, `x`, `p`, `o` … — is one undo step,
 *   including the Insert session it may start (`cw` + typing + Esc).
 * * **`.`** replays the last change's keys (with a new count if one is typed);
 *   a Visual change replays on as many characters/lines from the cursor.
 * * **Macros** are keys in a register (`q`/`@`), in Vim's notation.
 * * **The clipboard** may answer late (it lives in Rust, V3): a command that
 *   reads it is paused, and runs again once the text is there. Keys typed
 *   meanwhile wait in the queue, so nothing overtakes it.
 */

import type { EditorDocument } from "../document";
import type { SetOption } from "./ex";
import { comparePos, cursor, pos, range, type Pos } from "../position";
import { displayColumn, leadingWhitespace, nextGrapheme, prevGrapheme } from "../text";
import { CommandMode, SearchMemory, waitsForSearch, type CmdlineStatus, type ViMessage } from "./cmdmode";
import { InsertMode, type InsertHost } from "./insert";
import { isEscape, keysToText, parseKeys, type ViKey } from "./keys";
import { isLinewiseMotion, landing, motion, type FindState, type MotionResult, type ViContext } from "./motions";
import {
  blockCols,
  caseRegion,
  commentLines,
  deleteRegion,
  incrementNumber,
  joinLines,
  putRegister,
  regionContent,
  reindentLines,
  replaceChars,
  replaceRegion,
  shiftLines,
  surroundPair,
  surroundRegion,
  type CaseChange,
  type Region,
} from "./ops";
import { parse, type Body, type Parsed, type Target } from "./parse";
import { ClipboardPending, Registers, type ClipboardPort } from "./registers";
import { Marks } from "./marks";
import {
  headForSize,
  prevChar,
  regionEnd,
  regionLines,
  regionStart,
  visualRegion,
  visualSize,
  wholeLines,
  type VisualMode,
  type VisualSize,
} from "./regions";
import { clampNormal, firstNonBlank, nextPos } from "./scan";
import { isClassicObject, textObject, type ObjectRange } from "./textobjects";

export type ViMode = "normal" | "insert" | "replace" | "visual" | "visualLine" | "visualBlock";

/** What the machine asks the view to do. */
export type ViEffect =
  | { type: "bell" }
  | { type: "scroll"; line: number; to: "top" | "center" | "bottom" }
  | { type: "scrollLines"; delta: number }
  | { type: "save" }
  | { type: "quit"; force: boolean }
  | { type: "saveQuit" }
  /** `:e!`: the file as it is on disk, dropping the changes. */
  | { type: "reload" }
  /** `:set wrap`, `:set nu` … — the view's own options, for this editor only (V6). */
  | { type: "set"; option: SetOption; value: boolean | "toggle" }
  /** `]c`/`[c` in a diff (H9), `gf` there or on a path. */
  | { type: "hunk"; dir: 1 | -1 }
  | { type: "openFile" }
  /** A file mark (`'A`) that belongs to another file. */
  | { type: "fileMark"; file: string; at: Pos };

/** A tree-sitter text object (`if af ic ac ia aa`, V9) — the view supplies it (ED3.4). */
export type SyntaxObjects = (at: Pos, name: string, inner: boolean, count: number) => ObjectRange | null;

export interface ViEnv {
  ctx(): ViContext;
  effect(effect: ViEffect): void;
  /** A diff or other read-only surface: moving, selecting and yanking only (V1). */
  readOnly?: boolean;
  /** For `"%` and the file marks `A`–`Z`. */
  fileName?: string;
  fileKey?: string;
  syntaxObjects?: SyntaxObjects;
}

/**
 * What all documents' machines share: registers, the last `f`/`t`, file marks,
 * the last macro, and the last search, substitution and command lines.
 */
export class ViShared {
  readonly registers: Registers;
  lastFind: FindState | null = null;
  readonly fileMarks = new Map<string, { file: string; at: Pos }>();
  lastMacro: string | null = null;
  readonly search = new SearchMemory();

  constructor(clipboard: ClipboardPort | null, shareClipboard = true) {
    this.registers = new Registers(clipboard, shareClipboard);
    this.registers.readOnly["/"] = () => this.search.last?.pattern ?? "";
    this.registers.readOnly[":"] = () => this.search.lastEx ?? "";
  }
}

/** What the mode pill shows (V8): the mode, a half-typed command, a recording, the command line, a message. */
export interface ViStatus {
  mode: ViMode;
  pending: string;
  recording: string | null;
  cmdline: CmdlineStatus | null;
  message: ViMessage | null;
}

/** A queued key, and whether it is replayed (`.`, a macro) rather than typed. */
interface Queued {
  key: ViKey;
  replay: boolean;
}

/** The last change, as keys to replay (`.`). */
interface Change {
  register: string | null;
  count: number | null;
  plain: ViKey[];
  /** Keys typed in the Insert session the change started, ending with `<Esc>`. */
  insert: ViKey[];
  /** For a Visual change: how much it covered. */
  visual: VisualSize | null;
}

const CHANGE_COMMANDS = new Set([
  "x", "X", "s", "S", "D", "C", "p", "P", "gp", "gP", "J", "gJ", "~", "r", "i", "a", "I", "A", "gi", "gI", "o", "O",
  "R", "<C-a>", "<C-x>", "cs", "ds",
]);

/** Commands that enter Insert or Replace mode — writing ones a read-only surface refuses. */
const INSERTING = new Set(["i", "a", "I", "A", "gi", "gI", "o", "O", "R", "s", "S", "C"]);

export class ViMachine {
  mode: ViMode = "normal";
  private pending: ViKey[] = [];
  private queue: Queued[] = [];
  private blocked = false;
  private visualAnchor: Pos = pos(0, 0);
  /** Visual block after `$`: every line to its end. */
  private blockToEnd = false;
  private lastVisual: { mode: ViMode; anchor: Pos; head: Pos } | null = null;
  private goal: number | null = null;
  private readonly marks = new Marks();
  private lastChange: Change | null = null;
  /** The change being recorded while its Insert session runs. */
  private changeInProgress: Change | null = null;
  /** Ctrl-o in Insert mode: one Normal command, then back. */
  private oneShot = false;
  private recording: { register: string; keys: ViKey[] } | null = null;
  private readonly insertMode: InsertMode;
  private readonly commands: CommandMode;
  /** The last message (`E486: Pattern not found`), shown until the next key. */
  private message: ViMessage | null = null;
  private readonly unsubscribe: () => void;

  constructor(
    readonly doc: EditorDocument,
    readonly shared: ViShared,
    private readonly env: ViEnv,
  ) {
    this.unsubscribe = doc.onTextChange((change) => this.marks.follow(change));
    this.doc.setSelection(cursor(clampNormal(doc.store, doc.selection.head)));
    this.insertMode = new InsertMode(this.insertHost());
    this.commands = new CommandMode({
      doc,
      memory: shared.search,
      registers: shared.registers,
      readOnly: () => env.readOnly ?? false,
      cursor: () => this.cursor,
      jumpTo: (at) => {
        this.pushJump(this.cursor);
        this.setCursor(at);
      },
      mark: (name) => this.getMark(name),
      effect: (e) => env.effect(e),
      message: (m) => {
        this.message = m;
        if (m.error) this.bell();
      },
      resume: (cmd) => this.execute(cmd),
      closed: () => this.afterCommand(),
    });
  }

  /** The seam `InsertMode` uses to reach the mode field, the change being recorded, `<C-o>`, etc. */
  private insertHost(): InsertHost {
    return {
      doc: this.doc,
      marks: this.marks,
      registers: this.shared.registers,
      ctx: () => this.ctx,
      readOnly: () => this.env.readOnly ?? false,
      bell: () => this.bell(),
      setCursor: (p) => this.setCursor(p),
      setMode: (mode) => {
        this.mode = mode;
      },
      noteInsertChange: (keys) => {
        if (this.changeInProgress) this.changeInProgress.insert = keys;
      },
      finishChange: () => this.finishChange(),
      enableOneShot: () => {
        this.oneShot = true;
      },
    };
  }

  /** Detaches from the document; an Insert session's open undo group is closed, not left behind. */
  dispose(): void {
    this.unsubscribe();
    if (this.doc.inUndoGroup) this.doc.endUndoGroup();
  }

  // ------------------------------------------------------------------ feeding

  /** One key, as typed. */
  feed(key: ViKey): void {
    this.queue.push({ key, replay: false });
    this.drain();
  }

  /** Keys in Vim notation (tests, macros): `m.feedKeys("ciwfoo<Esc>")`. */
  feedKeys(notation: string): void {
    for (const key of parseKeys(notation)) this.feed(key);
  }

  private replay(keys: readonly ViKey[]): void {
    this.queue.unshift(...keys.map((key) => ({ key, replay: true })));
  }

  /** `"%` for the machine at work: the one `Registers` is shared, the file name is each machine's own. */
  private readonly fileName = (): string => this.env.fileName ?? "";

  private drain(): void {
    this.shared.registers.readOnly["%"] = this.fileName;
    while (!this.blocked && this.queue.length > 0) {
      const next = this.queue.shift()!;
      if (!next.replay && this.recording) this.recording.keys.push(next.key);
      this.step(next.key);
    }
  }

  private step(key: ViKey): void {
    this.message = null;
    if (this.commands.active) {
      this.guard(() => this.commands.key(key), [key]);
      return;
    }
    if (this.mode === "insert" || this.mode === "replace") {
      this.guard(() => this.insertMode.key(key), [key]);
      return;
    }
    this.pending.push(key);
    const parsed = parse(this.pending, this.isVisual() ? "visual" : "normal", this.recording !== null);
    if (parsed.status === "incomplete") return;
    const keys = this.pending;
    this.pending = [];
    if (parsed.status === "invalid") {
      if (!isEscape(key)) this.bell();
      return;
    }
    this.guard(() => this.execute(parsed.command), keys);
  }

  /** Runs `work`; if it has to wait for the clipboard, puts `keys` back and runs them again later. */
  private guard(work: () => void, keys: ViKey[]): void {
    try {
      work();
      this.shared.registers.settle();
    } catch (err) {
      if (!(err instanceof ClipboardPending)) throw err;
      this.replay(keys);
      this.blocked = true;
      void err.text
        .catch(() => "")
        .then((text) => {
          this.shared.registers.provide(text);
          this.blocked = false;
          this.drain();
        });
    }
  }

  // ------------------------------------------------------------------ status

  isVisual(): boolean {
    return this.mode === "visual" || this.mode === "visualLine" || this.mode === "visualBlock";
  }

  /** The cursor's shape (V8). */
  cursorShape(): "block" | "bar" | "underline" {
    if (this.mode === "insert") return "bar";
    if (this.mode === "replace") return "underline";
    return "block";
  }

  /** What the mode pill shows (V8). */
  status(): ViStatus {
    return {
      mode: this.mode,
      pending: keysToText(this.pending),
      recording: this.recording?.register ?? null,
      cmdline: this.commands.status(),
      message: this.message,
    };
  }

  /** Whether the command line (`:`, `/`, `?`) is open: typed text goes there. */
  get inCommandLine(): boolean {
    return this.commands.active;
  }

  /** hlsearch and incsearch matches on lines `first`–`last`, for drawing. */
  searchHighlights(first: number, last: number): ReturnType<CommandMode["highlights"]> {
    return this.commands.highlights(first, last);
  }

  /** Where the view should scroll to: an incsearch match while one is showing, else the cursor. */
  revealTarget(): Pos {
    return this.commands.revealTarget() ?? this.cursor;
  }

  /** The Visual selection, for drawing; `null` outside Visual mode. */
  visualRegion(): Region | null {
    return this.isVisual() ? this.currentVisualRegion() : null;
  }

  /** The Visual selection now (only meaningful in a Visual mode). */
  private currentVisualRegion(): Region {
    const mode = this.mode as VisualMode;
    return visualRegion(this.store, mode, this.visualAnchor, this.cursor, this.blockToEnd, this.ctx.tabSize);
  }

  get cursor(): Pos {
    return this.doc.selection.head;
  }

  /**
   * A click: the cursor goes to `at` (on a character in Normal mode), leaving
   * Visual mode; a pending command is dropped. Insert mode stays Insert.
   */
  placeCursor(at: Pos): void {
    this.pending = [];
    if (this.isVisual()) this.mode = "normal";
    this.setCursor(at);
  }

  /** A mouse drag from `anchor` to `head`: characterwise Visual mode over it. */
  selectVisual(anchor: Pos, head: Pos): void {
    this.pending = [];
    if (this.mode === "insert" || this.mode === "replace") return;
    this.visualAnchor = clampNormal(this.store, anchor);
    if (!this.isVisual()) this.mode = "visual";
    this.setCursor(head, true);
  }

  // ------------------------------------------------------------------ execution

  private get ctx(): ViContext {
    return this.env.ctx();
  }

  private get store() {
    return this.doc.store;
  }

  private bell(): void {
    this.env.effect({ type: "bell" });
  }

  private setCursor(p: Pos, keepGoal = false): void {
    const at = this.mode === "insert" || this.mode === "replace" ? p : clampNormal(this.store, p);
    if (this.isVisual()) this.doc.setSelection({ anchor: this.visualAnchor, head: at }, true);
    else this.doc.setSelection(cursor(at), true);
    if (!keepGoal) this.goal = null;
  }

  private isChange(cmd: Parsed): boolean {
    const b = cmd.body;
    if (b.kind === "operator") return b.op !== "y";
    if (b.kind === "command") return CHANGE_COMMANDS.has(b.name);
    return false;
  }

  private execute(cmd: Parsed): void {
    const search = waitsForSearch(cmd);
    if (search) {
      // `/`, `?`, `d/` …: the command runs once its pattern is typed (`CommandMode.resume`).
      if (this.isChange(cmd) && this.env.readOnly) return this.bell();
      this.commands.open(search, "", cmd);
      return;
    }
    if (this.isVisual()) {
      this.executeVisual(cmd);
      return;
    }
    const change = this.isChange(cmd);
    if (change && this.env.readOnly) {
      this.bell();
      return;
    }
    if (change) {
      this.changeInProgress = { register: cmd.register, count: cmd.count, plain: cmd.plain, insert: [], visual: null };
      if (!this.doc.inUndoGroup) this.doc.beginUndoGroup();
    }
    this.executeNormal(cmd);
    // `:` opened the command line: Ctrl-o's way back to Insert waits until it closes.
    if (this.commands.active) return;
    if (this.mode !== "insert" && this.mode !== "replace") {
      if (change) this.finishChange();
      if (this.doc.inUndoGroup) this.doc.endUndoGroup();
      this.afterCommand();
    }
  }

  /** After a Normal command (or a command line) done from Insert with Ctrl-o: back to Insert. */
  private afterCommand(): void {
    if (!this.oneShot || this.mode === "insert" || this.mode === "replace") return;
    this.oneShot = false;
    this.insertMode.enter(this.cursor, 1, false);
  }

  private finishChange(): void {
    if (!this.changeInProgress) return;
    this.lastChange = this.changeInProgress;
    this.changeInProgress = null;
    this.marks.set(".", this.cursor);
  }

  private executeNormal(cmd: Parsed): void {
    const b = cmd.body;
    if (b.kind === "motion") {
      this.moveBy(b, cmd.count);
      return;
    }
    if (b.kind === "operator") {
      this.operatorOnTarget(b.op, b.target, cmd.count, cmd.register, b.char);
      return;
    }
    this.command(b, cmd);
  }

  /** A plain motion: the cursor moves (and a jump is remembered). */
  private moveBy(spec: { name: string; char?: string }, count: number | null): void {
    const from = this.cursor;
    const r = this.runMotion(spec, count, from);
    if (!r) {
      this.bell();
      return;
    }
    if (r.jump) this.pushJump(from);
    this.setCursor(landing(this.store, r), true);
    this.goal = r.goal ?? (isLinewiseMotion(spec.name) ? this.goal : null);
    if (r.goal !== undefined) this.goal = r.goal;
  }

  private runMotion(spec: { name: string; char?: string }, count: number | null, from: Pos): MotionResult | null {
    if (spec.name === "n" || spec.name === "N") return this.commands.searchMotion(spec.name === "N", count, from);
    if (spec.name === "*" || spec.name === "#") return this.commands.starMotion(spec.name === "#", count, from);
    if (spec.name === "f" || spec.name === "F" || spec.name === "t" || spec.name === "T") {
      if (spec.char) this.shared.lastFind = { kind: spec.name, char: spec.char };
    }
    return motion(this.doc, from, spec, count, this.ctx, {
      lastFind: this.shared.lastFind,
      mark: (name) => this.getMark(name),
      goal: this.goal,
    });
  }

  // ------------------------------------------------------------------ operators

  /** The region an operator covers from the cursor with `target`, or `null` if it cannot. */
  private regionFor(op: string, target: Target, count: number | null): Region | null {
    const from = this.cursor;
    const store = this.store;
    if (target.kind === "line") {
      const last = Math.min(store.lineCount() - 1, from.line + (count ?? 1) - 1);
      // `yss` surrounds the line's text, not the line (vim-surround).
      if (op === "ys") {
        const start = pos(from.line, firstNonBlank(store, from.line));
        return { kind: "char", start, end: pos(last, store.line(last).length) };
      }
      return { kind: "line", first: from.line, last };
    }
    if (target.kind === "object") {
      const obj = this.objectAt(from, target.name, target.inner, count ?? 1);
      if (!obj) return null;
      if (obj.linewise) return { kind: "line", first: obj.start.line, last: obj.end.line };
      return { kind: "char", start: obj.start, end: obj.end };
    }
    // `cw` on a word changes to its end, like `ce` (Vim's special case).
    let spec = { name: target.name, char: target.char };
    if (op === "c" && (spec.name === "w" || spec.name === "W")) {
      const under = store.line(from.line)[from.col];
      const onWord = under !== undefined && under !== " " && under !== "\t";
      if (onWord) spec = { name: spec.name === "w" ? "e" : "E", char: undefined };
    }
    const r = this.runMotion(spec, count, from);
    if (!r) return null;
    if (r.jump) this.pushJump(from);
    return this.motionRegion(from, r, spec.name);
  }

  /** A motion's result as a region, with Vim's rules for exclusive motions that end a line. */
  private motionRegion(from: Pos, r: MotionResult, name: string): Region {
    const store = this.store;
    const to = r.pos.col < 0 ? pos(r.pos.line, firstNonBlank(store, r.pos.line)) : r.pos;
    if (r.linewise) return { kind: "line", first: Math.min(from.line, to.line), last: Math.max(from.line, to.line) };
    let start = comparePos(from, to) <= 0 ? from : to;
    let end = comparePos(from, to) <= 0 ? to : from;
    if (r.inclusive) end = nextPos(store, end) ?? end;
    // `dw` on a line's last word stops at the line's end, not the next word.
    if ((name === "w" || name === "W") && end.line > start.line) {
      end = pos(end.line - 1, store.line(end.line - 1).length);
      if (comparePos(end, start) < 0) end = start;
    }
    if (!r.inclusive && end.col === 0 && end.line > start.line) {
      // An exclusive motion ending at a line's start ends the line before;
      // starting at or before the first non-blank, it takes whole lines.
      if (start.col <= firstNonBlank(store, start.line)) return { kind: "line", first: start.line, last: end.line - 1 };
      end = pos(end.line - 1, store.line(end.line - 1).length);
    }
    start = start.col < 0 ? pos(start.line, 0) : start;
    return { kind: "char", start, end };
  }

  private objectAt(at: Pos, name: string, inner: boolean, count: number): ObjectRange | null {
    if (isClassicObject(name)) return textObject(this.store, at, name, inner, count);
    return this.env.syntaxObjects?.(at, name, inner, count) ?? null;
  }

  private operatorOnTarget(
    op: string,
    target: Target,
    count: number | null,
    register: string | null,
    char?: string,
  ): void {
    const region = this.regionFor(op, target, count);
    if (!region) {
      this.changeInProgress = null;
      this.bell();
      return;
    }
    this.applyOperator(op, region, register, target.kind === "line" ? 1 : (count ?? 1), char);
  }

  /** Applies operator `op` to `region`. `levels` is how far `>`/`<` shift. */
  private applyOperator(op: string, region: Region, register: string | null, levels: number, char?: string): void {
    const ctx = this.ctx;
    const store = this.store;
    this.setMarks(region);
    const lines = regionLines(region);
    switch (op) {
      case "y": {
        this.shared.registers.put(register, regionContent(store, region, ctx.tabSize), "yank");
        // Yanking lines keeps the column; the cursor goes up only if they began above it.
        if (region.kind !== "line") this.setCursor(regionStart(store, region, ctx.tabSize));
        else if (this.cursor.line > region.first) this.setCursor(pos(region.first, this.cursor.col));
        return;
      }
      case "d":
        this.shared.registers.put(register, regionContent(store, region, ctx.tabSize), "delete");
        this.setCursor(deleteRegion(this.doc, region, ctx));
        return;
      case "c":
        this.change(region, register);
        return;
      case ">":
      case "<":
        shiftLines(this.doc, lines.first, lines.last, op === ">" ? 1 : -1, levels, ctx);
        this.setCursor(this.cursor);
        return;
      case "=":
        reindentLines(this.doc, lines.first, lines.last, ctx);
        this.setCursor(this.cursor);
        return;
      case "g~":
      case "gu":
      case "gU": {
        const how: CaseChange = op === "g~" ? "toggle" : op === "gu" ? "lower" : "upper";
        this.setCursor(caseRegion(this.doc, region, how, ctx));
        return;
      }
      case "gc":
        commentLines(this.doc, lines.first, lines.last, ctx);
        this.setCursor(this.cursor);
        return;
      case "ys":
        if (char) this.setCursor(surroundRegion(this.doc, region, char, ctx));
        return;
    }
  }

  /** `c`: the region goes (into the register) and Insert mode begins where it was. */
  private change(region: Region, register: string | null): void {
    const ctx = this.ctx;
    const store = this.store;
    this.shared.registers.put(register, regionContent(store, region, ctx.tabSize), "delete");
    if (region.kind === "line") {
      const indent = leadingWhitespace(store.line(region.first));
      const r = range(pos(region.first, 0), pos(region.last, store.line(region.last).length));
      this.doc.edit([{ range: r, text: indent }], cursor(pos(region.first, indent.length)), "other");
      this.insertMode.enter(pos(region.first, indent.length), 1, false);
      return;
    }
    if (region.kind === "block") {
      const at = deleteRegion(this.doc, region, ctx);
      const start = pos(region.first, blockCols(store.line(region.first), region, ctx.tabSize)[0]);
      this.insertMode.enter(start.col >= at.col ? start : at, 1, false);
      this.insertMode.setBlock({ first: region.first, last: region.last, column: region.left, append: false });
      return;
    }
    this.doc.edit([{ range: range(region.start, region.end), text: "" }], cursor(region.start), "other");
    this.insertMode.enter(region.start, 1, false);
  }

  private setMarks(region: Region): void {
    const store = this.store;
    if (region.kind === "char") {
      this.marks.set("[", region.start);
      this.marks.set("]", region.end);
    } else {
      this.marks.set("[", pos(region.first, 0));
      this.marks.set("]", pos(region.last, store.line(region.last).length));
    }
  }

  // ------------------------------------------------------------------ commands

  /** A Normal-mode command: tries each group in turn, and bells if none of them know it. */
  private command(b: Extract<Body, { kind: "command" }>, cmd: Parsed): void {
    if (this.env.readOnly && INSERTING.has(b.name)) {
      this.bell();
      return;
    }
    const handled =
      this.commandEdit(b, cmd) ||
      this.commandInsertEntry(b, cmd) ||
      this.commandScrollAndJump(b, cmd) ||
      this.commandRecordAndMark(b, cmd) ||
      this.commandEffect(b, cmd);
    if (!handled) this.bell();
  }

  /** Character/line edits, undo/redo, `~`, `r`, increment, and surround (`cs`/`ds`). */
  private commandEdit(b: Extract<Body, { kind: "command" }>, cmd: Parsed): boolean {
    const count = cmd.count;
    const n = count ?? 1;
    const reg = cmd.register;
    const store = this.store;
    const ctx = this.ctx;
    const at = this.cursor;
    const line = store.line(at.line);
    switch (b.name) {
      case "x":
      case "X": {
        if (line.length === 0) {
          this.bell();
          return true;
        }
        const region = b.name === "x" ? this.charsRight(at, n) : this.charsLeft(at, n);
        if (!region) this.bell();
        else this.applyOperator("d", region, reg, 1);
        return true;
      }
      case "s":
        this.change(this.charsRight(at, n) ?? { kind: "char", start: at, end: at }, reg);
        return true;
      case "S":
        this.change({ kind: "line", first: at.line, last: Math.min(store.lineCount() - 1, at.line + n - 1) }, reg);
        return true;
      case "C":
      case "D": {
        const last = Math.min(store.lineCount() - 1, at.line + n - 1);
        const region: Region = { kind: "char", start: at, end: pos(last, store.line(last).length) };
        if (b.name === "C") this.change(region, reg);
        else this.applyOperator("d", region, reg, 1);
        return true;
      }
      case "Y": {
        const last = Math.min(store.lineCount() - 1, at.line + n - 1);
        this.applyOperator("y", { kind: "line", first: at.line, last }, reg, 1);
        return true;
      }
      case "p":
      case "P":
      case "gp":
      case "gP":
        this.put(at, reg, n, b.name === "p" || b.name === "gp", b.name.startsWith("g"));
        return true;
      case "J":
      case "gJ":
        this.setCursor(joinLines(this.doc, at.line, count ?? 2, b.name === "J", ctx));
        return true;
      case "~": {
        const region = this.charsRight(at, n);
        if (!region || region.kind !== "char") {
          this.bell();
          return true;
        }
        caseRegion(this.doc, region, "toggle", ctx);
        this.setCursor(clampNormal(store, region.end));
        return true;
      }
      case "r":
        if (!b.char || !replaceChars(this.doc, at, b.char, n, ctx)) this.bell();
        else this.setCursor(this.cursor);
        return true;
      case "u":
        for (let i = 0; i < n; i++) if (!this.doc.undo()) break;
        this.setCursor(this.cursor);
        return true;
      case "<C-r>":
        for (let i = 0; i < n; i++) if (!this.doc.redo()) break;
        this.setCursor(this.cursor);
        return true;
      case "<C-a>":
      case "<C-x>":
        if (!incrementNumber(this.doc, at, b.name === "<C-a>" ? n : -n, ctx)) this.bell();
        return true;
      case "cs":
      case "ds":
        this.surroundCommand(b.name, b.char ?? "", b.char2 ?? "");
        return true;
      default:
        return false;
    }
  }

  /** Commands that start an Insert or Replace session. */
  private commandInsertEntry(b: Extract<Body, { kind: "command" }>, cmd: Parsed): boolean {
    const n = cmd.count ?? 1;
    const store = this.store;
    const at = this.cursor;
    const line = store.line(at.line);
    switch (b.name) {
      case "i":
        this.insertMode.enter(at, n, false);
        return true;
      case "a":
        this.insertMode.enter(line.length === 0 ? at : pos(at.line, nextGrapheme(line, at.col)), n, false);
        return true;
      case "I":
        this.insertMode.enter(pos(at.line, firstNonBlank(store, at.line)), n, false);
        return true;
      case "gI":
        this.insertMode.enter(pos(at.line, 0), n, false);
        return true;
      case "A":
        this.insertMode.enter(pos(at.line, line.length), n, false);
        return true;
      case "gi":
        this.insertMode.enter(this.marks.get("^") ?? at, n, false);
        return true;
      case "o":
      case "O":
        this.openLine(at, b.name === "o", n);
        return true;
      case "R":
        this.insertMode.enter(at, n, true);
        return true;
      default:
        return false;
    }
  }

  /** Jumps (`<C-o>`/`<C-i>`) and scrolling (`z*`, `<C-e>`/`<C-y>`/`<C-d>`/`<C-u>`/`<C-f>`/`<C-b>`). */
  private commandScrollAndJump(b: Extract<Body, { kind: "command" }>, cmd: Parsed): boolean {
    const count = cmd.count;
    const n = count ?? 1;
    switch (b.name) {
      case "<C-o>":
        this.jumpOlder(n);
        return true;
      case "<C-i>":
      case "<Tab>":
        this.jumpNewer(n);
        return true;
      case "zt":
      case "z<CR>":
      case "zz":
      case "z.":
      case "zb":
      case "z-":
        this.scrollTo(b.name, count);
        return true;
      case "<C-e>":
      case "<C-y>":
        this.scrollLines(b.name === "<C-e>" ? n : -n);
        return true;
      case "<C-d>":
      case "<C-u>":
        this.scrollHalf(b.name === "<C-d>" ? 1 : -1, count);
        return true;
      case "<C-f>":
      case "<C-b>":
      case "<PageDown>":
      case "<PageUp>":
        this.scrollPage(b.name === "<C-f>" || b.name === "<PageDown>" ? n : -n);
        return true;
      default:
        return false;
    }
  }

  /** `.`, entering Visual, `gv`, macro recording/playback, and setting a mark. */
  private commandRecordAndMark(b: Extract<Body, { kind: "command" }>, cmd: Parsed): boolean {
    const n = cmd.count ?? 1;
    switch (b.name) {
      case ".":
        this.repeatChange(cmd.count);
        return true;
      case "v":
      case "V":
      case "<C-v>":
        this.enterVisual(b.name === "v" ? "visual" : b.name === "V" ? "visualLine" : "visualBlock");
        return true;
      case "gv":
        this.reselect();
        return true;
      case "q":
        this.toggleRecording(b.char);
        return true;
      case "@":
        this.runMacro(b.char ?? "", n);
        return true;
      case "m":
        this.setMark(b.char ?? "", this.cursor);
        return true;
      default:
        return false;
    }
  }

  /** No-op escapes, file/diff effects (hunks, `gf`, quitting), and the ex/search/`&` lines. */
  private commandEffect(b: Extract<Body, { kind: "command" }>, cmd: Parsed): boolean {
    switch (b.name) {
      case "<Esc>":
      case "<C-[>":
      case "<C-c>":
        return true;
      case "]c":
      case "[c":
        this.env.effect({ type: "hunk", dir: b.name === "]c" ? 1 : -1 });
        return true;
      case "gf":
        this.env.effect({ type: "openFile" });
        return true;
      case "ZZ":
      case "ZQ":
        // A read-only view (a diff) is left the way it was opened, not with Vi's quit (V1).
        if (this.env.readOnly) return false;
        this.env.effect(b.name === "ZZ" ? { type: "saveQuit" } : { type: "quit", force: true });
        return true;
      case ":":
        this.commands.open(":", cmd.count ? `.,.+${cmd.count - 1}` : "");
        return true;
      case "&":
        this.commands.repeatSubstitute();
        return true;
      default:
        return false;
    }
  }

  /** `count` characters from the cursor to the right (not past the line's end), as a region. */
  private charsRight(at: Pos, count: number): Region | null {
    const line = this.store.line(at.line);
    if (at.col >= line.length) return null;
    let end = at.col;
    for (let i = 0; i < count && end < line.length; i++) end = nextGrapheme(line, end);
    return { kind: "char", start: at, end: pos(at.line, end) };
  }

  private charsLeft(at: Pos, count: number): Region | null {
    if (at.col === 0) return null;
    const line = this.store.line(at.line);
    let start = at.col;
    for (let i = 0; i < count && start > 0; i++) start = prevGrapheme(line, start);
    return { kind: "char", start: pos(at.line, start), end: at };
  }

  private put(at: Pos, register: string | null, count: number, after: boolean, cursorAfter: boolean): void {
    const content = this.shared.registers.get(register ?? '"');
    if (!content) return this.bell();
    this.setCursor(putRegister(this.doc, at, content, { after, count, cursorAfter }, this.ctx));
  }

  /** `o`/`O`: a new line below/above with the current line's indentation, and Insert mode on it. */
  private openLine(at: Pos, below: boolean, count: number): void {
    const store = this.store;
    const indent = leadingWhitespace(store.line(at.line));
    if (below) {
      const end = pos(at.line, store.line(at.line).length);
      const at2 = pos(at.line + 1, indent.length);
      this.doc.edit([{ range: range(end, end), text: `\n${indent}` }], cursor(at2), "other");
      this.insertMode.enter(at2, count, false);
    } else {
      const start = pos(at.line, 0);
      const at2 = pos(at.line, indent.length);
      this.doc.edit([{ range: range(start, start), text: `${indent}\n` }], cursor(at2), "other");
      this.insertMode.enter(at2, count, false);
    }
    // `3o` repeats the new line with its text on Esc.
    if (count > 1) this.insertMode.prependKey("<CR>");
  }

  // ------------------------------------------------------------------ visual

  private enterVisual(mode: ViMode): void {
    if (this.isVisual() && this.mode === mode) return this.leaveVisual();
    if (!this.isVisual()) this.visualAnchor = this.cursor;
    this.mode = mode;
    this.blockToEnd = false;
    this.setCursor(this.cursor, true);
  }

  private leaveVisual(): void {
    const region = this.currentVisualRegion();
    this.lastVisual = { mode: this.mode, anchor: this.visualAnchor, head: this.cursor };
    const start = regionStart(this.store, region, this.ctx.tabSize);
    const end = regionEnd(this.store, region);
    this.marks.set("<", start);
    this.marks.set(">", end);
    this.mode = "normal";
    this.setCursor(this.cursor);
  }

  private reselect(): void {
    if (!this.lastVisual) return this.bell();
    this.visualAnchor = clampNormal(this.store, this.lastVisual.anchor);
    this.mode = this.lastVisual.mode;
    this.setCursor(this.lastVisual.head, true);
  }


  private executeVisual(cmd: Parsed): void {
    const b = cmd.body;
    if (b.kind === "motion") {
      if (b.name === "$" && this.mode === "visualBlock") this.blockToEnd = true;
      this.moveBy(b, cmd.count);
      return;
    }
    const writes = b.kind === "operator" ? b.op !== "y" : VISUAL_WRITES.has(b.name);
    if (writes && this.env.readOnly) return this.bell();
    const region = this.currentVisualRegion();
    const size = visualSize(this.mode as VisualMode, region);
    if (writes) {
      this.changeInProgress = { register: cmd.register, count: cmd.count, plain: cmd.plain, insert: [], visual: size };
      if (!this.doc.inUndoGroup) this.doc.beginUndoGroup();
    }
    const handled = b.kind === "operator" ? this.visualOperator(b.op, region, cmd) : this.visualCommand(b, region, cmd);
    if (!handled) {
      this.changeInProgress = null;
      if (this.doc.inUndoGroup && this.mode !== "insert") this.doc.endUndoGroup();
      return this.bell();
    }
    if (this.mode !== "insert" && this.mode !== "replace") {
      if (writes) this.finishChange();
      if (this.doc.inUndoGroup) this.doc.endUndoGroup();
    }
  }

  private exitVisualFor(region: Region): void {
    this.lastVisual = { mode: this.mode, anchor: this.visualAnchor, head: this.cursor };
    this.marks.set("<", regionStart(this.store, region, this.ctx.tabSize));
    this.marks.set(">", regionEnd(this.store, region));
    this.mode = "normal";
  }

  private visualOperator(op: string, region: Region, cmd: Parsed): boolean {
    this.exitVisualFor(region);
    const levels = cmd.count ?? 1;
    if (op === "c" && region.kind === "block") {
      this.change(region, cmd.register);
      return true;
    }
    this.applyOperator(op, region, cmd.register, levels);
    return true;
  }

  /** A Visual-mode command: tries each group in turn; `false` bells (from `executeVisual`). */
  private visualCommand(b: Extract<Body, { kind: "command" }>, region: Region, cmd: Parsed): boolean {
    return (
      this.visualModeCommand(b, region) ||
      this.visualEditCommand(b, region, cmd) ||
      this.visualObjectAndEffect(b, region, cmd)
    );
  }

  /** Leaving Visual, switching its kind, swapping the anchor (`o`/`O`), and `gv` (a no-op here). */
  private visualModeCommand(b: Extract<Body, { kind: "command" }>, region: Region): boolean {
    switch (b.name) {
      case "<Esc>":
      case "<C-[>":
      case "<C-c>":
        this.leaveVisual();
        return true;
      case "v":
      case "V":
      case "<C-v>":
        this.enterVisual(b.name === "v" ? "visual" : b.name === "V" ? "visualLine" : "visualBlock");
        return true;
      case "o": {
        const head = this.cursor;
        this.setCursorRaw(this.visualAnchor);
        this.visualAnchor = head;
        return true;
      }
      case "O": {
        if (this.mode !== "visualBlock") return this.visualModeCommand({ kind: "command", name: "o" }, region);
        const a = this.visualAnchor;
        const h = this.cursor;
        this.visualAnchor = pos(a.line, h.col);
        this.setCursorRaw(pos(h.line, a.col));
        return true;
      }
      case "gv":
        return true;
      default:
        return false;
    }
  }

  /** Visual's edits: delete/change/replace/case/shift/put/increment, and surround (`S`). */
  private visualEditCommand(b: Extract<Body, { kind: "command" }>, region: Region, cmd: Parsed): boolean {
    const ctx = this.ctx;
    const reg = cmd.register;
    const whole = wholeLines(region);
    switch (b.name) {
      case "x":
      case "d":
        this.visualOperator("d", region, cmd);
        return true;
      case "X":
      case "D":
        this.exitVisualFor(region);
        if (b.name === "D" && region.kind === "block") this.applyOperator("d", { ...region, right: Infinity }, reg, 1);
        else this.applyOperator("d", whole, reg, 1);
        return true;
      case "Y":
        this.exitVisualFor(region);
        this.applyOperator("y", whole, reg, 1);
        return true;
      case "s":
        this.exitVisualFor(region);
        this.change(region, reg);
        return true;
      case "C":
      case "R":
        this.exitVisualFor(region);
        if (b.name === "C" && region.kind === "block") this.change({ ...region, right: Infinity }, reg);
        else this.change(whole, reg);
        return true;
      case "S":
        // Visual `S{char}` surrounds the selection (vim-surround, V11).
        if (!b.char) return false;
        this.exitVisualFor(region);
        this.setCursor(surroundRegion(this.doc, region, b.char, ctx));
        return true;
      case "J":
      case "gJ": {
        this.exitVisualFor(region);
        const lines = regionLines(region);
        const count = Math.max(2, lines.last - lines.first + 1);
        this.setCursor(joinLines(this.doc, lines.first, count, b.name === "J", ctx));
        return true;
      }
      case "u":
      case "U":
      case "~": {
        this.exitVisualFor(region);
        const how: CaseChange = b.name === "u" ? "lower" : b.name === "U" ? "upper" : "toggle";
        this.setCursor(caseRegion(this.doc, region, how, ctx));
        return true;
      }
      case "r":
        if (!b.char) return false;
        this.exitVisualFor(region);
        this.setCursor(replaceRegion(this.doc, region, b.char, ctx));
        return true;
      case "p":
      case "P":
        return this.visualPut(region, cmd, b.name === "p");
      case ">":
      case "<":
      case "=":
        return this.visualOperator(b.name, region, cmd);
      case "I":
      case "A":
        return this.visualInsert(region, b.name === "A");
      case "<C-a>":
      case "<C-x>": {
        this.exitVisualFor(region);
        const lines = regionLines(region);
        for (let l = lines.first; l <= lines.last; l++) {
          incrementNumber(this.doc, pos(l, 0), (b.name === "<C-a>" ? 1 : -1) * (cmd.count ?? 1), ctx);
        }
        this.setCursor(pos(lines.first, firstNonBlank(this.store, lines.first)));
        return true;
      }
      default:
        return false;
    }
  }

  /** Visual text objects (`vi`/`va`), the ex line from a selection, search, and scrolling. */
  private visualObjectAndEffect(b: Extract<Body, { kind: "command" }>, region: Region, cmd: Parsed): boolean {
    const store = this.store;
    switch (b.name) {
      case "vi":
      case "va": {
        const obj = this.objectAt(this.cursor, b.char ?? "", b.name === "vi", cmd.count ?? 1);
        if (!obj) return false;
        if (obj.linewise && this.mode === "visual") this.mode = "visualLine";
        this.visualAnchor = obj.start;
        const end = obj.linewise ? pos(obj.end.line, 0) : obj.end;
        const last = comparePos(end, obj.start) > 0 ? prevChar(store, end) : obj.start;
        this.setCursorRaw(last);
        return true;
      }
      case ":":
        this.exitVisualFor(region);
        this.setCursor(this.cursor);
        this.commands.open(":", "'<,'>");
        return true;
      case "zt":
      case "zz":
      case "zb":
        this.scrollTo(b.name, null);
        return true;
      case "<C-e>":
      case "<C-y>":
        this.scrollLines(b.name === "<C-e>" ? cmd.count ?? 1 : -(cmd.count ?? 1));
        return true;
      case "<C-d>":
      case "<C-u>":
        this.scrollHalf(b.name === "<C-d>" ? 1 : -1, cmd.count);
        return true;
      case "<C-f>":
      case "<C-b>":
        this.scrollPage(b.name === "<C-f>" ? cmd.count ?? 1 : -(cmd.count ?? 1));
        return true;
      default:
        return false;
    }
  }

  /**
   * Visual `p`/`P`: the selection is replaced by the register. With `p` the
   * replaced text goes to the unnamed register (so the next `p` pastes it);
   * `P` leaves the registers alone. Lines are replaced by lines; characters
   * pasted over lines go on a line of their own.
   */
  private visualPut(region: Region, cmd: Parsed, swap: boolean): boolean {
    const store = this.store;
    const ctx = this.ctx;
    const content = this.shared.registers.get(cmd.register ?? '"');
    if (!content) return false;
    this.exitVisualFor(region);
    const replaced = regionContent(store, region, ctx.tabSize);
    const count = cmd.count ?? 1;
    const at = deleteRegion(this.doc, region, ctx);
    if (region.kind === "line") {
      const lines = content.kind === "line" ? content : { text: `${content.text}\n`, kind: "line" as const };
      // The lines went from the end of the text: put after what is now the last line.
      const atEnd = region.first >= store.lineCount();
      const target = pos(Math.min(region.first, store.lineCount() - 1), 0);
      this.setCursor(putRegister(this.doc, target, lines, { after: atEnd, count, cursorAfter: false }, ctx));
    } else {
      // At the region's start as it was — `deleteRegion` clamps its answer for Normal mode.
      const where = region.kind === "char" ? region.start : at;
      this.setCursor(putRegister(this.doc, where, content, { after: false, count, cursorAfter: false }, ctx));
    }
    if (swap) this.shared.registers.put(null, replaced, "delete");
    return true;
  }

  /** Visual `I`/`A`: a block inserts on every line (on Esc); otherwise at the selection's start or end. */
  private visualInsert(region: Region, append: boolean): boolean {
    const store = this.store;
    const ctx = this.ctx;
    this.exitVisualFor(region);
    if (region.kind === "block") {
      const column = append ? (region.right === Infinity ? Infinity : region.right + 1) : region.left;
      const first = store.line(region.first);
      const width = displayColumn(first, first.length, ctx.tabSize);
      let at: Pos;
      if (append && column === Infinity) at = pos(region.first, first.length);
      else if (append && width < column) {
        const pad = " ".repeat(column - width);
        const end = pos(region.first, first.length);
        this.doc.edit([{ range: range(end, end), text: pad }], cursor(end), "other");
        at = pos(region.first, first.length + pad.length);
      } else at = pos(region.first, blockCols(first, { left: column, right: column }, ctx.tabSize)[0]);
      this.insertMode.enter(at, 1, false);
      this.insertMode.setBlock({ first: region.first, last: region.last, column, append });
      return true;
    }
    const at = append ? regionEnd(store, region) : regionStart(store, region, ctx.tabSize);
    const lineLength = store.line(at.line).length;
    if (!append) this.insertMode.enter(at, 1, false);
    else {
      const insertAt = pos(at.line, region.kind === "char" ? Math.min(lineLength, at.col) : lineLength);
      this.insertMode.enter(insertAt, 1, false);
    }
    return true;
  }

  private setCursorRaw(p: Pos): void {
    this.doc.setSelection({ anchor: this.visualAnchor, head: clampNormal(this.store, p) }, true);
  }

  // ------------------------------------------------------------------ repeat, macros, marks, jumps

  private repeatChange(count: number | null): void {
    const change = this.lastChange;
    if (!change) return this.bell();
    const keys: ViKey[] = [];
    if (change.register) keys.push('"', change.register);
    const n = count ?? change.count;
    if (n !== null) keys.push(...String(n).split(""));
    keys.push(...change.plain, ...change.insert);
    if (change.visual) this.selectLike(change.visual);
    this.replay(keys);
  }

  /** Selects, from the cursor, a region of the size a Visual change had (for `.`). */
  private selectLike(size: VisualSize): void {
    this.visualAnchor = this.cursor;
    this.mode = size.mode;
    this.setCursorRaw(headForSize(this.store, this.cursor, size));
  }

  private toggleRecording(register: string | undefined): void {
    if (this.recording) {
      const keys = this.recording.keys.slice(0, -1); // without the closing `q`
      this.shared.registers.set(this.recording.register, { text: keysToText(keys), kind: "char" });
      this.recording = null;
      return;
    }
    if (!register || !/^[a-zA-Z0-9"]$/.test(register)) return this.bell();
    this.recording = { register, keys: [] };
  }

  private runMacro(register: string, count: number): void {
    const name = register === "@" ? this.shared.lastMacro : register;
    if (!name) return this.bell();
    if (name === ":") {
      this.shared.lastMacro = ":";
      for (let i = 0; i < count; i++) this.commands.repeatEx();
      return;
    }
    const content = this.shared.registers.get(name);
    if (!content) return this.bell();
    this.shared.lastMacro = name;
    const keys = parseKeys(content.text);
    const all: ViKey[] = [];
    for (let i = 0; i < count; i++) all.push(...keys);
    this.replay(all);
  }

  private setMark(name: string, at: Pos): void {
    if (/^[a-z'`[\]<>]$/.test(name)) {
      this.marks.set(name, at);
      return;
    }
    if (/^[A-Z]$/.test(name) && this.env.fileKey) {
      this.shared.fileMarks.set(name, { file: this.env.fileKey, at });
      return;
    }
    this.bell();
  }

  /** A mark's position here, or `null`. A file mark in another file asks the view to open it. */
  private getMark(name: string): Pos | null {
    if (/^[A-Z]$/.test(name)) {
      const mark = this.shared.fileMarks.get(name);
      if (!mark) return null;
      if (mark.file === this.env.fileKey) return mark.at;
      // A read-only view cannot open another file; the motion fails and rings.
      if (!this.env.readOnly) this.env.effect({ type: "fileMark", file: mark.file, at: mark.at });
      return null;
    }
    return this.marks.get(name);
  }

  private pushJump(from: Pos): void {
    this.marks.pushJump(from);
  }

  private jumpOlder(n: number): void {
    const target = this.marks.older(n, this.cursor);
    if (target) this.setCursor(target);
    else this.bell();
  }

  private jumpNewer(n: number): void {
    const target = this.marks.newer(n);
    if (target) this.setCursor(target);
    else this.bell();
  }

  // ------------------------------------------------------------------ scrolling

  private scrollTo(name: string, count: number | null): void {
    const line = count !== null ? Math.min(this.store.lineCount() - 1, count - 1) : this.cursor.line;
    const to = name === "zt" || name === "z<CR>" ? "top" : name === "zz" || name === "z." ? "center" : "bottom";
    if (name.length > 2 || name === "z." || name === "z-") this.setCursor(pos(line, firstNonBlank(this.store, line)));
    else if (count !== null) this.setCursor(pos(line, this.cursor.col));
    this.env.effect({ type: "scroll", line, to });
  }

  /** Ctrl-e / Ctrl-y: the view moves; the cursor only if it would leave the screen. */
  private scrollLines(delta: number): void {
    const view = this.ctx.viewport;
    this.env.effect({ type: "scrollLines", delta });
    const top = Math.max(0, view.top + delta);
    const bottom = Math.min(this.store.lineCount() - 1, view.bottom + delta);
    const line = Math.max(top, Math.min(bottom, this.cursor.line));
    if (line !== this.cursor.line) this.setCursor(pos(line, this.cursor.col), true);
  }

  /** Ctrl-d / Ctrl-u: half a screen, cursor and view together (a count sets how far). */
  private scrollHalf(dir: 1 | -1, count: number | null): void {
    const view = this.ctx.viewport;
    const amount = count ?? Math.max(1, Math.floor((view.bottom - view.top + 1) / 2));
    this.moveLinesAndScroll(dir * amount);
  }

  private scrollPage(pages: number): void {
    const view = this.ctx.viewport;
    const page = Math.max(1, view.bottom - view.top - 1);
    this.moveLinesAndScroll(pages * page);
  }

  private moveLinesAndScroll(delta: number): void {
    const store = this.store;
    const line = Math.max(0, Math.min(store.lineCount() - 1, this.cursor.line + delta));
    if (line === this.cursor.line) return this.bell();
    this.env.effect({ type: "scrollLines", delta });
    this.setCursor(pos(line, firstNonBlank(store, line)));
  }

  // ------------------------------------------------------------------ surround commands

  /** `cs{old}{new}` / `ds{char}`: the nearest pair of `old` around the cursor, changed or removed (V11). */
  private surroundCommand(name: "cs" | "ds" | string, old: string, next: string): void {
    const store = this.store;
    const objName = old === "b" ? "(" : old === "B" ? "{" : old === "r" ? "[" : old === "a" ? "<" : old;
    const inner = textObject(store, this.cursor, objName, true, 1);
    // A quote's `a"` takes blanks too, so around a quote is its inside plus one character each side.
    const outer = /["'`]/.test(objName)
      ? inner && { start: pos(inner.start.line, inner.start.col - 1), end: pos(inner.end.line, inner.end.col + 1) }
      : textObject(store, this.cursor, objName, false, 1);
    if (!inner || !outer || inner.linewise) return this.bell();
    const openEnd = pos(outer.start.line, outer.start.col + 1);
    const closeStart = pos(outer.end.line, outer.end.col - 1);
    // `ds(` also takes the padding a `(` surround added; `ds)` only the brackets.
    const padded = "([{".includes(old);
    const charAtPos = (p: Pos) => store.line(p.line)[p.col];
    const openTo = padded && charAtPos(openEnd) === " " ? pos(openEnd.line, openEnd.col + 1) : openEnd;
    const beforeClose = pos(closeStart.line, closeStart.col - 1);
    const closeFrom = padded && closeStart.col > 0 && charAtPos(beforeClose) === " " ? beforeClose : closeStart;
    const [open, close] = name === "cs" ? surroundPair(next) : ["", ""];
    this.doc.edit(
      [
        { range: range(closeFrom, outer.end), text: close },
        { range: range(outer.start, openTo), text: open },
      ],
      cursor(outer.start),
      "other",
    );
    this.setCursor(outer.start);
  }
}

/** Visual commands that change the text (for `.` and read-only surfaces). */
const VISUAL_WRITES = new Set([
  "x", "d", "X", "D", "s", "C", "S", "R", "J", "gJ", "u", "U", "~", "r", "p", "P", ">", "<", "=", "I", "A", "<C-a>",
  "<C-x>",
]);
