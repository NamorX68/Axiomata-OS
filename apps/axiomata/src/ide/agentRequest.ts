/**
 * Which agents the next opening of the Studio should show (A2A CP-A6a): starting a card makes a session, and the Studio
 * opens its pane — in the project the session belongs to, in the Agents mode. The Studio may not be mounted yet, so the
 * requests wait here, in order (two quick starts must not overwrite each other), and the view takes them once its first
 * project is in (the same shape as `modeRequest`).
 */

import { get, writable } from "svelte/store";

export interface AgentRequest {
  projectId: number;
  agentId: number;
}

export const agentRequests = writable<AgentRequest[]>([]);

export function requestAgent(request: AgentRequest): void {
  agentRequests.update((waiting) => [...waiting, request]);
}

/** Everything that waits, oldest first; the queue is empty afterwards. */
export function takeAgentRequests(): AgentRequest[] {
  const waiting = get(agentRequests);
  if (waiting.length > 0) agentRequests.set([]);
  return waiting;
}
