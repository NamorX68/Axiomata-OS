/**
 * What the Flow's planning panel decides before it asks the backend (A2A CP-A7b): which plan to show first, which cards
 * of it are proposals, who plans it, which roles a card may be given. Pure, so it is tested without a backend.
 */
import type {
  AgentState,
  BoardCard,
  BoardColumn,
  BoardPlan,
  CardTier,
  IdeAgent,
  PlanRunEvent,
  PlanSpend,
  PlanStatus,
} from "../core/backend";
import type { Role } from "../core/roster";
import {
  addTab,
  allGroups,
  allTabs,
  forceCloseTab,
  isGroup,
  isSplit,
  mapTabs,
  setSplitSizes,
  type DockTarget,
  type Layout,
  type LayoutNode,
  type PaneTab,
  type Split,
} from "./layout";
import type { Mode } from "./modes";
import { GRAPH_PANE, PLAN_PANE, TEAM_PANE, graphTab, planTab, teamTab } from "./paneKinds";

/** The role kinds that do not take cards: a reviewer judges them, a planner makes them. */
const NOT_ASSIGNABLE = new Set(["review", "plan", "grill"]);

const STATUS_LABEL: Record<PlanStatus, string> = { draft: "draft", approved: "released", closed: "closed" };

export function planStatusLabel(status: PlanStatus): string {
  return STATUS_LABEL[status];
}

/** The plans newest first: the one the owner made last is the one they want to see. */
export function newestFirst(plans: BoardPlan[]): BoardPlan[] {
  return [...plans].sort((a, b) => b.id - a.id);
}

/** The plan shown first: the newest draft (that is where the work is), else the newest of any state, else none. */
export function defaultPlanId(plans: BoardPlan[]): number | null {
  const sorted = newestFirst(plans);
  return (sorted.find((plan) => plan.status === "draft") ?? sorted[0])?.id ?? null;
}

/** The cards of a plan that are still on the board, in board order. */
export function cardsOfPlan(cards: BoardCard[], planId: number): BoardCard[] {
  return cards.filter((card) => card.plan_id === planId && card.archived_at === null);
}

/**
 * Whether a plan may be deleted: a draft or a closed plan always; an approved one only while nothing of it is under way —
 * no card held by a session and no integration line (`board::flow::delete_plan` refuses the same, so the button is not a
 * promise the backend breaks). A running plan is taken over or closed first.
 */
export function canDeletePlan(
  plan: Pick<BoardPlan, "status" | "base_branch">,
  cards: Pick<BoardCard, "claimed_by" | "archived_at" | "state">[],
): boolean {
  if (plan.status !== "approved") return true;
  const underWay = cards.some(
    (card) =>
      card.archived_at === null &&
      (card.claimed_by != null || card.state === "working" || card.state === "in_review" || card.state === "input_required"),
  );
  return !underWay && !plan.base_branch;
}

/** Every card of a plan, the archived ones (taken over) included: what a finished plan did is still its own. */
export function allCardsOfPlan(cards: BoardCard[], planId: number): BoardCard[] {
  return cards.filter((card) => card.plan_id === planId).sort((a, b) => a.id - b.id);
}

/** The cards of a plan that wait for the owner's yes. */
export function proposalsOf(cards: BoardCard[], planId: number): BoardCard[] {
  return cardsOfPlan(cards, planId).filter((card) => card.state === "proposed");
}

/** Whether the session plays a role of kind `grill`; the role is told by name, and a role this list lacks is a planner. */
function isGriller(agent: IdeAgent, roles: Pick<Role, "name" | "kind">[]): boolean {
  return roles.find((role) => role.name === agent.agent_role)?.kind === "grill";
}

/** The session the studio started to plan `planId` — one that cuts the goal into cards — if there is one. */
export function plannerOf(agents: IdeAgent[], planId: number, roles: Pick<Role, "name" | "kind">[] = []): IdeAgent | null {
  return agents.find((agent) => agent.plan_id === planId && !isGriller(agent, roles)) ?? null;
}

/** The session that grills `planId`'s goal — interviews the owner about it — if there is one. */
export function grillerOf(agents: IdeAgent[], planId: number, roles: Pick<Role, "name" | "kind">[] = []): IdeAgent | null {
  return agents.find((agent) => agent.plan_id === planId && isGriller(agent, roles)) ?? null;
}

/** A planner can be started for a draft that has none. */
export function canStartPlanner(
  plan: Pick<BoardPlan, "status" | "id">,
  agents: IdeAgent[],
  roles: Pick<Role, "name" | "kind">[] = [],
): boolean {
  return plan.status === "draft" && plannerOf(agents, plan.id, roles) === null;
}

