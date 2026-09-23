import { get } from "svelte/store";
import { describe, expect, it, vi } from "vitest";

import type { AgentStatus, IdeAgent } from "../core/backend";
import { createStatusPoller, describeStatus, POLL_INTERVAL_MS, SILENT_OWN_COMMAND_MS } from "./agentStatus";

vi.mock("./agents", () => ({ agentStates: vi.fn(async () => []) }));

function agent(over: Partial<IdeAgent> = {}): IdeAgent {
  return {
    id: 1,
    project_id: 1,
    name: "Builder",
    harness: "opencode",
    command: "",
    model: null,
    env: "",
    created_at: "2026-09-01T00:00:00Z",
    updated_at: "2026-09-01T00:00:00Z",
    worktree_path: null,
    branch: null,
    port: null,
    effective_command: "opencode",
    effective_env: "",
    ...over,
  };
}

function status(over: Partial<AgentStatus> = {}): AgentStatus {
  return { agent_id: 1, state: "working", since: null, started_at: null, plan: null, plan_document: null, ...over };
}

/** A hand-cranked clock and interval, so the test never waits for real time. */
function fakeTimers() {
  let fn: (() => void) | null = null;
  let cleared = 0;
  return {
    setInterval: vi.fn((f: () => void, ms: number) => {
      expect(ms).toBe(POLL_INTERVAL_MS);
      fn = f;
      return 1;
    }),
    clearInterval: vi.fn(() => {
      fn = null;
      cleared++;
    }),
    fire: () => fn?.(),
    running: () => fn !== null,
    cleared: () => cleared,
  };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("describeStatus", () => {
  const started = "2026-09-22T20:00:00Z";
  const at = (seconds: number) => Date.parse(started) + seconds * 1000;

  it("reads the harness's own word", () => {
    expect(describeStatus(status({ state: "waiting" }), agent(), 0)).toMatchObject({ tone: "waiting", label: "waiting" });
    expect(describeStatus(undefined, agent(), 0).tone).toBe("starting");
  });

  it("gives an own command ten seconds to report before calling it unconnected", () => {
    const own = agent({ command: "claude --resume" });
    const quiet = status({ state: "starting", started_at: started });
    expect(describeStatus(quiet, own, at(5)).tone).toBe("starting");
    expect(describeStatus(quiet, own, at(SILENT_OWN_COMMAND_MS / 1000 + 1))).toMatchObject({
      tone: "none",
      label: "no status",
    });
    // The generated command stays "starting" however long it takes.
    expect(describeStatus(quiet, agent(), at(60)).tone).toBe("starting");
    // And an own command that did report is believed.
    expect(describeStatus(status({ state: "idle", started_at: started }), own, at(60)).tone).toBe("idle");
  });

  it("has nothing to say for the mini harness", () => {
    expect(describeStatus(status({ state: "starting" }), agent({ harness: "mini" }), 0).tone).toBe("none");
  });
});

describe("createStatusPoller", () => {
  it("polls the watched project and stops cleanly", async () => {
    const timers = fakeTimers();
    const fetch = vi.fn(async (projectId: number) => [status({ agent_id: projectId * 10 })]);
    const poller = createStatusPoller({ fetch, ...timers, now: () => 42 });

    poller.watch(3);
    await flush();
    expect(get(poller.statuses)).toMatchObject({ projectId: 3, checkedAt: 42 });
    expect(get(poller.statuses).byAgent.get(30)?.state).toBe("working");

    timers.fire();
    await flush();
    expect(fetch).toHaveBeenCalledTimes(2);

    // Watching the same project again is not a second interval.
    poller.watch(3);
    expect(timers.setInterval).toHaveBeenCalledTimes(1);

    poller.watch(null);
    expect(timers.running()).toBe(false);
    expect(get(poller.statuses).byAgent.size).toBe(0);
  });

  it("drops an answer that a project switch has overtaken", async () => {
    const timers = fakeTimers();
    let release: (value: AgentStatus[]) => void = () => {};
    const fetch = vi.fn((projectId: number) =>
      projectId === 1
        ? new Promise<AgentStatus[]>((resolve) => (release = resolve))
        : Promise.resolve([status({ agent_id: 2 })]),
    );
    const poller = createStatusPoller({ fetch, ...timers, now: () => 0 });

    poller.watch(1);
    poller.watch(2);
    await flush();
    release([status({ agent_id: 1 })]);
    await flush();

    const snapshot = get(poller.statuses);
    expect(snapshot.projectId).toBe(2);
    expect([...snapshot.byAgent.keys()]).toEqual([2]);
    expect(timers.cleared()).toBe(1);
  });

  it("keeps the last answer when a tick fails", async () => {
    const timers = fakeTimers();
    const fetch = vi
      .fn<(projectId: number) => Promise<AgentStatus[]>>()
      .mockResolvedValueOnce([status()])
      .mockRejectedValueOnce(new Error("busy"));
    const poller = createStatusPoller({ fetch, ...timers, now: () => 0 });

    poller.watch(1);
    await flush();
    timers.fire();
    await flush();
    expect(get(poller.statuses).byAgent.get(1)?.state).toBe("working");
  });
});
