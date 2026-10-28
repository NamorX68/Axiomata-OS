import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";

import { invokeBackend } from "./backend";
import { SOLID_GLASS, glass, glassOpacity, startGlass, systemReducesTransparency } from "./glass";
import { windowTransparency } from "./stores";

vi.mock("./backend", () => ({ invokeBackend: vi.fn() }));
const invoke = vi.mocked(invokeBackend);

describe("glassOpacity", () => {
  it("keeps each theme's own look at the default setting", () => {
    expect(glassOpacity(50, false)).toBe(1);
  });

  it("runs from solid at 0 to fully transparent at 100", () => {
    expect(glassOpacity(0, false)).toBe(SOLID_GLASS);
    expect(glassOpacity(100, false)).toBe(0);
    expect(glassOpacity(75, false)).toBe(0.5);
    expect(glassOpacity(25, false)).toBeCloseTo(1.6);
  });

  it("makes even the thinnest authored alpha solid at the solid end", () => {
    const thinnestAlpha = 0.46;
    expect(thinnestAlpha * glassOpacity(0, false)).toBeGreaterThanOrEqual(1);
  });

  it("makes the glass solid when the system reduces transparency, whatever the setting", () => {
    expect(glassOpacity(100, true)).toBe(SOLID_GLASS);
    expect(glassOpacity(50, true)).toBe(SOLID_GLASS);
  });

  it("keeps a stored value out of range inside the knob's ends", () => {
    expect(glassOpacity(-20, false)).toBe(SOLID_GLASS);
    expect(glassOpacity(180, false)).toBe(0);
    expect(glassOpacity(Number.NaN, false)).toBe(1);
  });
});

describe("glass", () => {
  it("derives the multiplier in effect from the setting and the system", () => {
    windowTransparency.set(75);
    systemReducesTransparency.set(false);
    expect(get(glass)).toBe(0.5);
    systemReducesTransparency.set(true);
    expect(get(glass)).toBe(SOLID_GLASS);
    systemReducesTransparency.set(false);
    windowTransparency.set(50);
  });
});

describe("startGlass (G5, G12)", () => {
  const root = document.documentElement;
  const opacity = (): string => root.style.getPropertyValue("--ax-tile-glass-opacity");

  afterEach(() => {
    invoke.mockReset();
    windowTransparency.set(50);
    systemReducesTransparency.set(false);
    root.style.removeProperty("--ax-tile-glass-opacity");
    delete root.dataset.transparency;
  });

  it("writes the knob's multiplier on <html> and follows it", async () => {
    invoke.mockResolvedValue(false);
    const stop = startGlass();
    expect(opacity()).toBe("1");
    windowTransparency.set(100);
    expect(opacity()).toBe("0");
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("reduce_transparency"));
    expect(root.dataset.transparency).toBeUndefined();
    stop();
  });

  it("turns the glass solid when the Mac reduces transparency", async () => {
    invoke.mockResolvedValue(true);
    const stop = startGlass();
    await vi.waitFor(() => expect(root.dataset.transparency).toBe("reduced"));
    expect(opacity()).toBe(String(SOLID_GLASS));
    stop();
  });

  it("asks again when the window comes back to the front, and drops the dense glass once the Mac stops asking", async () => {
    invoke.mockResolvedValue(true);
    const stop = startGlass();
    await vi.waitFor(() => expect(root.dataset.transparency).toBe("reduced"));
    invoke.mockResolvedValue(false);
    window.dispatchEvent(new Event("focus"));
    await vi.waitFor(() => expect(root.dataset.transparency).toBeUndefined());
    expect(invoke).toHaveBeenCalledTimes(2);
    stop();
  });

  it("keeps the glass as it is when the question fails", async () => {
    invoke.mockRejectedValue(new Error("no backend"));
    const stop = startGlass();
    await vi.waitFor(() => expect(invoke).toHaveBeenCalled());
    await Promise.resolve();
    expect(get(systemReducesTransparency)).toBe(false);
    expect(root.dataset.transparency).toBeUndefined();
    stop();
  });

  it("stops asking and writing once stopped", async () => {
    invoke.mockResolvedValue(false);
    const stop = startGlass();
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    stop();
    window.dispatchEvent(new Event("focus"));
    windowTransparency.set(0);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(opacity()).toBe("1");
  });
});
