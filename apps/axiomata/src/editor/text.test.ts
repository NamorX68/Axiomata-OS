import { describe, expect, it } from "vitest";

import {
  colForDisplayColumn,
  displayColumn,
  graphemeCount,
  leadingWhitespace,
  nextGrapheme,
  nextWordEnd,
  prevGrapheme,
  prevWordStart,
  wordAt,
} from "./text";

describe("graphemes", () => {
  it("steps over a surrogate pair and a combining accent as one character", () => {
    const line = "a😀éb"; // a, emoji (2 units), e + combining acute (2 units), b
    expect(nextGrapheme(line, 0)).toBe(1);
    expect(nextGrapheme(line, 1)).toBe(3);
    expect(nextGrapheme(line, 3)).toBe(5);
    expect(prevGrapheme(line, 5)).toBe(3);
    expect(prevGrapheme(line, 3)).toBe(1);
    expect(graphemeCount(line)).toBe(4);
  });

  it("stops at the ends of the line", () => {
    expect(nextGrapheme("ab", 2)).toBe(2);
    expect(prevGrapheme("ab", 0)).toBe(0);
  });
});

describe("words", () => {
  it("jumps like ⌥←/⌥→ on a Mac: over punctuation to the next word", () => {
    const line = "let foo = bar.baz();";
    expect(nextWordEnd(line, 0)).toBe(3);
    expect(nextWordEnd(line, 3)).toBe(7);
    expect(nextWordEnd(line, 7)).toBe(13);
    expect(prevWordStart(line, 13)).toBe(10);
    expect(prevWordStart(line, 10)).toBe(4);
    expect(prevWordStart(line, 2)).toBe(0);
    expect(nextWordEnd("  ", 0)).toBe(2);
  });

  it("finds the word — or the run of punctuation — under a double-click", () => {
    const line = "foo(bar) baz";
    expect(wordAt(line, 5)).toEqual({ start: 4, end: 7 });
    expect(wordAt(line, 4)).toEqual({ start: 4, end: 7 });
    expect(wordAt(line, 12)).toEqual({ start: 9, end: 12 });
    expect(wordAt("Umlaute: Größe", 11)).toEqual({ start: 9, end: 14 });
  });
});

describe("display columns", () => {
  it("expands tabs to the next stop", () => {
    expect(displayColumn("\tx", 1, 4)).toBe(4);
    expect(displayColumn("ab\tx", 3, 4)).toBe(4);
    expect(displayColumn("a😀b", 3, 4)).toBe(3);
    expect(displayColumn("日本x", 2, 4)).toBe(4);
  });

  it("maps a display column back to the nearest real column", () => {
    expect(colForDisplayColumn("\tabc", 5, 4)).toBe(2);
    expect(colForDisplayColumn("\tabc", 1, 4)).toBe(0);
    expect(colForDisplayColumn("\tabc", 3, 4)).toBe(1);
    expect(colForDisplayColumn("ab", 9, 4)).toBe(2);
    expect(colForDisplayColumn("a😀b", 3, 4)).toBe(3);
    expect(colForDisplayColumn("a😀b", 1, 4)).toBe(1);
  });

  it("reads leading whitespace", () => {
    expect(leadingWhitespace("  \tx ")).toBe("  \t");
    expect(leadingWhitespace("x")).toBe("");
  });
});
