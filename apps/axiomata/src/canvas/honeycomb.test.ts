import { describe, expect, it } from "vitest";

import { hexPoints, honeycomb } from "./honeycomb";

describe("honeycomb", () => {
  it("has 1 + 3·n·(n+1) cells, the centre first and the rings in order", () => {
    const comb = honeycomb(3);
    expect(comb).toHaveLength(37);
    expect(comb[0]).toEqual({ x: 0, y: 0, ring: 0 });
    expect(comb.map((c) => c.ring)).toEqual([...comb.map((c) => c.ring)].sort((a, b) => a - b));
  });

  it("puts neighbours one cell width apart", () => {
    const ring1 = honeycomb(1).filter((c) => c.ring === 1);
    for (const c of ring1) expect(Math.hypot(c.x, c.y)).toBeCloseTo(Math.sqrt(3));
  });
});

describe("hexPoints", () => {
  it("starts at the top corner of a pointy-top hexagon", () => {
    expect(hexPoints(0, 0, 10).split(" ")[0]).toBe("0.00,-10.00");
  });
});
