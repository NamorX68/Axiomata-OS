/**
 * What the Flow is about (A2A CP-A9/A10): the board and plan chosen in the planning panel's list on the left. The team's
 * tiles and the graph follow it and have no selector of their own; only the planning panel writes it, from its own state,
 * so there is one place that decides.
 */
import { writable } from "svelte/store";

import type { BoardPlan } from "../core/backend";
import { defaultPlanId, newestFirst } from "./planning";

export interface FlowSelection {
  boardId: number | null;
  /** The plan shown, already resolved (the planning panel's choice, or its default) — `null` while there is none. */
  planId: number | null;
}

export const flowSelection = writable<FlowSelection>({ boardId: null, planId: null });

/** The plan the Flow shows: the picked one while it exists, else the default (the newest draft, else the newest). */
export function resolvePlan(plans: BoardPlan[], picked: number | null): BoardPlan | null {
  const sorted = newestFirst(plans);
  const id = picked !== null && sorted.some((p) => p.id === picked) ? picked : defaultPlanId(sorted);
  return sorted.find((p) => p.id === id) ?? null;
}
