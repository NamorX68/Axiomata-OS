import { describe, expect, it } from "vitest";

import { estimateLine, tokenCount } from "./vaultTagging";

describe("vault run estimate", () => {
  it("says what a step reads and costs", () => {
    expect(tokenCount(1_240_000)).toBe("1.2 M");
    expect(tokenCount(37_800)).toBe("38 k");
    expect(tokenCount(950)).toBe("950");
    const line = estimateLine({
      phase: "notes",
      notes: 252,
      bytes: 1_150_000,
      input_tokens: 1_240_000,
      output_tokens: 37_800,
      model: "m",
      cost_usd: 0.364,
    });
    expect(line).toMatch(/^252 notes \(/);
    expect(line).toContain("about 1.2 M tokens in, 38 k out");
    expect(line).toContain("about $0.36");
    expect(estimateLine({ phase: "list", notes: 1, bytes: 10, input_tokens: 5, output_tokens: 1, model: null, cost_usd: null }))
      .toContain("no price for the skill model");
  });
});
