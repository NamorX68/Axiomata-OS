import { describe, expect, it } from "vitest";

import { dayLightAt } from "./daylight";

describe("dayLightAt", () => {
  it("rises warm on the left, stands high and neutral midday, sets warm on the right", () => {
    const morning = dayLightAt(6);
    const noon = dayLightAt(13.5);
    const evening = dayLightAt(20.9);
    expect(morning.x).toBeCloseTo(5);
    expect(morning.warmth).toBeCloseTo(1);
    expect(noon.y).toBeLessThan(morning.y);
    expect(noon.warmth).toBeCloseTo(0);
    expect(noon.strength).toBeCloseTo(1);
    expect(noon.night).toBe(false);
    expect(evening.x).toBeGreaterThan(90);
  });

  it("keeps only a faint cool trace at night, whatever the hour is written as", () => {
    expect(dayLightAt(2)).toEqual({ x: 50, y: -20, strength: 0.25, warmth: 0, night: true });
    expect(dayLightAt(26)).toEqual(dayLightAt(2));
  });
});
