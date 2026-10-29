import { describe, expect, it } from "vitest";

import {
  basisCommands,
  BUNDLE_ROOT,
  bundleControls,
  dirKey,
  isFreshLink,
  linkBorn,
  straighten,
  type BundleLayout,
  type BundlePoint,
} from "./bundle";

const DAY = 86_400_000;

describe("bundle curves", () => {
  const points: BundlePoint[] = [
    { x: 0.6, y: 0 },
    { x: 0.3, y: 0.1 },
    { x: 0, y: 0 },
    { x: -0.3, y: 0.1 },
    { x: -0.6, y: 0 },
  ];

  it("straighten keeps the points at β 1, lays them on the line at β 0, and never moves the ends", () => {
    expect(straighten(points, 1)).toEqual(points);
    for (const p of straighten(points, 0)) expect(p.y).toBeCloseTo(0, 12);
    const half = straighten(points, 0.85);
    expect(half[0]).toEqual(points[0]);
    expect(half[4]).toEqual(points[4]);
    expect(half[1]!.y).toBeCloseTo(0.085, 12);
  });

  it("the B-spline starts on the first point, ends on the last and stays inside the control points' hull", () => {
    const cmds = basisCommands(points);
    expect(cmds[0]).toEqual({ kind: "move", x: 0.6, y: 0 });
    expect(cmds[cmds.length - 1]).toEqual({ kind: "line", x: -0.6, y: 0 });
    expect(cmds.filter((c) => c.kind === "curve")).toHaveLength(points.length - 1);
    for (const c of cmds) {
      if (c.kind !== "curve") continue;
      expect(c.y).toBeGreaterThanOrEqual(0);
      expect(c.y).toBeLessThanOrEqual(0.1);
    }
  });

  it("two points are a straight line, none no curve at all", () => {
    expect(basisCommands(points.slice(0, 2))).toEqual([
      { kind: "move", x: 0.6, y: 0 },
      { kind: "line", x: 0.3, y: 0.1 },
    ]);
    expect(basisCommands([])).toEqual([]);
  });

  it("finds no curve for an unknown note and does not loop on a parent map that runs in a circle", () => {
    const p = (x: number) => ({ x, y: 0 });
    const layout: BundleLayout = {
      arcs: [],
      points: new Map([
        ["a", p(1)],
        ["b", p(-1)],
        [dirKey("x"), p(0.5)],
        [dirKey("y"), p(-0.5)],
        [BUNDLE_ROOT, p(0)],
      ]),
      parent: new Map([
        ["a", dirKey("x")],
        ["b", dirKey("y")],
        [dirKey("x"), dirKey("y")],
        [dirKey("y"), dirKey("x")],
      ]),
    };
    expect(bundleControls(layout, "a", "missing")).toEqual([]);
    const path = bundleControls(layout, "a", "b");
    expect(path[0]).toEqual(p(1));
    expect(path[path.length - 1]).toEqual(p(-1));
  });

  it("a link is as old as the younger of its notes, and new for three weeks", () => {
    expect(linkBorn(10, 20)).toBe(20);
    expect(linkBorn(null, 20)).toBe(20);
    expect(linkBorn(10, null)).toBe(10);
    expect(linkBorn(null, null)).toBeNull();
    const now = 100 * DAY;
    expect(isFreshLink(now - 20 * DAY, now)).toBe(true);
    expect(isFreshLink(now - 22 * DAY, now)).toBe(false);
    // Not yet born at the moment shown (the time travel): not new either.
    expect(isFreshLink(now + DAY, now)).toBe(false);
    expect(isFreshLink(null, now)).toBe(false);
  });
});
