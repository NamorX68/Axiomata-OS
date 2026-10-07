/**
 * The dock panes of M7.3 CP9 (`docs/plans/git-layer.md`, H5, H14): a file in
 * the editor, and an agent's diff in a pane of its own. What a tab of each kind
 * carries, and how to tell "this file / this agent's diff is already open" —
 * opening either a second time brings the existing pane forward instead. Plus
 * the project's Files pane (`docs/plans/editor.md`, ED4, W9, W16), the tree the
 * files are opened from.
 */

import type { FileRef } from "../fileapp/tabs";
import { folderKey, renamedPath } from "../fileapp/treeModel";
import {
  activateTab,
  addTab,
  allGroups,
  allTabs,
  findTab,
  isSplit,
  mapTabs,
  setSplitSizes,
  setTabConfig,
  type Layout,
  type LayoutNode,
  type PaneTab,
} from "./layout";

export const FILE_PANE = "file";
export const AGENT_DIFF_PANE = "agent-diff";
/** The project's file tree (`docs/plans/editor.md`, ED4, W9). */
export const FILES_PANE = "files";

/** The project search (`docs/plans/editor.md`, ED5, T13): one per project layout, ⇧⌘F. */
export const SEARCH_PANE = "search";

/** A Search pane; `focus` is bumped to put the cursor in its field (⇧⌘F on an open one). */
export function searchTab(): PaneTab {
  return { id: crypto.randomUUID(), kind: SEARCH_PANE, title: "Search", config: { focus: Date.now() } };
}

/** The project's git panel (`docs/plans/editor-projekt-werkzeuge.md`, #48): changes, commit, push, branches. */
export const GIT_PANE = "git";
/** A task running in a terminal (Run/Tasks, #50): the tab names the task, never the command line (`ide/taskRuns.ts`). */
export const TASK_PANE = "task";

/** The Flow mode's planning panel (A2A CP-A7b): the plans of a board, their proposals, the owner's yes. One per Flow layout. */
export const PLAN_PANE = "plan";

/** The Flow mode's team pane (A2A CP-A9): what the studio's sessions are doing right now. One per Flow layout. */
export const TEAM_PANE = "team";

/** The Flow mode's graph of a plan (A2A CP-A10, A6): the cards as nodes, "needs first" as lines. One per Flow layout. */
export const GRAPH_PANE = "graph";

export function graphTab(): PaneTab {
  return { id: crypto.randomUUID(), kind: GRAPH_PANE, title: "Flowansicht", config: {} };
}

export function teamTab(): PaneTab {
  return { id: crypto.randomUUID(), kind: TEAM_PANE, title: "Agents", config: {} };
}

export function planTab(): PaneTab {
  return { id: crypto.randomUUID(), kind: PLAN_PANE, title: "Planning", config: {} };
}

export function gitTab(): PaneTab {
  return { id: crypto.randomUUID(), kind: GIT_PANE, title: "Git", config: {} };
}

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
  const tab = frontFileTab(layout);
  const config = tab ? filePaneConfig(tab) : null;
  return config && !config.untitled ? { root: config.root, rel: config.rel } : null;
}

/** The tab `frontFile` is about, or `null`. */
export function frontFileTab(layout: Layout): PaneTab | null {
  const group = allGroups(layout).find((g) => g.tabs.some((t) => t.kind === FILE_PANE));
  return group?.tabs.find((t) => t.id === group.active) ?? null;
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
  /** A preview tab (W7): the next single-click open replaces it, until editing or a double click pins it. */
  preview: boolean;
  /** A new note, not filed yet (`root`/`rel` empty): the editor starts a draft and the tab moves to the file once ⌘S filed it. */
  untitled: boolean;
  /** Changes when the tab is pointed at another file (a preview tab reused): the pane opens `root`/`rel` anew. */
  reopen: number;
}

function baseName(rel: string): string {
  return rel.split("/").pop() || rel;
}

export function fileTab(root: string, rel: string, line: number | null, preview = false): PaneTab {
  const config: FilePaneConfig = { root, rel, line, jump: Date.now(), preview, untitled: false, reopen: 0 };
  return { id: crypto.randomUUID(), kind: FILE_PANE, title: baseName(rel), config: { ...config } };
}

/** A new note (⌘N): a draft that belongs to no file yet. */
export function untitledTab(): PaneTab {
  const config: FilePaneConfig = { root: "", rel: "", line: null, jump: 0, preview: false, untitled: true, reopen: 0 };
  return { id: crypto.randomUUID(), kind: FILE_PANE, title: "New note", config: { ...config } };
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
    preview: c.preview === true,
    untitled: c.untitled === true,
    reopen: typeof c.reopen === "number" ? c.reopen : 0,
  };
}

/** Whether `tab` shows the file `root`/`rel`. */
export function showsFile(tab: PaneTab, root: string, rel: string): boolean {
  const c = filePaneConfig(tab);
  return c !== null && !c.untitled && c.root === root && c.rel === rel;
}

function withFileConfig(layout: Layout, tabId: string, change: (c: FilePaneConfig) => FilePaneConfig, title?: string): Layout {
  const hit = findTab(layout, tabId);
  const c = hit ? filePaneConfig(hit.tab) : null;
  if (!hit || !c) return layout;
  const next = setTabConfig(layout, tabId, { ...hit.tab.config, ...change(c) });
  if (title === undefined) return next;
  return mapTabs(next, (t) => (t.id === tabId ? { ...t, title } : t));
}

/**
 * A tab pointed at another file: its path, title and cursor line follow. With `reopen` (a preview tab
 * reused) the pane opens the file anew; without (a note the editor filed itself) it only learns the name.
 */
