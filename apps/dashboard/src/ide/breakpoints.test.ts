import { describe, expect, it } from "vitest";

import { get } from "svelte/store";

import {
  applyEdit,
  breakpointInfo,
  breakpoints,
  filesOf,
  linesOf,
  parseBreakpoints,
  renamed,
  shiftBreakpoints,
  specsOf,
  toggled,
  withInfo,
} from "./breakpoints";

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

describe("shiftBreakpoints", () => {
  const at = (l: number, c: number) => ({ line: l, col: c });
  const edit = (s: [number, number], o: [number, number], n: [number, number]) => ({ start: at(...s), oldEnd: at(...o), newEnd: at(...n) });

  it("lines after an inserted line move down, lines before stay", () => {
    // Enter at the end of line 3 (one-based): a line is added below it.
    expect(shiftBreakpoints([2, 3, 5], edit([2, 10], [2, 10], [3, 0]), 8)).toEqual([2, 3, 6]);
  });

  it("Enter at the start of a line pushes that line's breakpoint down", () => {
    expect(shiftBreakpoints([4], edit([3, 0], [3, 0], [4, 0]), 8)).toEqual([5]);
  });

  it("Enter in the middle of a line leaves the breakpoint where it is", () => {
    expect(shiftBreakpoints([4], edit([3, 5], [3, 5], [4, 0]), 8)).toEqual([4]);
  });

  it("deleting whole lines drops their breakpoints and pulls the later ones up", () => {
    // Lines 3–4 (one-based) removed: (2,0)–(4,0).
    expect(shiftBreakpoints([2, 3, 4, 6], edit([2, 0], [4, 0], [2, 0]), 9)).toEqual([2, 4]);
  });

  it("joining a line with the one before removes the later line's breakpoint", () => {
    // Backspace at the start of line 4: (2,end)–(3,0) becomes nothing.
    expect(shiftBreakpoints([3, 4, 5], edit([2, 8], [3, 0], [2, 8]), 8)).toEqual([3, 4]);
  });

  it("typing inside a line changes nothing", () => {
    expect(shiftBreakpoints([2, 5], edit([1, 3], [1, 3], [1, 4]), 8)).toEqual([2, 5]);
  });

  it("replacing the whole text keeps the breakpoints that still fit", () => {
    expect(shiftBreakpoints([2, 9], edit([0, 0], [7, 4], [4, 0]), 8)).toEqual([2]);
  });
});

describe("conditions follow their breakpoints", () => {
  const info = (c: string) => ({ condition: c });

  it("a condition is kept, trimmed, and an empty one means a plain breakpoint", () => {
    const set = withInfo({}, "p", "a.py", 3, { condition: "  i == 3 ", hit: "", log: " " });
    expect(set).toEqual({ p: { "a.py": { 3: { condition: "i == 3", hit: undefined, log: undefined } } } });
    expect(withInfo(set, "p", "a.py", 3, { condition: "" })).toEqual({});
  });

  it("the debugger is told every breakpoint with its extras", () => {
    const specs = specsOf({ p: { "a.py": [2, 5] } }, { p: { "a.py": { 5: info("x > 1") } } }, "p");
    expect(specs).toEqual([
      {
        rel: "a.py",
        breakpoints: [
          { line: 2, condition: null, hit_condition: null, log_message: null },
          { line: 5, condition: "x > 1", hit_condition: null, log_message: null },
        ],
      },
    ]);
  });

  it("an edit above moves the extras along; deleting the line drops them", () => {
    breakpoints.set({ p: { "a.py": [4, 9] } });
    breakpointInfo.set({ p: { "a.py": { 4: info("a"), 9: info("b") } } });
    const at = (l: number, c: number) => ({ line: l, col: c });
    // A line added at the end of line 2 (one-based): everything below moves down.
    expect(applyEdit("p", "a.py", { start: at(1, 5), oldEnd: at(1, 5), newEnd: at(2, 0) }, 12)).toEqual([5, 10]);
    expect(get(breakpointInfo).p["a.py"]).toEqual({ 5: info("a"), 10: info("b") });
    // Whole line 5 removed: its breakpoint and condition go, the later one moves up.
    expect(applyEdit("p", "a.py", { start: at(4, 0), oldEnd: at(5, 0), newEnd: at(4, 0) }, 13)).toEqual([9]);
    expect(get(breakpointInfo).p["a.py"]).toEqual({ 9: info("b") });
  });
});
