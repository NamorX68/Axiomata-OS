/**
 * Where a pane's shell starts: derived from the project, never trusted from
 * the stored layout.
 *
 * A terminal pane carries a `cwd` in its tab config, which is what makes an
 * IDE terminal open in the project rather than wherever the global terminal
 * setting points. That value also travels into `layout_json` — and a stored
 * value outlives the fact it was copied from. "Change path" repoints a
 * project, and a folder freed by a deleted project can later belong to a
 * different one; both leave a tab claiming a directory the project no longer
 * has anything to do with. The security audit for M7.1 CP3 raised it, and the
 * answer is not to validate the stored value harder but to stop relying on it:
 * a terminal pane's `cwd` is *recomputed* from the open project every time the
 * layout is loaded or the project moves.
 *
 * Only terminal panes. M7.2's agent panes live in a git worktree under
 * `~/.axiomata/worktrees/`, which is deliberately **not** inside the project's
 * folder, so they will bring their own rule rather than bend this one.
 */

import { allTabs, setTabConfig, type Layout } from "./layout";

/** The pane kinds whose working directory is the project's folder. */
const PROJECT_ROOTED_KINDS = new Set(["terminal"]);

/**
 * Returns the layout with every project-rooted pane's `cwd` set to `repoRoot`.
 *
 * Panes of other kinds, and any other config a pane holds, are left untouched.
 * The layout is returned unchanged when nothing needed correcting, so this is
 * safe to call on every open.
 */
export function applyProjectCwd(layout: Layout, repoRoot: string): Layout {
  let next = layout;
  for (const tab of allTabs(layout)) {
    if (!PROJECT_ROOTED_KINDS.has(tab.kind)) continue;
    if (tab.config?.cwd === repoRoot) continue;
    next = setTabConfig(next, tab.id, { ...tab.config, cwd: repoRoot });
  }
  return next;
}
