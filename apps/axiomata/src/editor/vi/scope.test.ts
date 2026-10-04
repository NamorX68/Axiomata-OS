/**
 * What belongs to one machine and what to all of them, and what a read-only
 * view refuses on the command line (`docs/plans/editor.md`, ED3.3 review).
 */

import { describe, expect, it } from "vitest";

import { ctx, docFrom, show } from "../testing";
import { ViMachine, ViShared, type ViEffect } from "./machine";

function machine(marked: string, shared: ViShared, env: { fileName?: string; fileKey?: string; readOnly?: boolean }) {
  const doc = docFrom(marked);
  const effects: ViEffect[] = [];
  const m = new ViMachine(doc, shared, {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    effect: (e) => effects.push(e),
    ...env,
  });
  return { doc, m, effects };
}

describe('"% with several machines on one ViShared', () => {
  it("is the file of the machine that reads it, whichever was made last", () => {
    const shared = new ViShared(null);
    const a = machine("|", shared, { fileName: "a.md" });
    const b = machine("|", shared, { fileName: "b.md" });
    a.m.feedKeys('"%p');
    b.m.feedKeys('"%p');
    expect(show(a.doc)).toBe("a.m|d");
    expect(show(b.doc)).toBe("b.m|d");
  });
});

describe("a read-only view on the command line (V1)", () => {
  function refuses(keys: string) {
    it(`refuses ${keys} with a message and no effect`, () => {
      const { m, effects } = machine("|a", new ViShared(null), { readOnly: true });
      m.feedKeys(keys);
      expect(effects.filter((e) => e.type !== "bell")).toEqual([]);
      expect(effects.some((e) => e.type === "bell")).toBe(true);
    });
  }

  refuses(":q<CR>");
  refuses(":q!<CR>");
  refuses(":e!<CR>");
  refuses(":set nu<CR>");
  refuses(":x<CR>");
  refuses("ZZ");
  refuses("ZQ");

  it("says why", () => {
    const { m } = machine("|a", new ViShared(null), { readOnly: true });
    m.feedKeys(":q<CR>");
    expect(m.status().message).toEqual({ text: "Not available in a read-only view", error: true });
  });

  it("does not open another file's mark", () => {
    const shared = new ViShared(null);
    const editable = machine("|a", shared, { fileKey: "workspace:a.md" });
    editable.m.feedKeys("mA");
    const view = machine("|b", shared, { fileKey: "workspace:b.md", readOnly: true });
    view.m.feedKeys("'A");
    expect(view.effects.filter((e) => e.type === "fileMark")).toEqual([]);
    expect(view.effects.some((e) => e.type === "bell")).toBe(true);
  });

  it("still moves with :{n} and turns the highlight off", () => {
    const { m } = machine("|a\nb", new ViShared(null), { readOnly: true });
    m.feedKeys(":2<CR>");
    expect(m.cursor).toEqual({ line: 1, col: 0 });
    m.feedKeys("/a<CR>:noh<CR>");
    expect(m.searchHighlights(0, 1).matches.size).toBe(0);
  });
});

describe("Visual text objects that are whole lines", () => {
  it("vip selects every line of the paragraph, the last one included", () => {
    const { doc, m } = machine("|a\nb\nc\n\nd", new ViShared(null), {});
    m.feedKeys("vip");
    expect(m.visualRegion()).toEqual({ kind: "line", first: 0, last: 2 });
    m.feedKeys("d");
    expect(show(doc)).toBe("|\nd");
  });

  it("vif takes a syntax object's last line too", () => {
    const shared = new ViShared(null);
    const doc = docFrom("fn f() {\n    |a;\n    b;\n}");
    const m = new ViMachine(doc, shared, {
      ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
      effect: () => undefined,
      syntaxObjects: () => ({ start: { line: 1, col: 0 }, end: { line: 2, col: 6 }, linewise: true }),
    });
    m.feedKeys("vif");
    expect(m.visualRegion()).toEqual({ kind: "line", first: 1, last: 2 });
  });
});
