import { get } from "svelte/store";
import { afterEach, describe, expect, it, vi } from "vitest";

import { invokeBackend } from "./backend";
import {
  MAX_GROUND_BLUR,
  MAX_GROUND_DIM,
  SOLID_GLASS,
  glass,
  glassOpacity,
  groundBlur,
  groundDim,
  startGlass,
  systemReducesTransparency,
} from "./glass";
import { glassBlur, glassDim, windowTransparency } from "./stores";

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
    expect(glassOpacity(25, false)).toBeCloseTo(1.75);
  });

  it("makes even the thinnest authored alpha solid at the solid end", () => {
    const thinnestAlpha = 0.4;
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

describe("groundBlur", () => {
  it("keeps a stored blur inside the knob's ends, in whole pixels", () => {
    expect(groundBlur(20)).toBe(20);
    expect(groundBlur(12.6)).toBe(13);
    expect(groundBlur(-5)).toBe(0);
    expect(groundBlur(500)).toBe(MAX_GROUND_BLUR);
    expect(groundBlur(Number.NaN)).toBe(0);
  });
});

describe("groundDim", () => {
  it("turns the stored % into the veil's alpha, inside the knob's ends", () => {
    expect(groundDim(35)).toBe(0.35);
    expect(groundDim(0)).toBe(0);
    expect(groundDim(200)).toBe(MAX_GROUND_DIM / 100);
    expect(groundDim(-10)).toBe(0);
    expect(groundDim(Number.NaN)).toBe(0.35);
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
    root.style.removeProperty("--ax-ground-blur");
    root.style.removeProperty("--ax-ground-dim");
    glassBlur.set(20);
    glassDim.set(35);
    delete root.dataset.transparency;
  });

  it("writes the ground's blur on <html> and follows the knob", () => {
    invoke.mockResolvedValue(false);
    const stop = startGlass();
    expect(root.style.getPropertyValue("--ax-ground-blur")).toBe("20px");
    glassBlur.set(4);
    expect(root.style.getPropertyValue("--ax-ground-blur")).toBe("4px");
    stop();
  });

  it("writes the veil over the picture on <html> and follows the knob", () => {
    invoke.mockResolvedValue(false);
    const stop = startGlass();
    expect(root.style.getPropertyValue("--ax-ground-dim")).toBe("0.35");
    glassDim.set(60);
    expect(root.style.getPropertyValue("--ax-ground-dim")).toBe("0.6");
    stop();
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
