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
import type { Rope } from "../rope";
import type { SearchGuard, Verdict } from "../search/guard";
import { jumpTo, matchAround, matchesOnLines } from "../search/jump";
import { allMatches, spansLines } from "../search/matches";
import { CommandLine, type CmdlineKind } from "./cmdline";
import { completeEx, parseEx, type LineRange, type SetOption } from "./ex";
import type { ViKey } from "./keys";
import type { MotionResult } from "./motions";
import type { Parsed } from "./parse";
import type { Registers } from "./registers";
import { firstNonBlank } from "./scan";
import {
  compilePattern,
  findMatch,
  isWordChar,
  matchesIn,
  wordPattern,
  type CaseMode,
  type SearchState,
} from "./search";
import { ConfirmSubstitute } from "./confirm";
import {
  parseSubstitute,
  substituteLines,
  substituteSpanning,
  type Substitution,
  type SubstituteSpec,
} from "./substitute";
import { LineAnchors, markedLines } from "./global";

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
  /** Vouches for a pattern before it runs (ED5, T5); `null` runs it straight away. */
  guard(): SearchGuard | null;
  /** The cursor to `at`, without a jump — `:g` visiting its lines. */
  moveTo(at: Pos): void;
  /** `:normal`: runs `keys` in Normal mode from the cursor, ending whatever they leave half done. */
  normal(keys: string): void;
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
  /** incsearch: the match the typed pattern goes to, worked out once per pattern and text. */
  private preview: { pattern: string; rope: Rope; range: Range | null } | null = null;
  private compiled: { pattern: string; re: RegExp | null } | null = null;
  /** `:s///c` asking at each match (T12): every key answers it. */
  private confirm: ConfirmSubstitute | null = null;
  /** Inside `:g`: a nested `:g` is refused, and each line's "not found" stays quiet. */
  private global: { errors: number; last: string | null } | null = null;
  /**
   * Inside a loop (`:g`, a ranged `:normal`, `3@:`): its patterns were vouched
   * for before it started, and nothing waits inside it (`guard.ts`'s header).
   */
  private looping = 0;

  constructor(private readonly host: CommandHost) {}

  /** Whether the line is open, or `:s///c` is asking: every key goes here. */
  get active(): boolean {
    return this.line !== null || this.confirm !== null;
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
    if (this.confirm) {
      this.confirm.key(key);
      if (this.confirm.done) this.confirm = null;
      return;
    }
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
        // Before anything closes: a pattern still waiting for its verdict
        // throws here, and the machine replays this <CR> once it is in.
        this.vouchFor(line.kind, result.text);
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

  /** A typed key changed the pattern: the incsearch match is worked out again when next drawn. */
  private updatePreview(): void {
    this.preview = null;
  }

  /** incsearch's match for the pattern being typed; nothing while its verdict is on its way. */
  private currentPreview(): Range | null {
    const line = this.line;
    if (!line || line.kind === ":" || line.text === "") return null;
    const rope = this.host.doc.store.snapshot();
    if (this.preview?.pattern === line.text && this.preview.rope === rope) return this.preview.range;
    const re = this.regexFor(line.text);
    const verdict = re ? this.peek(re) : null;
    if (verdict === "waiting") return null;
    const count = this.waiting?.count ?? 1;
    const found = re && verdict !== "bad" ? this.find(re, verdict, this.origin, line.kind === "?", count) : null;
    this.preview = { pattern: line.text, rope, range: found?.range ?? null };
    return this.preview.range;
  }

  /**
   * The verdict on `re` for the text as it is, without waiting: `"waiting"`
   * (and it is asked for) while none is there, `"bad"` for a pattern too
   * expensive or broken, `null` with no guard at all (run it as before).
   */
  private peek(re: RegExp): Verdict | "waiting" | "bad" | null {
    const guard = this.host.guard();
    if (!guard) return null;
    const rope = this.host.doc.store.snapshot();
    const verdict = guard.verdict(rope, re);
    if (!verdict) {
      void guard.request(rope, re);
      return "waiting";
    }
    return verdict.ok ? verdict : "bad";
  }

  /**
   * The verdict on `re` for the text as it is, waiting for it: throws
   * `SearchPending` while it is on its way. A bad one is reported and `false`
   * returned; without a guard (tests) it is simply `null`.
   */
  private vouched(re: RegExp): Verdict | null | false {
    const guard = this.host.guard();
    if (!guard) return null;
    const rope = this.host.doc.store.snapshot();
    // A loop's own edits make text no worker has seen; it was vouched for up front.
    if (this.looping > 0) {
      const known = guard.verdict(rope, re);
      return known?.ok ? known : null;
    }
    const verdict = guard.require(rope, re);
    if (verdict.ok) return verdict;
    this.error(verdict.message);
    return false;
  }

  /** Before a submitted line closes: waits for the verdict on every pattern it will run. */
  private vouchFor(kind: CmdlineKind, text: string): void {
    const guard = this.host.guard();
    if (!guard) return;
    const patterns: Array<{ pattern: string; mode: CaseMode }> = [];
    if (kind !== ":") {
      const pattern = text || this.host.memory.last?.pattern;
      if (pattern) patterns.push({ pattern, mode: "smart" });
    } else {
      patterns.push(...this.patternsOfEx(text));
    }
    this.vouchAll(patterns);
  }

  /** Waits (throws `SearchPending`) until every one of `patterns` has a verdict on the text as it is. */
  private vouchAll(patterns: Array<{ pattern: string; mode: CaseMode }>): void {
    const guard = this.host.guard();
    if (!guard || this.looping > 0) return;
    const rope = this.host.doc.store.snapshot();
    for (const { pattern, mode } of patterns) {
      const c = compilePattern(pattern, mode);
      if (!("error" in c)) guard.require(rope, c.re);
    }
  }

  /** The patterns a `:` line will search with — `:s` and `:&` — so they can be vouched for up front. */
  private patternsOfEx(text: string): Array<{ pattern: string; mode: CaseMode }> {
    const host = this.host;
    const cmd = parseEx(text, {
      line: host.cursor().line,
      lineCount: host.doc.store.lineCount(),
      mark: (name) => host.mark(name)?.line ?? null,
    });
    if ("error" in cmd) return [];
    const memory = host.memory;
    if (cmd.name === "global") {
      const pattern = cmd.pattern || memory.last?.pattern;
      return [...(pattern ? [{ pattern, mode: "smart" as CaseMode }] : []), ...this.patternsOfEx(cmd.command)];
    }
    const spec =
      cmd.name === "substitute"
        ? parseSubstitute(cmd.args, memory.substitute)
        : cmd.name === "repeatSubstitute"
          ? memory.substitute
          : null;
    if (!spec || "error" in spec) return [];
    const pattern = spec.pattern || memory.last?.pattern || memory.substitute?.pattern;
    return pattern ? [{ pattern, mode: spec.caseMode }] : [];
  }

  /**
   * The `count`-th match from `from`: from the verdict's matches when it has
   * them all, else by running the pattern — safe once vouched for, and what
   * happens without a guard.
   */
  private find(re: RegExp, verdict: Verdict | null, from: Pos, backward: boolean, count: number) {
    const store = this.host.doc.store;
    if (verdict?.ok && !verdict.truncated) return jumpTo(store.snapshot(), verdict.offsets, from, backward, count);
    if (spansLines(re)) {
      const rope = store.snapshot();
      return jumpTo(rope, allMatches(rope.text(), re, Infinity).offsets, from, backward, count);
    }
    return findMatch(store, re, from, backward, count);
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
    const verdict = this.vouched(re);
    if (verdict === false) return null;
    const backward = last.backward !== reverse;
    const found = this.find(re, verdict, from, backward, count ?? 1);
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

  /**
   * `gn` / `gN` (T12): the last search's match under the cursor, else the
   * next one after it (before it, backward). `null`, with a message, if none.
   */
  matchAt(from: Pos, backward: boolean): Range | null {
    const memory = this.host.memory;
    const last = memory.last;
    if (!last) {
      this.error("E35: No previous regular expression");
      return null;
    }
    const re = this.regexFor(last.pattern);
    const verdict = re ? this.vouched(re) : false;
    if (!re || verdict === false) {
      if (!re) this.error(`E486: Pattern not found: ${last.pattern}`);
      return null;
    }
    memory.highlight = true;
    const rope = this.host.doc.store.snapshot();
    const offsets = verdict?.ok && !verdict.truncated ? verdict.offsets : allMatches(rope.text(), re, Infinity).offsets;
    const found = matchAround(rope, offsets, from) ?? jumpTo(rope, offsets, from, backward)?.range ?? null;
    if (!found) this.error(`E486: Pattern not found: ${last.pattern}`);
    return found;
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

  /** The match `:s///c` is asking about, for the view to show and scroll to. */
  confirmingAt(): Range | null {
    return this.confirm?.current ?? null;
  }

  /** hlsearch and incsearch for lines `first`–`last`: every match, and the one a search would go to. */
  highlights(first: number, last: number): { matches: Map<number, Array<[number, number]>>; current: Range | null } {
    const memory = this.host.memory;
    const typing = this.line && this.line.kind !== ":" && this.line.text ? this.line.text : null;
    const pattern = typing ?? (memory.highlight ? memory.last?.pattern : null);
    const re = pattern ? this.regexFor(pattern) : null;
    const none = new Map<number, Array<[number, number]>>();
    const verdict = re ? this.peek(re) : null;
    if (!re || verdict === "waiting" || verdict === "bad") return { matches: none, current: this.currentPreview() };
    const store = this.host.doc.store;
    const rope = store.snapshot();
    const matches =
      verdict?.ok && !verdict.truncated
        ? matchesOnLines(rope, verdict.offsets, first, last)
        : spansLines(re)
          ? this.windowMatches(rope, re, first, last)
          : matchesIn(store, re, first, last);
    return { matches, current: this.confirmingAt() ?? this.currentPreview() };
  }

  /**
   * A line-spanning pattern's matches for lines `first`–`last` when the
   * verdict could not carry them all: looked for in those lines' text only,
   * so a redraw costs the window, not the file. A match reaching in from
   * outside the window is not shown.
   */
  private windowMatches(rope: Rope, re: RegExp, first: number, last: number): Map<number, Array<[number, number]>> {
    const top = Math.max(0, first);
    const bottom = Math.min(last, rope.lineCount() - 1);
    if (top > bottom) return new Map();
    const start = rope.offsetAt(pos(top, 0));
    const found = allMatches(rope.lines(top, bottom).join("\n"), re, 10_000).offsets.map((o) => o + start);
    return matchesOnLines(rope, found, top, bottom);
  }

  /** Where the view should look: the match `:s///c` asks about, the incsearch match, else `null` (the cursor). */
  revealTarget(): Pos | null {
    return (this.confirmingAt() ?? this.currentPreview())?.start ?? null;
  }

  // ---------------------------------------------------------------- ex

  private submitEx(text: string): void {
    this.host.memory.remember("cmd", text);
    this.runEx(text);
    this.host.closed();
  }

  /**
   * `@:` (`count` times): the last `:` line again. Its patterns are vouched for
   * once, before the first run — waiting half way would run the first repeats
   * twice when the keys come back.
   */
  repeatEx(count = 1): void {
    const last = this.host.memory.lastEx;
    if (!last) return this.error("E30: No previous command line");
    this.vouchAll(this.patternsOfEx(last));
    this.looping++;
    try {
      for (let i = 0; i < count; i++) this.runEx(last);
    } finally {
      this.looping--;
    }
  }

  /** `&`: the last `:s` on the cursor's line, without its flags. */
  repeatSubstitute(): void {
    const line = this.host.cursor().line;
    this.runSubstitute({ first: line, last: line }, parseSubstitute("", this.host.memory.substitute));
  }

  private runEx(text: string): void {
    this.host.memory.lastEx = text;
    this.execute(text);
  }

  /** One `:` command, as typed or as `:g` runs it on a line. */
  private execute(text: string): void {
    const host = this.host;
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
      case "delete":
        return this.deleteLines(cmd.range, cmd.register);
      case "normal":
        return this.runNormal(cmd.range, cmd.keys);
      case "global":
        return this.runGlobal(cmd.range, cmd.pattern, cmd.invert, cmd.command);
    }
  }

  /** `:d`: the lines into a register (as `dd` does), the cursor to the line that took their place. */
  private deleteLines(lines: LineRange, register: string | null): void {
    const host = this.host;
    if (host.readOnly()) return this.error("E21: Cannot make changes, 'modifiable' is off");
    const store = host.doc.store;
    const text: string[] = [];
    for (let l = lines.first; l <= lines.last; l++) text.push(store.line(l));
    host.registers.put(register, { text: `${text.join("\n")}\n`, kind: "line" }, "delete");
    const last = store.lineCount() - 1;
    const r =
      lines.last < last
        ? makeRange(pos(lines.first, 0), pos(lines.last + 1, 0))
        : lines.first > 0
          ? makeRange(pos(lines.first - 1, store.line(lines.first - 1).length), pos(last, store.line(last).length))
          : makeRange(pos(0, 0), pos(last, store.line(last).length));
    host.doc.edit([{ range: r, text: "" }], cursor(host.cursor()), "other");
    const line = Math.min(lines.first, store.lineCount() - 1);
    host.moveTo(pos(line, firstNonBlank(store, line)));
  }

  /** `:normal`: the keys on the cursor's line, or on each line of the range. */
  private runNormal(lines: LineRange | null, keys: string): void {
    if (!lines) return this.host.normal(keys);
    this.eachLine(
      Array.from({ length: lines.last - lines.first + 1 }, (_, i) => lines.first + i),
      () => this.host.normal(keys),
    );
  }

  /**
   * `:g` / `:v` (T12): marks the lines first, then runs `command` on each one
   * still there, as one undo step. A line the command finds nothing on stays
   * quiet; only when every line failed is the last error shown.
   */
  private runGlobal(lines: LineRange, typed: string, invert: boolean, command: string): void {
    const host = this.host;
    const memory = host.memory;
    if (this.global) return this.error("E147: Cannot do :global recursive");
    const pattern = typed || memory.last?.pattern;
    if (!pattern) return this.error("E35: No previous regular expression");
    const c = compilePattern(pattern);
    if ("error" in c) return this.error(`E486: ${c.error}`);
    if (this.vouched(c.re) === false) return;
    // Every pattern the command runs on each line, before the first line is touched.
    this.vouchAll(this.patternsOfEx(command));
    memory.last = { pattern, backward: memory.last?.backward ?? false };
    memory.highlight = true;
    const marked = markedLines(host.doc.store, c.re, lines, invert);
    if (marked.length === 0) return this.error(`E486: Pattern not found: ${pattern}`);
    this.global = { errors: 0, last: null };
    let outcome: { errors: number; last: string | null };
    try {
      this.eachLine(marked, () => this.execute(command));
    } finally {
      outcome = this.global;
      this.global = null;
    }
    if (outcome.errors === marked.length && outcome.last) this.error(outcome.last);
  }

  /** Runs `work` with the cursor on each of `lines` still there, following the lines as `work` edits. */
  private eachLine(lines: number[], work: () => void): void {
    const doc = this.host.doc;
    const anchors = new LineAnchors(lines);
    const unsubscribe = doc.onTextChange((change) => anchors.change(change));
    const grouped = !doc.inUndoGroup;
    if (grouped) doc.beginUndoGroup();
    this.looping++;
    try {
      for (let i = 0; i < anchors.length; i++) {
        const line = anchors.at(i);
        if (line === null || line >= doc.store.lineCount()) continue;
        this.host.moveTo(pos(line, 0));
        work();
      }
    } finally {
      this.looping--;
      unsubscribe();
      if (grouped) doc.endUndoGroup();
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
    const compiled = compilePattern(pattern, spec.caseMode);
    if ("error" in compiled) return this.error(`E486: ${compiled.error}`);
    if (this.vouched(compiled.re) === false) return;
    // The pattern is the last search now (`n` finds it), whether or not it matched.
    memory.last = { pattern, backward: memory.last?.backward ?? false };
    memory.highlight = true;
    memory.substitute = spec;
    const previous = memory.replacement;
    if (spec.confirm) {
      if (this.global) return this.error("E: :s with the c flag is not supported inside :g");
      memory.replacement = spec.replacement;
      const session = new ConfirmSubstitute(host, range, spec, compiled.re, previous);
      this.confirm = session.done ? null : session;
      return;
    }
    if (spansLines(compiled.re)) {
      const spanning = substituteSpanning(host.doc.store, range, spec, previous);
      if ("error" in spanning) return this.error(spanning.error);
      memory.replacement = spec.replacement;
      host.doc.edit([{ range: spanning.range, text: spanning.text }], cursor(host.cursor()), "other");
      host.doc.setSelection(cursor(pos(spanning.lastLine, firstNonBlank(host.doc.store, spanning.lastLine))));
      return;
    }
    const result = substituteLines(host.doc.store, range, spec, previous);
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
    if (this.global) {
      this.global.errors++;
      this.global.last = text;
      return;
    }
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
