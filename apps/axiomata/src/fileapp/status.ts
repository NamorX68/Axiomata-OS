/**
 * The file app's status bar text (`docs/plans/editor.md`, F3, F7, F11): where
 * the cursor is — the column as a person counts it, graphemes with tabs
 * expanded — and the file's line ending and indentation.
 */

import type { EditorDocument } from "../editor/document";
import { displayColumn } from "../editor/text";
import { tEditor } from "../i18n/editor";

export interface StatusParts {
  position: string;
  eol: string;
  indent: string;
}

export function statusParts(doc: EditorDocument, tabSize: number): StatusParts {
  const head = doc.selection.head;
  const column = displayColumn(doc.store.line(head.line), head.col, tabSize) + 1;
  const eol = doc.shape.mixedEol ? tEditor("eolWasMixed") : doc.shape.eol === "crlf" ? "CRLF" : "LF";
  const size = doc.indent.size;
  const indent =
    doc.indent.kind === "tabs"
      ? tEditor("tabs")
      : doc.indentDetected
        ? tEditor("indentSpaces", { n: size })
        : tEditor("indentSpacesDefault", { n: size });
  return { position: tEditor("cursorPosition", { line: head.line + 1, col: column }), eol, indent };
}
