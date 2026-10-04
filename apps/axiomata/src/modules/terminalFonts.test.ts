import { describe, expect, it } from "vitest";

import { BUNDLED_FONTS } from "./terminalFonts";

describe("BUNDLED_FONTS", () => {
  it("has eleven distinct family names (ten from the owner's original request, plus the Checkpoint 5m Nerd-Fonts-patched entry)", () => {
    const names = BUNDLED_FONTS.map((f) => f.family);
    expect(names).toHaveLength(11);
    expect(new Set(names).size).toBe(11);
  });

  it("includes the Checkpoint 5m Nerd-Fonts-patched entry, matching the owner's own Ghostty config exactly", () => {
    expect(BUNDLED_FONTS.some((f) => f.family === "JetBrainsMono Nerd Font Mono")).toBe(true);
  });

  it("every family bundles 400 (Regular) and 700 (Bold), ascending", () => {
    for (const font of BUNDLED_FONTS) {
      expect(font.weights, font.family).toContain(400);
      expect(font.weights, font.family).toContain(700);
      const sorted = [...font.weights].sort((a, b) => a - b);
      expect(font.weights, font.family).toEqual(sorted);
    }
  });

  it("every weight is a valid CSS font-weight number (1-1000)", () => {
    for (const font of BUNDLED_FONTS) {
      for (const w of font.weights) {
        expect(w, font.family).toBeGreaterThanOrEqual(1);
        expect(w, font.family).toBeLessThanOrEqual(1000);
      }
    }
  });

  it("includes at least one family with only Regular/Bold, per the owner's explicit ask", () => {
    // "auch gerne noch ein paar Monoschriften auch wenn sie kein Thin etc.
    // anbieten" — at least one bundled family should have exactly the two
    // weights every font here guarantees, nothing extra.
    expect(BUNDLED_FONTS.some((f) => f.weights.length === 2)).toBe(true);
  });
});
