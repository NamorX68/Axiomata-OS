/** Vi over closed folds (`docs/plans/editor.md`, ED5, T18): `j`/`k` step over one, linewise operators take it whole. */

import { describe, expect, it } from "vitest";

import { FoldState } from "../fold/state";
import { ctx, docFrom, show } from "../testing";
import { ViMachine, ViShared, type ViEffect } from "./machine";

/** A machine on `marked` with lines `start`–`end` folded. */
function setup(marked: string, fold: { start: number; end: number }) {
  const doc = docFrom(marked);
  const folds = new FoldState(doc);
  folds.close(fold);
  const effects: ViEffect[] = [];
  const m = new ViMachine(doc, new ViShared(null), {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 }, folds }),
    effect: (e) => effects.push(e),
  });
  return { doc, m, folds, effects };
}

const TEXT = "|a\nb {\n  c\n  d\n}\ne";

describe("Vi over closed folds", () => {
  it("j and k count a closed fold as one line, landing on its header", () => {
    const { doc, m } = setup(TEXT, { start: 1, end: 4 });
    m.feedKeys("jj");
    expect(show(doc)).toBe("a\nb {\n  c\n  d\n}\n|e");
    m.feedKeys("k");
    expect(show(doc)).toBe("a\n|b {\n  c\n  d\n}\ne");
    m.feedKeys("gg2j");
    expect(show(doc)).toBe("a\nb {\n  c\n  d\n}\n|e");
  });

  it("dd on a folded header deletes the whole fold", () => {
    const { doc, m, folds } = setup("a\n|b {\n  c\n}\ne", { start: 1, end: 3 });
    m.feedKeys("dd");
    expect(show(doc)).toBe("a\n|e");
    expect(folds.closed).toEqual([]);
    m.feedKeys("P");
    expect(show(doc)).toBe("a\n|b {\n  c\n}\ne");
  });

  it("yj over a fold yanks all of it", () => {
    const { doc, m } = setup("|a\nb {\n  c\n}\ne", { start: 1, end: 3 });
    m.feedKeys("yjGp");
    expect(show(doc)).toBe("a\nb {\n  c\n}\ne\n|a\nb {\n  c\n}");
  });

  it("zc zo za zM zR ask the view to fold", () => {
    const { m, effects } = setup(TEXT, { start: 1, end: 4 });
    m.feedKeys("zczozazMzR");
    expect(effects).toEqual([
      { type: "fold", action: "close" },
      { type: "fold", action: "open" },
      { type: "fold", action: "toggle" },
      { type: "fold", action: "closeAll" },
      { type: "fold", action: "openAll" },
    ]);
  });
});

