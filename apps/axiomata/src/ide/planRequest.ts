/**
 * Which plan the next showing of the Flow's planning panel asks for (`docs/plans/card-as-plan.md`): a card started as a
 * plan opens the Studio on that plan, where the owner decides about grilling and starts the planner. Like
 * `modeRequest.ts`, the panel may not be mounted yet, so the request waits here and the panel takes it once it is.
 */

import { writable } from "svelte/store";

import { emit } from "../core/bus";
import { requestMode } from "./modeRequest";
import { requestProject } from "./projectRequest";

export interface PlanRequest {
  boardId: number;
  planId: number;
}

export const planRequest = writable<PlanRequest | null>(null);

export function requestPlan(request: PlanRequest): void {
  planRequest.set(request);
}

/** Opens the Studio in the Flow mode of `projectId`, on the plan (`request`). */
export function showPlanInFlow(projectId: number, request: PlanRequest): void {
  requestPlan(request);
  requestProject(projectId);
  requestMode("flow");
  emit("shell:studio");
}
