import { describe, expect, it } from "vitest";

import { addTab, allGroups, allTabs, findTab, isSplit, singleGroupLayout, type PaneTab } from "./layout";
import {
  agentDiffOf,
  agentDiffTab,
  FILES_PANE,
  FILES_PANE_FRACTION,
  filePaneConfig,
  filesPaneConfig,
  filesTab,
  fileTab,
  frontFile,
  openOrFocus,
  projectRoot,
  showsFile,
  withFilesPane,
  worktreeAgent,
} from "./paneKinds";

describe("pane kinds (H14)", () => {
  it("makes a file tab named after the file, and reads its config back", () => {
    const tab = fileTab("worktree:3", "src/lib.rs", 12);
    expect(tab.kind).toBe("file");
    expect(tab.title).toBe("lib.rs");
    expect(filePaneConfig(tab)).toMatchObject({ root: "worktree:3", rel: "src/lib.rs", line: 12 });
    expect(showsFile(tab, "worktree:3", "src/lib.rs")).toBe(true);
    expect(showsFile(tab, "worktree:4", "src/lib.rs")).toBe(false);
  });

  it("refuses a malformed stored config instead of guessing", () => {
    expect(filePaneConfig({ id: "x", kind: "file", title: "x", config: { root: 3 } })).toBeNull();
    expect(filePaneConfig({ id: "x", kind: "terminal", title: "x", config: { root: "a", rel: "b" } })).toBeNull();
    expect(filePaneConfig({ id: "x", kind: "file", title: "x", config: { root: "a", rel: "b" } })).toEqual({
      root: "a",
      rel: "b",
      line: null,
      jump: 0,
    });
  });

  it("ties an agent-diff tab to its agent", () => {
    const tab = agentDiffTab(5, "Builder");
    expect(tab.title).toBe("Builder · Diffs");
    expect(agentDiffOf(tab)).toBe(5);
    expect(agentDiffOf(fileTab("workspace", "a.md", null))).toBeNull();
  });

  it("reads the agent out of a worktree root only", () => {
    expect(worktreeAgent("worktree:12")).toBe(12);
    expect(worktreeAgent("project:12")).toBeNull();
    expect(worktreeAgent("worktree:x")).toBeNull();
    expect(worktreeAgent("worktree:1a")).toBeNull();
    expect(worktreeAgent("workspace")).toBeNull();
    expect(worktreeAgent("")).toBeNull();
  });

  it("keeps a stored jump counter, and defaults a missing line to null", () => {
    const stored: PaneTab = {
      id: "x",
      kind: "file",
      title: "x",
      config: { root: "a", rel: "b", line: 7, jump: 42 },
    };
    expect(filePaneConfig(stored)).toEqual({ root: "a", rel: "b", line: 7, jump: 42 });
  });

  it("titles a file tab after its own name when there is no directory", () => {
    expect(fileTab("workspace", "README.md", null).title).toBe("README.md");
  });
});

