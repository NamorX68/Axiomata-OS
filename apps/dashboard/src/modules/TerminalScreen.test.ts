import { describe, expect, it } from "vitest";

import { resolveColor, type TermColor } from "./TerminalScreen";

describe("resolveColor", () => {
  it("defers a default color to the caller's fallback", () => {
    expect(resolveColor({ type: "default" }, "#abcdef")).toBe("#abcdef");
  });

  it("resolves the low 8 ANSI indices to the standard xterm palette", () => {
    expect(resolveColor({ type: "indexed", index: 0 }, "#fff")).toBe("#000000");
    expect(resolveColor({ type: "indexed", index: 1 }, "#fff")).toBe("#cd0000");
    expect(resolveColor({ type: "indexed", index: 7 }, "#fff")).toBe("#e5e5e5");
  });

  it("resolves the bright 8 ANSI indices (8-15)", () => {
    expect(resolveColor({ type: "indexed", index: 8 }, "#fff")).toBe("#7f7f7f");
    expect(resolveColor({ type: "indexed", index: 15 }, "#fff")).toBe("#ffffff");
  });

  it("resolves the 256-color cube (16-231) via the standard xterm level table", () => {
    // Index 16 is the cube's own (0,0,0) corner - pure black, distinct from
    // ANSI index 0 despite both being "black" in different palettes.
    expect(resolveColor({ type: "indexed", index: 16 }, "#fff")).toBe("rgb(0, 0, 0)");
    // Index 231 is the cube's (5,5,5) corner - pure white.
    expect(resolveColor({ type: "indexed", index: 231 }, "#fff")).toBe("rgb(255, 255, 255)");
    // A mid-cube value: r=1,g=2,b=3 -> 16 + 36*1 + 6*2 + 3 = 67.
    expect(resolveColor({ type: "indexed", index: 67 }, "#fff")).toBe("rgb(95, 135, 175)");
  });

  it("resolves the greyscale ramp (232-255)", () => {
    expect(resolveColor({ type: "indexed", index: 232 }, "#fff")).toBe("rgb(8, 8, 8)");
    expect(resolveColor({ type: "indexed", index: 255 }, "#fff")).toBe("rgb(238, 238, 238)");
  });

  it("passes truecolor rgb values through directly", () => {
    const color: TermColor = { type: "rgb", r: 10, g: 20, b: 30 };
    expect(resolveColor(color, "#fff")).toBe("rgb(10, 20, 30)");
  });
});
