import { describe, expect, it } from "vitest";

import { LineStore } from "./buffer";
import { FoldState } from "./fold/state";
import { clearOfSticky, enclosing, MAX_STICKY, NO_STICKY, stickyAt, type StickyLayout } from "./sticky";
import { VisualLayout } from "./visual";

/** 40 lines; `fn` 0–30 holds `if` 5–20, which holds `loop` 10–15. Rows are 10 px. */
const RANGES = [
  { start: 0, end: 30 },
  { start: 5, end: 20 },
  { start: 10, end: 15 },
];
const ROW = 10;

function layout(lines = 40) {
  return new VisualLayout(new LineStore(Array.from({ length: lines }, (_, i) => `l${i}`).join("\n")), {
    wrap: false,
    width: 80,
    tabSize: 4,
  });
}

describe("enclosing", () => {
  it("lists the blocks a line is inside, below their headers, outermost first", () => {
    expect(enclosing(RANGES, 12)).toEqual(RANGES);
    expect(enclosing(RANGES, 10)).toEqual(RANGES.slice(0, 2));
    expect(enclosing(RANGES, 25)).toEqual([RANGES[0]]);
    expect(enclosing(RANGES, 0)).toEqual([]);
  });
});

describe("stickyAt (T9)", () => {
  it("pins nothing at the top of the text", () => {
    expect(stickyAt(RANGES, layout(), 0, ROW).headers).toEqual([]);
  });

  it("pins a header once it has scrolled away, and the ones inside it as they do", () => {
    // Line 1 at the top: `fn` scrolled away; `if` (line 5) still in view below the one header.
    expect(stickyAt(RANGES, layout(), 1 * ROW, ROW).headers).toEqual([RANGES[0]]);
    // Line 12 at the top: all three blocks around it have scrolled away.
    const s = stickyAt(RANGES, layout(), 12 * ROW, ROW);
    expect(s.headers).toEqual(RANGES);
    expect(s.height).toBe(3 * ROW);
  });

  it("pushes the innermost header up as the line after its block arrives under it", () => {
    // `loop` ends at 15; with line 13 at the top the three headers cover lines 13–15, line 16 is right under.
    expect(stickyAt(RANGES, layout(), 13 * ROW, ROW)).toMatchObject({
      push: 0,
      height: 30,
    });
    const s = stickyAt(RANGES, layout(), 13 * ROW + 5, ROW);
    expect(s.headers).toEqual(RANGES);
    expect(s.push).toBe(-5);
    expect(s.height).toBe(25);
  });

  it("stops at the given number of headers", () => {
    expect(stickyAt(RANGES, layout(), 12 * ROW, ROW, 2).headers).toEqual(RANGES.slice(0, 2));
  });

  it("counts rows, not lines, when lines are folded", () => {
    const l = layout();
    const folds = new FoldState();
    folds.close({ start: 1, end: 4 });
    l.setFolds(folds);
    // Row 2 now shows line 6: inside `fn` and `if`, both scrolled away.
    expect(stickyAt(RANGES, l, 2 * ROW + 1, ROW).headers).toEqual(RANGES.slice(0, 2));
  });
});

describe("clearOfSticky", () => {
  it("puts a row just below the headers that stick at the resulting position", () => {
    const heightAt = (top: number) => stickyAt(RANGES, layout(), top, ROW).height;
    // Line 12 must show below the three headers there: the view starts three rows above it.
    expect(clearOfSticky(12 * ROW, heightAt)).toBe(9 * ROW);
    expect(clearOfSticky(0, heightAt)).toBe(0);
  });
});

