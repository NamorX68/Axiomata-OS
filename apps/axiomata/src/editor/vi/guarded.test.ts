/**
 * Vi's searches with the search guard in place (`docs/plans/editor.md`, ED5,
 * T5): nothing runs until the worker has vouched for the pattern, the keys
 * wait meanwhile, and the result is what it was without the guard.
 */

import { describe, expect, it, vi as vitest } from "vitest";

import { SearchGuard } from "../search/guard";
import { FakeSearchWorker, settle } from "../search/testing";
import { ctx, docFrom, show } from "../testing";
import * as matches from "../search/matches";
import { ViMachine, ViShared, type ViEffect } from "./machine";

function setup(marked: string, worker = new FakeSearchWorker(), timeoutMs = 1000) {
  const doc = docFrom(marked);
  const effects: ViEffect[] = [];
  const shared = new ViShared(null);
  shared.guard = new SearchGuard(() => worker, timeoutMs);
  const m = new ViMachine(doc, shared, {
    ctx: () => ({ ...ctx(), viewport: { top: 0, bottom: 9 } }),
    effect: (e) => effects.push(e),
    fileName: "notes.md",
  });
  return { doc, m, effects, shared, worker };
}

async function vi(before: string, keys: string): Promise<string> {
  const { doc, m } = setup(before);
  m.feedKeys(keys);
  await settle();
  return show(doc);
}

describe("searching through the guard", () => {
  const cases: Array<[string, string, string]> = [
    ["|foo bar foo", "/foo<CR>", "foo bar |foo"],
    ["|foo bar foo", "/foo<CR>n", "|foo bar foo"],
    ["foo |bar foo bar", "?foo<CR>n", "foo bar |foo bar"],
    ["|a b a b a b a", "/a<CR>3n", "|a b a b a b a"],
    ["|cat concat cat", "*", "cat concat |cat"],
    ["|one two one", "d/two<CR>", "|two one"],
    ["a|b b\nb", ":%s/b/X/g<CR>", "aX X\n|X"],
    // A pattern naming a line break reaches across lines (T5).
    ["|ab\ncd\nab\ncd", "/b\\nc<CR>", "a|b\ncd\nab\ncd"],
    ["|ab\ncd\nab\ncd", "/b\\nc<CR>n", "ab\ncd\na|b\ncd"],
  ];
  for (const [before, keys, after] of cases) {
    it(`${JSON.stringify(before)} ${keys}`, async () => {
      expect(await vi(before, keys)).toBe(after);
    });
  }

  it("waits for the verdict before moving, and runs the keys typed meanwhile after it", async () => {
    const { doc, m } = setup("|foo bar foo");
    m.feedKeys("/foo<CR>x");
    // Nothing yet: the search and the `x` behind it wait.
    expect(show(doc)).toBe("|foo bar foo");
    await settle();
    expect(show(doc)).toBe("foo bar |oo");
  });

  it("refuses a pattern the worker could not finish in time, and says so", async () => {
    const worker = new FakeSearchWorker((req) => req.source.includes("(a+)+"));
    const { doc, m } = setup("|aaaa b", worker, 20);
    m.feedKeys("/(a+)+b<CR>");
    await new Promise((resolve) => setTimeout(resolve, 60));
    await settle();
    expect(show(doc)).toBe("|aaaa b");
    expect(m.status().message).toMatchObject({ text: expect.stringMatching(/too expensive/), error: true });
    // The machine is not stuck: the next keys work.
    m.feedKeys("w");
    expect(show(doc)).toBe("aaaa |b");
  });

  it("highlights nothing until the verdict is in, then every match", async () => {
    const { m } = setup("|foo x foo");
    m.feedKeys("/foo");
    expect(m.searchHighlights(0, 0).matches.size).toBe(0);
    await settle();
    expect(m.searchHighlights(0, 0).matches.get(0)).toEqual([
      [0, 3],
      [6, 9],
    ]);
    expect(m.searchHighlights(0, 0).current?.start).toEqual({ line: 0, col: 6 });
  });

  it("asks the worker once per pattern and text, however often it is drawn", async () => {
    const { m, worker } = setup("|foo foo foo");
    m.feedKeys("/foo<CR>");
    await settle();
    for (let i = 0; i < 5; i++) m.searchHighlights(0, 0);
    m.feedKeys("n");
    await settle();
    expect(worker.seen.filter((r) => r.source.includes("foo"))).toHaveLength(1);
  });
});

