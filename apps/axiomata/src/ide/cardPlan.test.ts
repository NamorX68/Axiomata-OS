import { describe, expect, it } from "vitest";

import type { BoardCard, BoardPlan } from "../core/backend";
import {
  canStartAsPlan,
  cardPlans,
  phaseOf,
  planOfCard,
  progressOf,
  stepAtWork,
  waitingFor,
  visibleOnBoard,
} from "./cardPlan";

const plan = (id: number, over: Partial<BoardPlan> = {}): BoardPlan => ({
  id,
  board_id: 1,
  name: `P${id}`,
  goal: "",
  status: "approved",
  auto_start_max: 64,
  project_id: 3,
  max_cost_usd: null,
  max_tokens: null,
  created_at: "",
  updated_at: "",
  approved_at: null,
  origin_card_id: null,
  ...over,
});

const card = (id: number, planId: number | null, over: Partial<BoardCard> = {}): BoardCard =>
  ({
    id,
    plan_id: planId,
    archived_at: null,
    canceled_at: null,
    integrated_at: null,
    taken_over_at: null,
    origin_of: null,
    state: "ready",
    ...over,
  }) as BoardCard;

const ON_LINE = { integrated_at: "2026-10-12T10:00:00Z" };

describe("the steps of a card's plan", () => {
  const plans = [plan(1, { origin_card_id: 10 }), plan(2), plan(3, { origin_card_id: 11, status: "closed" })];
  const cards = [card(10, null, { origin_of: 1 }), card(20, 1), card(21, 2), card(22, 3), card(23, null)];

  it("stay off the board while the plan is open; other plans keep their cards there", () => {
    expect([...cardPlans(plans).keys()]).toEqual([1]);
    expect(visibleOnBoard(cards, plans).map((c) => c.id)).toEqual([10, 21, 22, 23]);
  });

  it("belong to the plan the card is the goal of", () => {
    expect(planOfCard(cards[0], plans)?.id).toBe(1);
    expect(planOfCard(cards[4], plans)).toBeNull();
  });

  it("count as done once on the line or taken over, and a called-off step does not count at all", () => {
    const steps = [
      card(1, 4, ON_LINE),
      card(2, 4, { taken_over_at: "2026-10-12T10:00:00Z", archived_at: "2026-10-12T10:00:00Z" }),
      card(3, 4),
      card(4, 4, { canceled_at: "2026-10-12T10:00:00Z", state: "canceled" }),
      card(5, 9, ON_LINE),
    ];
    expect(progressOf(steps, 4)).toEqual({ done: 2, total: 3 });
    expect(progressOf([], 4)).toEqual({ done: 0, total: 0 });
    // A step whose flags were left out is neither done nor called off.
    expect(progressOf([{ id: 6, plan_id: 4 } as BoardCard], 4)).toEqual({ done: 0, total: 1 });
  });

  it("make a session at work on one of them glow the card", () => {
    expect(stepAtWork(cards, 1, new Set([20]))).toBe(true);
    expect(stepAtWork(cards, 1, new Set([21]))).toBe(false);
  });

  it("make a session waiting on one of them, or on the card itself, show the card's waiting badge", () => {
    const waits = (id: number, cardId: number | null) => ({
      agent_id: id,
      project_id: 1,
      name: `a${id}`,
      card_id: cardId,
      card_title: null,
      since: null,
    });
    const waiting = [waits(1, 20), waits(2, 30), waits(3, null), waits(4, 99)];
    const steps = [card(20, 1), card(30, null)];
    expect(waitingFor({ id: 99, origin_of: 1 }, steps, waiting).map((a) => a.agent_id)).toEqual([1, 4]);
    expect(waitingFor({ id: 30, origin_of: null }, steps, waiting).map((a) => a.agent_id)).toEqual([2]);
    expect(waitingFor({ id: 50, origin_of: null }, steps, waiting)).toEqual([]);
  });
});

describe("where a card's plan stands", () => {
  it("is planning as a draft, running while a step is open, and waits once every step is on the line", () => {
    expect(phaseOf(plan(1, { status: "draft" }), [])).toBe("planning");
    expect(phaseOf(plan(1), [card(1, 1, ON_LINE), card(2, 1)])).toBe("running");
    expect(phaseOf(plan(1), [card(1, 1, ON_LINE), card(2, 1, { state: "canceled" })])).toBe("waiting");
    expect(phaseOf(plan(1), [])).toBe("running");
  });
});

describe("canStartAsPlan", () => {
  it("is for a ready card that is no step of a plan", () => {
    expect(canStartAsPlan({ state: "ready", plan_id: null })).toBe(true);
    expect(canStartAsPlan({ state: "ready", plan_id: 4 })).toBe(false);
    expect(canStartAsPlan({ state: "planned", plan_id: null })).toBe(false);
    expect(canStartAsPlan({ state: "blocked", plan_id: null })).toBe(false);
  });
});
