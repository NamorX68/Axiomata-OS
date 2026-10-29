// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { cellWidth } from "./cellWidth";

/** A measuring span 64 cells of 9.6 px wide, drawn at `scale` (the panel's opening flight) by its bounding box. */
function measurer(scale: number): HTMLSpanElement {
  const span = document.createElement("span");
  span.style.position = "absolute";
  span.style.width = `${64 * 9.6}px`;
  span.getBoundingClientRect = () => new DOMRect(0, 0, 64 * 9.6 * scale, 16 * scale);
  document.body.append(span);
  return span;
}

describe("cellWidth", () => {
  it("is the layout width, not the drawn one, while the panel around it is still scaled", () => {
    // The owner's new note: measured at the start of the flight, the caret sat half way along the typed text.
    const span = measurer(0.5);
    expect(span.getBoundingClientRect().width / 64).toBeCloseTo(4.8);
    expect(cellWidth(span, 64)).toBeCloseTo(9.6);
  });

  it("has no width for a span that is not laid out", () => {
    const span = document.createElement("span");
    expect(cellWidth(span, 64)).toBeNull();
  });
});
