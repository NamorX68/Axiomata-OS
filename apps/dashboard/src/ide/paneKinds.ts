/**
 * The dock panes of M7.3 CP9 (`docs/plans/git-layer.md`, H5, H14): a file in
 * the editor, and an agent's diff in a pane of its own. What a tab of each kind
 * carries, and how to tell "this file / this agent's diff is already open" —
 * opening either a second time brings the existing pane forward instead.
 */

import { activateTab, addTab, allGroups, allTabs, findTab, setTabConfig, type Layout, type PaneTab } from "./layout";

export const FILE_PANE = "file";
export const AGENT_DIFF_PANE = "agent-diff";

/** A file pane's config: which file, and a line to put the cursor on. */
export interface FilePaneConfig {
  root: string;
  rel: string;
  /** Zero-based; `null` leaves the cursor where the editor puts it. */
  line: number | null;
  /**
   * Changes with every "open at this line" request, so a pane that is already
   * open jumps again even to the same line it jumped to before.
   */
  jump: number;
}

function baseName(rel: string): string {
  return rel.split("/").pop() || rel;
}

export function fileTab(root: string, rel: string, line: number | null): PaneTab {
  const config: FilePaneConfig = { root, rel, line, jump: Date.now() };
  return { id: crypto.randomUUID(), kind: FILE_PANE, title: baseName(rel), config: { ...config } };
}

/** A file pane's config from a stored tab, or `null` if it is not one or is malformed. */
export function filePaneConfig(tab: PaneTab): FilePaneConfig | null {
  const c = tab.config ?? {};
  if (tab.kind !== FILE_PANE || typeof c.root !== "string" || typeof c.rel !== "string") return null;
  return {
    root: c.root,
    rel: c.rel,
    line: typeof c.line === "number" ? c.line : null,
    jump: typeof c.jump === "number" ? c.jump : 0,
  };
}

/** Whether `tab` shows the file `root`/`rel`. */
export function showsFile(tab: PaneTab, root: string, rel: string): boolean {
  const c = filePaneConfig(tab);
  return c !== null && c.root === root && c.rel === rel;
}

export function agentDiffTab(agentId: number, agentName: string): PaneTab {
  return { id: crypto.randomUUID(), kind: AGENT_DIFF_PANE, title: `${agentName} · Diffs`, config: { agentId } };
}

/** The agent an agent-diff tab belongs to, or `null`. */
export function agentDiffOf(tab: PaneTab): number | null {
  const id = tab.config?.agentId;
  return tab.kind === AGENT_DIFF_PANE && typeof id === "number" ? id : null;
}

/** The agent a file root belongs to (`worktree:<id>`), or `null` for any other root. */
export function worktreeAgent(root: string): number | null {
  const m = /^worktree:(\d+)$/.exec(root);
  return m ? Number(m[1]) : null;
}

/**
 * Opens `tab`, or brings forward the open tab `match` finds (taking over
 * `tab`'s config, so a file pane jumps to the new line). A new file joins a
 * group that already holds a file — files gather in one place rather than
 * splitting the dock again for each; anything else (an agent's diffs) docks to
 * the right of the pane it was opened from (`fromTabId`), beside its agent (H5).
 */
export function openOrFocus(
  layout: Layout,
  tab: PaneTab,
  match: (t: PaneTab) => boolean,
  fromTabId: string | null,
): Layout {
  const existing = allTabs(layout).find(match);
  if (existing) {
    const updated = tab.config ? setTabConfig(layout, existing.id, { ...existing.config, ...tab.config }) : layout;
    return activateTab(updated, existing.id);
  }
  const sameKind = tab.kind === FILE_PANE ? allGroups(layout).find((g) => g.tabs.some((t) => t.kind === FILE_PANE)) : null;
  if (sameKind) return addTab(layout, tab, { nodeId: sameKind.id, side: "center" });
  const from = fromTabId ? findTab(layout, fromTabId) : null;
  const groups = allGroups(layout);
  const target = from?.group.id ?? groups[groups.length - 1]?.id ?? layout.root.id;
  return addTab(layout, tab, { nodeId: target, side: "right" });
}
