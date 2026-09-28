/**
 * The file app's tabs in groups side by side or stacked (`docs/plans/editor-look.md`,
 * LK2, K4), as plain data and pure functions — what the view does with them is
 * `FileAppView`'s. The tree is the IDE's dock layout (`ide/layout.ts`); this
 * module adds what makes its tabs *files*:
 *
 * * **One tab per file, in the whole layout** (owner, LK2): opening a file that
 *   has a tab anywhere brings that tab forward and focuses its group. A file
 *   is never open twice — the same text in two groups would share one cursor
 *   (the selection lives in the document); two cursors on one file are a later
 *   checkpoint.
 * * **The focused group** is where new tabs open, ⌃Tab and ⌘1–9 go, and ⌘\\
 *   splits from.
 * * **The preview tab** (W7): opening *to look* reuses the focused group's
 *   preview tab; editing in it, or opening the file for good, makes it fixed.
 * * **Closing** moves to the neighbour on the right, else the left (the
 *   layout's own rule); an emptied group disappears.
 * * **Kept across restarts** under `settings.editor.dock`; tabs saved before
 *   groups existed (`settings.editor.tabs`) come back as one group.
 *
 * A tab's file is its `config` (`{ file, preview }`); `null` is a new note (at
 * most one).
 */

import { getSetting, setSetting } from "../core/persist";
import {
  activateTab,
  addTab,
  allGroups,
  allTabs,
  closeTab as closeLayoutTab,
  emptyLayout,
  findNode,
  findTab,
  isGroup,
  moveTab,
  parseLayout,
  setTabConfig,
  singleGroupLayout,
  type DockTarget,
  type Layout,
  type PaneTab,
  type TabGroup,
} from "../ide/layout";
import { FOREIGN_ROOT } from "./session";
import { parseTabs, type FileRef, type Tab } from "./tabs";

export type { FileRef, Tab };

/** The pane kind of a file tab in the layout. */
export const FILE_TAB = "file";

export interface FileDock {
  layout: Layout;
  /** The group new tabs open in; a stale id means the first group. */
  focus: string | null;
}

export function emptyDock(): FileDock {
  return { layout: emptyLayout(), focus: null };
}

function paneOf(tab: Tab): PaneTab {
  return { id: tab.id, kind: FILE_TAB, title: "", config: { file: tab.file, preview: tab.preview } };
}

/** A layout tab as a file tab. */
export function tabOf(pane: PaneTab): Tab {
  const config = (pane.config ?? {}) as { file?: FileRef | null; preview?: boolean };
  return { id: pane.id, file: config.file ?? null, preview: config.preview === true };
}

function sameFile(a: FileRef | null, b: FileRef | null): boolean {
  if (a === null || b === null) return a === b;
  return a.root === b.root && a.rel === b.rel;
}

/** Every tab, in reading order (left to right, top to bottom). */
export function tabsOf(dock: FileDock): Tab[] {
  return allTabs(dock.layout).map(tabOf);
}

/** The group with the focus: the focused one if it still exists, else the first. */
export function focusedGroup(dock: FileDock): TabGroup {
  const node = dock.focus ? findNode(dock.layout, dock.focus) : null;
  return node && isGroup(node) ? node : allGroups(dock.layout)[0];
}

/** The focused group's visible tab — the one the header and the keys are about. */
export function activeTab(dock: FileDock): Tab | null {
  const group = focusedGroup(dock);
  const pane = group.tabs.find((t) => t.id === group.active);
  return pane ? tabOf(pane) : null;
}

/** Tab `id` becomes visible in its group, and its group gets the focus. */
export function focusTab(dock: FileDock, id: string): FileDock {
  const hit = findTab(dock.layout, id);
  if (!hit) return dock;
  const layout = activateTab(dock.layout, id);
  return { layout, focus: findTab(layout, id)?.group.id ?? null };
}

/**
 * Opens `file` (`null` for a new note). Returns the new dock, the tab it is in,
 * and whether that tab must load it (`false`: it already shows it).
 */
