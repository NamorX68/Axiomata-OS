import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { getSetting, setSetting } from "../core/persist";
import {
  closeTab,
  cycleTab,
  loadTabs,
  NO_TABS,
  nthTab,
  openInTabs,
  parseTabs,
  pinTab,
  retargetTab,
  saveTabs,
  serializeTabs,
  type FileRef,
  type TabsState,
} from "./tabs";

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
function opened(steps: Array<[FileRef | null, boolean]>, newId = ids()): TabsState {
  let state = NO_TABS;
  for (const [file, preview] of steps) state = openInTabs(state, file, { preview }, newId).state;
  return state;
}

/** Each tab as `root:rel` (`*` for the preview tab) or `new`. */
const files = (s: TabsState) =>
  s.tabs.map((t) => (t.file ? `${t.file.root}:${t.file.rel}${t.preview ? "*" : ""}` : "new"));

describe("openInTabs (W7)", () => {
  it("gives each file one tab, after the active one, and brings an open one forward", () => {
    const newId = ids();
    let s = opened([[a, false], [b, false]], newId);
    expect(files(s)).toEqual(["workspace:a.md", "workspace:b.md"]);
    const again = openInTabs(s, a, { preview: false }, newId);
    expect(again).toMatchObject({ target: "t1", load: false });
    expect(again.state.active).toBe("t1");
    s = openInTabs(again.state, c, { preview: false }, newId).state;
    expect(files(s)).toEqual(["workspace:a.md", "project:1:a.md", "workspace:b.md"]);
  });

  it("reuses the one preview tab for looking, and fixes it when opened for good", () => {
    const newId = ids();
    let s = opened([[a, true]], newId);
    const next = openInTabs(s, b, { preview: true }, newId);
    expect(next).toMatchObject({ target: "t1", load: true });
    expect(files(next.state)).toEqual(["workspace:b.md*"]);
    s = openInTabs(next.state, b, { preview: false }, newId).state;
    expect(files(s)).toEqual(["workspace:b.md"]);
    s = openInTabs(s, a, { preview: true }, newId).state;
    expect(files(s)).toEqual(["workspace:b.md", "workspace:a.md*"]);
  });

  it("keeps a new note to one tab, never a preview", () => {
    const s = opened([[null, true], [null, false]]);
    expect(files(s)).toEqual(["new"]);
  });

  it("inserts a new tab at the end when `active` names no tab, as when nothing is active", () => {
    const newId = ids();
    let s = opened([[a, false], [b, false]], newId);
    s = { ...s, active: "gone" };
    s = openInTabs(s, c, { preview: false }, newId).state;
    expect(files(s)).toEqual(["workspace:a.md", "workspace:b.md", "project:1:a.md"]);
  });

  it("inserts a new tab at the end when nothing is active (an empty state)", () => {
    const newId = ids();
    const s = openInTabs(NO_TABS, a, { preview: false }, newId).state;
    expect(files(s)).toEqual(["workspace:a.md"]);
  });

  it("reuses the preview tab for a look even when it is not the active one", () => {
    const newId = ids();
    let s = opened([[a, true], [b, false]], newId);
    // `a`'s preview tab ("t1") is no longer active; "t2" (`b`) is.
    expect(s.active).toBe("t2");
    const next = openInTabs(s, c, { preview: true }, newId);
    expect(next).toMatchObject({ target: "t1", load: true });
    expect(files(next.state)).toEqual(["project:1:a.md*", "workspace:b.md"]);
    expect(next.state.active).toBe("t1");
  });

  it("opens a fresh preview tab once the old preview tab was closed", () => {
    const newId = ids();
    let s = opened([[a, true], [b, false]], newId);
    s = closeTab(s, "t1");
    expect(files(s)).toEqual(["workspace:b.md"]);
    const next = openInTabs(s, c, { preview: true }, newId);
    expect(next.load).toBe(true);
    expect(files(next.state)).toEqual(["workspace:b.md", "project:1:a.md*"]);
  });
});

