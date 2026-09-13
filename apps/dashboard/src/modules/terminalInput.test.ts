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

  it("returns null for keys with no C0 mapping (e.g. arrow keys, deferred)", () => {
    expect(keyToBytes("ArrowUp", false)).toBeNull();
    expect(keyToBytes("Shift", false)).toBeNull();
  });
});
