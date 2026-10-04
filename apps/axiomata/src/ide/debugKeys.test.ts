import { describe, expect, it } from "vitest";

import { debugKeyAction } from "./debugKeys";

const key = (k: string, extra: Partial<KeyboardEvent> = {}) => ({ key: k, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...extra });

describe("debug keys", () => {
  it("F1 continues, F2 steps over, F3 steps out, F4 steps into", () => {
    expect(["F1", "F2", "F3", "F4"].map((k) => debugKeyAction(key(k)))).toEqual(["continue", "next", "step_out", "step_in"]);
  });

  it("F5 and F10 still work; F11 is left to macOS", () => {
    expect(debugKeyAction(key("F5"))).toBe("continue");
    expect(debugKeyAction(key("F10"))).toBe("next");
    expect(debugKeyAction(key("F11"))).toBeNull();
  });

  it("a chord with a modifier is somebody else's", () => {
    expect(debugKeyAction(key("F2", { metaKey: true }))).toBeNull();
    expect(debugKeyAction(key("F5", { shiftKey: true }))).toBeNull();
  });
});
