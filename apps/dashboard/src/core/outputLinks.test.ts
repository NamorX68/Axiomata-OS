import { describe, expect, it } from "vitest";

import { findOutputRefs, outputRefAt, rowText } from "./outputLinks";

const paths = (text: string) => findOutputRefs(text).map((r) => [r.path, r.line, r.col]);

describe("file locations in output", () => {
  it("reads the common compiler and test shapes", () => {
    expect(paths("error[E0308]: mismatched types\n --> src/main.rs:12:5")).toEqual([["src/main.rs", 12, 5]]);
    expect(paths("tests/test_db.py:44: AssertionError")).toEqual([["tests/test_db.py", 44, null]]);
    expect(paths("src/ocht/core/models.py:7:1: F401 `os` imported but unused")).toEqual([["src/ocht/core/models.py", 7, 1]]);
    expect(paths("    at run (/Users/x/app/server.js:31:9)")).toEqual([["/Users/x/app/server.js", 31, 9]]);
  });

  it("reads a Python traceback and a tsc error", () => {
    expect(paths('  File "/repo/src/app.py", line 88, in main')).toEqual([["/repo/src/app.py", 88, null]]);
    expect(paths("src/a.ts(12,5): error TS2322: nope")).toEqual([["src/a.ts", 12, 5]]);
  });

  it("does not take URLs, hosts or plain numbers for files", () => {
    expect(paths("listening on http://localhost:8000/docs")).toEqual([]);
    expect(paths("connect to example.com:443 failed")).toEqual([]);
    expect(paths("took 12:30 minutes, version 1.2:3")).toEqual([]);
  });

  it("accepts a bare file name only with a known extension", () => {
    expect(paths("main.py:3: boom")).toEqual([["main.py", 3, null]]);
    expect(paths("thing.xyz:3: boom")).toEqual([]);
  });

  it("finds the one under the pointer by column, and nothing outside", () => {
    const text = "see src/lib.rs:9:2 for more";
    expect(outputRefAt(text, 6)?.path).toBe("src/lib.rs");
    expect(outputRefAt(text, 0)).toBeNull();
    expect(outputRefAt(text, text.length - 1)).toBeNull();
  });

  it("makes one column per terminal cell", () => {
    expect(rowText([{ ch: "a" }, { ch: "" }, { ch: "b" }])).toBe("a b");
  });
});
