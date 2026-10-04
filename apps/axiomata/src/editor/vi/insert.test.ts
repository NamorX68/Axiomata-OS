/**
 * More Insert/Replace and number coverage (`docs/plans/editor.md`, ED3, D17):
 * a Replace-mode count, Ctrl-a on a negative/hex number, a macro that pastes,
 * and undo after an Insert session an arrow key interrupted — as a complement
 * to the `"commands"`, `"macros"` and `"undo"` sections of `vi.test.ts`.
 */

import { describe, expect, it } from "vitest";

import { show, docFrom, ctx } from "../testing";
import { ViMachine, ViShared } from "./machine";

function setup(marked: string) {
  const doc = docFrom(marked);
  const m = new ViMachine(doc, new ViShared(null), {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    effect: () => {},
  });
  return { doc, m };
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

table("a Replace-mode count", [
  // `3Rab<Esc>`: the two typed characters overwrite the text three times over.
  ["|abcdefgh", "3Rab<Esc>", "ababa|bgh"],
  // Past the line's end, Replace mode inserts instead — same as plain Replace.
  ["|ab", "2Rxyz<Esc>", "xyzxy|z"],
]);

table("Ctrl-a / Ctrl-x on negative and hex numbers", [
  ["|x -5 y", "<C-a>", "x -|4 y"],
  ["|x -1 y", "<C-x>", "x -|2 y"],
  ["|x 0xff y", "<C-a>", "x 0x10|0 y"],
  ["|x 0Xff y", "<C-a>", "x 0X10|0 y"], // keeps the `0X` capital the same as typed
  ["|x 0x00 y", "<C-x>", "x 0x0|0 y"], // clamped at zero, not negative
]);

describe("macros that paste", () => {
  it("records a yank and its paste, then replays both further down", () => {
    const { doc, m } = setup("|foo\nbar\nbaz\nqux");
    m.feedKeys("qqyyjpq"); // yank the line, move down, paste it below
    expect(show(doc)).toBe("foo\nbar\n|foo\nbaz\nqux");
    m.feedKeys("j@q");
    // The macro re-yanks whatever line the cursor is on when it replays (here
    // "baz"), then moves down and pastes that — not the original "foo".
    expect(show(doc)).toBe("foo\nbar\nfoo\nbaz\nqux\n|baz");
  });

  it("a macro that reads a named register pastes what was yanked before recording", () => {
    const { doc, m } = setup("|one\ntwo\nthree");
    m.feedKeys('"ayyj'); // yank "one" into "a, move to "two"
    m.feedKeys('qq"apq'); // record: paste register a below the current line
    expect(show(doc)).toBe("one\ntwo\n|one\nthree");
    m.feedKeys("j@q");
    expect(show(doc)).toBe("one\ntwo\none\nthree\n|one");
  });
});

describe("undo after an Insert session an arrow key interrupted", () => {
  it("without an arrow, the whole Insert session is one undo step", () => {
    const { doc, m } = setup("|ab");
    m.feedKeys("ixy<Esc>u");
    expect(show(doc)).toBe("|ab");
  });

  it("an arrow key splits the session: what was typed before and after it undo separately", () => {
    const { doc, m } = setup("|ab");
    m.feedKeys("ix<Left>y<Esc>");
    expect(show(doc)).toBe("|yxab");
    m.feedKeys("u");
    expect(show(doc)).toBe("|xab");
    m.feedKeys("u");
    expect(show(doc)).toBe("|ab");
  });

  it("Ctrl-o's one command is its own undo step, between the typing around it", () => {
    const { doc, m } = setup("|ab");
    m.feedKeys("ix<C-o>xy<Esc>");
    expect(show(doc)).toBe("x|yb");
    m.feedKeys("u");
    expect(show(doc)).toBe("x|b");
    m.feedKeys("u");
    expect(show(doc)).toBe("x|ab");
    m.feedKeys("u");
    expect(show(doc)).toBe("|ab");
  });

  it("the split does work when the session reopens its own group (after Ctrl-o)", () => {
    const { doc, m } = setup("|ab");
    m.feedKeys("i1"); // opens the outer group itself
    m.feedKeys("<C-o>l"); // one Normal command closes that group, then reopens Insert fresh
    m.feedKeys("2<Left>3<Esc>"); // this session owns its group, so the arrow does split it
    expect(show(doc)).toBe("1a|32b");
    m.feedKeys("u");
    expect(show(doc)).toBe("1a|2b"); // only "3" undone
    m.feedKeys("u");
    expect(show(doc)).toBe("1a|b"); // "2" is a separate step
    m.feedKeys("u");
    expect(show(doc)).toBe("|ab"); // and "1" (from before Ctrl-o) another again
  });
});
