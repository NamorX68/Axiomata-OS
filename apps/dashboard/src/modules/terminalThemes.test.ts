import { describe, expect, it } from "vitest";

import { DEFAULT_THEME, THEME_DEFAULT_COLORS, THEMES } from "./terminalThemes";

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

  it("includes the Checkpoint 5g palettes (owner request: Catppuccin, Tokyo Night)", () => {
    expect(THEMES["catppuccin-mocha"]).toBeDefined();
    expect(THEMES["tokyo-night"]).toBeDefined();
  });

  it("only uses lowercase 6-digit hex colours", () => {
    for (const [name, palette] of Object.entries(THEMES)) {
      for (const color of palette) {
        expect(color, `theme ${name}`).toMatch(/^#[0-9a-f]{6}$/);
      }
    }
  });
});

describe("THEME_DEFAULT_COLORS", () => {
  it("has an entry for every named theme except xterm (Checkpoint 5k)", () => {
    const namedThemes = Object.keys(THEMES).filter((name) => name !== "xterm");
    for (const name of namedThemes) {
      expect(THEME_DEFAULT_COLORS[name], `theme ${name}`).toBeDefined();
    }
    expect(THEME_DEFAULT_COLORS.xterm).toBeUndefined();
  });

  it("only uses lowercase 6-digit hex colours", () => {
    for (const [name, colors] of Object.entries(THEME_DEFAULT_COLORS)) {
      expect(colors?.background, `theme ${name} background`).toMatch(/^#[0-9a-f]{6}$/);
      expect(colors?.foreground, `theme ${name} foreground`).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it("background and foreground are never the same colour (a real, readable theme)", () => {
    for (const [name, colors] of Object.entries(THEME_DEFAULT_COLORS)) {
      expect(colors?.background, name).not.toBe(colors?.foreground);
    }
  });
});
