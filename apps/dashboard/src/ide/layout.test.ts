import { describe, expect, it } from "vitest";

import type { Layout, LayoutNode, PaneTab, Split, TabGroup } from "./layout";
import {
  LAYOUT_VERSION,
  MIN_PANE_FRACTION,
  activateTab,
  addTab,
  allGroups,
  allTabs,
  closeTab,
  emptyLayout,
  findNode,
  findTab,
  isGroup,
  isSplit,
  moveTab,
  parseLayout,
  resizeSplit,
  serializeLayout,
  setSplitSizes,
  singleGroupLayout,
} from "./layout";

function tab(id: string, kind = "terminal"): PaneTab {
  return { id, kind, title: id };
}

/** The tree as a compact string, so a test asserts a shape instead of a page of objects. */
function shape(node: LayoutNode): string {
  if (isGroup(node)) return `[${node.tabs.map((t) => (t.id === node.active ? `*${t.id}` : t.id)).join(" ")}]`;
  return `${node.dir}(${node.children.map(shape).join(" ")})`;
}

function group(layout: Layout, tabId: string): TabGroup {
  const hit = findTab(layout, tabId);
  if (!hit) throw new Error(`no tab ${tabId}`);
  return hit.group;
}

/** The single split of a two-pane layout, for the resize tests. */
function onlySplit(layout: Layout): Split {
  if (!isSplit(layout.root)) throw new Error("root is not a split");
  return layout.root;
}

function sum(numbers: number[]): number {
  return numbers.reduce((a, b) => a + b, 0);
}

describe("starting layouts", () => {
  it("an empty layout is one empty group", () => {
    const layout = emptyLayout();
    expect(isGroup(layout.root)).toBe(true);
    expect(allTabs(layout)).toEqual([]);
    expect((layout.root as TabGroup).active).toBeNull();
  });

  it("a single group holds the tabs in order with the first one active", () => {
    const layout = singleGroupLayout([tab("a"), tab("b")]);
    expect(shape(layout.root)).toBe("[*a b]");
  });

  it("copies the tabs handed in, so a later mutation cannot reach the tree", () => {
    const original = tab("a");
    const layout = singleGroupLayout([original]);
    original.title = "changed outside";
    expect(allTabs(layout)[0].title).toBe("a");
  });
});

describe("addTab", () => {
  it("appends to a group and makes the new tab active", () => {
    const base = singleGroupLayout([tab("a")]);
    const next = addTab(base, tab("b"), { nodeId: base.root.id, side: "center" });
    expect(shape(next.root)).toBe("[a *b]");
  });

  it("honours the drop index among the tabs already there", () => {
    const base = singleGroupLayout([tab("a"), tab("b")]);
    const next = addTab(base, tab("c"), { nodeId: base.root.id, side: "center", index: 1 });
    expect(shape(next.root)).toBe("[a *c b]");
  });

  it("splits along the right axis, in the right order, half and half", () => {
    const base = singleGroupLayout([tab("a")]);

    const right = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });
    expect(shape(right.root)).toBe("row([*a] [*b])");
    expect(onlySplit(right).sizes).toEqual([0.5, 0.5]);

    expect(shape(addTab(base, tab("b"), { nodeId: base.root.id, side: "left" }).root)).toBe("row([*b] [*a])");
    expect(shape(addTab(base, tab("b"), { nodeId: base.root.id, side: "top" }).root)).toBe("col([*b] [*a])");
    expect(shape(addTab(base, tab("b"), { nodeId: base.root.id, side: "bottom" }).root)).toBe("col([*a] [*b])");
  });

  it("docks against the outer edge when the target is the root split", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "bottom" });
    const three = addTab(two, tab("c"), { nodeId: two.root.id, side: "left" });
    expect(shape(three.root)).toBe("row([*c] col([*a] [*b]))");
  });

  it("refuses a duplicate id, an unknown target and a centre drop on a split", () => {
    const base = singleGroupLayout([tab("a")]);
    const split = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });

    expect(addTab(base, tab("a"), { nodeId: base.root.id, side: "center" })).toBe(base);
    expect(addTab(base, tab("z"), { nodeId: "no-such-node", side: "center" })).toBe(base);
    expect(addTab(split, tab("z"), { nodeId: split.root.id, side: "center" })).toBe(split);
  });

  it("leaves the layout it was handed untouched", () => {
    const base = singleGroupLayout([tab("a")]);
    const before = shape(base.root);
    addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });
    expect(shape(base.root)).toBe(before);
  });
});

