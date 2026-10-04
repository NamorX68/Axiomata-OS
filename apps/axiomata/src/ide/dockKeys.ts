/**
 * The tab keys of the editor (⌃Tab, ⌘1–9, ⌘\) on the IDE's dock layout, as plain functions.
 *
 * The IDE has no stored "focused group"; the caller names the tab the user last worked in
 * (`lastWorkTab` in `IdeView`) and the group holding it counts as focused. Without one, or with a
 * stale id, the first group does.
 */

import { activateTab, allGroups, findTab, moveTab, type Layout, type TabGroup } from "./layout";

/** The group holding `tabId`, else the first group, else `null` (an empty layout). */
export function focusedGroupOf(layout: Layout, tabId: string | null): TabGroup | null {
  const found = tabId ? findTab(layout, tabId)?.group : undefined;
  return found ?? allGroups(layout)[0] ?? null;
}

/** ⌃Tab / ⇧⌃Tab: the next or previous tab of the focused group, wrapping round. */
export function cycleTab(layout: Layout, tabId: string | null, dir: 1 | -1): Layout {
  const group = focusedGroupOf(layout, tabId);
  if (!group || group.tabs.length === 0) return layout;
  const at = group.tabs.findIndex((t) => t.id === group.active);
  const n = group.tabs.length;
  return activateTab(layout, group.tabs[(at + dir + n) % n].id);
}

/** ⌘1–⌘9: the `n`-th tab (1-based) of the focused group, ⌘9 the last, as in browsers. */
export function nthTab(layout: Layout, tabId: string | null, n: number): Layout {
  const tabs = focusedGroupOf(layout, tabId)?.tabs ?? [];
  const tab = n >= 9 ? tabs[tabs.length - 1] : tabs[n - 1];
  return tab ? activateTab(layout, tab.id) : layout;
}

/** ⌘\ (right) and ⇧⌘\ (below): the group's visible tab moves into a new group beside it. A lone tab stays. */
export function splitActive(layout: Layout, tabId: string | null, side: "right" | "bottom"): Layout {
  const group = focusedGroupOf(layout, tabId);
  if (!group || group.tabs.length < 2 || !group.active) return layout;
  return moveTab(layout, group.active, { nodeId: group.id, side });
}
