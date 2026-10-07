import { describe, expect, it } from "vitest";

import type { BoardCard, BoardPlan, IdeAgent, TaskState } from "../core/backend";
import { dutyLabel, dutyOf, groupsOf, nowAt, nowLine, shortPath, stepLabel } from "./team";

function agent(id: number, extra: Partial<IdeAgent> = {}): IdeAgent {
  return { id, name: `s${id}`, agent_role: "builder", ...extra } as IdeAgent;
}

function card(id: number, plan: number | null, state: TaskState): BoardCard {
  return { id, plan_id: plan, state, title: `c${id}` } as BoardCard;
}

const plans = [{ id: 1, name: "Docs" }, { id: 2, name: "Tests" }] as BoardPlan[];

describe("who a session is", () => {
  it("tells a planner, a worker, a reviewer and a session made by hand apart", () => {
    expect(dutyOf(agent(1, { plan_id: 1 }))).toBe("planner");
    expect(dutyOf(agent(2, { card_id: 5 }))).toBe("worker");
    expect(dutyOf(agent(3, { card_id: 5, card_review: true }))).toBe("reviewer");
    expect(dutyOf(agent(4))).toBe("own");
    expect(dutyLabel("reviewer")).toBe("reviews");
  });
});

describe("the team's groups", () => {
  it("leaves out sessions made by hand and groups the rest by the plan of their card", () => {
    const groups = groupsOf(
      [agent(1, { card_id: 10 }), agent(2, { card_id: 20 }), agent(3), agent(4, { plan_id: 1 }), agent(5, { card_id: 30 })],
      [card(10, 1, "working"), card(20, 2, "working"), card(30, null, "working")],
      plans,
    );
    expect(groups.map((g) => [g.planId, g.tiles.map((t) => t.agent.id)])).toEqual([
      [1, [4, 1]],
      [2, [2]],
      [null, [5]],
    ]);
    expect(groups[0].title).toContain("Docs");
    expect(groups[2].title).toBe("No plan");
  });

  it("puts the one that needs the owner first, and a reviewer after the worker it judges", () => {
    const groups = groupsOf(
      [
        agent(1, { card_id: 10 }),
        agent(2, { card_id: 10, card_review: true }),
        agent(3, { card_id: 11 }),
        agent(4, { card_id: 12 }),
      ],
      [card(10, 1, "in_review"), card(11, 1, "input_required"), card(12, 1, "working")],
      plans,
    );
    expect(groups[0].tiles.map((t) => t.agent.id)).toEqual([3, 4, 1, 2]);
  });
});

describe("what a tile says it is doing", () => {
  const edit = { kind: "tool" as const, name: "Edit", detail: "/work/tree/src/deep/a.rs", at: null };
  it("shortens a file path and leaves a command whole", () => {
    expect(shortPath("a.rs")).toBe("a.rs");
    expect(shortPath("/work/tree/src/deep/a.rs")).toBe("…/deep/a.rs");
    expect(stepLabel(edit)).toBe("Edit …/deep/a.rs");
    expect(stepLabel({ kind: "tool", name: "Bash", detail: "cargo test -p x", at: null })).toBe("Bash cargo test -p x");
    expect(stepLabel({ kind: "tool", name: "report_done", detail: "", at: null })).toBe("report_done");
    expect(stepLabel({ kind: "say", name: "", detail: "Looking at it.", at: null })).toBe("Looking at it.");
  });

  it("says why there is no live line instead of leaving it blank", () => {
    expect(nowLine(undefined)).toBe("");
    expect(nowLine({ readable: false, steps: [] })).toBe("nothing recorded yet");
    expect(nowLine({ readable: true, steps: [] })).toBe("no step yet");
    expect(nowLine({ readable: true, steps: [edit] })).toBe("Edit …/deep/a.rs");
  });

  it("gives the time of the newest step, if there is one", () => {
    expect(nowAt(undefined)).toBeNull();
    expect(nowAt({ steps: [] })).toBeNull();
    expect(nowAt({ steps: [edit, { ...edit, at: "2026-10-06T10:00:00Z" }] })).toBe("2026-10-06T10:00:00Z");
  });
});
