import { get } from "svelte/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  DEFAULT_EDITOR_SETTINGS,
  PANEL_FONT_SIZE_MAX,
  PANEL_FONT_SIZE_MIN,
  editorSettings,
  formatsOnSave,
  languageList,
  panelFontSizeDown,
  panelFontSizeReset,
  panelFontSizeUp,
  parseEditorSettings,
  updateEditorSettings,
} from "./editorSettings";
import { nearestWeight, realWeights, weightName } from "./fonts";
import { surfaceSettings } from "./surfaceSettings";

vi.mock("../core/backend", () => ({ invokeBackend: vi.fn(() => Promise.resolve()) }));

describe("the floating window's own font size", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    editorSettings.set({ ...DEFAULT_EDITOR_SETTINGS });
  });

  afterEach(() => {
    vi.runAllTimers();
    vi.useRealTimers();
  });

  it("is larger than the Studio's by default and used when the field is missing", () => {
    expect(DEFAULT_EDITOR_SETTINGS.panelFontSize).toBe(16);
    expect(DEFAULT_EDITOR_SETTINGS.panelFontSize).toBeGreaterThan(DEFAULT_EDITOR_SETTINGS.fontSize);
    expect(parseEditorSettings({ fontSize: 20 }).panelFontSize).toBe(16);
  });

  it("keeps the Studio's font size and its 9–32 bounds as they were", () => {
    expect(DEFAULT_EDITOR_SETTINGS.fontSize).toBe(14);
    expect(parseEditorSettings({ fontSize: 8 }).fontSize).toBe(9);
    expect(parseEditorSettings({ fontSize: 33 }).fontSize).toBe(32);
  });

  it("clamps values beyond the bounds", () => {
    expect(parseEditorSettings({ panelFontSize: 9 }).panelFontSize).toBe(PANEL_FONT_SIZE_MIN);
    expect(parseEditorSettings({ panelFontSize: -3 }).panelFontSize).toBe(PANEL_FONT_SIZE_MIN);
    expect(parseEditorSettings({ panelFontSize: 41 }).panelFontSize).toBe(PANEL_FONT_SIZE_MAX);
    expect(parseEditorSettings({ panelFontSize: 1000 }).panelFontSize).toBe(PANEL_FONT_SIZE_MAX);
    expect(parseEditorSettings({ panelFontSize: 10 }).panelFontSize).toBe(10);
    expect(parseEditorSettings({ panelFontSize: 40 }).panelFontSize).toBe(40);
  });

  it("falls back to the default for a value that is no finite number", () => {
    for (const bad of ["18", null, true, {}, [], Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(parseEditorSettings({ panelFontSize: bad }).panelFontSize).toBe(16);
    }
  });

  it("rounds to whole points", () => {
    expect(parseEditorSettings({ panelFontSize: 18.4 }).panelFontSize).toBe(18);
    expect(parseEditorSettings({ panelFontSize: 18.5 }).panelFontSize).toBe(19);
  });

  it("steps one size up or down and resets to the default", () => {
    expect(panelFontSizeUp(16)).toBe(17);
    expect(panelFontSizeDown(16)).toBe(15);
    expect(panelFontSizeReset()).toBe(DEFAULT_EDITOR_SETTINGS.panelFontSize);
    expect(panelFontSizeUp(panelFontSizeDown(20))).toBe(20);
  });

  it("returns the same value when a step hits the upper or lower bound", () => {
    expect(panelFontSizeUp(PANEL_FONT_SIZE_MAX)).toBe(PANEL_FONT_SIZE_MAX);
    expect(panelFontSizeDown(PANEL_FONT_SIZE_MIN)).toBe(PANEL_FONT_SIZE_MIN);
    expect(panelFontSizeUp(PANEL_FONT_SIZE_MAX - 1)).toBe(PANEL_FONT_SIZE_MAX);
    expect(panelFontSizeDown(PANEL_FONT_SIZE_MIN + 1)).toBe(PANEL_FONT_SIZE_MIN);
  });

  it("changes one size without touching the other through updateEditorSettings", () => {
    updateEditorSettings({ panelFontSize: 22 });
    expect(get(editorSettings)).toMatchObject({ panelFontSize: 22, fontSize: 14 });
    updateEditorSettings({ fontSize: 18 });
    expect(get(editorSettings)).toMatchObject({ panelFontSize: 22, fontSize: 18 });
  });
});

describe("format on save (L14)", () => {
  it("is on by default, and a language on the list is saved as it is", () => {
    const d = parseEditorSettings({});
    expect(formatsOnSave(d, "rust")).toBe(true);
    expect(formatsOnSave(d, null)).toBe(false);
    const except = parseEditorSettings({ formatOnSaveExcept: [" Markdown", "python", "python", 3, "bad id!"] });
    expect(except.formatOnSaveExcept).toEqual(["markdown", "python"]);
    expect(formatsOnSave(except, "markdown")).toBe(false);
    expect(formatsOnSave(parseEditorSettings({ formatOnSave: false }), "rust")).toBe(false);
    expect(languageList("x")).toEqual([]);
  });
});

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

  it("keeps an installed font's name (T10), but refuses one that could break out of CSS", () => {
    expect(parseEditorSettings({ fontFamily: "Comic Sans MS" }).fontFamily).toBe("Comic Sans MS");
    for (const bad of ['Evil"; x', ".SF NS", "", "a\\b", "x".repeat(129)]) {
      expect(parseEditorSettings({ fontFamily: bad }).fontFamily).toBe("JetBrains Mono");
    }
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

describe("cursor animation and file icons (editor-look K8, K5)", () => {
  it("defaults to the strong glide and Catppuccin icons, and takes the names saved before K8", () => {
    expect(DEFAULT_EDITOR_SETTINGS.cursorAnimation).toBe("strong");
    expect(parseEditorSettings({ cursorAnimation: "trail" }).cursorAnimation).toBe("strong");
    expect(parseEditorSettings({ cursorAnimation: "glide" }).cursorAnimation).toBe("subtle");
    expect(parseEditorSettings({ cursorAnimation: "off" }).cursorAnimation).toBe("off");
    expect(parseEditorSettings({ cursorAnimation: "wild" }).cursorAnimation).toBe("strong");
    expect(DEFAULT_EDITOR_SETTINGS.fileIcons).toBe("catppuccin");
    expect(parseEditorSettings({ fileIcons: "jetbrains" }).fileIcons).toBe("jetbrains");
    expect(parseEditorSettings({ fileIcons: "emoji" }).fileIcons).toBe("catppuccin");
  });
});

describe("tab colours (editor-look K14)", () => {
  it("defaults to on, keeps off, and ignores a value that is no boolean", () => {
    expect(DEFAULT_EDITOR_SETTINGS.tabColors).toBe(true);
    expect(parseEditorSettings({}).tabColors).toBe(true);
    expect(parseEditorSettings({ tabColors: false }).tabColors).toBe(false);
    expect(parseEditorSettings({ tabColors: "no" }).tabColors).toBe(true);
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
