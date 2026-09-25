import { describe, expect, it } from "vitest";

import type { ViMode } from "../editor/vi/machine";
import { viKeyFor } from "./viKeys";

const press = (key: string, mods: { meta?: boolean; alt?: boolean; shift?: boolean; ctrl?: boolean } = {}) => ({
  key,
  meta: mods.meta ?? false,
  alt: mods.alt ?? false,
  shift: mods.shift ?? false,
  ctrl: mods.ctrl ?? false,
});

describe("viKeyFor (V5, V12)", () => {
  it("leaves characters to the textarea, whatever produced them", () => {
    expect(viKeyFor(press("d"), "normal")).toEqual({ kind: "text" });
    expect(viKeyFor(press("["), "normal")).toEqual({ kind: "text" });
    expect(viKeyFor(press("@", { alt: true }), "normal")).toEqual({ kind: "text" });
    expect(viKeyFor(press("Dead"), "normal")).toEqual({ kind: "text" });
    expect(viKeyFor(press("ä"), "insert")).toEqual({ kind: "text" });
  });

  it("turns special keys and Ctrl into Vi keys", () => {
    expect(viKeyFor(press("Escape"), "insert")).toEqual({ kind: "key", key: "<Esc>" });
    expect(viKeyFor(press("Enter"), "normal")).toEqual({ kind: "key", key: "<CR>" });
    expect(viKeyFor(press("r", { ctrl: true }), "normal")).toEqual({ kind: "key", key: "<C-r>" });
    expect(viKeyFor(press("[", { ctrl: true }), "insert")).toEqual({ kind: "key", key: "<C-[>" });
    expect(viKeyFor(press("Backspace"), "insert")).toEqual({ kind: "key", key: "<BS>" });
  });

  it("keeps ⌘'s Mac meaning outside Insert mode", () => {
    expect(viKeyFor(press("s", { meta: true }), "normal")).toEqual({ kind: "effect", effect: "save" });
    expect(viKeyFor(press("z", { meta: true }), "normal")).toEqual({ kind: "keys", notation: "u" });
    expect(viKeyFor(press("z", { meta: true, shift: true }), "normal")).toEqual({ kind: "keys", notation: "<C-r>" });
    expect(viKeyFor(press("c", { meta: true }), "visual")).toEqual({ kind: "keys", notation: '"+y' });
    expect(viKeyFor(press("c", { meta: true }), "normal")).toEqual({ kind: "keys", notation: '"+yy' });
    expect(viKeyFor(press("v", { meta: true }), "normal")).toEqual({ kind: "keys", notation: '"+P' });
    expect(viKeyFor(press("/", { meta: true }), "visualLine")).toEqual({ kind: "keys", notation: "gc" });
  });

  it("uses the whole Mac map in Insert mode", () => {
    expect(viKeyFor(press("Backspace", { alt: true }), "insert")).toEqual({
      kind: "key",
      key: { command: { type: "deleteBackward", unit: "word" } },
    });
    expect(viKeyFor(press("ArrowLeft", { meta: true }), "insert")).toEqual({
      kind: "key",
      key: { command: { type: "move", motion: "lineStart", extend: false } },
    });
    expect(viKeyFor(press("v", { meta: true }), "insert")).toEqual({ kind: "effect", effect: "paste" });
  });

  it("treats Replace mode exactly like Insert mode", () => {
    expect(viKeyFor(press("x"), "replace")).toEqual({ kind: "text" });
    expect(viKeyFor(press("Escape"), "replace")).toEqual({ kind: "key", key: "<Esc>" });
    expect(viKeyFor(press("Backspace"), "replace")).toEqual({ kind: "key", key: "<BS>" });
    expect(viKeyFor(press("v", { meta: true }), "replace")).toEqual({ kind: "effect", effect: "paste" });
    expect(viKeyFor(press("ArrowLeft", { meta: true }), "replace")).toEqual({
      kind: "key",
      key: { command: { type: "move", motion: "lineStart", extend: false } },
    });
  });

  it("turns Tab and Shift-Tab into Vi keys in Insert mode, recorded for `.`", () => {
    expect(viKeyFor(press("Tab"), "insert")).toEqual({ kind: "key", key: "<Tab>" });
    expect(viKeyFor(press("Tab", { shift: true }), "insert")).toEqual({ kind: "key", key: "<S-Tab>" });
    // ⌘Tab / ⌥Tab are not the editor's — the plain-special-key branch only fires unmodified.
    expect(viKeyFor(press("Tab", { meta: true }), "insert")).toBeNull();
  });

  it("turns PageUp/PageDown into Vi keys in every mode", () => {
    expect(viKeyFor(press("PageUp"), "normal")).toEqual({ kind: "key", key: "<PageUp>" });
    expect(viKeyFor(press("PageDown"), "normal")).toEqual({ kind: "key", key: "<PageDown>" });
    expect(viKeyFor(press("PageUp"), "insert")).toEqual({ kind: "key", key: "<PageUp>" });
    expect(viKeyFor(press("PageDown"), "visual")).toEqual({ kind: "key", key: "<PageDown>" });
  });

  it("keeps ⌘⇧V (preview) and ⌥Z (wrap) as effects in every mode, ahead of the mode-specific rules", () => {
    const modes: ViMode[] = ["normal", "insert", "replace", "visual", "visualLine", "visualBlock"];
    for (const mode of modes) {
      expect(viKeyFor(press("v", { meta: true, shift: true }), mode)).toEqual({
        kind: "effect",
        effect: "togglePreview",
      });
      expect(viKeyFor(press("z", { alt: true }), mode)).toEqual({ kind: "effect", effect: "toggleWrap" });
    }
  });

  it("refuses Ctrl with a key that is not a letter or '['", () => {
    expect(viKeyFor(press("1", { ctrl: true }), "normal")).toBeNull();
    expect(viKeyFor(press("]", { ctrl: true }), "normal")).toBeNull();
    expect(viKeyFor(press("Escape", { ctrl: true }), "normal")).toBeNull();
  });

  it("edits the command line: Escape, Enter, arrows and Tab/S-Tab become its own Vi keys", () => {
    expect(viKeyFor(press("Escape"), "normal", true)).toEqual({ kind: "key", key: "<Esc>" });
    expect(viKeyFor(press("Enter"), "normal", true)).toEqual({ kind: "key", key: "<CR>" });
    expect(viKeyFor(press("ArrowLeft"), "normal", true)).toEqual({ kind: "key", key: "<Left>" });
    expect(viKeyFor(press("ArrowRight"), "normal", true)).toEqual({ kind: "key", key: "<Right>" });
    expect(viKeyFor(press("ArrowUp"), "normal", true)).toEqual({ kind: "key", key: "<Up>" });
    expect(viKeyFor(press("ArrowDown"), "normal", true)).toEqual({ kind: "key", key: "<Down>" });
    expect(viKeyFor(press("Tab"), "normal", true)).toEqual({ kind: "key", key: "<Tab>" });
    expect(viKeyFor(press("Tab", { shift: true }), "normal", true)).toEqual({ kind: "key", key: "<S-Tab>" });
  });

  it("takes ⌥⌫ (delete word) as a Mac command on the command line, like in Insert mode", () => {
    expect(viKeyFor(press("Backspace", { alt: true }), "normal", true)).toEqual({
      kind: "key",
      key: { command: { type: "deleteBackward", unit: "word" } },
    });
  });

  it("turns ⌘V into a paste effect on the command line, and leaves other ⌘ shortcuts to it", () => {
    expect(viKeyFor(press("v", { meta: true }), "normal", true)).toEqual({ kind: "effect", effect: "paste" });
    // ⌘C is not "paste" and not a Mac editing command either: the command line has no use for it.
    expect(viKeyFor(press("c", { meta: true }), "normal", true)).toBeNull();
  });

  it("types characters and Ctrl-r on the command line like it does everywhere else", () => {
    expect(viKeyFor(press("x"), "normal", true)).toEqual({ kind: "text" });
    expect(viKeyFor(press("ä"), "normal", true)).toEqual({ kind: "text" });
    expect(viKeyFor(press("r", { ctrl: true }), "normal", true)).toEqual({ kind: "key", key: "<C-r>" });
  });

  it("⌘A selects the whole file from Visual mode too, dropping into Normal first", () => {
    expect(viKeyFor(press("a", { meta: true }), "visual")).toEqual({ kind: "keys", notation: "<Esc>ggVG" });
    expect(viKeyFor(press("a", { meta: true }), "visualLine")).toEqual({ kind: "keys", notation: "<Esc>ggVG" });
    expect(viKeyFor(press("a", { meta: true }), "normal")).toEqual({ kind: "keys", notation: "ggVG" });
  });
});
