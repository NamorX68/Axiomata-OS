// @vitest-environment node
// tree-sitter's WebAssembly loads from disk here, which needs Node, not jsdom.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { beforeAll, describe, expect, it } from "vitest";

import { range } from "../position";
import { docFrom } from "../testing";
import { SyntaxHighlighter } from "./highlighter";
import { syntaxObject, type SyntaxObjectName } from "./objects";
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

/**
 * The text object `name` at the `|` in `marked`, as the text it covers
 * (prefixed `L:` when it is linewise), or `null`.
 */
async function object(
  language: string,
  marked: string,
  name: SyntaxObjectName,
  inner: boolean,
  count = 1,
): Promise<string | null> {
  const doc = docFrom(marked);
  const h = (await SyntaxHighlighter.create(doc, runtime, language))!;
  const root = h.syntaxTree()!.rootNode;
  const found = syntaxObject(root, doc.store, doc.selection.head, name, inner, count);
  h.dispose();
  if (!found) return null;
  const text = doc.store.slice(range(found.start, found.end));
  return found.linewise ? `L:${text}` : text;
}

const RUST = [
  "impl Point {", // 0
  "    fn add(&self, a: i32, b: i32) -> i32 {", // 1
  "        let s = a + b;", // 2
  "        s", // 3
  "    }", // 4
  "}", // 5
].join("\n");

/** `RUST` with the cursor at `line`/`col`. */
function rustAt(line: number, col: number): string {
  const lines = RUST.split("\n");
  lines[line] = `${lines[line].slice(0, col)}|${lines[line].slice(col)}`;
  return lines.join("\n");
}

describe("syntaxObject in Rust", () => {
  it("af is the whole function, as lines; if its body between the braces", async () => {
    expect(await object("rust", rustAt(2, 10), "f", false)).toBe(`L:${RUST.split("\n").slice(1, 5).join("\n")}`);
    expect(await object("rust", rustAt(2, 10), "f", true)).toBe("L:        let s = a + b;\n        s");
  });

  it("ac is the impl, ic its inside", async () => {
    expect(await object("rust", rustAt(2, 10), "c", false)).toBe(`L:${RUST}`);
    expect(await object("rust", rustAt(2, 10), "c", true)).toBe(`L:${RUST.split("\n").slice(1, 5).join("\n")}`);
  });

  it("ia is a parameter, aa takes the comma after it — or before it for the last one", async () => {
    expect(await object("rust", rustAt(1, 22), "a", true)).toBe("a: i32");
    expect(await object("rust", rustAt(1, 22), "a", false)).toBe("a: i32, ");
    expect(await object("rust", rustAt(1, 30), "a", false)).toBe(", b: i32");
  });

  it("finds nothing outside a function", async () => {
    expect(await object("rust", "|use std::io;", "f", false)).toBeNull();
    expect(await object("rust", "|use std::io;", "a", true)).toBeNull();
  });
});

describe("syntaxObject in TypeScript and Python", () => {
  it("counts outwards: 2af is the function around the arrow function", async () => {
    // (Rust's closures would need `|` in the text, which `docFrom` reads as the cursor.)
    const text = "function outer() {\n  const f = (x) => x + |1;\n}";
    expect(await object("typescript", text, "f", false)).toBe("(x) => x + 1");
    expect(await object("typescript", text, "f", false, 2)).toBe("L:function outer() {\n  const f = (x) => x + 1;\n}");
  });

  it("takes an arrow function's expression body as it is", async () => {
    expect(await object("typescript", "const f = (a: number) => a |+ 1;", "f", true)).toBe("a + 1");
  });

  it("keeps a one-line body inside its braces as characters", async () => {
    expect(await object("typescript", "function f() { return |1; }", "f", true)).toBe(" return 1; ");
    expect(await object("typescript", "function f() {|}", "f", true)).toBe("");
  });

  it("finds a call's argument, nested ones first", async () => {
    expect(await object("typescript", "f(a, g(|b, c))", "a", true)).toBe("b");
    expect(await object("typescript", "f(a, g(|b, c))", "a", true, 2)).toBe("g(b, c)");
  });

  it("knows a class and its body", async () => {
    const text = "class A {\n  x = |1;\n}";
    expect(await object("typescript", text, "c", false)).toBe("L:class A {\n  x = 1;\n}");
  });

  it("takes a Python function's block as whole lines", async () => {
    const text = "def f(a):\n    b = |a\n    return b\n";
    expect(await object("python", text, "f", true)).toBe("L:    b = a\n    return b");
    expect(await object("python", text, "f", false)).toBe("L:def f(a):\n    b = a\n    return b");
  });
});

