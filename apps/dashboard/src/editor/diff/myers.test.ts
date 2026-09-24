import { describe, expect, it } from "vitest";

import { diffSequences, type DiffOp } from "./myers";

/** Applies an edit script to `a`, taking inserted elements from `b` — must give `b`. */
function apply<T>(a: readonly T[], b: readonly T[], ops: readonly DiffOp[]): T[] {
  const out: T[] = [];
  let i = 0;
  let j = 0;
  for (const op of ops) {
    if (op.kind === "equal") {
      for (let n = 0; n < op.count; n++) {
        expect(a[i]).toBe(b[j]);
        out.push(a[i++]);
        j++;
      }
    } else if (op.kind === "delete") i += op.count;
    else for (let n = 0; n < op.count; n++) out.push(b[j++]);
  }
  expect(i).toBe(a.length);
  return out;
}

function cost(ops: readonly DiffOp[]): number {
  return ops.filter((o) => o.kind !== "equal").reduce((sum, o) => sum + o.count, 0);
}

describe("diffSequences", () => {
  it("finds a minimal script and reports it as merged runs", () => {
    const a = [..."ABCABBA"];
    const b = [..."CBABAC"];
    const ops = diffSequences(a, b);
    expect(apply(a, b, ops)).toEqual(b);
    expect(cost(ops)).toBe(5); // the classic example: D = 5
    for (let i = 1; i < ops.length; i++) expect(ops[i].kind).not.toBe(ops[i - 1].kind);
  });

  it("handles empty sides and identical input", () => {
    expect(diffSequences([], [])).toEqual([]);
    expect(diffSequences(["a"], [])).toEqual([{ kind: "delete", count: 1 }]);
    expect(diffSequences([], ["a", "b"])).toEqual([{ kind: "insert", count: 2 }]);
    expect(diffSequences(["x", "y"], ["x", "y"])).toEqual([{ kind: "equal", count: 2 }]);
  });

  it("trims common ends around a change in the middle", () => {
    const a = ["1", "2", "3", "4", "5"];
    const b = ["1", "2", "X", "4", "5"];
    expect(diffSequences(a, b)).toEqual([
      { kind: "equal", count: 2 },
      { kind: "delete", count: 1 },
      { kind: "insert", count: 1 },
      { kind: "equal", count: 2 },
    ]);
  });

  it("stays correct for random input", () => {
    let seed = 7;
    const rand = () => (seed = (seed * 1103515245 + 12345) % 2 ** 31) % 4;
    for (let round = 0; round < 200; round++) {
      const a = Array.from({ length: rand() * 5 }, () => String(rand()));
      const b = Array.from({ length: rand() * 5 }, () => String(rand()));
      expect(apply(a, b, diffSequences(a, b))).toEqual(b);
    }
  });

  it("falls back to one block when the difference exceeds the bound", () => {
    const a = Array.from({ length: 50 }, (_, i) => `a${i}`);
    const b = Array.from({ length: 50 }, (_, i) => `b${i}`);
    const ops = diffSequences(a, b, { maxCost: 10 });
    expect(ops).toEqual([
      { kind: "delete", count: 50 },
      { kind: "insert", count: 50 },
    ]);
    expect(apply(a, b, ops)).toEqual(b);
  });
});
