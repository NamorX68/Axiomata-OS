import { get } from "svelte/store";
import { describe, expect, it } from "vitest";

import { dirtyFiles, isDirty, markDirty } from "./dirtyFiles";

describe("dirtyFiles (T14)", () => {
  it("keeps a file marked while any editor has unsaved changes in it", () => {
    const file = { root: "workspace", rel: "a.md" };
    markDirty("tab", file);
    markDirty("pane", file);
    markDirty("tab", null);
    expect(isDirty(get(dirtyFiles), "workspace", "a.md")).toBe(true);
    markDirty("pane", null);
    expect(isDirty(get(dirtyFiles), "workspace", "a.md")).toBe(false);
  });
});
