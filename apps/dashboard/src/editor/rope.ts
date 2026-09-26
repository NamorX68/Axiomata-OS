/**
 * The rope (`docs/plans/editor.md`, ED5, T1): the text store for files of any
 * size the editor edits (16 MiB), behind the same `TextStore` as `LineStore`.
 *
 * A balanced tree whose leaves hold runs of whole lines. Every node knows how
 * many lines it holds and how many UTF-16 units they have (line breaks not
 * counted), so finding line `i`, or turning a position into an offset, walks
 * one path from the root: O(log n). An edit rebuilds only the path to the
 * lines it touches; everything else is shared with the previous version.
 *
 * Three decisions carry it:
 *
 * * **Lines, not characters, are the unit.** The whole editor (positions,
 *   rendering, tree-sitter's parse callback) asks by line and column; a
 *   character rope would have to find line breaks for every one of those
 *   questions. A very long line is a single string in a leaf, and an edit
 *   inside it copies that string — the same cost `LineStore` has today.
 * * **Nodes never change once built.** A `Rope` is a value: `replace` returns
 *   a new one and leaves the old one intact. That makes a snapshot free (the
 *   search worker, T5, reads a fixed version while the owner keeps typing)
 *   and keeps an undo tree possible later (T3). `RopeStore` is the mutable
 *   `TextStore` around the current version.
 * * **A B-tree keeps it balanced**: all leaves sit at the same depth, leaves
 *   hold up to {@link MAX_LEAF_LINES} lines, branches up to
 *   {@link MAX_CHILDREN} children. An under-filled node is merged into a
 *   neighbour on the way back up, an over-full one split; the root grows or
 *   shrinks by one level at a time.
 */

import { splitLines, type TextStore } from "./buffer";
import { comparePos, pos, type Pos, type Range } from "./position";

/** Most lines one leaf holds; a leaf built fresh gets {@link LEAF_TARGET}. */
export const MAX_LEAF_LINES = 64;
/** Most children one branch holds. */
export const MAX_CHILDREN = 16;
const LEAF_TARGET = MAX_LEAF_LINES / 2;
const MIN_LEAF_LINES = MAX_LEAF_LINES / 4;
const MIN_CHILDREN = MAX_CHILDREN / 4;

interface Leaf {
  readonly kind: "leaf";
  readonly lines: readonly string[];
  /** UTF-16 units in all lines, breaks not counted. */
  readonly chars: number;
}

interface Branch {
  readonly kind: "branch";
  readonly children: readonly Node[];
  readonly lineCount: number;
  readonly chars: number;
  /** Levels below this one; every leaf under it is this far down. */
  readonly height: number;
}

type Node = Leaf | Branch;

function leaf(lines: readonly string[]): Leaf {
  let chars = 0;
  for (const line of lines) chars += line.length;
  return { kind: "leaf", lines, chars };
}

function branch(children: readonly Node[]): Branch {
  let lineCount = 0;
  let chars = 0;
  for (const child of children) {
    lineCount += linesIn(child);
    chars += child.chars;
  }
  return { kind: "branch", children, lineCount, chars, height: heightOf(children[0]) + 1 };
}

function linesIn(node: Node): number {
  return node.kind === "leaf" ? node.lines.length : node.lineCount;
}

function heightOf(node: Node): number {
  return node.kind === "leaf" ? 0 : node.height;
}

function underfull(node: Node): boolean {
  return node.kind === "leaf" ? node.lines.length < MIN_LEAF_LINES : node.children.length < MIN_CHILDREN;
}

/** `items` cut into runs of `target`, the last run taking a short remainder rather than standing alone. */
function chunk<T>(items: readonly T[], max: number, target: number): T[][] {
  if (items.length <= max) return [items.slice()];
  const out: T[][] = [];
  for (let at = 0; at < items.length; at += target) out.push(items.slice(at, at + target));
  const last = out[out.length - 1];
  if (out.length > 1 && last.length < target / 2) out[out.length - 2] = out[out.length - 2].concat(out.pop() ?? []);
  return out;
}

/** Leaves for `lines`, each at most {@link MAX_LEAF_LINES}. */
function leavesOf(lines: readonly string[]): Leaf[] {
  return lines.length === 0 ? [] : chunk(lines, MAX_LEAF_LINES, LEAF_TARGET).map(leaf);
}

/** Branches over same-height `nodes`, each at most {@link MAX_CHILDREN} children. */
function branchesOf(nodes: readonly Node[]): Branch[] {
  return nodes.length === 0 ? [] : chunk(nodes, MAX_CHILDREN, MAX_CHILDREN / 2).map(branch);
}

