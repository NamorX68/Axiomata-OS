import { describe, expect, it, vi } from "vitest";

import type { FileDiff } from "../core/backend";
import { loadGitDiff, type GitApi, type GitEntry, type RepoStatus } from "./gitBackend";
import { canCommit, groupEntries, markOf, pushState, splitPath } from "./gitModel";

const entry = (path: string, over: Partial<GitEntry> = {}): GitEntry => ({
  path,
  old_path: null,
  staged: null,
  unstaged: null,
  untracked: false,
  conflicted: false,
  ...over,
});

describe("the git panel's groups", () => {
  it("lists a path changed on both sides in both groups, and untracked files only among the changes", () => {
    const g = groupEntries([
      entry("a.rs", { staged: "modified", unstaged: "modified" }),
      entry("b.rs", { staged: "added" }),
      entry("c.rs", { unstaged: "deleted" }),
      entry("d.rs", { untracked: true, unstaged: "added" }),
    ]);
    expect(g.staged.map((r) => r.entry.path)).toEqual(["a.rs", "b.rs"]);
    expect(g.changes.map((r) => r.entry.path)).toEqual(["a.rs", "c.rs", "d.rs"]);
    expect(g.changes.map(markOf)).toEqual(["M", "D", "U"]);
    expect(g.staged.map(markOf)).toEqual(["M", "A"]);
  });

  it("puts conflicts first among the changes and marks them", () => {
    const g = groupEntries([entry("a", { unstaged: "modified" }), entry("z", { conflicted: true })]);
    expect(g.changes.map((r) => r.entry.path)).toEqual(["z", "a"]);
    expect(markOf(g.changes[0])).toBe("!");
  });

  it("splits a path and decides whether a commit is possible", () => {
    expect(splitPath("src/lib/x.rs")).toEqual({ name: "x.rs", dir: "src/lib" });
    expect(splitPath("x.rs")).toEqual({ name: "x.rs", dir: "" });
    const g = groupEntries([entry("a", { staged: "added" })]);
    expect(canCommit(g, "  ")).toBe(false);
    expect(canCommit(g, "msg")).toBe(true);
    expect(canCommit(groupEntries([]), "msg")).toBe(false);
  });
});

describe("loadGitDiff", () => {
  const diff = (over: Partial<FileDiff> = {}): FileDiff => ({
    path: "f",
    binary: false,
    hunks: [{ header: "@@ -1 +1 @@", lines: [{ kind: "remove", old_line: 1, new_line: null, text: "a" }] }],
    truncated: false,
    old_size: 2,
    new_size: 2,
    ...over,
  });
  const api = (d: FileDiff): GitApi =>
    ({
      diff: vi.fn(async () => d),
      blob: vi.fn(async (_r: string, _p: string, source: "head" | "index") => ({
        kind: "text" as const,
        text: source === "head" ? "from head\n" : "from index\n",
      })),
      workingText: vi.fn(async () => "from disk\n"),
    }) as unknown as GitApi;

  it("reads HEAD against the index for the staged side", async () => {
    const loaded = await loadGitDiff("project:1", { path: "f", old_path: "old" }, "staged", api(diff()));
    expect(loaded.oldText).toBe("from head\n");
    expect(loaded.newText).toBe("from index\n");
    expect(loaded.source.hunks).toHaveLength(1);
    expect(loaded.source.hunks[0].lines[0]).toMatchObject({ kind: "remove", oldLine: 1, newLine: null });
  });

  it("reads the index against the working file for the unstaged side", async () => {
    const loaded = await loadGitDiff("project:1", { path: "f", old_path: null }, "unstaged", api(diff()));
    expect(loaded.oldText).toBe("from index\n");
    expect(loaded.newText).toBe("from disk\n");
  });

  it("reads no text for a binary or a too-large file", async () => {
    const binary = await loadGitDiff("p", { path: "f", old_path: null }, "unstaged", api(diff({ binary: true, hunks: [] })));
    expect([binary.oldText, binary.newText]).toEqual([null, null]);
    const big = await loadGitDiff("p", { path: "f", old_path: null }, "unstaged", api(diff({ truncated: true, hunks: [] })));
    expect(big.source.oldLines).toBeNull();
  });
});

describe("the Push button", () => {
  const status = (over: Partial<RepoStatus> = {}): RepoStatus => ({
    branch: "main",
    head: "abc12345",
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    entries: [],
    ...over,
  });

  it("offers Push with the number of waiting commits, and Publish for a branch that follows nothing", () => {
    expect(pushState(status({ ahead: 2 }))).toMatchObject({ enabled: true, label: "Push ↑2" });
    expect(pushState(status({ ahead: 1 })).title).toContain("1 commit to origin/main");
    expect(pushState(status({ upstream: null }))).toMatchObject({ enabled: true, label: "Publish branch" });
  });

  it("is off when there is nothing to push, nothing committed, or no branch", () => {
    expect(pushState(status()).enabled).toBe(false);
    expect(pushState(status({ head: null })).enabled).toBe(false);
    expect(pushState(status({ branch: null, ahead: 3 })).enabled).toBe(false);
  });
});
