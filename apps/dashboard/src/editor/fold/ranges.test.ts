// @vitest-environment node
// tree-sitter's WebAssembly loads from disk here, which needs Node, not jsdom.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { beforeAll, describe, expect, it } from "vitest";

import { LineStore } from "../buffer";
import { SyntaxHighlighter } from "../syntax/highlighter";
import { GrammarRuntime, type GrammarSource } from "../syntax/runtime";
import { docFrom } from "../testing";
import { FoldRanges, LOCAL_LINES, headingRanges, indentRangeAt, indentRanges, treeRanges } from "./ranges";

const GRAMMARS = fileURLToPath(new URL("../../../public/grammars/", import.meta.url));
const RUNTIME = fileURLToPath(new URL("../../../node_modules/web-tree-sitter/web-tree-sitter.wasm", import.meta.url));

const source: GrammarSource = {
  runtimeWasm: () => RUNTIME,
  grammar: async (name) => new Uint8Array(await readFile(`${GRAMMARS}${name}.wasm`)),
  query: async (path) => readFile(`${GRAMMARS}${path}`, "utf8").catch(() => null),
};

let runtime: GrammarRuntime;

beforeAll(() => {
  runtime = new GrammarRuntime(source);
});

/** The tree's ranges of `text` in `language`, as `start-end` strings in order. */
async function treeFolds(language: string, text: string): Promise<string[]> {
  const doc = docFrom(text);
  const h = (await SyntaxHighlighter.create(doc, runtime, language))!;
  const found = treeRanges(h.syntaxTree()!, doc.store);
  h.dispose();
  return asList(found);
}

function asList(ranges: Map<number, number>): string[] {
  return [...ranges].sort((a, b) => a[0] - b[0]).map(([s, e]) => `${s}-${e}`);
}

describe("indentRanges", () => {
  it("folds every line followed by deeper ones, up to the last of them", () => {
    const store = new LineStore(["a:", "  b", "  c:", "    d", "e"].join("\n"));
    expect(asList(indentRanges(store, 4))).toEqual(["0-3", "2-3"]);
  });

  it("keeps blank lines inside a block but not after it", () => {
    const store = new LineStore(["a", "  b", "", "  c", "", "d"].join("\n"));
    expect(asList(indentRanges(store, 4))).toEqual(["0-3"]);
  });

  it("closes blocks still open at the end of the text", () => {
    const store = new LineStore(["a", "  b", "    c"].join("\n"));
    expect(asList(indentRanges(store, 4))).toEqual(["0-2", "1-2"]);
  });

  it("measures a tab at the tab size", () => {
    const store = new LineStore(["a", "\tb", "    c", "d"].join("\n"));
    // A tab and four spaces are the same depth at tab size 4, two different ones at 2.
    expect(asList(indentRanges(store, 4))).toEqual(["0-2"]);
    expect(asList(indentRanges(store, 2))).toEqual(["0-2", "1-2"]);
  });
});

describe("indentRangeAt", () => {
  const store = new LineStore(["a", "  b", "", "  c", "d", "e"].join("\n"));

  it("finds the same range as the whole pass, from its line alone", () => {
    expect(indentRangeAt(store, 0, 4, 100)).toEqual({ start: 0, end: 3 });
    expect(indentRangeAt(store, 4, 4, 100)).toBeNull();
    expect(indentRangeAt(store, 2, 4, 100)).toBeNull();
  });

  it("gives up on a block longer than it may look", () => {
    expect(indentRangeAt(store, 0, 4, 2)).toBeNull();
  });
});

describe("headingRanges", () => {
  it("folds a heading up to the next one at its level or above", () => {
    const store = new LineStore(["# A", "text", "## B", "more", "# C", "end"].join("\n"));
    expect(asList(headingRanges(store))).toEqual(["0-3", "2-3", "4-5"]);
  });

  it("ignores lines in code fences and leaves out trailing blank lines", () => {
    const store = new LineStore(["# A", "```", "# not a heading", "```", "", "# B"].join("\n"));
    expect(asList(headingRanges(store))).toEqual(["0-3"]);
  });
});

