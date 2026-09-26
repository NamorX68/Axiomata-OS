/**
 * The dock panes of M7.3 CP9 (`docs/plans/git-layer.md`, H5, H14): a file in
 * the editor, and an agent's diff in a pane of its own. What a tab of each kind
 * carries, and how to tell "this file / this agent's diff is already open" —
 * opening either a second time brings the existing pane forward instead. Plus
 * the project's Files pane (`docs/plans/editor.md`, ED4, W9, W16), the tree the
 * files are opened from.
 */

import type { FileRef } from "../fileapp/tabs";
import { folderKey } from "../fileapp/treeModel";
import {
  activateTab,
  addTab,
  allGroups,
  allTabs,
  findTab,
  isSplit,
  setSplitSizes,
  setTabConfig,
  type Layout,
  type PaneTab,
} from "./layout";

export const FILE_PANE = "file";
export const AGENT_DIFF_PANE = "agent-diff";
/** The project's file tree (`docs/plans/editor.md`, ED4, W9). */
export const FILES_PANE = "files";

/** The file-service root of an IDE project (`axiomata-files`' `project:<id>`). */
export function projectRoot(projectId: number): string {
  return `project:${projectId}`;
}

/** A Files pane's config: the open folders, as the tree's `folderKey`s, and the hidden toggle. */
export interface FilesPaneConfig {
  expanded: string[];
  showHidden: boolean;
}

/** A Files pane, its project's top folder open. */
export function filesTab(projectId: number): PaneTab {
  const config: FilesPaneConfig = { expanded: [folderKey(projectRoot(projectId), "")], showHidden: false };
  return { id: crypto.randomUUID(), kind: FILES_PANE, title: "Files", config: { ...config } };
}

/** The share of the width a Files pane starts with: narrow, a column rather than a pane (W16). */
export const FILES_PANE_FRACTION = 0.16;

/**
 * `layout` with a Files pane down its whole left side (W16: new projects start
 * this way). Existing layouts are never changed — this is for a starting one.
 */
export function withFilesPane(layout: Layout, projectId: number): Layout {
  const added = addTab(layout, filesTab(projectId), { nodeId: layout.root.id, side: "left" });
  const root = added.root;
  if (!isSplit(root)) return added;
  const sizes = root.children.map((_, i) =>
    i === 0 ? FILES_PANE_FRACTION : (1 - FILES_PANE_FRACTION) / (root.children.length - 1),
  );
  return setSplitSizes(added, root.id, sizes);
}

/**
 * The file in the front tab of the file group — the group `openOrFocus` sends
 * files to — or `null` when that tab is not a file (or there is no such group).
 * With files dragged into a second group, the first one in reading order counts.
 */
export function frontFile(layout: Layout): FileRef | null {
  const group = allGroups(layout).find((g) => g.tabs.some((t) => t.kind === FILE_PANE));
  const tab = group?.tabs.find((t) => t.id === group.active);
  const config = tab ? filePaneConfig(tab) : null;
  return config ? { root: config.root, rel: config.rel } : null;
}

/** A Files pane's config from a stored tab; missing or malformed fields fall back to closed and not hidden. */
export function filesPaneConfig(tab: PaneTab): FilesPaneConfig {
  const c = tab.config ?? {};
  const expanded = Array.isArray(c.expanded) ? c.expanded.filter((k): k is string => typeof k === "string") : [];
  return { expanded, showHidden: c.showHidden === true };
}

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
 * A first file opened from the Files pane goes left of the pane beside the tree.
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
  const sameKind =
    tab.kind === FILE_PANE ? allGroups(layout).find((g) => g.tabs.some((t) => t.kind === FILE_PANE)) : null;
  if (sameKind) return addTab(layout, tab, { nodeId: sameKind.id, side: "center" });
  const from = fromTabId ? findTab(layout, fromTabId) : null;
  const groups = allGroups(layout);
  // From the Files pane, the file takes its room from the pane next to the
  // tree, not from the tree's narrow column: it docks left of the neighbour.
  // "Next in reading order" is "next on screen" while the dock is one row —
  // what `layout.ts` flattening same-direction splits keeps it; after a drag
  // into a column the file still lands beside a pane, just maybe not the tree.
  if (from?.tab.kind === FILES_PANE) {
    const next = groups[groups.findIndex((g) => g.id === from.group.id) + 1];
    if (next) return addTab(layout, tab, { nodeId: next.id, side: "left" });
  }
  const target = from?.group.id ?? groups[groups.length - 1]?.id ?? layout.root.id;
  return addTab(layout, tab, { nodeId: target, side: "right" });
}
