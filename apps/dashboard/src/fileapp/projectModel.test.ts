import { describe, expect, it } from "vitest";

import type { IdeProject } from "../core/backend";
import { folderNameProblem, otherProjects, projectRootId, resolveProject, treeRootsOf } from "./projectModel";

const project = (id: number): IdeProject => ({
  id,
  name: `p${id}`,
  repo_root: `/code/p${id}`,
  layout_json: null,
  created_at: "2026-09-01T00:00:00Z",
  last_opened_at: null,
  root_exists: true,
});

describe("the editor's project", () => {
  it("names the project's file root", () => {
    expect(projectRootId(7)).toBe("project:7");
  });

  it("shows only the open project's folder in the tree — nothing when none is open", () => {
    const roots = [{ id: "workspace" }, { id: "project:1" }, { id: "project:2" }, { id: "grant:3" }];
    expect(treeRootsOf(roots, 2)).toEqual([{ id: "project:2" }]);
    expect(treeRootsOf(roots, null)).toEqual([]);
    expect(treeRootsOf(roots, 9)).toEqual([]);
  });

  it("forgets a remembered project that left the registry", () => {
    const all = [project(1), project(2)];
    expect(resolveProject(all, 2)?.name).toBe("p2");
    expect(resolveProject(all, 5)).toBeNull();
    expect(resolveProject(all, null)).toBeNull();
  });

  it("lists the other projects, capped", () => {
    const all = [1, 2, 3, 4].map(project);
    expect(otherProjects(all, 2).map((p) => p.id)).toEqual([1, 3, 4]);
    expect(otherProjects(all, null, 2).map((p) => p.id)).toEqual([1, 2]);
  });

  it("explains a folder name that would be refused, and accepts a plain one", () => {
    expect(folderNameProblem("  ")).toMatch(/Enter a folder name/);
    expect(folderNameProblem(".hidden")).toMatch(/dot/);
    expect(folderNameProblem("a/b")).toMatch(/must not contain/);
    expect(folderNameProblem("a:b")).toMatch(/must not contain/);
    expect(folderNameProblem("x".repeat(101))).toMatch(/too long/);
    expect(folderNameProblem(" my-app ")).toBeNull();
  });
});
