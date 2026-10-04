/**
 * What the find bar does (`docs/plans/editor.md`, ED5, T4, T15), without the
 * bar: the query and its options, the matches in the text as it is, moving
 * between them, replacing one or all, turning them into cursors, and the
 * "in selection" scope. No DOM — the surface draws it and the tests drive it.
 *
 * **No pattern runs before the guard vouched for it** (`guard.ts`, T5): the
 * matches come from the worker's verdict for the current text version. While a
 * verdict is on its way the count shows as pending and whatever was asked for
 * (a jump, a replacement) waits and runs once the verdict is there — unless
 * something newer was asked for in between. After a replacement the text is a
 * new version, so the move to the next match waits for its verdict too.
 * Without a guard (the unit tests' jsdom) the matches are found straight away.
 *
 * **Vi mode** (T15) has one cursor and no selection of the document's own: a
 * match is "the current one" when the cursor is on its start, and reaching a
 * match puts the cursor there (the host's `place`) instead of selecting it.
 * Turning matches into cursors is for the normal key map only (T6).
 *
 * **Several cursors** survive typing a query: the search while typing only
 * highlights then. A jump or a replacement goes to one match, and so leaves
 * one cursor, as any cursor jump does.
 */

import type { EditorDocument } from "../document";
import { mapPos } from "../multicursor";
import { comparePos, cursor, isEmpty, pos, range, selectionRange, type Pos, type Range } from "../position";
import type { Rope } from "../rope";
import { wordAt } from "../text";
import type { Compiled } from "../vi/search";
import { compileFind, DEFAULT_FIND_OPTIONS, escapePattern, expandReplacement, preserveCase, type FindOptions } from "./find";
import type { SearchGuard } from "./guard";
import { MAX_REPORTED } from "./guard";
import { matchesOnLines } from "./jump";
import { allMatches, spansLines, type AllMatches } from "./matches";

/** Most cursors ⌥⏎ makes; more would make every keystroke crawl. */
export const MAX_FIND_CURSORS = 10_000;

export interface FindHost {
  doc: EditorDocument;
  guard(): SearchGuard | null;
  /** Vi mode is on: one cursor, placed on a match's start. */
  vi(): boolean;
  /** Shows `r` as the match reached: selects it, or (Vi) puts the cursor on its start. */
  place(r: Range): void;
  /** Something to redraw: a verdict came, the selection or the text changed. */
  changed(): void;
  /** The search was used (a jump, a replacement): it becomes the last search everywhere (T15). */
  commit?(query: string, options: FindOptions): void;
}

/** What the bar's counter shows. */
export type FindStatus =
  | { kind: "empty" }
  | { kind: "pending" }
  | { kind: "error"; message: string }
  /** `index` is the match the selection (the Vi cursor) is on, from 0; `null` when on none. */
  | { kind: "ok"; count: number; truncated: boolean; index: number | null };

/** The matches of the query on the current text, or why there are none (yet). */
type Ready = { found: AllMatches; rope: Rope; compiled: Compiled };
type Lookup = Ready | "pending" | { error: string } | null;

/** The matches a scope leaves, cached for one offsets array and one scope. */
interface Scoped {
  source: Int32Array;
  from: number;
  to: number;
  offsets: Int32Array;
}

export class FindModel {
  query = "";
  replacement = "";
  options: FindOptions = { ...DEFAULT_FIND_OPTIONS };
  /** "In selection": only matches wholly inside this range count; it follows edits. */
  scope: Range | null = null;
  /** A line for the bar: "Replaced 12", "Wrapped", a refusal. Cleared by the next action. */
  message: string | null = null;
  /** The bar is showing: verdicts redraw it. */
  visible = false;

  /** Where the search while typing starts: the selection when the bar opened, or the last match reached. */
  private origin: Pos = pos(0, 0);
  private compiledKey = "";
  private compiledValue: Compiled | { error: string } | null = null;
  /** Matches found without a guard, per text version and pattern. */
  private readonly direct = new WeakMap<Rope, Map<string, AllMatches>>();
  private scoped: Scoped | null = null;
  /** Asked for while the verdict was not there yet; dropped when something newer is asked for. */
  private waiting: ((ready: Ready) => void) | null = null;
  private readonly unsubscribeVerdicts: () => void;
  private readonly unsubscribeText: () => void;

  constructor(private readonly host: FindHost) {
    this.unsubscribeVerdicts = host.guard()?.onVerdict(() => this.retry()) ?? (() => undefined);
    this.unsubscribeText = host.doc.onTextChange((change) => {
      if (this.scope) this.scope = range(mapPos(this.scope.start, change), mapPos(this.scope.end, change));
    });
  }

