import { describe, expect, it } from "vitest";

import { LineStore } from "./buffer";
import { glideMotion, indentGuides, stepCursor } from "./decorations";

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

describe("stepCursor and glideMotion (K8)", () => {
  const strong = { durationMs: 100, tailLag: 2.4 };
  const at = (x: number, y: number) => ({ x, y, tail: { x, y } });

  it("brings the head and then the tail to the target, in about the glide's duration", () => {
    let motion = at(0, 0);
    let done = false;
    let elapsed = 0;
    let headAt = -1;
    while (!done && elapsed < 2000) {
      ({ motion, done } = stepCursor(motion, { x: 100, y: 20 }, 16, strong));
      elapsed += 16;
      if (headAt < 0 && Math.hypot(100 - motion.x, 20 - motion.y) < 1) headAt = elapsed;
    }
    expect(done).toBe(true);
    expect(motion).toEqual(at(100, 20));
    expect(headAt).toBeLessThan(300);
    // The tail lands later: that is the smear shrinking into the target.
    expect(elapsed).toBeGreaterThan(headAt);
  });

  it("smears only when the tail lags: behind the head, on the way", () => {
    const moving = stepCursor(at(0, 0), { x: 500, y: 0 }, 16, strong).motion;
    expect(moving.tail.x).toBeLessThan(moving.x);
    const subtle = stepCursor(at(0, 0), { x: 500, y: 0 }, 16, { durationMs: 100, tailLag: 1 }).motion;
    expect(subtle.tail).toEqual({ x: subtle.x, y: subtle.y });
  });

  it("does not move backwards for a zero or negative frame time", () => {
    const { motion } = stepCursor(at(10, 0), { x: 20, y: 0 }, -5, strong);
    expect(motion.x).toBe(10);
  });

  it("takes longer the farther the jump, strong more than subtle", () => {
    const step = glideMotion(8, "strong").durationMs;
    const far = glideMotion(900, "strong").durationMs;
    expect(step).toBeLessThan(100);
    expect(far).toBe(240);
    expect(glideMotion(300, "strong").durationMs).toBeGreaterThan(glideMotion(300, "subtle").durationMs);
    expect(glideMotion(300, "subtle").tailLag).toBe(1);
  });
});
