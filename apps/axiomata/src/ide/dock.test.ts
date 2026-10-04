import { describe, expect, it } from "vitest";

import type { GroupGeometry, Rect } from "./dock";
import {
  DOCK_EDGE_RATIO,
  DRAG_THRESHOLD_PX,
  ROOT_NODE_ID,
  crossedDragThreshold,
  dividerFraction,
  dockSideAt,
  dropTarget,
  inRect,
  splitFractionAt,
  tabDropIndex,
} from "./dock";

/** A 200×100 pane at the origin, so a tenth is a round number in both axes. */
const pane: Rect = { x: 0, y: 0, w: 200, h: 100 };

describe("dockSideAt", () => {
  it("calls the middle a tab drop", () => {
    expect(dockSideAt(pane, 100, 50)).toBe("center");
    // Just inside the edge zone on every side.
    expect(dockSideAt(pane, 55, 50)).toBe("center");
    expect(dockSideAt(pane, 145, 50)).toBe("center");
    expect(dockSideAt(pane, 100, 30)).toBe("center");
    expect(dockSideAt(pane, 100, 70)).toBe("center");
  });

  it("names the edge the pointer is nearest", () => {
    expect(dockSideAt(pane, 10, 50)).toBe("left");
    expect(dockSideAt(pane, 190, 50)).toBe("right");
    expect(dockSideAt(pane, 100, 5)).toBe("top");
    expect(dockSideAt(pane, 100, 95)).toBe("bottom");
  });

  it("measures in fractions, so a wide pane's edge is no easier to hit", () => {
    const wide: Rect = { x: 0, y: 0, w: 2000, h: 100 };
    // 100px in is half the narrow pane's width but a twentieth of the wide
    // one's — the same pixel distance, two different answers.
    expect(dockSideAt(pane, 100, 50)).toBe("center");
    expect(dockSideAt(wide, 100, 50)).toBe("left");
  });

  it("gives a corner to the edge the pointer went deeper into", () => {
    // Top-left corner of a 200×100 pane: 10px in from the left is 5% of the
    // width, 10px down is 10% of the height, so the left edge is nearer.
    expect(dockSideAt(pane, 10, 10)).toBe("left");
    expect(dockSideAt(pane, 30, 5)).toBe("top");
  });

  it("is null outside the pane and for a rectangle with no area", () => {
    expect(dockSideAt(pane, -1, 50)).toBeNull();
    expect(dockSideAt(pane, 201, 50)).toBeNull();
    expect(dockSideAt(pane, 100, 101)).toBeNull();
    expect(dockSideAt({ x: 0, y: 0, w: 0, h: 0 }, 0, 0)).toBeNull();
  });

  it("takes a ratio, so a caller can widen or narrow the edge zone", () => {
    expect(dockSideAt(pane, 100, 30)).toBe("center");
    expect(dockSideAt(pane, 100, 30, 0.4)).toBe("top");
    expect(DOCK_EDGE_RATIO).toBe(0.25);
  });
});

describe("inRect", () => {
  it("includes the edges", () => {
    expect(inRect(pane, 0, 0)).toBe(true);
    expect(inRect(pane, 200, 100)).toBe(true);
    expect(inRect(pane, 200.5, 100)).toBe(false);
  });
});

describe("tabDropIndex", () => {
  const tabs: Rect[] = [
    { x: 0, y: 0, w: 100, h: 30 },
    { x: 100, y: 0, w: 100, h: 30 },
    { x: 200, y: 0, w: 100, h: 30 },
  ];

  it("inserts before the first tab whose midpoint the pointer has not passed", () => {
    expect(tabDropIndex(tabs, 10)).toBe(0);
    expect(tabDropIndex(tabs, 49)).toBe(0);
    expect(tabDropIndex(tabs, 51)).toBe(1);
    expect(tabDropIndex(tabs, 151)).toBe(2);
  });

  it("appends past the last midpoint, and on an empty bar", () => {
    expect(tabDropIndex(tabs, 290)).toBe(3);
    expect(tabDropIndex([], 10)).toBe(0);
  });
});