  dispose(): void {
    this.unsubscribeVerdicts();
    this.unsubscribeText();
    this.waiting = null;
  }

  /**
   * The bar opens (⌘F): the search while typing starts at the selection. A
   * selection within one line becomes the query; one over several lines
   * becomes the scope ("in selection"), as a Mac editor does.
   */
  open(): void {
    const doc = this.host.doc;
    const sel = selectionRange(doc.selection);
    this.origin = sel.start;
    this.message = null;
    if (!this.host.vi() && !isEmpty(sel)) {
      if (sel.start.line === sel.end.line) this.setQueryText(doc.store.slice(sel));
      else this.scope = sel;
    }
    this.visible = true;
  }

  close(): void {
    this.visible = false;
    this.waiting = null;
    this.scope = null;
    this.message = null;
    this.commitSearch();
  }

  /** A new query was typed: the first match from where the search started is shown. */
  setQuery(query: string): void {
    this.query = query;
    this.message = null;
    this.searchFromOrigin();
  }

  setOptions(patch: Partial<FindOptions>): void {
    this.options = { ...this.options, ...patch };
    this.message = null;
    this.searchFromOrigin();
  }

  /** ⌘E: the selection (the word at the cursor) is the query, as literal text. */
  useSelection(): void {
    const doc = this.host.doc;
    const sel = selectionRange(doc.selection);
    let text = doc.store.slice(sel);
    if (isEmpty(sel)) {
      const line = doc.store.line(sel.start.line);
      const w = wordAt(line, sel.start.col);
      text = line.slice(w.start, w.end);
    }
    if (!text || text.includes("\n")) return;
    this.setQueryText(text);
    this.origin = sel.start;
    this.message = null;
    this.commitSearch();
    this.host.changed();
  }

  /** "In selection" on or off; on needs a selection (in the normal key map). */
  toggleScope(): void {
    if (this.scope) {
      this.scope = null;
    } else {
      const sel = selectionRange(this.host.doc.selection);
      if (this.host.vi() || isEmpty(sel)) {
        this.message = "Select the text to search in first";
        this.host.changed();
        return;
      }
      this.scope = sel;
      this.origin = sel.start;
    }
    this.message = null;
    this.host.changed();
  }

  /** ⏎ / ⌘G (⇧ for backwards): the next match after the selection, wrapping round. */
  next(backward = false): void {
    this.message = null;
    this.commitSearch();
    this.whenReady(({ found, rope }) => {
      const offsets = this.scopedOffsets(found.offsets, rope);
      const total = offsets.length / 2;
      if (total === 0) return;
      const sel = this.currentRange();
      const at = rope.offsetAt(sel.start);
      let k = firstFrom(offsets, backward ? at : at + 1);
      if (backward) k -= 1;
      let wrapped = false;
      if (k >= total) {
        k = 0;
        wrapped = true;
      } else if (k < 0) {
        k = total - 1;
        wrapped = true;
      }
      this.reach(rope, offsets, k);
      if (wrapped) this.message = backward ? "Wrapped to the end" : "Wrapped to the start";
    });
  }

  /** Replace: the current match becomes the replacement, and the next one is shown; off a match, just the next. */
  replaceOne(): void {
    this.message = null;
    this.commitSearch();
    this.whenReady(({ found, rope, compiled }) => {
      const offsets = this.scopedOffsets(found.offsets, rope);
      const index = this.currentIndex(offsets, rope);
      if (index === null) {
        this.next();
        return;
      }
      const s = offsets[index * 2];
      const e = offsets[index * 2 + 1];
      const text = this.replacementAt(rope, compiled, s, e);
      const start = rope.posAt(s);
      const doc = this.host.doc;
      doc.edit([{ range: range(start, rope.posAt(e)), text }], cursor(start), "other");
      const after = doc.store.offsetAt(start) + text.length;
      this.host.place(range(start, start));
      // The next match on the new text, once it has a verdict; never one inside the replacement.
      this.whenReady(({ found: next, rope: now }) => {
        const list = this.scopedOffsets(next.offsets, now);
        if (list.length === 0) return;
        const k = firstFrom(list, after);
        this.reach(now, list, k < list.length / 2 ? k : 0);
      });
    });
  }

