import { describe, expect, it } from "vitest";

import { DEFAULT_BRAIN_VIEW, legacyMotionLevel, readBrainView, spinOf } from "./brainView";

describe("readBrainView", () => {
  it("starts with the defaults when nothing was stored", () => {
    expect(readBrainView(undefined)).toEqual(DEFAULT_BRAIN_VIEW);
  });

  it("takes the stored setting as it is, dropping the old motion switch", () => {
    const stored = { motion: false, speed: 0.06, labels: false, fileNames: true, shine: "twinkle" };
    expect(readBrainView(stored)).toEqual({ speed: 0.06, labels: false, fileNames: true, shine: "twinkle" });
  });

  it("takes over the old Second Brain preferences: their slider and file names", () => {
    expect(readBrainView(undefined, { spin: 0.08, fileNames: true })).toEqual({
      speed: 0.08,
      labels: true,
      fileNames: true,
      shine: "metal",
    });
  });

  it("lights the cells like metal unless another shimmer was chosen (G7)", () => {
    expect(readBrainView(undefined).shine).toBe("metal");
    expect(readBrainView({ shine: "glitter" }).shine).toBe("metal");
    expect(readBrainView({ shine: "calm" }).shine).toBe("calm");
  });

  it("keeps the default speed when the old preferences had the rotation at 0", () => {
    expect(readBrainView(undefined, { spin: 0 })).toMatchObject({ speed: DEFAULT_BRAIN_VIEW.speed });
  });

  it("ignores values of the wrong type or out of range", () => {
    expect(readBrainView({ motion: "yes", speed: 5, labels: 1, fileNames: null })).toEqual(DEFAULT_BRAIN_VIEW);
  });
});

describe("legacyMotionLevel", () => {
  it("reads a switched-off graph as the app-wide motion being off", () => {
    expect(legacyMotionLevel({ motion: false })).toBe("off");
    expect(legacyMotionLevel(undefined, { spin: 0 })).toBe("off");
  });

  it("says nothing when the old switch was on or never stored", () => {
    expect(legacyMotionLevel({ motion: true })).toBeNull();
    expect(legacyMotionLevel(undefined)).toBeNull();
    expect(legacyMotionLevel({ motion: true }, { spin: 0 })).toBeNull();
  });
});

describe("spinOf", () => {
  it("is the speed while the graph may turn and 0 when motion is off", () => {
    expect(spinOf({ ...DEFAULT_BRAIN_VIEW, speed: 0.05 }, "full")).toBe(0.05);
    expect(spinOf({ ...DEFAULT_BRAIN_VIEW, speed: 0.05 }, "calm")).toBe(0.05);
    expect(spinOf({ ...DEFAULT_BRAIN_VIEW, speed: 0.05 }, "off")).toBe(0);
  });
});
