import { describe, expect, it } from "vitest";

import type { BundledSkill, BundledState } from "../core/backend";
import { isOwners, needsUpdate, noticeText, stateLabel, stateTone, updatableCount } from "./bundledSkills";

const ALL: BundledState[] = ["up_to_date", "outdated", "missing", "changed", "differs"];
const list = (...states: BundledState[]): BundledSkill[] => states.map((state, i) => ({ name: `s${i}`, state }));

describe("bundled skills", () => {
  it("asks before replacing only what may be the owner's", () => {
    expect(ALL.filter(isOwners)).toEqual(["changed", "differs"]);
  });

  it("updates in one go only what holds nothing of the owner's", () => {
    expect(ALL.filter(needsUpdate)).toEqual(["outdated", "missing"]);
    expect(updatableCount(list(...ALL))).toBe(2);
  });

  it("stays calm: only an up-to-date skill has a tone of its own", () => {
    expect(ALL.map(stateTone)).toEqual(["ok", "neutral", "neutral", "neutral", "neutral"]);
  });

  it("names every state", () => {
    expect(ALL.map(stateLabel)).toEqual([
      "up to date",
      "outdated",
      "missing",
      "changed by you",
      "differs — may be yours",
    ]);
  });

  it("shows the notice only when something can be updated", () => {
    expect(noticeText(null)).toBeNull();
    expect(noticeText(list("up_to_date", "changed", "differs"))).toBeNull();
    expect(noticeText(list("outdated", "changed"))).toBe("1 bundled skill has an update");
    expect(noticeText(list("outdated", "missing"))).toBe("2 bundled skills have updates");
  });
});
