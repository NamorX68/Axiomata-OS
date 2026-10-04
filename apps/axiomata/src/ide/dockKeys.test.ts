import { describe, expect, it } from "vitest";

import { cycleTab, focusedGroupOf, nthTab, splitActive } from "./dockKeys";
import { allGroups, singleGroupLayout, type PaneTab } from "./layout";

const tab = (id: string): PaneTab => ({ id, kind: "terminal", title: id });
const three = () => singleGroupLayout([tab("a"), tab("b"), tab("c")]);
const active = (l: ReturnType<typeof three>) => allGroups(l)[0].active;

describe("dockKeys", () => {
  it("cycles forwards and backwards, wrapping", () => {
    const l = three();
    const first = active(l);
    const next = cycleTab(l, null, 1);
    expect(active(next)).not.toBe(first);
    expect(active(cycleTab(cycleTab(cycleTab(l, null, 1), null, 1), null, 1))).toBe(first);
    expect(active(cycleTab(l, null, -1))).toBe("c");
  });

  it("picks the n-th tab, and the last for 9", () => {
    expect(active(nthTab(three(), null, 2))).toBe("b");
    expect(active(nthTab(three(), null, 9))).toBe("c");
    const l = three();
    expect(nthTab(l, null, 5)).toBe(l);
  });

  it("falls back to the first group for a stale tab id", () => {
    expect(focusedGroupOf(three(), "gone")?.tabs).toHaveLength(3);
  });

  it("splits the visible tab off, but not a lone one", () => {
    const split = splitActive(three(), "a", "right");
    expect(allGroups(split)).toHaveLength(2);
    const lone = singleGroupLayout([tab("x")]);
    expect(splitActive(lone, null, "right")).toBe(lone);
  });
});
