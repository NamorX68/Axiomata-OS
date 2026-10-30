import { describe, expect, it } from "vitest";

import {
  choiceWithTier,
  contextLabel,
  engineOptionsFor,
  filterModels,
  isWebAddress,
  limitsProblem,
  modelsByProvider,
  priceLabel,
  proposalWaits,
  sourceHost,
  tierOrigin,
  toggledChoice,
  type ModelEntry,
} from "./models";

function model(key: string, provider: ModelEntry["provider"]): ModelEntry {
  return {
    key,
    provider,
    id: key.split("/").slice(1).join("/"),
    name: key,
    short_name: key,
    enabled: true,
    enabled_by_default: true,
    tier: "medium",
    tier_source: "price",
    owner_tier: null,
    proposal: null,
    price_tier: "medium",
    price: null,
    context_length: null,
  };
}

describe("modelsByProvider", () => {
  it("groups by provider in the tab's order and drops empty groups", () => {
    const groups = modelsByProvider([
      model("ollama/q", "ollama"),
      model("anthropic/a", "anthropic"),
      model("ollama/r", "ollama"),
    ]);
    expect(groups.map((g) => g.provider)).toEqual(["anthropic", "ollama"]);
    expect(groups[1].models.map((m) => m.key)).toEqual(["ollama/q", "ollama/r"]);
  });
});

describe("toggledChoice", () => {
  it("switches a model and keeps the owner's tier", () => {
    expect(toggledChoice({ enabled: true, enabled_by_default: true, owner_tier: "heavy" })).toEqual({
      enabled: false,
      tier: "heavy",
    });
  });

  it("says nothing once the switch is back at its default", () => {
    expect(toggledChoice({ enabled: false, enabled_by_default: true, owner_tier: null })).toEqual({
      enabled: null,
      tier: null,
    });
    expect(toggledChoice({ enabled: false, enabled_by_default: false, owner_tier: null })).toEqual({
      enabled: true,
      tier: null,
    });
  });
});

describe("the Models tab's helpers", () => {
  it("says where a tier comes from", () => {
    const proposal = { tier: "heavy" as const, reason: "r", source: "https://x", at: "2026-10-30T10:00:00Z" };
    expect(tierOrigin({ tier_source: "owner", owner_tier: "heavy", proposal })).toBe("Yours");
    expect(tierOrigin({ tier_source: "owner", owner_tier: "light", proposal })).toBe("Yours · the proposal says heavy");
    expect(tierOrigin({ tier_source: "research", owner_tier: null, proposal })).toBe("Proposal · not reviewed");
    expect(proposalWaits({ tier_source: "research", proposal })).toBe(true);
    expect(proposalWaits({ tier_source: "owner", proposal })).toBe(false);
    expect(tierOrigin({ tier_source: "unrated", owner_tier: null, proposal: null })).toContain("Unrated");
  });

  it("keeps the switch when the tier changes and drops what equals the default", () => {
    expect(choiceWithTier({ enabled: true, enabled_by_default: true }, "light")).toEqual({ enabled: null, tier: "light" });
    expect(choiceWithTier({ enabled: true, enabled_by_default: false }, null)).toEqual({ enabled: true, tier: null });
  });

  it("filters by key or name", () => {
    const models = [model("openrouter/z-ai/glm-5.3", "open_router"), model("openrouter/qwen/qwen3", "open_router")];
    expect(filterModels(models, "GLM").map((m) => m.key)).toEqual(["openrouter/z-ai/glm-5.3"]);
    expect(filterModels(models, "  ")).toHaveLength(2);
  });

  it("labels prices, contexts and sources", () => {
    expect(priceLabel({ provider: "open_router", price: { input_per_m: 0.1, output_per_m: 0.4 } })).toBe("$0.40 / M out");
    expect(priceLabel({ provider: "anthropic", price: { input_per_m: 4, output_per_m: 20 } })).toBe("$20 / M out");
    expect(priceLabel({ provider: "ollama", price: null })).toBe("local");
    expect(contextLabel(131_072)).toBe("131K context");
    expect(contextLabel(1_000_000)).toBe("1M context");
    expect(contextLabel(null)).toBe("");
    expect(sourceHost("https://www.lmarena.ai/leaderboard")).toBe("lmarena.ai");
    expect(isWebAddress("https://lmarena.ai")).toBe(true);
    expect(isWebAddress("javascript:alert(1)")).toBe(false);
    expect(isWebAddress("LMArena")).toBe(false);
  });

  it("checks the price limits and offers a tier's usable engines, the cheapest first", () => {
    expect(limitsProblem(1, 10)).toBeNull();
    expect(limitsProblem(11, 10)).not.toBeNull();
    expect(limitsProblem(Number.NaN, 10)).not.toBeNull();
    const e = (id: string, tier: "light" | "medium", price: number | null, unusable: string | null = null) => ({
      id,
      label: id,
      rated_tier: tier,
      output_per_m: price,
      unusable,
    });
    const options = engineOptionsFor(
      [e("sonnet", "medium", 10), e("glm", "medium", 2.2), e("haiku", "light", 0.5), e("gone", "medium", 1, "no key")],
      "medium",
    );
    expect(options.map((o) => o.id)).toEqual(["glm", "sonnet"]);
  });
});
