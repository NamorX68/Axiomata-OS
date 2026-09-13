import { describe, expect, it } from "vitest";

import { appNodeRadiusPx, rimNodeRadiusPx } from "./render";

describe("appNodeRadiusPx", () => {
  it("clamps to [12, 20] px", () => {
    expect(appNodeRadiusPx(10)).toBe(12);
    expect(appNodeRadiusPx(10_000)).toBe(20);
    expect(appNodeRadiusPx(250)).toBeCloseTo(15, 0);
  });
});

describe("rimNodeRadiusPx", () => {
  it("matches the plain size cap when few nodes share the ring", () => {
    // Plenty of arc length available — the size-based cap binds, same as
    // before this function existed.
    expect(rimNodeRadiusPx(300, 5)).toBe(Math.max(14, Math.min(24, 300 * 0.072)));
  });

  it("shrinks below the size cap once enough nodes would otherwise overlap", () => {
    const R = 288;
    const sizeCap = Math.max(14, Math.min(24, R * 0.072));
    const r = rimNodeRadiusPx(R, 60); // ORBIT_MAX-scale crowding
    expect(r).toBeLessThan(sizeCap);
    // Diameters must fit within the arc length each node actually gets,
    // with room to spare — this is the overlap this function exists to
    // prevent.
    const arcLength = (2 * Math.PI * R) / 60;
    expect(r * 2).toBeLessThan(arcLength);
  });

  it("never returns something implausibly small even at extreme crowding", () => {
    expect(rimNodeRadiusPx(300, 500)).toBeGreaterThanOrEqual(6);
  });

  it("falls back to the size cap for zero nodes (no division by zero)", () => {
    expect(rimNodeRadiusPx(300, 0)).toBe(Math.max(14, Math.min(24, 300 * 0.072)));
  });
});