describe("stickyAt edge cases", () => {
  it("pins nothing without ranges, without rows, or with no room for a header", () => {
    expect(stickyAt([], layout(), 12 * ROW, ROW)).toEqual(NO_STICKY);
    expect(stickyAt(RANGES, layout(), 12 * ROW, ROW, 0)).toEqual(NO_STICKY);
    expect(stickyAt(RANGES, layout(), 12 * ROW, ROW, -1)).toEqual(NO_STICKY);
    const empty: StickyLayout = {
      totalRows: 0,
      firstRow: () => 0,
      rowStarts: () => [0],
      lineAt: () => ({ line: 0 }),
    };
    expect(stickyAt(RANGES, empty, 12 * ROW, ROW)).toEqual(NO_STICKY);
  });

  it("does not repeat a header whose own line is still in view below the pinned ones", () => {
    // Line 4 at the top: `fn` is pinned over it, `if` (line 5) shows right under it and is not pinned.
    expect(stickyAt(RANGES, layout(), 4 * ROW, ROW).headers).toEqual([RANGES[0]]);
    // One row further, line 5 has gone under `fn`'s header: `if` sticks too.
    expect(stickyAt(RANGES, layout(), 5 * ROW, ROW).headers).toEqual(RANGES.slice(0, 2));
  });

  it("pins a wrapped header only once all of its rows have scrolled away", () => {
    // Line 0 wraps into four rows (0–3); lines 1–5 are its block, line 6 the one after.
    const text = ["fn " + "a".repeat(25), "b1", "b2", "b3", "b4", "b5", "c"].join("\n");
    const wrapped = new VisualLayout(new LineStore(text), {
      wrap: true,
      width: 10,
      tabSize: 4,
    });
    expect(wrapped.firstRow(1)).toBe(4);
    const block = [{ start: 0, end: 5 }];
    for (const top of [1, 2, 3]) expect(stickyAt(block, wrapped, top * ROW, ROW).headers).toEqual([]);
    expect(stickyAt(block, wrapped, 4 * ROW, ROW)).toEqual({
      headers: block,
      push: 0,
      height: ROW,
    });
    // Line 6 is row 9: it starts pushing once the header's one row reaches it.
    expect(stickyAt(block, wrapped, 8 * ROW, ROW)).toMatchObject({
      push: 0,
      height: ROW,
    });
    expect(stickyAt(block, wrapped, 8 * ROW + 5, ROW)).toMatchObject({
      push: -5,
      height: 5,
    });
  });

  it("pushes only the innermost of two blocks that end on the same line, then drops it", () => {
    const nested = [
      { start: 0, end: 10 },
      { start: 2, end: 10 },
    ];
    const l = layout(20);
    // Line 9 at the top: both headers cover lines 9–10, line 11 arrives right under them.
    expect(stickyAt(nested, l, 9 * ROW, ROW)).toEqual({
      headers: nested,
      push: 0,
      height: 2 * ROW,
    });
    expect(stickyAt(nested, l, 9 * ROW + 5, ROW)).toEqual({
      headers: nested,
      push: -5,
      height: 15,
    });
    // Line 10 at the top: the inner header is gone, the outer one now gets pushed in turn.
    expect(stickyAt(nested, l, 10 * ROW, ROW)).toEqual({
      headers: [nested[0]],
      push: 0,
      height: ROW,
    });
    expect(stickyAt(nested, l, 10 * ROW + 5, ROW)).toEqual({
      headers: [nested[0]],
      push: -5,
      height: 5,
    });
  });

  it("pushes a block ending on the last line out against the end of the text", () => {
    const l = layout(20);
    const all = [{ start: 0, end: 19 }];
    expect(stickyAt(all, l, 15 * ROW, ROW)).toMatchObject({
      push: 0,
      height: ROW,
    });
    expect(stickyAt(all, l, 18 * ROW, ROW)).toMatchObject({
      push: 0,
      height: ROW,
    });
    expect(stickyAt(all, l, 19 * ROW + 5, ROW)).toMatchObject({
      headers: all,
      push: -5,
      height: 5,
    });
  });

  it("never reports a negative height, however far the header is pushed", () => {
    for (let top = 0; top <= 39 * ROW; top += 3) {
      const s = stickyAt(RANGES, layout(), top, ROW);
      expect(s.height).toBeGreaterThanOrEqual(0);
      expect(s.push).toBeLessThanOrEqual(0);
      expect(s.headers.length).toBeLessThanOrEqual(MAX_STICKY);
    }
  });

  it("stops at MAX_STICKY headers for deeper nesting", () => {
    const deep = Array.from({ length: 8 }, (_, i) => ({
      start: i,
      end: 30 - i,
    }));
    const s = stickyAt(deep, layout(), 12 * ROW, ROW);
    expect(s.headers).toEqual(deep.slice(0, MAX_STICKY));
    expect(s.height).toBe(MAX_STICKY * ROW);
  });
});

describe("clearOfSticky edge cases", () => {
  it("never scrolls above the text", () => {
    expect(clearOfSticky(-50, () => 0)).toBe(0);
    expect(clearOfSticky(5, () => 30)).toBe(0);
  });

  it("subtracts a height that does not depend on the position", () => {
    expect(clearOfSticky(100, () => 25)).toBe(75);
  });

  it("leaves every row start clear of the headers, at most one row below them", () => {
    const l = layout();
    const heightAt = (top: number) => stickyAt(RANGES, l, top, ROW).height;
    for (let row = 0; row < 40; row++) {
      const y = row * ROW;
      const top = clearOfSticky(y, heightAt);
      // Not under the headers, and no more than a row of gap where no exact position exists.
      expect(top + heightAt(top)).toBeLessThanOrEqual(y);
      expect(y - (top + heightAt(top))).toBeLessThanOrEqual(ROW);
    }
  });
});

describe("clearOfSticky at any pixel", () => {
  it("leaves a position inside a row clear of the headers, too", () => {
    const heightAt = (top: number) => stickyAt(RANGES, layout(), top, ROW).height;
    for (const y of [156, 157, 206, 209, 307, 309]) {
      const top = clearOfSticky(y, heightAt);
      expect(top + heightAt(top)).toBeLessThanOrEqual(y);
    }
  });
});

describe("a wrapped header (review, ED5.6)", () => {
  it("sticks only once its last row has scrolled up, so it never shows twice", () => {
    const lines = ["# " + "long heading ".repeat(3), ...Array.from({ length: 20 }, (_, i) => `text ${i}`)];
    const l = new VisualLayout(new LineStore(lines.join("\n")), { wrap: true, width: 16, tabSize: 4 });
    const rows = l.rowStarts(0).length;
    expect(rows).toBeGreaterThan(1);
    const ranges = [{ start: 0, end: 20 }];
    // Part of the header's last row is still on screen: nothing pinned yet; the next line at the top pins it.
    expect(stickyAt(ranges, l, rows * ROW - 1, ROW).headers).toEqual([]);
    expect(stickyAt(ranges, l, rows * ROW, ROW).headers).toEqual(ranges);
  });
});