describe("normalisation", () => {
  it("flattens a split nested in a split of the same direction", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });
    const three = addTab(two, tab("c"), { nodeId: group(two, "b").id, side: "right" });

    expect(shape(three.root)).toBe("row([*a] [*b] [*c])");
    expect(sum(onlySplit(three).sizes)).toBeCloseTo(1);
    // b and c halved what b had, so the flattened row is 1/2, 1/4, 1/4.
    expect(onlySplit(three).sizes).toEqual([0.5, 0.25, 0.25]);
  });

  it("keeps a split nested in one of the other direction", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });
    const three = addTab(two, tab("c"), { nodeId: group(two, "b").id, side: "bottom" });
    expect(shape(three.root)).toBe("row([*a] col([*b] [*c]))");
  });
});

describe("closeTab", () => {
  it("activates the tab that slid into the closed one's place", () => {
    const base = singleGroupLayout([tab("a"), tab("b"), tab("c")]);
    const active = activateTab(base, "b");
    expect(shape(closeTab(active, "b").root)).toBe("[a *c]");
  });

  it("falls back to the new last tab when the last one was closed", () => {
    const base = activateTab(singleGroupLayout([tab("a"), tab("b")]), "b");
    expect(shape(closeTab(base, "b").root)).toBe("[*a]");
  });

  it("keeps the active tab when a different one is closed", () => {
    const base = singleGroupLayout([tab("a"), tab("b")]);
    expect(shape(closeTab(base, "b").root)).toBe("[*a]");
  });

  it("collapses the emptied group and the split left with one child", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });
    expect(shape(closeTab(two, "b").root)).toBe("[*a]");
  });

  it("keeps the root group, empty, when the last tab goes", () => {
    const base = singleGroupLayout([tab("a")]);
    const closed = closeTab(base, "a");
    expect(isGroup(closed.root)).toBe(true);
    expect(closed.root.id).toBe(base.root.id);
    expect(allTabs(closed)).toEqual([]);
  });

  it("ignores an id that is not in the tree", () => {
    const base = singleGroupLayout([tab("a")]);
    expect(closeTab(base, "nope")).toBe(base);
  });
});

describe("moveTab", () => {
  it("moves a tab into another group", () => {
    const base = singleGroupLayout([tab("a"), tab("b")]);
    const two = addTab(base, tab("c"), { nodeId: base.root.id, side: "right" });
    const moved = moveTab(two, "a", { nodeId: group(two, "c").id, side: "center" });
    expect(shape(moved.root)).toBe("row([*b] [c *a])");
  });

  it("collapses the source group when its last tab leaves", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });
    const moved = moveTab(two, "b", { nodeId: group(two, "a").id, side: "center" });
    expect(shape(moved.root)).toBe("[a *b]");
  });

  it("reorders within a group, counting the slot without the moved tab", () => {
    const base = singleGroupLayout([tab("a"), tab("b"), tab("c")]);
    const moved = moveTab(base, "c", { nodeId: base.root.id, side: "center", index: 0 });
    expect(shape(moved.root)).toBe("[*c a b]");
  });

  it("splits a group with one of its own tabs", () => {
    const base = singleGroupLayout([tab("a"), tab("b")]);
    const moved = moveTab(base, "b", { nodeId: base.root.id, side: "bottom" });
    expect(shape(moved.root)).toBe("col([*a] [*b])");
  });

  it("is a no-op when a group's only tab is dropped back on that group", () => {
    const base = singleGroupLayout([tab("a")]);
    // The identity matters as much as the shape: a tree that renders the same
    // but re-mints the group's id is what breaks CP2's drag state.
    for (const side of ["center", "left", "right", "top", "bottom"] as const) {
      expect(moveTab(base, "a", { nodeId: base.root.id, side })).toBe(base);
    }
  });

  it("ignores an unknown tab or target", () => {
    const base = singleGroupLayout([tab("a"), tab("b")]);
    expect(moveTab(base, "nope", { nodeId: base.root.id, side: "center" })).toBe(base);
    expect(moveTab(base, "a", { nodeId: "no-such-node", side: "right" })).toBe(base);
  });
});

