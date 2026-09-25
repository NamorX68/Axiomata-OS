/**
 * More motion coverage (`docs/plans/editor.md`, ED3, D17), as a table-driven
 * complement to the `"motions"` table in `vi.test.ts`: `ge` across lines, `%`
 * with a count, `|`, `g_`, `(`, `gj`/`gk` on an unwrapped layout, and a failed
 * `f` cancelling an operator.
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

table("more motions", [
  // `ge`: the end of the previous word, crossing a line break.
  ["foo\nba|r", "ge", "fo|o\nbar"],
  ["foo\n|bar", "ge", "fo|o\nbar"],
  // `%` with a count: the line `count` percent into the text (10 lines here).
  ["|l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9", "50%", "l0\nl1\nl2\nl3\n|l4\nl5\nl6\nl7\nl8\nl9"],
  ["|l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9", "100%", "l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\n|l9"],
  // `|`: to the display column, counting from 1.
  ["|abcdef", "4|", "abc|def"],
  ["abc|def", "1|", "|abcdef"],
  // `g_`: the last non-blank of the line.
  ["|abc   ", "g_", "ab|c   "],
  ["   |abc", "2g_", "   ab|c"],
  // `(`: back to the start of the current sentence, then the one before it.
  ["One. Two. Thre|e.", "(", "One. Two. |Three."],
  ["One. Two. Thre|e.", "((", "One. |Two. Three."],
]);

describe("gj/gk on an unwrapped layout", () => {
  it("moves the same as j/k when nothing wraps", () => {
    expect(vi("|a\nb\nc", "gj")).toBe("a\n|b\nc");
    expect(vi("a\n|b\nc", "gk")).toBe("|a\nb\nc");
  });

  it("keeps the display column across several unwrapped rows", () => {
    expect(vi("ab|c\na\nabc", "2gj")).toBe("abc\na\nab|c");
  });
});

describe("a failed f cancels its operator", () => {
  it("dfz leaves the text unchanged when z is not on the line", () => {
    const { doc, m, effects } = setup("|foo bar");
    m.feedKeys("dfz");
    expect(show(doc)).toBe("|foo bar");
    expect(effects.some((e) => e.type === "bell")).toBe(true);
  });

  it("cFz<Esc> never enters Insert mode either", () => {
    const { doc, m } = setup("foo |bar");
    m.feedKeys("cFz");
    expect(show(doc)).toBe("foo |bar");
    expect(m.status().mode).toBe("normal");
  });

  it("d;  with no previous find bells and changes nothing", () => {
    const { doc, m, effects } = setup("|foo bar");
    m.feedKeys("d;");
    expect(show(doc)).toBe("|foo bar");
    expect(effects.some((e) => e.type === "bell")).toBe(true);
  });
});
