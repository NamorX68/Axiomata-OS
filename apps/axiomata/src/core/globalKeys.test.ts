import { describe, expect, it } from "vitest";

import { globalKeyAction, type GlobalKeyEvent } from "./globalKeys";

function press(key: string, held: Partial<GlobalKeyEvent> = {}): GlobalKeyEvent {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...held };
}

describe("globalKeyAction", () => {
  it("maps ⌘K, ⌘, and ⌘W on a Mac", () => {
    expect(globalKeyAction(press("k", { metaKey: true }), true)).toBe("spotlight");
    expect(globalKeyAction(press("K", { metaKey: true }), true)).toBe("spotlight");
    expect(globalKeyAction(press(",", { metaKey: true }), true)).toBe("settings");
    expect(globalKeyAction(press("w", { metaKey: true }), true)).toBe("close");
  });

  it("leaves Ctrl to the terminal on a Mac", () => {
    expect(globalKeyAction(press("k", { ctrlKey: true }), true)).toBeNull();
    expect(globalKeyAction(press("w", { ctrlKey: true }), true)).toBeNull();
  });

  it("ignores a key with Shift or ⌥ held, and keys without a modifier", () => {
    expect(globalKeyAction(press("w", { metaKey: true, shiftKey: true }), true)).toBeNull();
    expect(globalKeyAction(press(",", { metaKey: true, altKey: true }), true)).toBeNull();
    expect(globalKeyAction(press("k"), true)).toBeNull();
  });

  it("uses Ctrl off the Mac but never takes Ctrl+W there", () => {
    expect(globalKeyAction(press("k", { ctrlKey: true }), false)).toBe("spotlight");
    expect(globalKeyAction(press(",", { ctrlKey: true }), false)).toBe("settings");
    expect(globalKeyAction(press("w", { ctrlKey: true }), false)).toBeNull();
  });
});
