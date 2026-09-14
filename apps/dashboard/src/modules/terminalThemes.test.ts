import { describe, expect, it } from "vitest";

import { DEFAULT_THEME, THEMES } from "./terminalThemes";

describe("THEMES", () => {
  it("defines exactly 16 colours for every named palette", () => {
    for (const [name, palette] of Object.entries(THEMES)) {
      expect(palette, `theme ${name}`).toHaveLength(16);
    }
  });

  it("keeps xterm as the original ANSI_16 table (backward compatible default)", () => {
    expect(THEMES.xterm[0]).toBe("#000000");
    expect(THEMES.xterm[15]).toBe("#ffffff");
  });

  it("has a DEFAULT_THEME that actually names an entry in THEMES", () => {
    expect(THEMES[DEFAULT_THEME]).toBeDefined();
    expect(DEFAULT_THEME).toBe("xterm");
  });
});
