/**
 * What the Flow's planning panel decides before it asks the backend (A2A CP-A7b): which plan to show first, which cards
 * of it are proposals, who plans it, which roles a card may be given. Pure, so it is tested without a backend.
 */
import type { BoardCard, BoardPlan, IdeAgent, PlanRunEvent, PlanStatus } from "../core/backend";
import type { Role } from "../core/roster";
import { addTab, allGroups, allTabs, type Layout, type PaneTab } from "./layout";
import type { Mode } from "./modes";
import { PLAN_PANE, planTab } from "./paneKinds";

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
  }
}

/** The sessions whose panes are to be closed after an event: a card that was integrated, or put back, has none left. */
export function endedSessions(event: PlanRunEvent): number[] {
  return event.event === "integrated" && event.outcome !== "busy" ? event.agent_ids : [];
}

/** A plan runs by itself once it is approved, set to, and has a project to run in (`card_session::runs_by_itself`). */
export function runsByItself(plan: Pick<BoardPlan, "status" | "auto_start_max" | "project_id">): boolean {
  return plan.status === "approved" && plan.auto_start_max != null && plan.project_id != null;
}
