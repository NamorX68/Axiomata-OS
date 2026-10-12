/**
 * A board card started as a plan (`docs/plans/card-as-plan.md`): the card is the plan's goal, the plan's own cards are
 * its steps. The board shows only the card (K2) — with where its plan stands and how many steps are done — and the
 * steps live in the Studio's Flow. Pure, so it is tested without a backend; the Rust side moves the card between the
 * columns.
 */

import type { BoardCard, BoardPlan } from "../core/backend";
import { readyToTakeOver } from "./planning";

/** Where the plan of a card stands, as the card says it. */
export type CardPlanPhase = "planning" | "running" | "waiting";

export const PHASE_LABEL: Record<CardPlanPhase, string> = {
  planning: "planning",
  running: "running",
  waiting: "waits for your take-over",
};

/** The open plans that were made from a card: their steps stay off the board. */
export function cardPlans(plans: BoardPlan[]): Map<number, BoardPlan> {
  return new Map(
    plans.filter((plan) => plan.origin_card_id != null && plan.status !== "closed").map((plan) => [plan.id, plan]),
  );
}

/** The cards the board shows: every card but the steps of an open plan made from a card. */
export function visibleOnBoard(cards: BoardCard[], plans: BoardPlan[]): BoardCard[] {
  const hidden = cardPlans(plans);
  return cards.filter((card) => card.plan_id === null || !hidden.has(card.plan_id));
}

/** The open plan `card` is the goal of, if there is one. */
export function planOfCard(card: Pick<BoardCard, "origin_of">, plans: BoardPlan[]): BoardPlan | null {
  if (card.origin_of == null) return null;
  return plans.find((plan) => plan.id === card.origin_of) ?? null;
}

/**
 * How far a plan's steps are: those on its line (or taken over) out of all that were not called off. Archived steps
 * count — a plan that was taken over archives its cards. The same count as the board's Markdown mirror.
 */
export function progressOf(cards: BoardCard[], planId: number): { done: number; total: number } {
  // `!= null`: a field a caller left out counts as unset, never as set.
  const steps = cards.filter((card) => card.plan_id === planId && card.canceled_at == null);
  const done = steps.filter((card) => card.integrated_at != null || card.taken_over_at != null).length;
  return { done, total: steps.length };
}

/** Where a card's plan stands: a draft is being planned; an approved one runs until every step is through. */
export function phaseOf(plan: BoardPlan, cards: BoardCard[]): CardPlanPhase {
  if (plan.status === "draft") return "planning";
  const steps = cards.filter((card) => card.plan_id === plan.id);
  return readyToTakeOver(plan, steps) ? "waiting" : "running";
}

/** Whether a session works on one of the plan's steps right now: then the card glows (the accent rule). */
export function stepAtWork(cards: BoardCard[], planId: number, atWork: ReadonlySet<number>): boolean {
  return cards.some((card) => card.plan_id === planId && atWork.has(card.id));
}

/** A card the owner may start as a plan: ready, and no step of a plan (that one starts in the Flow). */
export function canStartAsPlan(card: Pick<BoardCard, "state" | "plan_id">): boolean {
  return card.state === "ready" && card.plan_id === null;
}