  /** Replace all: every match (in the scope) in one undo step. */
  replaceAll(): void {
    this.message = null;
    this.commitSearch();
    this.whenReady(({ found, rope, compiled }) => {
      // The verdict lists the first matches only; the guard vouched for the whole text, so this run finishes.
      const every = found.truncated ? allMatches(rope.text(), compiled.re, Infinity).offsets : found.offsets;
      const offsets = this.scopedOffsets(every, rope);
      const total = offsets.length / 2;
      if (total === 0) {
        this.message = "Nothing to replace";
        return;
      }
      const text = rope.text();
      const first = offsets[0];
      const last = offsets[offsets.length - 1];
      let out = "";
      let at = first;
      for (let k = 0; k < total; k++) {
        const s = offsets[k * 2];
        const e = offsets[k * 2 + 1];
        out += text.slice(at, s) + this.replacementAt(rope, compiled, s, e);
        at = e;
      }
      const start = rope.posAt(first);
      this.host.doc.edit([{ range: range(start, rope.posAt(last)), text: out }], cursor(start), "other");
      this.host.place(range(start, start));
      this.message = total === 1 ? "Replaced 1 match" : `Replaced ${total} matches`;
    });
  }

  /** ⌥⏎: every match (in the scope) becomes a cursor with its match selected (T6). */
  selectAll(): void {
    this.message = null;
    if (this.host.vi()) {
      this.message = "Several cursors are for the normal key map";
      this.host.changed();
      return;
    }
    this.commitSearch();
    this.whenReady(({ found, rope }) => {
      const offsets = this.scopedOffsets(found.offsets, rope);
      const list: Array<{ anchor: Pos; head: Pos }> = [];
      for (let k = 0; k < offsets.length / 2; k++) {
        if (offsets[k * 2 + 1] > offsets[k * 2]) list.push({ anchor: rope.posAt(offsets[k * 2]), head: rope.posAt(offsets[k * 2 + 1]) });
      }
      if (list.length === 0) return;
      if (list.length > MAX_FIND_CURSORS || found.truncated) {
        this.message = `Too many matches for cursors (at most ${MAX_FIND_CURSORS.toLocaleString("en")})`;
        return;
      }
      const at = this.currentRange().start;
      const main = Math.max(0, list.findIndex((s) => comparePos(s.anchor, at) >= 0));
      this.host.doc.setSelections(list[main], list.filter((_, i) => i !== main));
      this.scope = null;
    });
  }

  status(): FindStatus {
    const found = this.lookup();
    if (found === null) return { kind: "empty" };
    if (found === "pending") return { kind: "pending" };
    if ("error" in found) return { kind: "error", message: found.error };
    const offsets = this.scopedOffsets(found.found.offsets, found.rope);
    const count = this.scope ? offsets.length / 2 : found.found.count;
    return { kind: "ok", count, truncated: found.found.truncated, index: this.currentIndex(offsets, found.rope) };
  }

  /** The matches on lines `first`–`last`, the one the selection is on, and the scope — to draw. */
  highlights(first: number, last: number): { matches: Map<number, Array<[number, number]>>; current: Range | null; scope: Range | null } {
    const found = this.lookup();
    if (found === null || found === "pending" || "error" in found) return { matches: new Map(), current: null, scope: this.scope };
    const offsets = this.scopedOffsets(found.found.offsets, found.rope);
    const index = this.currentIndex(offsets, found.rope);
    const current =
      index === null ? null : range(found.rope.posAt(offsets[index * 2]), found.rope.posAt(offsets[index * 2 + 1]));
    return { matches: matchesOnLines(found.rope, offsets, first, last), current, scope: this.scope };
  }

  /** Whether replacing makes sense: a query that compiles. */
  get valid(): boolean {
    const c = this.compiled();
    return c !== null && !("error" in c);
  }

  // ------------------------------------------------------------ internals

  /** Makes the query the last search everywhere, if it is one. */
  private commitSearch(): void {
    if (this.valid) this.host.commit?.(this.query, this.options);
  }

  private setQueryText(text: string): void {
    this.query = this.options.regex ? escapePattern(text) : text;
  }

  private searchFromOrigin(): void {
    this.whenReady(({ found, rope }) => {
      const offsets = this.scopedOffsets(found.offsets, rope);
      if (offsets.length === 0) return;
      const k = firstFrom(offsets, rope.offsetAt(this.origin));
      this.reach(rope, offsets, k < offsets.length / 2 ? k : 0, false);
    });
  }

  /** Shows match `k`; a jump (not the search while typing) also moves where typing searches from. */
  private reach(rope: Rope, offsets: Int32Array, k: number, moveOrigin = true): void {
    // Typing a query must not throw away cursors the owner built up (⌘D, ⌥-click).
    if (!moveOrigin && this.host.doc.extra.length > 0) return;
    const r = range(rope.posAt(offsets[k * 2]), rope.posAt(offsets[k * 2 + 1]));
    if (moveOrigin) this.origin = r.start;
    this.host.place(r);
  }

  /** The selection, or in Vi the cursor alone. */
  private currentRange(): Range {
    const sel = this.host.doc.selection;
    return this.host.vi() ? range(sel.head, sel.head) : selectionRange(sel);
  }

