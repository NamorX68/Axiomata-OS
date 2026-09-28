/**
 * Completion (`docs/plans/editor.md`, ED6.4, L7, L9): what a server's
 * `textDocument/completion` answer offers, filtered and ranked against what
 * is typed, and what taking an item does to the text. Part of the engine — no
 * DOM, no app imports.
 *
 * * **Filtering is the editor's**, not the server's: an answer is asked for
 *   once where a word begins and narrowed as the word grows — unless the
 *   server says its list is incomplete, then it is asked again.
 * * **Snippets (L9)** are inserted as their text: every placeholder becomes
 *   its default, the cursor goes to the first tab stop (or `$0`, or the end).
 *   Jumping through the stops is for later.
 * * **Taking an item** replaces the word being typed (or the range the server
 *   names on the cursor's line) and applies the item's additional edits (an
 *   auto-import) — all one undo step, as the `complete` command.
 */

import type { Change } from "../document";
import { comparePos, pos, range, type Pos, type Range } from "../position";

/** One offer, as the menu needs it; `raw` is the server's item, for `completionItem/resolve`. */
export interface CompletionItem {
  label: string;
  /** Beside the label (a signature, a module), from `labelDetails` or `detail`. */
  detail: string;
  kind: number | null;
  /** What the typed word is matched against. */
  filterText: string;
  sortText: string;
  /** What goes in: plain text, or a snippet's text with where the cursor lands. */
  insertText: string;
  snippet: boolean;
  /** The range to replace, from the server's `textEdit`, when it gave one. */
  range: Range | null;
  additionalEdits: Change[];
  documentation: string | null;
  preselect: boolean;
  raw: unknown;
}

export interface CompletionAnswer {
  items: CompletionItem[];
  /** The server may have more: ask again as the word grows. */
  incomplete: boolean;
}

/** `insertTextFormat` 2 is a snippet. */
const SNIPPET_FORMAT = 2;

/** The protocol's item kinds, as a short tag for the menu. */
const KIND_TAGS: Record<number, string> = {
  1: "txt",
  2: "fn",
  3: "fn",
  4: "new",
  5: "fld",
  6: "var",
  7: "cls",
  8: "trt",
  9: "mod",
  10: "prp",
  11: "unit",
  12: "val",
  13: "enum",
  14: "key",
  15: "snip",
  16: "col",
  17: "file",
  18: "ref",
  19: "dir",
  20: "enum",
  21: "const",
  22: "st",
  23: "evt",
  24: "op",
  25: "type",
};

/** The menu's tag for an item's kind (`fn`, `var`, …), empty when the server gave none. */
export function kindTag(kind: number | null): string {
  return kind === null ? "" : (KIND_TAGS[kind] ?? "");
}

type RawPos = { line?: unknown; character?: unknown };
type RawRange = { start?: RawPos; end?: RawPos };
type RawEdit = { range?: RawRange; insert?: RawRange; newText?: unknown };

interface RawItem {
  label?: unknown;
  labelDetails?: { detail?: unknown; description?: unknown };
  kind?: unknown;
  detail?: unknown;
  documentation?: unknown;
  preselect?: unknown;
  sortText?: unknown;
  filterText?: unknown;
  insertText?: unknown;
  insertTextFormat?: unknown;
  textEdit?: RawEdit;
  textEditText?: unknown;
  additionalTextEdits?: RawEdit[];
}

function posOf(raw: RawPos | undefined): Pos | null {
  return typeof raw?.line === "number" && typeof raw.character === "number" ? pos(raw.line, raw.character) : null;
}

function rangeOf(raw: RawRange | undefined): Range | null {
  const start = posOf(raw?.start);
  const end = posOf(raw?.end);
  return start && end ? range(start, end) : null;
}

