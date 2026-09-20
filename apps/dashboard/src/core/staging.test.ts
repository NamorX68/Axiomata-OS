import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import { bringToFront, closeStaged, openStaged, readAnchor, staged } from "./staging";

beforeAll(() => registerBuiltins());
beforeEach(() => staged.set([]));

describe("openStaged", () => {
  it("rejects a non-stageable module type", () => {
    expect(openStaged("dummy", {})).toBeNull();
    expect(get(staged)).toHaveLength(0);
  });

  it("opening the same path twice does not duplicate the panel", () => {
    const first = openStaged("md-file", { path: "a.md" })!;
    const second = openStaged("md-file", { path: "a.md" })!;
    expect(second.id).toBe(first.id);
    expect(get(staged)).toHaveLength(1);
  });

  it("opening a different path adds a second panel alongside the first", () => {
    const first = openStaged("md-file", { path: "a.md" })!;
    const second = openStaged("md-file", { path: "b.md" })!;
    const list = get(staged);
    expect(list).toHaveLength(2);
    expect(list.map((p) => p.id)).toEqual([first.id, second.id]);
  });

  it("re-opening an already-open (non-last) path raises it to the end instead of duplicating it", () => {
    const first = openStaged("md-file", { path: "a.md" })!;
    openStaged("md-file", { path: "b.md" });
    const again = openStaged("md-file", { path: "a.md" })!;
    expect(again.id).toBe(first.id);
    const list = get(staged);
    expect(list).toHaveLength(2);
    expect(list[list.length - 1].id).toBe(first.id);
  });

  it("closeStaged removes only the matching panel", () => {
    const a = openStaged("md-file", { path: "a.md" })!;
    openStaged("md-file", { path: "b.md" });
    closeStaged(a.id);
    const list = get(staged);
    expect(list).toHaveLength(1);
    expect(list[0].config.path).toBe("b.md");
  });

  it("closeStaged on an id that isn't staged is a no-op", () => {
    openStaged("md-file", { path: "a.md" });
    closeStaged("ghost");
    expect(get(staged)).toHaveLength(1);
  });
});

describe("bringToFront", () => {
  it("moves the panel to the end of the stack", () => {
    const a = openStaged("md-file", { path: "a.md" })!;
    const b = openStaged("md-file", { path: "b.md" })!;
    const c = openStaged("md-file", { path: "c.md" })!;
    bringToFront(a.id);
    expect(get(staged).map((p) => p.id)).toEqual([b.id, c.id, a.id]);
  });

  it("is a no-op for an id that isn't staged", () => {
    openStaged("md-file", { path: "a.md" });
    const before = get(staged);
    bringToFront("ghost");
    expect(get(staged)).toEqual(before);
  });

  it("is a no-op when the panel is already at the front", () => {
    const a = openStaged("md-file", { path: "a.md" })!;
    const b = openStaged("md-file", { path: "b.md" })!;
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
