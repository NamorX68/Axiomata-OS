/**
 * The command line at work (`docs/plans/editor.md`, ED3, V6, V7): `/` and `?`
 * with incsearch, `n N * #`, hlsearch, and the `:` commands — kept out of the
 * machine the way Insert mode is (`insert.ts`), behind a small host.
 *
 * The line lives inside the machine, not the view: its keys go through the
 * same queue as every other key, so a macro that runs `:s/a/b/<CR>` and a `.`
 * that repeats `d/foo<CR>` replay it exactly.
 */

import type { EditorDocument } from "../document";
import { cursor, pos, range as makeRange, type Pos, type Range } from "../position";
import { CommandLine, type CmdlineKind } from "./cmdline";
import { completeEx, parseEx, type LineRange, type SetOption } from "./ex";
import type { ViKey } from "./keys";
import type { MotionResult } from "./motions";
import type { Parsed } from "./parse";
import type { Registers } from "./registers";
import { firstNonBlank } from "./scan";
import { compilePattern, findMatch, isWordChar, matchesIn, wordPattern, type SearchState } from "./search";
import { parseSubstitute, substituteLines, type Substitution, type SubstituteSpec } from "./substitute";

/** How many entries each history keeps (Vim's default `history`). */
const HISTORY_SIZE = 100;
/** Vim's `report`: more changed lines than this and `:s` says how many. */
const REPORT_LINES = 2;
/** What a read-only view (a diff, V1) answers to `:q`, `:e!` and `:set` — it is left and set up its own way. */
const NOT_HERE = "Not available in a read-only view";

/** What every document's command line shares: the last search and substitution, the histories. */
export class SearchMemory {
  last: SearchState | null = null;
  /** hlsearch: on after every search, off after `:noh` until the next one. */
  highlight = true;
  substitute: SubstituteSpec | null = null;
  /** The last replacement, for `~` in the next one. */
  replacement = "";
  /** The last `:` line run, for `@:` and `":`. */
  lastEx: string | null = null;
  readonly history: { cmd: string[]; search: string[] } = { cmd: [], search: [] };

  /** Puts `text` last in a history (moving it there if it was already in), dropping the oldest past the cap. */
  remember(which: "cmd" | "search", text: string): void {
    if (!text) return;
    const list = this.history[which];
    const at = list.indexOf(text);
    if (at >= 0) list.splice(at, 1);
    list.push(text);
    if (list.length > HISTORY_SIZE) list.shift();
  }
}

/** A line for the status bar: an error in the danger colour, anything else plain. */
export interface ViMessage {
  text: string;
  error: boolean;
}

/** The command line as the pill shows it. */
export interface CmdlineStatus {
  kind: CmdlineKind;
  text: string;
  cursor: number;
  /** `<C-r>` is waiting for a register name. */
  register: boolean;
}

/** What `:` and search ask the view to do. */
export type CommandEffect =
  | { type: "save" }
  | { type: "quit"; force: boolean }
  | { type: "saveQuit" }
  | { type: "reload" }
  | { type: "set"; option: SetOption; value: boolean | "toggle" };

export interface CommandHost {
  doc: EditorDocument;
  memory: SearchMemory;
  registers: Registers;
  readOnly(): boolean;
  cursor(): Pos;
  /** The cursor to `at` after a jump (it goes on the jump list). */
  jumpTo(at: Pos): void;
  mark(name: string): Pos | null;
  effect(effect: CommandEffect): void;
  message(message: ViMessage): void;
  /** A command that waited for its search pattern, to run now. */
  resume(cmd: Parsed): void;
  /** The line closed without resuming a command (Ctrl-o goes back to Insert). */
  closed(): void;
}

/** Whether `cmd` moves by a pattern still to be typed: `/`, `?`, `d/`, `c?` … */
export function waitsForSearch(cmd: Parsed): "/" | "?" | null {
  const b = cmd.body;
  const target = b.kind === "operator" && b.target.kind === "motion" ? b.target.name : null;
  const name = b.kind === "motion" ? b.name : target;
  return name === "/" || name === "?" ? name : null;
}

/** `cmd` with its search target replaced by "the last search" (`n`), and the pattern in its keys for `.`. */
function resolved(cmd: Parsed, pattern: string): Parsed {
  const b = cmd.body;
  const body =
    b.kind === "motion"
      ? { ...b, name: "n" }
      : b.kind === "operator"
        ? { ...b, target: { kind: "motion" as const, name: "n" } }
        : b;
  const plain: ViKey[] = [...cmd.plain, { text: pattern }, "<CR>"];
  return { ...cmd, body, plain };
}

export class CommandMode {
  private line: CommandLine | null = null;
  /** The command a `/` or `?` belongs to (`d/`), run once the pattern is typed. */
  private waiting: Parsed | null = null;
  /** Where the cursor was when a search line opened: incsearch searches from here. */
  private origin: Pos = pos(0, 0);
  /** incsearch: the match the typed pattern would go to. */
  private preview: Range | null = null;
  private compiled: { pattern: string; re: RegExp | null } | null = null;

  constructor(private readonly host: CommandHost) {}

