import { describe, expect, it } from "vitest";

import { clampRect, MIN_H, MIN_W, moved, readRect, resizedEdge } from "./floatingRect";

const view = { w: 1600, h: 900 };

describe("clampRect", () => {
  it("leaves a rectangle that fits as it is", () => {
    const rect = { x: 100, y: 100, w: 400, h: 500 };
    expect(clampRect(rect, view)).toEqual(rect);
  });

  it("keeps part of the window on screen however far it was pushed", () => {
    const off = clampRect({ x: 5000, y: 5000, w: 400, h: 500 }, view);
    expect(off.x).toBeLessThan(view.w);
    expect(off.y).toBeLessThan(view.h);
    const left = clampRect({ x: -5000, y: -5000, w: 400, h: 500 }, view);
    expect(left.x + left.w).toBeGreaterThan(0);
    expect(left.y).toBe(0);
  });

  it("limits the size to the screen and to the minimum", () => {
    expect(clampRect({ x: 0, y: 0, w: 5000, h: 5000 }, view)).toMatchObject({ w: view.w, h: view.h });
    expect(clampRect({ x: 0, y: 0, w: 10, h: 10 }, view)).toMatchObject({ w: MIN_W, h: MIN_H });
  });
});

describe("moved and resizedEdge", () => {
  const rect = { x: 100, y: 100, w: 400, h: 500 };

  it("moves by the offset", () => {
    expect(moved(rect, 10, -20)).toEqual({ x: 110, y: 80, w: 400, h: 500 });
  });

  it("grows rightwards and downwards from the right and bottom edges", () => {
    expect(resizedEdge(rect, "e", 50)).toEqual({ ...rect, w: 450 });
    expect(resizedEdge(rect, "s", 50)).toEqual({ ...rect, h: 550 });
  });

  it("grows leftwards and upwards from the left and top edges, moving the origin", () => {
    expect(resizedEdge(rect, "w", 50)).toEqual({ ...rect, x: 50, w: 450 });
    expect(resizedEdge(rect, "n", 50)).toEqual({ ...rect, y: 50, h: 550 });
  });

  it("stops at the minimum size without dragging the opposite edge along", () => {
    const small = resizedEdge(rect, "w", -1000);
    expect(small.w).toBe(MIN_W);
    expect(small.x + small.w).toBe(rect.x + rect.w);
  });
});

describe("readRect", () => {
  it("reads a stored rectangle and refuses anything else", () => {
    expect(readRect({ x: 1, y: 2, w: 3, h: 4 })).toEqual({ x: 1, y: 2, w: 3, h: 4 });
    expect(readRect({ x: 1, y: 2, w: "3", h: 4 })).toBeNull();
    expect(readRect(null)).toBeNull();
  });
});
