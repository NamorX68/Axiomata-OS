import { afterEach, describe, expect, it, vi } from "vitest";

import type { RunSummary } from "./backend";
import { watchSkillRuns } from "./skillRun";

const run = (id: number, skill_name: string) => ({ id, skill_name }) as RunSummary;

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await Promise.resolve();
}

describe("watchSkillRuns", () => {
  afterEach(() => vi.useRealTimers());

  it("takes the first look as the baseline, then reports each newer run of its skill once", async () => {
    vi.useFakeTimers();
    let runs = [run(5, "mail-digest"), run(4, "calendar-digest")];
    const invoke = vi.fn(async () => runs) as never;
    const onNew = vi.fn();
    const stop = watchSkillRuns(invoke, () => "mail-digest", onNew, 1000);
    await settle();
    expect(onNew).not.toHaveBeenCalled();
    runs = [run(6, "calendar-digest"), ...runs];
    await vi.advanceTimersByTimeAsync(1000);
    expect(onNew).not.toHaveBeenCalled();
    runs = [run(7, "mail-digest"), ...runs];
    await vi.advanceTimersByTimeAsync(1000);
    expect(onNew).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(onNew).toHaveBeenCalledTimes(1);
    stop();
    runs = [run(8, "mail-digest"), ...runs];
    await vi.advanceTimersByTimeAsync(5000);
    expect(onNew).toHaveBeenCalledTimes(1);
  });
});
