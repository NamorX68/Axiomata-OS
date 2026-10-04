/**
 * A read-only surface (`docs/plans/editor.md`, ED3, V1) refuses every kind of
 * change but still moves, selects, yanks and searches — as a complement to
 * the single case in `vi.test.ts`'s "modes and status" describe block, which
 * only tried `dw`, `i`, `w` and `y`.
 */

import { describe, expect, it } from "vitest";

import { show, docFrom, ctx } from "../testing";
import { ViMachine, ViShared, type ViEffect } from "./machine";

function setup(marked: string) {
  const doc = docFrom(marked);
  const effects: ViEffect[] = [];
  const m = new ViMachine(doc, new ViShared(null), {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    effect: (e) => effects.push(e),
    readOnly: true,
  });
  return { doc, m, effects };
}

/**
 * Runs `keys` on a read-only `before`; the underlying text must not change (a
 * refused Visual write may still leave the selection extended, so this checks
 * the text alone, not the marked-up `show()` form) and a bell must ring.
 */
function refused(before: string, keys: string): void {
  it(`refuses ${keys}`, () => {
    const { doc, m, effects } = setup(before);
    const originalText = doc.store.text();
    m.feedKeys(keys);
    expect(doc.store.text()).toBe(originalText);
    expect(effects.some((e) => e.type === "bell")).toBe(true);
  });
}

describe("a read-only surface refuses every operator", () => {
  refused("|foo bar", "dw");
  refused("|foo bar", "cw");
  refused("|foo bar", ">j");
  refused("|foo bar", "<<");
  refused("|foo bar", "gUiw");
  refused("|foo bar", "gcc");
  refused("|foo bar", "ysiw)");
});

describe("a read-only surface refuses every command that would enter Insert or Replace mode", () => {
  refused("|foo bar", "i");
  refused("|foo bar", "a");
  refused("|foo bar", "I");
  refused("|foo bar", "A");
  refused("|foo bar", "o");
  refused("|foo bar", "O");
  refused("|foo bar", "R");
  refused("|foo bar", "s");
  refused("|foo bar", "S");
  refused("|foo bar", "C");

  it("stays in Normal mode after a refused i", () => {
    const { m } = setup("|foo");
    m.feedKeys("i");
    expect(m.status().mode).toBe("normal");
  });
});

describe("a read-only surface refuses every other change command", () => {
  refused("|foo bar", "x");
  refused("|foo bar", "X");
  refused("|foo bar", "D");
  refused("foo |bar", "p");
  refused("foo |bar", "P");
  refused("|a\nb", "J");
  refused("|foo bar", "~");
  refused("|foo bar", "rx");
  refused("|x 7 y", "<C-a>");
  refused("|x 7 y", "<C-x>");
  refused("'|foo' bar", "cs'\"");
  refused("[ |foo ] bar", "ds[");
});

describe("a read-only surface refuses Visual-mode writes", () => {
  refused("|foo bar", "vlld");
  refused("|foo bar", "vllx");
  refused("|a\nb\nc", "Vjd");
  refused("|foo bar", "veU");
  refused("|a\nb\nc", "Vj>");
  refused("|abc\nabc", "<C-v>jIx<Esc>");
});

describe("a read-only surface still allows moving, selecting, yanking and searching", () => {
  it("moves the cursor with plain motions", () => {
    const { doc, m } = setup("|foo bar");
    m.feedKeys("w");
    expect(show(doc)).toBe("foo |bar");
  });

  it("selects and yanks in Visual mode", () => {
    const { doc, m } = setup("|foo bar");
    m.feedKeys("vlly");
    expect(show(doc)).toBe("|foo bar");
  });

  it("yanks with an operator in Normal mode", () => {
    const { doc, m } = setup("|foo bar");
    m.feedKeys("yiw");
    expect(show(doc)).toBe("|foo bar");
  });

  it("hands ]c, [c and gf straight to the view, and opens the command line", () => {
    const { m, effects } = setup("|a\nb");
    m.feedKeys("]c[cgf");
    expect(effects).toEqual([{ type: "hunk", dir: 1 }, { type: "hunk", dir: -1 }, { type: "openFile" }]);
    m.feedKeys(":");
    expect(m.status().cmdline?.kind).toBe(":");
    m.feedKeys("<Esc>/b<CR>");
    expect(effects.filter((e) => e.type === "bell")).toEqual([]);
    expect(m.cursor).toEqual({ line: 1, col: 0 });
  });
});
