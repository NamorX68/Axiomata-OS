/**
 * `:s///c` (`docs/plans/editor.md`, ED5, T12): asking at each match —
 * `y` replace, `n` skip, `a` all the rest, `l` this one and stop, `q`/Esc stop.
 *
 * A session of its own, fed the keys while it lasts (`CommandMode.active`).
 * Every replacement it makes is one undo step together. The next match is
 * looked for in the text as the replacements so far left it, from just past
 * the last one (or the next line, without `g`).
 */

import type { EditorDocument } from "../document";
import { cursor, pos, range as makeRange, type Pos, type Range } from "../position";
import { spansLines } from "../search/matches";
import type { ViMessage } from "./cmdmode";
import type { LineRange } from "./ex";
import type { ViKey } from "./keys";
import { firstNonBlank } from "./scan";
import { expandReplacement, type SubstituteSpec } from "./substitute";

/** What a session needs from the command line. */
export interface ConfirmHost {
  doc: EditorDocument;
  moveTo(at: Pos): void;
  message(message: ViMessage): void;
}

export class ConfirmSubstitute {
  /** The range's last line, moved by the lines the replacements add or take. */
  private last: number;
  /** Where the next match is looked for. */
  private from: Pos;
  private match: { range: Range; m: RegExpExecArray } | null = null;
  private count = 0;
  private lastLine = -1;
  /** Whether any match was asked about: none at all is "pattern not found". */
  private found = false;
  private readonly grouped: boolean;
  private finished = false;

  /** Starts asking at the first match of `re` in `lines`; with none, it is `done` at once. */
  constructor(
    private readonly host: ConfirmHost,
    lines: LineRange,
    private readonly spec: SubstituteSpec,
    private readonly re: RegExp,
    /** The replacement before this one, for `~`. */
    private readonly previous: string,
  ) {
    this.last = lines.last;
    this.from = pos(lines.first, 0);
    this.grouped = !host.doc.inUndoGroup;
    if (this.grouped) host.doc.beginUndoGroup();
    this.next();
  }

  /** The round is over: the keys go back to Normal mode. */
  get done(): boolean {
    return this.finished;
  }

  /** The match being asked about, for the view to show and scroll to. */
  get current(): Range | null {
    return this.match?.range ?? null;
  }

  /** One answer. */
  key(key: ViKey): void {
    switch (key) {
      case "y":
        this.replace();
        return this.next();
      case "l":
        this.replace();
        return this.end();
      case "n":
        this.skip();
        return this.next();
      case "a":
        while (this.match) {
          this.replace();
          this.next();
        }
        return;
      case "q":
      case "<Esc>":
      case "<C-[>":
      case "<C-c>":
        return this.end();
      default:
        // Anything else asks again.
        this.ask();
    }
  }

  /** Finds the next match to ask about, or ends the round when there is none. */
  private next(): void {
    const store = this.host.doc.store;
    const re = this.re;
    this.match = null;
    if (spansLines(re)) {
      const rope = store.snapshot();
      if (this.from.line <= this.last) {
        re.lastIndex = rope.offsetAt(this.from);
        const m = re.exec(rope.text());
        const start = m ? rope.posAt(m.index) : null;
        if (m && start && start.line <= this.last) {
          this.match = { range: makeRange(start, rope.posAt(m.index + m[0].length)), m };
        }
      }
    } else {
      for (let line = this.from.line; line <= Math.min(this.last, store.lineCount() - 1) && !this.match; line++) {
        re.lastIndex = line === this.from.line ? this.from.col : 0;
        const m = re.exec(store.line(line));
        if (m) this.match = { range: makeRange(pos(line, m.index), pos(line, m.index + m[0].length)), m };
      }
    }
    if (!this.match) return this.end();
    this.found = true;
    this.ask();
  }

  private ask(): void {
    const m = this.match;
    const shown = m ? expandReplacement(this.spec.replacement, m.m, this.previous).replace(/\n/g, "^M") : "";
    this.host.message({ text: `replace with ${shown} (y/n/a/q/l)?`, error: false });
  }

  private replace(): void {
    const { range, m } = this.match!;
    const text = expandReplacement(this.spec.replacement, m, this.previous);
    this.host.doc.edit([{ range, text }], cursor(range.start), "other");
    const inserted = text.split("\n");
    const endLine = range.start.line + inserted.length - 1;
    const endCol = (inserted.length === 1 ? range.start.col : 0) + inserted[inserted.length - 1].length;
    this.last += inserted.length - 1 - (range.end.line - range.start.line);
    this.count++;
    this.lastLine = range.start.line;
    this.from = this.after(pos(endLine, endCol), range, endLine);
  }

  private skip(): void {
    const { range } = this.match!;
    this.from = this.after(range.end, range, range.start.line);
  }

  /** Where the search goes on after a match: past it with `g` (a step past an empty one), else the next line. */
  private after(end: Pos, match: Range, line: number): Pos {
    if (!this.spec.global) return pos(line + 1, 0);
    const empty = match.start.line === match.end.line && match.start.col === match.end.col;
    if (!empty) return end;
    const text = this.host.doc.store.line(end.line);
    if (end.col >= text.length) return pos(end.line + 1, 0);
    return pos(end.line, end.col + (text.codePointAt(end.col)! > 0xffff ? 2 : 1));
  }

  private end(): void {
    if (this.finished) return;
    this.finished = true;
    this.match = null;
    const doc = this.host.doc;
    if (this.grouped) doc.endUndoGroup();
    if (this.count === 0) {
      if (!this.found) this.host.message({ text: `E486: Pattern not found: ${this.spec.pattern}`, error: true });
      return;
    }
    const line = Math.min(this.lastLine, doc.store.lineCount() - 1);
    this.host.moveTo(pos(line, firstNonBlank(doc.store, line)));
    this.host.message({ text: `${this.count} substitution${this.count === 1 ? "" : "s"}`, error: false });
  }
}
