import { describe, expect, it } from "vitest";
import { ASK_INTERVAL_MS, OWNER_QUIET_MS, ownerIsQuiet, shouldAsk, type NudgeSituation } from "./nudge";

const base: NudgeSituation = { state: "idle", sinceOwnInputMs: null, sinceAskMs: null, busy: false };

describe("shouldAsk", () => {
  it("asks an idle agent whose owner has not typed", () => {
    expect(shouldAsk(base)).toBe(true);
  });

  it("never asks for an agent that is not idle", () => {
    for (const state of ["starting", "working", "waiting", "ended", undefined] as const) {
      expect(shouldAsk({ ...base, state }), String(state)).toBe(false);
    }
  });

  it("leaves the pane alone while the owner types, and resumes once they stopped", () => {
    expect(shouldAsk({ ...base, sinceOwnInputMs: 500 })).toBe(false);
    expect(shouldAsk({ ...base, sinceOwnInputMs: OWNER_QUIET_MS - 1 })).toBe(false);
    expect(shouldAsk({ ...base, sinceOwnInputMs: OWNER_QUIET_MS })).toBe(true);
  });

  it("does not ask twice in a row, nor while a line is being typed", () => {
    expect(shouldAsk({ ...base, sinceAskMs: ASK_INTERVAL_MS - 1 })).toBe(false);
    expect(shouldAsk({ ...base, sinceAskMs: ASK_INTERVAL_MS })).toBe(true);
    expect(shouldAsk({ ...base, busy: true })).toBe(false);
  });
});

describe("ownerIsQuiet", () => {
  it("is the rule asked again right before typing", () => {
    expect(ownerIsQuiet(null)).toBe(true);
    expect(ownerIsQuiet(OWNER_QUIET_MS - 1)).toBe(false);
    expect(ownerIsQuiet(OWNER_QUIET_MS)).toBe(true);
  });
});
