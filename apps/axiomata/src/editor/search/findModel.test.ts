import { describe, expect, it } from "vitest";

import type { EditorDocument } from "../document";
import { allSelections } from "../multicursor";
import { cursor, pos, range, selectionRange, type Range } from "../position";
import { docFrom, show } from "../testing";
import type { FindOptions } from "./find";
import { FindModel, MAX_FIND_CURSORS } from "./findModel";
import { SearchGuard, type VerifyReply, type VerifyRequest, type WorkerLike } from "./guard";
import { FakeSearchWorker, settle } from "./testing";

interface Setup {
  doc: EditorDocument;
  model: FindModel;
  commits: string[];
  /** Vi mode: the cursor sits on a match's start. */
  vi: { on: boolean };
}

function setup(marked: string, guard: SearchGuard | null = null): Setup {
  const doc = docFrom(marked);
  const commits: string[] = [];
  const vi = { on: false };
  const model = new FindModel({
    doc,
    guard: () => guard,
    vi: () => vi.on,
    place: (r: Range) => doc.setSelection(vi.on ? { anchor: r.start, head: r.start } : { anchor: r.start, head: r.end }),
    changed: () => undefined,
    commit: (query: string, _options: FindOptions) => commits.push(query),
  });
  return { doc, model, commits, vi };
}

function selected(doc: EditorDocument): string {
  return doc.store.slice(selectionRange(doc.selection));
}

