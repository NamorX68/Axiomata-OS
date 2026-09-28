import { describe, expect, it } from "vitest";

import { NO_TABS, parseTabs, type FileRef, type TabsState } from "./tabs";

function ids() {
  let n = 0;
  return () => `t${++n}`;
}

const a: FileRef = { root: "workspace", rel: "a.md" };

/** Each tab as `root:rel` (`*` for the preview tab) or `new`. */
const files = (s: TabsState) =>
  s.tabs.map((t) => (t.file ? `${t.file.root}:${t.file.rel}${t.preview ? "*" : ""}` : "new"));

describe("parseTabs — tabs saved before groups existed (W12, read once by loadDock)", () => {
  it("reads files, the preview flag, a new note and the active tab", () => {
    const back = parseTabs({ tabs: [{ ...a, preview: true }, { newNote: true }], active: 1 }, ids());
    expect(files(back)).toEqual(["workspace:a.md*", "new"]);
    expect(back.active).toBe(back.tabs[1].id);
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
