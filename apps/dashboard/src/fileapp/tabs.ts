/**
 * The file app's tabs (`docs/plans/editor.md`, ED4, W7, W12), as plain data
 * and pure functions — what the view does with them is `FileAppView`'s.
 *
 * * **One tab per file.** Opening a file that has a tab brings that tab
 *   forward; a new note is one tab too (W4).
 * * **The preview tab** (W7, as in VS Code): opening *to look* (a single click
 *   in the tree) reuses the one preview tab instead of adding another; editing
 *   in it, or opening the file for good, makes it a fixed tab.
 * * **Closing** moves to the neighbour on the right, else the left.
 * * **Kept across restarts** under `settings.editor.tabs` (W12): files and the
 *   active one; a new note comes back through its kept draft.
 */

import { getSetting, setSetting } from "../core/persist";

export interface FileRef {
  root: string;
  rel: string;
}

export interface Tab {
  id: string;
  /** `null`: a new note, not filed yet. */
  file: FileRef | null;
  /** The reusable look-only tab (W7); at most one. */
  preview: boolean;
}

export interface TabsState {
  tabs: Tab[];
  active: string | null;
}

export const NO_TABS: TabsState = { tabs: [], active: null };

function sameFile(a: FileRef | null, b: FileRef | null): boolean {
  if (a === null || b === null) return a === b;
  return a.root === b.root && a.rel === b.rel;
}

/**
 * Opens `file` (`null` for a new note). Returns the new state, the tab it is
 * in, and whether that tab must load it (`false`: it already shows it).
 */
export function openInTabs(
  state: TabsState,
  file: FileRef | null,
  options: { preview: boolean },
  newId: () => string,
): { state: TabsState; target: string; load: boolean } {
  const existing = state.tabs.find((t) => sameFile(t.file, file));
  if (existing) {
    const tabs = options.preview ? state.tabs : pinned(state.tabs, existing.id);
    return { state: { tabs, active: existing.id }, target: existing.id, load: false };
  }
  const reusable = options.preview ? state.tabs.find((t) => t.preview) : undefined;
  if (reusable) {
    const tabs = state.tabs.map((t) => (t.id === reusable.id ? { ...t, file } : t));
    return { state: { tabs, active: reusable.id }, target: reusable.id, load: true };
  }
  const tab: Tab = { id: newId(), file, preview: options.preview && file !== null };
  // After the active tab; with none (or a stale id), at the end.
  const activeAt = state.tabs.findIndex((t) => t.id === state.active);
  const at = activeAt >= 0 ? activeAt + 1 : state.tabs.length;
  const tabs = [...state.tabs.slice(0, at), tab, ...state.tabs.slice(at)];
  return { state: { tabs, active: tab.id }, target: tab.id, load: true };
}

function pinned(tabs: Tab[], id: string): Tab[] {
  return tabs.map((t) => (t.id === id && t.preview ? { ...t, preview: false } : t));
}

/** Tab `id` becomes a fixed tab (it was edited, or double-clicked). */
export function pinTab(state: TabsState, id: string): TabsState {
  const tab = state.tabs.find((t) => t.id === id);
  return tab?.preview ? { ...state, tabs: pinned(state.tabs, id) } : state;
}

/** Closes tab `id`; the active tab moves to its right neighbour, else its left. */
export function closeTab(state: TabsState, id: string): TabsState {
  const at = state.tabs.findIndex((t) => t.id === id);
  if (at < 0) return state;
  const tabs = state.tabs.filter((t) => t.id !== id);
  if (state.active !== id) return { tabs, active: state.active };
  const next = tabs[at] ?? tabs[at - 1] ?? null;
  return { tabs, active: next?.id ?? null };
}

/** The editor in tab `id` moved to another file (a filed note, a followed link). */
export function retargetTab(state: TabsState, id: string, file: FileRef): TabsState {
  return { ...state, tabs: state.tabs.map((t) => (t.id === id ? { ...t, file } : t)) };
}

/** ⌃Tab (`1`) / ⌃⇧Tab (`-1`): the next or previous tab, round. */
export function cycleTab(state: TabsState, dir: 1 | -1): TabsState {
  const n = state.tabs.length;
  if (n === 0) return state;
  const at = state.tabs.findIndex((t) => t.id === state.active);
  return { ...state, active: state.tabs[(at + dir + n) % n].id };
}

/** ⌘1–⌘9: the `n`-th tab (1-based), ⌘9 the last, as in browsers. */
export function nthTab(state: TabsState, n: number): TabsState {
  const tab = n >= 9 ? state.tabs[state.tabs.length - 1] : state.tabs[n - 1];
  return tab ? { ...state, active: tab.id } : state;
}

/** What is kept across restarts. */
export interface SavedTabs {
  tabs: Array<{ root: string; rel: string; preview: boolean } | { newNote: true }>;
  active: number;
}

export function serializeTabs(state: TabsState): SavedTabs {
  return {
    tabs: state.tabs.map((t) => (t.file ? { ...t.file, preview: t.preview } : { newNote: true as const })),
    active: Math.max(0, state.tabs.findIndex((t) => t.id === state.active)),
  };
}

/** Saved tabs back into a state; anything malformed is skipped. */
export function parseTabs(raw: unknown, newId: () => string): TabsState {
  const saved = raw as Partial<SavedTabs> | null;
  if (!saved || !Array.isArray(saved.tabs)) return NO_TABS;
  const tabs: Tab[] = [];
  for (const entry of saved.tabs) {
    const e = entry as Record<string, unknown> | null;
    if (!e || typeof e !== "object") continue;
    if (e.newNote === true) {
      if (!tabs.some((t) => t.file === null)) tabs.push({ id: newId(), file: null, preview: false });
    } else if (typeof e.root === "string" && typeof e.rel === "string" && e.rel) {
      const file = { root: e.root, rel: e.rel };
      if (!tabs.some((t) => sameFile(t.file, file))) tabs.push({ id: newId(), file, preview: e.preview === true });
    }
  }
  const index = typeof saved.active === "number" ? saved.active : 0;
  return { tabs, active: (tabs[index] ?? tabs[0])?.id ?? null };
}

const KEY = "editor";

/** The tabs of the last session (`settings.editor.tabs` in `dashboard.json`). */
export function loadTabs(newId: () => string): TabsState {
  return parseTabs(getSetting<{ tabs?: unknown }>(KEY)?.tabs, newId);
}

export function saveTabs(state: TabsState): void {
  setSetting(KEY, { ...getSetting<Record<string, unknown>>(KEY), tabs: serializeTabs(state) });
}
