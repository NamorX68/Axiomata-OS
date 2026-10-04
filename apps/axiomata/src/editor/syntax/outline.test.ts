// @vitest-environment node
// tree-sitter's WebAssembly loads from disk here, which needs Node, not jsdom.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { beforeAll, describe, expect, it } from "vitest";

import { docFrom } from "../testing";
import { SyntaxHighlighter } from "./highlighter";
import { filterOutline, outlineFromMarkdown, outlineFromTree, pathAt, type OutlineSymbol } from "./outline";
import { GrammarRuntime, type GrammarSource } from "./runtime";

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

async function outline(language: string, text: string): Promise<OutlineSymbol[]> {
  const doc = docFrom(text);
  const h = (await SyntaxHighlighter.create(doc, runtime, language))!;
  const out = outlineFromTree(h.syntaxTree()!, doc.store);
  h.dispose();
  return out;
}

/** `kind name` for every symbol, children indented two spaces. */
function shape(symbols: readonly OutlineSymbol[], depth = 0): string[] {
  return symbols.flatMap((s) => [`${"  ".repeat(depth)}${s.kind} ${s.name}`, ...shape(s.children, depth + 1)]);
}

describe("outline from a syntax tree", () => {
  it("Rust: items, impl blocks with their methods, nothing inside function bodies", async () => {
    const text = [
      "const LIMIT: usize = 3;",
      "struct Point { x: i32 }",
      "enum Kind { A, B }",
      "trait Shape { fn area(&self) -> f64; }",
      "impl Point {",
      "    fn new() -> Self { Point { x: 0 } }",
      "    fn norm(&self) -> i32 { fn helper() {} 1 }",
      "}",
      "impl Shape for Point {",
      "    fn area(&self) -> f64 { 0.0 }",
      "}",
      "mod util { pub fn run() {} }",
      "fn main() { let f = || 1; }",
    ].join("\n");
    expect(shape(await outline("rust", text))).toEqual([
      "constant LIMIT",
      "struct Point",
      "enum Kind",
      "trait Shape",
      "  method area",
      "impl Point",
      "  method new",
      "  method norm",
      "impl Shape for Point",
      "  method area",
      "module util",
      "  function run",
      "function main",
    ]);
  });

  it("TypeScript: classes, interfaces, arrow functions bound to a name, exports", async () => {
    const text = [
      "export interface Options { a: number }",
      "export type Id = string;",
      "export class Store {",
      "  load() {}",
      "  save = () => 1;",
      "}",
      "export function make() { function inner() {} }",
      "const run = () => 1;",
      "const plain = 3;",
      "enum Color { Red }",
    ].join("\n");
    expect(shape(await outline("typescript", text))).toEqual([
      "interface Options",
      "type Id",
      "class Store",
      "  method load",
      "function make",
      "function run",
      "enum Color",
    ]);
  });

  it("Python and Lua and Bash: functions and classes", async () => {
    expect(shape(await outline("python", "class A:\n    def m(self):\n        pass\n\ndef f():\n    pass\n"))).toEqual([
      "class A",
      "  method m",
      "function f",
    ]);
    expect(shape(await outline("bash", "greet() {\n  echo hi\n}\n"))).toEqual(["function greet"]);
    expect(shape(await outline("lua", "function helper()\nend\n"))).toEqual(["function helper"]);
  });

  it("covers each declaration's lines and points at its name", async () => {
    const out = await outline("rust", "fn alpha() {\n    1;\n}\n\nfn beta() {}\n");
    expect(out.map((s) => [s.name, s.line, s.endLine, s.nameLine, s.nameCol])).toEqual([
      ["alpha", 0, 2, 0, 3],
      ["beta", 4, 4, 4, 3],
    ]);
  });
});

describe("outline from Markdown", () => {
  it("nests headings by level, skips code fences, takes the closing hashes off", () => {
    const doc = docFrom(["# One", "text", "## Two ##", "```", "# not a heading", "```", "## Three", "# Four"].join("\n"));
    const out = outlineFromMarkdown(doc.store);
    expect(shape(out)).toEqual(["heading One", "  heading Two", "  heading Three", "heading Four"]);
    expect(out[0].endLine).toBe(6);
    expect(out[1].line).toBe(7);
  });
});

describe("pathAt and filterOutline", () => {
  const doc = docFrom(["# A", "## B", "x", "## C", "y", "# D"].join("\n"));
  const out = outlineFromMarkdown(doc.store);

  it("finds the symbols holding a line, outermost first", () => {
    expect(pathAt(out, 2).map((s) => s.name)).toEqual(["A", "B"]);
    expect(pathAt(out, 4).map((s) => s.name)).toEqual(["A", "C"]);
    expect(pathAt(out, 5).map((s) => s.name)).toEqual(["D"]);
  });

  it("filters by name and keeps the ancestors of a match", () => {
    expect(shape(filterOutline(out, "c"))).toEqual(["heading A", "  heading C"]);
    expect(shape(filterOutline(out, "  "))).toHaveLength(4);
    expect(filterOutline(out, "zzz")).toEqual([]);
  });
});
