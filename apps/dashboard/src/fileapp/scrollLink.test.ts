import { describe, expect, it } from "vitest";

import { ScrollLink } from "./scrollLink";

describe("ScrollLink (H12)", () => {
  it("lets the leader lead and swallows the follower's echo", () => {
    let t = 0;
    const link = new ScrollLink(() => t, 100);
    expect(link.scrolled("left")).toBe(true);
    t = 10;
    expect(link.scrolled("right")).toBe(false); // the echo of following
    t = 50;
    expect(link.scrolled("left")).toBe(true); // still scrolling: the lead is extended
    t = 120;
    expect(link.scrolled("right")).toBe(false);
  });

  it("hands the lead over once the leader has rested", () => {
    let t = 0;
    const link = new ScrollLink(() => t, 100);
    link.scrolled("left");
    t = 101;
    expect(link.scrolled("right")).toBe(true);
    t = 110;
    expect(link.scrolled("left")).toBe(false);
  });
});
