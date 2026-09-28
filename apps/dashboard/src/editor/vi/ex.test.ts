import { describe, expect, it } from "vitest";

import { completeEx, parseEx, type ExContext } from "./ex";

const ctx: ExContext = {
  line: 4,
  lineCount: 10,
  mark: (name) => ({ a: 1, "<": 2, ">": 5 })[name] ?? null,
};

describe("parseEx (V6)", () => {
  it.each([
    ["w", { name: "write", quit: false, onlyIfChanged: false }],
    ["write", { name: "write", quit: false, onlyIfChanged: false }],
    ["wq", { name: "write", quit: true, onlyIfChanged: false }],
    ["x", { name: "write", quit: true, onlyIfChanged: true }],
    ["q", { name: "quit", force: false }],
    ["q!", { name: "quit", force: true }],
    ["e!", { name: "reload" }],
    ["noh", { name: "nohlsearch" }],
    ["nohlsearch", { name: "nohlsearch" }],
    ["format", { name: "format" }],
    ["for", { name: "format" }],
    ["rename new_name", { name: "rename", newName: "new_name" }],
    ["ren  x ", { name: "rename", newName: "x" }],
    ["rename", { error: "E471: Argument required" }],
  ])(":%s", (line, expected) => {
    expect(parseEx(line, ctx)).toEqual(expected);
  });

  it("goes to a line with :{n}, clamping past the end, and knows . $ and offsets", () => {
    expect(parseEx("3", ctx)).toEqual({ name: "goto", line: 2 });
    expect(parseEx("999", ctx)).toEqual({ name: "goto", line: 9 });
    expect(parseEx("$", ctx)).toEqual({ name: "goto", line: 9 });
    expect(parseEx(".+2", ctx)).toEqual({ name: "goto", line: 6 });
    expect(parseEx("-", ctx)).toEqual({ name: "goto", line: 3 });
  });

  it("gives :s its range: the cursor's line, %, n,m, marks", () => {
    expect(parseEx("s/a/b/", ctx)).toEqual({ name: "substitute", range: { first: 4, last: 4 }, args: "/a/b/" });
    expect(parseEx("%s/a/b/g", ctx)).toMatchObject({ range: { first: 0, last: 9 }, args: "/a/b/g" });
    expect(parseEx("2,3s#a#b#", ctx)).toMatchObject({ range: { first: 1, last: 2 }, args: "#a#b#" });
    expect(parseEx("'<,'>s/a/b/", ctx)).toMatchObject({ range: { first: 2, last: 5 } });
    expect(parseEx("substitute/a/b/", ctx)).toMatchObject({ name: "substitute", args: "/a/b/" });
  });

  it("puts a backwards range in order", () => {
    expect(parseEx("5,2s/a/b/", ctx)).toMatchObject({ range: { first: 1, last: 4 } });
  });

  it("applies +n/-n offsets to a mark address too, not only . and $", () => {
    expect(parseEx("'a+1", ctx)).toEqual({ name: "goto", line: 2 });
    expect(parseEx("'>-1", ctx)).toEqual({ name: "goto", line: 4 });
  });

  it("counts the second address from the first after ;, and from the cursor after ,", () => {
    // The cursor is on line 4 (zero-based); mark `a` is on line 1.
    expect(parseEx("'a;+2s/x/y/", ctx)).toMatchObject({ range: { first: 1, last: 3 } });
    expect(parseEx("'a,+2s/x/y/", ctx)).toMatchObject({ range: { first: 1, last: 6 } });
    expect(parseEx("2;3s/a/b/", ctx)).toMatchObject({ range: { first: 1, last: 2 } });
  });

  it("reads :& and :&&", () => {
    expect(parseEx("&", ctx)).toEqual({ name: "repeatSubstitute", range: { first: 4, last: 4 }, keepFlags: false });
    expect(parseEx("%&&", ctx)).toMatchObject({ name: "repeatSubstitute", keepFlags: true });
  });

  it("reads :set with no/inv/! and the short names", () => {
    expect(parseEx("set wrap", ctx)).toEqual({ name: "set", option: "wrap", value: true });
    expect(parseEx("se nowrap", ctx)).toEqual({ name: "set", option: "wrap", value: false });
    expect(parseEx("set nu", ctx)).toEqual({ name: "set", option: "number", value: true });
    expect(parseEx("set nornu", ctx)).toEqual({ name: "set", option: "relativenumber", value: false });
    expect(parseEx("set list!", ctx)).toEqual({ name: "set", option: "list", value: "toggle" });
    expect(parseEx("set invwrap", ctx)).toEqual({ name: "set", option: "wrap", value: "toggle" });
  });

  it("explains what it cannot do in Vim's words", () => {
    expect(parseEx("frobnicate", ctx)).toEqual({ error: "E492: Not an editor command: frobnicate" });
    expect(parseEx("2,3w", ctx)).toEqual({ error: "E481: No range allowed" });
    expect(parseEx("'z,.s/a/b/", ctx)).toEqual({ error: "E20: Mark not set" });
    expect(parseEx("2,99s/a/b/", ctx)).toEqual({ error: "E16: Invalid range" });
    expect(parseEx("set bogus", ctx)).toEqual({ error: "E518: Unknown option: bogus" });
    expect(parseEx("set", ctx)).toEqual({ error: "E471: Argument required" });
    expect(parseEx("w other.txt", ctx)).toEqual({ error: "E488: Trailing characters: other.txt" });
    expect(parseEx("e foo", ctx)).toHaveProperty("error");
  });

  it("reads :g, :g!, :v with any delimiter, over the whole file unless given a range (T12)", () => {
    expect(parseEx("g/a/d", ctx)).toEqual({
      name: "global",
      range: { first: 0, last: 9 },
      pattern: "a",
      invert: false,
      command: "d",
    });
    expect(parseEx("g!#a#d", ctx)).toMatchObject({ invert: true, pattern: "a" });
    expect(parseEx("v/a/d", ctx)).toMatchObject({ invert: true });
    expect(parseEx("2,3global/x\\/y/s//z/", ctx)).toMatchObject({
      range: { first: 1, last: 2 },
      pattern: "x/y",
      command: "s//z/",
    });
    expect(parseEx("g/a/", ctx)).toHaveProperty("error");
  });

  it("reads :d with a register and :normal keeping its trailing space", () => {
    expect(parseEx("2,3d a", ctx)).toEqual({ name: "delete", range: { first: 1, last: 2 }, register: "a" });
    expect(parseEx("d", ctx)).toEqual({ name: "delete", range: { first: 4, last: 4 }, register: null });
    expect(parseEx("%norm A; ", ctx)).toEqual({ name: "normal", range: { first: 0, last: 9 }, keys: "A; " });
    expect(parseEx("normal", ctx)).toEqual({ error: "E471: Argument required" });
  });

  it("does nothing for an empty line", () => {
    expect(parseEx("", ctx)).toEqual({ error: "" });
  });
});

describe("completeEx (Tab)", () => {
  it("completes command names, keeping a range in front", () => {
    expect(completeEx("no")).toEqual(["nohlsearch", "normal"]);
    expect(completeEx("%su")).toEqual(["%substitute"]);
    expect(completeEx("w")).toEqual(["write", "wq"]);
  });

  it("completes :set's options, with a no in front kept", () => {
    expect(completeEx("set w")).toEqual(["set wrap"]);
    expect(completeEx("set nor")).toEqual(["set norelativenumber", "set nornu"]);
  });

  it("offers nothing after a whole name", () => {
    expect(completeEx("set wrap")).toEqual([]);
    expect(completeEx("s/a")).toEqual([]);
  });
});
