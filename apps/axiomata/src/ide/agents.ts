/**
 * Agent profiles: the frontend side of the CP4 commands, plus the two rules
 * about what a profile *means* that the view should not have to restate.
 *
 * A profile is not a running agent. Starting one is a PTY session belonging to
 * the pane that shows it (`panes/AgentPane.svelte`), and it does not outlive
 * the app — the deliberate price of owning the PTY engine instead of leaning
 * on tmux, recorded as question F7 in the plan.
 */

import {
  invokeBackend as invoke,
  type AgentSpec,
  type AgentStatus,
  type Harness,
  type IdeAgent,
  type ProvisionedAgent,
} from "../core/backend";

/** The harnesses, in the order a picker should offer them. */
export const HARNESSES: { id: Harness; label: string; hint: string }[] = [
  { id: "opencode", label: "Opencode", hint: "The harness skills and routines already run on" },
  { id: "claude_code", label: "Claude Code", hint: "Anthropic's CLI" },
  { id: "mini", label: "Mini (M7.4)", hint: "Axiomata's own loop — not built yet" },
];

// There is deliberately no copy of the harness-to-default-command table here.
// Rust sends `effective_command` with every agent (computed on read, like a
// project's `root_exists`), so a profile form shows exactly what will run
// instead of a second table that can drift from the one that starts it. An
// unsaved profile simply shows its placeholder until it has been saved.

/** The Agents panel's blank form: no choice made yet except the role, which is almost always the all-rounder. */
export function blankSpec(roles: readonly string[]): AgentSpec {
  return { name: "", engine_id: "", role: roles.includes("allrounder") ? "allrounder" : (roles[0] ?? "") };
}

/** The editable choices of an existing agent, for the same form. */
export function specOf(agent: IdeAgent): AgentSpec {
  return { name: agent.name, engine_id: agent.engine_id ?? "", role: agent.agent_role };
}

/** Whether the form can be saved: a name and an engine and a role chosen. */
export function specReady(spec: AgentSpec): boolean {
  return spec.name.trim() !== "" && spec.engine_id !== "" && spec.role !== "";
}

export function listAgents(projectId: number): Promise<IdeAgent[]> {
  return invoke<IdeAgent[]>("list_ide_agents", { projectId });
}

/** Makes an agent on an engine of the catalog. The engine's harness, model and environment become the agent's. */
export function createAgent(projectId: number, spec: AgentSpec): Promise<IdeAgent> {
  return invoke<IdeAgent>("create_ide_agent_on_engine", { projectId, spec });
}

/** Renames an agent and moves it to another engine and role. */
export function updateAgent(id: number, spec: AgentSpec): Promise<IdeAgent | null> {
  return invoke<IdeAgent | null>("update_ide_agent_on_engine", { id, spec });
}

export function deleteAgent(id: number): Promise<boolean> {
  return invoke<boolean>("delete_ide_agent", { id });
}

/**
 * Gives an agent its worktree and port, and says where it runs.
 *
 * Called before every start, not only on creation: it is idempotent, and it is
 * what repairs an agent whose worktree was deleted by hand or that predates
 * worktrees existing.
 */
export function prepareAgent(id: number): Promise<ProvisionedAgent> {
  return invoke<ProvisionedAgent>("prepare_ide_agent", { id });
}

/**
 * Forgets an Opencode agent's session, so its next start opens a fresh one
 * ("New session", opencode2.md Q9). The old session stays in Opencode's list.
 */
export function newAgentSession(id: number): Promise<boolean> {
  return invoke<boolean>("ide_agent_new_session", { id });
}

/**
 * The "you have mail" line to type into this agent's terminal, or `null` when there is nothing to announce
 * (A2A A8, way 2). `state` is the status the pane shows; the backend only answers for an idle agent.
 */
export function mailboxNudge(id: number, state: string): Promise<string | null> {
  return invoke<string | null>("ide_mailbox_nudge", { id, agentState: state });
}

/** Notes that the nudge was typed, so a message is announced a limited number of times. */
export function mailboxNudged(id: number): Promise<void> {
  return invoke<void>("ide_mailbox_nudged", { id });
}

/** Whether removing this agent's worktree would throw away uncommitted work. */
export function agentHasChanges(id: number): Promise<boolean> {
  return invoke<boolean>("ide_agent_has_changes", { id });
}

/** Removes an agent's worktree. `force` discards uncommitted work in it. */
export function discardWorktree(id: number, force: boolean): Promise<boolean> {
  return invoke<boolean>("discard_ide_agent_worktree", { id, force });
}

/** What every agent of a project is doing and planning — one call per tick. */
export function agentStates(projectId: number): Promise<AgentStatus[]> {
  return invoke<AgentStatus[]>("ide_agent_states", { projectId });
}