/** Two same-height neighbours as one node, or as two again when that overflows. */
function mergePair(a: Node, b: Node): Node[] {
  if (a.kind === "leaf" && b.kind === "leaf") return leavesOf([...a.lines, ...b.lines]);
  if (a.kind === "branch" && b.kind === "branch") return branchesOf(rebalance([...a.children, ...b.children]));
  throw new Error("rope: merging nodes of different heights");
}

/** Siblings with every under-filled one merged into a neighbour. */
function rebalance(nodes: Node[]): Node[] {
  if (nodes.length < 2) return nodes;
  const out: Node[] = [];
  for (const node of nodes) {
    const prev = out[out.length - 1];
    if (prev && (underfull(prev) || underfull(node))) out.splice(out.length - 1, 1, ...mergePair(prev, node));
    else out.push(node);
  }
  return out;
}

/**
 * `node` with `count` lines from line `from` replaced by `insert`, as zero or
 * more nodes of the same height (fewer when lines went, more when it split).
 * `from` names an existing line and `count` is at least 1: `Rope.replace`
 * always replaces the lines its range touches, so there is no pure append.
 */
function splice(node: Node, from: number, count: number, insert: readonly string[]): Node[] {
  if (node.kind === "leaf") {
    return leavesOf([...node.lines.slice(0, from), ...insert, ...node.lines.slice(from + count)]);
  }
  const end = from + count;
  const out: Node[] = [];
  let start = 0;
  let inserted = false;
  for (const child of node.children) {
    const childEnd = start + linesIn(child);
    if (!inserted && from < childEnd) {
      // The child holding line `from` takes the insertion and the part of the
      // deletion inside it.
      out.push(...splice(child, from - start, Math.min(end, childEnd) - from, insert));
      inserted = true;
    } else if (inserted && start < end) {
      // A later child the deletion still reaches into, from its first line.
      out.push(...splice(child, 0, Math.min(end, childEnd) - start, []));
    } else {
      out.push(child);
    }
    start = childEnd;
  }
  return branchesOf(rebalance(out));
}

/** The line at index `i` of `node`. */
function lineAt(node: Node, i: number): string {
  let current = node;
  let index = i;
  while (current.kind === "branch") {
    let next: Node | null = null;
    for (const child of current.children) {
      const n = linesIn(child);
      if (index < n) {
        next = child;
        break;
      }
      index -= n;
    }
    if (!next) throw new RangeError(`line ${i} out of range`);
    current = next;
  }
  return current.lines[index];
}

/** UTF-16 units in the lines before line `i`, breaks not counted. */
function charsBefore(node: Node, i: number): number {
  let current = node;
  let index = i;
  let chars = 0;
  while (current.kind === "branch") {
    let next: Node | null = null;
    for (const child of current.children) {
      const n = linesIn(child);
      if (index < n) {
        next = child;
        break;
      }
      index -= n;
      chars += child.chars;
    }
    if (!next) return chars;
    current = next;
  }
  for (let k = 0; k < index; k++) chars += current.lines[k].length;
  return chars;
}

/** Calls `visit` with every line in `[from, to]`, in order. */
function eachLine(node: Node, from: number, to: number, visit: (line: string) => void): void {
  if (node.kind === "leaf") {
    for (let i = Math.max(from, 0); i <= Math.min(to, node.lines.length - 1); i++) visit(node.lines[i]);
    return;
  }
  let start = 0;
  for (const child of node.children) {
    const n = linesIn(child);
    if (start > to) break;
    if (start + n > from) eachLine(child, from - start, to - start, visit);
    start += n;
  }
}

/** One version of a text. Never changes; {@link replace} returns the next one. */
export class Rope {
  private joined: string | null = null;

  private constructor(private readonly root: Node) {}

  /** A rope over `text`, split on LF, CRLF or a lone CR. */
  static of(text: string): Rope {
    return Rope.fromLines(splitLines(text));
  }

  static fromLines(lines: readonly string[]): Rope {
    let level: Node[] = leavesOf(lines.length === 0 ? [""] : lines);
    while (level.length > 1) level = branchesOf(level);
    return new Rope(level[0]);
  }

  lineCount(): number {
    return linesIn(this.root);
  }

  /** Every UTF-16 unit of the text, each line break counted as one. */
  length(): number {
    return this.root.chars + this.lineCount() - 1;
  }

  line(i: number): string {
    if (!Number.isInteger(i) || i < 0 || i >= this.lineCount()) {
      throw new RangeError(`line ${i} out of 0..${this.lineCount() - 1}`);
    }
    return lineAt(this.root, i);
  }

