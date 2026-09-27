import { describe, expect, it } from "vitest";

import { DEFAULT_EDITOR_SETTINGS, parseEditorSettings } from "./editorSettings";
import { nearestWeight, realWeights, weightName } from "./fonts";
import { surfaceSettings } from "./surfaceSettings";

describe("parseEditorSettings", () => {
  it("defaults everything for an empty or broken file", () => {
    expect(parseEditorSettings({})).toEqual(DEFAULT_EDITOR_SETTINGS);
    expect(parseEditorSettings(null)).toEqual(DEFAULT_EDITOR_SETTINGS);
    expect(parseEditorSettings("nonsense")).toEqual(DEFAULT_EDITOR_SETTINGS);
  });

  it("keeps good fields and replaces bad ones one by one", () => {
    const s = parseEditorSettings({
      fontFamily: "Fira Code",
      fontWeight: 250,
      fontSize: 99,
      lineNumbers: "roman",
      autosave: "delay",
      mode: "vi",
      ligatures: "yes",
    });
    expect(s).toMatchObject({
      fontFamily: "Fira Code",
      fontWeight: 300,
      fontSize: 32,
      lineNumbers: "hybrid",
      autosave: "delay",
      mode: "vi",
      ligatures: true,
    });
  });

  it("refuses a font that is not bundled", () => {
    expect(parseEditorSettings({ fontFamily: "Comic Sans MS" }).fontFamily).toBe("JetBrains Mono");
  });

  it("defaults viClipboard to 'shared' (D17) and keeps a good value", () => {
    expect(parseEditorSettings({}).viClipboard).toBe("shared");
    expect(parseEditorSettings({ viClipboard: "separate" }).viClipboard).toBe("separate");
    expect(parseEditorSettings({ viClipboard: "shared" }).viClipboard).toBe("shared");
  });

  it("falls back to 'shared' for an unknown viClipboard value", () => {
    expect(parseEditorSettings({ viClipboard: "nonsense" }).viClipboard).toBe("shared");
    expect(parseEditorSettings({ viClipboard: null }).viClipboard).toBe("shared");
    expect(parseEditorSettings({ viClipboard: 1 }).viClipboard).toBe("shared");
  });
});

describe("font weights (F13)", () => {
  it("lists only the weights a family really has", () => {
    expect(realWeights("Space Mono")).toEqual([400, 700]);
    expect(realWeights("Source Code Pro")).toEqual([200, 300, 400, 500, 600, 700, 800, 900]);
    expect(realWeights("JetBrains Mono")[0]).toBe(100);
  });

  it("draws the nearest real face, the lighter one on a tie", () => {
    expect(nearestWeight("Space Mono", 100)).toBe(400);
    expect(nearestWeight("Space Mono", 550)).toBe(400);
    expect(nearestWeight("Fira Code", 100)).toBe(300);
    expect(nearestWeight("JetBrains Mono", 900)).toBe(800);
    expect(nearestWeight("JetBrains Mono", 300)).toBe(300);
  });

  it("names weights the way font menus do", () => {
    expect(weightName(100)).toBe("Thin 100");
    expect(weightName(600)).toBe("SemiBold 600");
  });
});

describe("minimap and sticky scroll settings (ED5, T8/T9)", () => {
  it("defaults both to on", () => {
    expect(DEFAULT_EDITOR_SETTINGS).toMatchObject({
      minimap: true,
      stickyScroll: true,
    });
    expect(parseEditorSettings({})).toMatchObject({
      minimap: true,
      stickyScroll: true,
    });
  });

  it("keeps each switched off on its own", () => {
    expect(parseEditorSettings({ minimap: false })).toMatchObject({
      minimap: false,
      stickyScroll: true,
    });
    expect(parseEditorSettings({ stickyScroll: false })).toMatchObject({
      minimap: true,
      stickyScroll: false,
    });
  });

  it("falls back to on for a value that is not a boolean", () => {
    const s = parseEditorSettings({ minimap: "no", stickyScroll: 0 });
    expect(s).toMatchObject({ minimap: true, stickyScroll: true });
    expect(parseEditorSettings({ minimap: null, stickyScroll: "false" })).toMatchObject({
      minimap: true,
      stickyScroll: true,
    });
  });

  it("passes both through to the surface settings", () => {
    const off = {
      ...DEFAULT_EDITOR_SETTINGS,
      minimap: false,
      stickyScroll: false,
    };
    expect(surfaceSettings(off, false)).toMatchObject({
      minimap: false,
      stickyScroll: false,
    });
    expect(surfaceSettings(DEFAULT_EDITOR_SETTINGS, true)).toMatchObject({
      minimap: true,
      stickyScroll: true,
    });
  });
});