describe("pinTab, closeTab, retargetTab", () => {
  it("fixes a preview tab once it is edited", () => {
    const s = opened([[a, true]]);
    expect(files(pinTab(s, "t1"))).toEqual(["workspace:a.md"]);
    expect(pinTab(s, "nope")).toBe(s);
  });

  it("moves to the right neighbour on closing, else the left, else nothing", () => {
    let s = opened([[a, false], [b, false], [c, false]]);
    s = { ...s, active: "t2" };
    s = closeTab(s, "t2");
    expect(s.active).toBe("t3");
    s = closeTab(s, "t3");
    expect(s.active).toBe("t1");
    s = closeTab(s, "t1");
    expect(s).toEqual(NO_TABS);
  });

  it("keeps the active tab when another one closes", () => {
    const s = opened([[a, false], [b, false]]);
    expect(closeTab(s, "t1").active).toBe("t2");
  });

  it("points a tab at the file its editor moved to", () => {
    const s = opened([[null, false]]);
    expect(files(retargetTab(s, "t1", a))).toEqual(["workspace:a.md"]);
  });

  it("does not merge or dedupe when retargeted onto a file another tab already shows", () => {
    // Documented current behaviour, not a claimed invariant: `retargetTab` only ever
    // rewrites the one tab named by `id` and never looks at sibling tabs, so two tabs
    // can end up pointing at the same file (e.g. a followed link lands where another
    // tab already has it open).
    const s = opened([[a, false], [b, false]]);
    const retargeted = retargetTab(s, "t2", a);
    expect(files(retargeted)).toEqual(["workspace:a.md", "workspace:a.md"]);
    expect(retargeted.tabs).toHaveLength(2);
    expect(retargeted.active).toBe(s.active);
  });
});

describe("cycleTab and nthTab (W12)", () => {
  const s = { ...opened([[a, false], [b, false], [c, false]]), active: "t1" };

  it("goes round both ways", () => {
    expect(cycleTab(s, 1).active).toBe("t2");
    expect(cycleTab(s, -1).active).toBe("t3");
    expect(cycleTab(NO_TABS, 1)).toBe(NO_TABS);
  });

  it("jumps to the n-th, ⌘9 to the last", () => {
    expect(nthTab(s, 2).active).toBe("t2");
    expect(nthTab(s, 9).active).toBe("t3");
    expect(nthTab(s, 7)).toBe(s);
  });
});

describe("serializeTabs and parseTabs (W12)", () => {
  it("round-trips files, the preview flag, a new note and the active tab", () => {
    const s = { ...opened([[a, false], [null, false], [b, true]]), active: "t3" };
    const saved = serializeTabs(s);
    expect(saved.active).toBe(2);
    const back = parseTabs(JSON.parse(JSON.stringify(saved)), ids());
    expect(files(back)).toEqual(files(s));
    expect(back.tabs.findIndex((t) => t.id === back.active)).toBe(2);
  });

  it("does not keep a language server's read-only file (ED6.2)", () => {
    const foreign: FileRef = { root: "lsp:7", rel: "/opt/rust/lib/string.rs" };
    const s = { ...opened([[a, false], [foreign, false], [b, false]]), active: "t3" };
    const saved = serializeTabs(s);
    expect(saved.tabs.map((t) => ("rel" in t ? t.rel : "new"))).toEqual(["a.md", "b.md"]);
    expect(saved.active).toBe(1);
  });

  it("skips what is malformed or doubled, and falls back to the first tab", () => {
    const back = parseTabs(
      {
        tabs: [a, a, { root: 1, rel: "x" }, { root: "w", rel: "" }, null, { newNote: true }, { newNote: true }],
        active: 42,
      },
      ids(),
    );
    expect(files(back)).toEqual(["workspace:a.md", "new"]);
    expect(back.active).toBe(back.tabs[0].id);
    expect(parseTabs(null, ids())).toEqual(NO_TABS);
    expect(parseTabs({ tabs: "x" }, ids())).toEqual(NO_TABS);
  });
});

describe("loadTabs and saveTabs (W12, settings.editor.tabs)", () => {
  beforeEach(() => {
    getSettingMock.mockReset();
    setSettingMock.mockReset();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("loads NO_TABS when settings.editor has nothing saved yet", () => {
    getSettingMock.mockReturnValue(undefined);
    expect(loadTabs(ids())).toEqual(NO_TABS);
    expect(getSettingMock).toHaveBeenCalledWith("editor");
  });

  it("parses what was saved under settings.editor.tabs", () => {
    getSettingMock.mockReturnValue({ tabs: { tabs: [{ ...a, preview: false }], active: 0 } });
    const state = loadTabs(ids());
    expect(files(state)).toEqual(["workspace:a.md"]);
  });

  it("saves under settings.editor.tabs without touching a sibling key like `recent`", () => {
    getSettingMock.mockReturnValue({ recent: [{ root: "workspace", rel: "old.md" }] });
    const s = opened([[a, false], [b, true]]);
    saveTabs(s);
    expect(setSettingMock).toHaveBeenCalledTimes(1);
    const [key, value] = setSettingMock.mock.calls[0];
    expect(key).toBe("editor");
    expect(value).toMatchObject({ recent: [{ root: "workspace", rel: "old.md" }] });
    expect((value as { tabs: unknown }).tabs).toEqual(serializeTabs(s));
  });
});
