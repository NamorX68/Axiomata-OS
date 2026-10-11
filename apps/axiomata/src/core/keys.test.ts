import { describe, expect, it } from "vitest";

import {
  chord,
  detectPlatform,
  foreignModifier,
  inTerminalPane,
  isClosePress,
  isKeyboardSetting,
  platformLabel,
  primaryHeld,
  resolvePlatform,
  shiftedForm,
  type KeyLike,
} from "./keys";

function press(key: string, held: Partial<KeyLike> = {}): KeyLike {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...held };
}

describe("detectPlatform", () => {
  it("reads a Mac from the browser's platform", () => {
    expect(detectPlatform({ platform: "MacIntel" })).toBe("mac");
  });

  it("takes anything else for Windows/Linux, and a missing navigator too", () => {
    expect(detectPlatform({ platform: "Win32" })).toBe("other");
    expect(detectPlatform({ platform: "Linux x86_64" })).toBe("other");
    expect(detectPlatform(undefined as never)).toBe("other");
  });
});

describe("resolvePlatform", () => {
  it("follows the machine on Auto and the setting otherwise", () => {
    expect(resolvePlatform("auto", "mac")).toBe("mac");
    expect(resolvePlatform("auto", "other")).toBe("other");
    expect(resolvePlatform("other", "mac")).toBe("other");
    expect(resolvePlatform("mac", "other")).toBe("mac");
  });

  it("accepts only the three settings from a stored file", () => {
    expect(["auto", "mac", "other"].every(isKeyboardSetting)).toBe(true);
    expect(isKeyboardSetting("windows")).toBe(false);
    expect(isKeyboardSetting(undefined)).toBe(false);
  });
});

describe("chord on the Mac", () => {
  it("takes ⌘ as the primary key and ignores Ctrl", () => {
    expect(chord(press("k", { metaKey: true }), "mac").primary).toBe(true);
    expect(chord(press("k", { ctrlKey: true }), "mac").primary).toBe(false);
    expect(chord(press("k", { metaKey: true, ctrlKey: true }), "mac").primary).toBe(false);
  });

  it("passes ⌥ and Shift through and lower-cases the key", () => {
    const c = chord(press("K", { metaKey: true, altKey: true, shiftKey: true }), "mac");
    expect(c).toEqual({ primary: true, alt: true, shift: true, typing: false, key: "k" });
  });

  it("has no AltGr: ⌃⌥ stays what it says", () => {
    const c = chord(press("@", { ctrlKey: true, altKey: true }), "mac");
    expect(c.typing).toBe(false);
  });
});

describe("chord off the Mac", () => {
  it("takes Ctrl as the primary key and ignores ⌘/Super", () => {
    expect(chord(press("k", { ctrlKey: true }), "other").primary).toBe(true);
    expect(chord(press("k", { metaKey: true }), "other").primary).toBe(false);
    expect(chord(press("k", { ctrlKey: true, metaKey: true }), "other").primary).toBe(false);
  });

  it("keeps Alt and Shift as they are", () => {
    const c = chord(press("F", { ctrlKey: true, altKey: true, shiftKey: true }), "other");
    expect(c).toEqual({ primary: true, alt: true, shift: true, typing: false, key: "f" });
  });

  it("makes a terminal press need Ctrl+Shift, and absorbs that Shift", () => {
    expect(chord(press("k", { ctrlKey: true }), "other", true).primary).toBe(false);
    const c = chord(press("K", { ctrlKey: true, shiftKey: true }), "other", true);
    expect(c).toEqual({ primary: true, alt: false, shift: false, typing: false, key: "k" });
  });

  it("does not change the Mac's terminal: ⌘ never reaches a shell", () => {
    expect(chord(press("k", { metaKey: true }), "mac", true).primary).toBe(true);
  });
});

