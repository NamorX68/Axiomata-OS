import { beforeEach, describe, expect, it, vi } from "vitest";

import { getSetting, setSetting } from "../core/persist";
import { allGroups, isSplit } from "../ide/layout";
import {
  activeTab,
  closeTab,
  cycleTab,
  emptyDock,
  focusedGroup,
  focusTab,
  loadDock,
  moveTabTo,
  nthTab,
  openInDock,
  parseDock,
  pinTab,
  retargetTab,
  saveDock,
  serializeDock,
  splitActive,
  tabsOf,
  type FileDock,
  type FileRef,
} from "./fileDock";

vi.mock("../core/persist", () => ({ getSetting: vi.fn(), setSetting: vi.fn() }));

const getSettingMock = vi.mocked(getSetting);
const setSettingMock = vi.mocked(setSetting);

function ids() {
  let n = 0;
  return () => `t${++n}`;
}

const a: FileRef = { root: "workspace", rel: "a.md" };
const b: FileRef = { root: "workspace", rel: "b.md" };
const c: FileRef = { root: "project:1", rel: "a.md" };

/** Opens each `[file, preview]` in turn. */
function opened(steps: Array<[FileRef | null, boolean]>, newId = ids(), start = emptyDock()): FileDock {
  let dock = start;
  for (const [file, preview] of steps) dock = openInDock(dock, file, { preview }, newId).dock;
  return dock;
}

/** Each group's tabs as `root:rel` (`*` for a preview tab) or `new`, group by group. */
const groups = (d: FileDock) =>
  allGroups(d.layout).map((g) =>
    tabsOf({ layout: { root: g }, focus: null }).map((t) =>
      t.file ? `${t.file.root}:${t.file.rel}${t.preview ? "*" : ""}` : "new",
    ),
  );

beforeEach(() => {
  getSettingMock.mockReset();
  setSettingMock.mockReset();
});

describe("opening (W7, LK2)", () => {
  it("gives each file one tab, after the visible one, and brings an open one forward", () => {
    const newId = ids();
    let d = opened([[a, false], [b, false]], newId);
    expect(groups(d)).toEqual([["workspace:a.md", "workspace:b.md"]]);
    const again = openInDock(d, a, { preview: false }, newId);
    expect(again).toMatchObject({ target: "t1", load: false });
    expect(activeTab(again.dock)?.id).toBe("t1");
    d = openInDock(again.dock, c, { preview: false }, newId).dock;
    expect(groups(d)).toEqual([["workspace:a.md", "project:1:a.md", "workspace:b.md"]]);
  });

  it("reuses the focused group's preview tab for looking, and fixes it when opened for good", () => {
    const newId = ids();
    let d = opened([[a, true]], newId);
    const next = openInDock(d, b, { preview: true }, newId);
    expect(next).toMatchObject({ target: "t1", load: true });
    expect(groups(next.dock)).toEqual([["workspace:b.md*"]]);
    d = openInDock(next.dock, b, { preview: false }, newId).dock;
    expect(groups(d)).toEqual([["workspace:b.md"]]);
  });

  it("keeps a new note to one tab, never a preview", () => {
    expect(groups(opened([[null, true], [null, false]]))).toEqual([["new"]]);
  });

  it("brings forward a file open in another group — never a second tab — and focuses that group", () => {
    const newId = ids();
    let d = opened([[a, false], [b, false]], newId);
    d = splitActive(d, "right");
    expect(groups(d)).toEqual([["workspace:a.md"], ["workspace:b.md"]]);
    // The new group has the focus; `a` lives in the other one.
    const again = openInDock(d, a, { preview: false }, newId);
    expect(again).toMatchObject({ target: "t1", load: false });
    expect(groups(again.dock)).toEqual([["workspace:a.md"], ["workspace:b.md"]]);
    expect(focusedGroup(again.dock).id).toBe(allGroups(again.dock.layout)[0].id);
    // A new file opens where the focus is.
    const withC = openInDock(again.dock, c, { preview: false }, newId).dock;
    expect(groups(withC)).toEqual([["workspace:a.md", "project:1:a.md"], ["workspace:b.md"]]);
  });
});

describe("pinning, closing, retargeting", () => {
  it("fixes a preview tab once it is edited; retargets a tab to a new file", () => {
    const d = opened([[a, true]]);
    expect(groups(pinTab(d, "t1"))).toEqual([["workspace:a.md"]]);
    expect(pinTab(d, "nope")).toBe(d);
    expect(groups(retargetTab(pinTab(d, "t1"), "t1", b))).toEqual([["workspace:b.md"]]);
  });

  it("moves to the right neighbour on closing, and an emptied group goes with its focus", () => {
    let d = opened([[a, false], [b, false], [c, false]]);
    d = focusTab(d, "t2");
    d = closeTab(d, "t2");
    expect(activeTab(d)?.id).toBe("t3");
    d = splitActive(d, "right");
    expect(groups(d)).toEqual([["workspace:a.md"], ["project:1:a.md"]]);
    d = closeTab(d, "t3");
    expect(groups(d)).toEqual([["workspace:a.md"]]);
    expect(activeTab(d)?.id).toBe("t1");
    d = closeTab(d, "t1");
    expect(activeTab(d)).toBeNull();
  });
});

