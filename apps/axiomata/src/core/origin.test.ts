import { describe, expect, it } from "vitest";

import { ORIGIN_WINDOW_MS, originAt } from "./origin";

describe("originAt", () => {
  it("is the middle of the screen when nothing was pressed yet", () => {
    expect(originAt(null, 1000)).toBeNull();
  });

  it("is the point just clicked", () => {
    expect(originAt({ point: { x: 10, y: 20 }, at: 1000 }, 1200)).toEqual({ x: 10, y: 20 });
  });

  it("forgets a click that is too old to have opened the panel", () => {
    expect(originAt({ point: { x: 10, y: 20 }, at: 1000 }, 1000 + ORIGIN_WINDOW_MS + 1)).toBeNull();
  });

  it("is the middle of the screen when a key came after the click", () => {
    expect(originAt({ point: null, at: 1000 }, 1100)).toBeNull();
  });
});
