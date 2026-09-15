import { describe, expect, it } from "vitest";

import {
  BLOCK_ELEMENT_RECTS,
  cellFont,
  isCellSelected,
  resolveColor,
  SHADE_ALPHA,
  selectionText,
  type TermCell,
  type TermColor,
} from "./TerminalScreen";
import { THEMES } from "./terminalThemes";

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

  it("resolves an indexed colour against an explicit palette instead of the xterm default", () => {
    expect(resolveColor({ type: "indexed", index: 1 }, "#fff", { palette: THEMES.nord })).toBe(THEMES.nord[1]);
    expect(resolveColor({ type: "indexed", index: 1 }, "#fff", { palette: THEMES["gruvbox-dark"] })).toBe(
      THEMES["gruvbox-dark"][1],
    );
  });

  it("maps a bold low-8 index (0-7) to its +8 bright counterpart when bright is set", () => {
    expect(resolveColor({ type: "indexed", index: 1 }, "#fff", { bright: true })).toBe(
      resolveColor({ type: "indexed", index: 9 }, "#fff"),
    );
  });

  it("leaves an already-bright index (8-15) unchanged when bright is set", () => {
    expect(resolveColor({ type: "indexed", index: 9 }, "#fff", { bright: true })).toBe(
      resolveColor({ type: "indexed", index: 9 }, "#fff"),
    );
  });

  it("ignores bright for default and truecolor rgb colours (no bright variant exists)", () => {
    expect(resolveColor({ type: "default" }, "#abcdef", { bright: true })).toBe("#abcdef");
    const color: TermColor = { type: "rgb", r: 10, g: 20, b: 30 };
    expect(resolveColor(color, "#fff", { bright: true })).toBe("rgb(10, 20, 30)");
  });
});

describe("cellFont", () => {
  it("leaves the font shorthand untouched when neither bold nor a weight is set", () => {
    expect(cellFont("14px monospace", false)).toBe("14px monospace");
    expect(cellFont("14px monospace", false, undefined)).toBe("14px monospace");
  });

  it("prefixes a configured weight for a non-bold cell", () => {
    expect(cellFont("14px monospace", false, 300)).toBe("300 14px monospace");
  });

  it("always uses the literal bold keyword for a bold cell, ignoring any configured weight", () => {
    expect(cellFont("14px monospace", true)).toBe("bold 14px monospace");
    expect(cellFont("14px monospace", true, 300)).toBe("bold 14px monospace");
  });
});

describe("BLOCK_ELEMENT_RECTS / SHADE_ALPHA (Checkpoint 5k)", () => {
  it("together cover the full U+2580-259F Block Elements range", () => {
    // 32 codepoints in the range; every one is either a rect-fill entry or
    // a shade-alpha entry, none are both, and nothing outside the range
    // sneaks into either table.
    const covered = new Set([...Object.keys(BLOCK_ELEMENT_RECTS), ...Object.keys(SHADE_ALPHA)]);
    expect(covered.size).toBe(32);
    for (const ch of covered) {
      const code = ch.codePointAt(0) ?? 0;
      expect(code, ch).toBeGreaterThanOrEqual(0x2580);
      expect(code, ch).toBeLessThanOrEqual(0x259f);
    }
    for (const ch of Object.keys(BLOCK_ELEMENT_RECTS)) {
      expect(SHADE_ALPHA[ch], `${ch} in both tables`).toBeUndefined();
    }
  });

  it("every rectangle stays within the cell and has positive area", () => {
    for (const [ch, rects] of Object.entries(BLOCK_ELEMENT_RECTS)) {
      expect(rects.length, ch).toBeGreaterThan(0);
      for (const [x0, y0, x1, y1] of rects) {
        for (const v of [x0, y0, x1, y1]) {
          expect(v, ch).toBeGreaterThanOrEqual(0);
          expect(v, ch).toBeLessThanOrEqual(1);
        }
        expect(x1, ch).toBeGreaterThan(x0);
        expect(y1, ch).toBeGreaterThan(y0);
      }
    }
  });

  it("every shade alpha is a fraction strictly between 0 and 1", () => {
    for (const [ch, alpha] of Object.entries(SHADE_ALPHA)) {
      expect(alpha, ch).toBeGreaterThan(0);
      expect(alpha, ch).toBeLessThan(1);
    }
  });

  it("SHADE_ALPHA contains exactly the three shade characters ░▒▓", () => {
    expect(Object.keys(SHADE_ALPHA).sort()).toEqual(["░", "▒", "▓"].sort());
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
