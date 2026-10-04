import { describe, expect, it } from "vitest";

import { rectHull } from "./cursorGlide";

describe("rectHull — the smear's shape (K8)", () => {
  it("is one box when both are in the same place", () => {
    const hull = rectHull({ x: 0, y: 0 }, { x: 0, y: 0 }, 2, 10);
    expect(new Set(hull.map((p) => `${p.x},${p.y}`))).toEqual(new Set(["0,0", "2,0", "2,10", "0,10"]));
  });

  it("is the band a box sweeps out on a line, and a six-sided shape on a diagonal", () => {
    const along = rectHull({ x: 0, y: 0 }, { x: 100, y: 0 }, 2, 10);
    expect(new Set(along.map((p) => `${p.x},${p.y}`))).toEqual(new Set(["0,0", "102,0", "102,10", "0,10"]));
    const diagonal = rectHull({ x: 0, y: 0 }, { x: 100, y: 50 }, 2, 10);
    expect(diagonal).toHaveLength(6);
    expect(diagonal).toContainEqual({ x: 0, y: 0 });
    expect(diagonal).toContainEqual({ x: 102, y: 60 });
  });
});
