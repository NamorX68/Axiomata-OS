import { describe, expect, it } from "vitest";

import type { BoardCard, BoardPlan, IdeAgent, PlanRunEvent } from "../core/backend";
import type { Role } from "../core/roster";
import { addTab, allTabs, closeTab, emptyLayout, singleGroupLayout, type PaneTab } from "./layout";
import {
  agentTabsOf,
  assignableRoles,
  canApprove,
  canStartPlanner,
  cardsOfPlan,
  defaultPlanId,
  endedSessions,
  modeForAgent,
  needsLabel,
  newestFirst,
  plannerOf,
  planStatusLabel,
  proposalsOf,
  proposalTitle,
  runEventNote,
  readyToTakeOver,
  runsByItself,
  withPlanPane,
} from "./planning";

const plan = (id: number, status: BoardPlan["status"] = "draft"): BoardPlan => ({
  id,
  board_id: 1,
  name: `P${id}`,
  goal: "",
  status,
  auto_start_max: null,
  max_cost_usd: null,
  max_tokens: null,
  created_at: "",
  updated_at: "",
  approved_at: null,
});
const card = (id: number, planId: number | null, over: Partial<BoardCard> = {}): BoardCard =>
  ({ id, plan_id: planId, archived_at: null, state: "proposed", title: `c${id}`, depends_on: [], ...over }) as BoardCard;
const agent = (id: number, planId: number | null): IdeAgent => ({ id, plan_id: planId }) as IdeAgent;
const role = (name: string, kind: string): Role => ({ name, kind }) as Role;

describe("plans", () => {
  it("shows the newest draft first, else the newest plan, else nothing", () => {
    expect(defaultPlanId([plan(1), plan(2, "approved"), plan(3)])).toBe(3);
    expect(defaultPlanId([plan(1, "approved"), plan(2, "closed")])).toBe(2);
    expect(defaultPlanId([])).toBeNull();
    expect(newestFirst([plan(1), plan(5), plan(3)]).map((p) => p.id)).toEqual([5, 3, 1]);
  });

  it("labels a status in the owner's words", () => {
    expect(planStatusLabel("draft")).toBe("Entwurf");
    expect(planStatusLabel("approved")).toBe("freigegeben");
  });
});

describe("a plan's cards", () => {
  const cards = [
    card(1, 7),
    card(2, 7, { state: "ready" }),
    card(3, 7, { archived_at: "2026-10-05T00:00:00Z" }),
    card(4, 8),
    card(5, null),
  ];

  it("takes the cards of the plan that are still on the board", () => {
    expect(cardsOfPlan(cards, 7).map((c) => c.id)).toEqual([1, 2]);
  });

  it("takes as proposals only those that wait for the owner's yes", () => {
    expect(proposalsOf(cards, 7).map((c) => c.id)).toEqual([1]);
  });

  it("names what a proposal waits for", () => {
    expect(needsLabel({ depends_on: [12, 14] })).toBe("#12, #14");
    expect(needsLabel({ depends_on: [] })).toBe("");
  });

  it("falls back to the number for a title with nothing to read", () => {
    expect(proposalTitle({ id: 9, title: "  \nbody" })).toBe("Karte #9");
    expect(proposalTitle({ id: 9, title: "Add tokens\nmore" })).toBe("Add tokens");
  });
});

describe("the planner of a plan", () => {
  it("finds the session started for the plan", () => {
    expect(plannerOf([agent(1, null), agent(2, 7)], 7)?.id).toBe(2);
    expect(plannerOf([agent(1, null)], 7)).toBeNull();
  });

  it("starts one for a draft that has none, and only then", () => {
    expect(canStartPlanner(plan(7), [])).toBe(true);
    expect(canStartPlanner(plan(7), [agent(2, 7)])).toBe(false);
    expect(canStartPlanner(plan(7, "approved"), [])).toBe(false);
  });

  it("approves a draft that has a card, whatever state the owner left it in", () => {
    expect(canApprove(plan(7), [card(1, 7)])).toBe(true);
    expect(canApprove(plan(7), [card(1, 7, { state: "ready" })])).toBe(true);
    expect(canApprove(plan(7), [])).toBe(false);
    expect(canApprove(plan(7, "approved"), [card(1, 7)])).toBe(false);
  });

  it("finds the first of several sessions on the same plan", () => {
    expect(plannerOf([agent(3, 7), agent(2, 7)], 7)?.id).toBe(3);
  });
});

describe("roles a card may be given", () => {
  const roles = [role("allrounder", "implement"), role("reviewer", "review"), role("planner", "plan"), role("tester", "test")];

  it("offers those that work, not the reviewer or the planner", () => {
    expect(assignableRoles(roles, null)).toEqual(["allrounder", "tester"]);
  });

  it("keeps the role a card already has offered, even one that no longer works", () => {
    expect(assignableRoles(roles, "reviewer")).toEqual(["reviewer", "allrounder", "tester"]);
    expect(assignableRoles(roles, "tester")).toEqual(["allrounder", "tester"]);
  });
});

const agentTab = (id: string, agentId: number): PaneTab => ({ id, kind: "agent", title: id, config: { agentId } });

describe("where a session's pane belongs", () => {
  it("opens a planner in the Flow and every other session on the Canvas", () => {
    expect(modeForAgent({ plan_id: 4 })).toBe("flow");
    expect(modeForAgent({ plan_id: null })).toBe("agents");
    expect(modeForAgent({})).toBe("agents");
  });

  it("finds the agent panes that show the sessions asked for, and no other tab", () => {
    const layout = singleGroupLayout([agentTab("a", 1), agentTab("b", 2), { id: "t", kind: "terminal", title: "T" }]);
    expect(agentTabsOf(layout, [2, 9]).map((t) => t.id)).toEqual(["b"]);
    expect(agentTabsOf(layout, [])).toEqual([]);
    // Closing what was found leaves the rest.
    const closed = agentTabsOf(layout, [1, 2]).reduce((acc, t) => closeTab(acc, t.id), layout);
    expect(allTabs(closed).map((t) => t.id)).toEqual(["t"]);
  });
});

