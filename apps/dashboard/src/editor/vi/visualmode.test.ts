/**
 * More Visual-mode coverage (`docs/plans/editor.md`, ED3, V2): `o`/`O` in
 * block mode, `gv`, Visual `J`, `>` with a count, and `:` seeding `'<,'>` — as
 * a complement to the `"visual"` table in `vi.test.ts`.
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
  });
  return { doc, m, effects };
}

function vi(before: string, keys: string): string {
  const { doc, m } = setup(before);
  m.feedKeys(keys);
  return show(doc);
}

type Case = [string, string, string];

function table(name: string, cases: Case[]) {
  describe(name, () => {
    for (const [before, keys, after] of cases) {
      it(`${JSON.stringify(before)} ${keys}`, () => {
        expect(vi(before, keys)).toBe(after);
      });
    }
  });
}

describe("o and O in Visual-block mode", () => {
  it("o swaps the head to the opposite corner on the diagonal", () => {
    const { doc, m } = setup("abcdef\nabcdef\nabcdef");
    m.feedKeys("ll<C-v>jjllo");
    // Head moves from the bottom-right of the block to its top-left corner.
    expect(m.visualRegion()).toEqual({ kind: "block", first: 0, last: 2, left: 2, right: 4 });
    expect(doc.selection.head).toEqual({ line: 0, col: 2 });
  });

  it("O swaps only the column, keeping the head's row", () => {
    const { doc, m } = setup("abcdef\nabcdef\nabcdef");
    m.feedKeys("ll<C-v>jjllO");
    // Head keeps the bottom row but swaps to the block's left column.
    expect(doc.selection.head).toEqual({ line: 2, col: 2 });
  });

  it("O outside block mode behaves exactly like o", () => {
    expect(vi("|foo bar", "vllOd")).toBe(vi("|foo bar", "vlld"));
  });
});

describe("gv reselects the last Visual selection", () => {
  it("restores a characterwise selection after leaving with Esc", () => {
    const { m } = setup("|foo bar");
    m.feedKeys("vll<Esc>gv");
    expect(m.status().mode).toBe("visual");
    expect(m.visualRegion()).toEqual({ kind: "char", start: { line: 0, col: 0 }, end: { line: 0, col: 3 } });
  });

  it("restores a selection made after a change finished it (Visual d)", () => {
    const { doc, m } = setup("|foo bar baz");
    m.feedKeys("vlld"); // deletes "foo", selection was columns 0..2
    m.feedKeys("wgv");
    expect(m.status().mode).toBe("visual");
    expect(show(doc)).toContain("^");
  });

  it("bells when there is nothing to reselect", () => {
    const { m, effects } = setup("|foo");
    m.feedKeys("gv");
    expect(effects.some((e) => e.type === "bell")).toBe(true);
    expect(m.status().mode).toBe("normal");
  });

  it("gv while already in Visual mode is a no-op, not a bell", () => {
    const { m, effects } = setup("|foo bar");
    m.feedKeys("vllgv");
    expect(m.status().mode).toBe("visual");
    expect(effects.some((e) => e.type === "bell")).toBe(false);
  });
});

table("Visual J", [
  ["|a\nb\nc\nd", "VjJ", "a| b\nc\nd"],
  ["|a\n   b\nc", "VjgJ", "a|   b\nc"],
]);

describe("> and < with a count in Visual mode", () => {
  it("2> shifts two indentation levels at once", () => {
    expect(vi("|a\nb", "Vj2>")).toBe("        |a\n        b");
  });

  it("2< shifts two levels back out again", () => {
    expect(vi("        |a\n        b", "Vj2<")).toBe("|a\nb");
  });
});

describe(": in Visual mode seeds the command line with '<,'>", () => {
  it("opens the command line with the Visual range and leaves Visual mode", () => {
    const { m, effects } = setup("|a\nb\nc");
    m.feedKeys("Vj:");
    expect(effects).toContainEqual({ type: "commandLine", kind: ":", initial: "'<,'>" });
    expect(m.status().mode).toBe("normal");
  });
});
