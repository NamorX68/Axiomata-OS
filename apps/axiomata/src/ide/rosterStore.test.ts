import { describe, expect, it } from "vitest";

import { blankSpec, specOf, specReady } from "./agents";
import { engineLine } from "./rosterStore";

describe("the agent form's choices", () => {
  it("starts on the all-rounder when there is one, else on the first role, else on nothing", () => {
    expect(blankSpec(["reviewer", "allrounder"]).role).toBe("allrounder");
    expect(blankSpec(["reviewer", "planner"]).role).toBe("reviewer");
    expect(blankSpec([]).role).toBe("");
  });

  it("needs a name, an engine and a role before it can be saved", () => {
    const ready = { name: "Builder", engine_id: "opus", role: "allrounder" };
    expect(specReady(ready)).toBe(true);
    expect(specReady({ ...ready, name: "  " })).toBe(false);
    expect(specReady({ ...ready, engine_id: "" })).toBe(false);
    expect(specReady({ ...ready, role: "" })).toBe(false);
  });

  it("reads an existing agent back as choices, with no engine for one that was never assigned", () => {
    const agent = { name: "A", engine_id: null, agent_role: "reviewer" } as Parameters<typeof specOf>[0];
    expect(specOf(agent)).toEqual({ name: "A", engine_id: "", role: "reviewer" });
    expect(specOf({ ...agent, engine_id: "opus" })).toEqual({ name: "A", engine_id: "opus", role: "reviewer" });
  });
});

describe("engineLine", () => {
  it("names the harness and the model", () => {
    expect(engineLine({ harness: "claude_code", model: "opus" })).toBe("Claude Code · opus");
    expect(engineLine({ harness: "opencode", model: null })).toBe("Opencode · default model");
  });
});
