import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import { closeStaged, openStaged, staged } from "./staging";

beforeAll(() => registerBuiltins());
beforeEach(() => staged.set([]));

describe("openStaged", () => {
  it("rejects a non-stageable module type", () => {
    expect(openStaged("dummy", {}, "right")).toBeNull();
    expect(get(staged)).toHaveLength(0);
  });

  it("opening the same path on the same side twice does not duplicate the panel", () => {
    const first = openStaged("md-file", { path: "a.md" }, "right")!;
    const second = openStaged("md-file", { path: "a.md" }, "right")!;
    expect(second.id).toBe(first.id);
    expect(get(staged)).toHaveLength(1);
  });

  it("opening a different path on an occupied side replaces the panel, not stacks it", () => {
    const first = openStaged("md-file", { path: "a.md" }, "right")!;
    const second = openStaged("md-file", { path: "b.md" }, "right")!;
    const list = get(staged);
    expect(list).toHaveLength(1);
    expect(list[0].id).toBe(second.id);
    expect(list[0].id).not.toBe(first.id);
  });

  it("right and bottom are independent slots", () => {
    openStaged("md-file", { path: "a.md" }, "right");
    openStaged("md-file", { path: "b.md" }, "bottom");
    const list = get(staged);
    expect(list).toHaveLength(2);
    expect(list.map((p) => p.from).sort()).toEqual(["bottom", "right"]);
  });

  it("closeStaged removes exactly that panel", () => {
    const a = openStaged("md-file", { path: "a.md" }, "right")!;
    openStaged("md-file", { path: "b.md" }, "bottom");
    closeStaged(a.id);
    const list = get(staged);
    expect(list).toHaveLength(1);
    expect(list[0].from).toBe("bottom");
  });
});
