import { describe, expect, it } from "vitest";

import type { BoardCard, BoardPlan, IdeAgent } from "../core/backend";
import type { Role } from "../core/roster";
import { addTab, allTabs, closeTab, emptyLayout, singleGroupLayout, type PaneTab } from "./layout";
import {
  agentTabsOf,
  assignableRoles,
  canApprove,
  canStartPlanner,
  cardsOfPlan,
  defaultPlanId,
  modeForAgent,
  needsLabel,
  newestFirst,
  plannerOf,
  planStatusLabel,
  proposalsOf,
  proposalTitle,
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
