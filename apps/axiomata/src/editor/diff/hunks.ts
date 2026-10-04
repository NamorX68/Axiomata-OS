/**
 * Hunks — the unit a diff comes in (`docs/plans/git-layer.md`, H2, H7). The
 * shape mirrors what the git engine parses (`axiomata-ide`'s `FileDiff`), in
 * the engine's own words, so the view does not care whether a diff came from
 * git or from comparing two texts here.
 *
 * Line numbers are **one-based**, as git prints them; the model turns them into
 * indices.
 */

import { diffSequences, type DiffOptions } from "./myers";

export type DiffLineKind = "context" | "add" | "remove" | "no_newline";

export interface DiffHunkLine {
  kind: DiffLineKind;
  /** On the base side; `null` for an added line. */
  oldLine: number | null;
  /** On the changed side; `null` for a removed line. */
  newLine: number | null;
  text: string;
}

export interface DiffHunk {
  /** The whole `@@ -a,b +c,d @@ context` line. */
  header: string;
  lines: DiffHunkLine[];
}

/** What a hunk header says: where each side starts and how many lines it covers. */
export interface HunkRange {
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
  /** The text after the second `@@` — usually the enclosing function. */
  context: string;
}

const HEADER = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@ ?(.*)$/;

/** Reads a hunk header; `null` if it is not one. A missing count means 1. */
export function parseHunkHeader(header: string): HunkRange | null {
  const m = HEADER.exec(header);
  if (!m) return null;
  return {
    oldStart: Number(m[1]),
    oldCount: m[2] === undefined ? 1 : Number(m[2]),
    newStart: Number(m[3]),
    newCount: m[4] === undefined ? 1 : Number(m[4]),
    context: m[5].trim(),
  };
}

/** Lines of unchanged context kept around each change, as git's `-U3`. */
export const DEFAULT_CONTEXT = 3;

/** One side of a range for a header: git names the line *before* an empty range. */
function headerRange(first: number, count: number): string {
  const start = count === 0 ? first - 1 : first;
  return count === 1 ? String(start) : `${start},${count}`;
}

/** One entry per line of the edit script, with both sides' indices. */
type Step = { kind: "context" | "add" | "remove"; old: number; new: number };

/** Runs `diffSequences` and turns its ops into one step per line, both sides' indices tracked. */
function stepsFromTexts(oldLines: readonly string[], newLines: readonly string[], options: DiffOptions): Step[] {
  const steps: Step[] = [];
  let i = 0;
  let j = 0;
  for (const op of diffSequences(oldLines, newLines, options)) {
    for (let n = 0; n < op.count; n++) {
      if (op.kind === "equal") steps.push({ kind: "context", old: i++, new: j++ });
      else if (op.kind === "delete") steps.push({ kind: "remove", old: i++, new: j });
      else steps.push({ kind: "add", old: i, new: j++ });
    }
  }
  return steps;
}

/** Groups the changed steps into windows `[from, to)` with `context` lines either side; windows closer than
 *  twice that (their context overlaps) merge into one, as git does. */
function windowsAroundChanges(steps: readonly Step[], context: number): { from: number; to: number }[] {
  const windows: { from: number; to: number }[] = [];
  for (let s = 0; s < steps.length; s++) {
    if (steps[s].kind === "context") continue;
    const from = Math.max(0, s - context);
    let end = s;
    while (end + 1 < steps.length && steps[end + 1].kind !== "context") end++;
    const to = Math.min(steps.length, end + 1 + context);
    const last = windows[windows.length - 1];
    if (last && from <= last.to) last.to = to;
    else windows.push({ from, to });
    s = end;
  }
  return windows;
}

/** Turns one window of steps into a hunk, header and all. */
function hunkFromWindow(
  steps: readonly Step[],
  oldLines: readonly string[],
  newLines: readonly string[],
  from: number,
  to: number,
): DiffHunk {
  const lines: DiffHunkLine[] = steps.slice(from, to).map((st) => ({
    kind: st.kind,
    oldLine: st.kind === "add" ? null : st.old + 1,
    newLine: st.kind === "remove" ? null : st.new + 1,
    text: st.kind === "add" ? newLines[st.new] : oldLines[st.old],
  }));
  const oldCount = lines.filter((l) => l.kind !== "add").length;
  const newCount = lines.filter((l) => l.kind !== "remove").length;
  const oldFirst = steps[from].old + 1;
  const newFirst = steps[from].new + 1;
  return {
    header: `@@ -${headerRange(oldFirst, oldCount)} +${headerRange(newFirst, newCount)} @@`,
    lines,
  };
}

/**
 * The hunks between two texts given as lines (H7: the editor's text against the
 * file on disk), with `context` unchanged lines around each change; changes
 * closer than twice that share a hunk, as in git.
 */
export function hunksFromTexts(
  oldLines: readonly string[],
  newLines: readonly string[],
  context = DEFAULT_CONTEXT,
  options: DiffOptions = {},
): DiffHunk[] {
  const steps = stepsFromTexts(oldLines, newLines, options);
  const windows = windowsAroundChanges(steps, context);
  return windows.map(({ from, to }) => hunkFromWindow(steps, oldLines, newLines, from, to));
}