describe("Vi over closed folds, edge cases", () => {
  it("k from the line under a fold lands on its header, at the goal column", () => {
    const { doc, m } = setup("a\nbbb {\n  c\n}\ne|ee", { start: 1, end: 3 });
    m.feedKeys("k");
    expect(show(doc)).toBe("a\nb|bb {\n  c\n}\neee");
    m.feedKeys("k");
    expect(show(doc)).toBe("|a\nbbb {\n  c\n}\neee");
  });

  it("a count past either end stops at the last shown line, or the first", () => {
    const { doc, m } = setup(TEXT, { start: 1, end: 4 });
    m.feedKeys("9j");
    expect(show(doc)).toBe("a\nb {\n  c\n  d\n}\n|e");
    m.feedKeys("9k");
    expect(show(doc)).toBe("|a\nb {\n  c\n  d\n}\ne");
  });

  it("j on the header of a fold that ends the text fails and stays; a count stops on the header", () => {
    const { doc, m } = setup("|a\nb {\n  c\n}", { start: 1, end: 3 });
    m.feedKeys("j");
    expect(show(doc)).toBe("a\n|b {\n  c\n}");
    m.feedKeys("j");
    expect(show(doc)).toBe("a\n|b {\n  c\n}");
    m.feedKeys("gg5j");
    expect(show(doc)).toBe("a\n|b {\n  c\n}");
  });

  it("steps over two folds side by side one at a time", () => {
    const doc = docFrom("|a\nb\n c\nd\n e\nf");
    const folds = new FoldState(doc);
    folds.close({ start: 1, end: 2 });
    folds.close({ start: 3, end: 4 });
    const m = new ViMachine(doc, new ViShared(null), {
      ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 }, folds }),
      effect: () => {},
    });
    m.feedKeys("j");
    expect(doc.selection.head.line).toBe(1);
    m.feedKeys("j");
    expect(doc.selection.head.line).toBe(3);
    m.feedKeys("j");
    expect(doc.selection.head.line).toBe(5);
    m.feedKeys("2k");
    expect(doc.selection.head.line).toBe(1);
  });

  it("dj from a folded header takes the fold and the line after it", () => {
    const { doc, m } = setup("a\n|b {\n  c\n}\ne\nf", { start: 1, end: 3 });
    m.feedKeys("dj");
    expect(show(doc)).toBe("a\n|f");
  });

  it("dk from under a fold takes the whole fold with it", () => {
    const { doc, m } = setup("a\nb {\n  c\n}\n|e\nf", { start: 1, end: 3 });
    m.feedKeys("dk");
    expect(show(doc)).toBe("a\n|f");
  });

  it(">> on a folded header shifts every line of the fold", () => {
    const { doc, m, folds } = setup("a\n|b {\n  c\n}\ne", { start: 1, end: 3 });
    m.feedKeys(">>");
    // The shift width is the indentation the text itself uses (two spaces).
    expect(doc.store.text()).toBe("a\n  b {\n    c\n  }\ne");
    expect(doc.selection.head.line).toBe(1);
    // Shifting is typing on the header and among its hidden lines: the fold stays.
    expect(folds.closed).toEqual([{ start: 1, end: 3 }]);
  });

  it("cc on a folded header changes the whole fold", () => {
    const { doc, m, folds } = setup("a\n|b {\n  c\n}\ne", { start: 1, end: 3 });
    m.feedKeys("cc");
    expect(show(doc)).toBe("a\n|\ne");
    expect(m.mode).toBe("insert");
    expect(folds.closed).toEqual([]);
  });

  it("yy on a folded header yanks the whole fold", () => {
    const { doc, m } = setup("a\n|b {\n  c\n}\ne", { start: 1, end: 3 });
    m.feedKeys("yyGp");
    expect(show(doc)).toBe("a\nb {\n  c\n}\ne\n|b {\n  c\n}");
  });

  it("without folds in the context, j and k count lines as before", () => {
    const doc = docFrom(TEXT);
    const m = new ViMachine(doc, new ViShared(null), {
      ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
      effect: () => {},
    });
    m.feedKeys("2j");
    expect(doc.selection.head.line).toBe(2);
    m.feedKeys("9j");
    expect(doc.selection.head.line).toBe(5);
  });

  it("o and p on a folded header go below the whole fold, which stays closed", () => {
    const o = setup("a\n|b {\n  c\n}\ne", { start: 1, end: 3 });
    o.m.feedKeys("ox<Esc>");
    expect(show(o.doc)).toBe("a\nb {\n  c\n}\n|x\ne");
    expect(o.folds.closed).toEqual([{ start: 1, end: 3 }]);
    const p = setup("|a\nb {\n  c\n}\ne", { start: 1, end: 3 });
    p.m.feedKeys("yyjp");
    expect(show(p.doc)).toBe("a\nb {\n  c\n}\n|a\ne");
  });

  it("H M L, a bare G and paging land on a fold's header, never inside it", () => {
    const text = "|a\nb {\n  c\n  d\n}\ne";
    const g = setup(text, { start: 4, end: 5 });
    g.m.feedKeys("G");
    expect(g.doc.selection.head.line).toBe(4);
    const l = setup(text, { start: 1, end: 3 });
    l.m.feedKeys("3H");
    expect(l.doc.selection.head.line).toBe(4);
    const m = setup(text, { start: 3, end: 5 });
    m.m.feedKeys("M");
    expect(m.doc.selection.head.line).toBe(3);
    const d = setup(text, { start: 1, end: 3 });
    d.m.feedKeys("2<C-d>");
    expect(d.doc.selection.head.line).toBe(1);
  });
});
