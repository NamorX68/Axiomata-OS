import { describe, expect, it } from "vitest";

import { focusReport, mouseReport, wantsReport } from "./terminalMouse";

const none = { shiftKey: false, altKey: false, ctrlKey: false };
const text = (bytes: Uint8Array | null) => (bytes ? new TextDecoder().decode(bytes) : null);

describe("mouse reports", () => {
  it("send presses, releases, drags and moves in the SGR encoding", () => {
    expect(text(mouseReport("press", 0, none, "sgr", 4, 9))).toBe("\x1b[<0;10;5M");
    expect(text(mouseReport("release", 0, none, "sgr", 4, 9))).toBe("\x1b[<0;10;5m");
    expect(text(mouseReport("drag", 2, none, "sgr", 0, 0))).toBe("\x1b[<34;1;1M");
    expect(text(mouseReport("move", 0, none, "sgr", 0, 0))).toBe("\x1b[<35;1;1M");
    expect(text(mouseReport("press", 0, { ...none, ctrlKey: true }, "sgr", 0, 0))).toBe("\x1b[<16;1;1M");
  });

  it("send X10 bytes, with button 3 for a release, and nothing past what X10 can say", () => {
    expect(Array.from(mouseReport("press", 1, none, "x10", 0, 0) ?? [])).toEqual([0x1b, 0x5b, 0x4d, 33, 33, 33]);
    expect(Array.from(mouseReport("release", 1, none, "x10", 0, 0) ?? [])).toEqual([0x1b, 0x5b, 0x4d, 35, 33, 33]);
    expect(mouseReport("press", 0, none, "x10", 0, 300)).toBeNull();
    expect(mouseReport("press", 4, none, "sgr", 0, 0)).toBeNull();
  });

  it("carry Shift, Alt and Ctrl in X10's button byte", () => {
    const report = mouseReport("press", 0, { shiftKey: true, altKey: true, ctrlKey: true }, "x10", 0, 0);
    expect(Array.from(report ?? [])[3]).toBe(32 + 4 + 8 + 16);
  });

  it("go only where the program asked: clicks, then drags, then all motion", () => {
    expect(wantsReport("press", "clicks")).toBe(true);
    expect(wantsReport("drag", "clicks")).toBe(false);
    expect(wantsReport("drag", "drags")).toBe(true);
    expect(wantsReport("move", "drags")).toBe(false);
    expect(wantsReport("move", "motion")).toBe(true);
    expect(wantsReport("press", "off")).toBe(false);
  });

  it("report focus in and out", () => {
    expect(text(focusReport(true))).toBe("\x1b[I");
    expect(text(focusReport(false))).toBe("\x1b[O");
  });
});
