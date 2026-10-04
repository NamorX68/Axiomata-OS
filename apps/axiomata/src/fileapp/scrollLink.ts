/**
 * Two views scrolled in step without echo (`docs/plans/git-layer.md`, H12): the
 * Markdown source and its preview (ED2, G8), the two sides of a split diff.
 *
 * When one view is scrolled it leads and the other is moved to match — which
 * fires the other's scroll event in turn. Without a guard that echo would move
 * the leader back, and the two would chase each other. The rule is one for all
 * places: whichever view scrolled first stays the leader until neither has
 * scrolled for a moment, and the follower's scroll events meanwhile are echoes.
 * (ED2 had two guards — a time window in the file app, a frame flag in the
 * preview; this replaces both.)
 */

/** How long after its last scroll a view keeps the lead. */
export const SCROLL_ECHO_MS = 120;

export class ScrollLink {
  private leader: string | null = null;
  private until = 0;

  constructor(
    private readonly now: () => number = () => performance.now(),
    private readonly holdMs = SCROLL_ECHO_MS,
  ) {}

  /**
   * View `side` scrolled. Returns whether the other view should follow it —
   * `false` when this scroll is the echo of following the leader.
   */
  scrolled(side: string): boolean {
    const t = this.now();
    if (this.leader !== null && this.leader !== side && t < this.until) return false;
    this.leader = side;
    this.until = t + this.holdMs;
    return true;
  }
}
