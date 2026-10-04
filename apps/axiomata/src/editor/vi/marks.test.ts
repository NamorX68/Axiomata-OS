/**
 * Marks and the jump list (`docs/plans/editor.md`, ED3, D17): direct tests of
 * `Marks`, plus machine-level jump-list coverage with several jumps in a row —
 * a complement to the single-jump case in `vi.test.ts`'s "marks and jumps".
 */

import { describe, expect, it } from "vitest";

import { pos } from "../position";
import type { TextChange } from "../document";
import { Marks } from "./marks";
import { show, docFrom, ctx } from "../testing";
import { ViMachine, ViShared } from "./machine";

/** A `TextChange` with only the fields `Marks.follow` reads. */
function change(startLine: number, oldEndLine: number, newEndLine: number): TextChange {
  return {
    start: pos(startLine, 0),
    oldEnd: pos(oldEndLine, 0),
    newEnd: pos(newEndLine, 0),
    startIndex: 0,
    oldEndIndex: 0,
    newEndIndex: 0,
  };
}

describe("Marks — following text changes", () => {
  it("shifts a mark below an insertion of lines down by the same amount", () => {
    const marks = new Marks();
    marks.set("a", pos(5, 2));
    marks.follow(change(1, 1, 3)); // two lines inserted at line 1
    expect(marks.get("a")).toEqual(pos(7, 2));
  });

  it("shifts a mark below a deletion of lines up", () => {
    const marks = new Marks();
    marks.set("a", pos(5, 2));
    marks.follow(change(1, 3, 1)); // lines 1..3 removed
    expect(marks.get("a")).toEqual(pos(3, 2));
  });

  it("leaves a mark above the change untouched", () => {
    const marks = new Marks();
    marks.set("a", pos(0, 4));
    marks.follow(change(5, 8, 5));
    expect(marks.get("a")).toEqual(pos(0, 4));
  });

  it("lands a mark inside deleted lines at the start of the change", () => {
    const marks = new Marks();
    marks.set("a", pos(3, 7)); // inside lines 1..5, which are about to be removed
    marks.follow(change(1, 5, 1));
    expect(marks.get("a")).toEqual(pos(1, 0));
  });

  it("does nothing when the change does not add or remove lines", () => {
    const marks = new Marks();
    marks.set("a", pos(2, 3));
    marks.follow(change(2, 2, 2));
    expect(marks.get("a")).toEqual(pos(2, 3));
  });

  it("moves the jump list the same way as marks", () => {
    const marks = new Marks();
    marks.pushJump(pos(4, 0));
    marks.follow(change(0, 0, 2));
    // The one jump entry followed the insertion to line 6; asking to go back
    // from exactly that position finds nothing further behind it.
    expect(marks.older(1, pos(6, 0))).toBeNull();
  });
});

describe("Marks — the jump list with several jumps", () => {
  it("walks back through several jumps, then forward again, and stops at the ends", () => {
    const marks = new Marks();
    const p0 = pos(0, 0);
    const p1 = pos(1, 0);
    const p2 = pos(2, 0);
    marks.pushJump(p0);
    marks.pushJump(p1);
    marks.pushJump(p2);
    const here = pos(9, 0);

    expect(marks.older(1, here)).toEqual(p2);
    expect(marks.older(1, here)).toEqual(p1);
    expect(marks.older(1, here)).toEqual(p0);
    expect(marks.older(1, here)).toBeNull(); // nothing further back

    expect(marks.newer(1)).toEqual(p1);
    expect(marks.newer(1)).toEqual(p2);
    expect(marks.newer(1)).toEqual(here); // the place Ctrl-o started from, appended
    expect(marks.newer(1)).toBeNull(); // nothing further forward
  });

  it("jumping again from the middle of the list drops the abandoned forward entries", () => {
    const marks = new Marks();
    marks.pushJump(pos(0, 0));
    marks.pushJump(pos(1, 0));
    marks.pushJump(pos(2, 0));
    marks.older(2, pos(9, 0)); // back to the first jump; pos(9,0) and pos(2,0) still ahead
    marks.pushJump(pos(5, 0)); // a fresh jump discards what was ahead
    expect(marks.older(1, pos(6, 0))).toEqual(pos(5, 0));
    // `older` bookmarked pos(6,0) (where it was called from) as the newest entry;
    // `newer` returns to it first, then there is nothing further forward.
    expect(marks.newer(1)).toEqual(pos(6, 0));
    expect(marks.newer(1)).toBeNull();
  });
});

function setup(marked: string, shared = new ViShared(null)) {
  const doc = docFrom(marked);
  const m = new ViMachine(doc, shared, {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    effect: () => {},
  });
  return { doc, m };
}

describe("the machine's jump list end to end", () => {
  it("Ctrl-o with a count goes back several jumps at once, Ctrl-i forward again", () => {
    const { doc, m } = setup("a\nb\n|c\nd\ne");
    m.feedKeys("G"); // jump to the last line (records the start, line 2)
    m.feedKeys("gg"); // jump back to the first (records the start, line 4)
    expect(show(doc)).toBe("|a\nb\nc\nd\ne");
    m.feedKeys("2<C-o>");
    expect(show(doc)).toBe("a\nb\n|c\nd\ne"); // back two jumps, to where G started
    m.feedKeys("<C-i>");
    expect(show(doc)).toBe("a\nb\nc\nd\n|e"); // forward one, to where gg started
  });
});
