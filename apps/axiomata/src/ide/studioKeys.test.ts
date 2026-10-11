import { describe, expect, it } from "vitest";

import { studioKeyAction, type StudioKeyEvent } from "./studioKeys";

function press(key: string, held: Partial<StudioKeyEvent> = {}): StudioKeyEvent {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...held };
}

const cmd = { metaKey: true };
const ctrl = { ctrlKey: true };
const ctrlShift = { ctrlKey: true, shiftKey: true };

describe("studioKeyAction on the Mac", () => {
  it("keeps every key it had", () => {
    expect(studioKeyAction(press("p", cmd), "mac")).toEqual({ kind: "quick-open" });
    expect(studioKeyAction(press("n", cmd), "mac")).toEqual({ kind: "new-note" });
    expect(studioKeyAction(press("o", cmd), "mac")).toEqual({ kind: "open-file" });
    expect(studioKeyAction(press("b", cmd), "mac")).toEqual({ kind: "toggle-tree" });
    expect(studioKeyAction(press("w", cmd), "mac")).toEqual({ kind: "close-file" });
    expect(studioKeyAction(press("F", { ...cmd, shiftKey: true }), "mac")).toEqual({ kind: "search" });
    expect(studioKeyAction(press("3", cmd), "mac")).toEqual({ kind: "tab", number: 3 });
  });

  it("leaves an unshifted AZERTY `&` alone: only a digit key is a tab", () => {
    expect(studioKeyAction(press("&", { ...cmd, code: "Digit1" }), "mac")).toBeNull();
  });

  it("splits right on ⌘\\ and to the bottom on ⇧⌘\\", () => {
    expect(studioKeyAction(press("\\", { ...cmd, code: "Backslash" }), "mac")).toEqual({ kind: "split", where: "right" });
    const shifted = { ...cmd, code: "Backslash", shiftKey: true };
    expect(studioKeyAction(press("|", shifted), "mac")).toEqual({ kind: "split", where: "bottom" });
  });

  it("cycles tabs on Ctrl+Tab and back on Ctrl+Shift+Tab", () => {
    expect(studioKeyAction(press("Tab", ctrl), "mac")).toEqual({ kind: "cycle-tab", step: 1 });
    expect(studioKeyAction(press("Tab", ctrlShift), "mac")).toEqual({ kind: "cycle-tab", step: -1 });
  });

  it("does not take Shift or ⌥ forms it never had, nor Ctrl letters", () => {
    expect(studioKeyAction(press("p", { ...cmd, shiftKey: true }), "mac")).toBeNull();
    expect(studioKeyAction(press("f", cmd), "mac")).toBeNull();
    expect(studioKeyAction(press("1", { ...cmd, shiftKey: true, code: "Digit1" }), "mac")).toBeNull();
    expect(studioKeyAction(press("p", { ...cmd, altKey: true }), "mac")).toBeNull();
    expect(studioKeyAction(press("p", ctrl), "mac")).toBeNull();
    expect(studioKeyAction(press("k", cmd), "mac")).toBeNull();
  });
});

describe("studioKeyAction off the Mac", () => {
  it("takes Ctrl+Shift for the letter commands, which Vi and the shell would otherwise own", () => {
    expect(studioKeyAction(press("P", ctrlShift), "other")).toEqual({ kind: "quick-open" });
    expect(studioKeyAction(press("N", ctrlShift), "other")).toEqual({ kind: "new-note" });
    expect(studioKeyAction(press("O", ctrlShift), "other")).toEqual({ kind: "open-file" });
    expect(studioKeyAction(press("B", ctrlShift), "other")).toEqual({ kind: "toggle-tree" });
    expect(studioKeyAction(press("W", ctrlShift), "other")).toEqual({ kind: "close-file" });
    expect(studioKeyAction(press("F", ctrlShift), "other")).toEqual({ kind: "search" });
  });

  it("leaves plain Ctrl+letter to Vi and the editor", () => {
    for (const key of ["p", "n", "o", "b", "w", "f"]) expect(studioKeyAction(press(key, ctrl), "other"), key).toBeNull();
  });

  it("keeps digits and backslash on plain Ctrl", () => {
    expect(studioKeyAction(press("2", ctrl), "other")).toEqual({ kind: "tab", number: 2 });
    expect(studioKeyAction(press("\\", { ...ctrl, code: "Backslash" }), "other")).toEqual({ kind: "split", where: "right" });
    const shifted = { ...ctrlShift, code: "Backslash" };
    expect(studioKeyAction(press("|", shifted), "other")).toEqual({ kind: "split", where: "bottom" });
  });

  it("does not read ⌘/Super, or AltGr, as a command", () => {
    expect(studioKeyAction(press("p", { metaKey: true, shiftKey: true }), "other")).toBeNull();
    const altGr = { ...ctrlShift, altKey: true, getModifierState: () => true };
    expect(studioKeyAction(press("p", altGr), "other")).toBeNull();
  });

  it("gives a terminal its Ctrl keys: app keys there are Ctrl+Shift, and plain Ctrl+letter is the shell's", () => {
    expect(studioKeyAction(press("p", ctrl), "other", true)).toBeNull();
    expect(studioKeyAction(press("2", ctrl), "other", true)).toBeNull();
    expect(studioKeyAction(press("P", ctrlShift), "other", true)).toEqual({ kind: "quick-open" });
    expect(studioKeyAction(press("@", { ...ctrlShift, code: "Digit2" }), "other", true)).toEqual({ kind: "tab", number: 2 });
    expect(studioKeyAction(press("|", { ...ctrlShift, code: "Backslash" }), "other", true)).toEqual({
      kind: "split",
      where: "right",
    });
  });
});
