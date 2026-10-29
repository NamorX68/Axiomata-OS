import { describe, expect, it } from "vitest";

import { EditorDocument } from "../editor/document";
import { pos } from "../editor/position";
import type { Span } from "../editor/syntax/paint";
import { noteMarkAt, withNoteMarks } from "./noteHighlight";

const doc = (text: string) => new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });

describe("withNoteMarks (TL2)", () => {
  it("paints links, dangling links and tags over the grammar's colours, never in code", () => {
    const d = doc("# [[Here]] and [[Gone]] #tag\n```\n[[Code]] #code\n```\n`[[x]]` [[Here]]");
    const base = {
      spans: () => new Map<number, Span[]>([[0, [{ from: 0, to: 28, token: "heading" }]]]),
      syntaxTree: () => null,
    };
    const shown = withNoteMarks(base, d, (m) => m.target === "Here");
    const spans = shown.spans(0, 4);
    expect(spans.get(0)).toEqual([
      { from: 0, to: 2, token: "heading" },
      { from: 2, to: 10, token: "note-link" },
      { from: 10, to: 15, token: "heading" },
      { from: 15, to: 23, token: "note-link-missing" },
      { from: 23, to: 24, token: "heading" },
      { from: 24, to: 28, token: "note-tag" },
    ]);
    expect(spans.get(2)).toBeUndefined();
    expect(spans.get(4)).toEqual([{ from: 8, to: 16, token: "note-link" }]);
    expect(shown.syntaxTree?.()).toBeNull();
  });

  it("does not dim a link while the vault is not known, and works without a grammar", () => {
    const shown = withNoteMarks(null, doc("[[Any]]"), () => null);
    expect(shown.spans(0, 0).get(0)).toEqual([{ from: 0, to: 7, token: "note-link" }]);
    expect(shown.syntaxTree).toBeUndefined();
  });

  it("follows the text as it changes", () => {
    const d = doc("```\n[[A]]");
    const shown = withNoteMarks(null, d, () => true);
    expect(shown.spans(0, 1).get(1)).toBeUndefined();
    d.edit([{ range: { start: pos(0, 0), end: pos(0, 3) }, text: "x" }], { anchor: pos(0, 1), head: pos(0, 1) });
    expect(shown.spans(0, 1).get(1)).toEqual([{ from: 0, to: 5, token: "note-link" }]);
  });

  it("finds the mark under a position, not in code", () => {
    const d = doc("[[A]]\n```\n[[B]]\n```");
    expect(noteMarkAt(d, pos(0, 2))).toMatchObject({ kind: "link", target: "A" });
    expect(noteMarkAt(d, pos(2, 2))).toBeNull();
  });
});