describe("FindModel", () => {
  it("shows the first match from where the bar opened while typing, and counts", () => {
    const { doc, model } = setup("foo bar |foo baz foo");
    model.open();
    model.setQuery("fo");
    expect(show(doc)).toBe("foo bar ^fo|o baz foo");
    model.setQuery("foo");
    expect(show(doc)).toBe("foo bar ^foo| baz foo");
    expect(model.status()).toEqual({ kind: "ok", count: 3, truncated: false, index: 1 });
  });

  it("takes a selection within one line as the query, one over several lines as the scope", () => {
    const one = setup("a ^foo| b");
    one.model.open();
    expect(one.model.query).toBe("foo");
    const many = setup("x\n^foo\nfoo|\nfoo");
    many.model.open();
    expect(many.model.query).toBe("");
    many.model.setQuery("foo");
    expect(many.model.status()).toMatchObject({ kind: "ok", count: 2 });
  });

  it("goes forward and back, wrapping round and saying so", () => {
    const { doc, model } = setup("|a1 a2 a3");
    model.setQuery("a");
    expect(doc.selection.anchor).toEqual(pos(0, 0));
    model.next();
    expect(doc.selection.anchor).toEqual(pos(0, 3));
    model.next();
    model.next();
    expect(doc.selection.anchor).toEqual(pos(0, 0));
    expect(model.message).toBe("Wrapped to the start");
    model.next(true);
    expect(doc.selection.anchor).toEqual(pos(0, 6));
    expect(model.message).toBe("Wrapped to the end");
  });

  it("reports a broken pattern and finds nothing", () => {
    const { model } = setup("|abc");
    model.setOptions({ regex: true });
    model.setQuery("(");
    expect(model.status().kind).toBe("error");
    expect(model.valid).toBe(false);
  });

  it("replaces the current match and moves on to the next, never into the replacement", () => {
    const { doc, model } = setup("|a a a");
    model.setQuery("a");
    model.replacement = "aa";
    model.replaceOne();
    expect(doc.store.text()).toBe("aa a a");
    expect(show(doc)).toBe("aa ^a| a");
    model.replaceOne();
    model.replaceOne();
    expect(doc.store.text()).toBe("aa aa aa");
  });

  it("replaces off a match by first going to one", () => {
    const { doc, model } = setup("x |y a");
    model.setQuery("a");
    doc.setSelection({ anchor: pos(0, 0), head: pos(0, 0) });
    model.replacement = "b";
    model.replaceOne();
    expect(doc.store.text()).toBe("x y a");
    expect(selected(doc)).toBe("a");
  });

  it("replaces all with groups and kept case, as one undo step", () => {
    const { doc, model } = setup("|Foo=1\nfoo=2\nFOO=3");
    model.setOptions({ regex: true, preserveCase: true });
    model.setQuery("(foo)=(\\d)");
    model.replacement = "bar-$2";
    model.replaceAll();
    expect(doc.store.text()).toBe("Bar-1\nbar-2\nBAR-3");
    expect(model.message).toBe("Replaced 3 matches");
    doc.undo();
    expect(doc.store.text()).toBe("Foo=1\nfoo=2\nFOO=3");
  });

  it("replaces with `^` and lookbehind as the line sees them", () => {
    const { doc, model } = setup("|ab\nab");
    model.setOptions({ regex: true });
    model.setQuery("^a");
    model.replacement = "X";
    model.replaceAll();
    expect(doc.store.text()).toBe("Xb\nXb");
  });

  it("keeps to the scope, which follows the edits", () => {
    const { doc, model } = setup("a\n^a a\na|\na");
    model.toggleScope();
    expect(model.scope).toEqual(range(pos(1, 0), pos(2, 1)));
    model.setQuery("a");
    expect(model.status()).toMatchObject({ count: 3 });
    model.replacement = "bb";
    model.replaceAll();
    expect(doc.store.text()).toBe("a\nbb bb\nbb\na");
    expect(model.scope).toEqual(range(pos(1, 0), pos(2, 2)));
  });

  it("will not scope without a selection", () => {
    const { model } = setup("a |b");
    model.toggleScope();
    expect(model.scope).toBeNull();
    expect(model.message).toMatch(/Select/);
  });

  it("turns every match into a cursor, the one from the cursor on being the main one", () => {
    const { doc, model } = setup("x a |a a");
    model.setQuery("a");
    doc.setSelection({ anchor: pos(0, 3), head: pos(0, 3) });
    model.selectAll();
    expect(allSelections(doc).map((s) => s.anchor.col)).toEqual([4, 2, 6]);
  });

  it("in Vi mode, puts the cursor on a match's start and makes no cursors", () => {
    const { doc, model, vi } = setup("|x ab ab");
    vi.on = true;
    model.setQuery("ab");
    expect(doc.selection).toEqual({ anchor: pos(0, 2), head: pos(0, 2) });
    expect(model.status()).toMatchObject({ index: 0 });
    model.next();
    expect(doc.selection.head).toEqual(pos(0, 5));
    model.selectAll();
    expect(doc.extra).toEqual([]);
    model.replacement = "c";
    model.replaceOne();
    expect(doc.store.text()).toBe("x ab c");
  });

  it("makes the query the last search when it is used, not while it is typed", () => {
    const { model, commits } = setup("|a");
    model.setQuery("a");
    expect(commits).toEqual([]);
    model.next();
    model.close();
    expect(commits).toEqual(["a", "a"]);
  });

  it("⌘E takes the word at the cursor as literal text", () => {
    const { model } = setup("foo.b|ar");
    model.setOptions({ regex: true });
    model.useSelection();
    expect(model.query).toBe("bar");
    const dotted = setup("^a.b| c");
    dotted.model.setOptions({ regex: true });
    dotted.model.useSelection();
    expect(dotted.model.query).toBe("a\\.b");
  });

  it("⌘E does nothing when there is no word at the cursor", () => {
    const { model } = setup("|");
    model.useSelection();
    expect(model.query).toBe("");
  });

  it("replaces matches spanning a line break, moving between them automatically", () => {
    const { doc, model } = setup("|a\nb\na\nb");
    model.setOptions({ regex: true });
    model.setQuery("a\\nb");
    model.replacement = "X";
    // The search while typing already selected the first match, so this replaces it directly.
    model.replaceOne();
    expect(doc.store.text()).toBe("X\na\nb");
    model.replaceOne();
    expect(doc.store.text()).toBe("X\nX");
  });

  it("replaces every match spanning a line break in one go", () => {
    const { doc, model } = setup("|a\nb\na\nb");
    model.setOptions({ regex: true });
    model.setQuery("a\\nb");
    model.replacement = "X";
    model.replaceAll();
    expect(doc.store.text()).toBe("X\nX");
    expect(model.message).toBe("Replaced 2 matches");
  });

  it("steps through empty matches one position at a time, and counts one at the cursor as current", () => {
    const { doc, model } = setup("|aaa");
    model.setOptions({ regex: true });
    model.setQuery("x*");
    expect(model.status()).toMatchObject({ kind: "ok", count: 4, index: 0 });
    model.next();
    expect(doc.selection.anchor).toEqual(pos(0, 1));
    expect(model.status()).toMatchObject({ index: 1 });
    model.next();
    expect(doc.selection.anchor).toEqual(pos(0, 2));
  });

  it("replaces every empty match without looping forever", () => {
    const { doc, model } = setup("|aaa");
    model.setOptions({ regex: true });
    model.setQuery("x*");
    model.replacement = "-";
    model.replaceAll();
    expect(doc.store.text()).toBe("-a-a-a-");
  });

  it("in Vi mode, an empty match at the cursor counts as current", () => {
    const { model, vi } = setup("|aaa");
    vi.on = true;
    model.setOptions({ regex: true });
    model.setQuery("x*");
    expect(model.status()).toMatchObject({ index: 0 });
  });

  it("next() finds the match after an arbitrary selection", () => {
    // The search while typing would itself jump to a match, so the selection is set
    // only after that — `next()` then reads it as it stands, on "y", on no match at all.
    const { doc, model } = setup("x a y a z");
    model.setQuery("a");
    doc.setSelection({ anchor: pos(0, 4), head: pos(0, 5) });
    model.next();
    expect(doc.selection.anchor).toEqual(pos(0, 6));
  });

  it("next(true) finds the match before an arbitrary selection", () => {
    const { doc, model } = setup("x a y a z");
    model.setQuery("a");
    doc.setSelection({ anchor: pos(0, 4), head: pos(0, 5) });
    model.next(true);
    expect(doc.selection.anchor).toEqual(pos(0, 2));
  });

  it("next(true) from inside a match lands on that match's own start", () => {
    const { doc, model } = setup("z ab z ab z");
    model.setQuery("ab");
    doc.setSelection({ anchor: pos(0, 3), head: pos(0, 3) });
    model.next(true);
    expect(doc.selection.anchor).toEqual(pos(0, 2));
  });

  it("shifts the scope when an edit lands before it on the same line", () => {
    const { doc, model } = setup("abc ^def|ghi");
    model.toggleScope();
    expect(model.scope).toEqual(range(pos(0, 4), pos(0, 7)));
    doc.edit([{ range: range(pos(0, 0), pos(0, 0)), text: "XY" }], cursor(pos(0, 2)), "other");
    expect(model.scope).toEqual(range(pos(0, 6), pos(0, 9)));
  });

  it("collapses the scope's start into an edit that spans across it", () => {
    const { doc, model } = setup("abc ^def|ghi");
    model.toggleScope();
    doc.edit([{ range: range(pos(0, 2), pos(0, 7)), text: "Q" }], cursor(pos(0, 3)), "other");
    expect(model.scope).toEqual(range(pos(0, 3), pos(0, 3)));
  });

  it("refuses selectAll on more matches than can become cursors, without a truncated verdict", () => {
    const { model } = setup(`|${"a ".repeat(10_001)}`);
    model.setQuery("a");
    expect(model.status()).toMatchObject({ kind: "ok", count: 10_001, truncated: false });
    model.selectAll();
    expect(model.message).toBe(`Too many matches for cursors (at most ${MAX_FIND_CURSORS.toLocaleString("en")})`);
  });
});

