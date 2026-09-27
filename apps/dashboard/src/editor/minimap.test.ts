import { describe, expect, it } from "vitest";

import { inkRuns, minimapLayout, onSlider, scrollForClick, scrollForSlider, type MinimapInput } from "./minimap";

/** 1000 rows of 20 px, a 400 px view; the minimap draws rows 2 px high in 300 px. */
const TALL: MinimapInput = {
  totalRows: 1000,
  rowH: 20,
  viewH: 400,
  scrollTop: 0,
  rowPx: 2,
  height: 300,
};

describe("minimapLayout (T8)", () => {
  it("shows a short text whole, the slider over the rows in view", () => {
    const short = { ...TALL, totalRows: 100, scrollTop: 400 };
    expect(minimapLayout(short)).toEqual({
      miniScroll: 0,
      sliderTop: 40,
      sliderHeight: 40,
      firstRow: 0,
      lastRow: 99,
    });
  });

  it("scrolls a tall text's minimap in proportion, the slider travelling its whole height", () => {
    expect(minimapLayout(TALL)).toMatchObject({ miniScroll: 0, sliderTop: 0 });
    const end = minimapLayout({ ...TALL, scrollTop: 1000 * 20 - 400 });
    expect(end.miniScroll).toBe(2000 - 300);
    expect(end.sliderTop).toBe(300 - 40);
    expect(end.firstRow).toBe(850);
    expect(end.lastRow).toBe(999);
  });
});

describe("dragging and clicking", () => {
  it("dragging the slider to a place and reading it back agree", () => {
    const top = scrollForSlider(TALL, 130);
    expect(minimapLayout({ ...TALL, scrollTop: top }).sliderTop).toBeCloseTo(130);
    expect(scrollForSlider(TALL, -50)).toBe(0);
    expect(scrollForSlider(TALL, 1000)).toBe(1000 * 20 - 400);
  });

  it("a click centres the view on the row under it", () => {
    // Row 50 is at minimap y 100 when unscrolled: its middle goes to the view's middle.
    expect(scrollForClick(TALL, 100)).toBe(50 * 20 - 200);
    expect(scrollForClick(TALL, 0)).toBe(0);
  });

  it("knows whether a point is on the slider", () => {
    expect(onSlider(TALL, 10)).toBe(true);
    expect(onSlider(TALL, 60)).toBe(false);
  });
});

describe("inkRuns", () => {
  it("turns each coloured piece into runs of non-blank cells, tabs to their stop", () => {
    expect(inkRuns([{ text: "\tlet x", token: "keyword" }, { text: " = 1;" }], 4)).toEqual([
      { from: 4, to: 7, token: "keyword" },
      { from: 8, to: 9, token: "keyword" },
      { from: 10, to: 11, token: undefined },
      { from: 12, to: 14, token: undefined },
    ]);
  });

  it("starts a continuation row at its indent", () => {
    expect(inkRuns([{ text: "ab" }], 4, 2)).toEqual([{ from: 2, to: 4, token: undefined }]);
  });
});

