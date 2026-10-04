/**
 * When a pane may ask for, and type, a "you have mail" line into its agent's terminal (A2A A8, way 2).
 *
 * MCP is request and answer: the mailbox server cannot speak to a terminal agent by itself, so the studio types one
 * short line when the agent is waiting for its next prompt. Three rules keep that from being a nuisance, and this
 * module is where they live so they can be tested without a terminal:
 *
 * * only an **idle** agent — never a working one (it reads its inbox itself), never a waiting one (the line would
 *   answer its permission prompt);
 * * never while the **owner is typing** in this very pane — a line in the middle of their sentence is worse than a
 *   late nudge;
 * * not more often than every {@link ASK_INTERVAL_MS}, and never two at once — the status ticks every second and the
 *   backend counts how often each message was announced.
 */
import type { AgentState } from "../core/backend";

/**
 * How long the owner must have left the pane alone before a line is typed into it. Long on purpose: a half-written
 * prompt that sits in the input would get the line appended and sent, so a pause must be a real one.
 */
export const OWNER_QUIET_MS = 30_000;

/** The least time between two questions to the backend for one pane. */
export const ASK_INTERVAL_MS = 3_000;

/** Whether the owner has left the pane alone long enough — asked again right before typing, after the awaits. */
export function ownerIsQuiet(sinceOwnInputMs: number | null): boolean {
  return sinceOwnInputMs === null || sinceOwnInputMs >= OWNER_QUIET_MS;
}

export interface NudgeSituation {
  /** The agent's status; `undefined` before the first tick. */
  state: AgentState | undefined;
  /** How long ago the owner last typed here, `null` if never. */
  sinceOwnInputMs: number | null;
  /** How long ago this pane last asked the backend, `null` if never. */
  sinceAskMs: number | null;
  /** A nudge is being typed right now. */
  busy: boolean;
}

/** Whether this pane should ask the backend for a nudge line now. */
export function shouldAsk(s: NudgeSituation): boolean {
  if (s.busy || s.state !== "idle") return false;
  if (s.sinceAskMs !== null && s.sinceAskMs < ASK_INTERVAL_MS) return false;
  return ownerIsQuiet(s.sinceOwnInputMs);
}
