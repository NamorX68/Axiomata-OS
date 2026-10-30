import { describe, expect, it } from "vitest";

import type { IdeAgent } from "../core/backend";
import type { Role } from "../core/roster";
import { engineChoiceLabel, enginesOf, isFavorite, sessionLines, sessionSections, startableRoles } from "./agentStart";

const agent = (id: number, role = "allrounder"): IdeAgent => ({ id, agent_role: role, name: `a${id}` }) as IdeAgent;
const role = (name: string): Role => ({ name }) as Role;

describe("starting an agent", () => {
  it("offers the seven fixed roles first, in their order, then the own ones, never the master", () => {
    const names = startableRoles([role("zeta"), role("grill"), role("master"), role("allrounder"), role("alpha")]).map(
      (r) => r.name,
    );
    expect(names).toEqual(["allrounder", "grill", "alpha", "zeta"]);
  });

  it("finds a tier's engines and a favorite with or without an engine", () => {
    const byTier = [{ tier: "light" as const, engines: [{ id: "a" }, { id: "b" }] as never[] }];
    expect(enginesOf(byTier, "light")).toHaveLength(2);
    expect(enginesOf(byTier, "heavy")).toEqual([]);
    const favorites = [{ role: "allrounder", tier: "medium" as const, engine: null }];
    expect(isFavorite(favorites, { role: "allrounder", tier: "medium" })).toBe(true);
    expect(isFavorite(favorites, { role: "allrounder", tier: "medium", engine: "x" })).toBe(false);
  });

  it("names an engine by harness and model, an own one by its label", () => {
    expect(engineChoiceLabel({ origin: "auto", harness: "claude_code", model_name: "Haiku 5.5", label: "x" })).toBe(
      "Claude · Haiku 5.5",
    );
    expect(engineChoiceLabel({ origin: "own", harness: "glut", model_name: "GLM", label: "Mine" })).toBe("Mine");
  });
});

describe("the sessions the tab lists (Q17)", () => {
  it("shows running ones, ended ones with work under “waiting for you”, and hides ended ones without work", () => {
    const agents = [agent(1), agent(2), agent(3), agent(4), agent(5, "master")];
    const states: Record<number, string> = { 1: "working", 2: "ended", 3: "ended", 4: "ended", 5: "idle" };
    const sections = sessionSections(
      agents,
      [agent(9, "master")],
      (a) => a.id === 4,
      (a) => states[a.id],
      new Set([2]),
    );
    expect(sections.active.map((a) => a.id)).toEqual([9, 1, 4]);
    expect(sections.waiting.map((a) => a.id)).toEqual([2]);
  });

  it("does not count a session without a known state as running", () => {
    const sections = sessionSections(
      [agent(1), agent(2)],
      [],
      () => false,
      () => undefined,
      new Set([2]),
    );
    expect(sections.active).toEqual([]);
    expect(sections.waiting.map((a) => a.id)).toEqual([2]);
  });
});

describe("a session's two lines (Q8)", () => {
  it("reads the name and shows the engine the session runs on now", () => {
    const name = "Glut Medium Builder (GLM 5.3) #2";
    expect(sessionLines(name, undefined)).toEqual({ title: "Builder · Medium #2", subtitle: "Glut · GLM 5.3" });
    // Escalated: the name keeps the old tier and model, the second line follows the new engine.
    const now = { origin: "auto" as const, harness: "claude_code" as const, model_name: "Opus 5.5", label: "x" };
    expect(sessionLines(name, now)?.subtitle).toBe("Claude · Opus 5.5");
    expect(sessionLines("Builder", now)).toBeNull();
  });
});
