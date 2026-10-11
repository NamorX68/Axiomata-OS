import { describe, expect, it } from "vitest";

import { foldKeyAction, keyAction } from "../editor/keymap";
import { VI_CTRL_KEYS, editorKeyInput, type EditorKeyEvent } from "./editorKeys";

function press(key: string, held: Partial<EditorKeyEvent> = {}): EditorKeyEvent {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...held };
}

const ctrl = { ctrlKey: true };

describe("editorKeyInput on the Mac", () => {
  it("hands the press over untouched", () => {
    const e = press("ArrowLeft", { metaKey: true, shiftKey: true, keyCode: 37, code: "ArrowLeft" });
    expect(editorKeyInput(e, "mac", false)).toEqual({
      key: "ArrowLeft",
      meta: true,
      alt: false,
      shift: true,
      ctrl: false,
      keyCode: 37,
      code: "ArrowLeft",
    });
  });

  it("does not translate Ctrl, which stays Ctrl (Vi's, or nobody's)", () => {
    expect(editorKeyInput(press("ArrowLeft", ctrl), "mac", false)).toMatchObject({ ctrl: true, meta: false, alt: false });
  });
});

describe("editorKeyInput off the Mac", () => {
  const input = (e: EditorKeyEvent, vi = false) => editorKeyInput(e, "other", vi);

  it("moves and deletes by words on Ctrl, not by lines", () => {
    expect(input(press("ArrowLeft", ctrl))).toMatchObject({ alt: true, meta: false, ctrl: false });
    expect(input(press("ArrowRight", { ...ctrl, shiftKey: true }))).toMatchObject({ alt: true, shift: true, ctrl: false });
    expect(input(press("Backspace", ctrl))).toMatchObject({ alt: true, meta: false, ctrl: false });
    expect(input(press("Delete", ctrl))).toMatchObject({ alt: true, meta: false, ctrl: false });
  });

  it("goes to the document's ends on Ctrl+Home/End", () => {
    expect(input(press("Home", ctrl))).toMatchObject({ meta: true, alt: false, ctrl: false });
    expect(input(press("End", ctrl))).toMatchObject({ meta: true, alt: false, ctrl: false });
  });

  it("leaves Home/End, Alt+↑/↓ and the plain keys as they are", () => {
    expect(input(press("Home"))).toMatchObject({ meta: false, alt: false, ctrl: false });
    expect(input(press("ArrowUp", { altKey: true }))).toMatchObject({ alt: true, meta: false });
    expect(input(press("a"))).toMatchObject({ meta: false, alt: false, ctrl: false });
  });

  it("does not turn Ctrl+↑/↓ into ⌘↑/↓ (the document's ends)", () => {
    expect(input(press("ArrowUp", ctrl))).toMatchObject({ meta: false, ctrl: true });
    expect(input(press("ArrowDown", ctrl))).toMatchObject({ meta: false, ctrl: true });
  });

  it("keeps Ctrl+Alt+↑/↓ as add-cursor and gives Ctrl+Alt+←/→ no meaning", () => {
    const up = input(press("ArrowUp", { ...ctrl, altKey: true }));
    expect(up).toMatchObject({ meta: true, alt: true, ctrl: false });
    expect(keyAction(up)).toEqual(keyAction(editorKeyInput(press("ArrowUp", { metaKey: true, altKey: true }), "mac", false)));
    expect(keyAction(input(press("ArrowLeft", { ...ctrl, altKey: true })))).toBeNull();
  });

  it("turns every other Ctrl shortcut into its ⌘ twin, Shift and Alt kept", () => {
    expect(input(press("s", ctrl))).toMatchObject({ meta: true, ctrl: false, shift: false, alt: false });
    expect(input(press("z", { ...ctrl, shiftKey: true }))).toMatchObject({ meta: true, ctrl: false, shift: true });
    expect(input(press("f", { ...ctrl, altKey: true }))).toMatchObject({ meta: true, alt: true, ctrl: false });
  });

  it("folds and unfolds on Ctrl+Shift+[ and Ctrl+Shift+], by the physical key", () => {
    const fold = input(press("{", { ...ctrl, shiftKey: true, code: "BracketLeft" }));
    expect(fold).toMatchObject({ key: "[", meta: true, alt: true, shift: false, ctrl: false });
    expect(foldKeyAction(fold)).toBe("fold");
    expect(foldKeyAction(input(press("}", { ...ctrl, shiftKey: true, code: "BracketRight" })))).toBe("unfold");
    expect(foldKeyAction(input(press("0", { ...ctrl, altKey: true })))).toBe("foldAll");
    expect(foldKeyAction(input(press("j", { ...ctrl, altKey: true })))).toBe("unfoldAll");
  });

  it("reaches the same command the Mac key does", () => {
    const mac = (e: EditorKeyEvent) => keyAction(editorKeyInput(e, "mac", false));
    const other = (e: EditorKeyEvent) => keyAction(input(e));
    expect(other(press("ArrowLeft", ctrl))).toEqual(mac(press("ArrowLeft", { altKey: true })));
    expect(other(press("Backspace", ctrl))).toEqual(mac(press("Backspace", { altKey: true })));
    expect(other(press("Home", ctrl))).toEqual(mac(press("Home", { metaKey: true })));
    expect(other(press("s", ctrl))).toEqual(mac(press("s", { metaKey: true })));
    expect(other(press("ArrowLeft", ctrl))).not.toBeNull();
    expect(other(press("s", ctrl))).not.toBeNull();
  });

  it("shows plain typing for AltGr and for the Super key", () => {
    const altGr = { ...ctrl, altKey: true, getModifierState: () => true };
    expect(input(press("@", altGr))).toMatchObject({ ctrl: false, alt: false, meta: false });
    expect(keyAction(input(press("\\", { ...ctrl, altKey: true })))).toBeNull();
    expect(input(press("k", { metaKey: true }))).toMatchObject({ meta: false, ctrl: false, key: "Unidentified" });
  });
});

