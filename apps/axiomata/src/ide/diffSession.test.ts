import { describe, expect, it, vi } from "vitest";

import type { AgentDiffState, AgentFileChange, FileDiff } from "../core/backend";
import { AgentDiffSession, type DiffBackend } from "./diffSession.svelte";
import type { LoadedDiff } from "./git";

function change(path: string, uncommitted = false, oldPath: string | null = null): AgentFileChange {
  return { path, old_path: oldPath, kind: "modified", additions: 1, deletions: 1, binary: false, uncommitted };
}

function fileDiff(path: string, text = "new"): FileDiff {
  return {
    path,
    binary: false,
    truncated: false,
    old_size: 3,
    new_size: 3,
    hunks: [
      {
        header: "@@ -1 +1 @@",
        lines: [
          { kind: "remove", old_line: 1, new_line: null, text: "old" },
          { kind: "add", old_line: null, new_line: 1, text },
        ],
      },
    ],
  };
}

function loaded(c: AgentFileChange, text = "new"): LoadedDiff {
  const diff = fileDiff(c.path, text);
  return {
    change: c,
    diff,
    source: { hunks: [], oldLines: null, newLines: null },
    oldText: null,
    newText: null,
    images: null,
  };
}

function ready(files: AgentFileChange[]): AgentDiffState {
  return { state: "ready", changes: { base: { branch: "main", commit: "abc", fallback: false }, files } };
}

