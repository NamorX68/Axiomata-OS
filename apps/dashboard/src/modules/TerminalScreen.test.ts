import { describe, expect, it } from "vitest";

import { isCellSelected, resolveColor, selectionText, type TermCell, type TermColor } from "./TerminalScreen";

/** A minimal `TermCell` for selection tests — only `ch` matters there. */
function cell(ch: string): TermCell {
  return { ch, fg: { type: "default" }, bg: { type: "default" }, bold: false, underline: false };
}

function row(text: string): TermCell[] {
  return [...text].map(cell);
}

describe("resolveColor", () => {
  it("defers a default color to the caller's fallback", () => {
    expect(resolveColor({ type: "default" }, "#abcdef")).toBe("#abcdef");
  });

  it("resolves the low 8 ANSI indices to the standard xterm palette", () => {
    expect(resolveColor({ type: "indexed", index: 0 }, "#fff")).toBe("#000000");
    expect(resolveColor({ type: "indexed", index: 1 }, "#fff")).toBe("#cd0000");
    expect(resolveColor({ type: "indexed", index: 7 }, "#fff")).toBe("#e5e5e5");
  });

  it("resolves the bright 8 ANSI indices (8-15)", () => {
    expect(resolveColor({ type: "indexed", index: 8 }, "#fff")).toBe("#7f7f7f");
    expect(resolveColor({ type: "indexed", index: 15 }, "#fff")).toBe("#ffffff");
  });

  it("resolves the 256-color cube (16-231) via the standard xterm level table", () => {
    // Index 16 is the cube's own (0,0,0) corner - pure black, distinct from
    // ANSI index 0 despite both being "black" in different palettes.
    expect(resolveColor({ type: "indexed", index: 16 }, "#fff")).toBe("rgb(0, 0, 0)");
    // Index 231 is the cube's (5,5,5) corner - pure white.
    expect(resolveColor({ type: "indexed", index: 231 }, "#fff")).toBe("rgb(255, 255, 255)");
    // A mid-cube value: r=1,g=2,b=3 -> 16 + 36*1 + 6*2 + 3 = 67.
    expect(resolveColor({ type: "indexed", index: 67 }, "#fff")).toBe("rgb(95, 135, 175)");
  });

  it("resolves the greyscale ramp (232-255)", () => {
    expect(resolveColor({ type: "indexed", index: 232 }, "#fff")).toBe("rgb(8, 8, 8)");
    expect(resolveColor({ type: "indexed", index: 255 }, "#fff")).toBe("rgb(238, 238, 238)");
  });

  it("passes truecolor rgb values through directly", () => {
    const color: TermColor = { type: "rgb", r: 10, g: 20, b: 30 };
    expect(resolveColor(color, "#fff")).toBe("rgb(10, 20, 30)");
  });
});

describe("isCellSelected", () => {
  it("selects within a single-row range inclusively", () => {
    const start = { row: 0, col: 2 };
    const end = { row: 0, col: 5 };
    expect(isCellSelected(0, 1, start, end)).toBe(false);
    expect(isCellSelected(0, 2, start, end)).toBe(true);
    expect(isCellSelected(0, 5, start, end)).toBe(true);
    expect(isCellSelected(0, 6, start, end)).toBe(false);
  });

  it("spans full row width for rows strictly between the endpoints", () => {
    const start = { row: 0, col: 3 };
    const end = { row: 2, col: 1 };
    // Row 1 is strictly between - every column counts, even far to the
    // left of the start column or far to the right of the end column.
    expect(isCellSelected(1, 0, start, end)).toBe(true);
    expect(isCellSelected(1, 99, start, end)).toBe(true);
    // Row 0: only from the start column onward.
    expect(isCellSelected(0, 2, start, end)).toBe(false);
    expect(isCellSelected(0, 3, start, end)).toBe(true);
    // Row 2: only up to the end column.
    expect(isCellSelected(2, 1, start, end)).toBe(true);
    expect(isCellSelected(2, 2, start, end)).toBe(false);
  });

  it("is order-independent - dragging backwards selects the same cells", () => {
    const forward = { start: { row: 0, col: 1 }, end: { row: 1, col: 2 } };
    const backward = { start: { row: 1, col: 2 }, end: { row: 0, col: 1 } };
    for (const [row, col] of [
      [0, 0],
      [0, 1],
      [0, 5],
      [1, 0],
      [1, 2],
      [1, 3],
    ]) {
      expect(isCellSelected(row, col, forward.start, forward.end)).toBe(
        isCellSelected(row, col, backward.start, backward.end),
      );
    }
  });

  it("rejects rows entirely outside the range", () => {
    const start = { row: 1, col: 0 };
    const end = { row: 1, col: 5 };
    expect(isCellSelected(0, 0, start, end)).toBe(false);
    expect(isCellSelected(2, 0, start, end)).toBe(false);
  });
});

describe("selectionText", () => {
  it("extracts a single-row range inclusively", () => {
    const rows = [row("hello world")];
    expect(selectionText(rows, { row: 0, col: 0 }, { row: 0, col: 4 })).toBe("hello");
  });

  it("spans full rows strictly between the endpoints and joins with newlines", () => {
    const rows = [row("abcdef"), row("ghijkl"), row("mnopqr")];
    // From column 3 of row 0 to column 1 of row 2: row 1 comes through in
    // full even though the drag didn't touch its start/end columns.
    const text = selectionText(rows, { row: 0, col: 3 }, { row: 2, col: 1 });
    expect(text).toBe("def\nghijkl\nmn");
  });

  it("is order-independent - dragging backwards yields the same text", () => {
    const rows = [row("abcdef"), row("ghijkl")];
    const forward = selectionText(rows, { row: 0, col: 1 }, { row: 1, col: 2 });
    const backward = selectionText(rows, { row: 1, col: 2 }, { row: 0, col: 1 });
    expect(forward).toBe(backward);
  });

  it("keeps trailing spaces - a row's blank cells are real grid content", () => {
    const rows = [row("hi   ")];
    expect(selectionText(rows, { row: 0, col: 0 }, { row: 0, col: 4 })).toBe("hi   ");
  });

  it("clamps to the rows actually present rather than throwing on a stale endpoint", () => {
    const rows = [row("only one row")];
    // A selection dragged to a row that no longer exists (e.g. the screen
    // shrank since) shouldn't crash - it should just stop at what's there.
    expect(selectionText(rows, { row: 0, col: 0 }, { row: 5, col: 0 })).toBe("only one row");
  });
});
