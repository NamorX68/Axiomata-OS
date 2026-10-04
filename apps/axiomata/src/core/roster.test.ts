import { describe, expect, it } from "vitest";

import {
  blankEngine,
  blankRoleForm,
  checkEngine,
  checkRoleForm,
  isSlug,
  roleToForm,
  splitLines,
  splitList,
  type Role,
} from "./roster";

const ENGINES = ["opencode-flash", "claude-sonnet"];

describe("isSlug", () => {
  it("accepts lower-case words and refuses everything unsafe as a directory name", () => {
    for (const ok of ["a", "builder", "impl-light", "review_2", "x9"]) expect(isSlug(ok)).toBe(true);
    for (const bad of ["", "Builder", "-a", "a-", "a b", "a/b", "..", "a.b", "ä", "a".repeat(49)]) {
      expect(isSlug(bad), bad).toBe(false);
    }
  });
});

describe("splitList / splitLines", () => {
  it("splits on commas and newlines, trims, drops blanks and duplicates, keeps order", () => {
    expect(splitList(" b, a\n\nb ,c ")).toEqual(["b", "a", "c"]);
    expect(splitList("")).toEqual([]);
  });

  it("keeps commas inside a line for free text", () => {
    expect(splitLines("bash: ask, always\n  \nedit: allow")).toEqual(["bash: ask, always", "edit: allow"]);
  });
});

describe("checkEngine", () => {
  const valid = { ...blankEngine(), id: " claude-opus ", label: " Claude Code · Opus ", model: "  " };

  it("trims, and an empty model becomes null", () => {
    const result = checkEngine(valid);
    expect(result).toEqual({
      ok: true,
      value: { ...valid, id: "claude-opus", label: "Claude Code · Opus", model: null },
    });
  });

  it("points at the first bad field", () => {
    expect(checkEngine({ ...valid, id: "Not Valid" })).toMatchObject({ ok: false, field: "id" });
    expect(checkEngine({ ...valid, label: "   " })).toMatchObject({ ok: false, field: "label" });
    expect(checkEngine({ ...valid, label: "x".repeat(81) })).toMatchObject({ ok: false, field: "label" });
    expect(checkEngine({ ...valid, env: "FOO=1\nbroken line" })).toMatchObject({ ok: false, field: "env" });
    expect(checkEngine({ ...valid, env: "1BAD=x" })).toMatchObject({ ok: false, field: "env" });
    expect(checkEngine({ ...valid, env: "FOO=1\n\nBAR_2=x y" })).toMatchObject({ ok: true });
  });
});

describe("role form", () => {
  const filled = () => ({
    ...blankRoleForm(),
    name: " reviewer ",
    description: "Reviews diffs",
    kind: "review",
    tier: "heavy" as const,
    engine: "claude-sonnet",
    fallback: "opencode-flash",
    permissions: "bash: ask\nedit: allow",
    creates: "test, doc",
    maxCost: "1.5",
    maxTokens: "200000",
    maxSteps: "40",
    instructions: "  Check the diff.  ",
  });

  it("builds the role the form describes", () => {
    expect(checkRoleForm(filled(), ENGINES)).toEqual({
      ok: true,
      value: {
        name: "reviewer",
        description: "Reviews diffs",
        kind: "review",
        tier: "heavy",
        engine: "claude-sonnet",
        fallback_engines: ["opencode-flash"],
        permissions: ["bash: ask", "edit: allow"],
        limits: { max_cost_usd: 1.5, max_tokens: 200000, max_steps: 40 },
        creates: ["test", "doc"],
        instructions: "Check the diff.",
        source: "user",
      },
    });
  });

  it("round-trips a role through the form", () => {
    const role = (checkRoleForm(filled(), ENGINES) as { ok: true; value: Role }).value;
    expect(checkRoleForm(roleToForm(role), ENGINES)).toEqual({ ok: true, value: role });
  });

  it("an engine-less role with no limits is fine (the planner)", () => {
    const result = checkRoleForm({ ...blankRoleForm(), name: "planner", kind: "plan" }, ENGINES);
    expect(result).toMatchObject({
      ok: true,
      value: { engine: null, limits: { max_cost_usd: null, max_tokens: null, max_steps: null } },
    });
  });

  it("points at the first bad field", () => {
    const bad = (change: object, field: string) =>
      expect(checkRoleForm({ ...filled(), ...change }, ENGINES), field).toMatchObject({ ok: false, field });
    bad({ name: "Big Name" }, "name");
    bad({ kind: "two words" }, "kind");
    bad({ description: "a\nb" }, "description");
    bad({ engine: "missing" }, "engine");
    bad({ fallback: "missing" }, "fallback");
    bad({ fallback: "claude-sonnet" }, "fallback");
    bad({ creates: "ok, Bad Kind" }, "creates");
    bad({ maxCost: "-1" }, "maxCost");
    bad({ maxCost: "abc" }, "maxCost");
    bad({ maxTokens: "1.5" }, "maxTokens");
    bad({ maxSteps: "0" }, "maxSteps");
  });
});
