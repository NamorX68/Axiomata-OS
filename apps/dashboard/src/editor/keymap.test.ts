import { describe, expect, it } from "vitest";

import { findKeyAction, foldKeyAction, keyAction, type KeyInput } from "./keymap";

function key(k: string, mods: Partial<Omit<KeyInput, "key">> = {}): KeyInput {
  return { key: k, meta: false, alt: false, shift: false, ctrl: false, ...mods };
}

describe("keyAction", () => {
  it("maps arrows with ⌥, ⌘ and ⇧", () => {
    expect(keyAction(key("ArrowLeft"))).toEqual({ command: { type: "move", motion: "charLeft", extend: false } });
    expect(keyAction(key("ArrowLeft", { alt: true, shift: true }))).toEqual({
      command: { type: "move", motion: "wordLeft", extend: true },
    });
    expect(keyAction(key("ArrowRight", { meta: true }))).toEqual({
      command: { type: "move", motion: "lineEnd", extend: false },
    });
    expect(keyAction(key("ArrowUp", { meta: true, shift: true }))).toEqual({
      command: { type: "move", motion: "docStart", extend: true },
    });
  });

  it("moves and duplicates lines with ⌥↑/↓ and ⇧⌥↑/↓", () => {
    expect(keyAction(key("ArrowUp", { alt: true }))).toEqual({ command: { type: "moveLines", dir: -1 } });
    expect(keyAction(key("ArrowDown", { alt: true, shift: true }))).toEqual({
      command: { type: "duplicateLines", dir: 1 },
    });
  });

  it("maps the deletion keys", () => {
    expect(keyAction(key("Backspace", { meta: true }))).toEqual({ command: { type: "deleteBackward", unit: "line" } });
    expect(keyAction(key("Backspace", { alt: true }))).toEqual({ command: { type: "deleteBackward", unit: "word" } });
    expect(keyAction(key("Delete"))).toEqual({ command: { type: "deleteForward", unit: "char" } });
  });

  it("maps the ⌘ shortcuts by the letter printed on the key", () => {
    expect(keyAction(key("z", { meta: true }))).toEqual({ command: { type: "undo" } });
    expect(keyAction(key("Z", { meta: true, shift: true }))).toEqual({ command: { type: "redo" } });
    expect(keyAction(key("s", { meta: true }))).toEqual({ effect: "save" });
    expect(keyAction(key("v", { meta: true }))).toEqual({ effect: "paste" });
    expect(keyAction(key("V", { meta: true, shift: true }))).toEqual({ effect: "togglePreview" });
    expect(keyAction(key("l", { meta: true }))).toEqual({ command: { type: "selectLine" } });
    expect(keyAction(key("/", { meta: true, shift: true }))).toEqual({ command: { type: "toggleComment" } });
  });

  it("toggles wrapping with ⌥Z, which types Ω on a Mac", () => {
    expect(keyAction(key("Ω", { alt: true }))).toEqual({ effect: "toggleWrap" });
    expect(keyAction(key("z", { alt: true }))).toEqual({ effect: "toggleWrap" });
    expect(keyAction(key("Ω"))).toBeNull();
    // A layout where ⌥Z is a dead key (¨): only keyCode tells it apart.
    expect(keyAction(key("Dead", { alt: true, keyCode: 90 }))).toEqual({ effect: "toggleWrap" });
    expect(keyAction(key("Dead", { alt: true, keyCode: 85 }))).toBeNull();
  });

  it("leaves plain typing, ⌃ and unknown ⌘ keys alone", () => {
    expect(keyAction(key("a"))).toBeNull();
    expect(keyAction(key("ä", { shift: true }))).toBeNull();
    expect(keyAction(key("a", { ctrl: true }))).toBeNull();
    expect(keyAction(key("k", { meta: true }))).toBeNull();
    expect(keyAction(key("w", { meta: true }))).toBeNull();
  });

  it("maps ⇥, ⇧⇥ and ↩", () => {
    expect(keyAction(key("Tab"))).toEqual({ command: { type: "indent" } });
    expect(keyAction(key("Tab", { shift: true }))).toEqual({ command: { type: "outdent" } });
    expect(keyAction(key("Enter"))).toEqual({ command: { type: "newline" } });
  });
});

