import { describe, expect, it } from "vitest";

import { modelsByProvider, toggledChoice, type ModelEntry } from "./models";

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
