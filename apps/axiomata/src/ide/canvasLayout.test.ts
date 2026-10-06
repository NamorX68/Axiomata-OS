import { describe, expect, it } from "vitest";

import { CANVAS_COLUMNS, CANVAS_OTHER_SHARE, balanceCanvas, canvasAgentTarget } from "./canvasLayout";
import { addTab, allGroups, singleGroupLayout, type Layout, type PaneTab, type Split } from "./layout";

const terminal = (id: string): PaneTab => ({ id, kind: "terminal", title: "Terminal" });
const agent = (id: string, agentId: number): PaneTab => ({ id, kind: "agent", title: id, config: { agentId } });

/** What `IdeView.openAgent` does in the Canvas. */
function open(layout: Layout, tab: PaneTab): Layout {
  return balanceCanvas(addTab(layout, tab, canvasAgentTarget(layout)));
}

const root = (layout: Layout): Split => layout.root as Split;

describe("the Canvas as agents open", () => {
  it("gives the terminal a quarter of the width as soon as the first agent opens", () => {
    const layout = open(singleGroupLayout([terminal("t")]), agent("a", 1));
    expect(root(layout).dir).toBe("row");
    expect(root(layout).sizes[0]).toBeCloseTo(CANVAS_OTHER_SHARE);
    expect(root(layout).sizes[1]).toBeCloseTo(1 - CANVAS_OTHER_SHARE);
  });

  it("splits the rest evenly between the running agents", () => {
    let layout = singleGroupLayout([terminal("t")]);
    for (const [id, agentId] of [["a", 1], ["b", 2], ["c", 3]] as const) layout = open(layout, agent(id, agentId));
    const sizes = root(layout).sizes;
    expect(sizes[0]).toBeCloseTo(0.25);
    expect(sizes.slice(1).every((size) => Math.abs(size - 0.25) < 1e-9)).toBe(true);
  });

  it("gives the agents the whole width when no terminal is open", () => {
    let layout = singleGroupLayout([agent("a", 1)]);
    layout = open(layout, agent("b", 2));
    layout = open(layout, agent("c", 3));
    expect(root(layout).sizes.every((size) => Math.abs(size - 1 / 3) < 1e-9)).toBe(true);
  });

  it("starts a second row under the column with the fewest agents once the row is full", () => {
    let layout = singleGroupLayout([terminal("t")]);
    for (let i = 1; i <= CANVAS_COLUMNS; i++) layout = open(layout, agent(`a${i}`, i));
    const full = allGroups(layout).length;
    layout = open(layout, agent("extra", 99));
    // The fifth agent went under the first column: still four columns beside the terminal, one of them split.
    expect(root(layout).children).toHaveLength(CANVAS_COLUMNS + 1);
    expect(allGroups(layout)).toHaveLength(full + 1);
    const split = root(layout).children.find((child) => child.type === "split") as Split;
    expect(split.dir).toBe("col");
    expect(split.sizes[0]).toBeCloseTo(0.5);
  });

  it("leaves a layout without agents alone", () => {
    const layout = singleGroupLayout([terminal("t")]);
    expect(balanceCanvas(layout)).toBe(layout);
  });
});
