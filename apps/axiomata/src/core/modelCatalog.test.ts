import { describe, expect, it } from "vitest";

import { MAX_SHOWN, type ModelCatalog, engineOptions, filterOptions, priceHint, providerOptions } from "./modelCatalog";

const catalog: ModelCatalog = {
  version: 1,
  providers: {
    anthropic: { models: [{ id: "claude-haiku-5-5", name: "Claude Haiku 5.5" }], fetched_at: null, error: null },
    open_router: {
      models: [
        {
          id: "z-ai/glm-5.3-flash",
          name: "GLM 5.3 Flash",
          context_length: 131072,
          price: { input_per_m: 0.048, output_per_m: 0.16 },
        },
        { id: "a/free", name: "a/free", price: { input_per_m: 0, output_per_m: 0 } },
      ],
      fetched_at: "2026-10-19T18:00:00Z",
      error: null,
    },
    ollama: { models: [{ id: "gemma4:12b-mlx", name: "gemma4:12b-mlx" }], fetched_at: null, error: null },
  },
};

describe("model catalog options", () => {
  it("offers a provider's bare ids with price and context as the hint", () => {
    const options = providerOptions(catalog, "open_router");
    expect(options[0]).toEqual({ value: "z-ai/glm-5.3-flash", label: "GLM 5.3 Flash", hint: "$0.048 in / $0.16 out per M · 131k ctx" });
    expect(options[1]).toEqual({ value: "a/free", label: "", hint: "free" });
    expect(providerOptions(null, "ollama")).toEqual([]);
  });

  it("gives Glut engines the bare ids of the providers it speaks to", () => {
    expect(engineOptions(catalog, "glut").map((o) => o.value)).toEqual(["z-ai/glm-5.3-flash", "a/free", "gemma4:12b-mlx"]);
  });

  it("gives Claude Code engines bare Anthropic ids and Opencode provider/model", () => {
    expect(engineOptions(catalog, "claude_code").map((o) => o.value)).toEqual(["claude-haiku-5-5"]);
    expect(engineOptions(catalog, "opencode").map((o) => o.value)).toEqual([
      "anthropic/claude-haiku-5-5",
      "openrouter/z-ai/glm-5.3-flash",
      "openrouter/a/free",
      "ollama/gemma4:12b-mlx",
    ]);
  });

  it("filters by every typed word over value and label, and caps the list", () => {
    const options = engineOptions(catalog, "opencode");
    expect(filterOptions(options, "glm flash").map((o) => o.value)).toEqual(["openrouter/z-ai/glm-5.3-flash"]);
    expect(filterOptions(options, "HAIKU 5.5").map((o) => o.value)).toEqual(["anthropic/claude-haiku-5-5"]);
    expect(filterOptions(options, "nothing")).toEqual([]);
    const many = Array.from({ length: MAX_SHOWN + 5 }, (_, i) => ({ value: `m${i}`, label: "", hint: "" }));
    expect(filterOptions(many, "")).toHaveLength(MAX_SHOWN);
  });

  it("prints no price for a model without one", () => {
    expect(priceHint(undefined)).toBe("");
  });
});
