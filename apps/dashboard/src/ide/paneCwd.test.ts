import { describe, expect, it } from "vitest";

import { findTab, singleGroupLayout, type PaneTab } from "./layout";
import { applyProjectCwd } from "./paneCwd";

function terminal(id: string, config?: Record<string, unknown>): PaneTab {
  return { id, kind: "terminal", title: "Terminal", config };
}

describe("applyProjectCwd", () => {
  it("points a terminal pane at the project's folder", () => {
    const layout = singleGroupLayout([terminal("a")]);
    const next = applyProjectCwd(layout, "/repo");
    expect(findTab(next, "a")!.tab.config).toEqual({ cwd: "/repo" });
  });

  it("overwrites a cwd left over from where the project used to point", () => {
    // The case the security audit named: "Change path" moved the project, and
    // the stored tab still claims the old folder.
    const layout = singleGroupLayout([terminal("a", { cwd: "/old/place" })]);
    expect(findTab(applyProjectCwd(layout, "/new/place"), "a")!.tab.config).toEqual({ cwd: "/new/place" });
  });

  it("keeps the rest of a pane's config", () => {
    const layout = singleGroupLayout([terminal("a", { cwd: "/old", scrollback: 5000 })]);
    expect(findTab(applyProjectCwd(layout, "/repo"), "a")!.tab.config).toEqual({
      cwd: "/repo",
      scrollback: 5000,
    });
  });

  it("leaves other pane kinds alone — an agent brings its own worktree", () => {
    const agent: PaneTab = { id: "b", kind: "agent", title: "Claude", config: { cwd: "/worktrees/x" } };
    const layout = singleGroupLayout([terminal("a"), agent]);
    const next = applyProjectCwd(layout, "/repo");
    expect(findTab(next, "a")!.tab.config).toEqual({ cwd: "/repo" });
    expect(findTab(next, "b")!.tab.config).toEqual({ cwd: "/worktrees/x" });
  });

  it("returns the same layout when nothing needed correcting", () => {
    const layout = singleGroupLayout([terminal("a", { cwd: "/repo" })]);
    expect(applyProjectCwd(layout, "/repo")).toBe(layout);
  });

  it("corrects every pane, not just the first", () => {
    const layout = singleGroupLayout([terminal("a", { cwd: "/old" }), terminal("b", { cwd: "/older" })]);
    const next = applyProjectCwd(layout, "/repo");
    expect(findTab(next, "a")!.tab.config).toEqual({ cwd: "/repo" });
    expect(findTab(next, "b")!.tab.config).toEqual({ cwd: "/repo" });
  });
});
