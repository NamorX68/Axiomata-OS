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

import type { LocationList } from "../fileapp/locationList";
import type { OutlineInfo } from "../fileapp/outlineModel";
import type { FileRef } from "../fileapp/tabs";
import type { DockTarget, PaneTab } from "./layout";

export interface IdeDock {
  /** Make a tab the visible one in its group. */
  activate: (tabId: string) => void;
  /** Close a tab, and with it the pane inside. */
  close: (tabId: string) => void;
  /** Close a tab, asking first when it is a file with unsaved text (the tab's ×, ⌘W, Vi's `:q`). */
  requestClose: (tabId: string) => void;
  /**
   * Opens `tab` beside the pane `fromTabId`, or brings forward the open tab
   * `match` finds (`paneKinds.ts`'s `openOrFocus`) — a file from a diff, an
   * agent's diff in a pane of its own (H5, H14).
   */
  open: (tab: PaneTab, match: (t: PaneTab) => boolean, fromTabId: string | null) => void;
  /** A language server's list of places (ED6.3): shown in the Search pane, opened if need be. */
  showLocations: (list: LocationList) => void;
  /** A file pane's symbols and cursor line, for the sidebar's outline. */
  reportOutline: (file: FileRef, info: OutlineInfo) => void;
  /** Start a task pane's task again (its command line is resolved anew, so a changed confirmation counts). */
  restartTask: (tabId: string) => void;
  /** A preview tab becomes a tab of its own. */
  pin: (tabId: string) => void;
  /** A new note was filed: its tab now names the file. */
  filed: (tabId: string, file: FileRef) => void;
  /** A pane's module changed its config; it belongs on that pane's tab. */
  setConfig: (tabId: string, config: Record<string, unknown>) => void;
  /** A pointer went down on a tab: maybe a click, maybe the start of a drag. */
  startTabDrag: (tabId: string, event: PointerEvent) => void;
  /** A pointer went down on the divider after child `boundary` of a split. */
  startDividerDrag: (splitId: string, boundary: number, event: PointerEvent) => void;
  /** The file in the front tab of the file group, or `null`. Reactive. */
  activeFile: () => FileRef | null;
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
