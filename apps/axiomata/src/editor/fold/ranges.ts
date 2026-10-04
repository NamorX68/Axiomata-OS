/**
 * Where a document can fold (`docs/plans/editor.md`, ED5, T7): a range is a
 * header line that stays visible and the lines under it that a fold hides.
 *
 * Three sources, each for what it knows best:
 *
 * * **Indentation**, always — every non-blank line followed by more deeply
 *   indented ones. It is the only source in the light mode (T2) and in a diff,
 *   and fills the gaps a grammar leaves (Python's `if`, a method chain).
 * * **The syntax tree**, where there is one — functions and classes (the same
 *   node table as Vi's text objects), every bracketed node (`{ … }`, `[ … ]`,
 *   `( … )`) and multi-line comments, all spanning lines. It wins over the
 *   indentation on the lines it covers: badly indented code still folds right.
 *   A closing `}` (or `)`, `]`, `end`) on a line of its own stays visible, so a
 *   fold never swallows the `} else {` that opens the next block.
 * * **Markdown headings** in a Markdown file: a heading folds everything up to
 *   the next heading of the same or a higher level.
 *
 * All ranges are worked out at once and kept until the text (or the tree)
 * changes — one linear pass over the lines, and a walk over only those tree
 * nodes that span several lines. A very long text ({@link LOCAL_LINES} lines
 * and more, which has no tree anyway) is not passed over for the gutter: the
 * range at a line on screen is found by looking down from it (`near`).
 */

import type { Node, Tree } from "web-tree-sitter";

import type { TextStore } from "../buffer";
import { CLASSES, FUNCTIONS } from "../syntax/objects";
import { displayColumn } from "../text";

/** A fold: `start` stays visible, `start + 1` to `end` (inclusive) are hidden. */
export interface FoldRange {
  start: number;
  end: number;
}

