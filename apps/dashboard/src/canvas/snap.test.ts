import { describe, expect, it } from "vitest";

import {
  alignmentGuides,
  anchorFor,
  clampToBounds,
  displayRect,
  magnetMove,
  magnetResize,
  resolveOverlap,
  snapToGrid,
} from "./snap";

const A = { x: 100, y: 100, w: 200, h: 100 };

describe("snapToGrid", () => {
  it("rounds to the grid", () => {
    expect(snapToGrid(23)).toBe(16);
    expect(snapToGrid(25)).toBe(32);
    expect(Object.is(snapToGrid(-3), 0)).toBe(true);
  });
});

describe("magnetMove", () => {
  it("falls back to the grid without neighbours", () => {
    expect(magnetMove({ x: 37, y: 41, w: 100, h: 50 }, [])).toMatchObject({ x: 32, y: 48, guides: [] });
  });

  it("sticks to a neighbour's right edge within 8 px, not at 9 px", () => {
    const near = magnetMove({ x: 307, y: 100, w: 100, h: 50 }, [A]);
    expect(near.x).toBe(300);
    expect(near.guides).toContainEqual({ axis: "x", at: 300 });
    const far = magnetMove({ x: 309, y: 100, w: 100, h: 50 }, [A]);
    expect(far.x).toBe(304); // grid
  });

  it("ignores neighbours that are far away on the other axis", () => {
    const r = magnetMove({ x: 307, y: 400, w: 100, h: 50 }, [A]);
    expect(r.x).toBe(304);
  });

  it("prefers touching over aligning on a tie and the nearer edge otherwise", () => {
    // Just below A (within 8 px): left/right edges align at delta 4 → x = 100.
    const r = magnetMove({ x: 104, y: 205, w: 200, h: 50 }, [A]);
    expect(r.x).toBe(100);
    expect(r.y).toBe(200); // top touches A's bottom
    const t = magnetMove({ x: 296, y: 100, w: 100, h: 50 }, [A]); // touch right edge delta 4
    expect(t.x).toBe(300);
  });

  it("can be limited to the grid", () => {
    expect(magnetMove({ x: 307, y: 100, w: 100, h: 50 }, [A], { edges: false }).x).toBe(304);
  });
});

describe("magnetResize", () => {
  it("clamps the moving edge to a neighbour and never below min", () => {
    const r = magnetResize({ x: 0, y: 100, w: 95, h: 50 }, [A], "e", { w: 40, h: 40 });
    expect(r.w).toBe(100);
    const tiny = magnetResize({ x: 0, y: 100, w: 10, h: 50 }, [], "se", { w: 40, h: 40 });
    expect(tiny).toMatchObject({ w: 40, h: 48 });
  });
});

describe("alignmentGuides", () => {
  // A = { x: 100, y: 100, w: 200, h: 100 } → x-edges 100 / 200 / 300.
  it("emits an edge guide when a far-away tile's left edges line up", () => {
    const g = alignmentGuides({ x: 100, y: 900, w: 300, h: 200 }, [A]);
    expect(g).toContainEqual({ axis: "x", at: 100 });
  });

  it("emits a centre guide when centres align, within tolerance", () => {
    // A centre x = 200; moved centre x = 200 (x 160 + w/2 80). No edge match.
    const g = alignmentGuides({ x: 160, y: 0, w: 80, h: 40 }, [A]);
    expect(g).toEqual([{ axis: "x", at: 200 }]);
  });

  it("stays silent when nothing lines up", () => {
    expect(alignmentGuides({ x: 217, y: 913, w: 111, h: 77 }, [A])).toEqual([]);
  });

  it("dedupes a coincident guide contributed by several tiles", () => {
    const B = { x: 100, y: 500, w: 60, h: 60 };
    const g = alignmentGuides({ x: 100, y: 900, w: 40, h: 40 }, [A, B]);
    expect(g).toEqual([{ axis: "x", at: 100 }]);
  });
});

describe("resolveOverlap", () => {
  it("pushes the dropped tile out by the smallest move", () => {
    const r = resolveOverlap({ x: 250, y: 120, w: 100, h: 50 }, [A]);
    expect(r).toEqual({ x: 300, y: 120, w: 100, h: 50 });
  });

  it("uses the tie order right → down → left → up", () => {
    const r = resolveOverlap({ x: 150, y: 125, w: 100, h: 50 }, [A]); // right = 150, down = 75 → down wins by distance
    expect(r.y).toBe(200);
    const sq = resolveOverlap({ x: 200, y: 150, w: 100, h: 50 }, [A]); // right 100, down 50 → down
    expect(sq).toEqual({ x: 200, y: 200, w: 100, h: 50 });
  });

  it("chains through two neighbours and respects bounds", () => {
    const B = { x: 300, y: 100, w: 100, h: 100 };
    const r = resolveOverlap({ x: 250, y: 120, w: 100, h: 50 }, [A, B], { w: 500, h: 400 });
    expect([A, B].some((o) => r.x < o.x + o.w && r.x + r.w > o.x && r.y < o.y + o.h && r.y + r.h > o.y)).toBe(false);
    expect(r.x + r.w).toBeLessThanOrEqual(500);
  });

  it("returns the input when nothing overlaps", () => {
    expect(resolveOverlap({ x: 0, y: 0, w: 50, h: 50 }, [A])).toEqual({ x: 0, y: 0, w: 50, h: 50 });
  });
});

describe("clampToBounds / anchors / displayRect", () => {
  it("clamps into the canvas and shrinks only oversized tiles", () => {
    expect(clampToBounds({ x: 900, y: 700, w: 200, h: 100 }, { w: 1000, h: 800 }, { w: 100, h: 50 })).toEqual({ x: 800, y: 700, w: 200, h: 100 });
    expect(clampToBounds({ x: 0, y: 0, w: 1200, h: 100 }, { w: 1000, h: 800 }, { w: 100, h: 50 })).toEqual({ x: 0, y: 0, w: 1000, h: 100 });
  });

  it("anchors to the nearer edges", () => {
    expect(anchorFor({ x: 48, y: 48, w: 200, h: 100 }, { w: 1000, h: 800 })).toEqual({ x: "left", y: "top", w: 1000, h: 800 });
    expect(anchorFor({ x: 700, y: 600, w: 200, h: 100 }, { w: 1000, h: 800 })).toEqual({ x: "right", y: "bottom", w: 1000, h: 800 });
  });

  it("right-anchored tiles track the edge; left ones stay; all stay visible; growing restores", () => {
    const min = { w: 100, h: 50 };
    const right = { x: 700, y: 48, w: 200, h: 100 };
    const anchor = anchorFor(right, { w: 1000, h: 800 });
    expect(displayRect(right, anchor, { w: 1000, h: 800 }, min)).toEqual(right);
    expect(displayRect(right, anchor, { w: 1400, h: 800 }, min).x).toBe(1100); // +400 with the edge
    expect(displayRect(right, anchor, { w: 800, h: 800 }, min).x).toBe(500); // −200 with the edge
    const left = { x: 48, y: 48, w: 360, h: 150 };
    const la = anchorFor(left, { w: 1000, h: 800 });
    expect(displayRect(left, la, { w: 1400, h: 800 }, min)).toEqual(left);
    expect(displayRect(left, la, { w: 300, h: 800 }, min)).toEqual({ x: 0, y: 48, w: 300, h: 150 }); // clamped, shrunk
    expect(displayRect(left, la, { w: 1000, h: 800 }, min)).toEqual(left); // back to normal
    expect(displayRect(left, undefined, { w: 0, h: 0 }, min)).toEqual(left);
  });
});
