/**
 * The one answer to "which file is the user looking at" in the IDE: the outline,
 * the breadcrumbs, the tree's highlight, the outline's jump, the debugger and the
 * dock's `activeFile` all ask here, so two files side by side never disagree about
 * which one is active. Pure — no Svelte, no DOM — so the rule is tested on its own.
 */

import { findTab, type Layout, type PaneTab } from "./layout";
import { filePaneConfig, frontFileTab } from "./paneKinds";

/**
 * `tabId` as a file tab that is on screen: a real file (not an untitled note)
 * and the active tab of its group. A tab hidden behind another one does not count.
 */
function onScreenFile(layout: Layout, tabId: string | null): PaneTab | null {
  if (!tabId) return null;
  const hit = findTab(layout, tabId);
  if (!hit || hit.group.active !== hit.tab.id) return null;
  const config = filePaneConfig(hit.tab);
  return config && !config.untitled ? hit.tab : null;
}

/**
 * The file tab whose document the IDE shows, or `null`.
 *
 * The pane the user last worked in wins when it shows a file. Otherwise the
 * file pane the user last focused, if it is still on screen — a terminal or an
 * agent in the other pane does not take the outline away from the document. Only
 * when no file has been focused on screen does the front file group decide.
 */
export function shownFileTab(
  layout: Layout,
  lastWorkTabId: string | null,
  lastFocusedFileTabId: string | null,
): PaneTab | null {
  const work = onScreenFile(layout, lastWorkTabId);
  if (work) return work;
  const focused = onScreenFile(layout, lastFocusedFileTabId);
  if (focused) return focused;
  return onScreenFile(layout, frontFileTab(layout)?.id ?? null);
}
