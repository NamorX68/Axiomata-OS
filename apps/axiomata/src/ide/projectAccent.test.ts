import { describe, expect, it } from "vitest";

import { ACCENT_SLOTS, accentColor, markerSwatches } from "./projectAccent";
import { themeSwatches } from "../graph/model";

describe("accentColor", () => {
  const swatches = themeSwatches("#ff7a1a", false);

  it("has as many slots as the theme has area hues", () => {
    expect(swatches).toHaveLength(ACCENT_SLOTS);
  });

  it("looks a slot up in the palette of the theme in effect", () => {
    expect(accentColor(0, swatches)).toBe(swatches[0]);
    expect(accentColor(ACCENT_SLOTS - 1, swatches)).toBe(swatches[ACCENT_SLOTS - 1]);
    expect(accentColor(3, themeSwatches("#7aa2f7", true))).not.toBe(swatches[3]);
  });

  it("gives no colour for no slot, a slot that is not there, or an empty palette", () => {
    expect(accentColor(null, swatches)).toBeNull();
    expect(accentColor(undefined, swatches)).toBeNull();
    expect(accentColor(ACCENT_SLOTS, swatches)).toBeNull();
    expect(accentColor(-1, swatches)).toBeNull();
    expect(accentColor(1.5, swatches)).toBeNull();
    expect(accentColor(2, [])).toBeNull();
  });
});

describe("markerSwatches", () => {
  const luminance = (hsl: string): number => {
    const [h, s, l] = (/hsl\((\d+) (\d+)% (\d+)%\)/.exec(hsl) ?? []).slice(1).map(Number) as [number, number, number];
    const chroma = (1 - Math.abs((2 * l) / 100 - 1)) * (s / 100);
    const channel = (n: number) => {
      const k = (n + h / 30) % 12;
      return l / 100 - (chroma / 2) * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    };
    const lin = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    return 0.2126 * lin(channel(0)) + 0.7152 * lin(channel(8)) + 0.0722 * lin(channel(4));
  };

  it("leaves a dark theme's palette as it is", () => {
    const dark = themeSwatches("#ff7a1a", false);
    expect(markerSwatches(dark, false)).toEqual(dark);
  });

  it("deepens the bright hues of a light theme until they read on a near-white page", () => {
    for (const accent of ["#d0642c", "#0969da", "#8839ef"]) {
      const light = themeSwatches(accent, true);
      const marks = markerSwatches(light, true);
      expect(marks).toHaveLength(ACCENT_SLOTS);
      // 0.94 + 0.05 over 4:1, less 0.05.
      for (const mark of marks) expect(luminance(mark)).toBeLessThanOrEqual(0.1975);
      // Only lightness moves: the hue (and so the slot's identity) stays.
      marks.forEach((mark, i) => expect(mark.split(" ")[0]).toBe(light[i]?.split(" ")[0]));
    }
  });

  it("does not touch a colour it cannot read", () => {
    expect(markerSwatches(["not a colour"], true)).toEqual(["not a colour"]);
  });
});
