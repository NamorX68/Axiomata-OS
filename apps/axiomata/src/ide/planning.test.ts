import { describe, expect, it } from "vitest";

import type { BoardCard, BoardColumn, BoardPlan, IdeAgent, PlanRunEvent } from "../core/backend";
import type { Role } from "../core/roster";
import { addTab, allGroups, allTabs, closeTab, forceCloseTab, emptyLayout, singleGroupLayout, type PaneTab } from "./layout";
import {
  agentTabsOf,
  canDeletePlan,
  cardEditable,
  newCardColumn,
  canStartGrill,
  grillerOf,
  needsCandidates,
  needsDiff,
  proposalForm,
  proposalSavable,
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
  allCardsOfPlan,
  readyToTakeOver,
  spendLine,
  tokensLabel,
  runsByItself,
  resetFlowPanes,
  withFlowPanes,
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

describe("the Flow's three panels", () => {
  it("builds planning | team over graph, pinned, and repairs only what is missing", () => {
    const all = withFlowPanes(emptyLayout());
    expect(allTabs(all).map((t) => t.kind).sort()).toEqual(["graph", "plan", "team"]);
    expect(allTabs(all).every((t) => t.pinned)).toBe(true);
    // Three groups: planning, the team's tiles, the graph.
    expect(allGroups(all)).toHaveLength(3);
    expect(withFlowPanes(all)).toBe(all);
  });

  it("cannot be closed, but can be forced shut, and comes back where it started after a reset", () => {
    const all = withFlowPanes(emptyLayout());
    const team = allTabs(all).find((t) => t.kind === "team")!;
    expect(closeTab(all, team.id)).toBe(all);
    const reset = resetFlowPanes(forceCloseTab(all, team.id));
    expect(allTabs(reset).map((t) => t.kind).sort()).toEqual(["graph", "plan", "team"]);
    expect(allGroups(reset)).toHaveLength(3);
  });

  it("pins panels an older layout had and leaves them where the owner put them", () => {
    const together = singleGroupLayout([
      { id: "p", kind: "plan", title: "Planung" },
      { id: "g", kind: "graph", title: "Flowansicht" },
      { id: "t", kind: "team", title: "Agents" },
    ]);
    const pinned = withFlowPanes(together);
    expect(allGroups(pinned)).toHaveLength(1);
    expect(allTabs(pinned).every((t) => t.pinned)).toBe(true);
  });

  it("keeps the sessions' panes through a reset", () => {
    const base = withFlowPanes(emptyLayout());
    const first = allGroups(base)[0];
    const layout = addTab(base, agentTab("a", 5), { nodeId: first.id, side: "center" });
    expect(allTabs(resetFlowPanes(layout)).map((t) => t.kind).sort()).toEqual(["agent", "graph", "plan", "team"]);
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

describe("the cards of a finished plan", () => {
  it("include the archived ones a take-over closed, in the order they were made", () => {
    const card = (id: number, plan: number | null, archived: string | null) =>
      ({ id, plan_id: plan, archived_at: archived }) as Parameters<typeof allCardsOfPlan>[0][number];
    const cards = [card(63, 2, "2026-10-05T20:30:00Z"), card(7, 1, null), card(62, 2, "2026-10-05T20:30:00Z")];
    expect(allCardsOfPlan(cards, 2).map((c) => c.id)).toEqual([62, 63]);
    expect(allCardsOfPlan(cards, 3)).toEqual([]);
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

  it("tells an escalation, and closes the pane of the session that is handed over", () => {
    const escalated: PlanRunEvent = {
      event: "escalated",
      card_id: 7,
      plan_id: 1,
      project_id: 2,
      agent_id: 31,
      from_role: "builder",
      role: "builder-heavy",
      engine_id: "opus",
    };
    const note = runEventNote(escalated)!;
    expect(note.text).toContain("#7");
    expect(note.text).toContain("builder-heavy");
    expect(note.text).toContain("entscheidest du");
    expect(endedSessions(escalated)).toEqual([31]);
  });

  it("tells a paused plan and the day's cap apart, both as a warning", () => {
    const paused = runEventNote({ event: "paused", plan_id: 1, name: "Docs", reason: "6000000 tokens of 6000000 used" })!;
    expect(paused.tone).toBe("warning");
    expect(paused.text).toContain("Docs");
    expect(paused.text).toContain("Weiter");
    const day = runEventNote({ event: "day_cap_reached", reason: "$20.00 of the day's $20.00" })!;
    expect(day.tone).toBe("warning");
    expect(day.text).toContain("Tageslimit");
  });

  it("writes tokens in a reading size and shows dollars only when something was priced", () => {
    expect(tokensLabel(950)).toBe("950");
    expect(tokensLabel(340_400)).toBe("340 k");
    expect(tokensLabel(1_250_000)).toBe("1.3 M");
    const limits = { max_cost_usd: 15, max_tokens: 6_000_000 };
    expect(spendLine({ spent: { tokens: 1_500_000, steps: 3, cost_usd: 0 }, limits })).toBe("1.5 M von 6.0 M Token");
    expect(spendLine({ spent: { tokens: 1_500_000, steps: 3, cost_usd: 2.5 }, limits })).toBe(
      "1.5 M von 6.0 M Token · $2.50 von $15.00",
    );
  });

  it("closes the panes of the sessions that are gone, after an integration or a put-back", () => {
    expect(endedSessions(done)).toEqual([23, 24]);
    expect(endedSessions(conflict(false))).toEqual([25]);
    expect(endedSessions({ event: "integrated", outcome: "busy", card_id: 1 })).toEqual([]);
    expect(endedSessions({ event: "blocked", card_id: 5, reason: "x" })).toEqual([]);
  });
});

describe("deleting a plan", () => {
  const card = (state: string, claimed: string | null = null, archived: string | null = null) =>
    ({ state, claimed_by: claimed, archived_at: archived }) as Pick<BoardCard, "state" | "claimed_by" | "archived_at">;

  it("is always possible for a draft or a closed plan", () => {
    expect(canDeletePlan({ status: "draft", base_branch: null }, [card("proposed")])).toBe(true);
    expect(canDeletePlan({ status: "closed", base_branch: "main" }, [card("integrated", "agent:x-1")])).toBe(true);
  });

  it("is refused for an approved plan while a card is held or in work, and once it has a line", () => {
    const approved = { status: "approved" as const, base_branch: null };
    expect(canDeletePlan(approved, [card("ready")])).toBe(true);
    expect(canDeletePlan(approved, [card("working", "agent:x-1")])).toBe(false);
    expect(canDeletePlan(approved, [card("in_review")])).toBe(false);
    expect(canDeletePlan(approved, [card("ready", "agent:x-1")])).toBe(false);
    expect(canDeletePlan(approved, [card("working", "agent:x-1", "2026-10-06T10:00:00Z")])).toBe(true);
    expect(canDeletePlan({ ...approved, base_branch: "main" }, [card("ready")])).toBe(false);
  });
});

describe("editing a proposal", () => {
  const c = (id: number, depends_on: number[] = [], archived: string | null = null) =>
    ({ id, depends_on, archived_at: archived }) as Pick<BoardCard, "id" | "depends_on" | "archived_at">;

  it("starts from the card's own values, or empty for a card the owner adds", () => {
    expect(proposalForm()).toEqual({ title: "", body: "", acceptance: "", agent: "", tier: "", needs: [] });
    const card = { title: "T", body: "B", acceptance: "A", agent: "builder", tier: "heavy", depends_on: [9, 3] } as BoardCard;
    expect(proposalForm(card)).toEqual({ title: "T", body: "B", acceptance: "A", agent: "builder", tier: "heavy", needs: [3, 9] });
    expect(proposalSavable(proposalForm())).toBe(false);
    expect(proposalSavable({ ...proposalForm(), title: "  x " })).toBe(true);
  });

  it("works out which edges to add and which to remove", () => {
    expect(needsDiff([1, 2], [2, 3])).toEqual({ add: [3], remove: [1] });
    expect(needsDiff([], [5, 4, 5])).toEqual({ add: [4, 5], remove: [] });
    expect(needsDiff([1], [1])).toEqual({ add: [], remove: [] });
  });

  it("offers every other live card, but not one that already waits for this one, however indirectly", () => {
    // 2 needs 1; 3 needs 2: for card 1, neither 2 nor 3 may be needed (cycle); 4 is free; 5 is archived.
    const cards = [c(1), c(2, [1]), c(3, [2]), c(4), c(5, [], "2026-10-06T10:00:00Z")];
    expect(needsCandidates(cards, 1)).toEqual([4]);
    expect(needsCandidates(cards, 3)).toEqual([1, 2, 4]);
    expect(needsCandidates(cards, null)).toEqual([1, 2, 3, 4]);
  });
});

describe("the sessions of a plan", () => {
  const roles = [
    { name: "planner", kind: "plan" },
    { name: "grill", kind: "grill" },
  ];
  const session = (id: number, role: string, plan: number | null) =>
    ({ id, agent_role: role, plan_id: plan, name: `${role}-${id}` }) as IdeAgent;

  it("tells the planner from the grilling session by the kind of their roles", () => {
    const agents = [session(1, "grill", 5), session(2, "planner", 5), session(3, "planner", 6)];
    expect(plannerOf(agents, 5, roles)?.id).toBe(2);
    expect(grillerOf(agents, 5, roles)?.id).toBe(1);
    expect(grillerOf(agents, 6, roles)).toBeNull();
    expect(plannerOf([session(1, "grill", 5)], 5, roles)).toBeNull();
  });

  it("lets each be started on a draft until that session exists", () => {
    const draft = { status: "draft" as const, id: 5 };
    expect(canStartPlanner(draft, [session(1, "grill", 5)], roles)).toBe(true);
    expect(canStartGrill(draft, [session(1, "grill", 5)], roles)).toBe(false);
    expect(canStartGrill(draft, [session(2, "planner", 5)], roles)).toBe(true);
    expect(canStartGrill({ status: "approved", id: 5 }, [], roles)).toBe(false);
  });

  it("never offers the grilling role for a card", () => {
    expect(assignableRoles([{ name: "grill", kind: "grill" } as Role, { name: "b", kind: "implement" } as Role], null)).toEqual(["b"]);
  });
});

describe("changing a plan that is already running", () => {
  const card = (state: string, extra: Record<string, unknown> = {}) =>
    ({ state, claimed_by: null, integrated_at: null, archived_at: null, ...extra }) as Pick<
      BoardCard,
      "state" | "claimed_by" | "integrated_at" | "archived_at"
    >;

  it("lets the owner rework a card that still waits and no other", () => {
    for (const waiting of ["proposed", "ready", "blocked"]) expect(cardEditable(card(waiting))).toBe(true);
    for (const going of ["working", "input_required", "in_review", "verified", "integrated", "done", "failed"]) {
      expect(cardEditable(card(going))).toBe(false);
    }
    expect(cardEditable(card("ready", { claimed_by: "agent:w-1" }))).toBe(false);
    expect(cardEditable(card("ready", { integrated_at: "2026-10-06T10:00:00Z" }))).toBe(false);
    expect(cardEditable(card("ready", { archived_at: "2026-10-06T10:00:00Z" }))).toBe(false);
  });

  it("puts a new card into the proposal column of a draft and the first plain open column of a running plan", () => {
    const columns = [
      { id: 1, position: 0, maps_to_status: "open", stage: "proposal" },
      { id: 2, position: 2, maps_to_status: "open", stage: null },
      { id: 3, position: 1, maps_to_status: "open", stage: null },
      { id: 4, position: 3, maps_to_status: "doing", stage: null },
    ] as Pick<BoardColumn, "id" | "position" | "maps_to_status" | "stage">[];
    expect(newCardColumn(columns, { status: "draft" })).toBe(1);
    expect(newCardColumn(columns, { status: "approved" })).toBe(3);
    expect(newCardColumn(columns, { status: "closed" })).toBeNull();
    expect(newCardColumn([], { status: "approved" })).toBeNull();
  });
});
