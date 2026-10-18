import { describe, expect, it } from "vitest";

import type { IdeAgent } from "../core/backend";
import type { Role } from "../core/roster";
import { isSlug } from "../core/roster";
import {
  agentNameFor,
  engineIdFor,
  engineName,
  groupRoles,
  harnessBadge,
  harnessLabel,
  modelShort,
  roleFamilies,
  roleIdFor,
  roleTitle,
  roleTone,
  slugify,
  startableAgent,
  uniqueSlug,
  variantLabel,
} from "./rosterLook";

function agent(over: Partial<IdeAgent>): IdeAgent {
  return {
    id: 1,
    name: "A",
    harness: "opencode",
    model: null,
    engine_id: "e1",
    agent_role: "reviewer",
    updated_at: "2026-10-01T10:00:00Z",
    ...over,
  } as IdeAgent;
}

describe("slugify", () => {
  it("makes a name an id the roster accepts", () => {
    expect(slugify("Reviewer light")).toBe("reviewer-light");
    expect(slugify("  Claude Code · Opus 5.5  ")).toBe("claude-code-opus-5-5");
    expect(slugify("Überprüfer")).toBe("uberprufer");
    for (const text of ["Reviewer light", "a_b", "x".repeat(80), "Planer (groß)"]) {
      expect(isSlug(slugify(text)), text).toBe(true);
    }
  });

  it("is empty for a text with no letter or digit, and never ends in a dash after cutting", () => {
    expect(slugify("···")).toBe("");
    expect(slugify(`${"a".repeat(47)} b`)).toBe("a".repeat(47));
  });
});

describe("uniqueSlug", () => {
  it("keeps a free id and numbers a taken one", () => {
    expect(uniqueSlug("planner", [])).toBe("planner");
    expect(uniqueSlug("planner", ["planner"])).toBe("planner-2");
    expect(uniqueSlug("planner", new Set(["planner", "planner-2"]))).toBe("planner-3");
  });

  it("keeps the numbered id within the id length", () => {
    const long = "a".repeat(48);
    const next = uniqueSlug(long, [long]);
    expect(next.length).toBeLessThanOrEqual(48);
    expect(isSlug(next)).toBe(true);
    expect(next.endsWith("-2")).toBe(true);
  });
});

describe("derived ids", () => {
  it("builds a role's id from its name, else from kind and tier", () => {
    expect(roleIdFor("Reviewer light", "review", "light", [])).toBe("reviewer-light");
    expect(roleIdFor("", "review", "light", [])).toBe("review-light");
    expect(roleIdFor("Reviewer light", "review", "light", ["reviewer-light"])).toBe("reviewer-light-2");
  });

  it("builds an engine's id from its label, else from harness and model", () => {
    expect(engineIdFor("Claude Code · Haiku 5.5", "claude_code", "x", [])).toBe("claude-code-haiku-5-5");
    expect(engineIdFor("", "opencode", "openrouter/deepseek/deepseek-v4.1-flash", [])).toBe("opencode-deepseek-v4-1-flash");
    expect(engineIdFor("", "claude_code", null, [])).toBe("claude-code-default-model");
    expect(engineIdFor("Opus", "claude_code", null, ["opus"])).toBe("opus-2");
  });
});

describe("how an agent is named", () => {
  it("drops the provider path from a model id", () => {
    expect(modelShort("openrouter/deepseek/deepseek-v4.1-flash")).toBe("deepseek-v4.1-flash");
    expect(modelShort("claude-haiku-5-5")).toBe("claude-haiku-5-5");
    expect(modelShort(null)).toBe("default model");
    expect(modelShort("  ")).toBe("default model");
  });

  it("names the harness and its badge", () => {
    expect(harnessLabel("claude_code")).toBe("Claude Code");
    expect(harnessLabel("opencode")).toBe("Opencode");
    expect(harnessBadge("claude_code")).toBe("CC");
    expect(harnessBadge("opencode")).toBe("OC");
    expect(harnessBadge("mini")).toBe("M");
    expect(harnessBadge("other")).toBe("OT");
  });

  it("shows the engine's label, or harness and short model for an agent without an engine", () => {
    const engines = [{ id: "e1", label: "Claude Code · Haiku 5.5" }];
    expect(engineName(agent({ engine_id: "e1" }), engines)).toBe("Claude Code · Haiku 5.5");
    expect(
      engineName(agent({ engine_id: null, harness: "opencode", model: "openrouter/z-ai/glm-5.3-flash" }), engines),
    ).toBe("Opencode · glm-5.3-flash");
    expect(engineName(agent({ engine_id: "gone", harness: "claude_code", model: null }), engines)).toBe(
      "Claude Code · default model",
    );
  });
});

