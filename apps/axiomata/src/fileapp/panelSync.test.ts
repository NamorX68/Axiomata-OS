import { describe, expect, it } from "vitest";

import { panelTarget } from "./panelSync";

const state = (root: string, rel: string, untitled = false) => ({ root, rel, dirty: false, untitled });

describe("panelTarget (the panel's config follows its editor)", () => {
  it("does nothing while the config already names the open file — the round trip stops here", () => {
    expect(panelTarget({ path: "a.md", mode: "read" }, state("workspace", "a.md"))).toBeNull();
    expect(panelTarget({ path: "a.md", root: "project:1" }, state("project:1", "a.md"))).toBeNull();
  });

  it("follows a filed note, leaving the workspace root unnamed", () => {
    expect(panelTarget({ path: "", isNew: true }, state("workspace", "Inbox/idea.md"))).toEqual({
      path: "Inbox/idea.md",
      root: undefined,
      isNew: false,
    });
  });

  it("follows a link to another page, and another root by name", () => {
    expect(panelTarget({ path: "l/1.html" }, state("workspace", "l/2.html"))).toMatchObject({ path: "l/2.html" });
    expect(panelTarget({ path: "a.md" }, state("grant:3", "a.md"))).toEqual({
      path: "a.md",
      root: "grant:3",
      isNew: false,
    });
  });

  it("leaves the config alone for a note not filed yet, or nothing open", () => {
    expect(panelTarget({ path: "", isNew: true }, state("new-note", "Untitled.md", true))).toBeNull();
    expect(panelTarget({ path: "a.md" }, null)).toBeNull();
  });
});