/** A grilling session can be started for a draft that has none. */
export function canStartGrill(
  plan: Pick<BoardPlan, "status" | "id">,
  agents: IdeAgent[],
  roles: Pick<Role, "name" | "kind">[] = [],
): boolean {
  return plan.status === "draft" && grillerOf(agents, plan.id, roles) === null;
}

/**
 * A plan can be approved when it is a draft with a card to say yes to — a proposal still waiting, or cards the owner
 * accepted one by one (then approving is what ends the planner and closes the draft).
 */
export function canApprove(plan: Pick<BoardPlan, "status">, cards: BoardCard[]): boolean {
  return plan.status === "draft" && cards.length > 0;
}

/** The roles a proposal may be given: those that do work. The current one stays offered even if it no longer does. */
export function assignableRoles(roles: Role[], current: string | null): string[] {
  const names = roles.filter((role) => !NOT_ASSIGNABLE.has(role.kind)).map((role) => role.name);
  return current && !names.includes(current) ? [current, ...names] : names;
}

/** What a session started from a role runs on: the role's own engine, or — a role without one — the engine to ask for. */
export interface SessionEngine {
  /** The engine the session runs on; empty while there is none to pick from. */
  engineId: string;
  /** The role names none: the owner picks one, and it is saved into the role so the next start is one click. */
  ask: boolean;
}

/**
 * The engine for a session of `role`. The role carries its engine (a catalog entry that still exists); a role without one,
 * or whose engine was removed from the catalog, asks — with `picked` (else the first of the catalog) preselected.
 */
export function sessionEngine(
  role: Pick<Role, "engine"> | null,
  catalog: { id: string }[],
  picked: string,
): SessionEngine {
  if (role?.engine && catalog.some((engine) => engine.id === role.engine)) {
    return { engineId: role.engine, ask: false };
  }
  const chosen = catalog.some((engine) => engine.id === picked) ? picked : (catalog[0]?.id ?? "");
  return { engineId: chosen, ask: true };
}

/** The roles a start can offer: those of `kind`, the seeded one (`preferred`) first. */
export function rolesOfKind(roles: Pick<Role, "name" | "kind">[], kind: string, preferred: string): string[] {
  const names = roles.filter((role) => role.kind === kind).map((role) => role.name);
  return names.includes(preferred) ? [preferred, ...names.filter((name) => name !== preferred)] : names;
}

/** The ids of the cards a proposal waits for, as the proposal row shows them: `#12, #14`, or nothing. */
export function needsLabel(card: Pick<BoardCard, "depends_on">): string {
  return card.depends_on.map((id) => `#${id}`).join(", ");
}

/**
 * Whether the owner may still rework a card of a plan: it waits (a proposal, or ready or blocked in an open column), nobody
 * holds it and its work is not on the line. A card a session is working on, or has finished, is not changed from here.
 */
export function cardEditable(
  card: Pick<BoardCard, "state" | "claimed_by" | "integrated_at" | "archived_at">,
): boolean {
  return (
    (card.state === "proposed" || card.state === "ready" || card.state === "blocked") &&
    card.claimed_by == null &&
    card.integrated_at == null &&
    card.archived_at == null
  );
}

/**
 * The column a card the owner adds goes to: the proposal column while the plan is a draft (it is approved with the rest),
 * the first plain open column once the plan runs (the owner wrote it, there is nothing to say yes to). `null` for a closed
 * plan, or a board that lacks the column.
 */
export function newCardColumn(
  columns: Pick<BoardColumn, "id" | "position" | "maps_to_status" | "stage">[],
  plan: Pick<BoardPlan, "status">,
): number | null {
  if (plan.status === "closed") return null;
  if (plan.status === "draft") return columns.find((column) => column.stage === "proposal")?.id ?? null;
  const open = columns
    .filter((column) => column.maps_to_status === "open" && column.stage === null)
    .sort((a, b) => a.position - b.position);
  return open[0]?.id ?? null;
}

/** What the owner can change on a proposal: its text, its role and level, and which cards it waits for. */
export interface ProposalForm {
  title: string;
  body: string;
  acceptance: string;
  agent: string;
  tier: CardTier | "";
  needs: number[];
}

/** The form of an existing proposal, or an empty one for a card the owner adds. */
export function proposalForm(card?: BoardCard): ProposalForm {
  return {
    title: card?.title ?? "",
    body: card?.body ?? "",
    acceptance: card?.acceptance ?? "",
    agent: card?.agent ?? "",
    tier: card?.tier ?? "",
    needs: [...(card?.depends_on ?? [])].sort((a, b) => a - b),
  };
}

