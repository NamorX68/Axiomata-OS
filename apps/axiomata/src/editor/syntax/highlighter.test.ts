// @vitest-environment node
// tree-sitter's WebAssembly loads from disk here, which needs Node, not jsdom.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { beforeAll, describe, expect, it } from "vitest";

import { run } from "../commands";
import { ctx, docFrom } from "../testing";
import { SyntaxHighlighter } from "./highlighter";
import { GrammarRuntime, type GrammarSource } from "./runtime";

const GRAMMARS = fileURLToPath(new URL("../../../public/grammars/", import.meta.url));
const RUNTIME = fileURLToPath(new URL("../../../node_modules/web-tree-sitter/web-tree-sitter.wasm", import.meta.url));

/** The checked-in grammars, read from disk the way the app fetches them. */
const source: GrammarSource = {
  runtimeWasm: () => RUNTIME,
  grammar: async (name) => new Uint8Array(await readFile(`${GRAMMARS}${name}.wasm`)),
  query: async (path) => readFile(`${GRAMMARS}${path}`, "utf8").catch(() => null),
};

let runtime: GrammarRuntime;

beforeAll(() => {
  runtime = new GrammarRuntime(source);
});

/** The tokens covering the text `needle` on `line`, as `text:token` pairs. */
function tokensOn(h: SyntaxHighlighter, doc: ReturnType<typeof docFrom>, line: number): string[] {
  const text = doc.store.line(line);
  return (h.spans(line, line).get(line) ?? []).map((s) => `${text.slice(s.from, s.to)}:${s.token}`);
}

describe("SyntaxHighlighter", () => {
  it("colours Rust by its own grammar and queries", async () => {
    const doc = docFrom('|fn main() {\n    let s = "hi"; // hello\n}');
    const h = (await SyntaxHighlighter.create(doc, runtime, "rust"))!;
    expect(tokensOn(h, doc, 0)).toContain("fn:keyword");
    expect(tokensOn(h, doc, 0)).toContain("main:function");
    const second = tokensOn(h, doc, 1);
    expect(second).toContain('"hi":string');
    expect(second).toContain("// hello:comment");
    h.dispose();
  });

  it("colours C and C++ (C++ adds its own queries to C's)", async () => {
    const c = docFrom('|#include <stdio.h>\nint add(int a) { return a; } // sum\n');
    const hc = (await SyntaxHighlighter.create(c, runtime, "c"))!;
    expect(tokensOn(hc, c, 1)).toContain("int:type");
    expect(tokensOn(hc, c, 1)).toContain("return:keyword");
    expect(tokensOn(hc, c, 1)).toContain("// sum:comment");
    hc.dispose();
    const cpp = docFrom('|class Counter {\n  long add(long n) { return n; }\n};\n');
    const hp = (await SyntaxHighlighter.create(cpp, runtime, "cpp"))!;
    expect(tokensOn(hp, cpp, 0)).toContain("class:keyword");
    expect(tokensOn(hp, cpp, 1)).toContain("long:type");
    hp.dispose();
  });

  it("follows edits incrementally: the tree always matches the text", async () => {
    const doc = docFrom('fn a() {}\n|');
    const h = (await SyntaxHighlighter.create(doc, runtime, "rust"))!;
    h.spans(0, 1);
    const matchesFreshParse = async () => {
      const fresh = (await SyntaxHighlighter.create(docFrom(`|${doc.store.text()}`), runtime, "rust"))!;
      const tree = h.syntaxTree()!.rootNode;
      expect(tree.toString()).toBe(fresh.syntaxTree()!.rootNode.toString());
      expect(tree.endIndex).toBe(doc.store.text().length);
      fresh.dispose();
    };
    run(doc, { type: "move", motion: "docEnd", extend: false }, ctx());
    run(doc, { type: "newline" }, ctx());
    for (const piece of ["let x", " = ", '"é😀"', ";"]) run(doc, { type: "insert", text: piece }, ctx());
    run(doc, { type: "newline" }, ctx());
    run(doc, { type: "insert", text: "// done" }, ctx());
    await matchesFreshParse();
    expect(tokensOn(h, doc, 1)).toContain('"é😀":string');
    expect(tokensOn(h, doc, 2)).toContain("// done:comment");
    doc.undo();
    doc.undo();
    await matchesFreshParse();
    doc.reset("struct S;\n");
    await matchesFreshParse();
    expect(tokensOn(h, doc, 0)).toContain("struct:keyword");
    h.dispose();
  });

  it("builds TypeScript on JavaScript's queries", async () => {
    const doc = docFrom("|const n: number = 1;\ninterface A { b: string }");
    const h = (await SyntaxHighlighter.create(doc, runtime, "typescript"))!;
    expect(tokensOn(h, doc, 0)).toContain("const:keyword");
    expect(tokensOn(h, doc, 1)).toContain("interface:keyword");
    h.dispose();
  });

  it("colours a Markdown code fence in its own language once it has loaded", async () => {
    const doc = docFrom("|# Title\n\n```rust\nfn main() {}\n```\n");
    let loaded = 0;
    const h = (await SyntaxHighlighter.create(doc, runtime, "markdown", () => loaded++))!;
    h.spans(0, 5);
    await runtime.load("rust");
    await runtime.load("markdown_inline");
    expect(tokensOn(h, doc, 0).some((t) => t.endsWith(":heading"))).toBe(true);
    expect(tokensOn(h, doc, 3)).toContain("fn:keyword");
    h.dispose();
  });

  it("every bundled language loads and compiles its queries", async () => {
    for (const id of ["rust", "javascript", "typescript", "tsx", "json", "css", "html", "python", "bash",
      "markdown", "markdown_inline", "toml", "yaml", "lua", "svelte", "swift", "sql"]) {
      const lang = await runtime.load(id);
      expect(lang?.highlights, id).toBeTruthy();
    }
  });

  it("colours bracket pairs by depth, ignoring brackets inside strings", async () => {
    const doc = docFrom('|fn f() { g((1), "(") }');
    const h = (await SyntaxHighlighter.create(doc, runtime, "rust"))!;
    const spans = h.spans(0, 0, { brackets: true }).get(0) ?? [];
    const text = doc.store.line(0);
    const brackets = spans
      .filter((s) => s.token.startsWith("bracket"))
      .map((s) => `${text.slice(s.from, s.to)}${s.token.slice(-1)}`);
    // f's () and the body's { }: depth 0 (siblings, not nested; adjacent brackets
    // of one colour form one span); g's ( ): 1; the inner ( ): 2.
    expect(brackets).toEqual(["()1", "{1", "(2", "(3", ")3", ")2", "}1"]);
    expect(spans.some((s) => text.slice(s.from, s.to) === '"("' && s.token === "string")).toBe(true);
    h.dispose();
  });

  it("answers a request past the end with no colours instead of throwing", async () => {
    const doc = docFrom("|fn a() {}");
    const h = (await SyntaxHighlighter.create(doc, runtime, "rust"))!;
    expect(h.spans(40, 60).size).toBe(0);
    h.dispose();
  });

  it("has no grammar for plain text", async () => {
    expect(await SyntaxHighlighter.create(docFrom("|x"), runtime, "haskell")).toBeNull();
  });
});
