import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";

import { invokeBackend } from "./backend";
import {
  displayAt,
  isUiSize,
  scaleFor,
  startUiScale,
  uiScale,
  uiScaleAuto,
  uiSize,
  type UiDisplay,
} from "./uiScale";

vi.mock("./backend", () => ({ invokeBackend: vi.fn() }));
const invoke = vi.mocked(invokeBackend);

const macbook: UiDisplay = { x: 0, y: 0, width: 1512, height: 982, scale: 1 };
const wide: UiDisplay = { x: 1512, y: -400, width: 5120, height: 2160, scale: 1.25 };

describe("the UI scale (LK0, K9, K13)", () => {
  it("finds the display a window is on, else the nearest one", () => {
    expect(displayAt([macbook, wide], 700, 500)).toBe(macbook);
    expect(displayAt([macbook, wide], 4000, 800)).toBe(wide);
    // Straddling the edge: the centre decides; off every display: the nearest.
    expect(displayAt([macbook, wide], 1513, 0)).toBe(wide);
    expect(displayAt([macbook, wide], -300, 500)).toBe(macbook);
    expect(displayAt([], 0, 0)).toBeNull();
  });

  it("takes the display's scale for Auto and a percentage as it is", () => {
    expect(scaleFor("auto", wide)).toBe(1.25);
    expect(scaleFor("auto", null)).toBe(1);
    expect(scaleFor(110, wide)).toBe(1.1);
  });

  it("accepts only the offered sizes as a stored setting", () => {
    expect(isUiSize("auto")).toBe(true);
    expect(isUiSize(125)).toBe(true);
    expect(isUiSize(123)).toBe(false);
    expect(isUiSize("125")).toBe(false);
    expect(isUiSize(null)).toBe(false);
  });
});

describe("startUiScale (LK0)", () => {
  afterEach(() => {
    vi.useRealTimers();
    uiSize.set("auto");
  });

  it("applies the window's display, follows it to another one, obeys the setting and stops cleanly", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const displays = [macbook, wide];
    invoke.mockResolvedValue(displays);
    const at = (x: number) => {
      Object.defineProperty(window, "screenX", { value: x, configurable: true });
      Object.defineProperty(window, "screenY", { value: 0, configurable: true });
      Object.defineProperty(window, "outerWidth", { value: 200, configurable: true });
      Object.defineProperty(window, "outerHeight", { value: 200, configurable: true });
    };
    const applied = () => document.documentElement.style.getPropertyValue("--ax-ui-scale");
    at(100);
    const stop = startUiScale();
    await vi.waitFor(() => expect(applied()).toBe("1"));
    expect(get(uiScaleAuto)).toBe(1);

    // Still on the same display: nothing is asked again.
    invoke.mockClear();
    vi.advanceTimersByTime(1000);
    expect(invoke).not.toHaveBeenCalled();
    // Moved to the wide one: asked again, its scale applied.
    at(3000);
    vi.advanceTimersByTime(1000);
    await vi.waitFor(() => expect(applied()).toBe("1.25"));
    expect(get(uiScale)).toBe(1.25);

    uiSize.set(110);
    expect(applied()).toBe("1.1");

    stop();
    invoke.mockClear();
    uiSize.set(150);
    at(100);
    vi.advanceTimersByTime(5000);
    window.dispatchEvent(new Event("focus"));
    expect(applied()).toBe("1.1");
    expect(invoke).not.toHaveBeenCalled();
  });
});