describe("activateTab", () => {
  it("switches the visible tab and leaves an already active one alone", () => {
    const base = singleGroupLayout([tab("a"), tab("b")]);
    expect(shape(activateTab(base, "b").root)).toBe("[a *b]");
    expect(activateTab(base, "a")).toBe(base);
    expect(activateTab(base, "nope")).toBe(base);
  });
});

describe("sizes", () => {
  it("moves only the two panes touching the dragged divider", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });
    const three = addTab(two, tab("c"), { nodeId: group(two, "b").id, side: "right" });

    // The pair either side of the divider shares 0.75; c keeps its quarter.
    const dragged = resizeSplit(three, three.root.id, 0, 0.6);
    const sizes = onlySplit(dragged).sizes;
    expect(sizes[0]).toBeCloseTo(0.6);
    expect(sizes[1]).toBeCloseTo(0.15);
    expect(sizes[2]).toBeCloseTo(0.25);
    expect(sum(sizes)).toBeCloseTo(1);
  });

  it("stops a drag at the minimum rather than letting a pane vanish", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });

    const squashed = resizeSplit(two, two.root.id, 0, 0.99);
    expect(onlySplit(squashed).sizes[1]).toBeCloseTo(MIN_PANE_FRACTION);
    const other = resizeSplit(two, two.root.id, 0, -5);
    expect(onlySplit(other).sizes[0]).toBeCloseTo(MIN_PANE_FRACTION);
  });

  it("ignores a divider index that is not between two children", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });
    expect(resizeSplit(two, two.root.id, 1, 0.5)).toBe(two);
    expect(resizeSplit(two, two.root.id, -1, 0.5)).toBe(two);
    expect(resizeSplit(two, two.root.id, 0.5, 0.5)).toBe(two);
    expect(resizeSplit(two, group(two, "a").id, 0, 0.5)).toBe(two);
    expect(resizeSplit(two, two.root.id, 0, Number.NaN)).toBe(two);
  });

  it("repairs wholesale sizes instead of refusing them", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });

    expect(onlySplit(setSplitSizes(two, two.root.id, [3, 1])).sizes).toEqual([0.75, 0.25]);
    // One usable entry: the missing one takes its average, so the split is even.
    expect(onlySplit(setSplitSizes(two, two.root.id, [2])).sizes).toEqual([0.5, 0.5]);
    expect(sum(onlySplit(setSplitSizes(two, two.root.id, [0, -1])).sizes)).toBeCloseTo(1);
  });

  it("holds the minimum for a preset too, not only for a drag", () => {
    const base = singleGroupLayout([tab("a")]);
    const two = addTab(base, tab("b"), { nodeId: base.root.id, side: "right" });

    const sliver = onlySplit(setSplitSizes(two, two.root.id, [0.001, 0.999])).sizes;
    expect(sliver[0]).toBeCloseTo(MIN_PANE_FRACTION);
    expect(sum(sliver)).toBeCloseTo(1);
  });

  it("shares evenly rather than promising a minimum it cannot keep", () => {
    // Sixteen panes cannot all have 0.08. The floor drops to an even share
    // instead of producing sizes that no longer sum to 1.
    const count = 16;
    const raw = {
      version: LAYOUT_VERSION,
      root: {
        type: "split",
        id: "s1",
        dir: "row",
        sizes: Array.from({ length: count }, (_, i) => (i === 0 ? 0.9 : 0.1 / (count - 1))),
        children: Array.from({ length: count }, (_, i) => ({
          type: "tabs",
          id: `g${i}`,
          active: `t${i}`,
          tabs: [tab(`t${i}`)],
        })),
      },
    };
    const sizes = (parseLayout(raw)!.root as Split).sizes;
    expect(sizes).toHaveLength(count);
    expect(sum(sizes)).toBeCloseTo(1);
    expect(Math.min(...sizes)).toBeCloseTo(1 / count);
  });
});

