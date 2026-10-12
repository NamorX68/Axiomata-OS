import { describe, expect, it } from "vitest";

import type { IdeAgent } from "../core/backend";
import { trailAuthor } from "./trailAuthor";

const agent = (id: number, project_id: number, worktree_path: string | null): IdeAgent =>
  ({ id, project_id, name: `agent-${id}`, harness: "opencode", worktree_path }) as IdeAgent;

const AGENTS = [agent(1, 3, "/w/1"), agent(2, 3, null), agent(3, 3, null), agent(4, 5, null)];

describe("trailAuthor", () => {
  it("names the agent whose worktree the file is in, working or not", () => {
    expect(trailAuthor("worktree:1", AGENTS, () => false)?.id).toBe(1);
  });

  it("names the one agent at work in a shared project folder", () => {
    expect(trailAuthor("project:3", AGENTS, (id) => id === 2)?.id).toBe(2);
  });

  it("names nobody when several or none work there, for an unknown agent, or for any other root", () => {
    expect(trailAuthor("project:3", AGENTS, (id) => id === 2 || id === 3)).toBeNull();
    expect(trailAuthor("project:3", AGENTS, () => false)).toBeNull();
    expect(trailAuthor("worktree:9", AGENTS, () => true)).toBeNull();
    expect(trailAuthor("workspace", AGENTS, () => true)).toBeNull();
  });

  it("does not count an agent with a worktree of its own for the project folder", () => {
    expect(trailAuthor("project:3", AGENTS, (id) => id === 1)).toBeNull();
  });
});