describe("more cursors (ED5, T6)", () => {
  it("binds ⌥⌘↑/↓, ⌘D, ⌘U, ⇧⌘L and Esc", () => {
    expect(keyAction(key("ArrowUp", { alt: true, meta: true }))).toEqual({
      command: { type: "addCursorVertical", dir: -1 },
    });
    expect(keyAction(key("ArrowDown", { alt: true, meta: true }))).toEqual({
      command: { type: "addCursorVertical", dir: 1 },
    });
    expect(keyAction(key("d", { meta: true }))).toEqual({ command: { type: "addNextOccurrence" } });
    expect(keyAction(key("u", { meta: true }))).toEqual({ command: { type: "removeLastCursor" } });
    expect(keyAction(key("l", { meta: true, shift: true }))).toEqual({ command: { type: "selectAllOccurrences" } });
    expect(keyAction(key("l", { meta: true }))).toEqual({ command: { type: "selectLine" } });
    expect(keyAction(key("Escape"))).toEqual({ command: { type: "singleCursor" } });
    // ⌥↑ still moves the line.
    expect(keyAction(key("ArrowUp", { alt: true }))).toEqual({ command: { type: "moveLines", dir: -1 } });
  });
});

describe("findKeyAction", () => {
  it("maps ⌘F, ⌥⌘F, ⌘G, ⇧⌘G and ⌘E", () => {
    expect(findKeyAction(key("f", { meta: true }))).toBe("find");
    expect(findKeyAction(key("ƒ", { meta: true, alt: true }))).toBe("findReplace");
    // A layout where ⌥F is something else: the key code still says F.
    expect(findKeyAction(key("Dead", { meta: true, alt: true, keyCode: 70 }))).toBe("findReplace");
    expect(findKeyAction(key("g", { meta: true }))).toBe("findNext");
    expect(findKeyAction(key("G", { meta: true, shift: true }))).toBe("findPrevious");
    expect(findKeyAction(key("e", { meta: true }))).toBe("useSelectionForFind");
    expect(keyAction(key("f", { meta: true }))).toEqual({ effect: "find" });
  });

  it("leaves ⇧⌘F to the project search and plain letters to typing", () => {
    expect(findKeyAction(key("F", { meta: true, shift: true }))).toBeNull();
    expect(findKeyAction(key("f"))).toBeNull();
    expect(findKeyAction(key("f", { meta: true, ctrl: true }))).toBeNull();
  });

  it("leaves every other ⌥⌘ combination alone, and ⇧⌘E too", () => {
    expect(findKeyAction(key("g", { meta: true, alt: true }))).toBeNull();
    expect(findKeyAction(key("e", { meta: true, alt: true }))).toBeNull();
    expect(findKeyAction(key("E", { meta: true, shift: true }))).toBeNull();
  });
});

describe("foldKeyAction (ED5, T7)", () => {
  const fold = { meta: true, alt: true };

  it("reads ⌥⌘[ ⌥⌘] ⌥⌘0 ⌥⌘J as typed, and as ⌥ turns them on a US layout", () => {
    expect(foldKeyAction(key("[", fold))).toBe("fold");
    expect(foldKeyAction(key("“", fold))).toBe("fold");
    expect(foldKeyAction(key("‘", fold))).toBe("unfold");
    expect(foldKeyAction(key("º", fold))).toBe("foldAll");
    expect(foldKeyAction(key("∆", fold))).toBe("unfoldAll");
  });

  it("recognises the physical key when the layout puts something else there", () => {
    expect(foldKeyAction(key("ü", { ...fold, code: "BracketLeft" }))).toBe("fold");
    expect(foldKeyAction(key("+", { ...fold, code: "BracketRight" }))).toBe("unfold");
  });

  it("needs ⌥⌘ and nothing else", () => {
    expect(foldKeyAction(key("[", { meta: true }))).toBeNull();
    expect(foldKeyAction(key("[", { ...fold, shift: true }))).toBeNull();
    expect(foldKeyAction(key("j", { ...fold, ctrl: true }))).toBeNull();
    expect(foldKeyAction(key("x", fold))).toBeNull();
  });

  it("reads a capital J (Caps Lock) as unfold all, and needs ⌘ as well as ⌥", () => {
    expect(foldKeyAction(key("J", fold))).toBe("unfoldAll");
    expect(foldKeyAction(key("[", { alt: true, code: "BracketLeft" }))).toBeNull();
  });
});
