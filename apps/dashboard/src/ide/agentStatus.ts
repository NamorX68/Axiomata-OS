/**
 * What the open project's agents are doing, polled once a second while the
 * IDE is on screen (M7.2 CP6/CP6b).
 *
 * The harnesses write their state into a file channel (`axiomata_ide::
 * lifecycle`); `ide_agent_states` reads the whole project's channels in one
 * call. Polling rather than an event push is deliberate for now — only this
 * view is watching, and a one-second tick of a few small file reads is not
 * worth a watcher thread. The real push path is CP12b, and it is the board's.
 *
 * Two things live here rather than in the components:
 *
 * * **A slower answer never wins.** Switching projects while a tick is in
 *   flight must not paint the old project's statuses onto the new one — the
 *   same sequence guard as `projectSession.ts`.
 * * **How a status reads** (`describeStatus`), including the one rule that
 *   needs a clock: an agent started with an own command that has not reported
 *   anything for {@link SILENT_OWN_COMMAND_MS} is not "starting" any more, it
 *   simply has no channel (E13 in `docs/plans/agent-lifecycle.md`).
 */

import { writable, type Readable } from "svelte/store";

import type { AgentState, AgentStatus, IdeAgent } from "../core/backend";

import { agentStates } from "./agents";

/** How often the open project's statuses are read. */
export const POLL_INTERVAL_MS = 1000;

/** How long an own command may stay silent before it counts as unconnected. */
export const SILENT_OWN_COMMAND_MS = 10_000;

export interface StatusSnapshot {
  /** The project these statuses belong to; `null` while nothing is watched. */
  projectId: number | null;
  byAgent: Map<number, AgentStatus>;
  /** When the last tick came back, for rules that need a clock. */
  checkedAt: number;
}

/** A status reduced to what a dot and a word need. */
export interface StatusView {
  /** `none` = there is no channel to listen to. */
  tone: AgentState | "none";
  label: string;
  /** A longer explanation for a tooltip. */
  title: string;
}

const LABELS: Record<AgentState, { label: string; title: string }> = {
  starting: { label: "starting", title: "Started — the harness has not reported yet" },
  idle: { label: "idle", title: "Finished its turn, ready for the next prompt" },
  working: { label: "working", title: "In the middle of a turn" },
  waiting: { label: "waiting", title: "Waiting for you — a permission prompt or a question" },
  ended: { label: "ended", title: "The harness has exited" },
};

/**
 * How an agent's status reads, given its profile and the time.
 *
 * `status` is `undefined` before the first tick has come back, which reads
 * the same as "starting": the pane is up, nothing has been heard yet.
 */
export function describeStatus(status: AgentStatus | undefined, agent: IdeAgent, now: number): StatusView {
  if (agent.harness === "mini") {
    return { tone: "none", label: "no status", title: "The mini harness reports its status from M7.4 on" };
  }
  const state = status?.state ?? "starting";
  if (state === "starting" && agent.command.trim() && status?.started_at) {
    const silentFor = now - Date.parse(status.started_at);
    if (silentFor > SILENT_OWN_COMMAND_MS) {
      return {
        tone: "none",
        label: "no status",
        title:
          "No status channel (own command). Opencode connects on its own; for Claude Code, " +
          'add --settings "$AXIOMATA_CLAUDE_SETTINGS" to the command.',
      };
    }
  }
  return { tone: state, ...LABELS[state] };
}

export interface PollerDeps {
  fetch: (projectId: number) => Promise<AgentStatus[]>;
  setInterval: (fn: () => void, ms: number) => unknown;
  clearInterval: (handle: unknown) => void;
  now: () => number;
}

export interface StatusPoller {
  statuses: Readable<StatusSnapshot>;
  /** Starts watching a project, or stops with `null`. Idempotent. */
  watch: (projectId: number | null) => void;
}

const EMPTY: StatusSnapshot = { projectId: null, byAgent: new Map(), checkedAt: 0 };

/** Builds a poller. The deps are injectable so a test needs no real time. */
/**
 * A plan's title, for the take-over message (`docs/plans/git-layer.md`, H10):
 * the first `# ` heading of Claude Code's plan-mode plan, or `null`. The task
 * list has no title — its first task is a step, not what the work was.
 */
export function planTitle(status: AgentStatus | undefined): string | null {
  return status?.plan_document?.markdown.match(/^#\s+(.+?)\s*$/m)?.[1] ?? null;
}

export function createStatusPoller(deps: PollerDeps): StatusPoller {
  const state = writable<StatusSnapshot>(EMPTY);
  let watching: number | null = null;
  let handle: unknown = null;
  let sequence = 0;

  async function tick(projectId: number) {
    const mine = ++sequence;
    try {
      const statuses = await deps.fetch(projectId);
      // Overtaken by a newer tick or a project switch: drop it.
      if (mine !== sequence || watching !== projectId) return;
      state.set({
        projectId,
        byAgent: new Map(statuses.map((s) => [s.agent_id, s])),
        checkedAt: deps.now(),
      });
    } catch {
      // A failed read keeps the last answer; the next tick tries again. A
      // toast every second would be worse than a status that is a tick old.
    }
  }

  function watch(projectId: number | null) {
    if (projectId === watching) return;
    if (handle !== null) deps.clearInterval(handle);
    handle = null;
    watching = projectId;
    sequence++;
    state.set(EMPTY);
    if (projectId === null) return;
    void tick(projectId);
    handle = deps.setInterval(() => void tick(projectId), POLL_INTERVAL_MS);
  }

  return { statuses: { subscribe: state.subscribe }, watch };
}

/** The app's one poller, driven by `IdeView.svelte`. */
export const agentStatus = createStatusPoller({
  fetch: agentStates,
  setInterval: (fn, ms) => setInterval(fn, ms),
  clearInterval: (handle) => clearInterval(handle as ReturnType<typeof setInterval>),
  now: () => Date.now(),
});
