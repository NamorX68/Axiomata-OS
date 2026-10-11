import { describe, expect, it } from "vitest";

import { foldKeyAction } from "../editor/keymap";
import { diffHunkAction, diffViewAction } from "../fileapp/diffKeys";
import { editorKeyInput } from "../fileapp/editorKeys";
import { studioKeyAction } from "../ide/studioKeys";
import { globalKeyAction } from "./globalKeys";
import { keyHints, otherNotation, OTHER_KEYS } from "./keyHints";
import type { KeyLike } from "./keys";

const VI = new Set(["c", "d", "f", "v"]);

describe("keyHints", () => {
  it("gives the Mac its text back untouched", () => {
    const text = "Search the project (⇧⌘F), Reload (⌘R), Next match (⏎, ⌘G)";
    expect(keyHints(text, "mac")).toBe(text);
  });

  it("writes the glyphs as words elsewhere, Ctrl before Alt before Shift", () => {
    expect(keyHints("Search the project (⇧⌘F)", "other")).toBe("Search the project (Ctrl+Shift+F)");
    expect(keyHints("Match case (⌥⌘C)", "other")).toBe("Match case (Ctrl+Alt+C)");
    expect(keyHints("Format (⇧⌥F)", "other")).toBe("Format (Alt+Shift+F)");
    expect(keyHints("Complete: ⌃Space, ⌘F12, ⌘Home", "other")).toBe("Complete: Ctrl+Space, Ctrl+F12, Ctrl+Home");
  });

  it("names the keys that have a glyph", () => {
    expect(keyHints("Previous match (⇧⏎, ⇧⌘G)", "other")).toBe("Previous match (Shift+Enter, Ctrl+Shift+G)");
    expect(keyHints("⌫ / ⌥⌫ / ⌦ / ⇥", "other")).toBe("Backspace / Alt+Backspace / Delete / Tab");
    expect(keyHints("Write to the session … (⌘↩ sends)", "other")).toBe("Write to the session … (Ctrl+Enter sends)");
  });

  it("takes Ctrl+Shift for the commands Vi or a shell would own", () => {
    expect(keyHints("Open a file (⌘O)", "other")).toBe("Open a file (Ctrl+Shift+O)");
    expect(keyHints("Close (⌘W)", "other")).toBe("Close (Ctrl+Shift+W)");
    expect(keyHints("Reload (⌘R)", "other")).toBe("Reload (Ctrl+Shift+R)");
    expect(keyHints("Discard (⌘⌫)", "other")).toBe("Discard (Ctrl+Shift+Backspace)");
    expect(keyHints("Fold (⌥⌘[)", "other")).toBe("Fold (Ctrl+Shift+[)");
  });

  it("follows ⌘-click to Ctrl+click and keeps a comma key", () => {
    expect(keyHints("⌘-click a file:line to open it", "other")).toBe("Ctrl+click a file:line to open it");
    expect(keyHints("⌘,", "other")).toBe("Ctrl+,");
  });

  it("adds Shift to a plain ⌘ letter that Vi binds, only while Vi is on", () => {
    expect(keyHints("⌘C / ⌘Z", "other", { vi: true, viLetters: VI })).toBe("Ctrl+Shift+C / Ctrl+Z");
    expect(keyHints("⌘C", "other", { vi: false, viLetters: VI })).toBe("Ctrl+C");
    expect(keyHints("⌥⌘F", "other", { vi: true, viLetters: VI })).toBe("Ctrl+Alt+F");
  });
});

describe("otherNotation", () => {
  it("is one spec at a time", () => {
    expect(otherNotation("⇧⌘Z")).toBe("Ctrl+Shift+Z");
  });
});

describe("the keys that differ from a plain swap", () => {
  const ctrlShift = (key: string, code?: string): KeyLike => ({
    key, code, metaKey: false, ctrlKey: true, altKey: false, shiftKey: true,
  });

  it("are the keys the matching code really answers to", () => {
    expect(studioKeyAction(ctrlShift("P"), "other")).toEqual({ kind: "quick-open" });
    expect(studioKeyAction(ctrlShift("N"), "other")).toEqual({ kind: "new-note" });
    expect(studioKeyAction(ctrlShift("O"), "other")).toEqual({ kind: "open-file" });
    expect(studioKeyAction(ctrlShift("B"), "other")).toEqual({ kind: "toggle-tree" });
    expect(studioKeyAction(ctrlShift("W"), "other")).toEqual({ kind: "close-file" });
    expect(globalKeyAction(ctrlShift("W"), "other")).toBe("close");
    expect(diffViewAction(ctrlShift("R"), "other")).toBe("refresh");
    expect(diffHunkAction(ctrlShift("Backspace"), "other")).toBe("discard-hunk");
    const fold = editorKeyInput(ctrlShift("{", "BracketLeft"), "other", false);
    expect(foldKeyAction(fold)).toBe("fold");
    expect(foldKeyAction(editorKeyInput(ctrlShift("}", "BracketRight"), "other", false))).toBe("unfold");
  });

  it("are all covered above", () => {
    expect(Object.keys(OTHER_KEYS).sort()).toEqual(["⌘B", "⌘N", "⌘O", "⌘P", "⌘R", "⌘W", "⌘⌫", "⌥⌘[", "⌥⌘]"].sort());
  });
});
