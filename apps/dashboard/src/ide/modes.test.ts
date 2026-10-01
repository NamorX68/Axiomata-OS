import { describe, expect, it } from "vitest";

import { allTabs, serializeLayout, singleGroupLayout, type PaneTab } from "./layout";
import { emptyEditorLayout, parseWorkspace, serializeWorkspace, switchMode, type Workspace } from "./modes";

const tab = (id: string, kind = "terminal"): PaneTab => ({ id, kind, title: id });
const fallback = { editor: emptyEditorLayout, agents: () => singleGroupLayout([tab("fresh")]) };
const ws = (): Workspace => ({ mode: "agents", active: singleGroupLayout([tab("a")]), parked: singleGroupLayout([tab("f", "file")]) });

describe("modes", () => {
  it("switching parks the shown layout and shows the parked one", () => {
    const next = switchMode(ws());
    expect(next.mode).toBe("editor");
    expect(allTabs(next.active).map((t) => t.id)).toEqual(["f"]);
    expect(allTabs(next.parked).map((t) => t.id)).toEqual(["a"]);
    expect(switchMode(next).mode).toBe("agents");
  });

  it("round-trips both layouts and the mode", () => {
    const back = parseWorkspace(serializeWorkspace(switchMode(ws())), fallback)!;
    expect(back.mode).toBe("editor");
    expect(allTabs(back.active).map((t) => t.id)).toEqual(["f"]);
    expect(allTabs(back.parked).map((t) => t.id)).toEqual(["a"]);
  });

  it("reads an old single layout as the Agents layout, Editor empty", () => {
    const back = parseWorkspace(serializeLayout(singleGroupLayout([tab("old")])), fallback)!;
    expect(back.mode).toBe("agents");
    expect(allTabs(back.active).map((t) => t.id)).toEqual(["old"]);
    expect(allTabs(back.parked)).toEqual([]);
  });

  it("keeps one half when the other is damaged", () => {
    const stored = JSON.stringify({ mode: "agents", layouts: { editor: "junk", agents: JSON.parse(serializeLayout(singleGroupLayout([tab("a")]))) } });
    const back = parseWorkspace(stored, fallback)!;
    expect(allTabs(back.active).map((t) => t.id)).toEqual(["a"]);
    expect(allTabs(back.parked)).toEqual([]);
  });

  it("refuses what holds nothing usable", () => {
    expect(parseWorkspace("{ not json", fallback)).toBeNull();
    expect(parseWorkspace({ layouts: { editor: 1, agents: 2 } }, fallback)).toBeNull();
  });
});