/** A form can be saved with a title: a card with none could not be told from the next one. */
export function proposalSavable(form: ProposalForm): boolean {
  return form.title.trim() !== "";
}

/** The "needs first" edges to add and to remove to get from `have` to `want`; sorted, so the order of the changes is the same every time. */
export function needsDiff(have: number[], want: number[]): { add: number[]; remove: number[] } {
  const haveSet = new Set(have);
  const wantSet = new Set(want);
  return {
    add: [...wantSet].filter((id) => !haveSet.has(id)).sort((a, b) => a - b),
    remove: [...haveSet].filter((id) => !wantSet.has(id)).sort((a, b) => a - b),
  };
}

/**
 * The cards a proposal may wait for: every other live card of its plan. A card that already waits for this one is left
 * out — the edge would close a cycle, which the board refuses; offering it only to refuse it is no help.
 */
export function needsCandidates(
  cards: Pick<BoardCard, "id" | "depends_on" | "archived_at">[],
  selfId: number | null,
): number[] {
  const live = cards.filter((card) => card.archived_at === null);
  const waitsForSelf = new Set<number>();
  if (selfId !== null) {
    // Everything that waits for this card, through any chain.
    let grew = true;
    while (grew) {
      grew = false;
      for (const card of live) {
        if (waitsForSelf.has(card.id)) continue;
        if (card.depends_on.some((id) => id === selfId || waitsForSelf.has(id))) {
          waitsForSelf.add(card.id);
          grew = true;
        }
      }
    }
  }
  return live
    .filter((card) => card.id !== selfId && !waitsForSelf.has(card.id))
    .map((card) => card.id)
    .sort((a, b) => a - b);
}

/** What the Studio calls a proposal whose card has no title worth reading (a planner may send an empty line). */
export function proposalTitle(card: Pick<BoardCard, "id" | "title">): string {
  const first = card.title.split("\n")[0].trim();
  return first === "" ? `Card #${card.id}` : first;
}

/**
 * The mode a session's pane belongs in: everything the studio made for the board — a planner or grill (`plan_id`), the
 * worker or reviewer of a card (`card_id`, with or without a plan) — in the Flow, beside the tile it already shows
 * there; only a session the owner made by hand stays on the Canvas. Tile and pane must answer the same way, or a card
 * sits in the Flow while its agent sits on the Canvas.
 */
export function modeForAgent(agent: Pick<IdeAgent, "plan_id" | "card_id">): Mode {
  return agent.plan_id != null || agent.card_id != null ? "flow" : "agents";
}

/** How many session panes sit side by side in the Flow before a further one joins the newest as a tab: more columns
 * are too narrow to read. */
const MAX_AGENT_COLUMNS = 3;

/**
 * Where a new session pane goes in the Flow: to the right of the team's tiles, then to the right of the newest
 * session pane
 * — and, once [`MAX_AGENT_COLUMNS`] stand side by side, as a tab in the newest. Never beside the graph, which is the
 * last group
 * and put the terminal under the plan's width. `null` outside the Flow (a layout without the team panel), where the
 * Canvas's
 * own rule holds.
 */
export function flowAgentTarget(layout: Layout): DockTarget | null {
  const groups = allGroups(layout);
  const team = groups.find((group) => group.tabs.some((tab) => tab.kind === TEAM_PANE));
  if (!team) return null;
  const withAgent = groups.filter((group) => group.tabs.some((tab) => tab.kind === "agent"));
  if (withAgent.length === 0) return { nodeId: team.id, side: "right" };
  const newest = withAgent[withAgent.length - 1];
  return withAgent.length >= MAX_AGENT_COLUMNS
    ? { nodeId: newest.id, side: "center" }
    : { nodeId: newest.id, side: "right" };
}

/**
 * Whether a planner is done with its turn and has something to be grilled: it waits for the next prompt (`idle`) and
 * the plan
 * holds at least one proposal. A planner that only asked a question has no card yet and is not done.
 */
export function plannerDone(state: AgentState | undefined, proposalCount: number): boolean {
  return state === "idle" && proposalCount > 0;
}

/** The agent panes of `layout` that show one of `agentIds`. */
export function agentTabsOf(layout: Layout, agentIds: number[]): PaneTab[] {
  const wanted = new Set(agentIds);
  return allTabs(layout).filter(
    (tab) => tab.kind === "agent" && typeof tab.config?.agentId === "number" && wanted.has(tab.config.agentId),
  );
}

/**
 * The layout with a planning panel in it. The Flow is where plans are made, and a panel that was closed has no other
 * way back (nothing else opens one), so every Flow layout is repaired — on load and whenever it is shown.
 */
