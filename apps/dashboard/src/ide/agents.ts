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
  type AgentFields,
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

/** A blank profile for the "new agent" form. */
export function blankFields(): AgentFields {
  return { name: "", harness: "opencode", command: "", model: null, env: "" };
}

/** The editable fields of an existing agent, for the same form. */
export function fieldsOf(agent: IdeAgent): AgentFields {
  return {
    name: agent.name,
    harness: agent.harness,
    command: agent.command,
    model: agent.model,
    env: agent.env,
  };
}

export function listAgents(projectId: number): Promise<IdeAgent[]> {
  return invoke<IdeAgent[]>("list_ide_agents", { projectId });
}

export function createAgent(projectId: number, fields: AgentFields): Promise<IdeAgent> {
  return invoke<IdeAgent>("create_ide_agent", { projectId, fields });
}

export function updateAgent(id: number, fields: AgentFields): Promise<IdeAgent | null> {
  return invoke<IdeAgent | null>("update_ide_agent", { id, fields });
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

/** Whether removing this agent's worktree would throw away uncommitted work. */
export function agentHasChanges(id: number): Promise<boolean> {
  return invoke<boolean>("ide_agent_has_changes", { id });
}

/** Removes an agent's worktree. `force` discards uncommitted work in it. */
export function discardWorktree(id: number, force: boolean): Promise<boolean> {
  return invoke<boolean>("discard_ide_agent_worktree", { id, force });
}
