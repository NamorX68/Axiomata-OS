import { describe, expect, it } from "vitest";

import { LineStore } from "./buffer";
import { run } from "./commands";
import { gutterDigits, lineLabel } from "./gutter";
import { ctx, docFrom, show } from "./testing";
import { VisualLayout } from "./visual";
import { wrapLine } from "./wrap";

describe("wrapLine (F6)", () => {
  it("leaves a line that fits alone", () => {
    expect(wrapLine("short", 10, 4)).toEqual({ starts: [0], indent: 0 });
  });

  it("breaks after the last space that fits, letting spaces hang", () => {
    expect(wrapLine("aaaa bbbb cccc", 8, 4).starts).toEqual([0, 5, 10]);
    // "aaaa bbbb" is exactly nine cells: it fits, the space after it hangs.
    expect(wrapLine("aaaa bbbb cccc", 9, 4).starts).toEqual([0, 10]);
  });

  it("breaks a word longer than a row hard", () => {
    expect(wrapLine("abcdefghij", 4, 4).starts).toEqual([0, 4, 8]);
  });

  it("keeps the indentation on continuation rows, unless it takes half the row", () => {
    expect(wrapLine("  aaa bbb ccc", 8, 4)).toEqual({ starts: [0, 6, 10], indent: 2 });
    expect(wrapLine("        aaa bbb", 10, 4).indent).toBe(0);
  });

  it("counts a wide character as two cells", () => {
    expect(wrapLine("日本語日本語", 4, 4).starts).toEqual([0, 2, 4]);
  });
});

describe("VisualLayout", () => {
  it("maps rows to lines and back, with and without wrapping", () => {
    const store = new LineStore("aaaa bbbb cccc\nx\nyyyy zzzz");
    const layout = new VisualLayout(store, { wrap: true, width: 8, tabSize: 4 });
    expect(layout.totalRows).toBe(3 + 1 + 2);
    expect(layout.firstRow(2)).toBe(4);
    expect(layout.lineAt(0)).toEqual({ line: 0, sub: 0 });
    expect(layout.lineAt(2)).toEqual({ line: 0, sub: 2 });
    expect(layout.lineAt(3)).toEqual({ line: 1, sub: 0 });
    expect(layout.lineAt(5)).toEqual({ line: 2, sub: 1 });
    expect(layout.lineAt(99)).toEqual({ line: 2, sub: 1 });

    layout.setOptions({ wrap: false, width: 8, tabSize: 4 });
    expect(layout.totalRows).toBe(3);
    expect(layout.rowStarts(0)).toEqual([0]);
  });

  it("follows an edit after refresh", () => {
    const doc = docFrom("aaaa bbbb cccc|");
    const layout = new VisualLayout(doc.store, { wrap: true, width: 8, tabSize: 4 });
    run(doc, { type: "insert", text: " dddd" }, ctx());
    layout.refresh();
    expect(layout.totalRows).toBe(4);
  });

  it("drives ↓ through wrapped rows as the commands' RowLayout", () => {
    const doc = docFrom("aa|aa bbbb cccc");
    const layout = new VisualLayout(doc.store, { wrap: true, width: 8, tabSize: 4 });
    run(doc, { type: "move", motion: "down", extend: false }, ctx({ layout }));
    expect(show(doc)).toBe("aaaa bb|bb cccc");
  });
});

describe("gutter labels", () => {
  it("numbers absolutely, relatively, or hybrid", () => {
    expect(lineLabel(4, 2, "absolute")).toBe("5");
    expect(lineLabel(4, 2, "relative")).toBe("2");
    expect(lineLabel(2, 2, "relative")).toBe("0");
    expect(lineLabel(2, 2, "hybrid")).toBe("3");
    expect(lineLabel(0, 2, "hybrid")).toBe("2");
  });

  it("reserves room for the largest number", () => {
    expect(gutterDigits(9, "absolute")).toBe(2);
    expect(gutterDigits(12345, "hybrid")).toBe(5);
    expect(gutterDigits(12345, "off")).toBe(0);
    expect(lineLabel(4, 2, "off")).toBe("");
  });
});
