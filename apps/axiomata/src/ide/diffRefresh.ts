/**
 * When the Diffs view reloads by itself (`docs/plans/git-layer.md`, G4): when it
 * comes into view, when the agent stops working, and every few seconds while it
 * is in view **and** the agent works. Nothing polls while it is hidden or the
 * agent is idle — the manual ↻ (⌘R) covers everything else.
 */

import type { AgentState } from "../core/backend";

/** The poll interval while the agent is working and the view is visible (G4). */
export const DIFF_POLL_MS = 5000;

export class DiffRefresh {
  private visible = false;
  private state: AgentState | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(
    private readonly refresh: () => void,
    private readonly pollMs = DIFF_POLL_MS,
  ) {}

  /** Tell it whenever the view's visibility or the agent's state changes. */
  update(visible: boolean, state: AgentState | null): void {
    const cameIntoView = visible && !this.visible;
    // Working → anything else (idle, waiting for a permission, ended): the
    // agent has just put something down worth looking at.
    const stopped = this.state === "working" && state !== "working" && state !== null;
    this.visible = visible;
    this.state = state;
    if (cameIntoView || (visible && stopped)) this.refresh();
    this.schedule();
  }

  dispose(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  private schedule(): void {
    const polling = this.visible && this.state === "working";
    if (polling && !this.timer) this.timer = setInterval(this.refresh, this.pollMs);
    else if (!polling) this.dispose();
  }
}
