import { describe, expect, it } from "vitest";

import { DEFAULT_EFFECTS, readEffects } from "./effects";

describe("readEffects", () => {
  it("takes the defaults for anything missing or not a boolean", () => {
    expect(readEffects(undefined)).toEqual(DEFAULT_EFFECTS);
    expect(readEffects({ pointerLight: false, dayLight: "no", other: 1 })).toEqual({
      pointerLight: false,
      dayLight: true,
      ignition: true,
    });
    expect(readEffects({ ignition: false }).ignition).toBe(false);
  });
});
