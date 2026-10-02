/**
 * The outline of a document (`docs/plans/editor-projekt-werkzeuge.md`, #49): its named
 * symbols as a tree — functions, types, modules, and in Markdown the headings — for the
 * outline view and the breadcrumbs above the editor.
 *
 * Like Vi's text objects and the folds, it reads the syntax tree with a small table of node
 * types across the bundled grammars (`SYMBOLS`) rather than a query file per language. A
 * language whose node types are not in the table has no outline. Names are sliced from the
 * `TextStore`, never read from `node.text` (see `AGENTS.md`).
 *
 * * **What becomes a symbol**: declarations with a name. A `const f = () => …` or
 *   `const f = function …` counts as a function; other `const`s do not.
 * * **What does not**: anything inside a function body — closures and local helpers are not
 *   part of a file's shape.
 * * **Kinds** only pick the icon and the sort of label: a function inside a class, impl,
 *   trait or interface is a `method`.
 */

import type { Node, Tree } from "web-tree-sitter";

import type { TextStore } from "../buffer";

export type SymbolKind =
  | "function"
  | "method"
  | "class"
  | "struct"
  | "enum"
  | "interface"
  | "trait"
  | "impl"
  | "module"
  | "constant"
  | "type"
  | "macro"
  | "heading";

export interface OutlineSymbol {
  name: string;
  kind: SymbolKind;
  /** The line the declaration starts on, and the last line it covers. */
  line: number;
  endLine: number;
  /** Where the name starts on `line`'s name row — where a jump puts the cursor. */
  nameLine: number;
  nameCol: number;
  /** Markdown only: the heading level (1–6). */
  level?: number;
  children: OutlineSymbol[];
}

/** Symbols kept at most; a generated file of hundreds of thousands of items is not an outline. */
export const MAX_SYMBOLS = 5000;

/** Node type → kind, across the grammars (they agree on most names). */
const SYMBOLS: ReadonlyMap<string, SymbolKind> = new Map([
  // Rust
  ["function_item", "function"],
  ["function_signature_item", "function"],
  ["struct_item", "struct"],
  ["enum_item", "enum"],
  ["union_item", "struct"],
  ["trait_item", "trait"],
  ["impl_item", "impl"],
  ["mod_item", "module"],
  ["const_item", "constant"],
  ["static_item", "constant"],
  ["type_item", "type"],
  ["macro_definition", "macro"],
  // JavaScript, TypeScript
  ["function_declaration", "function"],
  ["generator_function_declaration", "function"],
  ["method_definition", "method"],
  ["method_signature", "method"],
  ["class_declaration", "class"],
  ["abstract_class_declaration", "class"],
  ["interface_declaration", "interface"],
  ["enum_declaration", "enum"],
  ["type_alias_declaration", "type"],
  ["internal_module", "module"],
  // Python, Bash, Lua
  ["function_definition", "function"],
  ["class_definition", "class"],
  // Swift
  ["protocol_declaration", "interface"],
]);

/** Kinds that hold members: a function inside one is a method. */
const CONTAINERS: ReadonlySet<SymbolKind> = new Set(["class", "struct", "impl", "trait", "interface", "enum"]);
/** Kinds whose insides are not part of the outline. */
const OPAQUE: ReadonlySet<SymbolKind> = new Set(["function", "method", "constant", "type", "macro"]);

function textOf(store: TextStore, node: Node): string {
  const row = node.startPosition.row;
  const line = store.line(row);
  const end = node.endPosition.row === row ? node.endPosition.column : line.length;
  return line.slice(node.startPosition.column, end).trim();
}

/** A Rust `impl` has no `name`: `impl Type` or `impl Trait for Type`. */
function implName(store: TextStore, node: Node): { name: string; at: Node } | null {
  const type = node.childForFieldName("type");
  if (!type) return null;
  const trait = node.childForFieldName("trait");
  const name = trait ? `${textOf(store, trait)} for ${textOf(store, type)}` : textOf(store, type);
  return { name, at: trait ?? type };
}

/** `const f = () => …`, `const f = function …`: the declarator's name if its value is a function. */
function declaredFunction(store: TextStore, node: Node): { name: string; at: Node } | null {
  if (node.type !== "variable_declarator") return null;
  const value = node.childForFieldName("value");
  const name = node.childForFieldName("name");
  if (!name || !value) return null;
  if (value.type !== "arrow_function" && value.type !== "function_expression" && value.type !== "function") return null;
  return { name: textOf(store, name), at: name };
}

