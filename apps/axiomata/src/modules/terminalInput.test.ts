import { describe, expect, it } from "vitest";

import type { KeyLike } from "../core/keys";
import { encodeKey, PLAIN_KEYS, terminalKeyRoute, type KeyPress } from "./terminalInput";

describe("encodeKey", () => {
  const press = (key: string, held: Partial<KeyPress> = {}): KeyPress => ({
    key,
    code: "",
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...held,
  });
  const text = (bytes: Uint8Array | null) => (bytes ? new TextDecoder().decode(bytes) : null);
  const plain = (key: string, held: Partial<KeyPress> = {}) => text(encodeKey(press(key, held), PLAIN_KEYS));
  const kitty = (key: string, held: Partial<KeyPress> = {}) =>
    text(encodeKey(press(key, held), { appCursorKeys: false, kittyFlags: 1 }));

  it("sends the control bytes a shell's line editor expects", () => {
    expect(plain("a", { ctrlKey: true, code: "KeyA" })).toBe("\x01");
    expect(plain("C", { ctrlKey: true, shiftKey: true, code: "KeyC" })).toBe("\x03");
    expect(plain("[", { ctrlKey: true })).toBe("\x1b");
    expect(plain(" ", { ctrlKey: true, code: "Space" })).toBe("\x00");
    expect(plain("Enter")).toBe("\r");
    expect(plain("Backspace")).toBe("\x7f");
    expect(plain("Tab")).toBe("\t");
    expect(plain("Tab", { shiftKey: true })).toBe("\x1b[Z");
    expect(plain("Escape")).toBe("\x1b");
    expect(plain("Delete")).toBe("\x1b[3~");
    expect(plain("PageUp")).toBe("\x1b[5~");
  });

  it("leaves printable text and modifier keys alone to the input event", () => {
    expect(plain("a", { code: "KeyA" })).toBeNull();
    expect(plain("A", { shiftKey: true, code: "KeyA" })).toBeNull();
    expect(plain("Shift")).toBeNull();
    expect(plain("k", { metaKey: true, code: "KeyK" })).toBeNull();
  });

  it("puts modifiers on cursor, editing and F-keys and honours the cursor keys' application mode", () => {
    expect(plain("ArrowUp")).toBe("\x1b[A");
    expect(text(encodeKey(press("ArrowUp"), { appCursorKeys: true, kittyFlags: 0 }))).toBe("\x1bOA");
    expect(plain("ArrowRight", { ctrlKey: true })).toBe("\x1b[1;5C");
    expect(plain("ArrowUp", { altKey: true })).toBe("\x1b[1;3A");
    expect(plain("Home", { shiftKey: true })).toBe("\x1b[1;2H");
    expect(plain("Delete", { ctrlKey: true })).toBe("\x1b[3;5~");
    expect(plain("F1")).toBe("\x1bOP");
    expect(plain("F2", { shiftKey: true })).toBe("\x1b[1;2Q");
    expect(plain("F5")).toBe("\x1b[15~");
    expect(plain("F12", { ctrlKey: true })).toBe("\x1b[24;5~");
  });

  it("tells Shift, Alt and Ctrl with Enter and Backspace apart", () => {
    expect(plain("Enter", { shiftKey: true })).toBe("\x1b\r");
    expect(plain("Enter", { altKey: true })).toBe("\x1b\r");
    expect(plain("Backspace", { altKey: true })).toBe("\x1b\x7f");
    expect(plain("Backspace", { ctrlKey: true })).toBe("\x08");
    expect(kitty("Enter", { shiftKey: true })).toBe("\x1b[13;2u");
    expect(kitty("Enter", { ctrlKey: true })).toBe("\x1b[13;5u");
    expect(kitty("Enter"), "plain Enter stays legacy under the kitty protocol").toBe("\r");
  });

  it("uses Option as Alt only where it composes nothing (German layout: Option+L is @)", () => {
    expect(plain("@", { altKey: true, code: "KeyL" })).toBeNull();
    expect(plain("∫", { altKey: true, code: "KeyB" })).toBeNull();
    expect(plain("b", { altKey: true, code: "KeyB" })).toBe("\x1bb");
    expect(kitty("b", { altKey: true, code: "KeyB" })).toBe("\x1b[98;3u");
  });

  it("disambiguates under the kitty protocol: Escape and Ctrl+letter as CSI u", () => {
    expect(kitty("Escape")).toBe("\x1b[27u");
    expect(kitty("c", { ctrlKey: true, code: "KeyC" })).toBe("\x1b[99;5u");
    expect(kitty("Tab", { shiftKey: true }), "Shift+Tab stays CSI Z, which crossterm reads as BackTab").toBe("\x1b[Z");
  });

  it("reads Ctrl with Option by the key's plain letter, and keeps Ctrl+Shift+Tab as CSI Z", () => {
    expect(plain("ﬁ", { ctrlKey: true, altKey: true, code: "KeyF" })).toBe("\x1b\x06");
    expect(plain("Tab", { ctrlKey: true, shiftKey: true })).toBe("\x1b[Z");
    expect(kitty("Backspace", { altKey: true })).toBe("\x1b[127;3u");
  });

  it("leaves characters to the input event while AltGr types them", () => {
    expect(text(encodeKey(press("@", { ctrlKey: true, altKey: true, code: "KeyQ" }), PLAIN_KEYS, true))).toBeNull();
  });
});

describe("terminalKeyRoute", () => {
  const press = (key: string, held: Partial<KeyLike> = {}): KeyLike => ({
    key,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...held,
  });
  const ctrl = { ctrlKey: true };
  const ctrlShift = { ctrlKey: true, shiftKey: true };

  it("leaves everything to the shell on the Mac, as before", () => {
    expect(terminalKeyRoute(press("c", ctrl), "mac")).toBe("shell");
    expect(terminalKeyRoute(press("C", ctrlShift), "mac")).toBe("shell");
    expect(terminalKeyRoute(press("k", { metaKey: true }), "mac")).toBe("shell");
  });

  it("gives every plain Ctrl key to the shell elsewhere, Ctrl+C included", () => {
    expect(terminalKeyRoute(press("c", ctrl), "other")).toBe("shell");
    expect(terminalKeyRoute(press("w", ctrl), "other")).toBe("shell");
    expect(terminalKeyRoute(press("k", ctrl), "other")).toBe("shell");
    expect(terminalKeyRoute(press("a"), "other")).toBe("shell");
  });

  it("copies and pastes on Ctrl+Shift+C / Ctrl+Shift+V", () => {
    expect(terminalKeyRoute(press("C", ctrlShift), "other")).toBe("copy");
    expect(terminalKeyRoute(press("V", ctrlShift), "other")).toBe("paste");
  });

  it("keeps the other Ctrl+Shift keys from the shell: they are the app's", () => {
    for (const key of ["K", "P", "W", "F", "N"]) expect(terminalKeyRoute(press(key, ctrlShift), "other"), key).toBe("app");
  });

  it("reads AltGr as typing, so the character is not turned into a control byte", () => {
    expect(terminalKeyRoute(press("@", { ...ctrl, altKey: true }), "other")).toBe("typing");
    expect(terminalKeyRoute(press("q", { ...ctrl, altKey: true, getModifierState: () => true }), "other")).toBe("typing");
  });
});