describe("syntaxObject in further Rust cases (gap check ED3.4)", () => {
  const STRUCT = "struct S {\n    |x: i32,\n    y: i32,\n}\n";

  it("ic/ac work on a struct, not only an impl", async () => {
    expect(await object("rust", STRUCT, "c", true)).toBe("L:    x: i32,\n    y: i32,");
    expect(await object("rust", STRUCT, "c", false)).toBe("L:struct S {\n    x: i32,\n    y: i32,\n}");
  });

  it("ic works on a trait", async () => {
    const text = "trait T {\n    fn f(&self) {\n        |let x = 1;\n    }\n}\n";
    expect(await object("rust", text, "c", true)).toBe("L:    fn f(&self) {\n        let x = 1;\n    }");
  });

  it("aa on a sole argument (no comma either side) is the same as ia", async () => {
    const text = "fn f(|a: i32) -> i32 { a }\n";
    expect(await object("rust", text, "a", true)).toBe("a: i32");
    expect(await object("rust", text, "a", false)).toBe("a: i32");
  });

  it("finds nothing for ia/aa with the cursor on the comma between arguments", async () => {
    // The comma itself is unnamed and not an argument, and none of its ancestors are either.
    const text = "fn f(a: i32|, b: i32) {}\n";
    expect(await object("rust", text, "a", true)).toBeNull();
    expect(await object("rust", text, "a", false)).toBeNull();
  });

  it("if on a function with an empty body is an empty stretch, not null", async () => {
    expect(await object("rust", "fn f() {|}\n", "f", true)).toBe("");
  });

  it("a count beyond the outermost match is null", async () => {
    const text = "fn outer() {\n    fn inner() {\n        |1;\n    }\n}\n";
    expect(await object("rust", text, "f", false, 1)).not.toBeNull();
    expect(await object("rust", text, "f", false, 2)).not.toBeNull();
    expect(await object("rust", text, "f", false, 3)).toBeNull();
  });
});

describe("syntaxObject in JavaScript", () => {
  it("af/if find a function declaration and its body between the braces", async () => {
    const text = "function outer() {\n  |return 1;\n}\n";
    expect(await object("javascript", text, "f", false)).toBe("L:function outer() {\n  return 1;\n}");
    expect(await object("javascript", text, "f", true)).toBe("L:  return 1;");
  });

  it("ac/ic find a class and a method's body inside it", async () => {
    const text = "class A {\n  method(a, b) {\n    return |a;\n  }\n}\n";
    expect(await object("javascript", text, "c", false)).toBe("L:class A {\n  method(a, b) {\n    return a;\n  }\n}");
    expect(await object("javascript", text, "c", true)).toBe("L:  method(a, b) {\n    return a;\n  }");
  });

  it("if takes an arrow function's expression body as it is", async () => {
    expect(await object("javascript", "const f = (a, b) => |a + b;\n", "f", true)).toBe("a + b");
  });

  it("aa takes a call argument with its trailing comma", async () => {
    expect(await object("javascript", "f(|a, b);\n", "a", false)).toBe("a, ");
  });
});

describe("syntaxObject in TypeScript's type parameters", () => {
  it("ia/aa treat a generic's type parameters like arguments", async () => {
    const text = "function outer<|T, U>(a: T, b: U): void {}\n";
    expect(await object("typescript", text, "a", true)).toBe("T");
    expect(await object("typescript", text, "a", false)).toBe("T, ");
  });
});

describe("syntaxObject in Lua", () => {
  it("af/if find a function and its block as whole lines", async () => {
    const text = "function outer(a, b)\n  local x = |a + b\n  return x\nend\n";
    expect(await object("lua", text, "f", false)).toBe("L:function outer(a, b)\n  local x = a + b\n  return x\nend");
    expect(await object("lua", text, "f", true)).toBe("L:  local x = a + b\n  return x");
  });

  it("aa takes a parameter with its comma, like Rust's", async () => {
    expect(await object("lua", "function f(a, |b) end\n", "a", false)).toBe(", b");
  });
});

describe("syntaxObject in Bash", () => {
  it("af/if find a `function name() { … }` and its body between the braces", async () => {
    const text = "function foo() {\n  |echo hi\n}\n";
    expect(await object("bash", text, "f", false)).toBe("L:function foo() {\n  echo hi\n}");
    expect(await object("bash", text, "f", true)).toBe("L:  echo hi");
  });
});

describe("syntaxObject in Swift", () => {
  it("af/if and ac/ic find a method and its class", async () => {
    const text = "class A {\n  func f(a: Int, b: Int) -> Int {\n    return |a + b\n  }\n}\n";
    expect(await object("swift", text, "f", false)).toBe("L:  func f(a: Int, b: Int) -> Int {\n    return a + b\n  }");
    expect(await object("swift", text, "f", true)).toBe("L:    return a + b");
    expect(await object("swift", text, "c", false)).toBe(
      "L:class A {\n  func f(a: Int, b: Int) -> Int {\n    return a + b\n  }\n}",
    );
    expect(await object("swift", text, "c", true)).toBe("L:  func f(a: Int, b: Int) -> Int {\n    return a + b\n  }");
  });

  it("takes a Swift closure's statements as its inner (it has no `body` field)", async () => {
    expect(await object("swift", "let f = { (x: Int) in x |+ 1 }\n", "f", true)).toBe("x + 1");
  });

  it("ia/aa on a Swift function parameter, which hangs straight off the function", async () => {
    // tree-sitter-swift wraps parameters in no list node, unlike every other bundled grammar.
    expect(await object("swift", "func f(a: Int, |b: Int) {}\n", "a", true)).toBe("b: Int");
    expect(await object("swift", "func f(a: Int, |b: Int) {}\n", "a", false)).toBe(", b: Int");
    expect(await object("swift", "func f(|a: Int, b: Int) {}\n", "a", false)).toBe("a: Int, ");
  });
});
