import { describe, expect, it } from "vitest";

import type { FileDiff } from "../core/backend";
import { isImagePath, sameFileDiff, toHunks, worktreeRoot } from "./git";

function diff(text = "b"): FileDiff {
  return {
    path: "a.rs",
    binary: false,
    truncated: false,
    old_size: 2,
    new_size: 2,
    hunks: [
      {
        header: "@@ -1 +1 @@",
        lines: [
          { kind: "remove", old_line: 1, new_line: null, text: "a" },
          { kind: "add", old_line: null, new_line: 1, text },
        ],
      },
    ],
  };
}

describe("ide/git", () => {
  it("tells an unchanged diff from one with a changed line", () => {
    expect(sameFileDiff(diff(), diff())).toBe(true);
    expect(sameFileDiff(diff(), diff("c"))).toBe(false);
    expect(sameFileDiff(diff(), { ...diff(), new_size: 3 })).toBe(false);
    expect(sameFileDiff(diff(), { ...diff(), hunks: [] })).toBe(false);
  });

  it("maps git's wire shape onto the engine's hunks", () => {
    expect(toHunks(diff())[0].lines[1]).toEqual({ kind: "add", oldLine: null, newLine: 1, text: "b" });
  });

  it("names the worktree root and recognises pictures", () => {
    expect(worktreeRoot(7)).toBe("worktree:7");
    expect(isImagePath("assets/Logo.PNG")).toBe(true);
    expect(isImagePath("icon.svg")).toBe(false);
  });
});
