import { describe, expect, it } from "vitest";

import { LineStore } from "../editor/buffer";
import { VisualLayout } from "../editor/visual";
import { scrollTopFor, visibleLines } from "./viScroll";

/** Ten short lines, unwrapped: one row each. */
function flat(): VisualLayout {
  const text = Array.from({ length: 10 }, (_, i) => `line ${i}`).join("\n");
  return new VisualLayout(new LineStore(text), { wrap: false, width: 80, tabSize: 4 });
}

describe("visibleLines", () => {
  it("names the lines whose rows are fully on screen", () => {
    expect(visibleLines(flat(), { scrollTop: 0, viewH: 100, rowH: 20 })).toEqual({ top: 0, bottom: 4 });
    // A half-hidden row at the bottom does not count.
    expect(visibleLines(flat(), { scrollTop: 40, viewH: 110, rowH: 20 })).toEqual({ top: 2, bottom: 6 });
  });

  it("stops at the last line when the view is taller than the file", () => {
    expect(visibleLines(flat(), { scrollTop: 0, viewH: 1000, rowH: 20 })).toEqual({ top: 0, bottom: 9 });
  });

  it("counts wrapped rows towards their line", () => {
    const layout = new VisualLayout(new LineStore("aaaa bbbb cccc\nx\ny"), { wrap: true, width: 5, tabSize: 4 });
    // Line 0 takes three rows; a view of three rows shows only it.
    expect(visibleLines(layout, { scrollTop: 0, viewH: 60, rowH: 20 })).toEqual({ top: 0, bottom: 0 });
    expect(visibleLines(layout, { scrollTop: 0, viewH: 100, rowH: 20 })).toEqual({ top: 0, bottom: 2 });
  });
});

describe("scrollTopFor (zt zz zb)", () => {
  const view = { scrollTop: 0, viewH: 100, rowH: 20 };

  it("puts the line at the top, the centre or the bottom", () => {
    expect(scrollTopFor(flat(), view, 5, "top")).toBe(100);
    expect(scrollTopFor(flat(), view, 5, "center")).toBe(60);
    expect(scrollTopFor(flat(), view, 5, "bottom")).toBe(20);
  });

  it("never scrolls above the first line", () => {
    expect(scrollTopFor(flat(), view, 1, "bottom")).toBe(0);
    expect(scrollTopFor(flat(), view, 0, "center")).toBe(0);
  });
});
