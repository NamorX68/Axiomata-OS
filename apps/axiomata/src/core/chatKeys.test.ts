import { describe, expect, it } from "vitest";

import { isNewSessionPress } from "./chatKeys";

const press = (key: string, mods: Partial<KeyboardEvent> = {}) =>
  ({ key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods }) as KeyboardEvent;

describe("the chat input's keys", () => {
  it("takes ⌘N on the Mac and nothing else held with it", () => {
    expect(isNewSessionPress(press("n", { metaKey: true }), "mac")).toBe(true);
    expect(isNewSessionPress(press("N", { metaKey: true, shiftKey: true }), "mac")).toBe(false);
    expect(isNewSessionPress(press("n", { metaKey: true, altKey: true }), "mac")).toBe(false);
    expect(isNewSessionPress(press("n", { ctrlKey: true }), "mac")).toBe(false);
    expect(isNewSessionPress(press("n"), "mac")).toBe(false);
  });

  it("takes Ctrl+Shift+N off the Mac, leaving plain Ctrl+N alone", () => {
    expect(isNewSessionPress(press("N", { ctrlKey: true, shiftKey: true }), "other")).toBe(true);
    expect(isNewSessionPress(press("n", { ctrlKey: true }), "other")).toBe(false);
    expect(isNewSessionPress(press("n", { metaKey: true }), "other")).toBe(false);
    expect(isNewSessionPress(press("N", { ctrlKey: true, altKey: true, shiftKey: true }), "other"), "AltGr").toBe(false);
  });
});
