import { describe, expect, it } from "vitest";

import { filesOf, linesOf, parseBreakpoints, renamed, toggled } from "./breakpoints";

describe("breakpoints", () => {
  it("toggles a line on and off, keeps the lines sorted, and drops empty files and roots", () => {
    let map = toggled({}, "project:1", "a.py", 9);
    map = toggled(map, "project:1", "a.py", 3);
    expect(map["project:1"]["a.py"]).toEqual([3, 9]);
    map = toggled(map, "project:1", "a.py", 3);
    map = toggled(map, "project:1", "a.py", 9);
    expect(map).toEqual({});
  });

  it("answers per file and per root", () => {
    const map = toggled(toggled({}, "project:1", "a.py", 2), "project:1", "src/b.py", 5);
    expect([...linesOf(map, "project:1", "a.py")]).toEqual([2]);
    expect(linesOf(map, "project:2", "a.py").size).toBe(0);
    expect(filesOf(map, "project:1")).toEqual([
      { rel: "a.py", lines: [2] },
      { rel: "src/b.py", lines: [5] },
    ]);
  });

  it("reads back what was saved and drops what is malformed", () => {
    expect(parseBreakpoints({ "project:1": { "a.py": [3, 3, 1, -2, 0, 1.5, "x"], "b.py": "no", "c.py": [] }, "project:2": 7 })).toEqual({
      "project:1": { "a.py": [1, 3] },
    });
    expect(parseBreakpoints(null)).toEqual({});
  });

  it("follows a renamed file or folder", () => {
    const map = toggled(toggled({}, "project:1", "src/a.py", 4), "project:1", "other.py", 1);
    expect(renamed(map, "project:1", "src", "lib")["project:1"]).toEqual({ "lib/a.py": [4], "other.py": [1] });
    expect(renamed(map, "project:1", "nothing", "x")).toBe(map);
  });
});
