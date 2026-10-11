import { get } from "svelte/store";
import { describe, expect, it } from "vitest";

import { animates, effectiveMotion, isMotionLevel, motion, motionSetting, systemReducesMotion, turns } from "./motion";

describe("effectiveMotion", () => {
  it("keeps the setting while the system does not ask for less", () => {
    expect(effectiveMotion("full", false)).toBe("full");
    expect(effectiveMotion("calm", false)).toBe("calm");
  });

  it("turns everything off when the system reduces motion", () => {
    expect(effectiveMotion("full", true)).toBe("off");
    expect(effectiveMotion("calm", true)).toBe("off");
  });
});

describe("motion levels", () => {
  it("lets the graph turn unless motion is off", () => {
    expect(turns("full")).toBe(true);
    expect(turns("calm")).toBe(true);
    expect(turns("off")).toBe(false);
  });

  it("animates the glow only at full motion", () => {
    expect(animates("full")).toBe(true);
    expect(animates("calm")).toBe(false);
    expect(animates("off")).toBe(false);
  });

  it("accepts only the three stored ids", () => {
    expect(isMotionLevel("calm")).toBe(true);
    expect(isMotionLevel("fast")).toBe(false);
    expect(isMotionLevel(undefined)).toBe(false);
  });

  it("derives the level in effect from the setting and the system", () => {
    motionSetting.set("calm");
    systemReducesMotion.set(false);
    expect(get(motion)).toBe("calm");
    systemReducesMotion.set(true);
    expect(get(motion)).toBe("off");
    systemReducesMotion.set(false);
    motionSetting.set("full");
  });
});
