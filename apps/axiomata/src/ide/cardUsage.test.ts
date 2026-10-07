import { describe, expect, it } from "vitest";

import type { SessionUsage } from "../core/backend";
import { formatTokens, unmeasuredNote, usageLines } from "./cardUsage";

function session(over: Partial<SessionUsage> = {}): SessionUsage {
  return {
    agent_id: 1,
    name: "builder-1",
    role: "builder",
    review: false,
    billing: "metered",
    usage: { input_tokens: 900, output_tokens: 100, steps: 30 },
    cost_usd: 0.25,
    limits: { max_cost_usd: 0.5, max_tokens: 2_000, max_steps: 60 },
    measured: true,
    unmeasured: null,
    stopped: null,
    ...over,
  };
}

describe("formatTokens", () => {
  it("speaks of a token count the way a limit is spoken of", () => {
    expect(formatTokens(950)).toBe("950");
    expect(formatTokens(340_000)).toBe("340 k");
    expect(formatTokens(1_500_000)).toBe("1,5 M");
  });
});

describe("usageLines", () => {
  it("shows steps, tokens and money with the share of each limit that is used", () => {
    const lines = usageLines(session());
    expect(lines.map((l) => l.label)).toEqual(["Steps", "Tokens", "Cost"]);
    expect(lines[0]).toMatchObject({ used: "30", allowed: "60", share: 0.5, reached: false });
    expect(lines[1]).toMatchObject({ used: "1 k", allowed: "2 k", share: 0.5 });
    expect(lines[2]).toMatchObject({ used: "$0.25", allowed: "$0.50", share: 0.5 });
  });

  it("has no money line where nothing was metered", () => {
    expect(usageLines(session({ cost_usd: null, billing: "subscription" })).map((l) => l.label)).toEqual([
      "Steps",
      "Tokens",
    ]);
  });

  it("marks a used-up limit and caps the bar", () => {
    const [steps] = usageLines(session({ usage: { input_tokens: 0, output_tokens: 0, steps: 75 } }));
    expect(steps).toMatchObject({ reached: true, share: 1 });
  });
});

describe("unmeasuredNote", () => {
  it("says nothing about a session that was measured", () => {
    expect(unmeasuredNote({ measured: true, unmeasured: null })).toBeNull();
  });

  it("tells a session that has just started from one whose record cannot be read, and says what it means for the limit", () => {
    const fresh = unmeasuredNote({ measured: false, unmeasured: "no_record_yet" })!;
    expect(fresh).toContain("just started");
    expect(fresh).toContain("limit only applies");
    expect(unmeasuredNote({ measured: false, unmeasured: "service_down" })).toContain("Opencode service is not running");
    expect(unmeasuredNote({ measured: false, unmeasured: null })).toContain("could not be read");
  });
});
