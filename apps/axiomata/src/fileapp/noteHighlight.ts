/**
 * A Markdown note's wikilinks and tags in their own colours (`docs/plans/tags-links.md`, TL2), painted over whatever
 * the grammar coloured: a wrapper the surface takes for its `highlighter`, answering the same two questions.
 *
 * * **Code stays code**: fenced blocks, the frontmatter and inline code carry no links or tags (`noteLinks.ts`); which
 *   lines are code is worked out once per state of the text, not per frame.
 * * **A link into nothing is dimmed** — once the vault's notes are known (`resolve`); until then every link looks
 *   alive, so a slow first scan never flashes them all as dangling.
 */

import type { Tree } from "web-tree-sitter";

import type { EditorDocument } from "../editor/document";
import type { Pos } from "../editor/position";
import { overlaySpans, type Span } from "../editor/syntax/paint";
import { codeLines, lineMarks, type NoteMark } from "./noteLinks";

/** What the surface asks of a highlighter (`EditorSurface`'s `highlighter` prop). */
export interface SpanSource {
  spans(first: number, last: number, options?: { brackets?: boolean }): Map<number, Span[]>;
  syntaxTree?(): Tree | null;
}

/** Whether a link's target exists: `true`/`false`, or `null` while the vault is not known yet. */
export type LinkExists = (mark: Extract<NoteMark, { kind: "link" }>) => boolean | null;

/** The spans of one line's marks. */
export function markSpans(marks: readonly NoteMark[], exists: LinkExists): Span[] {
  return marks.map((m) => ({
    from: m.from,
    to: m.to,
    token: m.kind === "tag" ? "note-tag" : exists(m) === false ? "note-link-missing" : "note-link",
  }));
}

/** `base` (the grammar's colours, if any) with the note's links and tags painted over. */
export function withNoteMarks(base: SpanSource | null, doc: EditorDocument, exists: LinkExists): SpanSource {
  let codeAt = -1;
  let code = new Set<number>();
  const inCode = (line: number) => {
    if (codeAt !== doc.revision) {
      codeAt = doc.revision;
      code = codeLines({ lineCount: doc.store.lineCount(), line: (n) => doc.store.line(n) });
    }
    return code.has(line);
  };
  const source: SpanSource = {
    spans(first, last, options) {
      const spans = base ? base.spans(first, last, options) : new Map<number, Span[]>();
      const end = Math.min(last, doc.store.lineCount() - 1);
      for (let line = first; line <= end; line++) {
        if (inCode(line)) continue;
        const text = doc.store.line(line);
        if (!text.includes("[[") && !text.includes("#")) continue;
        const extra = markSpans(lineMarks(text), exists);
        if (extra.length > 0) spans.set(line, overlaySpans(spans.get(line), extra));
      }
      return spans;
    },
  };
  // Vi's syntax text objects read the grammar's tree; a wrapper without a grammar has none.
  if (base?.syntaxTree) source.syntaxTree = () => base.syntaxTree?.() ?? null;
  return source;
}

/** The link or tag at `at` in `doc` — never one in code (a fenced block, the frontmatter, inline code). */
export function noteMarkAt(doc: EditorDocument, at: Pos): NoteMark | null {
  if (at.line >= doc.store.lineCount()) return null;
  const code = codeLines({ lineCount: doc.store.lineCount(), line: (n) => doc.store.line(n) });
  if (code.has(at.line)) return null;
  return lineMarks(doc.store.line(at.line)).find((m) => at.col >= m.from && at.col < m.to) ?? null;
}