/** A backend; with `manual`, file loads wait until the test releases them one by one. */
function backend(files: AgentFileChange[], manual = true) {
  const loads: { path: string; resolve: (d: LoadedDiff) => void }[] = [];
  const api: DiffBackend = {
    changes: vi.fn(async () => ready(files)),
    fileDiff: vi.fn(async (_id, path) => fileDiff(path)),
    load: vi.fn((_id, c) =>
      manual
        ? new Promise<LoadedDiff>((resolve) => {
            loads.push({ path: c.path, resolve: (d) => resolve(d) });
          })
        : Promise.resolve(loaded(c)),
    ),
    discard: vi.fn(async () => {}),
    discardHunk: vi.fn(async () => {}),
    commit: vi.fn(async () => "c0ffee"),
    takeOver: vi.fn(async () => ({ outcome: "done" as const, commit: "beef" })),
    lastSubject: vi.fn(async () => "Add words"),
  };
  return { api, loads };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("AgentDiffSession", () => {
  it("opens the first file of a fresh list", async () => {
    const a = change("a.rs");
    const { api, loads } = backend([a, change("b.rs")]);
    const s = new AgentDiffSession(1, api);
    const done = s.refresh(false);
    await flush();
    loads[0].resolve(loaded(a));
    await done;
    expect(s.selected).toBe("a.rs");
    expect(s.loaded?.change.path).toBe("a.rs");
    expect(s.model).not.toBeNull();
  });

  it("drops a load that arrives after another file was picked", async () => {
    const a = change("a.rs");
    const b = change("b.rs");
    const { api, loads } = backend([a, b]);
    const s = new AgentDiffSession(1, api);
    const first = s.select(a);
    const second = s.select(b);
    loads[1].resolve(loaded(b));
    await second;
    loads[0].resolve(loaded(a));
    await first;
    expect(s.loaded?.change.path).toBe("b.rs");
  });

  it("keeps the open diff on a poll that finds it unchanged, and reloads when forced", async () => {
    const a = change("a.rs");
    const { api, loads } = backend([a]);
    const s = new AgentDiffSession(1, api);
    const open = s.select(a);
    loads[0].resolve(loaded(a));
    await open;
    const shown = s.model;
    await s.refresh(false);
    expect(s.model).toBe(shown);
    expect(loads).toHaveLength(1);
    const forced = s.refresh(true);
    await flush();
    expect(loads).toHaveLength(2);
    loads[1].resolve(loaded(a));
    await forced;
    expect(s.model).not.toBe(shown);
  });

  it("discards a hunk by its header and reloads, even when refused", async () => {
    const a = change("a.rs", true);
    const { api } = backend([a], false);
    const s = new AgentDiffSession(1, api);
    await s.refresh(false);
    vi.mocked(api.discardHunk).mockRejectedValueOnce(new Error("a.rs changed since its diff was shown"));
    await expect(s.discardHunk(0)).rejects.toThrow("changed since");
    expect(api.discardHunk).toHaveBeenCalledWith(1, "a.rs", null, 0, "@@ -1 +1 @@");
    expect(api.changes).toHaveBeenCalled();
  });

  it("knows about uncommitted work, and commits it", async () => {
    const { api } = backend([change("a.rs", true)], false);
    const s = new AgentDiffSession(1, api);
    await s.refresh(false);
    expect(s.hasUncommitted).toBe(true);
    const committing = s.commit("wip: Builder");
    await expect(committing).resolves.toBe("c0ffee");
    expect(api.commit).toHaveBeenCalledWith(1, "wip: Builder");
  });

  it("discards a renamed file's new and old path together", async () => {
    const renamed = change("kept.txt", false, "keep.txt");
    const { api } = backend([renamed], false);
    const s = new AgentDiffSession(1, api);
    await s.refresh(false);
    await s.discardFile();
    expect(api.discard).toHaveBeenCalledWith(1, ["kept.txt", "keep.txt"]);
  });

  it("discards a non-renamed file by its own path only", async () => {
    const { api } = backend([change("a.rs")], false);
    const s = new AgentDiffSession(1, api);
    await s.refresh(false);
    await s.discardFile();
    expect(api.discard).toHaveBeenCalledWith(1, ["a.rs"]);
  });

  it("discardFile is a no-op when nothing is selected", async () => {
    const { api } = backend([], false);
    const s = new AgentDiffSession(1, api);
    await s.discardFile();
    expect(api.discard).not.toHaveBeenCalled();
  });

  it("stepFile stays within the list's bounds and skips loading when it does not move", async () => {
    const a = change("a.rs");
    const b = change("b.rs");
    const { api } = backend([a, b], false);
    const s = new AgentDiffSession(1, api);
    s.diffState = ready([a, b]);
    s.selected = "a.rs";

    s.stepFile(-1);
    expect(s.selected).toBe("a.rs");
    expect(api.load).not.toHaveBeenCalled();

    s.stepFile(1);
    expect(s.selected).toBe("b.rs");

    s.stepFile(1);
    expect(s.selected).toBe("b.rs");
    expect(api.load).toHaveBeenCalledTimes(1);
  });

  it("stepFile does nothing on an empty file list", () => {
    const { api } = backend([], false);
    const s = new AgentDiffSession(1, api);
    s.diffState = ready([]);
    s.stepFile(1);
    expect(s.selected).toBeNull();
    expect(api.load).not.toHaveBeenCalled();
  });

  it("toggleWholeFile flips the flag, tells the model, and bumps the revision", async () => {
    const a = change("a.rs");
    const { api } = backend([a], false);
    const s = new AgentDiffSession(1, api);
    await s.refresh(false);
    const model = s.model;
    expect(model).not.toBeNull();
    const setWholeFile = vi.spyOn(model!, "setWholeFile");
    const revisionBefore = s.revision;

    s.toggleWholeFile();

    expect(s.wholeFile).toBe(true);
    expect(setWholeFile).toHaveBeenCalledWith(true);
    expect(s.revision).toBe(revisionBefore + 1);

    s.toggleWholeFile();
    expect(s.wholeFile).toBe(false);
    expect(setWholeFile).toHaveBeenCalledWith(false);
  });

  it("resolves to a conflict outcome rather than throwing", async () => {
    const { api } = backend([change("a.rs", true)], false);
    vi.mocked(api.takeOver).mockResolvedValueOnce({ outcome: "conflict", files: ["a.rs"] });
    const s = new AgentDiffSession(1, api);
    await s.refresh(false);

    const result = await s.takeOver("squash", "Take it over");

    expect(result).toEqual({ outcome: "conflict", files: ["a.rs"] });
    expect(api.takeOver).toHaveBeenCalledWith(1, "squash", "Take it over");
    // A conflict still reloads the list, same as a successful take-over.
    expect(api.changes).toHaveBeenCalledTimes(2);
  });

  it("keeps the old list and reports the error when a refresh fails", async () => {
    const a = change("a.rs");
    const { api } = backend([a], false);
    const s = new AgentDiffSession(1, api);
    await s.refresh(false);
    expect(s.files).toEqual([a]);
    expect(s.listError).toBe("");

    vi.mocked(api.changes).mockRejectedValueOnce(new Error("network hiccup"));
    await s.refresh(true);

    expect(s.listError).toBe("network hiccup");
    expect(s.files).toEqual([a]);
  });

  it("drops an older refresh's list answer when a newer refresh has already landed", async () => {
    const a = change("a.rs");
    const b = change("b.rs");
    // `changes()` is held open manually here (unlike `backend()`'s helper, which
    // only lets tests control file loads) so both refreshes' list answers can
    // be released out of order.
    const releases: ((v: AgentDiffState) => void)[] = [];
    const api: DiffBackend = {
      changes: vi.fn(() => new Promise<AgentDiffState>((resolve) => releases.push(resolve))),
      fileDiff: vi.fn(async (_id, path) => fileDiff(path)),
      load: vi.fn(async (_id, c) => loaded(c)),
      discard: vi.fn(async () => {}),
      discardHunk: vi.fn(async () => {}),
      commit: vi.fn(async () => "c0ffee"),
      takeOver: vi.fn(async () => ({ outcome: "done" as const, commit: "beef" })),
      lastSubject: vi.fn(async () => "Add words"),
    };
    const s = new AgentDiffSession(1, api);

    const older = s.refresh(false);
    const newer = s.refresh(false);

    // The newer refresh's answer lands first...
    releases[1](ready([b]));
    await newer;
    expect(s.files).toEqual([b]);

    // ...then the older, overtaken one arrives late and must be dropped.
    releases[0](ready([a, b]));
    await older;
    expect(s.files).toEqual([b]);
    expect(s.listError).toBe("");
  });
});
