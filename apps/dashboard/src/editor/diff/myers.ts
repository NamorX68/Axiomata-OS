/**
 * The difference between two sequences (`docs/plans/git-layer.md`, H4, H7):
 * Myers' O(ND) algorithm, used for the words inside a changed line and for
 * comparing the editor's text with the file on disk. An agent's diff does not
 * come from here — git computes it (CP7), so the view agrees with `+n −m`.
 *
 * * **Common ends are trimmed first**, so an edit in the middle of a long file
 *   costs as much as the edit, not the file.
 * * **Bounded.** Past `maxCost` differences the rest is reported as one block
 *   removed and one inserted: still a correct diff, just not a minimal one —
 *   better than a frozen window on two unrelated files.
 */

/** A run of the edit script: `equal` consumes both sides, `delete` only `a`, `insert` only `b`. */
export interface DiffOp {
  kind: "equal" | "delete" | "insert";
  count: number;
}

export interface DiffOptions {
  /** The most differences searched for before giving up on a minimal diff. */
  maxCost?: number;
}

/** The default bound: a thousand edits is far past anything worth diffing finely. */
const DEFAULT_MAX_COST = 1000;

/** Appends `count` of `kind`, merging with the run before it. */
function push(ops: DiffOp[], kind: DiffOp["kind"], count: number): void {
  if (count <= 0) return;
  const last = ops[ops.length - 1];
  if (last?.kind === kind) last.count += count;
  else ops.push({ kind, count });
}

/** The edit script turning `a` into `b`, as runs in order. Elements compare with `===`. */
export function diffSequences<T>(a: readonly T[], b: readonly T[], options: DiffOptions = {}): DiffOp[] {
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const ops: DiffOp[] = [];
  push(ops, "equal", start);
  for (const op of middle(a.slice(start, endA), b.slice(start, endB), options.maxCost ?? DEFAULT_MAX_COST)) {
    push(ops, op.kind, op.count);
  }
  push(ops, "equal", a.length - endA);
  return ops;
}

/** Myers on what trimming left over; the fallback block when it costs too much. */
function middle<T>(a: readonly T[], b: readonly T[], maxCost: number): DiffOp[] {
  const n = a.length;
  const m = b.length;
  if (n === 0 || m === 0) {
    const ops: DiffOp[] = [];
    push(ops, "delete", n);
    push(ops, "insert", m);
    return ops;
  }
  const limit = Math.min(n + m, Math.max(1, maxCost));
  const offset = limit + 1;
  const v = new Int32Array(2 * limit + 3);
  // trace[d] holds the furthest x on each diagonal k ∈ [-d, d] after step d.
  const trace: Int32Array[] = [];
  for (let d = 0; d <= limit; d++) {
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1]) ? v[offset + k + 1] : v[offset + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) {
        x++;
        y++;
      }
      v[offset + k] = x;
      if (x >= n && y >= m) {
        trace.push(v.slice(offset - d, offset + d + 1));
        return backtrack(trace, n, m);
      }
    }
    trace.push(v.slice(offset - d, offset + d + 1));
  }
  return [
    { kind: "delete", count: n },
    { kind: "insert", count: m },
  ];
}

/** Walks the trace back from the end, collecting single steps, and returns them as runs. */
function backtrack(trace: readonly Int32Array[], n: number, m: number): DiffOp[] {
  const steps: DiffOp["kind"][] = [];
  let x = n;
  let y = m;
  for (let d = trace.length - 1; d > 0; d--) {
    const prev = trace[d - 1];
    const at = (k: number) => prev[k + d - 1];
    const k = x - y;
    const prevK = k === -d || (k !== d && at(k - 1) < at(k + 1)) ? k + 1 : k - 1;
    const prevX = at(prevK);
    const prevY = prevX - prevK;
    while (x > prevX && y > prevY) {
      steps.push("equal");
      x--;
      y--;
    }
    if (x === prevX) {
      steps.push("insert");
      y--;
    } else {
      steps.push("delete");
      x--;
    }
  }
  while (x > 0 && y > 0) {
    steps.push("equal");
    x--;
    y--;
  }
  const ops: DiffOp[] = [];
  for (let i = steps.length - 1; i >= 0; i--) push(ops, steps[i], 1);
  return ops;
}
