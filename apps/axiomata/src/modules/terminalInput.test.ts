import { describe, expect, it } from "vitest";

import { keyToBytes } from "./terminalInput";

describe("keyToBytes", () => {
  it("maps Ctrl+letter to its C0 control byte (Ctrl+A..Z -> 0x01..0x1a)", () => {
    expect(keyToBytes("a", true)).toEqual(new Uint8Array([0x01]));
    expect(keyToBytes("A", true)).toEqual(new Uint8Array([0x01]));
    expect(keyToBytes("c", true)).toEqual(new Uint8Array([0x03]));
    expect(keyToBytes("z", true)).toEqual(new Uint8Array([0x1a]));
  });

  it("does not map Ctrl+non-letter", () => {
    expect(keyToBytes("1", true)).toBeNull();
    expect(keyToBytes("Enter", true)).not.toBeNull(); // Enter is its own case, not Ctrl-mapped
  });

  it("maps Enter/Backspace/Tab/Escape to their fixed C0 bytes regardless of Ctrl", () => {
    expect(keyToBytes("Enter", false)).toEqual(new Uint8Array([0x0d]));
    expect(keyToBytes("Backspace", false)).toEqual(new Uint8Array([0x7f]));
    expect(keyToBytes("Tab", false)).toEqual(new Uint8Array([0x09]));
    expect(keyToBytes("Escape", false)).toEqual(new Uint8Array([0x1b]));
  });

  it("returns null for a plain printable key (left to the input-event path)", () => {
    expect(keyToBytes("a", false)).toBeNull();
    expect(keyToBytes("5", false)).toBeNull();
  });

  it("returns null for keys with no mapping at all (bare modifier keys, unhandled function keys)", () => {
    expect(keyToBytes("Shift", false)).toBeNull();
    expect(keyToBytes("F1", false)).toBeNull();
  });

  // Checkpoint 5i (owner-reported: shell autosuggestion-accept via → did
  // nothing) — standard xterm "normal cursor key mode" (DECCKM reset) CSI
  // sequences, the encoding a shell's own line editor always expects. See
  // this file's own `keyToBytes` comment for the full story, including the
  // one known limitation (a full-screen program in DECCKM "application
  // cursor key" mode isn't specially handled — not tracked by the engine
  // at all yet).
  it("maps arrow keys to their xterm normal-mode CSI sequences", () => {
    expect(keyToBytes("ArrowUp", false)).toEqual(new Uint8Array([0x1b, 0x5b, 0x41]));
    expect(keyToBytes("ArrowDown", false)).toEqual(new Uint8Array([0x1b, 0x5b, 0x42]));
    expect(keyToBytes("ArrowRight", false)).toEqual(new Uint8Array([0x1b, 0x5b, 0x43]));
    expect(keyToBytes("ArrowLeft", false)).toEqual(new Uint8Array([0x1b, 0x5b, 0x44]));
  });

  it("maps Home/End/PageUp/PageDown/Delete to their standard CSI sequences", () => {
    expect(keyToBytes("Home", false)).toEqual(new Uint8Array([0x1b, 0x5b, 0x48]));
    expect(keyToBytes("End", false)).toEqual(new Uint8Array([0x1b, 0x5b, 0x46]));
    expect(keyToBytes("PageUp", false)).toEqual(new Uint8Array([0x1b, 0x5b, 0x35, 0x7e]));
    expect(keyToBytes("PageDown", false)).toEqual(new Uint8Array([0x1b, 0x5b, 0x36, 0x7e]));
    expect(keyToBytes("Delete", false)).toEqual(new Uint8Array([0x1b, 0x5b, 0x33, 0x7e]));
  });

  // Checkpoint 5q (owner-reported: Shift+Tab — Claude Code's own mode-
  // cycling shortcut — did nothing in this terminal). `shiftKey` is a new,
  // defaulted-to-`false` third parameter; every other `keyToBytes` call in
  // this file (all two-argument) exercises that default and must keep
  // returning plain Tab's 0x09, unchanged.
  it("maps Shift+Tab to CSI Z, and plain Tab to 0x09 regardless of the shiftKey default", () => {
    expect(keyToBytes("Tab", false, true)).toEqual(new Uint8Array([0x1b, 0x5b, 0x5a]));
    expect(keyToBytes("Tab", false, false)).toEqual(new Uint8Array([0x09]));
    expect(keyToBytes("Tab", false)).toEqual(new Uint8Array([0x09])); // shiftKey defaults to false
  });

  it("Ctrl+Shift+Tab still maps to CSI Z (Ctrl-letter branch only ever matches single-character keys)", () => {
    expect(keyToBytes("Tab", true, true)).toEqual(new Uint8Array([0x1b, 0x5b, 0x5a]));
  });

  it("arrow/navigation keys are unaffected by Ctrl (no Ctrl+key mapping exists for any of them)", () => {
    // `ctrlKey && key.length === 1` only matches single-character keys —
    // every key in this group is a multi-character `e.key` value, so the
    // Ctrl-letter branch never applies to any of them regardless of
    // `ctrlKey`; each still maps to its own plain sequence.
    const bytes = new Uint8Array([0x1b, 0x5b, 0x43]);
    for (const key of ["ArrowRight", "Home", "End", "PageUp", "PageDown", "Delete"]) {
      expect(keyToBytes(key, true), key).not.toBeNull();
    }
    expect(keyToBytes("ArrowRight", true)).toEqual(bytes);
  });
});