describe("treeRanges", () => {
  const RUST = [
    "impl Point {", // 0
    "    fn add(&self) -> i32 {", // 1
    "        if x {", // 2
    "            1", // 3
    "        } else {", // 4
    "            2", // 5
    "        }", // 6
    "    }", // 7
    "}", // 8
  ].join("\n");

  it("folds functions, classes and bracketed blocks, leaving the closing line visible", async () => {
    expect(await treeFolds("rust", RUST)).toEqual(["0-7", "1-6", "2-3", "4-5"]);
  });

  it("does not fold a line comment that takes its line break", async () => {
    const text = ["//! Crate doc.", "", "/// Step.", "fn f() {", "    1", "}"].join("\n");
    expect(await treeFolds("rust", text)).toEqual(["3-4"]);
  });

  it("folds a multi-line comment whole", async () => {
    const text = ["/**", " * Doc.", " */", "fn f() {}"].join("\n");
    expect(await treeFolds("rust", text)).toEqual(["0-2"]);
  });

  it("folds an object literal inside a call from the line it opens on", async () => {
    const text = ["call(a, {", "  b: 1,", "});"].join("\n");
    expect(await treeFolds("typescript", text)).toEqual(["0-1"]);
  });
});

describe("treeRanges in other languages", () => {
  it("TypeScript: a class, its method and an if/else, each closing line left visible", async () => {
    const text = [
      "class A {", // 0
      "  m(): void {", // 1
      "    if (x) {", // 2
      "      y();", // 3
      "    } else {", // 4
      "      z();", // 5
      "    }", // 6
      "  }", // 7
      "}", // 8
    ].join("\n");
    expect(await treeFolds("typescript", text)).toEqual(["0-7", "1-6", "2-3", "4-5"]);
  });

  it("TypeScript: a multi-line array and a JSDoc comment", async () => {
    const text = ["/**", " * Doc.", " */", "const a = [", "  1,", "  2,", "];"].join("\n");
    expect(await treeFolds("typescript", text)).toEqual(["0-2", "3-5"]);
  });

  it("Python: a class and its method fold to their last line, with no closing line to keep", async () => {
    const text = ["class A:", "    def m(self):", "        return 1", "", "x = 2"].join("\n");
    expect(await treeFolds("python", text)).toEqual(["0-2", "1-2"]);
  });

  it("Lua: a function leaves its `end` visible", async () => {
    const text = ["function f()", "  return 1", "end"].join("\n");
    expect(await treeFolds("lua", text)).toEqual(["0-1"]);
  });

  it("does not fold a node on one line", async () => {
    expect(await treeFolds("typescript", "function f() { return 1; }")).toEqual([]);
  });
});

describe("FoldRanges", () => {
  it("prefers the tree on a line, and keeps indentation where the tree has none", async () => {
    const doc = docFrom(["def f():", "    if x:", "        y", "    z"].join("\n"));
    const h = (await SyntaxHighlighter.create(doc, runtime, "python"))!;
    const ranges = new FoldRanges(doc.store, () => doc.revision, { tree: () => h.syntaxTree(), tabSize: 4 });
    expect(ranges.all()).toEqual([
      { start: 0, end: 3 },
      { start: 1, end: 2 },
    ]);
    h.dispose();
  });

  it("lists the ranges around a line innermost first, and caches until the text changes", () => {
    const doc = docFrom(["a", "  b", "    c", "d"].join("\n"));
    const ranges = new FoldRanges(doc.store, () => doc.revision, { tabSize: 4 });
    expect(ranges.around(2)).toEqual([
      { start: 1, end: 2 },
      { start: 0, end: 2 },
    ]);
    const first = ranges.starts();
    expect(ranges.starts()).toBe(first);
    doc.edit([{ range: { start: { line: 3, col: 0 }, end: { line: 3, col: 0 } }, text: "  " }], doc.selection);
    expect(ranges.starts()).not.toBe(first);
    expect(ranges.at(0)).toEqual({ start: 0, end: 3 });
  });

  it("asks line by line only for a very long text", () => {
    const doc = docFrom(Array.from({ length: LOCAL_LINES }, (_, i) => (i % 2 ? "  x" : "y")).join("\n"));
    const ranges = new FoldRanges(doc.store, () => doc.revision, { tabSize: 4 });
    expect(ranges.local).toBe(true);
    expect(ranges.near(0)).toEqual({ start: 0, end: 1 });
    expect(new FoldRanges(docFrom("a").store, () => 0, { tabSize: 4 }).local).toBe(false);
  });

  it("folds Markdown by its headings", () => {
    const doc = docFrom(["# A", "- item", "  - sub", "# B"].join("\n"));
    const ranges = new FoldRanges(doc.store, () => doc.revision, { markdown: true, tabSize: 4 });
    expect(ranges.at(0)).toEqual({ start: 0, end: 2 });
    expect(ranges.at(1)).toEqual({ start: 1, end: 2 });
  });
});
