import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import { invokeAction, manifest } from "./registry";
import {
  bringToFront,
  closeStaged,
  openFilePanel,
  openNewNote,
  openStaged,
  readAnchor,
  requestClose,
  setCloseGuard,
  staged,
} from "./staging";

beforeAll(() => registerBuiltins());
beforeEach(() => staged.set([]));

describe("openStaged", () => {
  it("rejects a non-stageable module type", () => {
    expect(openStaged("dummy", {})).toBeNull();
    expect(get(staged)).toHaveLength(0);
  });

  it("opening the same path twice does not duplicate the panel", () => {
    const first = openStaged("file", { path: "a.md" })!;
    const second = openStaged("file", { path: "a.md" })!;
    expect(second.id).toBe(first.id);
    expect(get(staged)).toHaveLength(1);
  });

  it("opening a different path adds a second panel alongside the first", () => {
    const first = openStaged("file", { path: "a.md" })!;
    const second = openStaged("file", { path: "b.md" })!;
    const list = get(staged);
    expect(list).toHaveLength(2);
    expect(list.map((p) => p.id)).toEqual([first.id, second.id]);
  });

  it("re-opening an already-open (non-last) path raises it to the end instead of duplicating it", () => {
    const first = openStaged("file", { path: "a.md" })!;
    openStaged("file", { path: "b.md" });
    const again = openStaged("file", { path: "a.md" })!;
    expect(again.id).toBe(first.id);
    const list = get(staged);
    expect(list).toHaveLength(2);
    expect(list[list.length - 1].id).toBe(first.id);
  });

  it("closeStaged removes only the matching panel", () => {
    const a = openStaged("file", { path: "a.md" })!;
    openStaged("file", { path: "b.md" });
    closeStaged(a.id);
    const list = get(staged);
    expect(list).toHaveLength(1);
    expect(list[0].config.path).toBe("b.md");
  });

  it("closeStaged on an id that isn't staged is a no-op", () => {
    openStaged("file", { path: "a.md" });
    closeStaged("ghost");
    expect(get(staged)).toHaveLength(1);
  });
});

describe("bringToFront", () => {
  it("moves the panel to the end of the stack", () => {
    const a = openStaged("file", { path: "a.md" })!;
    const b = openStaged("file", { path: "b.md" })!;
    const c = openStaged("file", { path: "c.md" })!;
    bringToFront(a.id);
    expect(get(staged).map((p) => p.id)).toEqual([b.id, c.id, a.id]);
  });

  it("is a no-op for an id that isn't staged", () => {
    openStaged("file", { path: "a.md" });
    const before = get(staged);
    bringToFront("ghost");
    expect(get(staged)).toEqual(before);
  });

  it("is a no-op when the panel is already at the front", () => {
    const a = openStaged("file", { path: "a.md" })!;
    const b = openStaged("file", { path: "b.md" })!;
    bringToFront(b.id);
    expect(get(staged).map((p) => p.id)).toEqual([a.id, b.id]);
  });
});

describe("readAnchor", () => {
  it("accepts a complete rect", () => {
    expect(readAnchor({ x: 10, y: 20, w: 300, h: 200 })).toEqual({ x: 10, y: 20, w: 300, h: 200 });
  });

  it("refuses anything incomplete or not a rect, so a panel falls back to centred", () => {
    for (const bad of [null, undefined, 42, "rect", {}, { x: 1, y: 2, w: 3 }, { x: 1, y: 2, w: 3, h: "4" }]) {
      expect(readAnchor(bad)).toBeNull();
    }
  });

  it("refuses NaN and Infinity, which would position a panel nowhere", () => {
    expect(readAnchor({ x: NaN, y: 0, w: 1, h: 1 })).toBeNull();
    expect(readAnchor({ x: 0, y: Infinity, w: 1, h: 1 })).toBeNull();
  });
});

describe("the file panel's openers and close guards (editor plan ED4, W10, W11)", () => {
  it("opens a workspace file, and a new note once", () => {
    openFilePanel("notes/a.md");
    openFilePanel("notes/a.md", "edit");
    openNewNote();
    openNewNote();
    expect(get(staged).map((p) => p.config)).toEqual([
      { path: "notes/a.md", mode: "read" },
      { path: "", mode: "edit", isNew: true },
    ]);
  });

  it("keeps the same path under another root apart", () => {
    openStaged("file", { path: "a.md" });
    openStaged("file", { path: "a.md", root: "project:1" });
    expect(get(staged)).toHaveLength(2);
  });

  it("closes only when the guard agrees", async () => {
    const panel = openFilePanel("a.md")!;
    let answer = false;
    setCloseGuard(panel.id, async () => answer);
    await requestClose(panel.id);
    expect(get(staged)).toHaveLength(1);
    answer = true;
    await requestClose(panel.id);
    expect(get(staged)).toHaveLength(0);
  });

  it("forgets a closed panel's guard", async () => {
    const panel = openFilePanel("a.md")!;
    setCloseGuard(panel.id, async () => false);
    closeStaged(panel.id);
    const again = openFilePanel("a.md")!;
    await requestClose(again.id);
    expect(get(staged)).toHaveLength(0);
  });
});

describe("the shell's openFile action (W2)", () => {
  it("is in the manifest and opens the file panel", async () => {
    expect(manifest()[0]).toMatchObject({ instance_id: "shell" });
    expect(manifest()[0].actions.map((a) => a.name)).toContain("openFile");
    await invokeAction("shell", "openFile", { path: "Mail/x.md" });
    expect(get(staged)).toMatchObject([{ type: "file", config: { path: "Mail/x.md", mode: "read" } }]);
    await expect(invokeAction("shell", "openFile", { path: " " })).rejects.toThrow(/needs a path/);
    await expect(invokeAction("shell", "nope", {})).rejects.toThrow(/no action/);
  });
});