  /** The match the selection is on (exactly), or in Vi the one starting at the cursor. */
  private currentIndex(offsets: Int32Array, rope: Rope): number | null {
    const sel = this.currentRange();
    const s = rope.offsetAt(sel.start);
    const k = firstFrom(offsets, s);
    if (k >= offsets.length / 2 || offsets[k * 2] !== s) return null;
    if (!this.host.vi() && offsets[k * 2 + 1] !== rope.offsetAt(sel.end)) return null;
    return k;
  }

  /** What the match at `[s, e)` is replaced with: the template expanded with its groups, case kept if asked. */
  private replacementAt(rope: Rope, compiled: Compiled, s: number, e: number): string {
    const re = new RegExp(compiled.re.source, compiled.re.flags.replace("g", "") + "y");
    let m: RegExpExecArray | null;
    // The match again, where the search found it: a pattern searched line by line sees only its line.
    if (spansLines(compiled.re)) {
      re.lastIndex = s;
      m = re.exec(rope.text());
    } else {
      const at = rope.posAt(s);
      re.lastIndex = at.col;
      m = re.exec(rope.line(at.line));
    }
    const matched = m?.[0] ?? rope.text().slice(s, e);
    const fallback = Object.assign([matched], { index: s, input: matched }) as unknown as RegExpExecArray;
    const text = expandReplacement(this.replacement, m ?? fallback, this.options.regex);
    return this.options.preserveCase ? preserveCase(matched, text) : text;
  }

  private compiled(): Compiled | { error: string } | null {
    const key = `${this.query}\u0000${this.options.regex}\u0000${this.options.caseSensitive}\u0000${this.options.wholeWord}`;
    if (key !== this.compiledKey) {
      this.compiledKey = key;
      this.compiledValue = compileFind(this.query, this.options);
    }
    return this.compiledValue;
  }

  /** The matches on the text as it is now, asking the guard for them if need be. */
  private lookup(): Lookup {
    const compiled = this.compiled();
    if (compiled === null) return null;
    if ("error" in compiled) return { error: compiled.error };
    const rope = this.host.doc.store.snapshot();
    const guard = this.host.guard();
    if (!guard) {
      let forRope = this.direct.get(rope);
      if (!forRope) {
        forRope = new Map();
        this.direct.set(rope, forRope);
      }
      let found = forRope.get(this.compiledKey);
      if (!found) {
        found = allMatches(rope.text(), compiled.re, MAX_REPORTED);
        forRope.set(this.compiledKey, found);
      }
      return { found, rope, compiled };
    }
    const verdict = guard.verdict(rope, compiled.re);
    if (!verdict) {
      void guard.request(rope, compiled.re);
      return "pending";
    }
    if (!verdict.ok) return { error: verdict.message };
    return { found: verdict, rope, compiled };
  }

  /** Runs `action` on the matches now, or once their verdict is in; it replaces anything still waiting. */
  private whenReady(action: (ready: Ready) => void): void {
    this.waiting = null;
    const found = this.lookup();
    if (found === "pending") {
      this.waiting = action;
    } else if (found !== null && !("error" in found)) {
      action(found);
    }
    this.host.changed();
  }

  /** A verdict arrived (for any surface): run what waited for it, if it is ours; redraw the counter. */
  private retry(): void {
    const action = this.waiting;
    if (action) {
      const found = this.lookup();
      if (found === "pending") return;
      this.waiting = null;
      if (found !== null && !("error" in found)) action(found);
    }
    if (action || this.visible) this.host.changed();
  }

  /** `offsets` less the matches outside the scope (the same array without one). */
  private scopedOffsets(offsets: Int32Array, rope: Rope): Int32Array {
    if (!this.scope) return offsets;
    const from = rope.offsetAt(this.scope.start);
    const to = rope.offsetAt(this.scope.end);
    const cached = this.scoped;
    if (cached && cached.source === offsets && cached.from === from && cached.to === to) return cached.offsets;
    const kept: number[] = [];
    for (let k = firstFrom(offsets, from); k < offsets.length / 2 && offsets[k * 2] < to; k++) {
      if (offsets[k * 2 + 1] <= to) kept.push(offsets[k * 2], offsets[k * 2 + 1]);
    }
    const result = Int32Array.from(kept);
    this.scoped = { source: offsets, from, to, offsets: result };
    return result;
  }
}

/** Index of the first match starting at or after `offset` (`offsets` holds sorted `[start, end)` pairs). */
function firstFrom(offsets: Int32Array, offset: number): number {
  let lo = 0;
  let hi = offsets.length / 2;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (offsets[mid * 2] < offset) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}