export function retargetFileTab(
  layout: Layout,
  tabId: string,
  file: FileRef,
  line: number | null = null,
  reopen = true,
): Layout {
  return withFileConfig(
    layout,
    tabId,
    (c) => ({
      ...c,
      root: file.root,
      rel: file.rel,
      untitled: false,
      line,
      jump: line === null ? c.jump : Date.now(),
      reopen: reopen ? Date.now() : c.reopen,
    }),
    baseName(file.rel),
  );
}

/** A preview tab becomes a tab of its own (editing, a double click). */
export function pinFileTab(layout: Layout, tabId: string): Layout {
  const hit = findTab(layout, tabId);
  return hit && filePaneConfig(hit.tab)?.preview ? withFileConfig(layout, tabId, (c) => ({ ...c, preview: false })) : layout;
}

/**
 * A single click in the tree or a search hit: the file shows in the preview tab, which it replaces when
 * there is one — or in a tab already showing it, or a new preview tab. Same placement as `openOrFocus`.
 */
export function openFilePreview(layout: Layout, file: FileRef, line: number | null, fromTabId: string | null): Layout {
  const shown = allTabs(layout).find((t) => showsFile(t, file.root, file.rel));
  if (shown) return openOrFocus(layout, fileTab(file.root, file.rel, line, filePaneConfig(shown)?.preview ?? false), (t) => t.id === shown.id, fromTabId);
  const preview = allTabs(layout).find((t) => filePaneConfig(t)?.preview);
  if (preview) return activateTab(retargetFileTab(layout, preview.id, file, line), preview.id);
  return openOrFocus(layout, fileTab(file.root, file.rel, line, true), (t) => showsFile(t, file.root, file.rel), fromTabId);
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
 * Where a file opened from `fromTabId` should count as coming from: a click in
 * the Files tree, a search hit or ⌘P (`fromTabId` the tree, the Search pane or
 * nothing) comes from the pane the user last **worked** in — `lastWorkTabId`,
 * the terminal or agent that had the focus before — so a first file becomes a
 * tab there rather than beside whatever group happens to follow the tree
 * (owner, after the first IDE test). Anything else keeps its own origin.
 */
export function fileOrigin(layout: Layout, fromTabId: string | null, lastWorkTabId: string | null): string | null {
  const from = fromTabId ? findTab(layout, fromTabId) : null;
  const helper = !from || from.tab.kind === FILES_PANE || from.tab.kind === SEARCH_PANE || from.tab.kind === GIT_PANE;
  const lastWork = lastWorkTabId ? findTab(layout, lastWorkTabId) : null;
  return helper && lastWork ? lastWork.tab.id : fromTabId;
}

/** Whether focusing `tab` makes it the pane the user works in (not a helper). */
export function isWorkPane(tab: PaneTab): boolean {
  return tab.kind !== FILES_PANE && tab.kind !== SEARCH_PANE && tab.kind !== GIT_PANE;
}

/**
 * Opens `tab`, or brings forward the open tab `match` finds (taking over
 * `tab`'s config, so a file pane jumps to the new line). A new file joins a
 * group that already holds a file — files gather in one place rather than
 * splitting the dock again for each. A first file never splits the dock
 * either (owner, after the first IDE test): it becomes a tab in the group it
 * was opened from — the terminal's, the agent's — or, from the Files pane, in
 * the group beside the tree; dragging it out makes it a pane of its own, and
 * later files then gather there. Anything else (an agent's diffs) docks to the
 * right of the pane it was opened from (`fromTabId`), beside its agent (H5).
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
  if (tab.kind === FILE_PANE) {
    // From the Files pane, the file joins the group next to the tree, not the
    // tree's own narrow column. "Next in reading order" is "next on screen"
    // while the dock is one row — what `layout.ts` flattening same-direction
    // splits keeps it; with nothing beside the tree it docks to its right.
    if (from?.tab.kind === FILES_PANE) {
      const next = groups[groups.findIndex((g) => g.id === from.group.id) + 1];
      return next
        ? addTab(layout, tab, { nodeId: next.id, side: "center" })
        : addTab(layout, tab, { nodeId: from.group.id, side: "right" });
    }
    const home = from?.group.id ?? groups[groups.length - 1]?.id ?? layout.root.id;
    return addTab(layout, tab, { nodeId: home, side: "center" });
  }
  const target = from?.group.id ?? groups[groups.length - 1]?.id ?? layout.root.id;
  return addTab(layout, tab, { nodeId: target, side: "right" });
}

/**
 * A rename in `root` (`files:renamed`): every file tab on `from` — or inside the renamed folder —
 * now names the new path, with its title. The layout is returned as it was when nothing matched.
 */
export function layoutAfterRename(layout: Layout, root: string, from: string, to: string): Layout {
  let changed = false;
  const walk = (node: LayoutNode): LayoutNode => {
    if (isSplit(node)) return { ...node, children: node.children.map(walk) };
    return {
      ...node,
      tabs: node.tabs.map((tab) => {
        const c = filePaneConfig(tab);
        const rel = c && c.root === root ? renamedPath(c.rel, from, to) : null;
        if (!c || rel === null) return tab;
        changed = true;
        return { ...tab, title: baseName(rel), config: { ...tab.config, root: c.root, rel, line: c.line, jump: c.jump } };
      }),
    };
  };
  const next = walk(layout.root);
  return changed ? { ...layout, root: next } : layout;
}

export function taskTab(label: string, taskId: string): PaneTab {
  return { id: crypto.randomUUID(), kind: TASK_PANE, title: label, config: { taskId } };
}

/** The task a task pane runs, or `null`. */
export function taskIdOf(tab: PaneTab): string | null {
  const id = tab.config?.taskId;
  return tab.kind === TASK_PANE && typeof id === "string" ? id : null;
}
