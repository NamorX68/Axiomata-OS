import { describe, expect, it } from "vitest";
import { FRESH_MEMORY, MAX_AUTO_ATTEMPTS, applyAnswer, harnessRuns, healWhile } from "./reach";

const missed = { state: "unreachable", reason: "refused" } as const;

describe("harnessRuns", () => {
  it("counts a session that is idle, working or waiting as running", () => {
    expect(["idle", "working", "waiting"].every((s) => harnessRuns(s as never))).toBe(true);
  });

  it("does not count one that is starting, ended or unknown", () => {
    expect(harnessRuns("starting")).toBe(false);
    expect(harnessRuns("ended")).toBe(false);
    expect(harnessRuns(undefined)).toBe(false);
  });
});

describe("healWhile", () => {
  it("only looks at the first miss: a server that is just starting has no lock yet", () => {
    expect(healWhile("idle", FRESH_MEMORY)).toBeNull();
  });

  it("repairs a running session after a second miss, naming its status", () => {
    expect(healWhile("idle", { misses: 1, failedHeals: 0 })).toBe("idle");
    expect(healWhile("working", { misses: 2, failedHeals: MAX_AUTO_ATTEMPTS - 1 })).toBe("working");
  });

  it("gives up by itself after the allowed failures", () => {
    expect(healWhile("idle", { misses: 5, failedHeals: MAX_AUTO_ATTEMPTS })).toBeNull();
  });

  it("never registers a server for an ended or unknown session", () => {
    expect(healWhile("ended", { misses: 3, failedHeals: 0 })).toBeNull();
    expect(healWhile(undefined, { misses: 3, failedHeals: 0 })).toBeNull();
  });
});

describe("applyAnswer", () => {
  it("reads a reachable or reconnected session as reachable and forgets earlier misses", () => {
    const memory = { misses: 2, failedHeals: 2 };
    expect(applyAnswer({ state: "reachable" }, false, memory)).toEqual({
      view: "reachable",
      reason: null,
      memory: FRESH_MEMORY,
    });
    expect(applyAnswer({ state: "reconnected" }, true, memory).memory).toEqual(FRESH_MEMORY);
  });

  it("does not show a first miss, but does show the second", () => {
    const first = applyAnswer(missed, false, FRESH_MEMORY);
    expect(first.view).toBe("unknown");
    expect(first.memory).toEqual({ misses: 1, failedHeals: 0 });
    const second = applyAnswer(missed, false, first.memory);
    expect(second).toEqual({ view: "unreachable", reason: "refused", memory: { misses: 2, failedHeals: 0 } });
  });

  it("counts a repair that failed, and shows it at once", () => {
    const step = applyAnswer(missed, true, { misses: 1, failedHeals: 1 });
    expect(step.view).toBe("unreachable");
    expect(step.memory).toEqual({ misses: 2, failedHeals: 2 });
  });

  it("passes a session without a registration through", () => {
    expect(applyAnswer({ state: "not_applicable" }, true, FRESH_MEMORY).view).toBe("not_applicable");
  });
});
