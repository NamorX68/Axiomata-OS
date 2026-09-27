import { beforeEach, describe, expect, it, vi } from "vitest";

const settings = new Map<string, unknown>();
vi.mock("../core/persist", () => ({
  getSetting: (key: string) => settings.get(key),
  setSetting: (key: string, value: unknown) => settings.set(key, value),
}));

const { LIMIT, foldKey, forgetFolds, rememberedFolds, rememberFolds, updateRememberedFolds } = await import(
  "./foldMemory"
);

describe("foldMemory (ED5, T7)", () => {
  beforeEach(() => {
    settings.clear();
    settings.set("editor", { tabs: { tabs: [], active: 0 } });
  });

  it("keeps folds per file beside the tabs, and forgets a file with none", () => {
    const key = foldKey("workspace", "a.md");
    rememberFolds(key, [[1, 3]]);
    expect(rememberedFolds(key)).toEqual([[1, 3]]);
    expect((settings.get("editor") as { tabs: unknown }).tabs).toEqual({ tabs: [], active: 0 });
    rememberFolds(key, []);
    expect(rememberedFolds(key)).toBeUndefined();
  });

  it("updates on leaving only a file it keeps — a closed tab stays forgotten", () => {
    const key = foldKey("workspace", "a.md");
    rememberFolds(key, [[1, 3]]);
    forgetFolds(key);
    updateRememberedFolds(key, [[2, 4]]);
    expect(rememberedFolds(key)).toBeUndefined();
    rememberFolds(key, [[1, 3]]);
    updateRememberedFolds(key, [[2, 4]]);
    expect(rememberedFolds(key)).toEqual([[2, 4]]);
  });

  it("keeps at most LIMIT files, dropping the least recently written", () => {
    for (let i = 0; i <= LIMIT; i++) rememberFolds(foldKey("workspace", `${i}.md`), [[0, 1]]);
    expect(rememberedFolds(foldKey("workspace", "0.md"))).toBeUndefined();
    expect(rememberedFolds(foldKey("workspace", "1.md"))).toEqual([[0, 1]]);
    expect(rememberedFolds(foldKey("workspace", `${LIMIT}.md`))).toEqual([[0, 1]]);
  });

  it("rewriting a file makes it the most recent, so it outlives files written after it the first time", () => {
    const first = foldKey("workspace", "first.md");
    rememberFolds(first, [[0, 1]]);
    for (let i = 0; i < LIMIT - 1; i++) rememberFolds(foldKey("workspace", `${i}.md`), [[0, 1]]);
    rememberFolds(first, [[2, 3]]);
    rememberFolds(foldKey("workspace", "new.md"), [[0, 1]]);
    expect(rememberedFolds(first)).toEqual([[2, 3]]);
    expect(rememberedFolds(foldKey("workspace", "0.md"))).toBeUndefined();
  });

  it("keeps files apart by root as well as by path", () => {
    rememberFolds(foldKey("workspace", "a.md"), [[0, 1]]);
    rememberFolds(foldKey("project:1", "a.md"), [[2, 3]]);
    expect(rememberedFolds(foldKey("workspace", "a.md"))).toEqual([[0, 1]]);
    expect(rememberedFolds(foldKey("project:1", "a.md"))).toEqual([[2, 3]]);
  });

  it("reads a damaged or missing store as no folds, and forgetting an unknown file writes nothing", () => {
    settings.set("editor", { folds: [[1, 2]] });
    expect(rememberedFolds(foldKey("workspace", "a.md"))).toBeUndefined();
    settings.delete("editor");
    expect(rememberedFolds(foldKey("workspace", "a.md"))).toBeUndefined();
    forgetFolds(foldKey("workspace", "a.md"));
    rememberFolds(foldKey("workspace", "a.md"), []);
    expect(settings.has("editor")).toBe(false);
    rememberFolds(foldKey("workspace", "a.md"), [[0, 1]]);
    expect(rememberedFolds(foldKey("workspace", "a.md"))).toEqual([[0, 1]]);
  });
});
