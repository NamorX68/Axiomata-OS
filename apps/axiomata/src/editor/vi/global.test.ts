import { describe, expect, it } from "vitest";

import { EditorDocument } from "../document";
import { cursor, pos, range } from "../position";
import { LineAnchors, markedLines } from "./global";
import { LineStore } from "../buffer";

describe("markedLines", () => {
  it("marks the lines with a match, or without one", () => {
    const store = new LineStore("a1\nb\na2\nc");
    const all = { first: 0, last: 3 };
    expect(markedLines(store, /a/gu, all, false)).toEqual([0, 2]);
    expect(markedLines(store, /a/gu, all, true)).toEqual([1, 3]);
    expect(markedLines(store, /a/gu, { first: 1, last: 3 }, false)).toEqual([2]);
  });

  it("marks the line a match across a break starts on", () => {
    expect(markedLines(new LineStore("x\nab\ncd\nab"), /b\nc/gu, { first: 0, last: 3 }, false)).toEqual([1]);
  });
});

describe("LineAnchors", () => {
  function follow(text: string, anchors: number[], edit: (doc: EditorDocument) => void): Array<number | null> {
    const doc = new EditorDocument(text, { indentFallback: { kind: "spaces", width: 2 } as never });
    const a = new LineAnchors(anchors);
    doc.onTextChange((c) => a.change(c));
    edit(doc);
    return anchors.map((_, i) => a.at(i));
  }

  it("follows a deleted line: gone itself, the ones below moved up", () => {
    const got = follow("0\n1\n2\n3", [0, 1, 2, 3], (d) =>
      d.edit([{ range: range(pos(1, 0), pos(2, 0)), text: "" }], cursor(pos(1, 0))),
    );
    expect(got).toEqual([0, null, 1, 2]);
  });

  it("follows the last lines being deleted", () => {
    const got = follow("0\n1\n2", [0, 1, 2], (d) =>
      d.edit([{ range: range(pos(0, 1), pos(2, 1)), text: "" }], cursor(pos(0, 0))),
    );
    expect(got).toEqual([0, null, null]);
  });

  it("moves lines below a split line down, and keeps the split one", () => {
    const got = follow("ab\ncd", [0, 1], (d) =>
      d.edit([{ range: range(pos(0, 1), pos(0, 1)), text: "\n" }], cursor(pos(1, 0))),
    );
    expect(got).toEqual([0, 2]);
  });
});