/** A `MarkupContent` or string as Markdown; plain text is fenced so it is not read as Markdown. */
function docText(raw: unknown): string | null {
  if (typeof raw === "string") return raw.trim() === "" ? null : raw;
  if (typeof raw === "object" && raw !== null) {
    const d = raw as { kind?: unknown; value?: unknown };
    if (typeof d.value !== "string" || d.value.trim() === "") return null;
    if (d.kind === "plaintext") return d.value.replace(/[\\`*_{}[\]()#+\-.!<>]/g, "\\$&");
    return d.value;
  }
  return null;
}

/** Reads one item; `null` for one without a label. `defaults` is the list's `itemDefaults`. */
export function parseItem(raw: unknown, defaults: { editRange?: unknown; insertTextFormat?: unknown } = {}):
  | CompletionItem
  | null {
  const r = raw as RawItem | null;
  if (typeof r?.label !== "string") return null;
  const label = r.label;
  const edit = r.textEdit;
  const defaultRange = defaults.editRange as RawRange & { insert?: RawRange };
  // An `InsertReplaceEdit` names two ranges; the editor inserts (it replaces only the typed word).
  const editRange = rangeOf(edit?.range ?? edit?.insert) ?? rangeOf(defaultRange?.insert ?? defaultRange);
  const newText =
    typeof edit?.newText === "string"
      ? edit.newText
      : typeof r.textEditText === "string"
        ? r.textEditText
        : typeof r.insertText === "string"
          ? r.insertText
          : label;
  const format = r.insertTextFormat ?? defaults.insertTextFormat;
  const labelDetail = [r.labelDetails?.detail, r.labelDetails?.description]
    .filter((d): d is string => typeof d === "string" && d !== "")
    .join(" ");
  const additionalEdits: Change[] = [];
  for (const e of Array.isArray(r.additionalTextEdits) ? r.additionalTextEdits : []) {
    const at = rangeOf(e?.range);
    if (at && typeof e.newText === "string") additionalEdits.push({ range: at, text: e.newText });
  }
  return {
    label,
    detail: labelDetail || (typeof r.detail === "string" ? r.detail : ""),
    kind: typeof r.kind === "number" ? r.kind : null,
    filterText: typeof r.filterText === "string" ? r.filterText : label,
    sortText: typeof r.sortText === "string" ? r.sortText : label,
    insertText: newText,
    snippet: format === SNIPPET_FORMAT,
    range: editRange,
    additionalEdits,
    documentation: docText(r.documentation),
    preselect: r.preselect === true,
    raw,
  };
}

/** Reads a completion answer: a `CompletionList`, a bare list of items, or nothing. */
export function parseCompletion(result: unknown): CompletionAnswer {
  if (Array.isArray(result)) return { items: result.map((i) => parseItem(i)).filter(isItem), incomplete: false };
  if (typeof result !== "object" || result === null) return { items: [], incomplete: false };
  const list = result as { items?: unknown; isIncomplete?: unknown; itemDefaults?: Record<string, unknown> };
  const defaults = list.itemDefaults ?? {};
  const items = Array.isArray(list.items) ? list.items.map((i) => parseItem(i, defaults)).filter(isItem) : [];
  return { items, incomplete: list.isIncomplete === true };
}

function isItem(item: CompletionItem | null): item is CompletionItem {
  return item !== null;
}

/**
 * How well `query` matches `candidate` — `null` when it does not: every
 * character of the query in order, case-insensitive. Higher is better: a
 * match at the start, at word starts (`_`, a capital) and in one run scores
 * more; the exact case adds a little.
 */
export function matchScore(query: string, candidate: string): number | null {
  if (query === "") return 0;
  const q = query.toLowerCase();
  const c = candidate.toLowerCase();
  let score = 0;
  let at = 0;
  let previous = -2;
  for (let i = 0; i < q.length; i++) {
    const found = c.indexOf(q[i], at);
    if (found < 0) return null;
    const boundary =
      found === 0 ||
      /[^A-Za-z0-9]/.test(candidate[found - 1]) ||
      (candidate[found] !== candidate[found].toLowerCase() && candidate[found - 1] === candidate[found - 1].toLowerCase());
    if (found === 0) score += 8;
    else if (boundary) score += 5;
    if (found === previous + 1) score += 3;
    if (candidate[found] === query[i]) score += 1;
    score -= Math.min(found - at, 5) * 0.5;
    previous = found;
    at = found + 1;
  }
  return score;
}

/** The items that match `query`, best first; ties keep the server's `sortText` order. */
export function filterItems(items: readonly CompletionItem[], query: string): CompletionItem[] {
  const scored: { item: CompletionItem; score: number }[] = [];
  for (const item of items) {
    const score = matchScore(query, item.filterText);
    if (score !== null) scored.push({ item, score });
  }
  // With nothing typed yet (after `.`), the server's order is the order.
  if (query === "") return scored.map((s) => s.item).sort((a, b) => compareText(a.sortText, b.sortText));
  return scored
    .sort((a, b) => b.score - a.score || compareText(a.item.sortText, b.item.sortText))
    .map((s) => s.item);
}

function compareText(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * A snippet's text (L9): tab stops and placeholders become their default text
 * (`${1:name}` → `name`, `$1` → nothing, `${1|a,b|}` → `a`), variables their
 * default or nothing, `\` escapes are resolved. `cursor` is where the first tab
 * stop was (by number, `$0` last), or the end.
 */
export function expandSnippet(snippet: string): { text: string; cursor: number } {
  const stops = new Map<number, number>();
  let i = 0;
  const parse = (stopAt: string | null): string => {
    let out = "";
    while (i < snippet.length) {
      const ch = snippet[i];
      if (stopAt !== null && ch === stopAt) return out;
      if (ch === "\\" && i + 1 < snippet.length && "$}\\,|".includes(snippet[i + 1])) {
        out += snippet[i + 1];
        i += 2;
        continue;
      }
      if (ch !== "$") {
        out += ch;
        i++;
        continue;
      }
      const simple = /^\$(\d+|[A-Za-z_][A-Za-z0-9_]*)/.exec(snippet.slice(i));
      if (simple) {
        i += simple[0].length;
        if (/^\d+$/.test(simple[1])) note(Number(simple[1]), outBase + out.length);
        continue;
      }
      const open = /^\$\{(\d+|[A-Za-z_][A-Za-z0-9_]*)/.exec(snippet.slice(i));
      if (!open) {
        out += ch;
        i++;
        continue;
      }
      i += open[0].length;
      const isStop = /^\d+$/.test(open[1]);
      const at = outBase + out.length;
      if (isStop) note(Number(open[1]), at);
      if (snippet[i] === "|") {
        // A choice: the first option.
        const end = snippet.indexOf("|}", i + 1);
        const options = snippet.slice(i + 1, end < 0 ? snippet.length : end);
        out += options.split(",")[0] ?? "";
        i = end < 0 ? snippet.length : end + 2;
        continue;
      }
      if (snippet[i] === ":") {
        i++;
        const saved = outBase;
        outBase = at;
        out += parse("}");
        outBase = saved;
      } else if (snippet[i] === "/") {
        // A transform: the variable's value would be rewritten; without values there is nothing.
        let depth = 0;
        while (i < snippet.length && !(snippet[i] === "}" && depth === 0)) {
          if (snippet[i] === "\\") i++;
          else if (snippet[i] === "{") depth++;
          else if (snippet[i] === "}") depth--;
          i++;
        }
      }
      if (snippet[i] === "}") i++;
    }
    return out;
  };
  let outBase = 0;
  const note = (n: number, at: number) => {
    if (!stops.has(n)) stops.set(n, at);
  };
  const text = parse(null);
  const numbered = [...stops.keys()].filter((n) => n > 0).sort((a, b) => a - b);
  const first = numbered.length > 0 ? stops.get(numbered[0]) : stops.get(0);
  return { text, cursor: first ?? text.length };
}

/** What taking an item does, as the `complete` command carries it (see `commands.ts`). */
export interface CompletionEdit {
  /** Characters before the cursor on its line that are replaced (the typed word). */
  before: number;
  /** Characters after the cursor on its line that are replaced. */
  after: number;
  text: string;
  /** Where the cursor goes, as an offset into `text`. */
  cursor: number;
  /** Edits elsewhere (an auto-import), in the document's coordinates before this edit. */
  extra: Change[];
}

/**
 * What taking `item` at `at` does. `wordStart` is where the typed word begins
 * on the cursor's line; a server range on that line containing the cursor
 * wins over it. `indent` is the line's own indentation, given to every further
 * line of a multi-line insert.
 */
export function completionEdit(item: CompletionItem, at: Pos, wordStart: number, indent: string): CompletionEdit {
  const r = item.range;
  const usable = r !== null && r.start.line === at.line && r.end.line === at.line && r.start.col <= at.col;
  const start = usable ? r.start.col : wordStart;
  const end = usable ? Math.max(r.end.col, at.col) : at.col;
  const expanded = item.snippet ? expandSnippet(item.insertText) : { text: item.insertText, cursor: item.insertText.length };
  const indented = (s: string) => s.split("\n").join(`\n${indent}`);
  const text = indented(expanded.text);
  const cursor = indented(expanded.text.slice(0, expanded.cursor)).length;
  // Additional edits must not touch the replaced range; ones that do are dropped.
  const replaced = range(pos(at.line, start), pos(at.line, end));
  const extra = item.additionalEdits.filter(
    (e) => comparePos(e.range.end, replaced.start) <= 0 || comparePos(e.range.start, replaced.end) >= 0,
  );
  return { before: at.col - start, after: end - at.col, text, cursor, extra };
}
