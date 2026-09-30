import { beforeEach, describe, expect, it, vi } from "vitest";

import { getSetting, setSetting } from "../core/persist";
import {
  baseName,
  clampWidth,
  DEFAULT_TREE,
  expandedAfterDelete,
  expandedAfterRename,
  folderKey,
  isHidden,
  isUnder,
  joinRel,
  loadTreePrefs,
  moveTarget,
  nameProblem,
  parentOf,
  parseTreePrefs,
  renamedPath,
  saveTreePrefs,
} from "./treeModel";

vi.mock("../core/persist", () => ({ getSetting: vi.fn(), setSetting: vi.fn() }));

const getSettingMock = vi.mocked(getSetting);
const setSettingMock = vi.mocked(setSetting);

describe("treeModel (W6, W13)", () => {
  it("hides dotfiles and the usual build folders by default", () => {
    expect(isHidden(".git")).toBe(true);
    expect(isHidden(".env")).toBe(true);
    expect(isHidden("node_modules")).toBe(true);
    expect(isHidden("target")).toBe(true);
    expect(isHidden("src")).toBe(false);
  });

  it("puts paths together and apart", () => {
    expect(joinRel("", "a.md")).toBe("a.md");
    expect(joinRel("notes", "a.md")).toBe("notes/a.md");
    expect(parentOf("notes/sub/a.md")).toBe("notes/sub");
    expect(parentOf("a.md")).toBe("");
    expect(baseName("notes/a.md")).toBe("a.md");
  });

  it("refuses names the file system or the tree cannot take", () => {
    expect(nameProblem("ok.md")).toBeNull();
    expect(nameProblem("v2.0 notes.md")).toBeNull();
    expect(nameProblem("a\u0000b")).not.toBeNull();
    expect(nameProblem("  ")).not.toBeNull();
    expect(nameProblem("..")).not.toBeNull();
    expect(nameProblem("a/b")).not.toBeNull();
  });

  it("finds where an open file went after a rename — the file itself or anything in a renamed folder", () => {
    expect(renamedPath("a.md", "a.md", "b.md")).toBe("b.md");
    expect(renamedPath("notes/x/a.md", "notes", "archive")).toBe("archive/x/a.md");
    expect(renamedPath("notes-old/a.md", "notes", "archive")).toBeNull();
    expect(isUnder("notes/a.md", "notes")).toBe(true);
    expect(isUnder("notesX/a.md", "notes")).toBe(false);
  });

  it("reads saved tree settings, clamping and defaulting what is off", () => {
    expect(parseTreePrefs(null)).toEqual(DEFAULT_TREE);
    expect(parseTreePrefs({ expanded: ["w\0a", 3], width: 5000, visible: false, showHidden: "yes", project: 4, outlineOpen: false, outlineHeight: 1 })).toEqual({
      expanded: ["w\0a"],
      width: 640,
      visible: false,
      showHidden: false,
      project: 4,
      outlineOpen: false,
      outlineHeight: 80,
    });
    expect(parseTreePrefs({ project: "4" }).project).toBeNull();
    expect(clampWidth(10)).toBe(160);
  });
});

describe("loadTreePrefs and saveTreePrefs (settings.editor.tree)", () => {
  beforeEach(() => {
    getSettingMock.mockReset();
    setSettingMock.mockReset();
  });

  it("defaults when settings.editor has nothing saved yet", () => {
    getSettingMock.mockReturnValue(undefined);
    expect(loadTreePrefs()).toEqual(DEFAULT_TREE);
    expect(getSettingMock).toHaveBeenCalledWith("editor");
  });

  it("reads what was saved under settings.editor.tree", () => {
    getSettingMock.mockReturnValue({
      tree: { expanded: ["workspace\u0000notes"], width: 300, visible: false, showHidden: true, project: 3 },
    });
    expect(loadTreePrefs()).toEqual({
      expanded: ["workspace\u0000notes"],
      width: 300,
      visible: false,
      showHidden: true,
      project: 3,
      outlineOpen: true,
      outlineHeight: 240,
    });
  });

  it("saves under settings.editor.tree without touching a sibling key like `tabs`", () => {
    getSettingMock.mockReturnValue({ tabs: { tabs: [], active: null } });
    saveTreePrefs({ expanded: ["w\u0000a"], width: 400, visible: true, showHidden: false, project: 2, outlineOpen: true, outlineHeight: 240 });
    expect(setSettingMock).toHaveBeenCalledTimes(1);
    const [key, value] = setSettingMock.mock.calls[0];
    expect(key).toBe("editor");
    expect(value).toMatchObject({ tabs: { tabs: [], active: null } });
    expect((value as { tree: unknown }).tree).toEqual({
      expanded: ["w\u0000a"],
      width: 400,
      visible: true,
      showHidden: false,
      project: 2,
      outlineOpen: true,
      outlineHeight: 240,
    });
  });
});

describe("open folders after a rename or delete (W13)", () => {
  const k = (root: string, rel: string) => folderKey(root, rel);

  it("keep open under the new name, the renamed folder and those in it", () => {
    const expanded = [k("w", ""), k("w", "notes"), k("w", "notes/sub"), k("w", "notes-old"), k("p", "notes")];
    expect(expandedAfterRename(expanded, "w", "notes", "archive")).toEqual([
      k("w", ""),
      k("w", "archive"),
      k("w", "archive/sub"),
      k("w", "notes-old"),
      k("p", "notes"),
    ]);
  });

  it("are forgotten with a deleted folder, only in that root", () => {
    const expanded = [k("w", "notes"), k("w", "notes/sub"), k("w", "other"), k("p", "notes")];
    expect(expandedAfterDelete(expanded, "w", "notes")).toEqual([k("w", "other"), k("p", "notes")]);
  });
});

describe("moveTarget (T11)", () => {
  const a = (rel: string) => ({ root: "project:1", rel });

  it("lands in the folder under its own name", () => {
    expect(moveTarget(a("src/a.rs"), a("lib"))).toEqual({ to: "lib/a.rs" });
    expect(moveTarget(a("src/deep"), a(""))).toEqual({ to: "deep" });
  });

  it("does nothing where it already is", () => {
    expect(moveTarget(a("src/a.rs"), a("src"))).toBeNull();
    expect(moveTarget(a("a.rs"), a(""))).toBeNull();
  });

  it("refuses another root and a folder into itself", () => {
    expect(moveTarget(a("a.rs"), { root: "workspace", rel: "" })).toHaveProperty("refused");
    expect(moveTarget(a("src"), a("src/deep"))).toHaveProperty("refused");
    expect(moveTarget(a("src"), a("src"))).toBeNull();
    // A sibling whose name starts the same is not inside it.
    expect(moveTarget(a("src"), a("src2"))).toEqual({ to: "src2/src" });
  });
});
