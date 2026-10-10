/**
 * Whether an Opencode session's team tools can be reached, and when the pane should bring them back (card #72).
 *
 * The service runs a location's MCP servers per directory and drops a location that sat idle; the server goes with
 * it while the pane still shows an idle session, and mail to that session comes back undeliverable. The pane asks
 * the backend every {@link WATCH_INTERVAL_MS} and registers the server again when it is gone. The rules live here,
 * pure, so they are tested without a component.
 */
import type { AgentState } from "../core/backend";

/** What `ide_session_reach` answers. */
export type ReachAnswer =
  | { state: "reachable" }
  | { state: "reconnected" }
  | { state: "unreachable"; reason: string | null }
  | { state: "not_applicable" };

/** What the pane shows about the team tools beyond the entry written at the start. */
export type ReachView = "unknown" | "reachable" | "reconnecting" | "unreachable" | "not_applicable";

/** How often a running session is looked at. */
export const WATCH_INTERVAL_MS = 30_000;

/** Wait before the first look: a start has just registered the server, and its lock appears a moment later. */
export const FIRST_LOOK_MS = 5_000;

/** Failed attempts in a row after which the pane stops trying by itself; the Reconnect button still works. */
export const MAX_AUTO_ATTEMPTS = 3;

/** What the pane remembers between two looks. */
export interface ReachMemory {
  /** Looks in a row that found no server. */
  misses: number;
  /** Repairs in a row that did not bring it back. */
  failedHeals: number;
}

export const FRESH_MEMORY: ReachMemory = { misses: 0, failedHeals: 0 };

/**
 * Whether the harness is running, so that registering a server for it is right. An ended one must stay unreachable:
 * a server registered for it would make a finished session look alive.
 */
export function harnessRuns(state: AgentState | undefined): boolean {
  return state === "idle" || state === "working" || state === "waiting";
}

/**
 * The status to hand the backend as permission to repair, or `null` for a look only. A repair needs a running
 * harness, automatic attempts left, and a second miss in a row: a server that is just starting has not taken its
 * lock yet, and registering anew would change its secret under it.
 */
export function healWhile(state: AgentState | undefined, memory: ReachMemory): AgentState | null {
  if (!harnessRuns(state) || state === undefined) return null;
  return memory.failedHeals < MAX_AUTO_ATTEMPTS && memory.misses >= 1 ? state : null;
}

export interface ReachStep {
  view: ReachView;
  /** Why the server could not be brought back, when it could not. */
  reason: string | null;
  memory: ReachMemory;
}

/** Folds one answer into what the pane knows. `healed` says whether the request was allowed to repair. */
export function applyAnswer(answer: ReachAnswer, healed: boolean, memory: ReachMemory): ReachStep {
  switch (answer.state) {
    case "reachable":
    case "reconnected":
      return { view: "reachable", reason: null, memory: FRESH_MEMORY };
    case "unreachable": {
      const next = {
        misses: memory.misses + 1,
        failedHeals: healed ? memory.failedHeals + 1 : memory.failedHeals,
      };
      // The first miss alone is not shown: a server that is just starting looks the same.
      const confirmed = healed || next.misses >= 2;
      return { view: confirmed ? "unreachable" : "unknown", reason: confirmed ? answer.reason : null, memory: next };
    }
    case "not_applicable":
      return { view: "not_applicable", reason: null, memory: FRESH_MEMORY };
  }
}
