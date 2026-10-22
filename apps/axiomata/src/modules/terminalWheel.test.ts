import { describe, expect, it } from "vitest";

import {
  WheelAccumulator,
  historyKeyAction,
  mouseWheelReport,
  offsetForThumb,
  scrollKeyLines,
  scrollThumb,
  wheelAction,
} from "./terminalWheel";

const text = (bytes: Uint8Array): string => new TextDecoder().decode(bytes);

describe("wheelAction", () => {
  it("reports the wheel in SGR form to a program that tracks the mouse, one report per line", () => {
    const action = wheelAction(2, { mouse: "sgr", altScreen: true }, 4, 9);
    expect(action?.kind).toBe("send");
    if (action?.kind === "send") expect(text(action.bytes)).toBe("\x1b[<64;10;5M\x1b[<64;10;5M");
  });

  it("reports a turn towards the user as button 65", () => {
    const action = wheelAction(-1, { mouse: "sgr", altScreen: false }, 0, 0);
    if (action?.kind === "send") expect(text(action.bytes)).toBe("\x1b[<65;1;1M");
    else throw new Error("expected bytes");
  });

  it("turns the wheel into arrow keys on the alternate screen without mouse tracking", () => {
    const up = wheelAction(3, { mouse: "off", altScreen: true }, 0, 0);
    const down = wheelAction(-1, { mouse: "off", altScreen: true }, 0, 0);
    if (up?.kind !== "send" || down?.kind !== "send") throw new Error("expected bytes");
    expect(text(up.bytes)).toBe("\x1b[A\x1b[A\x1b[A");
    expect(text(down.bytes)).toBe("\x1b[B");
  });

  it("sends the arrow keys in application mode when the program asked for it", () => {
    const up = wheelAction(1, { mouse: "off", altScreen: true, appCursorKeys: true }, 0, 0);
    if (up?.kind !== "send") throw new Error("expected bytes");
    expect(text(up.bytes)).toBe("\x1bOA");
  });

  it("scrolls the terminal's own history on the primary screen", () => {
    expect(wheelAction(3, { mouse: "off", altScreen: false }, 0, 0)).toEqual({ kind: "history", lines: 3 });
  });

  it("does nothing for zero lines", () => {
    expect(wheelAction(0, { mouse: "sgr", altScreen: true }, 0, 0)).toBeNull();
  });
});

describe("mouseWheelReport", () => {
  it("encodes X10 reports as bytes offset by 32", () => {
    expect(Array.from(mouseWheelReport(true, "x10", 1, 2) ?? [])).toEqual([0x1b, 0x5b, 0x4d, 96, 35, 34]);
  });

  it("refuses an X10 position past what one byte can carry, but not in SGR", () => {
    expect(mouseWheelReport(true, "x10", 0, 300)).toBeNull();
    expect(text(mouseWheelReport(true, "sgr", 0, 300) ?? new Uint8Array())).toBe("\x1b[<64;301;1M");
  });
});

describe("WheelAccumulator", () => {
  it("collects small trackpad deltas until a whole line is reached", () => {
    const acc = new WheelAccumulator();
    expect(acc.lines(-6, 0, 16, 20)).toBe(0);
    expect(acc.lines(-6, 0, 16, 20)).toBe(0);
    expect(acc.lines(-6, 0, 16, 20)).toBe(1);
  });

  it("reads line and page deltas directly, with up as positive", () => {
    const acc = new WheelAccumulator();
    expect(acc.lines(3, 1, 16, 20)).toBe(-3);
    expect(acc.lines(-1, 2, 16, 20)).toBe(20);
  });

  it("forgets a partial line on reset", () => {
    const acc = new WheelAccumulator();
    acc.lines(-12, 0, 16, 20);
    acc.reset();
    expect(acc.lines(-12, 0, 16, 20)).toBe(0);
  });
});

describe("historyKeyAction", () => {
  const shift = { shiftKey: true, ctrlKey: false, altKey: false, metaKey: false };

  it("pages with Shift+PageUp/PageDown, keeping one line of context", () => {
    expect(historyKeyAction("PageUp", shift, false, 24)).toEqual({ kind: "lines", lines: 23 });
    expect(historyKeyAction("PageDown", shift, false, 24)).toEqual({ kind: "lines", lines: -23 });
  });

  it("jumps to the top and back to the live bottom with Shift+Home/End", () => {
    expect(historyKeyAction("Home", shift, false, 24)).toEqual({ kind: "top" });
    expect(historyKeyAction("End", shift, false, 24)).toEqual({ kind: "bottom" });
  });

  it("leaves the keys to the program on the alternate screen, and without Shift", () => {
    expect(historyKeyAction("PageUp", shift, true, 24)).toBeNull();
    expect(historyKeyAction("PageUp", { ...shift, shiftKey: false }, false, 24)).toBeNull();
    expect(historyKeyAction("PageUp", { ...shift, metaKey: true }, false, 24)).toBeNull();
    expect(historyKeyAction("ArrowUp", shift, false, 24)).toBeNull();
  });
});

describe("scrollThumb / offsetForThumb", () => {
  it("has no thumb without history", () => {
    expect(scrollThumb(24, 0, 0)).toBeNull();
  });

  it("sits at the bottom when live and at the top when scrolled all the way back", () => {
    const live = scrollThumb(20, 80, 0);
    const back = scrollThumb(20, 80, 80);
    expect(live?.size).toBeCloseTo(0.2);
    expect(live?.top).toBeCloseTo(0.8);
    expect(back?.top).toBeCloseTo(0);
  });

  it("keeps a minimum size for a long history", () => {
    expect(scrollThumb(10, 10_000, 0)?.size).toBeCloseTo(0.06);
  });

  it("maps a dragged thumb back to the offset it shows", () => {
    for (const offset of [0, 10, 40, 80]) {
      const thumb = scrollThumb(20, 80, offset);
      expect(offsetForThumb(thumb?.top ?? 0, 20, 80)).toBe(offset);
    }
    expect(offsetForThumb(-1, 20, 80)).toBe(80);
    expect(offsetForThumb(2, 20, 80)).toBe(0);
  });
});

describe("scrollKeyLines", () => {
  const press = (key: string, mods: Partial<KeyboardEvent> = {}) => ({
    key,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...mods,
  });

  it("scrolls with ⌘⇧↑/↓ on the Mac", () => {
    expect(scrollKeyLines(press("ArrowUp", { metaKey: true, shiftKey: true }), "mac")).toBe(3);
    expect(scrollKeyLines(press("ArrowDown", { metaKey: true, shiftKey: true }), "mac")).toBe(-3);
  });

  it("scrolls with Ctrl+Shift+↑/↓ elsewhere", () => {
    expect(scrollKeyLines(press("ArrowUp", { ctrlKey: true, shiftKey: true }), "other")).toBe(3);
  });

  it("leaves ⌘↑ without Shift, Alt variants and other keys alone", () => {
    expect(scrollKeyLines(press("ArrowUp", { metaKey: true }), "mac")).toBeNull();
    expect(scrollKeyLines(press("ArrowUp", { metaKey: true, shiftKey: true, altKey: true }), "mac")).toBeNull();
    expect(scrollKeyLines(press("ArrowLeft", { metaKey: true, shiftKey: true }), "mac")).toBeNull();
    expect(scrollKeyLines(press("ArrowUp", { ctrlKey: true }), "other")).toBeNull();
  });
});
