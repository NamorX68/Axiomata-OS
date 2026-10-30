import { describe, expect, it } from "vitest";

import { mockEngineEntries, mockModelCommand } from "./devmockModels";
import type { Engine, EngineEntry, Role } from "./roster";

const own: Engine = {
  id: "claude-code-claude-opus-5-5",
  label: "Mine",
  harness: "claude_code",
  command: "",
  model: "claude-opus-5-5",
  env: "",
  billing: "subscription",
  tier: "medium",
};

const role = { name: "allrounder" } as Role;

describe("the mock's engines and agents", () => {
  it("lists own engines first and lets one win the id of an automatic engine", () => {
    const entries = mockEngineEntries([own], () => 0);
    expect(entries[0]).toMatchObject({ id: own.id, origin: "own", rated_tier: "medium" });
    expect(entries.filter((e) => e.id === own.id)).toHaveLength(1);
    expect(entries.some((e) => e.id === "glut-openrouter-deepseek-deepseek-v4-1-flash" && e.origin === "auto")).toBe(
      true,
    );
  });

  it("names agents harness, tier, role and model, the tier's cheapest engine first", () => {
    const engines = (): EngineEntry[] => mockEngineEntries([], () => 0);
    const answer = mockModelCommand("list_agent_choices", {}, engines, () => [role]);
    const choices = answer?.value as { name: string; tier: string; standard: boolean }[];
    const light = choices.filter((c) => c.tier === "light");
    expect(light[0]).toMatchObject({ name: "Glut Light Allrounder (DeepSeek V4.1 Flash)", standard: true });
    expect(light[1].name).toBe("Claude Light Allrounder (Haiku 5.5)");
    expect(mockModelCommand("get_app_info", {}, engines, () => [])).toBeUndefined();
  });

  it("forgets the proposals a new research asks for", () => {
    const engines = (): EngineEntry[] => mockEngineEntries([], () => 0);
    const list = () =>
      (mockModelCommand("list_models", {}, engines, () => [])?.value as { models: { key: string; proposal: unknown }[] })
        .models;
    expect(list().find((m) => m.key === "openrouter/z-ai/glm-5.3")?.proposal).not.toBeNull();
    const asked = mockModelCommand("research_model_tiers", { keys: ["openrouter/z-ai/glm-5.3"] }, engines, () => []);
    expect(asked?.value).toEqual(["openrouter/z-ai/glm-5.3"]);
    expect(list().find((m) => m.key === "openrouter/z-ai/glm-5.3")?.proposal).toBeNull();
    expect(() => mockModelCommand("research_model_tiers", { keys: ["x/y"] }, engines, () => [])).toThrow();
  });
});