describe("FindModel with the search guard", () => {
  it("waits for the verdict, then does what was asked last", async () => {
    const guard = new SearchGuard(() => new FakeSearchWorker());
    const { doc, model } = setup("|a b a", guard);
    model.setQuery("a");
    expect(model.status().kind).toBe("pending");
    model.next();
    await settle();
    expect(model.status()).toMatchObject({ kind: "ok", count: 2 });
    expect(doc.selection.anchor).toEqual(pos(0, 4));
    guard.dispose();
  });

  it("replaces all only after the verdict on the text, and moves on after a verdict on the new text", async () => {
    const guard = new SearchGuard(() => new FakeSearchWorker());
    const { doc, model } = setup("|a a a", guard);
    model.setQuery("a");
    await settle();
    model.replacement = "b";
    model.replaceOne();
    expect(doc.store.text()).toBe("b a a");
    await settle();
    expect(selected(doc)).toBe("a");
    expect(doc.selection.anchor).toEqual(pos(0, 2));
    guard.dispose();
  });

  it("reports a pattern too expensive and never runs it", async () => {
    const hang = (req: VerifyRequest) => req.source.includes("+)+");
    const guard = new SearchGuard(() => new FakeSearchWorker(hang), 10);
    const { doc, model } = setup("|xxxx", guard);
    model.setOptions({ regex: true });
    model.setQuery("(x+x+)+y");
    model.replaceAll();
    await new Promise((r) => setTimeout(r, 30));
    expect(model.status()).toMatchObject({ kind: "error" });
    expect(doc.store.text()).toBe("xxxx");
    guard.dispose();
  });

  it("only the most recently requested action runs once the pending verdict arrives", async () => {
    const guard = new SearchGuard(() => new FakeSearchWorker());
    const { doc, model } = setup("x a |y a z", guard);
    model.setQuery("a");
    expect(model.status().kind).toBe("pending");
    // Forward, then immediately superseded by backward before the verdict is in.
    model.next();
    model.next(true);
    await settle();
    expect(doc.selection.anchor).toEqual(pos(0, 2));
    guard.dispose();
  });
});