describe("editorKeyInput off the Mac with Vi on", () => {
  const input = (e: EditorKeyEvent) => editorKeyInput(e, "other", true);

  it("knows the Ctrl letters of the grammar", () => {
    for (const key of ["r", "v", "o", "i", "f", "b", "d", "u", "a", "x", "e", "y", "n", "p", "c", "["]) {
      expect(VI_CTRL_KEYS.has(key), key).toBe(true);
    }
    expect(VI_CTRL_KEYS.has("z")).toBe(false);
    expect(VI_CTRL_KEYS.has("s")).toBe(false);
  });

  it("leaves a Vi Ctrl letter to Vi", () => {
    for (const key of VI_CTRL_KEYS) expect(input(press(key, ctrl)), key).toMatchObject({ ctrl: true, meta: false });
  });

  it("gives the app's command on such a letter the Ctrl+Shift form", () => {
    expect(input(press("C", { ...ctrl, shiftKey: true }))).toMatchObject({ meta: true, ctrl: false, shift: false });
    expect(input(press("V", { ...ctrl, shiftKey: true }))).toMatchObject({ meta: true, ctrl: false, shift: false });
  });

  it("does not let Vi take a Ctrl+Alt chord: find and replace stays reachable", () => {
    expect(input(press("f", { ...ctrl, altKey: true }))).toMatchObject({ meta: true, alt: true, ctrl: false });
  });

  it("still gives a letter Vi does not know its plain Ctrl form", () => {
    expect(input(press("z", ctrl))).toMatchObject({ meta: true, ctrl: false });
    expect(input(press("s", ctrl))).toMatchObject({ meta: true, ctrl: false });
    expect(input(press("z", { ...ctrl, shiftKey: true }))).toMatchObject({ meta: true, shift: true });
  });

  it("moves by words and ends the same as without Vi", () => {
    expect(input(press("ArrowLeft", ctrl))).toMatchObject({ alt: true, meta: false });
    expect(input(press("Home", ctrl))).toMatchObject({ meta: true });
  });
});
