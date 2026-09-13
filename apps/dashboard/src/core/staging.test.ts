import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import { closeStaged, openStaged, staged } from "./staging";

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

  it("opening a different path replaces the panel, not stacks it", () => {
    const first = openStaged("md-file", { path: "a.md" })!;
    const second = openStaged("md-file", { path: "b.md" })!;
    const list = get(staged);
    expect(list).toHaveLength(1);
    expect(list[0].id).toBe(second.id);
    expect(list[0].id).not.toBe(first.id);
  });

  it("closeStaged removes the panel", () => {
    const a = openStaged("md-file", { path: "a.md" })!;
    closeStaged(a.id);
    expect(get(staged)).toHaveLength(0);
  });

  it("closeStaged on an id that isn't staged is a no-op", () => {
    openStaged("md-file", { path: "a.md" });
    closeStaged("ghost");
    expect(get(staged)).toHaveLength(1);
  });
});
