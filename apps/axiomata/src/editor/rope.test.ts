import { describe, expect, it } from "vitest";

import { LineStore } from "./buffer";
import { pos, range, type Pos } from "./position";
import { MAX_LEAF_LINES, Rope, RopeStore } from "./rope";

/** A small deterministic generator, so a failing run can be replayed from its seed. */
function random(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function numbered(n: number): string {
  return Array.from({ length: n }, (_, i) => `line ${i}`).join("\n");
}

function somePos(store: LineStore, rnd: () => number): Pos {
  const line = Math.floor(rnd() * store.lineCount());
  return pos(line, Math.floor(rnd() * (store.line(line).length + 1)));
}

const PIECES = ["", "x", "é", "ab\ncd", "\n", "\n\n\n", "🙂", numbered(40), numbered(150)];

describe("Rope", () => {
  it("builds balanced trees of any size", () => {
    for (const n of [1, 2, MAX_LEAF_LINES, MAX_LEAF_LINES + 1, 1000, 100_000]) {
      const rope = Rope.of(numbered(n));
      rope.checkInvariants();
      expect(rope.lineCount()).toBe(n);
      expect(rope.line(n - 1)).toBe(`line ${n - 1}`);
    }
    // 100 000 lines stay a shallow tree: half-full 16-way branches over 32-line leaves.
    expect(Rope.of(numbered(100_000)).depth()).toBeLessThanOrEqual(5);
  });

  it("matches LineStore through thousands of random edits, staying balanced", () => {
    for (const seed of [1, 2, 3]) {
      const rnd = random(seed);
      const text = numbered(300);
      const lines = new LineStore(text);
      const rope = new RopeStore(text);
      for (let step = 0; step < 1500; step++) {
        let a = somePos(lines, rnd);
        let b = rnd() < 0.3 ? a : somePos(lines, rnd);
        if (a.line > b.line || (a.line === b.line && a.col > b.col)) [a, b] = [b, a];
        // Mostly small edits, now and then a big deletion or paste.
        const big = rnd() < 0.05;
        const r = big ? range(pos(Math.floor(a.line / 2), 0), b) : range(a, b);
        const insert = PIECES[Math.floor(rnd() * PIECES.length)];
        const expected = lines.replace(r, insert);
        expect(rope.replace(r, insert), `seed ${seed} step ${step}`).toEqual(expected);
        rope.snapshot().checkInvariants();
        expect(rope.lineCount()).toBe(lines.lineCount());
        if (step % 50 === 0) {
          expect(rope.text()).toBe(lines.text());
          const p = somePos(lines, rnd);
          expect(rope.offsetAt(p)).toBe(lines.offsetAt(p));
          const q = somePos(lines, rnd);
          const [s, e] = p.line < q.line || (p.line === q.line && p.col <= q.col) ? [p, q] : [q, p];
          expect(rope.slice(range(s, e))).toBe(lines.slice(range(s, e)));
        }
      }
      expect(rope.text()).toBe(lines.text());
    }
  });

  it("leaves an earlier version untouched: a snapshot is a fixed text", () => {
    const store = new RopeStore(numbered(500));
    const before = store.snapshot();
    store.replace(range(pos(10, 0), pos(400, 0)), "gone\n");
    expect(before.lineCount()).toBe(500);
    expect(before.line(200)).toBe("line 200");
    // Lines 0–9, "gone", then 400–499.
    expect(store.lineCount()).toBe(111);
    expect(store.line(10)).toBe("gone");
  });

  it("deletes everything down to one empty line", () => {
    const store = new RopeStore(numbered(5000));
    const last = store.lineCount() - 1;
    store.replace(range(pos(0, 0), pos(last, store.line(last).length)), "");
    store.snapshot().checkInvariants();
    expect(store.lineCount()).toBe(1);
    expect(store.text()).toBe("");
  });

  it("counts its length with each line break as one unit", () => {
    expect(Rope.of("ab\ncdé\n").length()).toBe(7);
    expect(Rope.of("").length()).toBe(0);
  });

  it("holds a single very long line (a minified file) without splitting it", () => {
    const long = "x".repeat(500_000);
    const rope = Rope.of(long);
    rope.checkInvariants();
    expect(rope.lineCount()).toBe(1);
    expect(rope.line(0)).toBe(long);
    expect(rope.slice(range(pos(0, 100_000), pos(0, 100_010)))).toBe(long.slice(100_000, 100_010));
    expect(rope.offsetAt(pos(0, 250_000))).toBe(250_000);
    const { rope: next } = rope.replace(range(pos(0, 100_000), pos(0, 100_000)), "INSERTED");
    next.checkInvariants();
    expect(next.line(0)).toBe(long.slice(0, 100_000) + "INSERTED" + long.slice(100_000));
  });

  it("appends after the very last line of a large, multi-level document", () => {
    const n = 100_000;
    const text = numbered(n);
    const lines = new LineStore(text);
    const rope = new RopeStore(text);
    // 100 000 lines is a multi-level tree (see "builds balanced trees…" above),
    // so the last leaf sits under a chain of branches that are each the last
    // child at their level — exactly the path an append walks.
    expect(rope.snapshot().depth()).toBeGreaterThan(1);
    const end = pos(n - 1, lines.line(n - 1).length);
    const expected = lines.replace(range(end, end), "\nnew last line");
    expect(rope.replace(range(end, end), "\nnew last line")).toEqual(expected);
    rope.snapshot().checkInvariants();
    expect(rope.lineCount()).toBe(n + 1);
    expect(rope.line(n)).toBe("new last line");
    expect(rope.text()).toBe(lines.text());
  });

  it("deletes a range spanning many leaves and whole subtrees", () => {
    const n = 100_000;
    const text = numbered(n);
    const lines = new LineStore(text);
    const rope = new RopeStore(text);
    const deleteRange = range(pos(1000, 2), pos(90_000, 3));
    const expected = lines.replace(deleteRange, "");
    expect(rope.replace(deleteRange, "")).toEqual(expected);
    rope.snapshot().checkInvariants();
    expect(rope.lineCount()).toBe(lines.lineCount());
    expect(rope.text()).toBe(lines.text());
    // The tree shrank along with the content (well below the pre-deletion
    // depth for the full 100 000-line document), not just failed to grow.
    expect(rope.snapshot().depth()).toBeLessThan(5);
  });

  it("lines(from, to) clamps out-of-range bounds instead of throwing, unlike line()", () => {
    const rope = Rope.of("a\nb\nc");
    expect(rope.lines(-5, 1000)).toEqual(["a", "b", "c"]);
    expect(rope.lines(1, 1)).toEqual(["b"]);
    // A reversed range simply visits nothing.
    expect(rope.lines(5, 2)).toEqual([]);
  });

  it("rejects a non-integer line index", () => {
    const rope = Rope.of("a\nb\nc");
    expect(() => rope.line(1.5)).toThrow(RangeError);
    expect(() => rope.line(Number.NaN)).toThrow(RangeError);
  });

  it("never hands out a stale cached text() after a later replace", () => {
    const store = new RopeStore("alpha\nbeta");
    const before = store.snapshot();
    expect(before.text()).toBe("alpha\nbeta"); // populates before's cache
    expect(store.text()).toBe("alpha\nbeta"); // populates the store's current-version cache

    store.replace(range(pos(0, 0), pos(0, 5)), "ALPHA");
    expect(store.text()).toBe("ALPHA\nbeta");
    // The earlier snapshot is a different, immutable Rope: its cache must not
    // have been affected by the edit on the new version.
    expect(before.text()).toBe("alpha\nbeta");
  });
});

describe("Rope.posAt", () => {
  it("is offsetAt's inverse everywhere, across leaves and line breaks", () => {
    const rope = Rope.of(numbered(3000).replace(/line 7/g, "léne 7"));
    const store = new LineStore(rope.text());
    const rnd = random(9);
    for (let i = 0; i < 2000; i++) {
      const p = somePos(store, rnd);
      expect(rope.posAt(rope.offsetAt(p))).toEqual(p);
    }
    expect(rope.posAt(0)).toEqual(pos(0, 0));
    expect(rope.posAt(-5)).toEqual(pos(0, 0));
    const last = rope.lineCount() - 1;
    expect(rope.posAt(rope.length())).toEqual(pos(last, rope.line(last).length));
    expect(rope.posAt(rope.length() + 99)).toEqual(pos(last, rope.line(last).length));
  });

  it("puts the offset of a line break at the end of the line it ends", () => {
    const rope = Rope.of("ab\ncd");
    expect(rope.posAt(2)).toEqual(pos(0, 2));
    expect(rope.posAt(3)).toEqual(pos(1, 0));
  });

  it("resolves offsets right at a leaf boundary to the line either side of it, deterministically", () => {
    // Each line is "line N" (6-7 chars) with MAX_LEAF_LINES = 64: three leaves' worth of lines,
    // so line 63/64 and 127/128 fall exactly on a leaf boundary rather than by random chance.
    const n = MAX_LEAF_LINES * 3;
    const rope = Rope.of(numbered(n));
    rope.checkInvariants();
    for (const boundaryLine of [MAX_LEAF_LINES - 1, MAX_LEAF_LINES, 2 * MAX_LEAF_LINES - 1, 2 * MAX_LEAF_LINES]) {
      // The offset right after the boundary line's own last character (before its line break)
      // resolves to that line's end, not the next line's start.
      const endOfLine = rope.offsetAt(pos(boundaryLine, rope.line(boundaryLine).length));
      expect(rope.posAt(endOfLine)).toEqual(pos(boundaryLine, rope.line(boundaryLine).length));
      // One offset further (past the line break) is the next line's very start.
      expect(rope.posAt(endOfLine + 1)).toEqual(pos(boundaryLine + 1, 0));
      // offsetAt/posAt agree on every column of the boundary line itself.
      for (let col = 0; col <= rope.line(boundaryLine).length; col++) {
        const p = pos(boundaryLine, col);
        expect(rope.posAt(rope.offsetAt(p))).toEqual(p);
      }
    }
  });
});