  slice(r: Range): string {
    const { start, end } = this.checked(r);
    if (start.line === end.line) return this.line(start.line).slice(start.col, end.col);
    const parts: string[] = [];
    eachLine(this.root, start.line, end.line, (line) => parts.push(line));
    parts[0] = parts[0].slice(start.col);
    parts[parts.length - 1] = parts[parts.length - 1].slice(0, end.col);
    return parts.join("\n");
  }

  /** The text of lines `from` to `to` (inclusive), in order. */
  lines(from: number, to: number): string[] {
    const out: string[] = [];
    eachLine(this.root, from, to, (line) => out.push(line));
    return out;
  }

  /** The next version, and the position right after the inserted text. */
  replace(r: Range, text: string): { rope: Rope; end: Pos } {
    const { start, end } = this.checked(r);
    const first = this.line(start.line);
    const last = start.line === end.line ? first : this.line(end.line);
    const inserted = splitLines(text);
    const tail = inserted.length - 1;
    const endCol = (tail === 0 ? start.col : 0) + inserted[tail].length;
    inserted[0] = first.slice(0, start.col) + inserted[0];
    inserted[tail] += last.slice(end.col);
    let level: Node[] = splice(this.root, start.line, end.line - start.line + 1, inserted);
    while (level.length > 1) level = branchesOf(level);
    let root = level[0];
    // A root left with one child is a level too many.
    while (root.kind === "branch" && root.children.length === 1) root = root.children[0];
    return { rope: new Rope(root), end: pos(start.line + tail, endCol) };
  }

  /** The whole text, lines joined with `\n` — built once per version. */
  text(): string {
    if (this.joined === null) this.joined = this.lines(0, this.lineCount() - 1).join("\n");
    return this.joined;
  }

  offsetAt(p: Pos): number {
    return charsBefore(this.root, p.line) + p.line + p.col;
  }

  /** Throws on a range that is reversed or points outside the text. */
  private checked(r: Range): Range {
    if (comparePos(r.start, r.end) > 0) throw new RangeError("reversed range");
    for (const p of [r.start, r.end]) {
      if (p.line < 0 || p.line >= this.lineCount() || p.col < 0 || p.col > this.line(p.line).length) {
        throw new RangeError(`position ${p.line}:${p.col} outside the text`);
      }
    }
    return r;
  }

  /**
   * Checks the tree's invariants and throws on the first one broken — for
   * tests: every leaf at the same depth, no node over its limit, no node but
   * the root under its minimum, cached counts matching what is below them.
   */
  checkInvariants(): void {
    const visit = (node: Node, isRoot: boolean): number => {
      if (node.kind === "leaf") {
        if (node.lines.length === 0 || node.lines.length > MAX_LEAF_LINES) throw new Error("leaf size out of bounds");
        if (!isRoot && node.lines.length < MIN_LEAF_LINES) throw new Error("leaf under-filled");
        if (node.chars !== node.lines.reduce((sum, l) => sum + l.length, 0)) throw new Error("leaf chars wrong");
        return 0;
      }
      if (node.children.length > MAX_CHILDREN) throw new Error("branch over-full");
      if (node.children.length < (isRoot ? 2 : MIN_CHILDREN)) throw new Error("branch under-filled");
      const heights = new Set(node.children.map((c) => visit(c, false)));
      if (heights.size !== 1 || [...heights][0] !== node.height - 1) throw new Error("unbalanced");
      if (node.lineCount !== node.children.reduce((sum, c) => sum + linesIn(c), 0)) throw new Error("line count");
      if (node.chars !== node.children.reduce((sum, c) => sum + c.chars, 0)) throw new Error("branch chars wrong");
      return node.height;
    };
    visit(this.root, true);
  }

  /** How many levels the tree has — for tests. */
  depth(): number {
    return heightOf(this.root) + 1;
  }
}

/** The editor's `TextStore`: the current {@link Rope}, replaced by every edit. */
export class RopeStore implements TextStore {
  private rope: Rope;

  constructor(text = "") {
    this.rope = Rope.of(text);
  }

  /** The current version, fixed — it does not follow later edits. */
  snapshot(): Rope {
    return this.rope;
  }

  lineCount(): number {
    return this.rope.lineCount();
  }

  line(i: number): string {
    return this.rope.line(i);
  }

  slice(r: Range): string {
    return this.rope.slice(r);
  }

  replace(r: Range, text: string): Pos {
    const next = this.rope.replace(r, text);
    this.rope = next.rope;
    return next.end;
  }

  text(): string {
    return this.rope.text();
  }

  offsetAt(p: Pos): number {
    return this.rope.offsetAt(p);
  }
}
