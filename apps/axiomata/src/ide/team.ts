/**
 * The Flow's team pane as plain data (A2A CP-A9): which sessions there are, what each is on, and what it is doing right
 * now. The component only draws this. A tile per session, grouped by the plan its card belongs to — so the owner sees who
 * works on what, and what each of them did last, without opening a terminal.
 */
import type { BoardCard, BoardPlan, IdeAgent, SessionActivityStep, TaskState } from "../core/backend";

/** What a session is for, as the tile says it. */
export type Duty = "worker" | "reviewer" | "planner" | "own";

export interface Tile {
  agent: IdeAgent;
  duty: Duty;
  /** The card the session works on or reviews; `null` for a planner and for a session made by hand. */
  card: BoardCard | null;
}

export interface TeamGroup {
  /** The plan's id, or `null` for sessions of no plan. */
  planId: number | null;
  title: string;
  tiles: Tile[];
}

/** What the studio started a session for. A session of the owner's own making has none of it. */
export function dutyOf(agent: IdeAgent): Duty {
  if (agent.plan_id != null) return "planner";
  if (agent.card_id != null) return agent.card_review ? "reviewer" : "worker";
  return "own";
}

/** Busy sessions first, a reviewer after the worker whose card it judges: the order a human reads a team in. */
const ORDER: Record<TaskState, number> = {
  input_required: 0,
  working: 1,
  in_review: 2,
  blocked: 3,
  ready: 4,
  verified: 5,
  done: 6,
  integrated: 7,
  taken_over: 8,
  failed: 9,
  canceled: 10,
  proposed: 11,
};

/**
 * The sessions of a project as groups: one per plan that has a session, and one for the rest. Sessions made by hand (no
 * card, no plan) are the Canvas's business and stay out — the Flow shows what the studio runs.
 */
export function groupsOf(agents: IdeAgent[], cards: BoardCard[], plans: BoardPlan[]): TeamGroup[] {
  const cardById = new Map(cards.map((card) => [card.id, card]));
  const planName = new Map(plans.map((plan) => [plan.id, plan.name]));
  const groups = new Map<number | null, Tile[]>();
  for (const agent of agents) {
    const duty = dutyOf(agent);
    if (duty === "own") continue;
    const card = agent.card_id != null ? (cardById.get(agent.card_id) ?? null) : null;
    const planId = agent.plan_id ?? card?.plan_id ?? null;
    const tiles = groups.get(planId) ?? [];
    tiles.push({ agent, duty, card });
    groups.set(planId, tiles);
  }
  const rank = (tile: Tile): number => (tile.card ? ORDER[tile.card.state] : -1);
  const result: TeamGroup[] = [...groups.entries()].map(([planId, tiles]) => ({
    planId,
    title: planId === null ? "Ohne Plan" : `Plan #${planId} · ${planName.get(planId) ?? "?"}`,
    tiles: tiles.sort(
      (a, b) =>
        rank(a) - rank(b) ||
        (a.card?.id ?? 0) - (b.card?.id ?? 0) ||
        Number(a.duty === "reviewer") - Number(b.duty === "reviewer") ||
        a.agent.id - b.agent.id,
    ),
  }));
  // Plans in id order, the sessions of no plan last.
  return result.sort((a, b) => (a.planId ?? Infinity) - (b.planId ?? Infinity));
}

/** The verb a tile starts its card line with. */
export function dutyLabel(duty: Duty): string {
  switch (duty) {
    case "worker":
      return "arbeitet an";
    case "reviewer":
      return "prüft";
    case "planner":
      return "plant";
    case "own":
      return "";
  }
}

/** A path shortened to its last two parts: a tile is narrow, and the worktree's prefix says nothing. */
export function shortPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts.length <= 2 ? path : `…/${parts.slice(-2).join("/")}`;
}

/** The tools whose target is a file, for [`stepLabel`]. */
const FILE_TOOLS = new Set(["Edit", "Write", "Read", "MultiEdit", "NotebookEdit", "edit", "write", "read"]);

/** One step as a line: `Edit …/src/a.rs`, `Bash cargo test`, or what the session said. */
export function stepLabel(step: SessionActivityStep): string {
  if (step.kind === "say") return step.detail;
  const detail = FILE_TOOLS.has(step.name) ? shortPath(step.detail) : step.detail;
  return detail ? `${step.name} ${detail}` : step.name;
}

/** What the tile's live line says: the newest step, or why there is none. */
export function nowLine(activity: { readable: boolean; steps: SessionActivityStep[] } | undefined): string {
  if (!activity) return "";
  if (!activity.readable) return "noch nichts aufgezeichnet";
  const last = activity.steps[activity.steps.length - 1];
  return last ? stepLabel(last) : "noch kein Schritt";
}

/** When the newest step happened, as the harness wrote it; `null` where there is no step or the record has no time. */
export function nowAt(activity: { steps: SessionActivityStep[] } | undefined): string | null {
  if (!activity || activity.steps.length === 0) return null;
  return activity.steps[activity.steps.length - 1].at;
}
