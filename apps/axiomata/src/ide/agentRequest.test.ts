import { beforeEach, describe, expect, it } from "vitest";
import { agentRequests, requestAgent, takeAgentRequests } from "./agentRequest";

describe("agent requests", () => {
  beforeEach(() => agentRequests.set([]));

  it("keeps two quick starts in order instead of overwriting the first", () => {
    requestAgent({ projectId: 1, agentId: 10 });
    requestAgent({ projectId: 1, agentId: 11 });
    expect(takeAgentRequests().map((r) => r.agentId)).toEqual([10, 11]);
  });

  it("hands every request out once", () => {
    requestAgent({ projectId: 2, agentId: 5 });
    expect(takeAgentRequests()).toHaveLength(1);
    expect(takeAgentRequests()).toEqual([]);
  });
});
