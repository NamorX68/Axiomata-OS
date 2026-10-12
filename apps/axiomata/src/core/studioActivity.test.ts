import { get } from "svelte/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  sessionCards,
  sessionCount,
  startActivityPoll,
  studioActivity,
  studioCards,
  watchActivity,
} from "./studioActivity";

describe("sessionCount", () => {
  it("counts the sessions of a list and nothing else", () => {
    expect(sessionCount([{ card_id: 1 }, { card_id: 2 }])).toBe(2);
    expect(sessionCount([])).toBe(0);
    expect(sessionCount(null)).toBe(0);
    expect(sessionCount([{ nope: 1 }, "x", null])).toBe(0);
  });
});

describe("sessionCards", () => {
  it("names the cards at work, each once, and nothing for an answer that is no list", () => {
    expect([...sessionCards([{ card_id: 3 }, { card_id: 7 }, { card_id: 3 }])].sort()).toEqual([3, 7]);
    expect(sessionCards(null).size).toBe(0);
    expect(sessionCards([{ nope: 1 }]).size).toBe(0);
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
    expect(report).toHaveBeenLastCalledWith(1, new Set([1]));
    fetchOpen.mockResolvedValue([]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(report).toHaveBeenLastCalledWith(0, new Set());
    stop();
    const calls = fetchOpen.mock.calls.length;
    await vi.advanceTimersByTimeAsync(5000);
    expect(fetchOpen.mock.calls.length).toBe(calls);
  });

  it("keeps the last answer when a call fails, and skips rounds while the window is hidden", async () => {
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

describe("watchActivity", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("runs one poll for all watchers, feeds the stores, and stops with the last watcher", async () => {
    const fetchOpen = vi.fn().mockResolvedValue([{ card_id: 4 }, { card_id: 9 }]);
    const stopA = watchActivity(fetchOpen);
    const stopB = watchActivity(fetchOpen);
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchOpen).toHaveBeenCalledTimes(1);
    expect(get(studioActivity)).toBe(2);
    expect([...get(studioCards)].sort()).toEqual([4, 9]);
    stopA();
    stopA();
    await vi.advanceTimersByTimeAsync(5000);
    expect(fetchOpen).toHaveBeenCalledTimes(2);
    stopB();
    await vi.advanceTimersByTimeAsync(20_000);
    expect(fetchOpen).toHaveBeenCalledTimes(2);
  });
});