export function openInDock(
  dock: FileDock,
  file: FileRef | null,
  options: { preview: boolean },
  newId: () => string,
): { dock: FileDock; target: string; load: boolean } {
  const existing = tabsOf(dock).find((t) => sameFile(t.file, file));
  if (existing) {
    const pinned = options.preview ? dock : pinTab(dock, existing.id);
    return { dock: focusTab(pinned, existing.id), target: existing.id, load: false };
  }
  const group = focusedGroup(dock);
  const reusable = options.preview ? group.tabs.map(tabOf).find((t) => t.preview) : undefined;
  if (reusable) {
    const layout = setTabConfig(dock.layout, reusable.id, { file, preview: true });
    return { dock: focusTab({ ...dock, layout }, reusable.id), target: reusable.id, load: true };
  }
  const tab: Tab = { id: newId(), file, preview: options.preview && file !== null };
  // After the group's visible tab; with none, at the end.
  const activeAt = group.tabs.findIndex((t) => t.id === group.active);
  const index = activeAt >= 0 ? activeAt + 1 : group.tabs.length;
  const layout = addTab(dock.layout, paneOf(tab), { nodeId: group.id, side: "center", index });
  return { dock: focusTab({ ...dock, layout }, tab.id), target: tab.id, load: true };
}

/** Tab `id` becomes a fixed tab (it was edited, or opened for good). */
export function pinTab(dock: FileDock, id: string): FileDock {
  const hit = findTab(dock.layout, id);
  if (!hit || !tabOf(hit.tab).preview) return dock;
  return { ...dock, layout: setTabConfig(dock.layout, id, { file: tabOf(hit.tab).file, preview: false }) };
}

/** The editor in tab `id` moved to another file (a filed note, a followed link, a rename). */
export function retargetTab(dock: FileDock, id: string, file: FileRef): FileDock {
  const hit = findTab(dock.layout, id);
  if (!hit) return dock;
  return { ...dock, layout: setTabConfig(dock.layout, id, { file, preview: tabOf(hit.tab).preview }) };
}

/** Closes tab `id`; its neighbour becomes visible, an emptied group goes, the focus stays where it can. */
export function closeTab(dock: FileDock, id: string): FileDock {
  const hit = findTab(dock.layout, id);
  if (!hit) return dock;
  const layout = closeLayoutTab(dock.layout, id);
  const stays = dock.focus !== null && findNode(layout, dock.focus) !== null;
  // The closed tab's group is gone: the focus goes to whichever group now holds its old place.
  return { layout, focus: stays ? dock.focus : (allGroups(layout)[0]?.id ?? null) };
}

/** ⌃Tab (`1`) / ⌃⇧Tab (`-1`): the next or previous tab of the focused group, round. */
export function cycleTab(dock: FileDock, dir: 1 | -1): FileDock {
  const group = focusedGroup(dock);
  const n = group.tabs.length;
  if (n === 0) return dock;
  const at = group.tabs.findIndex((t) => t.id === group.active);
  return focusTab(dock, group.tabs[(at + dir + n) % n].id);
}

/** ⌘1–⌘9: the `n`-th tab (1-based) of the focused group, ⌘9 the last, as in browsers. */
export function nthTab(dock: FileDock, n: number): FileDock {
  const tabs = focusedGroup(dock).tabs;
  const tab = n >= 9 ? tabs[tabs.length - 1] : tabs[n - 1];
  return tab ? focusTab(dock, tab.id) : dock;
}

/**
 * ⌘\\ (right) and ⇧⌘\\ (below): the focused group's visible tab moves into a new
 * group beside it, which gets the focus. A group with a single tab has nothing
 * to leave behind — a file is never open twice — so the dock stays as it is.
 */
export function splitActive(dock: FileDock, side: "right" | "bottom"): FileDock {
  const group = focusedGroup(dock);
  if (group.tabs.length < 2 || !group.active) return dock;
  return moveTabTo(dock, group.active, { nodeId: group.id, side });
}