describe("roleTone", () => {
  it("tints a role by its kind of work", () => {
    expect(roleTone("plan")).toBe("plan");
    expect(roleTone("Review")).toBe("review");
    expect(roleTone("implement")).toBe("implement");
    expect(roleTone("test")).toBe("implement");
    expect(roleTone("allrounder")).toBe("other");
    expect(roleTone(undefined)).toBe("other");
  });
});

describe("new agents for a role", () => {
  it("reads an id as a name", () => {
    expect(roleTitle("reviewer-light")).toBe("Reviewer light");
    expect(roleTitle("planner")).toBe("Planner");
  });

  it("numbers a name the project already has, whatever its case", () => {
    expect(agentNameFor("reviewer", [])).toBe("Reviewer");
    expect(agentNameFor("reviewer", ["Reviewer"])).toBe("Reviewer 2");
    expect(agentNameFor("reviewer", ["reviewer", "Reviewer 2"])).toBe("Reviewer 3");
  });
});

describe("startableAgent", () => {
  const role = { name: "reviewer", engine: "e1" };
  const none = new Set<number>();

  it("opens the newest agent of the role on the role's engine that has no pane", () => {
    const agents = [
      agent({ id: 1, updated_at: "2026-10-01T10:00:00Z" }),
      agent({ id: 2, updated_at: "2026-10-03T10:00:00Z" }),
      agent({ id: 3, updated_at: "2026-10-05T10:00:00Z", agent_role: "planner" }),
      agent({ id: 4, updated_at: "2026-10-06T10:00:00Z", engine_id: "e2" }),
    ];
    expect(startableAgent(agents, role, none)?.id).toBe(2);
  });

  it("skips an agent whose pane is open and makes a new one when none is left", () => {
    const agents = [agent({ id: 1 }), agent({ id: 2, updated_at: "2026-10-03T10:00:00Z" })];
    expect(startableAgent(agents, role, new Set([2]))?.id).toBe(1);
    expect(startableAgent(agents, role, new Set([1, 2]))).toBeNull();
    expect(startableAgent([], role, none)).toBeNull();
  });

  it("takes any engine for a role that has none of its own", () => {
    const planner = { name: "planner", engine: null };
    expect(startableAgent([agent({ id: 7, agent_role: "planner", engine_id: "e9" })], planner, none)?.id).toBe(7);
  });
});

describe("edges", () => {
  it("falls back when a name has no Latin letter or digit", () => {
    expect(slugify("役割")).toBe("");
    expect(roleIdFor("役割", "review", "light", [])).toBe("review-light");
    expect(roleIdFor("役割", "···", "···", [])).toBe("role");
    expect(engineIdFor("役割", "claude_code", null, [])).toBe("claude-code-default-model");
    expect(engineIdFor("···", "···" as never, "···", [])).toBe("engine");
  });

  it("does not end a numbered id in a dash when the base is cut", () => {
    const base = `${"a".repeat(45)}-b`;
    const next = uniqueSlug(base, [base]);
    expect(isSlug(next)).toBe(true);
    expect(next).toBe(`${"a".repeat(45)}-2`);
  });

  it("reads a model with a trailing slash as its last real segment", () => {
    expect(modelShort("a/b/")).toBe("b");
    expect(modelShort("/")).toBe("default model");
  });

  it("tints the planner, reviewer, implementer and doc roles by their kind words", () => {
    expect(roleTone("planner")).toBe("plan");
    expect(roleTone("reviewer")).toBe("review");
    expect(roleTone("implementer")).toBe("implement");
    expect(roleTone("doc")).toBe("implement");
  });

  it("names a new agent past padded and mixed-case names", () => {
    expect(agentNameFor("reviewer", ["  REVIEWER "])).toBe("Reviewer 2");
  });

  it("picks the first agent on equal dates, and treats an unreadable date as the oldest", () => {
    const role = { name: "reviewer", engine: "e1" };
    const same = [agent({ id: 1 }), agent({ id: 2 })];
    expect(startableAgent(same, role, new Set())?.id).toBe(1);
    const odd = [agent({ id: 1, updated_at: "not a date" }), agent({ id: 2, updated_at: "2026-10-01T10:00:00Z" })];
    expect(startableAgent(odd, role, new Set())?.id).toBe(2);
    expect(startableAgent([...odd].reverse(), role, new Set())?.id).toBe(2);
  });
});