function symbolOf(store: TextStore, node: Node, parent: OutlineSymbol | null): OutlineSymbol | null {
  let kind = SYMBOLS.get(node.type);
  let named: { name: string; at: Node } | null = null;
  if (kind) {
    if (node.type === "impl_item") named = implName(store, node);
    else {
      const name = node.childForFieldName("name");
      if (name) named = { name: textOf(store, name), at: name };
    }
  } else {
    named = declaredFunction(store, node);
    if (named) kind = "function";
  }
  if (!kind || !named || named.name === "") return null;
  if ((kind === "function" || kind === "method") && parent && CONTAINERS.has(parent.kind)) kind = "method";
  else if (kind === "method") kind = parent && CONTAINERS.has(parent.kind) ? "method" : "function";
  const last = node.endPosition.column === 0 ? node.endPosition.row - 1 : node.endPosition.row;
  return {
    name: named.name,
    kind,
    line: node.startPosition.row,
    endLine: Math.max(last, node.startPosition.row),
    nameLine: named.at.startPosition.row,
    nameCol: named.at.startPosition.column,
    children: [],
  };
}

/** The outline of a syntax tree: nested symbols in source order. */
export function outlineFromTree(tree: Tree, store: TextStore): OutlineSymbol[] {
  const top: OutlineSymbol[] = [];
  let count = 0;
  const visit = (node: Node, parent: OutlineSymbol | null): void => {
    if (count >= MAX_SYMBOLS) return;
    const symbol = symbolOf(store, node, parent);
    let into = parent;
    if (symbol) {
      count++;
      (parent ? parent.children : top).push(symbol);
      if (OPAQUE.has(symbol.kind)) return;
      into = symbol;
    }
    for (const child of node.children) if (child) visit(child, into);
  };
  visit(tree.rootNode, null);
  return top;
}

const ATX_HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const FENCE = /^\s{0,3}(```|~~~)/;

/** The outline of a Markdown file: headings (outside code fences), nested by level. */
export function outlineFromMarkdown(store: TextStore): OutlineSymbol[] {
  const top: OutlineSymbol[] = [];
  const open: OutlineSymbol[] = [];
  let fence: string | null = null;
  let lastText = -1;
  let count = 0;
  const close = (s: OutlineSymbol) => {
    s.endLine = Math.max(s.line, lastText);
  };
  for (let line = 0; line < store.lineCount(); line++) {
    const text = store.line(line);
    const f = FENCE.exec(text);
    if (f) fence = fence === null ? f[1] : fence === f[1] ? null : fence;
    const m = fence === null && !f ? ATX_HEADING.exec(text) : null;
    if (m && m[2] !== "" && count < MAX_SYMBOLS) {
      const level = m[1].length;
      while (open.length > 0 && (open[open.length - 1].level ?? 0) >= level) close(open.pop()!);
      const symbol: OutlineSymbol = {
        name: m[2],
        kind: "heading",
        line,
        endLine: line,
        nameLine: line,
        nameCol: text.indexOf(m[2]),
        level,
        children: [],
      };
      (open.length > 0 ? open[open.length - 1].children : top).push(symbol);
      open.push(symbol);
      count++;
    }
    if (text.trim() !== "") lastText = line;
  }
  for (const s of open) close(s);
  return top;
}

/** The symbols holding `line`, outermost first — the cursor's place in the outline and the breadcrumbs. */
export function pathAt(outline: readonly OutlineSymbol[], line: number): OutlineSymbol[] {
  const path: OutlineSymbol[] = [];
  let level = outline;
  for (;;) {
    // The last symbol that starts at or above `line` and still covers it.
    const next = [...level].reverse().find((s) => s.line <= line && line <= s.endLine);
    if (!next) return path;
    path.push(next);
    level = next.children;
  }
}

/** The symbols matching `query` (case-insensitive substring), each with its ancestors; all of them for an empty query. */
export function filterOutline(outline: readonly OutlineSymbol[], query: string): OutlineSymbol[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...outline];
  const keep = (s: OutlineSymbol): OutlineSymbol | null => {
    const children = s.children.map(keep).filter((c): c is OutlineSymbol => c !== null);
    return s.name.toLowerCase().includes(q) || children.length > 0 ? { ...s, children } : null;
  };
  return outline.map(keep).filter((s): s is OutlineSymbol => s !== null);
}
