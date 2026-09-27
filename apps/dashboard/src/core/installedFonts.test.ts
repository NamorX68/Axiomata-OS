import { get } from "svelte/store";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("./backend", () => ({ invokeBackend: (...a: unknown[]) => invoke(...a) }));

const FONTS = [
  { family: "Menlo", weights: [400, 700], monospace: true },
  { family: "Helvetica", weights: [300, 400, 700], monospace: false },
];

describe("installed fonts (T10)", () => {
  beforeEach(() => {
    vi.resetModules();
    invoke.mockReset();
  });

  it("asks once, and finds a family case-insensitively", async () => {
    invoke.mockResolvedValue(FONTS);
    const m = await import("./installedFonts");
    expect(get(m.installedFonts).loaded).toBe(false);
    await Promise.all([m.ensureInstalledFonts(), m.ensureInstalledFonts()]);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(get(m.installedFonts).loaded).toBe(true);
    expect(m.installedFont("menlo")?.weights).toEqual([400, 700]);
    expect(m.installedFont("Courier")).toBeUndefined();
  });

  it("a failed lookup counts as loaded and empty", async () => {
    invoke.mockRejectedValue(new Error("no CoreText"));
    const m = await import("./installedFonts");
    await m.ensureInstalledFonts();
    expect(get(m.installedFonts)).toEqual({ loaded: true, fonts: [] });
  });

  it("the fonts module reads an installed family's weights and draws the default for a missing one", async () => {
    invoke.mockResolvedValue(FONTS);
    const installed = await import("./installedFonts");
    const fonts = await import("../fileapp/fonts");
    expect(fonts.drawnFamily("Menlo", false)).toBe("Menlo");
    // Before the list arrives nothing counts as missing.
    expect(fonts.drawnFamily("Gone Mono", false)).toBe("Gone Mono");
    await installed.ensureInstalledFonts();
    expect(fonts.realWeights("Helvetica")).toEqual([300, 400, 700]);
    expect(fonts.nearestWeight("Menlo", 600)).toBe(700);
    expect(fonts.drawnFamily("Gone Mono", true)).toBe(fonts.DEFAULT_FONT_FAMILY);
    expect(fonts.drawnFamily("Fira Code", true)).toBe("Fira Code");
  });

  it("usableFamilyName matches the Rust rule", async () => {
    const { usableFamilyName } = await import("./installedFonts");
    expect(usableFamilyName("Hiragino Kaku Gothic ProN")).toBe(true);
    const controls = ["a\nb", "a\u0080b", "a\u009fb"];
    for (const bad of ["", ".SFNS", 'a"b', "a'b", "a\\b", "a;b", "a{b", "a<b", ...controls, "ä".repeat(65)]) {
      expect(usableFamilyName(bad)).toBe(false);
    }
  });
});
