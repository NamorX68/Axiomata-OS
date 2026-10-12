/**
 * Whose trail it is (`docs/plans/ui-ux-round.md` UX6c, G8): the agent that changed an open file, as far as the file's
 * root can tell — and nobody when it cannot, so a change the owner made elsewhere never carries an agent's flag.
 */

import type { IdeAgent } from "../core/backend";
import { worktreeAgent } from "../ide/paneKinds";

/** The agent behind a change to a file under `root`, or `null` when it is not clear. */
export function trailAuthor(
  root: string,
  agents: readonly IdeAgent[],
  isWorking: (agentId: number) => boolean,
): IdeAgent | null {
  // A worktree is one agent's own: whatever changes there, it changed.
  const own = worktreeAgent(root);
  if (own !== null) return agents.find((a) => a.id === own) ?? null;
  // A project's own folder is shared by the owner and the agents without a worktree: only if exactly one of them is
  // at work right now is the change plausibly theirs.
  const project = /^project:(\d+)$/.exec(root);
  if (!project) return null;
  const id = Number(project[1]);
  const writers = agents.filter((a) => a.project_id === id && a.worktree_path === null && isWorking(a.id));
  return writers.length === 1 ? (writers[0] ?? null) : null;
}

/** An agent's trail as the editor surface draws it: the lines that glow, and the agent's flag at the last one. */
export interface EditorTrail {
  lines: ReadonlySet<number>;
  /** When the change came in; a new one restarts the glow. */
  at: number;
  /** How long the trail stays on screen, in ms: the flag fades out at its end. */
  ms: number;
  /** Who did it, if that is known: a hex avatar's letters and ring, the agent's name, and the harness for the label. */
  flag: { line: number; text: string; ring: string; name: string; label: string } | null;
}