describe("FindModel with a truncated verdict", () => {
  /** A worker that always says the pattern was too common to report in full. */
  class TruncatedWorker implements WorkerLike {
    onmessage: ((event: { data: VerifyReply }) => void) | null = null;
    onerror: ((event: unknown) => void) | null = null;

    postMessage(message: VerifyRequest): void {
      queueMicrotask(() => {
        // Claims only the first match was found, though the text below has two.
        this.onmessage?.({ data: { id: message.id, ok: true, offsets: Int32Array.from([0, 1]), count: 1, truncated: true } });
      });
    }

    terminate(): void {
      /* nothing to clean up */
    }
  }

  it("replaceAll runs the pattern again in full rather than trusting the truncated offsets", async () => {
    const guard = new SearchGuard(() => new TruncatedWorker());
    const { doc, model } = setup("|a a", guard);
    model.setQuery("a");
    await settle();
    expect(model.status()).toMatchObject({ kind: "ok", count: 1, truncated: true });
    model.replacement = "b";
    model.replaceAll();
    expect(doc.store.text()).toBe("b b");
    guard.dispose();
  });

  it("refuses selectAll on a truncated verdict even though the reported offsets alone would fit", async () => {
    const guard = new SearchGuard(() => new TruncatedWorker());
    const { model } = setup("|a a", guard);
    model.setQuery("a");
    await settle();
    model.selectAll();
    expect(model.message).toMatch(/Too many/);
    guard.dispose();
  });
});

describe("FindModel and several cursors", () => {
  it("keeps the cursors while a query is typed, and goes to one match on a jump", () => {
    const { doc, model } = setup("|a b a b");
    doc.setSelections({ anchor: pos(0, 0), head: pos(0, 1) }, [{ anchor: pos(0, 4), head: pos(0, 5) }]);
    model.setQuery("b");
    expect(doc.extra).toHaveLength(1);
    expect(model.status()).toMatchObject({ kind: "ok", count: 2 });
    model.next();
    expect(doc.extra).toEqual([]);
    expect(selected(doc)).toBe("b");
  });
});
