import { describe, expect, it } from "vitest";

import {
  clampRect,
  clampSize,
  MIN_H,
  MIN_W,
  moved,
  readRect,
  resizedBy,
  resizedEdge,
  resizedWithin,
  windowStyle,
} from "./floatingRect";

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

describe("a caller's own minimum", () => {
  const min = { w: 480, h: 240 };

  it("holds the size to it instead of the detail window's", () => {
    expect(clampSize({ w: 10, h: 10 }, view, min)).toEqual(min);
    expect(clampRect({ x: 0, y: 0, w: 10, h: 10 }, view, min)).toMatchObject(min);
    expect(resizedEdge({ x: 0, y: 0, w: 500, h: 300 }, "e", -1000, min).w).toBe(min.w);
  });

  it("lets the minimum win on a screen smaller than it", () => {
    expect(clampSize({ w: 1000, h: 1000 }, { w: 300, h: 200 }, min)).toEqual(min);
  });
});

describe("resizedBy", () => {
  const rect = { x: 100, y: 100, w: 400, h: 500 };

  it("pulls one side from an edge, ignoring the other axis", () => {
    expect(resizedBy(rect, "e", { dw: 50, dh: 30 })).toEqual({ ...rect, w: 450 });
    expect(resizedBy(rect, "n", { dw: 50, dh: 30 })).toEqual({ ...rect, y: 70, h: 530 });
  });

  it("pulls both sides it joins from a corner, the opposite corner staying put", () => {
    const pulled = resizedBy(rect, "nw", { dw: 50, dh: 30 });
    expect(pulled).toEqual({ x: 50, y: 70, w: 450, h: 530 });
    expect(pulled.x + pulled.w).toBe(rect.x + rect.w);
    expect(pulled.y + pulled.h).toBe(rect.y + rect.h);
    expect(resizedBy(rect, "se", { dw: 50, dh: 30 })).toEqual({ ...rect, w: 450, h: 530 });
  });
});

describe("resizedWithin", () => {
  const rect = { x: 100, y: 50, w: 400, h: 500 };

  it("stops a pulled edge at the screen's edge", () => {
    expect(resizedWithin(rect, "w", { dw: 500, dh: 0 }, view)).toEqual({ ...rect, x: 0, w: 500 });
    expect(resizedWithin(rect, "n", { dw: 0, dh: 500 }, view)).toEqual({ ...rect, y: 0, h: 550 });
    expect(resizedWithin(rect, "se", { dw: 5000, dh: 5000 }, view)).toEqual({ ...rect, w: 1500, h: 850 });
  });

  it("never holds back shrinking, even past the screen", () => {
    const beyond = { x: -50, y: 0, w: 400, h: 500 };
    expect(resizedWithin(beyond, "w", { dw: -100, dh: 0 }, view)).toEqual({ ...beyond, x: 50, w: 300 });
  });
});

describe("readRect", () => {
  it("reads a stored rectangle and refuses anything else", () => {
    expect(readRect({ x: 1, y: 2, w: 3, h: 4 })).toEqual({ x: 1, y: 2, w: 3, h: 4 });
    expect(readRect({ x: 1, y: 2, w: "3", h: 4 })).toBeNull();
    expect(readRect(null)).toBeNull();
  });
});

describe("windowStyle", () => {
  it("hands the caller's style back unchanged without a gesture", () => {
    expect(windowStyle("width: 100%; height: 100%;", null, null, 1.25)).toBe("width: 100%; height: 100%;");
    expect(windowStyle("", null, null, 1)).toBe("");
  });

  it("puts the gesture's geometry after the caller's style, drawn times the scale", () => {
    expect(windowStyle("width: 100%", { x: 10, y: 20 }, { w: 300, h: 200 }, 2)).toBe(
      "width: 100%; left: 20px; top: 40px; width: 600px; height: 400px;",
    );
    expect(windowStyle("", null, { w: 300, h: 200 }, 1)).toBe("width: 300px; height: 200px;");
  });
});
