/**
 * Syntax highlighting for one open document (`docs/plans/editor.md`, D4, G4,
 * G10): a tree-sitter tree that follows every edit incrementally, and the
 * coloured spans of whichever lines are on screen.
 *
 * * **Incremental.** The document reports each replacement (`onTextChange`);
 *   the tree is edited at once and re-parsed lazily, on the next request for
 *   spans, reading the text straight from the store line by line — no copy of
 *   the whole text per keystroke.
 * * **Only what is visible is queried.** Captures are asked for the requested
 *   lines, so the cost per frame does not grow with the file.
 * * **Injected languages** (a Markdown code fence's Rust, Svelte's `<script>`)
 *   are parsed on their own and cached by language and text: an unchanged fence
 *   is never parsed twice. A language that is not loaded yet starts loading and
 *   the owner is told (`onLoaded`) so it can redraw once it arrives.
 */

import {
  Edit,
  Parser,
  Query,
  type Language,
  type Node,
  type Point,
  type QueryMatch,
  type Tree,
} from "web-tree-sitter";

import type { EditorDocument, TextChange } from "../document";
import { pos, range } from "../position";
import { languageForName } from "./languages";
import { paintLines, type BracketToken, type Capture, type Span } from "./paint";
import type { GrammarRuntime, LoadedLanguage } from "./runtime";
import { tokenFor } from "./tokens";

/** Injected snippets kept parsed; cleared wholesale when it grows past this. */
const INJECTION_CACHE_LIMIT = 2000;

const OPENING = new Set(["(", "[", "{"]);
const BRACKETS = ["(", ")", "[", "]", "{", "}"];
/** How far into a node's children an opening bracket is looked for. */
const BRACKET_CHILD_SCAN = 3;

/** Per grammar: a query for the bracket tokens it has (not every one has all). */
const bracketQueries = new WeakMap<Language, Query | null>();

function bracketQuery(language: Language): Query | null {
  if (bracketQueries.has(language)) return bracketQueries.get(language) ?? null;
  const known = BRACKETS.filter((b) => {
    try {
      new Query(language, `"${b}" @bracket`).delete();
      return true;
    } catch {
      return false;
    }
  });
  const query = known.length > 0 ? new Query(language, known.map((b) => `"${b}" @bracket`).join("\n")) : null;
  bracketQueries.set(language, query);
  return query;
}

/** Whether `node` itself is bracketed — an opening bracket among its first children. */
function isBracketed(node: Node): boolean {
  const n = Math.min(node.childCount, BRACKET_CHILD_SCAN);
  for (let i = 0; i < n; i++) if (OPENING.has(node.child(i)?.type ?? "")) return true;
  return false;
}

/** A bracket's colour: how many bracketed nodes enclose the pair it belongs to. */
function bracketToken(bracket: Node): BracketToken {
  let depth = 0;
  for (let a = bracket.parent?.parent ?? null; a; a = a.parent) if (isBracketed(a)) depth++;
  return `bracket-${(depth % 3) + 1}` as BracketToken;
}

export interface SpanOptions {
  /** Colour bracket pairs by depth (D8). */
  brackets?: boolean;
}

function point(p: { line: number; col: number }): Point {
  return { row: p.line, column: p.col };
}

/** A tree-sitter row range covering lines `first..last`, for `Query.captures`/`matches`. */
function rowRange(first: number, last: number): { startPosition: Point; endPosition: Point } {
  return { startPosition: { row: first, column: 0 }, endPosition: { row: last + 1, column: 0 } };
}

/** A node's extent as a `Capture`'s position fields. */
function nodeSpan(node: Node): Pick<Capture, "startRow" | "startCol" | "endRow" | "endCol"> {
  const s = node.startPosition;
  const e = node.endPosition;
  return { startRow: s.row, startCol: s.column, endRow: e.row, endCol: e.column };
}

/** A query's captures as paintable captures (coloured ones only), limited to `rows` if given. */
function toCaptures(query: Query, node: Node, rows?: { first: number; last: number }): Capture[] {
  const out: Capture[] = [];
  const range = rows ? rowRange(rows.first, rows.last) : undefined;
  for (const c of query.captures(node, range)) {
    const token = tokenFor(c.name);
    if (!token) continue;
    out.push({ ...nodeSpan(c.node), token, priority: c.patternIndex });
  }
  return out;
}

/** An injected snippet's capture, offset into its host: only the first row's column moves. */
function shiftCapture(c: Capture, start: Point): Capture {
  return {
    ...c,
    startRow: start.row + c.startRow,
    startCol: c.startRow === 0 ? start.column + c.startCol : c.startCol,
    endRow: start.row + c.endRow,
    endCol: c.endRow === 0 ? start.column + c.endCol : c.endCol,
  };
}

export class SyntaxHighlighter {
  private tree: Tree | null = null;
  private stale = true;
  private readonly unsubscribe: () => void;
  private readonly injected = new Map<string, Capture[]>();
  private snippetParser: Parser | null = null;

  private constructor(
    private readonly doc: EditorDocument,
    private readonly runtime: GrammarRuntime,
    private readonly lang: LoadedLanguage,
    private readonly parser: Parser,
    private readonly onLoaded: () => void,
  ) {
    parser.setLanguage(lang.language);
    this.unsubscribe = doc.onTextChange((change) => this.edit(change));
  }

