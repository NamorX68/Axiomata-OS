/**
 * "Open this file of the agent's" from a diff (`docs/plans/git-layer.md`, H5):
 * a file pane on the agent's worktree, beside the pane the request came from,
 * or the pane already showing that file brought forward and moved to the line.
 * Call during component setup — it reads the dock from Svelte context.
 */

import { getDock } from "../dockContext";
import { worktreeRoot } from "../git";
import { fileTab, showsFile } from "../paneKinds";

export function openFileBeside(fromTabId: () => string | null): (agentId: number, rel: string, line: number) => void {
  const dock = getDock();
  return (agentId, rel, line) => {
    const root = worktreeRoot(agentId);
    dock.open(fileTab(root, rel, line), (t) => showsFile(t, root, rel), fromTabId());
  };
}