export function withPlanPane(layout: Layout): Layout {
  if (allTabs(layout).some((tab) => tab.kind === PLAN_PANE)) return layout;
  const first = allGroups(layout)[0];
  return addTab(layout, planTab(), { nodeId: first ? first.id : layout.root.id, side: "center" });
}

/** The share of the Flow's height the graph gets under the team's tiles: the tiles and the sessions need more. */
const GRAPH_SHARE = 0.4;

/** The share of the Flow's width the session columns take together: this much for none, plus a step for each, up to a cap. */
const FLOW_AGENT_SHARE_BASE = 0.3;
const FLOW_AGENT_SHARE_STEP = 0.15;
const FLOW_AGENT_SHARE_MAX = 0.75;

/** The share of the Flow's width the planning panel gets: the sessions and the graph on the right need the room. */
const PLAN_SHARE = 0.3;

/** The split that has a group holding a tab of `kind` as a direct child. */
function splitHolding(node: LayoutNode, kind: string): Split | null {
  if (!isSplit(node)) return null;
  if (node.children.some((child) => isGroup(child) && child.tabs.some((tab) => tab.kind === kind))) return node;
  for (const child of node.children) {
    const found = splitHolding(child, kind);
    if (found) return found;
  }
  return null;
}

/** The panes the Flow is built from: pinned, so they cannot be closed — a pane with no way back is a Flow with a hole in it. */
const FLOW_PANES = [PLAN_PANE, TEAM_PANE, GRAPH_PANE];

/** Whether `node` holds a session pane anywhere below it. */
const holdsAgent = (node: LayoutNode): boolean =>
  isGroup(node) ? node.tabs.some((tab) => tab.kind === "agent") : node.children.some(holdsAgent);

/** The Flow's row of columns that the session panes stand in: the one whose children include the agents' columns. */
function agentRow(node: LayoutNode): Split | null {
  if (!isSplit(node)) return null;
  for (const child of node.children) {
    const found = agentRow(child);
    if (found) return found;
  }
  return node.dir === "row" && node.children.some(holdsAgent) ? node : null;
}

/**
 * The layout with the session columns of equal width. They stand in a row beside the team's tiles (above the graph);
 * what is no session keeps its proportions among itself and gives them room: 45 % of the row for one column, 60 % for
 * two, 75 % for three ([`FLOW_AGENT_SHARE_MAX`]). A layout without a session column stays as it is.
 */
export function balanceFlow(layout: Layout): Layout {
  const row = agentRow(layout.root);
  if (!row) return layout;
  const agents = row.children.filter(holdsAgent).length;
  const others = row.children.length - agents;
  if (others === 0) return setSplitSizes(layout, row.id, row.children.map(() => 1 / agents));
  const agentShare = Math.min(FLOW_AGENT_SHARE_MAX, FLOW_AGENT_SHARE_BASE + FLOW_AGENT_SHARE_STEP * agents);
  const otherTotal = row.children.reduce((sum, child, i) => (holdsAgent(child) ? sum : sum + row.sizes[i]), 0);
  const sizes = row.children.map((child, i) =>
    holdsAgent(child) ? agentShare / agents : ((1 - agentShare) * row.sizes[i]) / otherTotal,
  );
  return setSplitSizes(layout, row.id, sizes);
}

/**
 * The layout with the Flow's three panels in it, pinned: planning on the left, the team's tiles top right and the graph
 * below them. A missing one is added in that place — on load and whenever the Flow is shown — and one the owner moved
 * stays where it was put; they can be resized and moved, not closed (`resetFlowPanes` puts them back).
 */
export function withFlowPanes(layout: Layout): Layout {
  let next = withPlanPane(layout);
  const groupOf = (kind: string) => allGroups(next).find((group) => group.tabs.some((tab) => tab.kind === kind));
  if (!allTabs(next).some((tab) => tab.kind === TEAM_PANE)) {
    const plan = groupOf(PLAN_PANE);
    next = addTab(next, teamTab(), { nodeId: plan ? plan.id : next.root.id, side: "right" });
    const row = splitHolding(next.root, TEAM_PANE);
    if (row && row.children.length === 2) next = setSplitSizes(next, row.id, [PLAN_SHARE, 1 - PLAN_SHARE]);
  }
  if (!allTabs(next).some((tab) => tab.kind === GRAPH_PANE)) {
    const team = groupOf(TEAM_PANE);
    next = addTab(next, graphTab(), { nodeId: team ? team.id : next.root.id, side: "bottom" });
    const split = splitHolding(next.root, GRAPH_PANE);
    if (split && split.children.length === 2) next = setSplitSizes(next, split.id, [1 - GRAPH_SHARE, GRAPH_SHARE]);
  }
  return mapTabs(next, (tab) => (FLOW_PANES.includes(tab.kind) && !tab.pinned ? { ...tab, pinned: true } : tab));
}

