import { describe, expect, it } from "vitest";

import { DEFAULT_BRAIN_VIEW, readBrainView, spinOf } from "./brainView";

describe("readBrainView", () => {
  it("starts with the defaults when nothing was stored", () => {
    expect(readBrainView(undefined)).toEqual(DEFAULT_BRAIN_VIEW);
  });

  it("takes the stored setting as it is", () => {
    const stored = { motion: false, speed: 0.06, labels: false, fileNames: true };
    expect(readBrainView(stored)).toEqual(stored);
  });

  it("takes over the old Second Brain preferences: their slider and file names", () => {
    expect(readBrainView(undefined, { spin: 0.08, fileNames: true })).toEqual({
      motion: true,
      speed: 0.08,
      labels: true,
      fileNames: true,
    });
  });

  it("reads a rotation of 0 in the old preferences as the motion being off, keeping the default speed", () => {
    expect(readBrainView(undefined, { spin: 0 })).toMatchObject({ motion: false, speed: DEFAULT_BRAIN_VIEW.speed });
  });

  it("ignores values of the wrong type or out of range", () => {
    expect(readBrainView({ motion: "yes", speed: 5, labels: 1, fileNames: null })).toEqual(DEFAULT_BRAIN_VIEW);
  });
});

describe("spinOf", () => {
  it("is the speed while the motion is on and 0 when it is off", () => {
    expect(spinOf({ ...DEFAULT_BRAIN_VIEW, speed: 0.05 })).toBe(0.05);
    expect(spinOf({ ...DEFAULT_BRAIN_VIEW, speed: 0.05, motion: false })).toBe(0);
  });
});
