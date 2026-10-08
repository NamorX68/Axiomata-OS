import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { sessionCount, startActivityPoll } from "./studioActivity";

describe("sessionCount", () => {
  it("counts the sessions of a list and nothing else", () => {
    expect(sessionCount([{ card_id: 1 }, { card_id: 2 }])).toBe(2);
    expect(sessionCount([])).toBe(0);
    expect(sessionCount(null)).toBe(0);
    expect(sessionCount([{ nope: 1 }, "x", null])).toBe(0);
  });
});

describe("startActivityPoll", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("reports at once and then on every round until stopped", async () => {
    const fetchOpen = vi.fn().mockResolvedValue([{ card_id: 1 }]);
    const report = vi.fn();
    const stop = startActivityPoll(fetchOpen, report, 1000, () => false);
    await vi.advanceTimersByTimeAsync(0);
    expect(report).toHaveBeenLastCalledWith(1);
    fetchOpen.mockResolvedValue([]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(report).toHaveBeenLastCalledWith(0);
    stop();
    const calls = fetchOpen.mock.calls.length;
    await vi.advanceTimersByTimeAsync(5000);
    expect(fetchOpen.mock.calls.length).toBe(calls);
  });

  it("keeps the last count when a call fails, and skips rounds while the window is hidden", async () => {
    const fetchOpen = vi.fn().mockResolvedValueOnce([{ card_id: 1 }]).mockRejectedValue(new Error("down"));
    const report = vi.fn();
    let hidden = false;
    const stop = startActivityPoll(fetchOpen, report, 1000, () => hidden);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(1000);
    expect(report).toHaveBeenCalledTimes(1);
    hidden = true;
    const calls = fetchOpen.mock.calls.length;
    await vi.advanceTimersByTimeAsync(3000);
    expect(fetchOpen.mock.calls.length).toBe(calls);
    stop();
  });
});
