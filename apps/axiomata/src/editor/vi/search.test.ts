import { describe, expect, it } from "vitest";

import { LineStore } from "../buffer";
import { pos } from "../position";
import { compilePattern, findMatch, lineMatches, matchesIn, wordPattern } from "./search";

function re(pattern: string, mode?: "smart" | "ignore" | "match"): RegExp {
  const c = compilePattern(pattern, mode);
  if ("error" in c) throw new Error(c.error);
  return c.re;
}

describe("compilePattern (V7)", () => {
  it("ignores case for a lower-case pattern and matches it for one with a capital (smartcase)", () => {
    expect(re("foo").test("FOO")).toBe(true);
    expect(re("Foo").test("foo")).toBe(false);
    expect(re("Foo").test("Foo")).toBe(true);
  });

  it("lets \\c and \\C override smartcase, wherever they stand", () => {
    expect(re("Foo\\c").test("foo")).toBe(true);
    expect(re("\\Cfoo").test("FOO")).toBe(false);
  });

  it("does not count an escaped capital (\\S, \\W) as a capital", () => {
    expect(re("a\\Sc").test("AXC")).toBe(true);
  });

  it("follows :s's i and I flags", () => {
    expect(re("Foo", "ignore").test("foo")).toBe(true);
    expect(re("foo", "match").test("FOO")).toBe(false);
  });

  it("turns \\< and \\> into word edges, umlauts counting as letters", () => {
    expect(lineMatches("cat concat cats", re("\\<cat\\>"))).toEqual([[0, 3]]);
    expect(lineMatches("über überall", re("\\<über\\>"))).toEqual([[0, 4]]);
  });

  it("keeps \\< inside a character class as it is", () => {
    expect(lineMatches("a<b", re("[\\<]"))).toEqual([[1, 2]]);
  });

  it("reports a broken pattern instead of throwing", () => {
    expect(compilePattern("(")).toHaveProperty("error");
  });
});

describe("wordPattern (* and #)", () => {
  it("searches a word as a whole word, case exactly, and escapes what regexes treat specially", () => {
    expect(wordPattern("foo")).toBe("\\<foo\\>\\C");
    expect(lineMatches("Foo foo food", re(wordPattern("foo")))).toEqual([[4, 7]]);
    expect(wordPattern("$x")).toBe("\\$x\\C");
  });

  it("escapes regex specials in the middle of a word too, keeping the word edges", () => {
    expect(wordPattern("a.b+c")).toBe("\\<a\\.b\\+c\\>\\C");
    expect(lineMatches("a.b+c aXbXc", re(wordPattern("a.b+c")))).toEqual([[0, 5]]);
  });
});

describe("lineMatches", () => {
  it("steps past empty matches instead of looping", () => {
    expect(lineMatches("ab", re("x*"))).toEqual([
      [0, 0],
      [1, 1],
      [2, 2],
    ]);
  });

  it("stops at the limit", () => {
    expect(lineMatches("aaaa", re("a"), 2)).toHaveLength(2);
  });

  it("finds an astral character (emoji) as one match, columns counted in UTF-16 units", () => {
    expect(lineMatches("a\u{1F600}b\u{1F600}c", re("\u{1F600}"))).toEqual([
      [1, 3],
      [4, 6],
    ]);
  });

  it("steps past an empty match by the whole surrogate pair, not into the middle of it", () => {
    expect(lineMatches("\u{1F600}b", re("x*"))).toEqual([
      [0, 0],
      [2, 2],
      [3, 3],
    ]);
  });
});

describe("findMatch", () => {
  const store = new LineStore("foo bar\nbaz foo\nfoo");

  it("finds the next match after the cursor, not the one it stands on", () => {
    expect(findMatch(store, re("foo"), pos(0, 0), false)).toEqual({
      range: { start: pos(1, 4), end: pos(1, 7) },
      wrapped: false,
    });
  });

  it("goes backwards with `?`", () => {
    expect(findMatch(store, re("foo"), pos(2, 0), true)?.range.start).toEqual(pos(1, 4));
  });

  it("wraps around the end and says so", () => {
    expect(findMatch(store, re("foo"), pos(2, 0), false)).toEqual({
      range: { start: pos(0, 0), end: pos(0, 3) },
      wrapped: true,
    });
    expect(findMatch(store, re("foo"), pos(0, 0), true)).toMatchObject({ range: { start: pos(2, 0) }, wrapped: true });
  });

  it("counts: the third match from here", () => {
    expect(findMatch(store, re("foo"), pos(0, 0), false, 3)?.range.start).toEqual(pos(0, 0));
  });

  it("finds a lone match again after going all the way round", () => {
    const one = new LineStore("a x b");
    expect(findMatch(one, re("x"), pos(0, 2), false)).toMatchObject({ range: { start: pos(0, 2) }, wrapped: true });
  });

  it("returns null when the pattern is nowhere", () => {
    expect(findMatch(store, re("nope"), pos(0, 0), false)).toBeNull();
  });

  it("counts backward across a wrap, remembering that it wrapped even once it stops wrapping", () => {
    // From (0,0) there is no earlier "foo" on line 0 itself, so the first step already wraps to
    // line 2; the second step then finds line 1 without wrapping again, but `wrapped` stays true.
    expect(findMatch(store, re("foo"), pos(0, 0), true, 2)).toEqual({
      range: { start: pos(1, 4), end: pos(1, 7) },
      wrapped: true,
    });
  });
});

describe("matchesIn (hlsearch)", () => {
  it("lists the non-empty matches of the lines asked for", () => {
    const store = new LineStore("a a\nb\na");
    expect([...matchesIn(store, re("a"), 0, 5)]).toEqual([
      [
        0,
        [
          [0, 1],
          [2, 3],
        ],
      ],
      [2, [[0, 1]]],
    ]);
    expect(matchesIn(store, re("x*"), 0, 2).size).toBe(0);
  });
});
