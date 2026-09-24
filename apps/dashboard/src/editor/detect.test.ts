import { describe, expect, it } from "vitest";

import { bodyForStore, detectIndent, detectShape, joinForSave } from "./detect";

describe("detectShape", () => {
  it("keeps LF or CRLF and notices a final line break", () => {
    expect(detectShape("a\nb\n")).toEqual({ eol: "lf", mixedEol: false, finalNewline: true });
    expect(detectShape("a\r\nb")).toEqual({ eol: "crlf", mixedEol: false, finalNewline: false });
    expect(detectShape("")).toEqual({ eol: "lf", mixedEol: false, finalNewline: false });
  });

  it("calls a mix of both mixed and saves it as LF", () => {
    expect(detectShape("a\r\nb\nc")).toEqual({ eol: "lf", mixedEol: true, finalNewline: false });
  });
});

describe("round trip", () => {
  it("saves exactly what was opened", () => {
    for (const text of ["a\nb\n", "a\r\nb\r\n", "no newline", "", "\n", "a\n\n", "x\r\n\r\n"]) {
      const shape = detectShape(text);
      expect(joinForSave(bodyForStore(text, shape), shape)).toBe(text);
    }
  });
});

describe("detectIndent", () => {
  it("recognises tabs", () => {
    expect(detectIndent("fn a() {\n\tb();\n\t\tc();\n}")).toEqual({ kind: "tabs", size: 4 });
  });

  it("recognises the step between indented lines", () => {
    expect(detectIndent("a:\n  b:\n    c: 1\n  d: 2")).toEqual({ kind: "spaces", size: 2 });
    expect(detectIndent("fn a() {\n    b();\n    if x {\n        c();\n    }\n}")).toEqual({ kind: "spaces", size: 4 });
  });

  it("gives up when nothing is indented", () => {
    expect(detectIndent("# Title\n\nJust prose.\n")).toBeNull();
    expect(detectIndent("")).toBeNull();
  });
});