describe("groupRoles", () => {
  const role = (name: string, kind: string, tier: Role["tier"] = "medium") => ({ name, kind, tier }) as Role;

  it("groups by kind in reading order and sorts a group by tier, then name", () => {
    const groups = groupRoles([
      role("reviewer-heavy", "review", "heavy"),
      role("tester", "test"),
      role("implementer-light", "implement", "light"),
      role("planner", "plan", "heavy"),
      role("reviewer-light", "review", "light"),
      role("allrounder", "implement"),
      role("implementer-heavy", "implement", "heavy"),
    ]);
    expect(groups.map((g) => g.title)).toEqual(["Planning", "Implementation", "Review", "Testing"]);
    expect(groups[1]?.roles.map((r) => r.name)).toEqual(["implementer-light", "allrounder", "implementer-heavy"]);
    expect(groups[2]?.roles.map((r) => r.name)).toEqual(["reviewer-light", "reviewer-heavy"]);
  });

  it("puts kinds it does not know after the known ones, alphabetically, under their own name", () => {
    const groups = groupRoles([role("grill", "grill"), role("zed", "zeta"), role("doc", "doc"), role("p", "plan")]);
    expect(groups.map((g) => g.kind)).toEqual(["plan", "doc", "grill", "zeta"]);
    expect(groups[2]?.title).toBe("Grill");
  });

  it("folds one job on several models into a family under the chip's name", () => {
    const families = roleFamilies([
      role("implementer-light", "implement", "light"),
      role("allrounder", "implement"),
      role("implementer-haiku", "implement"),
      role("allrounder-heavy", "implement", "heavy"),
    ]);
    expect(families.map((f) => [f.base, f.roles.map((r) => r.name)])).toEqual([
      ["implementer", ["implementer-light", "implementer-haiku"]],
      ["allrounder", ["allrounder", "allrounder-heavy"]],
    ]);
    expect(roleFamilies([])).toEqual([]);
  });

  it("calls a variant by the rest of its name, the plain role by its engine", () => {
    const engines = [
      { id: "opus", label: "Claude Opus 5.5", model: null },
      { id: "mimo", label: "Opencode · MiMo", model: "openrouter/xiaomi/mimo-v2-flash" },
    ];
    const named = (name: string, engine: string | null) => ({ name, engine }) as Role;
    expect(variantLabel(named("planner-haiku", "h"), "planner", engines)).toBe("haiku");
    expect(variantLabel(named("planner", "opus"), "planner", engines)).toBe("Claude Opus 5.5");
    expect(variantLabel(named("planner", "mimo"), "planner", engines)).toBe("mimo-v2-flash");
    expect(variantLabel(named("planner", null), "planner", engines)).toBe("pick engine");
    expect(variantLabel(named("planner", "gone"), "planner", engines)).toBe("planner");
  });

  it("gives no group for no roles, and sorts an unknown tier last", () => {
    expect(groupRoles([])).toEqual([]);
    const odd = groupRoles([role("b", "review", "heavy"), role("a", "review", "weird" as Role["tier"])]);
    expect(odd[0]?.roles.map((r) => r.name)).toEqual(["b", "a"]);
  });
});

