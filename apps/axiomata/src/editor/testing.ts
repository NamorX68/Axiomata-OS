/**
 * Test helpers for the editor model: a document written as text with markers.
 *
 * `|` is the cursor (the selection's head), `^` the anchor when there is a
 * selection: `"let ^x|= 1"` is `x` selected left to right. `show` prints a
 * document back in the same notation, so a test reads as before → after.
 */

import { UNWRAPPED, type CommandContext } from "./commands";
import { EditorDocument } from "./document";
import { comparePos, pos, type Pos } from "./position";

function find(text: string, marker: string): { at: Pos | null; text: string } {
  const index = text.indexOf(marker);
  if (index < 0) return { at: null, text };
  const before = text.slice(0, index).split("\n");
  const at = pos(before.length - 1, before[before.length - 1].length);
  return { at, text: text.slice(0, index) + text.slice(index + 1) };
}

/** A document from marked-up text; four-space indentation unless the text says otherwise. */
export function docFrom(marked: string, fallback = { kind: "spaces" as const, size: 4 }): EditorDocument {
  // Remove the earlier marker first; the later one is then found in the text
  // it will really sit in.
  const caretFirst = marked.indexOf("^") < 0 || marked.indexOf("|") < marked.indexOf("^");
  let text = marked;
  let head: Pos | null;
  let anchor: Pos | null;
  if (caretFirst) {
    ({ at: head, text } = find(text, "|"));
    ({ at: anchor, text } = find(text, "^"));
  } else {
    ({ at: anchor, text } = find(text, "^"));
    ({ at: head, text } = find(text, "|"));
  }
  const doc = new EditorDocument(text, { indentFallback: fallback });
  const h = head ?? pos(0, 0);
  doc.setSelection({ anchor: anchor ?? h, head: h });
  return doc;
}

/** The document's text with `|` and `^` put back in. */
export function show(doc: EditorDocument): string {
  const lines = doc.store.text().split("\n");
  const marks: { at: Pos; mark: string }[] = [{ at: doc.selection.head, mark: "|" }];
  if (comparePos(doc.selection.anchor, doc.selection.head) !== 0) marks.push({ at: doc.selection.anchor, mark: "^" });
  // Insert right to left so earlier columns stay valid.
  marks.sort((a, b) => comparePos(b.at, a.at));
  for (const { at, mark } of marks) {
    const line = lines[at.line];
    lines[at.line] = line.slice(0, at.col) + mark + line.slice(at.col);
  }
  return lines.join("\n");
}

export function ctx(overrides: Partial<CommandContext> = {}): CommandContext {
  return { tabSize: 4, layout: UNWRAPPED, pageRows: 3, commentPrefix: "//", now: 0, ...overrides };
}
