import { describe, expect, it } from "vitest";

import { pos, range } from "../position";
import { Rope } from "../rope";
import { allMatches } from "./matches";
import { jumpTo, matchesOnLines } from "./jump";

function setup(text: string, re: RegExp) {
  const rope = Rope.of(text);
  return { rope, offsets: allMatches(rope.text(), re, 1000).offsets };
}

describe("jumpTo", () => {
  it("goes to the next match after the cursor, not the one it is on", () => {
    const { rope, offsets } = setup("foo foo\nfoo", /foo/gu);
    expect(jumpTo(rope, offsets, pos(0, 0), false)).toEqual({ range: range(pos(0, 4), pos(0, 7)), wrapped: false });
    expect(jumpTo(rope, offsets, pos(0, 4), false)?.range.start).toEqual(pos(1, 0));
  });

  it("wraps around both ends and says so", () => {
    const { rope, offsets } = setup("foo\nbar\nfoo", /foo/gu);
    expect(jumpTo(rope, offsets, pos(2, 0), false)).toEqual({ range: range(pos(0, 0), pos(0, 3)), wrapped: true });
    expect(jumpTo(rope, offsets, pos(0, 0), true)).toEqual({ range: range(pos(2, 0), pos(2, 3)), wrapped: true });
    expect(jumpTo(rope, offsets, pos(2, 0), true)?.range.start).toEqual(pos(0, 0));
  });

  it("counts: 3n goes three matches on", () => {
    const { rope, offsets } = setup("a a a a", /a/gu);
    expect(jumpTo(rope, offsets, pos(0, 0), false, 3)?.range.start).toEqual(pos(0, 6));
    expect(jumpTo(rope, offsets, pos(0, 0), false, 5)).toEqual({ range: range(pos(0, 2), pos(0, 3)), wrapped: true });
  });

  it("returns null when there is no match", () => {
    const { rope, offsets } = setup("abc", /z/gu);
    expect(jumpTo(rope, offsets, pos(0, 0), false)).toBeNull();
  });

  it("reaches a match across a line break", () => {
    const { rope, offsets } = setup("ab\ncd\nab\ncd", /b\nc/gu);
    expect(jumpTo(rope, offsets, pos(0, 0), false)).toEqual({ range: range(pos(0, 1), pos(1, 1)), wrapped: false });
  });
});

describe("matchesOnLines", () => {
  it("lists the matches of the lines asked for", () => {
    const { rope, offsets } = setup("ab ab\nx\nab", /ab/gu);
    expect([...matchesOnLines(rope, offsets, 0, 1)]).toEqual([[0, [[0, 2], [3, 5]]]]);
    expect([...matchesOnLines(rope, offsets, 2, 9)]).toEqual([[2, [[0, 2]]]]);
  });

  it("shows a match across a break on each line it touches, even from above the window", () => {
    const { rope, offsets } = setup("xab\ncdy", /ab\ncd/gu);
    expect([...matchesOnLines(rope, offsets, 0, 1)]).toEqual([
      [0, [[1, 3]]],
      [1, [[0, 2]]],
    ]);
    expect([...matchesOnLines(rope, offsets, 1, 1)]).toEqual([[1, [[0, 2]]]]);
  });

  it("leaves out empty matches", () => {
    const { rope, offsets } = setup("abc", /x*/gu);
    expect(matchesOnLines(rope, offsets, 0, 0).size).toBe(0);
  });
});
