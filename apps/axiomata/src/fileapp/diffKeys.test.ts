import { describe, expect, it } from "vitest";

import { diffHunkAction, diffViewAction } from "./diffKeys";
import type { KeyLike } from "../core/keys";

function press(key: string, held: Partial<KeyLike> = {}): KeyLike {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...held };
}

const cmd = { metaKey: true };
const ctrl = { ctrlKey: true };
const ctrlShift = { ctrlKey: true, shiftKey: true };

describe("diffViewAction", () => {
  it("keeps the Mac's ⌥⌘↓/↑, ⌘R and ⇧⌘D", () => {
    expect(diffViewAction(press("ArrowDown", { ...cmd, altKey: true }), "mac")).toBe("next-file");
    expect(diffViewAction(press("ArrowUp", { ...cmd, altKey: true, shiftKey: true }), "mac")).toBe("previous-file");
    expect(diffViewAction(press("r", cmd), "mac")).toBe("refresh");
    expect(diffViewAction(press("D", { ...cmd, shiftKey: true }), "mac")).toBe("layout");
  });

  it("flips the layout on ⌥⇧⌘D as before", () => {
    expect(diffViewAction(press("D", { ...cmd, altKey: true, shiftKey: true }), "mac")).toBe("layout");
  });

  it("does not take the Mac keys it never had", () => {
    expect(diffViewAction(press("R", { ...cmd, shiftKey: true }), "mac")).toBeNull();
    expect(diffViewAction(press("d", cmd), "mac")).toBeNull();
    expect(diffViewAction(press("r", ctrl), "mac")).toBeNull();
    expect(diffViewAction(press("r", { ...cmd, altKey: true }), "mac")).toBeNull();
  });

  it("uses Ctrl+Alt+↓/↑, Ctrl+Shift+R and Ctrl+Shift+D elsewhere, leaving Vi's Ctrl+R/D alone", () => {
    expect(diffViewAction(press("ArrowDown", { ...ctrl, altKey: true }), "other")).toBe("next-file");
    expect(diffViewAction(press("R", ctrlShift), "other")).toBe("refresh");
    expect(diffViewAction(press("D", ctrlShift), "other")).toBe("layout");
    expect(diffViewAction(press("r", ctrl), "other")).toBeNull();
    expect(diffViewAction(press("d", ctrl), "other")).toBeNull();
  });

  it("never reads AltGr as a command", () => {
    const altGr = { ...ctrl, altKey: true, getModifierState: () => true };
    expect(diffViewAction(press("ArrowDown", altGr), "other")).toBeNull();
  });
});

describe("diffHunkAction", () => {
  it("steps hunks on ⌥↓/⌥↑ and opens on a plain ⏎, on every platform", () => {
    for (const on of ["mac", "other"] as const) {
      expect(diffHunkAction(press("ArrowDown", { altKey: true }), on)).toBe("next-hunk");
      expect(diffHunkAction(press("ArrowUp", { altKey: true }), on)).toBe("previous-hunk");
      expect(diffHunkAction(press("Enter"), on)).toBe("open");
      expect(diffHunkAction(press("Enter", { shiftKey: true }), on)).toBeNull();
    }
  });

  it("discards a hunk on ⌘⌫, and on Ctrl+Shift+⌫ elsewhere but never on Ctrl+⌫ (delete a word)", () => {
    expect(diffHunkAction(press("Backspace", cmd), "mac")).toBe("discard-hunk");
    expect(diffHunkAction(press("Backspace", { ...cmd, shiftKey: true }), "mac")).toBeNull();
    expect(diffHunkAction(press("Backspace", ctrlShift), "other")).toBe("discard-hunk");
    expect(diffHunkAction(press("Backspace", ctrl), "other")).toBeNull();
    expect(diffHunkAction(press("Backspace"), "other")).toBeNull();
  });
});
