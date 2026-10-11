import { describe, expect, it } from "vitest";

import { travel } from "./fromOrigin";

const rect = { left: 100, top: 50, width: 200, height: 100 } as DOMRectReadOnly;
const viewport = { w: 1000, h: 800 };

describe("travel", () => {
  it("leads from the panel's centre to the point that was clicked", () => {
    expect(travel({ x: 10, y: 20 }, rect, viewport)).toEqual({ x: -190, y: -80 });
  });

  it("leads to the middle of the screen when nothing was clicked", () => {
    expect(travel(null, rect, viewport)).toEqual({ x: 300, y: 300 });
  });
});
