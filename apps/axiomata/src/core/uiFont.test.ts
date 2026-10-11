import { describe, expect, it } from "vitest";

import { DEFAULT_UI_FONT, UI_FONTS, isUiFont } from "./uiFont";

describe("ui font", () => {
  it("offers Geist, Inter and IBM Plex Sans, Geist first and by default", () => {
    expect(UI_FONTS.map((f) => f.id)).toEqual(["geist", "inter", "plex"]);
    expect(DEFAULT_UI_FONT).toBe("geist");
  });

  it("accepts only a bundled font from storage", () => {
    expect(isUiFont("inter")).toBe(true);
    expect(isUiFont("helvetica")).toBe(false);
    expect(isUiFont(3)).toBe(false);
  });
});
