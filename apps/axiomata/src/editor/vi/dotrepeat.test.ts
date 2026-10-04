/**
 * More `.`-repeat coverage (`docs/plans/editor.md`, ED3, D17): `ciw`, `>>`,
 * and a Visual `>` — as a complement to the `"repeat"` table in `vi.test.ts`,
 * which only tries `dw`, `cw`, `A`, a Visual `d` and `r`.
 */

import { describe, expect, it } from "vitest";

import { show, docFrom, ctx } from "../testing";
import { ViMachine, ViShared } from "./machine";

function vi(before: string, keys: string): string {
  const doc = docFrom(before);
  const m = new ViMachine(doc, new ViShared(null), {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    effect: () => {},
  });
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

table("dot-repeat of ciw", [
  ["|foo bar baz", "ciwX<Esc>w.", "X |X baz"],
]);

table("dot-repeat of >>", [
  ["|a\nb\nc", ">>j.", "    a\n    |b\nc"],
]);

table("dot-repeat of a Visual >", [
  // `Vj>` shifts lines 0-1 once, landing on line 0; `j` moves to line 1; `.`
  // reselects the same size (two lines) from there and shifts lines 1-2 —
  // line 1 a second time, line 2 for the first time.
  ["|a\nb\nc\nd", "Vj>j.", "    a\n        |b\n    c\nd"],
]);
