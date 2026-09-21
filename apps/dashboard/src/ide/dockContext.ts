/**
 * The handful of callbacks every node in the dock tree needs from the view.
 *
 * Passed through Svelte context rather than props because the tree renders
 * itself recursively (`DockNode` mounts `DockNode`), and threading six
 * callbacks through an unbounded number of levels would make every one of them
 * a place to make a mistake. The view is the single owner of the layout; the
 * nodes only report what the user did to it.
 *
 * `draggingTab` and `hint` are functions, not values: called inside a
 * component's markup they read the view's `$state` and so re-run when it
 * changes, which a value captured at context-creation time would not.
 */

import { getContext, setContext } from "svelte";

import type { DockTarget } from "./layout";

export interface IdeDock {
  /** Make a tab the visible one in its group. */
  activate: (tabId: string) => void;
  /** Close a tab, and with it the pane inside. */
  close: (tabId: string) => void;
  /** Open a new pane in that group — the tab bar's `+`. */
  addPane: (groupId: string) => void;
  /** A pane's module changed its config; it belongs on that pane's tab. */
  setConfig: (tabId: string, config: Record<string, unknown>) => void;
  /** A pointer went down on a tab: maybe a click, maybe the start of a drag. */
  startTabDrag: (tabId: string, event: PointerEvent) => void;
  /** A pointer went down on the divider after child `boundary` of a split. */
  startDividerDrag: (splitId: string, boundary: number, event: PointerEvent) => void;
  /** The tab being dragged right now, or `null`. Reactive. */
  draggingTab: () => string | null;
  /** Where the current drag would land, or `null`. Reactive. */
  hint: () => DockTarget | null;
}

const DOCK_KEY = Symbol("ide-dock");

export function setDock(dock: IdeDock): void {
  setContext(DOCK_KEY, dock);
}

export function getDock(): IdeDock {
  return getContext<IdeDock>(DOCK_KEY);
}
