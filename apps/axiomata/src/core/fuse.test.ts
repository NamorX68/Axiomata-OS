import { describe, expect, it } from "vitest";

import { DEFAULT_FUSE_MS, fuseProgress } from "./fuse";

describe("fuseProgress", () => {
  it("burns from the last firing to the next", () => {
    expect(fuseProgress(0, 100, 25)).toBeCloseTo(0.25);
  });

  it("is a one-hour fuse for a routine that never fired", () => {
    expect(fuseProgress(null, DEFAULT_FUSE_MS * 2, DEFAULT_FUSE_MS * 1.5)).toBeCloseTo(0.5);
  });

  it("stays whole until the hour before a far-off first firing, and burnt once the time is past", () => {
    expect(fuseProgress(null, DEFAULT_FUSE_MS * 10, 0)).toBe(0);
    expect(fuseProgress(0, 100, 150)).toBe(1);
  });

  it("ignores a last firing that is not before the next", () => {
    expect(fuseProgress(200, 100, 100 - DEFAULT_FUSE_MS / 2)).toBeCloseTo(0.5);
  });
});
