import { describe, expect, it } from "vitest";

import { keyAction, type KeyInput } from "./keymap";

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
