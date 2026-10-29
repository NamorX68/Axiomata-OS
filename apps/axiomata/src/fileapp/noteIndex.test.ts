import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../core/backend", () => ({
  invokeBackend: vi.fn(async (cmd: string) => {
    if (cmd === "file_roots") {
      return [
        { id: "workspace", label: "Second Brain", path: "/u/vault", kind: "workspace" },
        { id: "project:1", label: "Vault", path: "/u/vault/", kind: "project" },
        { id: "project:2", label: "Deep", path: "/u/vault/KI", kind: "project" },
        { id: "project:3", label: "Code", path: "/u/code", kind: "project" },
        { id: "worktree:1", label: "W", path: "/u/vault/KI", kind: "worktree" },
      ];
    }
    if (cmd === "get_workspace_graph") return { files: [{ path: "a.md", area: null, is_markdown: true }] };
    if (cmd === "get_tag_list") return [{ tag: "rust", description: "Rust" }];
    throw new Error(cmd);
  }),
  listenBackend: vi.fn(async () => () => {}),
}));

import {
  loadNoteIndex,
  loadTagList,
  peekNoteIndex,
  prefixIn,
  resetNoteIndexForTests,
  rootRel,
  vaultPrefix,
  vaultRel,
} from "./noteIndex";

afterEach(() => resetNoteIndexForTests());

describe("where a root lies in the vault", () => {
  it("prefixIn compares folders, not string beginnings", () => {
    expect(prefixIn("/u/vault", "/u/vault")).toBe("");
    expect(prefixIn("/u/vault/", "/u/vault/KI/Sub")).toBe("KI/Sub");
    expect(prefixIn("/u/vault", "/u/vault2")).toBeNull();
    expect(prefixIn("/u/vault", "/u")).toBeNull();
  });

  it("vaultPrefix knows the workspace, projects on the vault, and nothing else", async () => {
    expect(await vaultPrefix("workspace")).toBe("");
    expect(await vaultPrefix("project:1")).toBe("");
    expect(await vaultPrefix("project:2")).toBe("KI");
    expect(await vaultPrefix("project:3")).toBeNull();
    expect(await vaultPrefix("worktree:1")).toBeNull();
    expect(await vaultPrefix("project:9")).toBeNull();
  });

  it("maps paths between the vault and a root inside it", () => {
    expect(vaultRel("KI", "Modelle.md")).toBe("KI/Modelle.md");
    expect(vaultRel("", "a.md")).toBe("a.md");
    expect(rootRel("KI", "KI/Modelle.md")).toBe("Modelle.md");
    expect(rootRel("KI", "Arbeit/Plan.md")).toBeNull();
    expect(rootRel("", "Arbeit/Plan.md")).toBe("Arbeit/Plan.md");
  });
});

describe("the shared note index", () => {
  it("is read once, then answered from memory", async () => {
    expect(peekNoteIndex()).toBeNull();
    const index = await loadNoteIndex();
    expect(index?.has("a.md")).toBe(true);
    expect(peekNoteIndex()).toBe(index);
    expect(await loadNoteIndex()).toBe(index);
    expect(await loadTagList()).toEqual([{ tag: "rust", description: "Rust" }]);
  });
});

describe("a vault that cannot be read", () => {
  it("is not asked again on every draw", async () => {
    const { invokeBackend } = await import("../core/backend");
    const mocked = vi.mocked(invokeBackend);
    mocked.mockRejectedValueOnce(new Error("no workspace"));
    expect(await loadNoteIndex()).toBeNull();
    const calls = mocked.mock.calls.length;
    peekNoteIndex();
    peekNoteIndex();
    expect(mocked.mock.calls.length).toBe(calls);
  });
});
