import { describe, expect, it } from "vitest";

import { paintLines, rowSegments, type Capture } from "./paint";
import type { SyntaxToken } from "./tokens";

function cap(sr: number, sc: number, er: number, ec: number, token: SyntaxToken, priority = 0): Capture {
  return { startRow: sr, startCol: sc, endRow: er, endCol: ec, token, priority };
}

const lengths = (widths: number[]) => (line: number) => widths[line] ?? 0;

describe("paintLines", () => {
  it("lets an inner capture override the outer one it sits in", () => {
    // "a\n" — a string with an escape inside.
    const spans = paintLines([[cap(0, 0, 0, 6, "string"), cap(0, 2, 0, 4, "constant")]], 0, 0, lengths([6]));
    expect(spans.get(0)).toEqual([
      { from: 0, to: 2, token: "string" },
      { from: 2, to: 4, token: "constant" },
      { from: 4, to: 6, token: "string" },
    ]);
  });

  it("gives an equal extent to the earlier pattern", () => {
    const spans = paintLines([[cap(0, 0, 0, 3, "type", 5), cap(0, 0, 0, 3, "variable", 2)]], 0, 0, lengths([3]));
    expect(spans.get(0)).toEqual([{ from: 0, to: 3, token: "variable" }]);
  });

  it("splits a multi-line capture over its lines and clips to the range asked for", () => {
    const spans = paintLines([[cap(0, 4, 3, 2, "comment")]], 1, 2, lengths([6, 5, 0, 4]));
    expect(spans.has(0)).toBe(false);
    expect(spans.get(1)).toEqual([{ from: 0, to: 5, token: "comment" }]);
    expect(spans.has(2)).toBe(false);
    expect(spans.has(3)).toBe(false);
  });

  it("paints an injected layer over its host", () => {
    const host = [cap(0, 0, 0, 10, "code")];
    const injected = [cap(0, 3, 0, 5, "keyword")];
    expect(paintLines([host, injected], 0, 0, lengths([10])).get(0)).toEqual([
      { from: 0, to: 3, token: "code" },
      { from: 3, to: 5, token: "keyword" },
      { from: 5, to: 10, token: "code" },
    ]);
  });

  it("ignores columns past the line's end", () => {
    expect(paintLines([[cap(0, 2, 0, 99, "string")]], 0, 0, lengths([4])).get(0)).toEqual([
      { from: 2, to: 4, token: "string" },
    ]);
  });
});

describe("rowSegments", () => {
  const spans = [
    { from: 0, to: 2, token: "keyword" as const },
    { from: 3, to: 7, token: "function" as const },
  ];

  it("covers the whole row, gaps uncoloured", () => {
    expect(rowSegments("fn main()", spans, 0, 9)).toEqual([
      { text: "fn", token: "keyword" },
      { text: " ", token: null },
      { text: "main", token: "function" },
      { text: "()", token: null },
    ]);
  });

  it("cuts spans at a wrapped row's edges", () => {
    expect(rowSegments("fn main()", spans, 5, 9)).toEqual([
      { text: "in", token: "function" },
      { text: "()", token: null },
    ]);
  });

  it("is one plain segment without spans", () => {
    expect(rowSegments("text", undefined, 1, 3)).toEqual([{ text: "ex", token: null }]);
  });
});
