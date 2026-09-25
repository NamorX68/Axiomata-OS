/**
 * Syntax colours for a document, started the one way every editor surface
 * does it (`docs/plans/editor.md`, G4): the language from the file's name (and
 * first line), its grammar awaited, the highlighter handed over — or disposed
 * at once when the caller moved on meanwhile (`stale`), so a slow grammar never
 * leaves a parser behind. What is handed over, the caller disposes.
 */

import type { EditorDocument } from "../editor/document";
import { SyntaxHighlighter } from "../editor/syntax/highlighter";
import { detectLanguage } from "../editor/syntax/languages";
import { grammarRuntime } from "./grammars";

export interface HighlightOptions {
  /** Picks the language, with the document's first line (a shebang, a modeline). */
  fileName?: string;
  /** A language id instead of detecting one (the settings panel's TypeScript sample). */
  language?: string;
  /** An injected grammar arrived later: the colours changed, redraw. */
  onColours?: () => void;
  /** Asked once the grammar is there: `true` drops the result. */
  stale?: () => boolean;
}

/** A highlighter for `doc`, or `null` for plain text, an unknown language, or a caller that moved on. */
export async function highlightFor(doc: EditorDocument, options: HighlightOptions): Promise<SyntaxHighlighter | null> {
  const id = options.language ?? detectLanguage(options.fileName ?? "", doc.store.line(0));
  if (!id) return null;
  const created = await SyntaxHighlighter.create(doc, grammarRuntime, id, options.onColours);
  if (options.stale?.()) {
    created?.dispose();
    return null;
  }
  return created;
}