describe("minimapLayout edge cases", () => {
  it("draws no rows for an empty text and cannot scroll it", () => {
    const empty = { ...TALL, totalRows: 0, scrollTop: 50 };
    const m = minimapLayout(empty);
    expect(m).toMatchObject({ miniScroll: 0, sliderTop: 0, firstRow: 0 });
    expect(m.lastRow).toBeLessThan(m.firstRow);
    expect(scrollForSlider(empty, 100)).toBe(0);
    expect(scrollForClick(empty, 100)).toBe(0);
  });

  it("keeps the slider at the top when the whole text fits in the view", () => {
    const fits = { ...TALL, totalRows: 10, scrollTop: 0 };
    expect(minimapLayout(fits)).toMatchObject({
      miniScroll: 0,
      sliderTop: 0,
      firstRow: 0,
      lastRow: 9,
    });
    expect(scrollForSlider(fits, 100)).toBe(0);
    expect(scrollForClick(fits, 15)).toBe(0);
  });

  it("clamps a scroll position past either end of the text", () => {
    const past = minimapLayout({ ...TALL, scrollTop: 1e6 });
    const end = minimapLayout({ ...TALL, scrollTop: 1000 * 20 - 400 });
    expect(past).toEqual(end);
    expect(minimapLayout({ ...TALL, scrollTop: -100 })).toEqual(minimapLayout(TALL));
  });

  it("never lets the slider be taller than the minimap", () => {
    const huge = minimapLayout({ ...TALL, viewH: 20 * 400 });
    expect(huge.sliderHeight).toBe(300);
    expect(huge.sliderTop).toBe(0);
  });

  it("handles fractional row heights, the cut-off rows included", () => {
    const frac = { ...TALL, rowPx: 1.5, rowH: 19.5 };
    const mid = minimapLayout({ ...frac, scrollTop: (1000 * 19.5 - 400) / 2 });
    expect(mid.miniScroll).toBeCloseTo((1000 * 1.5 - 300) / 2);
    expect(mid.firstRow).toBe(Math.floor(mid.miniScroll / 1.5));
    expect(mid.lastRow).toBe(Math.ceil((mid.miniScroll + 300) / 1.5));
    expect(mid.sliderHeight).toBeCloseTo((400 / 19.5) * 1.5);
    // Dragging and reading back still agree.
    const top = scrollForSlider(frac, 77.7);
    expect(minimapLayout({ ...frac, scrollTop: top }).sliderTop).toBeCloseTo(77.7);
  });
});

describe("dragging and clicking at the edges", () => {
  it("puts the text at its top and bottom with the slider at the minimap's", () => {
    const range = 300 - 40;
    expect(scrollForSlider(TALL, 0)).toBe(0);
    expect(scrollForSlider(TALL, range)).toBe(1000 * 20 - 400);
  });

  it("scales the drag to a short text's own slider range", () => {
    // 100 rows fill 200 px of the minimap: the slider travels 160 px for a 1600 px scroll.
    const short = { ...TALL, totalRows: 100 };
    expect(scrollForSlider(short, 80)).toBe(800);
    expect(scrollForSlider(short, 250)).toBe(100 * 20 - 400);
  });

  it("clamps a click at the very bottom of a scrolled minimap to the end of the text", () => {
    const end = { ...TALL, scrollTop: 1000 * 20 - 400 };
    expect(scrollForClick(end, 300)).toBe(1000 * 20 - 400);
    // A click at its top centres on the first row shown.
    expect(scrollForClick(end, 0)).toBe(((2000 - 300) / 2) * 20 - 200);
  });

  it("counts both slider edges as on it", () => {
    expect(onSlider(TALL, 0)).toBe(true);
    expect(onSlider(TALL, 40)).toBe(true);
    expect(onSlider(TALL, 40.5)).toBe(false);
    expect(onSlider(TALL, -1)).toBe(false);
  });
});

describe("inkRuns edge cases", () => {
  it("has no ink for an empty or blank row", () => {
    expect(inkRuns([], 4)).toEqual([]);
    expect(inkRuns([{ text: "" }], 4)).toEqual([]);
    expect(inkRuns([{ text: "  \t  \t" }], 4)).toEqual([]);
  });

  it("gives a character outside the BMP one cell, not two", () => {
    expect(inkRuns([{ text: "a😀b c" }], 4)).toEqual([
      { from: 0, to: 3, token: undefined },
      { from: 4, to: 5, token: undefined },
    ]);
  });

  it("takes a tab in the middle of a row to the next stop, a full stop when already on one", () => {
    expect(inkRuns([{ text: "ab\tc" }], 4)).toEqual([
      { from: 0, to: 2, token: undefined },
      { from: 4, to: 5, token: undefined },
    ]);
    expect(inkRuns([{ text: "abcd\te" }], 4)).toEqual([
      { from: 0, to: 4, token: undefined },
      { from: 8, to: 9, token: undefined },
    ]);
  });

  it("keeps tab stops counted from the row's indent onwards", () => {
    expect(inkRuns([{ text: "\tx" }], 4, 2)).toEqual([{ from: 4, to: 5, token: undefined }]);
  });

  it("splits a word across two coloured pieces into two runs, carrying each token", () => {
    expect(
      inkRuns(
        [
          { text: "ab", token: "a" },
          { text: "cd", token: null },
        ],
        4,
      ),
    ).toEqual([
      { from: 0, to: 2, token: "a" },
      { from: 2, to: 4, token: null },
    ]);
  });
});