describe("gn and cgn (T12)", () => {
  const cases: Array<[string, string, string]> = [
    // cgn changes the next match; `.` changes the one after.
    ["|foo x foo x foo", "/foo<CR>cgnbar<Esc>", "foo x ba|r x foo"],
    ["|foo x foo x foo", "/foo<CR>cgnbar<Esc>.", "foo x bar x ba|r"],
    // On a match, gn takes that one.
    ["x f|oo foo", "/foo<CR>Ndgn", "x | foo"],
    ["|a foo b foo", "/foo<CR>ggdgN", "a foo b| "],
  ];
  for (const [before, keys, after] of cases) {
    it(`${JSON.stringify(before)} ${keys}`, async () => {
      expect(await vi(before, keys)).toBe(after);
    });
  }

  it("selects the match in Visual mode, and stretches the selection to the next", async () => {
    const { doc, m } = setup("|ab foo cd foo");
    m.feedKeys("/foo<CR>gg");
    await settle();
    m.feedKeys("gn");
    await settle();
    expect(m.status().mode).toBe("visual");
    // The cursor sits on the match's last character, as Visual mode's selection is inclusive.
    expect(doc.selection).toEqual({ anchor: { line: 0, col: 3 }, head: { line: 0, col: 5 } });
    m.feedKeys("gn");
    await settle();
    expect(doc.selection.head).toEqual({ line: 0, col: 12 });
  });

  it("gN in Visual mode stretches backward to the previous match", async () => {
    const { doc, m } = setup("|ab foo cd foo ef foo");
    m.feedKeys("/foo<CR>");
    m.feedKeys("$"); // end of the line, past every match
    await settle();
    m.feedKeys("gN");
    await settle();
    expect(m.status().mode).toBe("visual");
    expect(doc.selection).toEqual({ anchor: { line: 0, col: 19 }, head: { line: 0, col: 17 } });
    m.feedKeys("gN");
    await settle();
    expect(doc.selection.head).toEqual({ line: 0, col: 10 });
  });

  it("gn on the match's own last character still takes that match, not the next one", async () => {
    const { doc, m } = setup("|foo bar foo");
    m.feedKeys("/foo<CR>");
    await settle();
    m.feedKeys("0ll"); // column 2: the last "o" of the first "foo"
    m.feedKeys("gn");
    await settle();
    expect(m.status().mode).toBe("visual");
    expect(doc.selection).toEqual({ anchor: { line: 0, col: 0 }, head: { line: 0, col: 2 } });
  });

  // Found in the ED5.2 review: the anchor was taken unclamped from the match.
  it("gn's Visual anchor for an empty match never lands past the line's last valid column", async () => {
    const { doc, m } = setup("|ab");
    // `/x*/` matches the empty string at every position, including one column past "ab" (col 2).
    m.feedKeys("/x*<CR>");
    await settle();
    m.feedKeys("gn");
    await settle();
    expect(m.status().mode).toBe("visual");
    // A real cursor on "ab" (length 2) never rests past column 1; the anchor should not either.
    expect(doc.selection.anchor.col).toBeLessThanOrEqual(1);
  });
});

describe(":g combined with :s under a real guard (ED5.2)", () => {
  // Found in the ED5.2 review: each line's :s waited for a verdict on text :g itself had just
  // edited, and the replayed <CR> landed in Normal mode. A loop is vouched for once, up front.
  it("applies :s to every marked line, the same as without a guard", async () => {
    const { doc, m } = setup("|a1\na2\na3");
    m.feedKeys(":g/a/s/\\d/N/<CR>");
    await settle();
    await settle();
    expect(show(doc)).toBe("aN\naN\n|aN");
  });
});

describe("a verdict with more matches than it carries (ED5.2)", () => {
  // The worker keeps the first MAX_REPORTED matches but always scans to the end, so a
  // truncated verdict still proves the whole text finishes in time; redraws only look at the
  // lines on screen.
  const text = "a".repeat(120_000);

  it("counts every match, having scanned the whole text", async () => {
    const { m, shared, doc } = setup(`|${text}`);
    m.feedKeys("/\\n?<CR>");
    await settle();
    // Smartcase: a pattern without capitals is compiled case-insensitive (`giu`).
    const verdict = shared.guard!.verdict(doc.store.snapshot(), /\n?/giu);
    expect(verdict).toMatchObject({ ok: true, truncated: true, count: 120_001 });
  });

  it("draws highlights from the window only, never by scanning the whole text again", async () => {
    const { m } = setup(`|${text}`);
    m.feedKeys("/\\n?<CR>");
    await settle();
    const spy = vitest.spyOn(matches, "allMatches");
    m.searchHighlights(0, 0);
    const limits = spy.mock.calls.filter((c) => (c[1] as RegExp).source === "\\n?").map((c) => c[2]);
    expect(limits.every((limit) => limit !== Infinity)).toBe(true);
    spy.mockRestore();
  });
});

describe("loops wait for nothing inside (ED5.2)", () => {
  it("repeats a :g with @: without running the first repeats twice", async () => {
    const { doc, m } = setup("|a\nb");
    m.feedKeys(":g/./normal A!<CR>");
    await settle();
    m.feedKeys("2@:");
    await settle();
    expect(show(doc).replace("|", "")).toBe("a!!!\nb!!!");
  });

  it("runs a ranged :normal whose keys search, on every line", async () => {
    const { doc, m } = setup("|x a\nx a");
    m.feedKeys("/a<CR>gg");
    await settle();
    m.feedKeys(":%normal 0nrA<CR>");
    await settle();
    expect(show(doc).replace("|", "")).toBe("x A\nx A");
  });
});
