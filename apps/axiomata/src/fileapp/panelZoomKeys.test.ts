import { describe, expect, it } from "vitest";

import { DEFAULT_EDITOR_SETTINGS, PANEL_FONT_SIZE_MAX, PANEL_FONT_SIZE_MIN } from "./editorSettings";
import { applyPanelZoomStep, panelZoomStep, previewZoom, type ZoomKeyEvent } from "./panelZoomKeys";

function press(key: string, mods: Partial<ZoomKeyEvent> = {}): ZoomKeyEvent {
  return { key, metaKey: true, ctrlKey: false, altKey: false, shiftKey: false, ...mods };
}

describe("panelZoomStep", () => {
  it("grows on ⌘+ and on ⌘= (the same key without Shift)", () => {
    expect(panelZoomStep(press("+"), "mac")).toBe("up");
    expect(panelZoomStep(press("="), "mac")).toBe("up");
  });

  it("shrinks on ⌘- and on its shifted form", () => {
    expect(panelZoomStep(press("-"), "mac")).toBe("down");
    expect(panelZoomStep(press("_"), "mac")).toBe("down");
  });

  it("resets on ⌘0", () => {
    expect(panelZoomStep(press("0"), "mac")).toBe("reset");
  });

  it("leaves the keys without ⌘ alone, so Vi's `+`, `-` and `0` keep working", () => {
    for (const key of ["+", "-", "0"]) expect(panelZoomStep(press(key, { metaKey: false }), "mac")).toBeNull();
  });

  it("leaves ⌘ together with Ctrl or Alt alone", () => {
    expect(panelZoomStep(press("+", { ctrlKey: true }), "mac")).toBeNull();
    expect(panelZoomStep(press("-", { altKey: true }), "mac")).toBeNull();
  });

  it("ignores other keys", () => {
    expect(panelZoomStep(press("s"), "mac")).toBeNull();
    expect(panelZoomStep(press("1"), "mac")).toBeNull();
  });
});

describe("panelZoomStep off the Mac", () => {
  const ctrl = { metaKey: false, ctrlKey: true };

  it("zooms on Ctrl+ + - 0", () => {
    expect(panelZoomStep(press("+", ctrl), "other")).toBe("up");
    expect(panelZoomStep(press("=", ctrl), "other")).toBe("up");
    expect(panelZoomStep(press("-", ctrl), "other")).toBe("down");
    expect(panelZoomStep(press("0", ctrl), "other")).toBe("reset");
  });

  it("leaves ⌘/Super, Alt and AltGr alone", () => {
    expect(panelZoomStep(press("+"), "other")).toBeNull();
    expect(panelZoomStep(press("-", { ...ctrl, altKey: true }), "other")).toBeNull();
    expect(panelZoomStep(press("0", { ...ctrl, altKey: true, getModifierState: () => true }), "other")).toBeNull();
  });
});

describe("applyPanelZoomStep", () => {
  it("steps up and down by one point", () => {
    expect(applyPanelZoomStep(16, "up")).toBe(17);
    expect(applyPanelZoomStep(16, "down")).toBe(15);
  });

  it("resets to the default whatever the size was", () => {
    expect(applyPanelZoomStep(33, "reset")).toBe(DEFAULT_EDITOR_SETTINGS.panelFontSize);
  });

  it("stays inside the bounds", () => {
    expect(applyPanelZoomStep(PANEL_FONT_SIZE_MAX, "up")).toBe(PANEL_FONT_SIZE_MAX);
    expect(applyPanelZoomStep(PANEL_FONT_SIZE_MIN, "down")).toBe(PANEL_FONT_SIZE_MIN);
  });
});

describe("previewZoom", () => {
  it("is 1 at the default size and follows the size proportionally", () => {
    expect(previewZoom(DEFAULT_EDITOR_SETTINGS.panelFontSize)).toBe(1);
    expect(previewZoom(32)).toBe(2);
    expect(previewZoom(8)).toBe(0.5);
  });
});
