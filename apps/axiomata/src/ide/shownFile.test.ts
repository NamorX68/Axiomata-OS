import { describe, expect, it } from "vitest";

import { activateTab, addTab, singleGroupLayout, type Layout, type PaneTab } from "./layout";
import { agentDiffTab, fileTab, untitledTab } from "./paneKinds";
import { shownFileTab } from "./shownFile";

const terminal: PaneTab = { id: "terminal", kind: "terminal", title: "Terminal" };

/** `tab` beside everything already in `layout`, in a group of its own. */
function beside(layout: Layout, tab: PaneTab): Layout {
  return addTab(layout, tab, { nodeId: layout.root.id, side: "right" });
}

/** `tab` as a new tab of the group holding `layout`'s root, made the active one. */
function inGroup(layout: Layout, tab: PaneTab): Layout {
  return addTab(layout, tab, { nodeId: layout.root.id, side: "center" });
}

describe("shownFileTab", () => {
  it("shows the file pane the user last worked in, even when another file pane is in front", () => {
    const a = fileTab("project:1", "a.ts", null);
    const b = fileTab("project:1", "b.ts", null);
    const layout = beside(singleGroupLayout([a]), b);
    expect(shownFileTab(layout, a.id, b.id)?.id).toBe(a.id);
    expect(shownFileTab(layout, b.id, a.id)?.id).toBe(b.id);
  });

  it("falls back to the last-focused file pane when the last-work pane is a terminal", () => {
    const a = fileTab("project:1", "a.ts", null);
    const layout = beside(singleGroupLayout([a]), terminal);
    expect(shownFileTab(layout, terminal.id, a.id)?.id).toBe(a.id);
  });

  it("falls back to the last-focused file pane when the last-work id is stale", () => {
    const a = fileTab("project:1", "a.ts", null);
    const layout = singleGroupLayout([a]);
    expect(shownFileTab(layout, "gone", a.id)?.id).toBe(a.id);
  });

  it("falls back to the last-focused file pane when the last-work pane is an untitled note", () => {
    const a = fileTab("project:1", "a.ts", null);
    const note = untitledTab();
    const layout = beside(singleGroupLayout([a]), note);
    expect(shownFileTab(layout, note.id, a.id)?.id).toBe(a.id);
  });

  it("falls back to the front file group when no file has been focused yet", () => {
    const a = fileTab("project:1", "a.ts", null);
    const layout = inGroup(singleGroupLayout([terminal]), a);
    expect(shownFileTab(layout, null, null)?.id).toBe(a.id);
    expect(shownFileTab(layout, "gone", "gone-too")?.id).toBe(a.id);
  });

  it("shows nothing when no file is on screen at all", () => {
    const layout = singleGroupLayout([terminal]);
    expect(shownFileTab(layout, null, null)).toBeNull();
    expect(shownFileTab(layout, terminal.id, "gone")).toBeNull();
  });

  it("does not count a file tab that is no longer its group's active tab", () => {
    const a = fileTab("project:1", "a.ts", null);
    const b = fileTab("project:1", "b.ts", null);
    // `b` covers `a` in the same group.
    const layout = inGroup(singleGroupLayout([a]), b);
    expect(shownFileTab(layout, a.id, null)?.id).toBe(b.id);
    expect(shownFileTab(layout, null, a.id)?.id).toBe(b.id);
    const back = activateTab(layout, a.id);
    expect(shownFileTab(back, b.id, null)?.id).toBe(a.id);
  });

  it("does not count a hidden last-focused file pane, and uses the front group instead", () => {
    const a = fileTab("project:1", "a.ts", null);
    const b = fileTab("project:1", "b.ts", null);
    const layout = beside(inGroup(singleGroupLayout([a]), b), terminal);
    // The terminal is the last-work pane; `a` was focused but `b` covers it, so the front group shows `b`.
    expect(shownFileTab(layout, terminal.id, a.id)?.id).toBe(b.id);
  });

  it("never shows an agent diff as the file", () => {
    const diff = agentDiffTab(1, "Builder");
    const layout = singleGroupLayout([diff]);
    expect(shownFileTab(layout, diff.id, diff.id)).toBeNull();
  });
});
