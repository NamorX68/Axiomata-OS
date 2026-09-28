import { describe, expect, it } from "vitest";

import { run, UNWRAPPED } from "./commands";
import { EditorDocument } from "./document";
import { cursor, pos, range } from "./position";
import { applyTextEdits, mapPosition, storeBody, textChanges } from "./textEdits";

const ctx = { tabSize: 4, layout: UNWRAPPED, pageRows: 10, commentPrefix: null, now: 0 };

/** Applies `changes` (in `text`'s coordinates) the way the `replaceText` command does. */
function applied(text: string, changes: ReturnType<typeof textChanges>): string {
  const d = new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });
  run(d, { type: "replaceText", changes }, ctx);
  return d.store.text();
}

describe("applyTextEdits", () => {
  it("applies edits given in the original text's coordinates, in any order", () => {
    const text = "let a=1;\nlet b=2;";
    const edits = [
      { range: range(pos(1, 5), pos(1, 6)), text: " = " },
      { range: range(pos(0, 5), pos(0, 6)), text: " = " },
      { range: range(pos(2, 0), pos(2, 0)), text: "\n" },
    ];
    expect(applyTextEdits(text, edits)).toBe("let a = 1;\nlet b = 2;\n");
  });

  it("keeps two insertions at one place in their order, and turns CRLF into LF", () => {
    const edits = [
      { range: range(pos(0, 1), pos(0, 1)), text: "x" },
      { range: range(pos(0, 1), pos(0, 1)), text: "y\r\n" },
    ];
    expect(applyTextEdits("ab", edits)).toBe("axy\nb");
  });
});

describe("textChanges", () => {
  const cases: [string, string][] = [
    ["a\nb\nc", "a\nB\nc"],
    ["a\nb\nc", "a\nc"],
    ["a\nb\nc", "a\nb"],
    ["a\nb", "x\na\nb"],
    ["a\nb", "a\nb\nc\nd"],
    ["a", ""],
    ["", "new"],
    ["fn x(){\n1\n}", "fn x() {\n    1\n}"],
  ];
  for (const [before, after] of cases) {
    it(`${JSON.stringify(before)} → ${JSON.stringify(after)}`, () => {
      expect(applied(before, textChanges(before, after))).toBe(after);
    });
  }

  it("touches only the lines that differ", () => {
    expect(textChanges("a\nb\nc", "a\nB\nc")).toEqual([{ range: range(pos(1, 0), pos(1, 1)), text: "B" }]);
    expect(textChanges("same", "same")).toEqual([]);
  });
});

describe("mapPosition and the replaceText command", () => {
  it("moves a position by the lines added or removed above it and keeps it inside a changed line", () => {
    const changes = textChanges("x\na\nb\ncursor here", "import y\n\nx\na b\ncursor here");
    expect(mapPosition(pos(3, 7), changes)).toEqual(pos(4, 7));
  });

  it("keeps the cursor on its line through a format, clamped to the new line", () => {
    const d = new EditorDocument("fn x(){\nlet  long_name=1;\n}", { indentFallback: { kind: "spaces", size: 4 } });
    d.setSelection(cursor(pos(1, 16)));
    run(d, { type: "replaceText", changes: textChanges(d.store.text(), "fn x() {\n    let long_name = 1;\n}") }, ctx);
    expect(d.selection.head.line).toBe(1);
    d.undo();
    expect(d.store.text()).toBe("fn x(){\nlet  long_name=1;\n}");
  });

  it("drops the formatter's final line break (the store keeps it as the file's shape)", () => {
    expect(storeBody("a\r\nb\n")).toBe("a\nb");
  });
});
