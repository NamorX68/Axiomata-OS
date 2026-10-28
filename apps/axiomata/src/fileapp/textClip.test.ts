import { describe, expect, it } from "vitest";

import { textClipPath } from "./textClip";

describe("textClipPath", () => {
  it("keeps the gap between gutter and text when nothing is scrolled", () => {
    // The text starts one cell (8 px) after a 40 px gutter: the line tint in that gap stays visible.
    expect(textClipPath(0, 40, 48)).toBe("inset(0 -100vw -100vh -8px)");
  });

  it("cuts what scrolled under the gutter", () => {
    expect(textClipPath(100, 40, 48)).toBe("inset(0 -100vw -100vh 92px)");
  });
});