describe("openOrFocus (H5, H14)", () => {
  const agent: PaneTab = { id: "agent", kind: "agent", title: "Builder", config: { agentId: 1 } };

  it("opens a first file as a tab in the group it was opened from, not as a split", () => {
    const file = fileTab("worktree:1", "a.rs", 3);
    const layout = openOrFocus(singleGroupLayout([agent]), file, () => false, "agent");
    expect(allGroups(layout)).toHaveLength(1);
    expect(findTab(layout, file.id)?.group.tabs.map((t) => t.id)).toEqual(["agent", file.id]);
    expect(findTab(layout, file.id)?.group.active).toBe(file.id);
  });

  it("gathers files in a group they were dragged out into", () => {
    const first = fileTab("worktree:1", "a.rs", null);
    const other: PaneTab = { id: "term", kind: "terminal", title: "Terminal" };
    // Two groups: the agent's, and one holding a file the user dragged out.
    let layout = openOrFocus(singleGroupLayout([agent]), agentDiffTab(1, "Builder"), () => false, "agent");
    layout = addTab(layout, first, { nodeId: findTab(layout, "agent")!.group.id, side: "right" });
    layout = addTab(layout, other, { nodeId: findTab(layout, "agent")!.group.id, side: "center" });
    const second = fileTab("worktree:1", "b.rs", null);
    layout = openOrFocus(layout, second, () => false, "term");
    expect(findTab(layout, second.id)?.group.id).toBe(findTab(layout, first.id)?.group.id);
  });

  it("still docks an agent's diffs beside the pane they were opened from", () => {
    const layout = openOrFocus(singleGroupLayout([agent]), agentDiffTab(1, "Builder"), () => false, "agent");
    expect(allGroups(layout)).toHaveLength(2);
  });

  it("gathers files in the group that already holds one", () => {
    const first = fileTab("worktree:1", "a.rs", null);
    let layout = openOrFocus(singleGroupLayout([agent]), first, () => false, "agent");
    const second = fileTab("worktree:1", "b.rs", null);
    layout = openOrFocus(layout, second, (t) => showsFile(t, "worktree:1", "b.rs"), "agent");
    expect(allGroups(layout)).toHaveLength(1);
    expect(findTab(layout, second.id)?.group.tabs.map((t) => t.id)).toEqual(["agent", first.id, second.id]);
  });

  it("brings an open file forward and hands it the new line", () => {
    const first = fileTab("worktree:1", "a.rs", 1);
    let layout = openOrFocus(singleGroupLayout([agent]), first, () => false, "agent");
    layout = openOrFocus(layout, fileTab("worktree:1", "b.rs", null), () => false, "agent");
    const again = fileTab("worktree:1", "a.rs", 40);
    layout = openOrFocus(layout, again, (t) => showsFile(t, "worktree:1", "a.rs"), "agent");
    const found = findTab(layout, first.id);
    expect(found?.group.active).toBe(first.id);
    expect(filePaneConfig(found!.tab)?.line).toBe(40);
    expect(findTab(layout, again.id)).toBeNull();
  });

  it("docks beside the last existing group when no origin tab is given", () => {
    let layout = openOrFocus(singleGroupLayout([agent]), fileTab("worktree:1", "a.rs", null), () => false, "agent");
    const before = allGroups(layout).length;
    layout = openOrFocus(layout, agentDiffTab(1, "Builder"), () => false, null);
    expect(allGroups(layout)).toHaveLength(before + 1);
  });

  it("brings an already-open agent-diff pane forward instead of opening a second one", () => {
    const first = agentDiffTab(1, "Builder");
    let layout = openOrFocus(singleGroupLayout([agent]), first, () => false, "agent");
    const groupsAfterFirst = allGroups(layout).length;
    const again = agentDiffTab(1, "Builder");
    layout = openOrFocus(layout, again, (t) => agentDiffOf(t) === 1, "agent");
    expect(allGroups(layout)).toHaveLength(groupsAfterFirst);
    expect(findTab(layout, first.id)?.group.active).toBe(first.id);
    expect(findTab(layout, again.id)).toBeNull();
  });

  it("never gathers a second agent's diff into an existing diff group, docking beside fromTabId instead", () => {
    // Unlike files, agent-diff tabs have no "same kind" pooling: `openOrFocus`
    // only pools `FILE_PANE` tabs (`sameKind` in `paneKinds.ts`), so each
    // agent's diffs get their own group, opened beside whatever tab it came from.
    const firstDiff = agentDiffTab(1, "Builder");
    let layout = openOrFocus(singleGroupLayout([agent]), firstDiff, () => false, "agent");
    const groupsAfterFirst = allGroups(layout).length;
    const firstGroupId = findTab(layout, firstDiff.id)?.group.id;

    const secondDiff = agentDiffTab(2, "Reviewer");
    layout = openOrFocus(layout, secondDiff, () => false, firstDiff.id);

    expect(allGroups(layout)).toHaveLength(groupsAfterFirst + 1);
    const secondEntry = findTab(layout, secondDiff.id);
    // Docked beside the tab it was opened from, not merged into its group.
    expect(secondEntry?.group.id).not.toBe(firstGroupId);
    expect(secondEntry?.group.tabs.map((t) => t.id)).toEqual([secondDiff.id]);
  });
});

