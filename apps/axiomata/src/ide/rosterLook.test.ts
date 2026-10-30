import { describe, expect, it } from "vitest";

import type { IdeAgent } from "../core/backend";
import { isSlug } from "../core/roster";
import {
  engineIdFor,
  engineName,
  harnessBadge,
  harnessLabel,
  modelShort,
  roleIdFor,
  roleBadge,
  roleTitle,
  roleTone,
  slugify,
  uniqueSlug,
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
    expect(harnessBadge("glut")).toBe("GL");
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
    expect(roleBadge("allrounder")).toBe("AL");
    expect(roleBadge("grill")).toBe("GR");
    expect(roleBadge("-")).toBe("?");
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


});

