/**
 * What the Flow's planning panel decides before it asks the backend (A2A CP-A7b): which plan to show first, which cards
 * of it are proposals, who plans it, which roles a card may be given. Pure, so it is tested without a backend.
 */
import type { BoardCard, BoardPlan, IdeAgent, PlanRunEvent, PlanSpend, PlanStatus } from "../core/backend";
import type { Role } from "../core/roster";
import { addTab, allGroups, allTabs, type Layout, type PaneTab } from "./layout";
import type { Mode } from "./modes";
import { PLAN_PANE, TEAM_PANE, planTab, teamTab } from "./paneKinds";

/** The role kinds that do not take cards: a reviewer judges them, a planner makes them. */
const NOT_ASSIGNABLE = new Set(["review", "plan"]);

const STATUS_LABEL: Record<PlanStatus, string> = { draft: "Entwurf", approved: "freigegeben", closed: "abgeschlossen" };

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

/** Every card of a plan, the archived ones (taken over) included: what a finished plan did is still its own. */
export function allCardsOfPlan(cards: BoardCard[], planId: number): BoardCard[] {
  return cards.filter((card) => card.plan_id === planId).sort((a, b) => a.id - b.id);
}

/** The cards of a plan that wait for the owner's yes. */
export function proposalsOf(cards: BoardCard[], planId: number): BoardCard[] {
  return cardsOfPlan(cards, planId).filter((card) => card.state === "proposed");
}

/** The session the studio started to plan `planId`, if there is one. */
export function plannerOf(agents: IdeAgent[], planId: number): IdeAgent | null {
  return agents.find((agent) => agent.plan_id === planId) ?? null;
}

/** A planner can be started for a draft that has none. */
export function canStartPlanner(plan: Pick<BoardPlan, "status" | "id">, agents: IdeAgent[]): boolean {
  return plan.status === "draft" && plannerOf(agents, plan.id) === null;
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

/** The ids of the cards a proposal waits for, as the proposal row shows them: `#12, #14`, or nothing. */
export function needsLabel(card: Pick<BoardCard, "depends_on">): string {
  return card.depends_on.map((id) => `#${id}`).join(", ");
}

/** What the Studio calls a proposal whose card has no title worth reading (a planner may send an empty line). */
export function proposalTitle(card: Pick<BoardCard, "id" | "title">): string {
  const first = card.title.split("\n")[0].trim();
  return first === "" ? `Karte #${card.id}` : first;
}

/** The mode a session's pane belongs in: a planner in the Flow beside the plan panel, everything else on the Canvas. */
export function modeForAgent(agent: Pick<IdeAgent, "plan_id">): Mode {
  return agent.plan_id != null ? "flow" : "agents";
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

/**
 * The layout with the Flow's two panels in it: planning, and — to its right, so both are in view — the team's tiles. Both
 * are repaired on load and whenever the Flow is shown, since nothing else opens a closed one.
 */
export function withFlowPanes(layout: Layout): Layout {
  const planned = withPlanPane(layout);
  if (allTabs(planned).some((tab) => tab.kind === TEAM_PANE)) return planned;
  const plan = allGroups(planned).find((group) => group.tabs.some((tab) => tab.kind === PLAN_PANE));
  return addTab(planned, teamTab(), { nodeId: plan ? plan.id : planned.root.id, side: "right" });
}

/** What the owner is told about one thing the plans that run by themselves did — `null` for what needs no word. */
export function runEventNote(event: PlanRunEvent): { text: string; tone: "info" | "warning" } | null {
  switch (event.event) {
    case "started":
      return { text: `Plan #${event.plan_id}: Karte #${event.card_id} läuft von selbst an.`, tone: "info" };
    case "integrated":
      if (event.outcome === "done") {
        return { text: `Karte #${event.card_id} ist im Plan #${event.plan_id} integriert.`, tone: "info" };
      }
      if (event.outcome === "conflict") {
        const files = event.files.join(", ");
        return event.gave_up
          ? {
              text: `Karte #${event.card_id} passt auch beim zweiten Mal nicht in den Plan (${files}); sie bleibt, wie sie ist. Entscheide du.`,
              tone: "warning",
            }
          : {
              text: `Karte #${event.card_id} passte nicht mehr in den Plan (${files}) und wird auf dem neuen Stand noch einmal gemacht.`,
              tone: "warning",
            };
      }
      return null;
    case "blocked":
      return { text: `Karte #${event.card_id} kam nicht voran: ${event.reason}`, tone: "warning" };
    case "ready_to_take_over":
      return { text: `Der Plan „${event.name}“ ist fertig: alle Karten sind integriert. Bereit zum Übernehmen.`, tone: "info" };
    case "escalated":
      return {
        text: `Karte #${event.card_id} wurde zweimal zurückgegeben: „${event.role}“ auf „${event.engine_id}“ übernimmt von „${event.from_role}“ (gleicher Worktree, gleicher Zweig). Klappt auch das nicht, entscheidest du.`,
        tone: "info",
      };
    case "paused":
      return {
        text: `Plan „${event.name}“ pausiert: ${event.reason}. Es startet nichts Neues, bis du im Flow „Weiter“ sagst.`,
        tone: "warning",
      };
    case "day_cap_reached":
      return {
        text: `Tageslimit der Studio-Sitzungen erreicht (${event.reason}). Es startet nichts Neues bis morgen oder bis du es in der Config anhebst.`,
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
  const tokens = `${tokensLabel(spend.spent.tokens)} von ${tokensLabel(spend.limits.max_tokens)} Token`;
  // A subscription engine has no dollar figure: nothing is shown rather than "$0.00", which would read as "free".
  if (spend.spent.cost_usd <= 0) return tokens;
  return `${tokens} · $${spend.spent.cost_usd.toFixed(2)} von $${spend.limits.max_cost_usd.toFixed(2)}`;
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