describe("dropTarget", () => {
  const root: Rect = { x: 0, y: 0, w: 1000, h: 600 };

  /** A group occupying the given horizontal half, with a 30px tab bar on top. */
  function half(nodeId: string, x: number, tabs: number): GroupGeometry {
    return {
      nodeId,
      rect: { x, y: 0, w: 500, h: 600 },
      tabBar: { x, y: 0, w: 500, h: 30 },
      tabs: Array.from({ length: tabs }, (_, i) => ({ x: x + i * 100, y: 0, w: 100, h: 30 })),
    };
  }

  const groups = [half("left-group", 0, 2), half("right-group", 500, 1)];

  it("reads a tab bar as a tab drop at a position", () => {
    expect(dropTarget(root, groups, 250, 15)).toEqual({ nodeId: "left-group", side: "center", index: 2 });
    expect(dropTarget(root, groups, 40, 15)).toEqual({ nodeId: "left-group", side: "center", index: 0 });
    expect(dropTarget(root, groups, 520, 15)).toEqual({ nodeId: "right-group", side: "center", index: 0 });
  });

  it("gives the tab bar precedence over the root's edge strip", () => {
    // x = 5 is inside the root's 3% strip, but it is also on a tab bar, and the
    // bar is the smaller, more deliberate target.
    expect(dropTarget(root, groups, 5, 15)).toEqual({ nodeId: "left-group", side: "center", index: 0 });
  });

  it("docks against the whole layout in the outer strip", () => {
    expect(dropTarget(root, groups, 5, 300)).toEqual({ nodeId: ROOT_NODE_ID, side: "left" });
    expect(dropTarget(root, groups, 995, 300)).toEqual({ nodeId: ROOT_NODE_ID, side: "right" });
    expect(dropTarget(root, groups, 500, 595)).toEqual({ nodeId: ROOT_NODE_ID, side: "bottom" });
  });

  it("falls to the group under the pointer everywhere else", () => {
    expect(dropTarget(root, groups, 250, 300)).toEqual({ nodeId: "left-group", side: "center" });
    expect(dropTarget(root, groups, 40, 300)).toEqual({ nodeId: "left-group", side: "left" });
    expect(dropTarget(root, groups, 250, 580)).toEqual({ nodeId: "left-group", side: "bottom" });
    expect(dropTarget(root, groups, 750, 300)).toEqual({ nodeId: "right-group", side: "center" });
  });

  it("offers no root edge while there is only one group to dock against", () => {
    // With a single group the root *is* that group; naming the root would only
    // be a longer way to say the same thing.
    const single = [half("only", 0, 1)];
    expect(dropTarget(root, single, 5, 300)).toEqual({ nodeId: "only", side: "left" });
  });

  it("is null when the pointer is over nothing", () => {
    expect(dropTarget(root, groups, 2000, 300)).toBeNull();
    expect(dropTarget(root, [], 500, 300)).toBeNull();
  });
});

describe("splitFractionAt", () => {
  it("measures along the split's own axis", () => {
    expect(splitFractionAt(pane, "row", 50, 10)).toBe(0.25);
    expect(splitFractionAt(pane, "col", 10, 25)).toBe(0.25);
  });

  it("clamps a drag that left the split", () => {
    expect(splitFractionAt(pane, "row", -400, 50)).toBe(0);
    expect(splitFractionAt(pane, "row", 4000, 50)).toBe(1);
  });

  it("survives a rectangle with no width", () => {
    expect(splitFractionAt({ x: 0, y: 0, w: 0, h: 0 }, "row", 10, 10)).toBe(0);
  });
});

describe("crossedDragThreshold", () => {
  it("lets a shaky click stay a click", () => {
    expect(crossedDragThreshold({ x: 100, y: 100 }, 102, 101)).toBe(false);
    expect(crossedDragThreshold({ x: 100, y: 100 }, 100, 100)).toBe(false);
  });

  it("is a drag once the pointer has carried the tab away", () => {
    expect(crossedDragThreshold({ x: 100, y: 100 }, 100, 104)).toBe(true);
    expect(crossedDragThreshold({ x: 100, y: 100 }, 140, 300)).toBe(true);
  });

  it("measures in any direction, and takes its own threshold", () => {
    expect(crossedDragThreshold({ x: 100, y: 100 }, 97, 97)).toBe(true);
    expect(crossedDragThreshold({ x: 100, y: 100 }, 97, 97, 20)).toBe(false);
    expect(DRAG_THRESHOLD_PX).toBe(4);
  });
});

describe("dividerFraction", () => {
  it("subtracts what the children before the pair already take", () => {
    // Three panes at 0.5 / 0.25 / 0.25; dragging the second divider to 80%
    // leaves the middle pane 0.3 of the split, not 0.8.
    expect(dividerFraction([0.5, 0.25, 0.25], 1, 0.8)).toBeCloseTo(0.3);
    expect(dividerFraction([0.5, 0.5], 0, 0.7)).toBeCloseTo(0.7);
  });

  it("can go negative, which resizeSplit then clamps to the minimum", () => {
    expect(dividerFraction([0.5, 0.25, 0.25], 1, 0.2)).toBeCloseTo(-0.3);
  });
});
