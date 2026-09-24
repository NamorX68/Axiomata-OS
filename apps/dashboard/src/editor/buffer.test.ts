import { describe, expect, it } from "vitest";

import { LineStore, clampPos, endOfText, splitLines, type TextStore } from "./buffer";
import { pos, range } from "./position";

// Written against `TextStore`, not `LineStore`: the rope that replaces it in ED5
// must pass the same suite (F1).
function stores(): [string, (text: string) => TextStore][] {
  return [["LineStore", (text) => new LineStore(text)]];
}

describe.each(stores())("%s", (_name, make) => {
  it("keeps an empty document as one empty line", () => {
    const s = make("");
    expect(s.lineCount()).toBe(1);
    expect(s.line(0)).toBe("");
    expect(endOfText(s)).toEqual(pos(0, 0));
  });

  it("splits on LF, CRLF and a lone CR", () => {
    expect(splitLines("a\nb\r\nc\rd")).toEqual(["a", "b", "c", "d"]);
    expect(make("a\r\nb").text()).toBe("a\nb");
  });

  it("slices within a line and across lines", () => {
    const s = make("alpha\nbeta\ngamma");
    expect(s.slice(range(pos(0, 1), pos(0, 3)))).toBe("lp");
    expect(s.slice(range(pos(0, 3), pos(2, 2)))).toBe("ha\nbeta\nga");
    expect(s.slice(range(pos(1, 4), pos(2, 0)))).toBe("\n");
  });

  it("replaces and returns the position after the inserted text", () => {
    const s = make("alpha\nbeta");
    expect(s.replace(range(pos(0, 2), pos(0, 4)), "XY")).toEqual(pos(0, 4));
    expect(s.text()).toBe("alXYa\nbeta");
    expect(s.replace(range(pos(0, 5), pos(1, 0)), "")).toEqual(pos(0, 5));
    expect(s.text()).toBe("alXYabeta");
    expect(s.replace(range(pos(0, 2), pos(0, 2)), "1\n22\n3")).toEqual(pos(2, 1));
    expect(s.text()).toBe("al1\n22\n3XYabeta");
  });

  it("refuses a reversed range or a position outside the text", () => {
    const s = make("ab\ncd");
    expect(() => s.slice(range(pos(0, 0), pos(0, 3)))).toThrow(RangeError);
    expect(() => s.replace({ start: pos(1, 0), end: pos(0, 0) }, "")).toThrow(RangeError);
    expect(() => s.line(2)).toThrow(RangeError);
  });

  it("clamps a position into the text", () => {
    const s = make("ab\ncdef");
    expect(clampPos(s, pos(-1, 5))).toEqual(pos(0, 2));
    expect(clampPos(s, pos(9, 9))).toEqual(pos(1, 4));
  });
});