/** Opening brackets a bracketed node begins with among its first children. */
const OPENING = new Set(["{", "[", "("]);
/** How far into a node's children the opening bracket is looked for (`fn f() {` has it later). */
const BRACKET_CHILD_SCAN = 3;
/** A line that starts by closing what its header opened: it stays visible under a fold. */
const CLOSING_LINE = /^\s*([)\]}]|end\b)/;
const ATX_HEADING = /^(#{1,6})(\s|$)/;
/** From this many lines on, the gutter asks line by line (`near`) rather than for every range. */
export const LOCAL_LINES = 50_000;
/** How far `near` looks down for the end of a block before it gives up on it. */
export const NEAR_SCAN_LINES = 5_000;
const FENCE = /^\s{0,3}(```|~~~)/;

/**
 * Every range that starts at a non-blank line followed by more deeply indented
 * ones, up to the last of them (blank lines inside belong to it, trailing ones
 * do not). One pass with a stack of the headers still open.
 */
export function indentRanges(store: TextStore, tabSize: number): Map<number, number> {
  const out = new Map<number, number>();
  const open: { line: number; indent: number }[] = [];
  let lastText = -1;
  const count = store.lineCount();
  for (let line = 0; line < count; line++) {
    const text = store.line(line);
    const lead = /^[ \t]*/.exec(text)?.[0].length ?? 0;
    if (lead === text.length) continue;
    const indent = displayColumn(text, lead, tabSize);
    while (open.length > 0 && open[open.length - 1].indent >= indent) {
      const header = open.pop()!;
      if (lastText > header.line) out.set(header.line, lastText);
    }
    open.push({ line, indent });
    lastText = line;
  }
  for (const header of open) if (lastText > header.line) out.set(header.line, lastText);
  return out;
}

/**
 * The indentation range starting at `line` alone, looking down at most
 * `limit` lines for its end (`null` if it has none, or it goes on further).
 */
export function indentRangeAt(store: TextStore, line: number, tabSize: number, limit: number): FoldRange | null {
  const depth = (text: string) => {
    const lead = /^[ \t]*/.exec(text)?.[0].length ?? 0;
    return lead === text.length ? null : displayColumn(text, lead, tabSize);
  };
  const header = depth(store.line(line));
  if (header === null) return null;
  const count = store.lineCount();
  let lastText = line;
  for (let i = line + 1; i < count; i++) {
    if (i - line > limit) return null;
    const d = depth(store.line(i));
    if (d === null) continue;
    if (d <= header) break;
    lastText = i;
  }
  return lastText > line ? { start: line, end: lastText } : null;
}

/** Markdown headings (outside code fences), each up to the next heading at its level or above. */
export function headingRanges(store: TextStore): Map<number, number> {
  const out = new Map<number, number>();
  const open: { line: number; level: number }[] = [];
  let lastText = -1;
  let fence: string | null = null;
  const close = (header: { line: number }) => {
    if (lastText > header.line) out.set(header.line, lastText);
  };
  for (let line = 0; line < store.lineCount(); line++) {
    const text = store.line(line);
    const f = FENCE.exec(text);
    if (f) fence = fence === null ? f[1] : fence === f[1] ? null : fence;
    const heading = fence === null && !f ? ATX_HEADING.exec(text) : null;
    if (heading) {
      const level = heading[1].length;
      while (open.length > 0 && open[open.length - 1].level >= level) close(open.pop()!);
      open.push({ line, level });
    }
    if (text.trim() !== "") lastText = line;
  }
  for (const header of open) close(header);
  return out;
}

function isBracketed(node: Node): boolean {
  const n = Math.min(node.childCount, BRACKET_CHILD_SCAN);
  for (let i = 0; i < n; i++) if (OPENING.has(node.child(i)?.type ?? "")) return true;
  return false;
}

/** Whether `node` is a fold of its own — asked only of nodes that span lines. */
function folds(node: Node): boolean {
  return FUNCTIONS.has(node.type) || CLASSES.has(node.type) || node.type.includes("comment") || isBracketed(node);
}

/**
 * The tree's ranges: per start line the outermost node that folds (the
 * function, not its body block that starts on the same line). Only nodes
 * spanning several lines are walked into — the rest cannot hold a fold.
 */
export function treeRanges(tree: Tree, store: TextStore): Map<number, number> {
  const out = new Map<number, number>();
  const visit = (node: Node): void => {
    const start = node.startPosition.row;
    // A node that ends at a line's very start (a line comment takes its line break) ends the line before.
    const last = node.endPosition.column === 0 ? node.endPosition.row - 1 : node.endPosition.row;
    if (last <= start) return;
    if (!out.has(start) && folds(node)) {
      const keepsLast = !node.type.includes("comment") && CLOSING_LINE.test(store.line(last));
      const end = keepsLast ? last - 1 : last;
      if (end > start) out.set(start, end);
    }
    for (const child of node.children) if (child) visit(child);
  };
  visit(tree.rootNode);
  return out;
}

export interface FoldSourceOptions {
  /** The current syntax tree, if the file has one (not in the light mode). */
  tree?: () => Tree | null;
  /** A Markdown file: headings fold. */
  markdown?: boolean;
  tabSize: number;
}

/**
 * Every range a document can fold, kept until the text or the tree changes.
 * `version` tells when the text changed (a document's `revision`).
 */
export class FoldRanges {
  private cached: { version: unknown; tree: Tree | null; starts: Map<number, number> } | null = null;

  constructor(
    private readonly store: TextStore,
    private readonly version: () => unknown,
    private readonly options: FoldSourceOptions,
  ) {}

  /** Start line → last hidden line, for every range. */
  starts(): Map<number, number> {
    const version = this.version();
    const tree = this.options.tree?.() ?? null;
    const c = this.cached;
    if (c && c.version === version && c.tree === tree) return c.starts;
    const starts = indentRanges(this.store, this.options.tabSize);
    if (this.options.markdown) for (const [start, end] of headingRanges(this.store)) starts.set(start, end);
    else if (tree) for (const [start, end] of treeRanges(tree, this.store)) starts.set(start, end);
    this.cached = { version, tree, starts };
    return starts;
  }

  /** Whether the gutter should ask {@link near} line by line instead of for every range. */
  get local(): boolean {
    return this.store.lineCount() >= LOCAL_LINES;
  }

  /** The range at `line` for the gutter of a very long text: indentation only, looked for from the line. */
  near(line: number): FoldRange | null {
    return indentRangeAt(this.store, line, this.options.tabSize, NEAR_SCAN_LINES);
  }

  /** The range that starts at `line`, if any. */
  at(line: number): FoldRange | null {
    const end = this.starts().get(line);
    return end === undefined ? null : { start: line, end };
  }

  /** Every range, in order of their start. */
  all(): FoldRange[] {
    return [...this.starts()].map(([start, end]) => ({ start, end })).sort((a, b) => a.start - b.start);
  }

  /** The ranges that hold `line` (as header or hidden line), innermost first. */
  around(line: number): FoldRange[] {
    return this.all()
      .filter((r) => r.start <= line && line <= r.end)
      .sort((a, b) => b.start - a.start || a.end - b.end);
  }
}
