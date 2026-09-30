/**
 * What the outline view and the breadcrumbs are given about the front editor
 * (`docs/plans/editor-projekt-werkzeuge.md`, #49): its symbols and where the cursor is.
 */

import type { Tree } from "web-tree-sitter";

import type { TextStore } from "../editor/buffer";
import { outlineFromMarkdown, outlineFromTree, type OutlineSymbol } from "../editor/syntax/outline";

export interface OutlineInfo {
  /** The file's symbols; `null` when it has none to show (plain text, a data file, the light mode). */
  symbols: OutlineSymbol[] | null;
  /** The cursor's line (zero-based). */
  line: number;
}

export const MARKDOWN_FILE = /\.(md|markdown)$/i;

/** The outline of one document: Markdown headings, or the syntax tree's symbols; `null` if there is neither. */
export function symbolsOf(store: TextStore, fileName: string, tree: Tree | null): OutlineSymbol[] | null {
  if (MARKDOWN_FILE.test(fileName)) return outlineFromMarkdown(store);
  return tree ? outlineFromTree(tree, store) : null;
}