  /** Whether the line is open: every key goes to it. */
  get active(): boolean {
    return this.line !== null;
  }

  /** Opens the line for `:`, `/` or `?`; `waiting` is the command a search belongs to (`d/`). */
  open(kind: CmdlineKind, initial = "", waiting: Parsed | null = null): void {
    const memory = this.host.memory;
    const history = kind === ":" ? memory.history.cmd : memory.history.search;
    this.line = new CommandLine(kind, initial, history, kind === ":" ? completeEx : undefined);
    this.waiting = waiting;
    this.origin = this.host.cursor();
    this.preview = null;
  }

  /** The line for the pill, `null` while it is closed. */
  status(): CmdlineStatus | null {
    const line = this.line;
    return line ? { kind: line.kind, text: line.text, cursor: line.cursor, register: line.pendingRegister } : null;
  }

  /** A key while the line is open. May throw `ClipboardPending` (`<C-r>+`); the machine replays the key. */
  key(key: ViKey): void {
    const line = this.line!;
    const result = line.key(key);
    switch (result.kind) {
      case "edited":
        this.updatePreview();
        return;
      case "register": {
        let text: string;
        try {
          text = this.host.registers.get(result.name)?.text ?? "";
        } catch (err) {
          line.expectRegister();
          throw err;
        }
        line.insert(text);
        this.updatePreview();
        return;
      }
      case "cancel":
        this.close();
        this.waiting = null;
        this.host.closed();
        return;
      case "submit":
        this.close();
        if (line.kind === ":") this.submitEx(result.text);
        else this.submitSearch(line.kind, result.text);
    }
  }

  private close(): void {
    this.line = null;
    this.preview = null;
  }

  // ---------------------------------------------------------------- search

  private updatePreview(): void {
    const line = this.line;
    if (!line || line.kind === ":" || line.text === "") {
      this.preview = null;
      return;
    }
    const re = this.regexFor(line.text);
    const count = this.waiting?.count ?? 1;
    const found = re ? findMatch(this.host.doc.store, re, this.origin, line.kind === "?", count) : null;
    this.preview = found?.range ?? null;
  }

  /** A compiled pattern, kept while the same pattern is asked for again (every redraw asks). */
  private regexFor(pattern: string): RegExp | null {
    if (this.compiled?.pattern !== pattern) {
      const c = compilePattern(pattern);
      this.compiled = { pattern, re: "error" in c ? null : c.re };
    }
    return this.compiled.re;
  }

  private submitSearch(kind: "/" | "?", typed: string): void {
    const memory = this.host.memory;
    const waiting = this.waiting;
    this.waiting = null;
    const pattern = typed || memory.last?.pattern;
    if (!pattern) {
      this.error("E35: No previous regular expression");
      return this.host.closed();
    }
    const c = compilePattern(pattern);
    if ("error" in c) {
      this.error(`E486: ${c.error}`);
      return this.host.closed();
    }
    memory.remember("search", typed);
    memory.last = { pattern, backward: kind === "?" };
    memory.highlight = true;
    if (waiting) this.host.resume(resolved(waiting, pattern));
    else this.host.closed();
  }

  /** `n` (`reverse` false) and `N`, as a motion an operator can use too. */
  searchMotion(reverse: boolean, count: number | null, from: Pos): MotionResult | null {
    const last = this.host.memory.last;
    if (!last) {
      this.error("E35: No previous regular expression");
      return null;
    }
    const re = this.regexFor(last.pattern);
    if (!re) {
      this.error(`E486: Pattern not found: ${last.pattern}`);
      return null;
    }
    const backward = last.backward !== reverse;
    const found = findMatch(this.host.doc.store, re, from, backward, count ?? 1);
    this.host.memory.highlight = true;
    if (!found) {
      this.error(`E486: Pattern not found: ${last.pattern}`);
      return null;
    }
    if (found.wrapped) {
      const text = backward ? "search hit TOP, continuing at BOTTOM" : "search hit BOTTOM, continuing at TOP";
      this.host.message({ text, error: false });
    }
    return { pos: found.range.start, linewise: false, inclusive: false, jump: true };
  }

  /** `*` and `#`: the word under (or after) the cursor, as a whole word. */
  starMotion(backward: boolean, count: number | null, from: Pos): MotionResult | null {
    const text = this.host.doc.store.line(from.line);
    const word = wordAt(text, from.col);
    if (!word) {
      this.error("E348: No string under cursor");
      return null;
    }
    const pattern = wordPattern(text.slice(word.start, word.end));
    this.host.memory.remember("search", pattern);
    this.host.memory.last = { pattern, backward };
    return this.searchMotion(false, count, pos(from.line, word.start));
  }

  /** hlsearch and incsearch for lines `first`–`last`: every match, and the one a search would go to. */
  highlights(first: number, last: number): { matches: Map<number, Array<[number, number]>>; current: Range | null } {
    const memory = this.host.memory;
    const typing = this.line && this.line.kind !== ":" && this.line.text ? this.line.text : null;
    const pattern = typing ?? (memory.highlight ? memory.last?.pattern : null);
    const re = pattern ? this.regexFor(pattern) : null;
    const matches = re ? matchesIn(this.host.doc.store, re, first, last) : new Map<number, Array<[number, number]>>();
    return { matches, current: this.preview };
  }