describe("the Files pane (W9, W16)", () => {
  const terminal: PaneTab = { id: "t", kind: "terminal", title: "Terminal" };

  it("starts with the project's top folder open and dotfiles hidden", () => {
    const tab = filesTab(7);
    expect(tab.kind).toBe(FILES_PANE);
    expect(filesPaneConfig(tab)).toEqual({ expanded: [`${projectRoot(7)}\0`], showHidden: false });
  });

  it("reads a malformed stored config as closed and not hidden", () => {
    const config = { expanded: [1, "a\0"], showHidden: "yes" };
    const tab: PaneTab = { id: "f", kind: FILES_PANE, title: "Files", config };
    expect(filesPaneConfig(tab)).toEqual({ expanded: ["a\0"], showHidden: false });
    expect(filesPaneConfig({ id: "g", kind: FILES_PANE, title: "Files" })).toEqual({ expanded: [], showHidden: false });
  });

  it("puts a new project's Files pane down the left side, narrow", () => {
    const layout = withFilesPane(singleGroupLayout([terminal]), 3);
    const root = layout.root;
    expect(isSplit(root)).toBe(true);
    if (!isSplit(root)) return;
    expect(root.dir).toBe("row");
    expect(root.sizes[0]).toBeCloseTo(FILES_PANE_FRACTION);
    expect(allTabs(layout).map((t) => t.kind)).toEqual([FILES_PANE, "terminal"]);
  });

  it("opens a file from the tree as a tab beside the terminal, then gathers files there", () => {
    let layout = withFilesPane(singleGroupLayout([terminal]), 3);
    const files = allTabs(layout)[0];
    const a = fileTab("project:3", "a.rs", null);
    layout = openOrFocus(layout, a, (t) => showsFile(t, "project:3", "a.rs"), files.id);
    expect(allTabs(layout).map((t) => t.kind)).toEqual([FILES_PANE, "terminal", "file"]);
    // A tab in the terminal's group, not a split, and not in the tree's narrow column.
    expect(allGroups(layout)).toHaveLength(2);
    expect(findTab(layout, a.id)?.group.id).toBe(findTab(layout, terminal.id)?.group.id);
    const root = layout.root;
    expect(isSplit(root) && root.sizes[0]).toBeCloseTo(FILES_PANE_FRACTION);
    const b = fileTab("project:3", "b.rs", null);
    layout = openOrFocus(layout, b, (t) => showsFile(t, "project:3", "b.rs"), files.id);
    expect(findTab(layout, b.id)?.group.id).toBe(findTab(layout, a.id)?.group.id);
  });

  it("names the file in the file group's front tab, and nothing when that tab is not a file", () => {
    let layout = withFilesPane(singleGroupLayout([terminal]), 3);
    expect(frontFile(layout)).toBeNull();
    const a = fileTab("project:3", "a.rs", null);
    layout = openOrFocus(layout, a, (t) => showsFile(t, "project:3", "a.rs"), allTabs(layout)[0].id);
    expect(frontFile(layout)).toEqual({ root: "project:3", rel: "a.rs" });
    const b = fileTab("project:3", "b.rs", null);
    layout = openOrFocus(layout, b, (t) => showsFile(t, "project:3", "b.rs"), null);
    expect(frontFile(layout)).toEqual({ root: "project:3", rel: "b.rs" });
    const terminal2: PaneTab = { id: "t2", kind: "terminal", title: "Terminal" };
    const group = findTab(layout, a.id)?.group.id ?? "";
    layout = addTab(layout, terminal2, { nodeId: group, side: "center" });
    // The file group's front tab is now a terminal: no file to highlight.
    expect(frontFile(layout)).toBeNull();
  });
});
