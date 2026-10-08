import { describe, expect, it } from "vitest";

import { allTabs, serializeLayout, singleGroupLayout, type PaneTab } from "./layout";
import {
  MODE_ICON,
  MODE_LABEL,
  MODES,
  emptyEditorLayout,
  layoutsOf,
  parkedLayouts,
  parseWorkspace,
  serializeWorkspace,
  switchMode,
  type Workspace,
} from "./modes";

const tab = (id: string, kind = "terminal"): PaneTab => ({ id, kind, title: id });
const fallback = {
  editor: emptyEditorLayout,
  agents: () => singleGroupLayout([tab("fresh")]),
  flow: () => singleGroupLayout([tab("plan-fresh", "plan")]),
};
const ids = (layout: Parameters<typeof allTabs>[0]) => allTabs(layout).map((t) => t.id);
const ws = (): Workspace => ({
  mode: "agents",
  active: singleGroupLayout([tab("a")]),
  parked: { editor: singleGroupLayout([tab("f", "file")]), flow: singleGroupLayout([tab("p", "plan")]) },
});

describe("modes", () => {
  it("names the Canvas for what is stored as `agents`", () => {
    expect(MODES).toEqual(["editor", "agents", "flow"]);
    expect(MODE_LABEL).toEqual({ editor: "Editor", agents: "Canvas", flow: "Flow" });
  });

  it("gives every mode its own icon", () => {
    expect(new Set(MODES.map((m) => MODE_ICON[m])).size).toBe(MODES.length);
  });

  it("switching parks the shown layout and shows the one parked for the target", () => {
    const next = switchMode(ws(), "flow");
    expect(next.mode).toBe("flow");
    expect(ids(next.active)).toEqual(["p"]);
    expect(Object.keys(next.parked).sort()).toEqual(["agents", "editor"]);
    expect(ids(next.parked.agents!)).toEqual(["a"]);
    expect(ids(next.parked.editor!)).toEqual(["f"]);
    // And back: nothing was lost on the way.
    const back = switchMode(next, "agents");
    expect(ids(back.active)).toEqual(["a"]);
    expect(ids(back.parked.flow!)).toEqual(["p"]);
  });

  it("switching to the mode that is shown changes nothing", () => {
    const same = ws();
    expect(switchMode(same, "agents")).toBe(same);
  });

  it("a mode with no layout yet starts empty", () => {
    const old: Workspace = { mode: "agents", active: singleGroupLayout([tab("a")]), parked: {} };
    expect(ids(switchMode(old, "flow").active)).toEqual([]);
  });

  it("lists every layout that is not on screen, so their panes can stay mounted", () => {
    expect(parkedLayouts(ws().parked).flatMap(ids).sort()).toEqual(["f", "p"]);
    expect(parkedLayouts({})).toEqual([]);
  });

  it("round-trips all three layouts and the mode", () => {
    const back = parseWorkspace(serializeWorkspace(switchMode(ws(), "flow")), fallback)!;
    expect(back.mode).toBe("flow");
    expect(ids(back.active)).toEqual(["p"]);
    const all = layoutsOf(back);
    expect(ids(all.agents)).toEqual(["a"]);
    expect(ids(all.editor)).toEqual(["f"]);
  });

  it("reads a two-mode row, which has no Flow, with the starting Flow layout", () => {
    const stored = JSON.stringify({
      mode: "editor",
      layouts: {
        editor: JSON.parse(serializeLayout(singleGroupLayout([tab("f", "file")]))),
        agents: JSON.parse(serializeLayout(singleGroupLayout([tab("a")]))),
      },
    });
    const back = parseWorkspace(stored, fallback)!;
    expect(back.mode).toBe("editor");
    expect(ids(back.active)).toEqual(["f"]);
    expect(ids(back.parked.flow!)).toEqual(["plan-fresh"]);
    expect(ids(back.parked.agents!)).toEqual(["a"]);
  });

  it("reads an old single layout as the Canvas layout, the others starting", () => {
    const back = parseWorkspace(serializeLayout(singleGroupLayout([tab("old")])), fallback)!;
    expect(back.mode).toBe("agents");
    expect(ids(back.active)).toEqual(["old"]);
    expect(ids(back.parked.editor!)).toEqual([]);
    expect(ids(back.parked.flow!)).toEqual(["plan-fresh"]);
  });

  it("keeps the other halves when one is damaged", () => {
    const stored = JSON.stringify({
      mode: "agents",
      layouts: { editor: "junk", agents: JSON.parse(serializeLayout(singleGroupLayout([tab("a")]))), flow: 3 },
    });
    const back = parseWorkspace(stored, fallback)!;
    expect(ids(back.active)).toEqual(["a"]);
    expect(ids(back.parked.editor!)).toEqual([]);
    expect(ids(back.parked.flow!)).toEqual(["plan-fresh"]);
  });

  it("falls back to the Canvas for a mode it does not know", () => {
    const stored = JSON.stringify({
      mode: "cinema",
      layouts: { agents: JSON.parse(serializeLayout(singleGroupLayout([tab("a")]))) },
    });
    expect(parseWorkspace(stored, fallback)!.mode).toBe("agents");
  });

  it("refuses what holds nothing usable", () => {
    expect(parseWorkspace("{ not json", fallback)).toBeNull();
    expect(parseWorkspace({ layouts: { editor: 1, agents: 2, flow: 3 } }, fallback)).toBeNull();
  });
});