  /** Where the view should look: the incsearch match while typing, else `null` (the cursor). */
  revealTarget(): Pos | null {
    return this.preview?.start ?? null;
  }

  // ---------------------------------------------------------------- ex

  private submitEx(text: string): void {
    this.host.memory.remember("cmd", text);
    this.runEx(text);
    this.host.closed();
  }

  /** `@:`: the last `:` line again. */
  repeatEx(): void {
    const last = this.host.memory.lastEx;
    if (!last) return this.error("E30: No previous command line");
    this.runEx(last);
  }

  /** `&`: the last `:s` on the cursor's line, without its flags. */
  repeatSubstitute(): void {
    const line = this.host.cursor().line;
    this.runSubstitute({ first: line, last: line }, parseSubstitute("", this.host.memory.substitute));
  }

  private runEx(text: string): void {
    const host = this.host;
    host.memory.lastEx = text;
    const doc = host.doc;
    const cmd = parseEx(text, {
      line: host.cursor().line,
      lineCount: doc.store.lineCount(),
      mark: (name) => host.mark(name)?.line ?? null,
    });
    if ("error" in cmd) {
      if (cmd.error) this.error(cmd.error);
      return;
    }
    switch (cmd.name) {
      case "write":
        if (host.readOnly()) return this.error("E45: 'readonly' option is set (add ! to override)");
        if (!cmd.quit) return host.effect({ type: "save" });
        return host.effect(cmd.onlyIfChanged && !doc.dirty ? { type: "quit", force: false } : { type: "saveQuit" });
      case "quit":
        if (host.readOnly()) return this.error(NOT_HERE);
        if (!cmd.force && doc.dirty) return this.error("E37: No write since last change (add ! to override)");
        return host.effect({ type: "quit", force: cmd.force });
      case "reload":
        return host.readOnly() ? this.error(NOT_HERE) : host.effect({ type: "reload" });
      case "goto":
        return host.jumpTo(pos(cmd.line, firstNonBlank(doc.store, cmd.line)));
      case "substitute":
        return this.runSubstitute(cmd.range, parseSubstitute(cmd.args, host.memory.substitute));
      case "repeatSubstitute": {
        const last = host.memory.substitute;
        return this.runSubstitute(cmd.range, cmd.keepFlags ? last : parseSubstitute("", last));
      }
      case "nohlsearch":
        host.memory.highlight = false;
        return;
      case "set":
        if (host.readOnly()) return this.error(NOT_HERE);
        return host.effect({ type: "set", option: cmd.option, value: cmd.value });
    }
  }

  private runSubstitute(range: LineRange, parsed: SubstituteSpec | { error: string } | null): void {
    const host = this.host;
    const memory = host.memory;
    if (parsed === null) return this.error("E35: No previous regular expression");
    if ("error" in parsed) return this.error(parsed.error);
    if (host.readOnly()) return this.error("E21: Cannot make changes, 'modifiable' is off");
    const pattern = parsed.pattern || memory.last?.pattern || memory.substitute?.pattern;
    if (!pattern) return this.error("E35: No previous regular expression");
    const spec = { ...parsed, pattern };
    const result = substituteLines(host.doc.store, range, spec, memory.replacement);
    // The pattern is the last search now (`n` finds it), whether or not it matched.
    memory.last = { pattern, backward: memory.last?.backward ?? false };
    memory.highlight = true;
    memory.substitute = spec;
    if ("error" in result) return this.error(result.error);
    memory.replacement = spec.replacement;
    this.applySubstitution(result);
  }

  /** Writes a successful `:s`'s changed lines and moves the cursor to the last one made. */
  private applySubstitution(result: Substitution): void {
    const host = this.host;
    const store = host.doc.store;
    const changes = result.lines.map(({ line, text }) => ({
      range: makeRange(pos(line, 0), pos(line, store.line(line).length)),
      text,
    }));
    // Line breaks put in push the last changed line down; the cursor goes to the last line made.
    const shift = result.lines
      .filter((l) => l.line <= result.lastLine)
      .reduce((n, l) => n + (l.text.match(/\n/g)?.length ?? 0), 0);
    const target = result.lastLine + shift;
    host.doc.edit(changes, cursor(host.cursor()), "other");
    host.doc.setSelection(cursor(pos(target, firstNonBlank(store, target))));
    if (result.lines.length > REPORT_LINES) {
      host.message({ text: `${result.count} substitutions on ${result.lines.length} lines`, error: false });
    }
  }

  private error(text: string): void {
    this.host.message({ text, error: true });
  }
}

/** The word `*` searches for: under the cursor, else the next one on the line. */
function wordAt(text: string, col: number): { start: number; end: number } | null {
  let i = col;
  while (i < text.length && !isWordChar(text[i])) i++;
  if (i >= text.length) return null;
  let start = i;
  while (start > 0 && isWordChar(text[start - 1])) start--;
  let end = i;
  while (end < text.length && isWordChar(text[end])) end++;
  return { start, end };
}