describe("AltGr", () => {
  it("is typing when the webview says so, whatever letter it carries", () => {
    const altGr = { ctrlKey: true, altKey: true, getModifierState: (k: string) => k === "AltGraph" };
    const c = chord(press("q", altGr), "other");
    expect(c).toMatchObject({ typing: true, primary: false, alt: false, shift: false });
  });

  it("is typing without that report when Ctrl+Alt wrote a symbol (@ € \\ [ ] { })", () => {
    for (const key of ["@", "€", "\\", "[", "]", "{", "}", "ä", "~"]) {
      const c = chord(press(key, { ctrlKey: true, altKey: true }), "other");
      expect(c.typing, key).toBe(true);
      expect(c.primary, key).toBe(false);
    }
  });

  it("leaves a real Ctrl+Alt chord alone: a letter, a digit or an arrow", () => {
    for (const key of ["f", "F", "7", "ArrowDown"]) {
      const c = chord(press(key, { ctrlKey: true, altKey: true }), "other");
      expect(c.typing, key).toBe(false);
      expect(c.primary, key).toBe(true);
    }
  });

  it("is not typing for a plain Alt press", () => {
    expect(chord(press("@", { altKey: true }), "other").typing).toBe(false);
  });
});

describe("isClosePress", () => {
  it("is ⌘W on the Mac and nothing else", () => {
    expect(isClosePress(press("w", { metaKey: true }), "mac")).toBe(true);
    expect(isClosePress(press("W", { metaKey: true, shiftKey: true }), "mac")).toBe(false);
    expect(isClosePress(press("w", { ctrlKey: true }), "mac")).toBe(false);
  });

  it("is Ctrl+Shift+W elsewhere, in a terminal too, and never plain Ctrl+W", () => {
    expect(isClosePress(press("W", { ctrlKey: true, shiftKey: true }), "other")).toBe(true);
    expect(isClosePress(press("W", { ctrlKey: true, shiftKey: true }), "other", true)).toBe(true);
    expect(isClosePress(press("w", { ctrlKey: true }), "other")).toBe(false);
    expect(isClosePress(press("w", { ctrlKey: true }), "other", true)).toBe(false);
  });
});

describe("primaryHeld", () => {
  it("reads ⌘ on the Mac and Ctrl elsewhere, for a click", () => {
    expect(primaryHeld({ metaKey: true, ctrlKey: false }, "mac")).toBe(true);
    expect(primaryHeld({ metaKey: false, ctrlKey: true }, "mac")).toBe(false);
    expect(primaryHeld({ metaKey: false, ctrlKey: true }, "other")).toBe(true);
    expect(primaryHeld({ metaKey: true, ctrlKey: false }, "other")).toBe(false);
    expect(primaryHeld({ metaKey: true, ctrlKey: true }, "other")).toBe(false);
  });
});

describe("foreignModifier", () => {
  it("is Ctrl on the Mac and Super elsewhere", () => {
    expect(foreignModifier({ metaKey: false, ctrlKey: true }, "mac")).toBe(true);
    expect(foreignModifier({ metaKey: true, ctrlKey: false }, "mac")).toBe(false);
    expect(foreignModifier({ metaKey: true, ctrlKey: false }, "other")).toBe(true);
    expect(foreignModifier({ metaKey: false, ctrlKey: true }, "other")).toBe(false);
  });
});

describe("shiftedForm", () => {
  it("is the Shift held, or in a terminal the Ctrl+Shift that stood for Ctrl", () => {
    const c = chord(press("k", { ctrlKey: true }), "other");
    expect(shiftedForm(c, false)).toBe(false);
    expect(shiftedForm(c, true)).toBe(true);
    expect(shiftedForm(chord(press("K", { ctrlKey: true, shiftKey: true }), "other"), false)).toBe(true);
  });
});

describe("inTerminalPane and platformLabel", () => {
  it("finds a terminal pane around the target, and none for a bare target", () => {
    const pane = document.createElement("div");
    pane.className = "terminal";
    const input = document.createElement("input");
    pane.append(input);
    expect(inTerminalPane({ target: input })).toBe(true);
    expect(inTerminalPane({ target: document.createElement("input") })).toBe(false);
    expect(inTerminalPane({ target: null })).toBe(false);
  });

  it("spells the platforms for the settings", () => {
    expect(platformLabel("mac")).toBe("Mac");
    expect(platformLabel("other")).toBe("Windows & Linux");
  });
});
