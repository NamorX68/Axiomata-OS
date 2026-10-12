import { describe, expect, it } from "vitest";

import { trailOf } from "./trail";

describe("trailOf", () => {
  it("marks a changed line and puts the flag there", () => {
    expect(trailOf("a\nb\nc", "a\nB\nc")).toEqual({ lines: [1], last: 1 });
  });

  it("marks inserted lines, the flag at the last of the last block", () => {
    expect(trailOf("a\nd", "a\nb\nc\nd\ne")).toEqual({ lines: [1, 2, 4], last: 4 });
  });

  it("marks the line that stands where lines were removed", () => {
    expect(trailOf("a\nb\nc\nd", "a\nd")).toEqual({ lines: [1], last: 1 });
  });

  it("keeps a removal at the end inside the text", () => {
    expect(trailOf("a\nb\nc", "a\nb")).toEqual({ lines: [1], last: 1 });
  });

  it("marks lines inserted or removed at the very start", () => {
    expect(trailOf("b\nc", "a\nb\nc")).toEqual({ lines: [0], last: 0 });
    expect(trailOf("a\nb\nc", "b\nc")).toEqual({ lines: [0], last: 0 });
  });

  it("marks a whole text that came into an empty file", () => {
    expect(trailOf("", "a\nb")).toEqual({ lines: [0, 1], last: 1 });
  });

  it("puts the flag on the new line of a replacement, not on the gap of the old one", () => {
    expect(trailOf("a\nold\nc", "a\nnew one\nc")).toEqual({ lines: [1], last: 1 });
  });

  it("finds nothing when nothing changed", () => {
    expect(trailOf("same\ntext", "same\ntext")).toEqual({ lines: [], last: null });
  });
});
