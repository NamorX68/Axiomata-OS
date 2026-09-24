import { describe, expect, it } from "vitest";

import { LineStore } from "./buffer";
import { cursorCell, posAtCell, rowSlice, selectionRuns } from "./geometry";
import { pos, range } from "./position";
import { VisualLayout } from "./visual";

// "  aaa bbb ccc" wraps at width 8 into rows [0, 6, 10], continuation indent 2.
const TEXT = "  aaa bbb ccc\n\n\tx";

function setup(wrap = true) {
  const store = new LineStore(TEXT);
  return { store, layout: new VisualLayout(store, { wrap, width: 8, tabSize: 4 }) };
}

describe("rowSlice", () => {
  it("describes a continuation row with its indent", () => {
    const { store, layout } = setup();
    expect(rowSlice(layout, store, 1)).toEqual({
      line: 0,
      sub: 1,
      start: 6,
      end: 10,
      text: "bbb ",
      indent: 2,
      last: false,
    });
    expect(rowSlice(layout, store, 3)).toMatchObject({ line: 1, text: "", last: true });
  });
});

describe("cursorCell", () => {
  it("places the cursor after the indent on a continuation row", () => {
    const { store, layout } = setup();
    expect(cursorCell(layout, store, pos(0, 7), 4)).toEqual({ row: 1, cell: 3 });
    // At the wrap point the column belongs to the row it starts.
    expect(cursorCell(layout, store, pos(0, 6), 4)).toEqual({ row: 1, cell: 2 });
  });

  it("counts a tab to its stop", () => {
    const { store, layout } = setup();
    expect(cursorCell(layout, store, pos(2, 1), 4)).toEqual({ row: 4, cell: 4 });
  });
});

describe("selectionRuns", () => {
  it("covers each visual row, with one cell for a selected line break", () => {
    const { store, layout } = setup();
    const runs = selectionRuns(layout, store, range(pos(0, 3), pos(2, 1)), 0, 10, 4);
    expect(runs).toEqual([
      { row: 0, from: 3, to: 6 },
      { row: 1, from: 2, to: 6 },
      { row: 2, from: 2, to: 6 },
      { row: 3, from: 0, to: 1 },
      { row: 4, from: 0, to: 4 },
    ]);
  });

  it("only returns rows inside the visible window", () => {
    const { store, layout } = setup();
    expect(selectionRuns(layout, store, range(pos(0, 0), pos(2, 2)), 3, 3, 4)).toEqual([{ row: 3, from: 0, to: 1 }]);
  });

  it("draws nothing for an empty selection", () => {
    const { store, layout } = setup();
    expect(selectionRuns(layout, store, range(pos(0, 3), pos(0, 3)), 0, 10, 4)).toEqual([]);
  });
});

describe("posAtCell", () => {
  it("finds the nearest column, respecting the indent", () => {
    const { store, layout } = setup();
    expect(posAtCell(layout, store, 1, 3.4, 4)).toEqual(pos(0, 7));
    expect(posAtCell(layout, store, 1, 0, 4)).toEqual(pos(0, 6));
  });

  it("stays on a wrapped row when clicking past its end", () => {
    const { store, layout } = setup();
    expect(posAtCell(layout, store, 0, 40, 4)).toEqual(pos(0, 5));
    expect(posAtCell(layout, store, 2, 40, 4)).toEqual(pos(0, 13));
  });

  it("clamps rows outside the document", () => {
    const { store, layout } = setup();
    expect(posAtCell(layout, store, -3, 0, 4)).toEqual(pos(0, 0));
    expect(posAtCell(layout, store, 99, 99, 4)).toEqual(pos(2, 2));
  });

  it("snaps inside a tab to the nearer side", () => {
    const { store, layout } = setup();
    expect(posAtCell(layout, store, 4, 1, 4)).toEqual(pos(2, 0));
    expect(posAtCell(layout, store, 4, 3, 4)).toEqual(pos(2, 1));
  });
});
