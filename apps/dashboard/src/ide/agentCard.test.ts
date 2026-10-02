import { describe, expect, it } from "vitest";

import type { AgentStatus, IdeAgent, PlanStep } from "../core/backend";
import { cardOf } from "./agentCard";

const agent = { id: 1, name: "Builder", harness: "claude_code", command: "", model: null } as unknown as IdeAgent;
const NOW = Date.parse("2026-10-02T12:00:00Z");

function status(partial: Partial<AgentStatus>): AgentStatus {
  return {
    agent_id: 1,
    state: "working",
    since: "2026-10-02T11:57:00Z",
    started_at: null,
    plan: null,
    plan_document: null,
    ...partial,
  };
}
const step = (text: string, state: PlanStep["state"]): PlanStep => ({ text, state, detail: null });

describe("cardOf", () => {
  it("names the step in progress and counts the finished ones", () => {
    const plan = { steps: [step("read", "done"), step("write", "doing"), step("test", "todo"), step("old", "cancelled")], updated_at: null, from_earlier_session: false };
    const card = cardOf(agent, status({ plan }), true, NOW);
    expect(card.step).toBe("write");
    expect(card.progress).toEqual({ done: 1, total: 3 });
    expect(card.since).toBe("3 min ago");
    expect(card.status.tone).toBe("working");
  });

  it("falls back to the next step to do when none is in progress", () => {
    const plan = { steps: [step("read", "done"), step("test", "todo")], updated_at: null, from_earlier_session: false };
    expect(cardOf(agent, status({ plan, state: "idle" }), true, NOW).step).toBe("test");
  });

  it("has no step or progress without a task list, and reads a pane-less idle agent as closed", () => {
    const card = cardOf(agent, status({ state: "idle" }), false, NOW);
    expect(card.step).toBeNull();
    expect(card.progress).toBeNull();
    expect(card.status.tone).toBe("ended");
  });

  it("takes the plan title from the plan document", () => {
    const plan_document = { markdown: "# Fix the thing\n\nbody", name: "x", updated_at: null, from_earlier_session: false };
    expect(cardOf(agent, status({ plan_document }), true, NOW).plan).toBe("Fix the thing");
  });
});
