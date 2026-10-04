/**
 * What an agent's card in the Agents panel says, as plain data: the status (with the closed-pane rule),
 * the step it is on, how far its task list is, and since when. The component only draws this.
 */

import type { AgentStatus, IdeAgent } from "../core/backend";
import { relativeTime } from "../core/format";

import { describeStatus, planTitle, withoutPane, type StatusView } from "./agentStatus";

export interface AgentCardData {
  status: StatusView;
  /** The step in progress, else the next one to do; `null` without a task list. */
  step: string | null;
  /** Steps done of all (cancelled ones do not count); `null` without a task list. */
  progress: { done: number; total: number } | null;
  /** The plan-mode plan's title, if the agent has written one. */
  plan: string | null;
  /** "3 min ago" — since the agent entered its state; `null` when unknown. */
  since: string | null;
}

export function cardOf(
  agent: IdeAgent,
  status: AgentStatus | undefined,
  hasPane: boolean,
  now: number,
): AgentCardData {
  const view = describeStatus(status, agent, now);
  const steps = (status?.plan?.steps ?? []).filter((s) => s.state !== "cancelled");
  const current = steps.find((s) => s.state === "doing") ?? steps.find((s) => s.state === "todo") ?? null;
  return {
    status: hasPane ? view : withoutPane(view),
    step: current?.text ?? null,
    progress: steps.length > 0 ? { done: steps.filter((s) => s.state === "done").length, total: steps.length } : null,
    plan: planTitle(status),
    since: status?.since ? relativeTime(status.since, now) : null,
  };
}
