/**
 * Vi's text objects from the syntax tree (`docs/plans/editor.md`, ED3, V9, D4):
 * `if`/`af` a function, `ic`/`ac` a class (struct, impl, trait, interface,
 * enum …), `ia`/`aa` an argument or parameter.
 *
 * Which node is which is a small table of tree-sitter node types across the
 * bundled grammars — the grammars agree on most names (`function_declaration`,
 * `class_declaration`, `arguments` …), so one table per object reads better
 * than a query file per language. A language whose node types are not in the
 * table simply has no such object there.
 *
 * * **around** is the whole node — as whole lines when it starts at its line's
 *   first non-blank and ends its last line, like Vim's `ap`;
 * * **inner** is its body: between the braces (whole lines when they sit on
 *   their own lines), or the body block itself (Python, Lua);
 * * **an argument's around** takes one separating comma with it — the one after
 *   it, or the one before it for the last argument.
 */

import type { Node } from "web-tree-sitter";

import type { TextStore } from "../buffer";
import { pos, type Pos } from "../position";
import type { ObjectRange } from "../vi/textobjects";

/** A text object's stretch — the same shape the classic objects have (`end` exclusive, or a last line). */
export type SyntaxObjectRange = ObjectRange;

/** `f`, `c`, `a` — the letters after `i`/`a` (V9). */
export type SyntaxObjectName = "f" | "c" | "a";

/** Function-like nodes — `if`/`af`, and the folds of ED5.5 (T7). */
export const FUNCTIONS: ReadonlySet<string> = new Set([
  // Rust
  "function_item",
  "closure_expression",
  // JavaScript, TypeScript, Swift, Lua
  "function_declaration",
  "function_expression",
  "generator_function_declaration",
  "arrow_function",
  "method_definition",
  "function_signature",
  "lambda_literal",
  // Python, Bash, Lua
  "function_definition",
  "lambda",
]);

/** Class-like nodes — `ic`/`ac`, and the folds of ED5.5 (T7). */
export const CLASSES: ReadonlySet<string> = new Set([
  "class_declaration",
  "abstract_class_declaration",
  "class_definition",
  "class",
  "interface_declaration",
  "enum_declaration",
  "protocol_declaration",
  "struct_item",
  "enum_item",
  "union_item",
  "impl_item",
  "trait_item",
  "mod_item",
]);

/**
 * Nodes whose named children are arguments or parameters. Swift has no such list: its
 * `parameter` nodes hang straight off the function (`isArgument`).
 */
const ARGUMENT_LISTS = new Set([
  "arguments",
  "argument_list",
  "formal_parameters",
  "parameters",
  "parameter_list",
  "closure_parameters",
  "lambda_parameters",
  "type_arguments",
  "type_parameters",
  "value_arguments",
]);

/** An argument or parameter: a named child of an argument list, or Swift's bare `parameter`. */
function isArgument(n: Node): boolean {
  const parent = n.parent;
  if (!n.isNamed || !parent) return false;
  return ARGUMENT_LISTS.has(parent.type) || (n.type === "parameter" && FUNCTIONS.has(parent.type));
}

function toPos(p: { row: number; column: number }): Pos {
  return pos(p.row, p.column);
}

/** The innermost node at `at`, then its ancestors, outwards. */
function* outwards(root: Node, at: Pos): Generator<Node> {
  let node: Node | null = root.descendantForPosition({ row: at.line, column: at.col });
  while (node) {
    yield node;
    node = node.parent;
  }
}

/** The `count`-th node (1 = innermost) around `at` that `matches`. */
function enclosing(root: Node, at: Pos, count: number, matches: (n: Node) => boolean): Node | null {
  let seen = 0;
  for (const node of outwards(root, at)) {
    if (matches(node) && ++seen === count) return node;
  }
  return null;
}

function firstNonBlankCol(text: string): number {
  const m = /\S/.exec(text);
  return m ? m.index : text.length;
}

/** `start`–`end` as whole lines if nothing else shares their first and last line, else as characters. */
function asLinesIfWhole(store: TextStore, start: Pos, end: Pos): SyntaxObjectRange {
  const startsLine = start.col <= firstNonBlankCol(store.line(start.line));
  const endsLine = store.line(end.line).slice(end.col).trim() === "";
  if (!startsLine || !endsLine) return { start, end, linewise: false };
  return { start: pos(start.line, 0), end: pos(end.line, store.line(end.line).length), linewise: true };
}

/** A body between braces: its inside, as whole lines when the braces sit alone at its ends. */
function insideBraces(store: TextStore, body: Node): SyntaxObjectRange {
  const open = toPos(body.startPosition);
  const close = toPos(body.endPosition);
  const first = pos(open.line, open.col + 1);
  const last = pos(close.line, close.col - 1);
  const openEndsLine = store.line(open.line).slice(first.col).trim() === "";
  const closeStartsLine = last.col <= firstNonBlankCol(store.line(close.line));
  if (openEndsLine && closeStartsLine && close.line - open.line >= 2) {
    return {
      start: pos(open.line + 1, 0),
      end: pos(close.line - 1, store.line(close.line - 1).length),
      linewise: true,
    };
  }
  // `{}`: nothing inside, an empty stretch just after the brace.
  return { start: first, end: last.col < first.col && last.line === first.line ? first : last, linewise: false };
}

/** A function's or class's body: inside `{ … }`, or a block (Python) or an expression (`x => x + 1`) as it is. */
function innerOf(store: TextStore, node: Node): SyntaxObjectRange | null {
  // Swift's closure has no `body` field; its statements are a child of their own.
  const body = node.childForFieldName("body") ?? node.namedChildren.find((c) => c.type === "statements") ?? null;
  if (!body) return null;
  if (store.line(body.startPosition.row)[body.startPosition.column] === "{") return insideBraces(store, body);
  return asLinesIfWhole(store, toPos(body.startPosition), toPos(body.endPosition));
}

function argumentRange(node: Node, around: boolean): SyntaxObjectRange {
  const start = toPos(node.startPosition);
  const end = toPos(node.endPosition);
  if (!around) return { start, end, linewise: false };
  // Around: the comma after it and the space up to the next argument; for the last one, the comma before.
  const next = node.nextNamedSibling;
  const after = node.nextSibling;
  if (next && after?.type === ",") return { start, end: toPos(next.startPosition), linewise: false };
  const prev = node.previousNamedSibling;
  const before = node.previousSibling;
  if (prev && before?.type === ",") return { start: toPos(prev.endPosition), end, linewise: false };
  return { start, end, linewise: false };
}

/**
 * The text object `name` around `at` in the tree `root` (`inner` for `i…`,
 * else `a…`), the `count`-th one outwards; `null` if there is none.
 */
export function syntaxObject(
  root: Node,
  store: TextStore,
  at: Pos,
  name: SyntaxObjectName,
  inner: boolean,
  count = 1,
): SyntaxObjectRange | null {
  if (name === "a") {
    const arg = enclosing(root, at, count, isArgument);
    return arg ? argumentRange(arg, !inner) : null;
  }
  const kinds = name === "f" ? FUNCTIONS : CLASSES;
  const node = enclosing(root, at, count, (n) => kinds.has(n.type));
  if (!node) return null;
  if (inner) return innerOf(store, node);
  return asLinesIfWhole(store, toPos(node.startPosition), toPos(node.endPosition));
}
