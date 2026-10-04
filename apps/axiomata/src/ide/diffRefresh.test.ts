import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DiffRefresh } from "./diffRefresh";

describe("DiffRefresh (G4)", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("refreshes when the view comes into view, not again while it stays", () => {
    const refresh = vi.fn();
    const r = new DiffRefresh(refresh, 1000);
    r.update(false, "idle");
    expect(refresh).not.toHaveBeenCalled();
    r.update(true, "idle");
    r.update(true, "idle");
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("polls only while visible and working, and once more when the agent stops", () => {
    const refresh = vi.fn();
    const r = new DiffRefresh(refresh, 1000);
    r.update(true, "working"); // came into view
    vi.advanceTimersByTime(3000);
    expect(refresh).toHaveBeenCalledTimes(4);
    r.update(true, "idle"); // stopped working
    expect(refresh).toHaveBeenCalledTimes(5);
    vi.advanceTimersByTime(5000);
    expect(refresh).toHaveBeenCalledTimes(5);
  });

  it("does not poll while hidden, and catches up when shown", () => {
    const refresh = vi.fn();
    const r = new DiffRefresh(refresh, 1000);
    r.update(false, "working");
    vi.advanceTimersByTime(5000);
    r.update(false, "idle");
    expect(refresh).not.toHaveBeenCalled();
    r.update(true, "idle");
    expect(refresh).toHaveBeenCalledTimes(1);
    r.dispose();
  });
});
