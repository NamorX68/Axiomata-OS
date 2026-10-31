import { describe, expect, it } from "vitest";

import { removalQuestion, unsavedWorkQuestion } from "./agentRemoval";

describe("removalQuestion", () => {
  it("asks only the plain question when nothing would be lost", () => {
    expect(removalQuestion("Remove the agent?", false)).toBe("Remove the agent?");
  });

  it("names the unsaved work when the worktree holds some", () => {
    const text = removalQuestion("Remove the agent?", true);
    expect(text.startsWith("Remove the agent?")).toBe(true);
    expect(text.endsWith(unsavedWorkQuestion())).toBe(true);
  });
});