/** A dragged tab dropped on a group's middle (joins its tabs) or edge (splits it). */
export function moveTabTo(dock: FileDock, id: string, target: DockTarget): FileDock {
  const layout = moveTab(dock.layout, id, target);
  return layout === dock.layout ? dock : focusTab({ ...dock, layout }, id);
}

/** What is kept across restarts. */
export interface SavedDock {
  layout: unknown;
  /** The focused group's position among the groups, in reading order. */
  focus: number;
}

export function serializeDock(dock: FileDock): SavedDock {
  // A language server's file (`lsp:<handle>`) is readable only while that server runs; it is not kept.
  let layout = dock.layout;
  for (const tab of tabsOf(dock)) {
    if (tab.file?.root.startsWith(FOREIGN_ROOT)) layout = closeLayoutTab(layout, tab.id);
  }
  const focus = focusedGroup({ layout, focus: dock.focus }).id;
  return {
    layout: JSON.parse(JSON.stringify({ root: layout.root })),
    focus: Math.max(0, allGroups(layout).findIndex((g) => g.id === focus)),
  };
}

/**
 * A saved dock back — anything malformed is dropped: a tab that is not a
 * file tab, a second tab on one file, a second new note. A tab gets a fresh id
 * (ids are per run); `null` for nothing usable.
 */
export function parseDock(raw: unknown, newId: () => string): FileDock | null {
  const saved = raw as Partial<SavedDock> | null;
  const parsed = saved && typeof saved === "object" ? parseLayout(saved.layout) : null;
  if (!parsed) return null;
  let layout = parsed;
  const seen: (FileRef | null)[] = [];
  for (const pane of allTabs(parsed)) {
    const config = pane.config as { file?: unknown } | undefined;
    const file = validFile(config?.file);
    const usable = pane.kind === FILE_TAB && file !== undefined && !seen.some((f) => sameFile(f, file));
    if (!usable) {
      layout = closeLayoutTab(layout, pane.id);
      continue;
    }
    seen.push(file);
    layout = renameTab(layout, pane.id, newId());
  }
  const groups = allGroups(layout);
  const focus = groups[typeof saved?.focus === "number" ? saved.focus : 0] ?? groups[0];
  return { layout, focus: focus?.id ?? null };
}

/** A stored `file`: `null` (a new note), a root and a path, or `undefined` when it is neither. */
function validFile(raw: unknown): FileRef | null | undefined {
  if (raw === null) return null;
  const f = raw as { root?: unknown; rel?: unknown } | undefined;
  return typeof f?.root === "string" && typeof f.rel === "string" && f.rel ? { root: f.root, rel: f.rel } : undefined;
}

/** The layout with tab `from` carrying id `to` (and staying the visible one if it was). */
function renameTab(layout: Layout, from: string, to: string): Layout {
  const rename = (node: Layout["root"]): Layout["root"] =>
    isGroup(node)
      ? {
          ...node,
          tabs: node.tabs.map((t) => (t.id === from ? { ...t, id: to } : t)),
          active: node.active === from ? to : node.active,
        }
      : { ...node, children: node.children.map(rename) };
  return { root: rename(layout.root) };
}

const KEY = "editor";

/** The tabs of the last session: the dock, else the tabs saved before groups existed, as one group. */
export function loadDock(newId: () => string): FileDock {
  const settings = getSetting<{ dock?: unknown; tabs?: unknown }>(KEY);
  const dock = parseDock(settings?.dock, newId);
  if (dock) return dock;
  const legacy = parseTabs(settings?.tabs, newId);
  const layout = singleGroupLayout(legacy.tabs.map(paneOf));
  const group = allGroups(layout)[0];
  const withActive = legacy.active ? activateTab(layout, legacy.active) : layout;
  return { layout: withActive, focus: group?.id ?? null };
}

export function saveDock(dock: FileDock): void {
  const { tabs: _legacy, ...rest } = getSetting<Record<string, unknown>>(KEY) ?? {};
  setSetting(KEY, { ...rest, dock: serializeDock(dock) });
}
