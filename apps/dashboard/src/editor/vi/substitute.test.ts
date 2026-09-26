import { describe, expect, it } from "vitest";

import { LineStore } from "../buffer";
import { pos, range } from "../position";
import { expandReplacement, parseSubstitute, substituteLines, substituteSpanning, type SubstituteSpec } from "./substitute";

const spec = (over: Partial<SubstituteSpec> = {}): SubstituteSpec => ({
  pattern: "a",
  replacement: "b",
  global: false,
  caseMode: "smart",
  ...over,
});

describe("parseSubstitute", () => {
  it("reads pattern, replacement and flags with any delimiter", () => {
    expect(parseSubstitute("/foo/bar/gi", null)).toEqual(
      spec({ pattern: "foo", replacement: "bar", global: true, caseMode: "ignore" }),
    );
    expect(parseSubstitute("#a/b#c#", null)).toEqual(spec({ pattern: "a/b", replacement: "c" }));
  });

  it("makes an escaped delimiter literal, escaping it again where the regex needs it", () => {
    expect(parseSubstitute("/a\\/b/c/", null)).toMatchObject({ pattern: "a\\/b" });
    expect(parseSubstitute("#a\\#b#c#", null)).toMatchObject({ pattern: "a#b" });
    expect(parseSubstitute("|a\\|b|c|", null)).toHaveProperty("error");
    expect(parseSubstitute(".a\\.b.c.", null)).toMatchObject({ pattern: "a\\.b" });
  });

  it("allows the closing delimiters to be left off", () => {
    expect(parseSubstitute("/foo", null)).toEqual(spec({ pattern: "foo", replacement: "" }));
    expect(parseSubstitute("/foo/bar", null)).toEqual(spec({ pattern: "foo", replacement: "bar" }));
  });

  it("repeats the last one without its flags for a bare :s, with them after &", () => {
    const last = spec({ pattern: "x", replacement: "y", global: true, caseMode: "match" });
    expect(parseSubstitute("", last)).toEqual(spec({ pattern: "x", replacement: "y" }));
    expect(parseSubstitute("&", last)).toEqual(last);
    expect(parseSubstitute("/p/q/&", last)).toMatchObject({ pattern: "p", global: true, caseMode: "match" });
    expect(parseSubstitute("", null)).toEqual({ error: "E35: No previous regular expression" });
  });

  it("refuses a letter as delimiter and unknown flags", () => {
    expect(parseSubstitute("xaxbx", null)).toHaveProperty("error");
    expect(parseSubstitute("/a/b/z", null)).toEqual({ error: "E488: Trailing characters: z" });
  });
});

describe("expandReplacement", () => {
  const m = /(\w+)-(\w+)/.exec("foo-bar")!;

  it.each([
    ["&", "foo-bar"],
    ["\\0", "foo-bar"],
    ["\\2 \\1", "bar foo"],
    ["\\&", "&"],
    ["\\\\", "\\"],
    ["a\\rb", "a\nb"],
    ["a\\nb", "a\nb"],
    ["\\t", "\t"],
    ["\\u\\1", "Foo"],
    ["\\U\\1\\E-\\2", "FOO-bar"],
    ["\\L\\U&", "FOO-BAR"],
    ["~!", "prev!"],
    ["\\~", "~"],
  ])("%s", (replacement, expected) => {
    expect(expandReplacement(replacement, m, "prev")).toBe(expected);
  });
});

describe("substituteLines", () => {
  const store = new LineStore("a a\nb\na");

  it("replaces the first match on each line, or all with g, bottom line first", () => {
    expect(substituteLines(store, { first: 0, last: 2 }, spec(), "")).toEqual({
      lines: [
        { line: 2, text: "b" },
        { line: 0, text: "b a" },
      ],
      count: 2,
      lastLine: 2,
    });
    expect(substituteLines(store, { first: 0, last: 0 }, spec({ global: true }), "")).toMatchObject({
      lines: [{ line: 0, text: "b b" }],
      count: 2,
    });
  });

  it("handles empty matches with g without looping", () => {
    const everyGap = spec({ pattern: "x*", replacement: "-", global: true });
    expect(substituteLines(new LineStore("ab"), { first: 0, last: 0 }, everyGap, "")).toMatchObject({
      lines: [{ line: 0, text: "-a-b-" }],
    });
  });

  it("lets \\c in the pattern override the I flag that forces exact case", () => {
    const spec: SubstituteSpec = { pattern: "foo\\c", replacement: "x", global: false, caseMode: "match" };
    expect(substituteLines(new LineStore("FOO"), { first: 0, last: 0 }, spec, "")).toMatchObject({
      lines: [{ line: 0, text: "x" }],
    });
  });

  it("says E486 when nothing matched, and passes a bad pattern's error on", () => {
    expect(substituteLines(store, { first: 1, last: 1 }, spec(), "")).toEqual({ error: "E486: Pattern not found: a" });
    expect(substituteLines(store, { first: 0, last: 0 }, spec({ pattern: "(" }), "")).toHaveProperty("error");
  });
});

describe("substituteSpanning", () => {
  it("joins lines on a pattern naming a line break", () => {
    const store = new LineStore("a,\nb");
    expect(substituteSpanning(store, { first: 0, last: 1 }, spec({ pattern: ",\\n", replacement: " " }), "")).toEqual({
      range: range(pos(0, 0), pos(1, 1)),
      text: "a b",
      count: 1,
      lastLine: 0,
    });
  });

  it("without g, caps at one replacement per starting line, even when a second match also starts on it", () => {
    // "x" matches alone (no break) right before "a\n" also starting on line 0: the alternation lets
    // two matches start on the very same line, which a plain join pattern (always eating a break) cannot.
    const store = new LineStore("xa\nb\na\nc");
    const result = substituteSpanning(store, { first: 0, last: 3 }, spec({ pattern: "x|a\\n", replacement: "Z" }), "");
    // "x" (line 0) and the second "a\n" (line 2, "a" before "c") are replaced; the "a\n" that would be
    // a *second* match starting on line 0 is left alone, as the docstring promises.
    expect(result).toMatchObject({ text: "Za\nb\nZc", count: 2 });
  });

  it("with g, replaces every match, including back-to-back ones", () => {
    const store = new LineStore("a,\nb,\nc,\nd");
    const result = substituteSpanning(
      store,
      { first: 0, last: 3 },
      spec({ pattern: ",\\n", replacement: " ", global: true }),
      "",
    );
    expect(result).toMatchObject({ text: "a b c d", count: 3 });
  });

  it("puts a real line break in with \\r, growing the range it changed", () => {
    const store = new LineStore("a b\nc");
    const result = substituteSpanning(store, { first: 0, last: 1 }, spec({ pattern: " ", replacement: "\\r" }), "");
    expect(result).toMatchObject({ text: "a\nb\nc", count: 1 });
  });

  it("handles an empty match without looping forever", () => {
    const store = new LineStore("a\nb");
    const result = substituteSpanning(
      store,
      { first: 0, last: 1 },
      spec({ pattern: "x*\\n?", replacement: "-", global: true }),
      "",
    );
    expect("error" in result).toBe(false);
  });

  it("says E486 when nothing matched, and passes a bad pattern's error on", () => {
    const store = new LineStore("a\nb");
    expect(substituteSpanning(store, { first: 0, last: 1 }, spec({ pattern: "z\\n" }), "")).toEqual({
      error: "E486: Pattern not found: z\\n",
    });
    expect(substituteSpanning(store, { first: 0, last: 1 }, spec({ pattern: "(\\n" }), "")).toHaveProperty("error");
  });
});
