import { describe, expect, it } from "vitest";

import {
  branchGrowth,
  countBornBy,
  flash,
  GOLDEN_ANGLE,
  GROWTH_MS,
  growthMoment,
  layoutGrowth,
  sprout,
  sunflower,
  type GrowthFile,
} from "./growth";

const DAY = 86_400_000;

describe("growth timing", () => {
  it("maps the replay's fixed duration onto the vault's whole life, however old it is", () => {
    for (const days of [3, 400, 4000]) {
      const span = { from: 1_000_000, to: 1_000_000 + days * DAY };
      expect(growthMoment(0, span)).toBe(span.from);
      expect(growthMoment(GROWTH_MS / 2, span)).toBe(span.from + (days * DAY) / 2);
      expect(growthMoment(GROWTH_MS, span)).toBe(span.to);
      // Past either end it stays at the end.
      expect(growthMoment(-5, span)).toBe(span.from);
      expect(growthMoment(GROWTH_MS * 2, span)).toBe(span.to);
    }
  });

  it("lets a note sprout from a quarter of its size at its birth to full size a moment later", () => {
    const span = { from: 0, to: 1000 * DAY };
    const born = 500 * DAY;
    expect(sprout(born - 1, born, span)).toBe(0);
    expect(sprout(born, born, span)).toBeCloseTo(0.25, 9);
    expect(sprout(born + 100 * DAY, born, span)).toBe(1);
    // A note without a time is there from the start.
    expect(sprout(span.from, null, span)).toBe(1);
  });

  it("flashes a new link at its birth and lets it fade, and grows a branch with its area's first note", () => {
    const span = { from: 0, to: 1000 * DAY };
    expect(flash(100 * DAY, 100 * DAY, span)).toBe(1);
    expect(flash(99 * DAY, 100 * DAY, span)).toBe(0);
    expect(flash(120 * DAY, 100 * DAY, span)).toBeGreaterThan(0);
    expect(flash(200 * DAY, 100 * DAY, span)).toBe(0);
    expect(branchGrowth(0, 10 * DAY, span)).toBe(0);
    expect(branchGrowth(10 * DAY, 10 * DAY, span)).toBe(0);
    expect(branchGrowth(40 * DAY, 10 * DAY, span)).toBeCloseTo(0.6, 9);
    expect(branchGrowth(0, null, span)).toBe(1);
  });

  it("counts the notes born by a moment", () => {
    const sorted = [1, 2, 2, 5, 9];
    expect(countBornBy(sorted, 0)).toBe(0);
    expect(countBornBy(sorted, 2)).toBe(3);
    expect(countBornBy(sorted, 9)).toBe(5);
    expect(countBornBy([], 9)).toBe(0);
  });
});

describe("growth positions", () => {
  it("puts the k-th note of a sunflower spiral at spacing·√(k + ½), turned by the golden angle", () => {
    const p = sunflower(3, 0.1);
    expect(Math.hypot(p.x, p.y)).toBeCloseTo(0.1 * Math.sqrt(3.5), 12);
    expect(Math.atan2(p.y, p.x)).toBeCloseTo(Math.atan2(Math.sin(3 * GOLDEN_ANGLE), Math.cos(3 * GOLDEN_ANGLE)), 12);
  });

  function files(): GrowthFile[] {
    const out: GrowthFile[] = [];
    for (const [area, n] of [
      ["Dev", 40],
      ["Work", 12],
      ["Home", 5],
    ] as const) {
      // Born out of order, so the layout has to sort them.
      for (let i = 0; i < n; i++) out.push({ id: `${area}/${i}`, area, born: ((i * 7) % n) * DAY });
    }
    out.push({ id: "root", area: null, born: null });
    return out;
  }

  it("lays each area's notes on a spiral around its branch, the oldest at its heart", () => {
    const layout = layoutGrowth(files(), ["Dev", "Work", "Home"]);
    expect(layout.branches.map((b) => b.area)).toEqual(["Dev", "Work", "Home", null]);
    const dev = layout.branches[0]!;
    const mine = [...layout.cells.values()].filter((c) => c.area === "Dev").sort((a, b) => a.rank - b.rank);
    const distance = (c: { x: number; y: number }) => Math.hypot(c.x - dev.x, c.y - dev.y);
    // Further out the higher the rank, and the rank follows the birth.
    for (let i = 1; i < mine.length; i++) {
      expect(distance(mine[i]!)).toBeGreaterThan(distance(mine[i - 1]!));
      expect(mine[i]!.born!).toBeGreaterThanOrEqual(mine[i - 1]!.born!);
    }
    expect(dev.first).toBe(0);
    expect(dev.count).toBe(40);
    // The root's note has no time: its branch is there from the start.
    expect(layout.branches[3]!.first).toBeNull();
  });

  it("keeps every cell inside the lens and the spirals of neighbouring areas apart", () => {
    const layout = layoutGrowth(files(), ["Dev", "Work", "Home"]);
    const cells = [...layout.cells.values()];
    for (const c of cells) expect(Math.hypot(c.x, c.y) + layout.unit).toBeLessThan(1);
    for (const a of cells) {
      for (const b of cells) {
        if (a === b) continue;
        expect(Math.hypot(a.x - b.x, a.y - b.y)).toBeGreaterThan(layout.unit);
      }
    }
  });

  it("grows a vault of one area from the core itself", () => {
    const layout = layoutGrowth([{ id: "a", area: "Only", born: 0 }], ["Only"]);
    expect(layout.branches[0]).toMatchObject({ x: 0, y: 0 });
  });
});
