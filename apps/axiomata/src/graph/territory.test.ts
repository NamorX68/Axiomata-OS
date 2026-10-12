import { describe, expect, it } from "vitest";

import { axialToPixel, buildTerritory, cellKey, NEIGHBOURS, pixelToAxial, type TerritoryNote } from "./territory";

const DAY = 86_400_000;
const notes = (area: string | null, n: number, from = 0): TerritoryNote[] =>
  Array.from({ length: n }, (_, i) => ({ id: `${area ?? "loose"}/note-${i}.md`, area, born: from + i * DAY }));

/** How many pieces a country falls into: its fields grouped by touching neighbours. */
function pieces(cells: { q: number; r: number }[]): number {
  const left = new Set(cells.map((c) => cellKey(c.q, c.r)));
  let count = 0;
  while (left.size > 0) {
    count++;
    const [first] = left;
    const queue = [first!];
    left.delete(first!);
    while (queue.length > 0) {
      const [q, r] = queue.pop()!.split(",").map(Number) as [number, number];
      for (const { dq, dr } of NEIGHBOURS) {
        const key = cellKey(q + dq, r + dr);
        if (left.delete(key)) queue.push(key);
      }
    }
  }
  return count;
}

describe("pixelToAxial", () => {
  it("finds the field a point lies in, the inverse of axialToPixel at its centre", () => {
    for (const [q, r] of [
      [0, 0],
      [3, -1],
      [-2, 5],
    ] as const) {
      const [x, y] = axialToPixel(q, r);
      expect(pixelToAxial(x + 0.2, y - 0.1)).toEqual([q, r]);
    }
  });
});

describe("the grid", () => {
  it("names each neighbour by the direction its shared edge faces", () => {
    for (const { dq, dr, angle } of NEIGHBOURS) {
      const [x, y] = axialToPixel(dq, dr);
      expect((Math.atan2(y, x) * 180) / Math.PI).toBeCloseTo(angle === 180 ? 180 : angle, 6);
    }
  });

  it("finds a field up to its edge and the neighbour just beyond, also left of and above the origin", () => {
    const inradius = Math.sqrt(3) / 2;
    for (const [q, r] of [
      [0, 0],
      [-4, -3],
    ] as const) {
      const [cx, cy] = axialToPixel(q, r);
      for (const { dq, dr, angle } of NEIGHBOURS) {
        const a = (angle * Math.PI) / 180;
        expect(pixelToAxial(cx + Math.cos(a) * inradius * 0.95, cy + Math.sin(a) * inradius * 0.95)).toEqual([q, r]);
        expect(pixelToAxial(cx + Math.cos(a) * inradius * 1.05, cy + Math.sin(a) * inradius * 1.05)).toEqual([
          q + dq,
          r + dr,
        ]);
      }
    }
  });
});

describe("buildTerritory", () => {
  const vault = [...notes("Dev", 60), ...notes("Life", 35), ...notes("Mail", 25), ...notes(null, 4)];
  const map = buildTerritory(vault);

  it("gives every note a field of its own", () => {
    expect(map).toHaveLength(vault.length);
    expect(new Set(map.map((c) => c.id)).size).toBe(vault.length);
    expect(new Set(map.map((c) => cellKey(c.q, c.r))).size).toBe(vault.length);
  });

  it("grows each area as one country, not scattered fields", () => {
    const areaOf = new Map(vault.map((n) => [n.id, n.area]));
    for (const area of ["Dev", "Life", "Mail"]) {
      expect(pieces(map.filter((c) => areaOf.get(c.id) === area))).toBe(1);
    }
  });

  it("puts a country's oldest note at its heart and the youngest on its edge", () => {
    const dev = map.filter((c) => c.id.startsWith("Dev/"));
    const centre = dev
      .map((c) => axialToPixel(c.q, c.r))
      .reduce(([sx, sy], [x, y]) => [sx + x / dev.length, sy + y / dev.length], [0, 0]);
    const dist = (id: string) => {
      const c = dev.find((d) => d.id === id)!;
      const [x, y] = axialToPixel(c.q, c.r);
      return Math.hypot(x - centre[0], y - centre[1]);
    };
    expect(dist("Dev/note-0.md")).toBeLessThan(dist("Dev/note-59.md"));
  });

  it("is the same map for the same notes, whatever order they come in", () => {
    const again = buildTerritory([...vault].reverse());
    const at = (cells: typeof map) => new Map(cells.map((c) => [c.id, cellKey(c.q, c.r)]));
    expect(at(again)).toEqual(at(map));
  });

  it("is empty for an empty vault", () => {
    expect(buildTerritory([])).toEqual([]);
  });

  it("maps a vault of one area, or of loose notes only, as one country", () => {
    for (const only of [notes("Dev", 30), notes(null, 12)]) {
      const map = buildTerritory(only);
      expect(map).toHaveLength(only.length);
      expect(pieces(map)).toBe(1);
    }
  });

  it("keeps every field its own with very unequal countries, undated notes going last", () => {
    const undated: TerritoryNote[] = [{ id: "Tiny/undated.md", area: "Tiny", born: null }];
    const vault = [...notes("Big", 300), ...notes("Tiny", 1, 5 * DAY), ...undated, ...notes("Pair", 2)];
    const map = buildTerritory(vault);
    expect(new Set(map.map((c) => cellKey(c.q, c.r))).size).toBe(vault.length);
    // Within a country the undated note comes after the dated ones: it is placed later, further out.
    const tiny = map.filter((c) => c.id.startsWith("Tiny/"));
    expect(tiny.map((c) => c.id)).toEqual(["Tiny/note-0.md", "Tiny/undated.md"]);
  });
});
