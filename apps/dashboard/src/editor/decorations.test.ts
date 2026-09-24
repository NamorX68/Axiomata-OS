import { describe, expect, it } from "vitest";

import { LineStore } from "./buffer";
import { indentGuides, stepCursor } from "./decorations";

describe("indentGuides", () => {
  it("draws one guide per indentation level", () => {
    const store = new LineStore("fn a() {\n    if x {\n        y();\n    }\n}");
    const guides = indentGuides(store, 0, 4, 4, 4);
    expect(guides.has(0)).toBe(false);
    expect(guides.get(1)).toEqual([0]);
    expect(guides.get(2)).toEqual([0, 4]);
    expect(guides.get(3)).toEqual([0]);
  });

  it("carries the guides through a blank line inside a block", () => {
    const store = new LineStore("{\n    a\n\n    b\n}");
    expect(indentGuides(store, 2, 2, 4, 4).get(2)).toEqual([0]);
  });

  it("counts tabs by their width", () => {
    const store = new LineStore("a\n\t\tb");
    expect(indentGuides(store, 1, 1, 4, 4).get(1)).toEqual([0, 4]);
  });
});

describe("stepCursor", () => {
  const options = { durationMs: 100, trail: true };

  it("approaches the target and arrives in about the glide's duration", () => {
    let motion = { x: 0, y: 0, trail: [] as { x: number; y: number }[] };
    let done = false;
    let elapsed = 0;
    while (!done && elapsed < 1000) {
      ({ motion, done } = stepCursor(motion, { x: 100, y: 20 }, 16, options));
      elapsed += 16;
    }
    expect(done).toBe(true);
    expect(motion).toEqual({ x: 100, y: 20, trail: [] });
    expect(elapsed).toBeLessThan(300);
  });

  it("leaves a trail while moving, capped, and none without the option", () => {
    let motion = { x: 0, y: 0, trail: [] as { x: number; y: number }[] };
    for (let i = 0; i < 20; i++) ({ motion } = stepCursor(motion, { x: 1000, y: 0 }, 1, options));
    expect(motion.trail.length).toBe(8);
    expect(stepCursor({ x: 0, y: 0, trail: [] }, { x: 50, y: 0 }, 16, { durationMs: 100, trail: false }).motion.trail)
      .toEqual([]);
  });

  it("does not move backwards for a zero or negative frame time", () => {
    const { motion } = stepCursor({ x: 10, y: 0, trail: [] }, { x: 20, y: 0 }, -5, options);
    expect(motion.x).toBe(10);
  });
});