/** The three panels back in their starting places, whatever the owner did with them; the sessions' panes stay as they are. */
export function resetFlowPanes(layout: Layout): Layout {
  const stripped = allTabs(layout)
    .filter((tab) => FLOW_PANES.includes(tab.kind))
    .reduce((acc, tab) => forceCloseTab(acc, tab.id), layout);
  return withFlowPanes(stripped);
}

/** What the owner is told about one thing the plans that run by themselves did — `null` for what needs no word. */
export function runEventNote(event: PlanRunEvent): { text: string; tone: "info" | "warning" } | null {
  switch (event.event) {
    case "started":
      return { text: `Plan #${event.plan_id}: card #${event.card_id} starts by itself.`, tone: "info" };
    case "integrated":
      if (event.outcome === "done") {
        return { text: `Card #${event.card_id} is integrated into plan #${event.plan_id}.`, tone: "info" };
      }
      if (event.outcome === "conflict") {
        const files = event.files.join(", ");
        return event.gave_up
          ? {
              text: `Card #${event.card_id} does not fit the plan the second time either (${files}); it stays as it is. You decide.`,
              tone: "warning",
            }
          : {
              text: `Card #${event.card_id} no longer fit the plan (${files}) and will be redone on the new state.`,
              tone: "warning",
            };
      }
      return null;
    case "blocked":
      return { text: `Card #${event.card_id} made no progress: ${event.reason}`, tone: "warning" };
    case "ready_to_take_over":
      return { text: `The plan “${event.name}” is finished: all cards are integrated. Ready to take over.`, tone: "info" };
    case "escalated":
      return {
        text: `Card #${event.card_id} was returned twice: “${event.role}” on “${event.engine_id}” takes over from “${event.from_role}” (same worktree, same branch). If that does not work either, you decide.`,
        tone: "info",
      };
    case "paused":
      return {
        text: `Plan “${event.name}” is paused: ${event.reason}. Nothing new starts until you say “Continue” in the Flow.`,
        tone: "warning",
      };
    case "day_cap_reached":
      return {
        text: `Daily limit of the Studio sessions reached (${event.reason}). Nothing new starts until tomorrow or until you raise it in the config.`,
        tone: "warning",
      };
  }
}

/** Tokens in the owner's reading size: `1.2 M`, `340 k`. */
export function tokensLabel(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)} M`;
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)} k`;
  return String(tokens);
}

/** One line for a plan's spending: tokens and — only when something was priced — dollars, each against its limit. */
export function spendLine(spend: Pick<PlanSpend, "spent" | "limits">): string {
  const tokens = `${tokensLabel(spend.spent.tokens)} of ${tokensLabel(spend.limits.max_tokens)} tokens`;
  // A subscription engine has no dollar figure: nothing is shown rather than "$0.00", which would read as "free".
  if (spend.spent.cost_usd <= 0) return tokens;
  return `${tokens} · $${spend.spent.cost_usd.toFixed(2)} of $${spend.limits.max_cost_usd.toFixed(2)}`;
}

/**
 * The sessions whose panes are to be closed after an event: a card that was integrated, or put back, has none left; an
 * escalated one is closed to be opened again — the open is what starts the new engine.
 */
export function endedSessions(event: PlanRunEvent): number[] {
  if (event.event === "escalated") return [event.agent_id];
  return event.event === "integrated" && event.outcome !== "busy" ? event.agent_ids : [];
}

/**
 * A plan that runs by itself is ready to be taken over when every card still on the board is on its line or was called
 * off, and at least one is on it (`card_session::take_over_plan` checks the same, and more).
 */
export function readyToTakeOver(
  plan: Pick<BoardPlan, "status" | "auto_start_max" | "project_id">,
  cards: Pick<BoardCard, "state" | "integrated_at" | "archived_at">[],
): boolean {
  if (!runsByItself(plan)) return false;
  const active = cards.filter((card) => !card.archived_at);
  return (
    active.length > 0 &&
    active.every((card) => card.integrated_at != null || card.state === "canceled") &&
    active.some((card) => card.integrated_at != null)
  );
}

/** A plan runs by itself once it is approved, set to, and has a project to run in (`card_session::runs_by_itself`). */
export function runsByItself(plan: Pick<BoardPlan, "status" | "auto_start_max" | "project_id">): boolean {
  return plan.status === "approved" && plan.auto_start_max != null && plan.project_id != null;
}
