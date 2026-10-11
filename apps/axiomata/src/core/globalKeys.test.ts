import { describe, expect, it } from "vitest";

import { globalKeyAction, type GlobalKeyEvent } from "./globalKeys";

function press(key: string, held: Partial<GlobalKeyEvent> = {}): GlobalKeyEvent {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...held };
}

describe("globalKeyAction on the Mac", () => {
  it("maps ⌘K, ⌘, and ⌘W", () => {
    expect(globalKeyAction(press("k", { metaKey: true }), "mac")).toBe("spotlight");
    expect(globalKeyAction(press("K", { metaKey: true }), "mac")).toBe("spotlight");
    expect(globalKeyAction(press(",", { metaKey: true }), "mac")).toBe("settings");
    expect(globalKeyAction(press("w", { metaKey: true }), "mac")).toBe("close");
  });

  it("leaves Ctrl to the terminal", () => {
    expect(globalKeyAction(press("k", { ctrlKey: true }), "mac")).toBeNull();
    expect(globalKeyAction(press("w", { ctrlKey: true }), "mac")).toBeNull();
  });

  it("ignores a key with Shift or ⌥ held, and keys without a modifier", () => {
    expect(globalKeyAction(press("w", { metaKey: true, shiftKey: true }), "mac")).toBeNull();
    expect(globalKeyAction(press(",", { metaKey: true, altKey: true }), "mac")).toBeNull();
    expect(globalKeyAction(press("k"), "mac")).toBeNull();
  });

  it("is the same inside a terminal pane", () => {
    expect(globalKeyAction(press("k", { metaKey: true }), "mac", true)).toBe("spotlight");
    expect(globalKeyAction(press("k", { ctrlKey: true }), "mac", true)).toBeNull();
  });
});

describe("globalKeyAction off the Mac", () => {
  it("uses Ctrl for the spotlight and the settings", () => {
    expect(globalKeyAction(press("k", { ctrlKey: true }), "other")).toBe("spotlight");
    expect(globalKeyAction(press(",", { ctrlKey: true }), "other")).toBe("settings");
  });

  it("closes on Ctrl+Shift+W and never on Ctrl+W", () => {
    expect(globalKeyAction(press("W", { ctrlKey: true, shiftKey: true }), "other")).toBe("close");
    expect(globalKeyAction(press("w", { ctrlKey: true }), "other")).toBeNull();
  });

  it("ignores ⌘/Super, Alt and a Shift that makes it another key", () => {
    expect(globalKeyAction(press("k", { metaKey: true }), "other")).toBeNull();
    expect(globalKeyAction(press("k", { ctrlKey: true, altKey: true }), "other")).toBeNull();
    expect(globalKeyAction(press("k", { ctrlKey: true, shiftKey: true }), "other")).toBeNull();
  });

  it("opens the settings from a terminal although Shift makes the comma a `<`", () => {
    const keys = { ctrlKey: true, shiftKey: true, code: "Comma" };
    expect(globalKeyAction(press("<", keys), "other", true)).toBe("settings");
    expect(globalKeyAction(press("<", { ...keys, metaKey: true, ctrlKey: false }), "mac", true)).toBeNull();
  });

  it("never reads AltGr as a shortcut", () => {
    const altGr = { ctrlKey: true, altKey: true, getModifierState: () => true };
    expect(globalKeyAction(press("k", altGr), "other")).toBeNull();
    expect(globalKeyAction(press(",", { ctrlKey: true, altKey: true }), "other")).toBeNull();
  });

  it("gives a terminal pane the shell's Ctrl keys: its app keys take Ctrl+Shift", () => {
    expect(globalKeyAction(press("k", { ctrlKey: true }), "other", true)).toBeNull();
    expect(globalKeyAction(press("w", { ctrlKey: true }), "other", true)).toBeNull();
    expect(globalKeyAction(press("K", { ctrlKey: true, shiftKey: true }), "other", true)).toBe("spotlight");
    expect(globalKeyAction(press("W", { ctrlKey: true, shiftKey: true }), "other", true)).toBe("close");
  });
});