describe("the focused group's keys", () => {
  it("cycles and jumps within the focused group only", () => {
    let d = opened([[a, false], [b, false], [c, false]]);
    d = focusTab(d, "t3");
    d = splitActive(d, "bottom");
    d = focusTab(d, "t1");
    expect(activeTab(cycleTab(d, 1))?.id).toBe("t2");
    expect(activeTab(cycleTab(d, -1))?.id).toBe("t2");
    expect(activeTab(nthTab(d, 9))?.id).toBe("t2");
    expect(activeTab(nthTab(d, 5))?.id).toBe("t1");
  });
});

describe("splitting and moving", () => {
  it("splits the visible tab off to the right or below; a lone tab stays", () => {
    const one = opened([[a, false]]);
    expect(splitActive(one, "right")).toBe(one);
    const d = splitActive(opened([[a, false], [b, false]]), "bottom");
    const root = d.layout.root;
    expect(isSplit(root) && root.dir).toBe("col");
    expect(activeTab(d)?.file).toEqual(b);
  });

  it("moves a dragged tab into another group's middle or against an edge", () => {
    let d = splitActive(opened([[a, false], [b, false], [c, false]]), "right");
    expect(groups(d)).toEqual([["workspace:a.md", "workspace:b.md"], ["project:1:a.md"]]);
    const [left, right] = allGroups(d.layout);
    d = moveTabTo(d, "t1", { nodeId: right.id, side: "center", index: 0 });
    expect(groups(d)).toEqual([["workspace:b.md"], ["workspace:a.md", "project:1:a.md"]]);
    expect(activeTab(d)?.id).toBe("t1");
    d = moveTabTo(d, "t2", { nodeId: d.layout.root.id, side: "top" });
    expect(groups(d)).toEqual([["workspace:b.md"], ["workspace:a.md", "project:1:a.md"]]);
    expect(isSplit(d.layout.root) && d.layout.root.dir).toBe("col");
    void left;
  });
});

describe("keeping across restarts (W12)", () => {
  it("comes back with the same groups, fresh ids and the focus", () => {
    let d = opened([[a, false], [b, false], [null, false]]);
    d = splitActive(d, "right");
    const saved = serializeDock(d);
    const back = parseDock(JSON.parse(JSON.stringify(saved)), () => `n${Math.random()}`);
    expect(back && groups(back)).toEqual([["workspace:a.md", "workspace:b.md"], ["new"]]);
    expect(back && focusedGroup(back).id).toBe(back && allGroups(back.layout)[1].id);
    expect(back && tabsOf(back).every((t) => t.id.startsWith("n"))).toBe(true);
  });

  it("leaves out a language server's files, a second tab on one file and foreign tabs", () => {
    let d = opened([[a, false], [{ root: "lsp:3", rel: "/opt/std.rs" }, false]]);
    expect(groups(parseDock(serializeDock(d), ids())!)).toEqual([["workspace:a.md"]]);
    d = opened([[a, false], [b, false]]);
    const saved = serializeDock(d) as { layout: { root: { tabs: { config: unknown; kind: string }[] } } };
    saved.layout.root.tabs[1].config = { file: a, preview: false };
    expect(groups(parseDock(saved, ids())!)).toEqual([["workspace:a.md"]]);
    saved.layout.root.tabs[1].kind = "terminal";
    expect(groups(parseDock(saved, ids())!)).toEqual([["workspace:a.md"]]);
    expect(parseDock(null, ids())).toBeNull();
  });

  it("loads the saved dock, or the tabs saved before groups existed as one group", () => {
    getSettingMock.mockReturnValue({
      tabs: { tabs: [{ root: "workspace", rel: "a.md", preview: false }, { newNote: true }], active: 1 },
    });
    const legacy = loadDock(ids());
    expect(groups(legacy)).toEqual([["workspace:a.md", "new"]]);
    expect(activeTab(legacy)?.file).toBeNull();

    getSettingMock.mockReturnValue({ fontSize: 14, tabs: { tabs: [], active: 0 } });
    saveDock(legacy);
    const written = setSettingMock.mock.calls[0][1] as Record<string, unknown>;
    expect(written.fontSize).toBe(14);
    expect(written.tabs).toBeUndefined();
    getSettingMock.mockReturnValue(written);
    expect(groups(loadDock(ids()))).toEqual([["workspace:a.md", "new"]]);
  });
});
