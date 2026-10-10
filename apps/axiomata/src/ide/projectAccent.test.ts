import { describe, expect, it } from "vitest";

import { ACCENT_SLOTS, accentColor } from "./projectAccent";
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
