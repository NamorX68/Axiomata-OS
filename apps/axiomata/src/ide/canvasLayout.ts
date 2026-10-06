/**
 * How the Canvas lays its panes out as agents come and go: whatever was there before (a terminal the owner opened) keeps
 * a quarter of the width, and the running agents share the rest evenly — up to [`CANVAS_COLUMNS`] side by side, then a
 * second row under the columns with the fewest. Without anything but agents they share the whole width. Pure functions
 * over the dock layout (`layout.ts`); the Flow has its own rule (`planning.flowAgentTarget`).
 */
import {
  allGroups,
  findNode,
  isGroup,
  isSplit,
  setSplitSizes,
  type DockTarget,
  type Layout,
  type LayoutNode,
  type Split,
  type TabGroup,
} from "./layout";

/** How many agent columns stand side by side before a further agent starts a second row. */
export const CANVAS_COLUMNS = 4;

/** The share of the width the panes that are no agent's (a terminal) keep once an agent is open. */
export const CANVAS_OTHER_SHARE = 0.25;

function groupsOf(node: LayoutNode): TabGroup[] {
  return isGroup(node) ? [node] : node.children.flatMap(groupsOf);
}

const isAgentGroup = (group: TabGroup): boolean => group.tabs.some((tab) => tab.kind === "agent");
const hasAgent = (node: LayoutNode): boolean => groupsOf(node).some(isAgentGroup);

/** The row of columns the agents stand in: the deepest row split with a child that holds an agent pane. */
function agentRow(node: LayoutNode): Split | null {
  if (!isSplit(node)) return null;
  for (const child of node.children) {
    const found = agentRow(child);
    if (found) return found;
  }
  return node.dir === "row" && node.children.some(hasAgent) ? node : null;
}

/**
 * Where a new agent pane goes in the Canvas: against the outer edge for the first one, else beside the last column, or
 * under a column once the row is full.
 */
export function canvasAgentTarget(layout: Layout): DockTarget {
  if (allGroups(layout).every((group) => !isAgentGroup(group))) return { nodeId: layout.root.id, side: "right" };
  const row = agentRow(layout.root);
  const cells = row ? row.children.filter(hasAgent) : [layout.root];
  const lastGroup = (cell: LayoutNode): TabGroup => {
    const groups = groupsOf(cell);
    return groups[groups.length - 1];
  };
  if (cells.length < CANVAS_COLUMNS) return { nodeId: lastGroup(cells[cells.length - 1]).id, side: "right" };
  let fewest = cells[0];
  for (const cell of cells) {
    if (groupsOf(cell).filter(isAgentGroup).length < groupsOf(fewest).filter(isAgentGroup).length) fewest = cell;
  }
  return { nodeId: lastGroup(fewest).id, side: "bottom" };
}

/**
 * The layout with the agents' columns of equal width, the other panes sharing [`CANVAS_OTHER_SHARE`], and the rows of a
 * column of equal height.
 */
export function balanceCanvas(layout: Layout): Layout {
  const row = agentRow(layout.root);
  if (!row) return layout;
  const agents = row.children.filter(hasAgent).length;
  const others = row.children.length - agents;
  const sizes = row.children.map((child) =>
    hasAgent(child)
      ? (others > 0 ? 1 - CANVAS_OTHER_SHARE : 1) / agents
      : CANVAS_OTHER_SHARE / others,
  );
  let next = setSplitSizes(layout, row.id, sizes);
  for (const child of row.children) {
    if (isSplit(child) && hasAgent(child)) {
      const fresh = findNode(next, child.id);
      if (fresh && isSplit(fresh)) {
        next = setSplitSizes(next, fresh.id, fresh.children.map(() => 1 / fresh.children.length));
      }
    }
  }
  return next;
}
