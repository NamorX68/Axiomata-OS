import { describe, expect, it } from "vitest";

import { buildHive, emberOf, fibonacciSphere, metalLight, MIN_CELLS, mixRgb, travelSpan, type HiveFile } from "./hive";

const files = (area: string, n: number): HiveFile[] =>
  Array.from({ length: n }, (_, i) => ({ id: `${area}/note-${i}.md`, area }));

describe("fibonacciSphere", () => {
  it("puts every point on the unit sphere", () => {
    for (const p of fibonacciSphere(50)) expect(Math.hypot(...p)).toBeCloseTo(1);
  });
});

describe("buildHive", () => {
  it("has spare, unsettled cells and gives every note exactly one", () => {
    const vault = [...files("Work", 60), ...files("Mail", 40)];
    const hive = buildHive(vault);
    expect(hive.length).toBeGreaterThan(vault.length);
    const settled = hive.filter((c) => c.fileId !== null).map((c) => c.fileId);
    expect(new Set(settled).size).toBe(vault.length);
  });

  it("keeps a young vault a whole ball", () => {
    expect(buildHive(files("A", 3))).toHaveLength(MIN_CELLS);
  });

  it("is the same ball for the same files, whatever their order", () => {
    const vault = [...files("Work", 20), ...files("Mail", 10)];
    const a = buildHive(vault).map((c) => c.fileId);
    const b = buildHive([...vault].reverse()).map((c) => c.fileId);
    expect(a).toEqual(b);
  });

  it("lets an area grow as a landmass: its cells lie closer to each other than to another area's", () => {
    const hive = buildHive([...files("Work", 40), ...files("Mail", 40)]);
    const centre = (area: string) => {
      const cells = hive.filter((c) => c.fileId?.startsWith(area));
      return cells.reduce((s, c) => [s[0] + c.p[0], s[1] + c.p[1], s[2] + c.p[2]], [0, 0, 0]);
    };
    const [w, m] = [centre("Work"), centre("Mail")];
    const dot = (w[0] * m[0] + w[1] * m[1] + w[2] * m[2]) / (Math.hypot(...w) * Math.hypot(...m));
    expect(dot).toBeLessThan(0.5);
  });
});

describe("emberOf (W2)", () => {
  const now = Date.UTC(2026, 9, 12, 12);
  it("glows for what was touched today and fades over the day", () => {
    expect(emberOf(now, now).heat).toBeCloseTo(0.85);
    expect(emberOf(now - 12 * 3_600_000, now).heat).toBeCloseTo(0.425);
    expect(emberOf(now - 2 * 86_400_000, now).heat).toBe(0);
  });

  it("cools to ash over weeks, and half for a note without a time", () => {
    expect(emberOf(now - 40 * 86_400_000, now).ash).toBeCloseTo(0.85);
    expect(emberOf(null, now)).toEqual({ heat: 0, ash: 0.5 });
  });
});

describe("travelSpan (G17)", () => {
  const now = Date.UTC(2026, 9, 12);
  it("runs from the earliest note to now, ignoring missing dates", () => {
    const first = "2025-01-02T00:00:00Z";
    expect(travelSpan([null, "2026-01-01T00:00:00Z", first, undefined], now)).toEqual({
      from: Date.parse(first),
      to: now,
    });
  });

  it("spans a day for a vault without dates", () => {
    expect(travelSpan([null], now)).toEqual({ from: now - 86_400_000, to: now });
  });
});

describe("mixRgb and metalLight", () => {
  it("mixes two colours, clamped", () => {
    expect(mixRgb([0, 0, 0], [100, 200, 50], 0.5)).toEqual([50, 100, 25]);
    expect(mixRgb([0, 0, 0], [100, 200, 50], 2)).toEqual([100, 200, 50]);
  });

  it("lights a plate facing the upper left brighter than one facing away", () => {
    const lit = metalLight([-0.45, -0.55, 0.7], 1);
    const away = metalLight([0.45, 0.55, -0.7], 1);
    expect(lit.shade).toBeGreaterThan(away.shade);
    expect(away.spec).toBe(0);
  });
});