  /**
   * A highlighter for `doc` in language `id`, or `null` if the language has no
   * grammar. `onLoaded` is called when an injected language finished loading.
   */
  static async create(
    doc: EditorDocument,
    runtime: GrammarRuntime,
    id: string,
    onLoaded: () => void = () => {},
  ): Promise<SyntaxHighlighter | null> {
    const lang = await runtime.load(id);
    if (!lang?.highlights) return null;
    return new SyntaxHighlighter(doc, runtime, lang, await runtime.parser(), onLoaded);
  }

  /** The spans of lines `first..last`, keyed by line; lines without colour are absent. */
  spans(first: number, last: number, options: SpanOptions = {}): Map<number, Span[]> {
    const tree = this.parse();
    if (!tree || !this.lang.highlights) return new Map();
    const store = this.doc.store;
    const lastLine = Math.min(last, store.lineCount() - 1);
    const layers = [toCaptures(this.lang.highlights, tree.rootNode, { first, last: lastLine })];
    if (this.lang.injections) layers.push(this.injectedCaptures(tree, first, lastLine));
    if (options.brackets) layers.push(this.bracketCaptures(tree, first, lastLine));
    return paintLines(layers, first, lastLine, (line) => store.line(line).length);
  }

  /** Bracket captures on lines `first..last`, coloured by depth; over everything else. */
  private bracketCaptures(tree: Tree, first: number, last: number): Capture[] {
    const query = bracketQuery(this.lang.language);
    if (!query) return [];
    const range = rowRange(first, last);
    return query.captures(tree.rootNode, range).map((c) => ({
      ...nodeSpan(c.node),
      token: bracketToken(c.node),
      priority: 0,
    }));
  }

  /** The current tree (for tests and, later, folding and text objects). */
  syntaxTree(): Tree | null {
    return this.parse();
  }

  dispose(): void {
    this.unsubscribe();
    this.tree?.delete();
    this.tree = null;
    this.parser.delete();
    this.snippetParser?.delete();
  }

  private edit(change: TextChange): void {
    this.tree?.edit(
      new Edit({
        startIndex: change.startIndex,
        oldEndIndex: change.oldEndIndex,
        newEndIndex: change.newEndIndex,
        startPosition: point(change.start),
        oldEndPosition: point(change.oldEnd),
        newEndPosition: point(change.newEnd),
      }),
    );
    this.stale = true;
  }

  /**
   * Re-parses if the text changed since the last parse, reusing the old tree.
   *
   * The callback answers by position, which the parser always passes correctly.
   * `web-tree-sitter`'s `node.text` reuses this callback but repeats the start
   * position while advancing only the index — so this class never asks a node
   * for its text and slices the store instead (`textOf`).
   */
  private parse(): Tree | null {
    if (!this.stale) return this.tree;
    const store = this.doc.store;
    const count = store.lineCount();
    const next = this.parser.parse((_index, at) => {
      if (at.row >= count) return undefined;
      const line = store.line(at.row);
      return at.row < count - 1 ? `${line.slice(at.column)}\n` : line.slice(at.column);
    }, this.tree);
    this.tree?.delete();
    this.tree = next;
    this.stale = false;
    return this.tree;
  }

  /** Captures of every injected snippet overlapping `first..last`. */
  private injectedCaptures(tree: Tree, first: number, last: number): Capture[] {
    const query = this.lang.injections;
    if (!query) return [];
    const out: Capture[] = [];
    for (const match of query.matches(tree.rootNode, rowRange(first, last))) {
      const content = match.captures.find((c) => c.name === "injection.content")?.node;
      const id = this.injectionLanguage(match);
      if (!content || !id) continue;
      const lang = this.runtime.peek(id);
      if (lang === undefined) {
        void this.runtime.load(id).then((loaded) => loaded && this.onLoaded());
        continue;
      }
      if (!lang?.highlights) continue;
      const start = content.startPosition;
      for (const c of this.snippetCaptures(lang, this.textOf(content))) out.push(shiftCapture(c, start));
    }
    return out;
  }

  /** The language an injection match names: its `#set!` property, or its `injection.language` node. */
  private injectionLanguage(match: QueryMatch): string | null {
    const nameNode = match.captures.find((c) => c.name === "injection.language")?.node;
    const name = match.setProperties?.["injection.language"] ?? (nameNode ? this.textOf(nameNode) : undefined);
    return name ? languageForName(name) : null;
  }

  /** A node's text, from the store (see `parse` for why not `node.text`). */
  private textOf(node: Node): string {
    const s = node.startPosition;
    const e = node.endPosition;
    return this.doc.store.slice(range(pos(s.row, s.column), pos(e.row, e.column)));
  }

  /** A snippet's captures relative to its own start, parsed once per text. */
  private snippetCaptures(lang: LoadedLanguage, text: string): Capture[] {
    const key = `${lang.spec.id}\0${text}`;
    const hit = this.injected.get(key);
    if (hit) return hit;
    if (this.injected.size > INJECTION_CACHE_LIMIT) this.injected.clear();
    this.snippetParser ??= new Parser();
    this.snippetParser.setLanguage(lang.language);
    const tree = this.snippetParser.parse(text);
    const captures = tree && lang.highlights ? toCaptures(lang.highlights, tree.rootNode) : [];
    tree?.delete();
    this.injected.set(key, captures);
    return captures;
  }
}