describe("the Flow's planning panel", () => {
  it("is added to a Flow layout that lost it, and only then", () => {
    const lost = singleGroupLayout([agentTab("p", 5)]);
    const repaired = withPlanPane(lost);
    expect(allTabs(repaired).map((t) => t.kind).sort()).toEqual(["agent", "plan"]);
    expect(withPlanPane(repaired)).toBe(repaired);
  });

  it("comes back in a layout whose last tab was closed", () => {
    const closed = closeTab(singleGroupLayout([{ id: "x", kind: "plan", title: "Planung" }]), "x");
    expect(allTabs(closed)).toEqual([]);
    expect(allTabs(withPlanPane(closed)).map((t) => t.kind)).toEqual(["plan"]);
    expect(allTabs(withPlanPane(emptyLayout())).map((t) => t.kind)).toEqual(["plan"]);
  });

  it("is added beside the tabs that are there, in the first group", () => {
    const base = singleGroupLayout([{ id: "t", kind: "terminal", title: "T" }]);
    const split = addTab(base, agentTab("a", 1), { nodeId: (base.root as { id: string }).id, side: "right" });
    expect(allTabs(withPlanPane(split)).map((t) => t.kind).sort()).toEqual(["agent", "plan", "terminal"]);
  });
});

describe("a plan that runs by itself", () => {
  it("is an approved plan with a project that is set to start its cards", () => {
    const base = { status: "approved", auto_start_max: 64, project_id: 3 } as const;
    expect(runsByItself(base)).toBe(true);
    expect(runsByItself({ ...base, status: "draft" })).toBe(false);
    expect(runsByItself({ ...base, auto_start_max: null })).toBe(false);
    expect(runsByItself({ ...base, project_id: null })).toBe(false);
    expect(runsByItself({ status: "approved", auto_start_max: 1 })).toBe(false);
  });
});

describe("a plan that is ready to be taken over", () => {
  const plan = { status: "approved", auto_start_max: 64, project_id: 3 } as const;
  const on = { state: "integrated", integrated_at: "2026-10-05T20:05:00Z", archived_at: null } as const;
  const open = { state: "working", integrated_at: null, archived_at: null } as const;
  const off = { state: "canceled", integrated_at: null, archived_at: null } as const;

  it("has every card on the line, and a called-off card does not hold it back", () => {
    expect(readyToTakeOver(plan, [on, on])).toBe(true);
    expect(readyToTakeOver(plan, [on, off])).toBe(true);
    expect(readyToTakeOver(plan, [on, open])).toBe(false);
  });

  it("needs a card on the line, and a plan that runs by itself", () => {
    expect(readyToTakeOver(plan, [])).toBe(false);
    expect(readyToTakeOver(plan, [off])).toBe(false);
    expect(readyToTakeOver({ ...plan, status: "closed" }, [on])).toBe(false);
    expect(readyToTakeOver({ ...plan, auto_start_max: null }, [on])).toBe(false);
  });

  it("ignores archived cards, which an earlier take-over closed", () => {
    expect(readyToTakeOver(plan, [{ ...on, archived_at: "2026-10-05T21:00:00Z" }])).toBe(false);
    expect(readyToTakeOver(plan, [on, { ...open, archived_at: "2026-10-05T21:00:00Z" }])).toBe(true);
  });
});

describe("what the owner is told about a plan's run", () => {
  const done: PlanRunEvent = { event: "integrated", outcome: "done", card_id: 59, plan_id: 1, project_id: 1, commit: "abc", agent_ids: [23, 24] };
  const conflict = (gaveUp: boolean): PlanRunEvent => ({
    event: "integrated",
    outcome: "conflict",
    card_id: 60,
    plan_id: 1,
    files: ["a.txt", "b.txt"],
    gave_up: gaveUp,
    agent_ids: [25],
  });

  it("says a card started by itself and one that was integrated", () => {
    expect(runEventNote({ event: "started", card_id: 59, plan_id: 1, project_id: 1, agent_id: 23 })?.text).toContain("#59");
    expect(runEventNote(done)).toMatchObject({ tone: "info" });
    expect(runEventNote(done)?.text).toContain("integriert");
  });

  it("tells a card that is done again from one that is left for the owner", () => {
    const again = runEventNote(conflict(false))!;
    expect(again.text).toContain("noch einmal gemacht");
    expect(again.text).toContain("a.txt, b.txt");
    const gaveUp = runEventNote(conflict(true))!;
    expect(gaveUp.text).toContain("Entscheide du");
    expect(gaveUp.tone).toBe("warning");
  });

  it("says nothing about a card whose worker was merely busy, and names what blocked a card", () => {
    expect(runEventNote({ event: "integrated", outcome: "busy", card_id: 1 })).toBeNull();
    expect(runEventNote({ event: "blocked", card_id: 5, reason: "no engine" })?.text).toContain("no engine");
    expect(runEventNote({ event: "ready_to_take_over", plan_id: 1, name: "Docs" })?.text).toContain("Docs");
  });

  it("closes the panes of the sessions that are gone, after an integration or a put-back", () => {
    expect(endedSessions(done)).toEqual([23, 24]);
    expect(endedSessions(conflict(false))).toEqual([25]);
    expect(endedSessions({ event: "integrated", outcome: "busy", card_id: 1 })).toEqual([]);
    expect(endedSessions({ event: "blocked", card_id: 5, reason: "x" })).toEqual([]);
  });
});