describe("serialisation", () => {
  function sample(): Layout {
    const base = singleGroupLayout([tab("a"), tab("b")]);
    const two = addTab(base, tab("c", "agent"), { nodeId: base.root.id, side: "right" });
    return addTab(two, tab("d", "diff"), { nodeId: group(two, "c").id, side: "bottom" });
  }

  it("round-trips a layout unchanged", () => {
    const layout = sample();
    const back = parseLayout(serializeLayout(layout));
    expect(back).not.toBeNull();
    expect(shape(back!.root)).toBe(shape(layout.root));
    expect(back!.root).toEqual(layout.root);
  });

  it("writes the schema version", () => {
    expect(JSON.parse(serializeLayout(sample())).version).toBe(LAYOUT_VERSION);
  });

  it("takes an already-parsed value as well as a string", () => {
    const layout = sample();
    const back = parseLayout(JSON.parse(serializeLayout(layout)));
    expect(back!.root).toEqual(layout.root);
  });

  it("returns null for anything it cannot make a layout out of", () => {
    expect(parseLayout("not json at all {")).toBeNull();
    expect(parseLayout(null)).toBeNull();
    expect(parseLayout(42)).toBeNull();
    expect(parseLayout({})).toBeNull();
    expect(parseLayout({ version: 1, root: { type: "tabs", tabs: [] } })).toBeNull();
    expect(parseLayout({ version: 1, root: { type: "dock" } })).toBeNull();
  });

  it("drops tabs without an id or kind, and a second tab reusing an id", () => {
    const raw = {
      version: 1,
      root: {
        type: "tabs",
        id: "g1",
        active: "a",
        tabs: [tab("a"), { id: "b" }, { kind: "terminal" }, tab("a"), tab("c")],
      },
    };
    expect(allTabs(parseLayout(raw)!).map((t) => t.id)).toEqual(["a", "c"]);
  });

  it("falls back to the first tab when the stored active one is gone", () => {
    const raw = { version: 1, root: { type: "tabs", id: "g1", active: "gone", tabs: [tab("a"), tab("b")] } };
    expect(shape(parseLayout(raw)!.root)).toBe("[*a b]");
  });

  it("repairs a split whose sizes do not match its children", () => {
    const raw = {
      version: 1,
      root: {
        type: "split",
        id: "s1",
        dir: "row",
        sizes: [0.5],
        children: [
          { type: "tabs", id: "g1", active: "a", tabs: [tab("a")] },
          { type: "tabs", id: "g2", active: "b", tabs: [tab("b")] },
        ],
      },
    };
    const layout = parseLayout(raw)!;
    expect(sum((layout.root as Split).sizes)).toBeCloseTo(1);
    expect(shape(layout.root)).toBe("row([*a] [*b])");
  });

  it("collapses a split whose children did not survive", () => {
    const raw = {
      version: 1,
      root: {
        type: "split",
        id: "s1",
        dir: "row",
        sizes: [0.5, 0.5],
        children: [{ type: "tabs", id: "g1", tabs: [] }, { type: "tabs", id: "g2", tabs: [tab("a")] }],
      },
    };
    expect(shape(parseLayout(raw)!.root)).toBe("[*a]");
  });

  it("carries a pane's own config through untouched", () => {
    const withConfig: PaneTab = { id: "a", kind: "file", title: "a.rs", config: { path: "/tmp/a.rs", line: 12 } };
    const back = parseLayout(serializeLayout(singleGroupLayout([withConfig])))!;
    expect(allTabs(back)[0].config).toEqual({ path: "/tmp/a.rs", line: 12 });
  });

  it("renames a node that reuses another node's id", () => {
    const raw = {
      version: LAYOUT_VERSION,
      root: {
        type: "split",
        id: "g1",
        dir: "row",
        sizes: [0.5, 0.5],
        children: [
          { type: "tabs", id: "g1", active: "a", tabs: [tab("a")] },
          { type: "tabs", id: "g1", active: "b", tabs: [tab("b")] },
        ],
      },
    };
    const layout = parseLayout(raw)!;
    const ids = [layout.root.id, ...allGroups(layout).map((g) => g.id)];
    expect(new Set(ids).size).toBe(ids.length);
    // And the surviving "g1" is the one a lookup reaches, so a dock target
    // cannot validate against one node and then be written to another.
    expect(findNode(layout, "g1")).toBe(layout.root);
  });

  it("gives an unnamed node an id so every operation still has a handle", () => {
    const layout = parseLayout({ version: 1, root: { type: "tabs", tabs: [tab("a")] } })!;
    expect(layout.root.id).toBeTruthy();
    expect(findNode(layout, layout.root.id)).toBe(layout.root);
  });
});
