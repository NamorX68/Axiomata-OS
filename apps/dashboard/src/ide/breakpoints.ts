/**
 * Breakpoints as the editor keeps them (`docs/plans/editor-projekt-werkzeuge.md`, #51): per project root, per
 * file, a set of lines — **one-based**, as DAP and the owner count them. Remembered across restarts in
 * `settings.ide.breakpoints`; handed to a session when it starts and to the running one when they change.
 *
 * A breakpoint is a line number, not a marker that follows the text: editing above it leaves it where it was.
 * That is how a first version stays simple; the adapter reports where it actually landed.
 */

import { writable, get } from "svelte/store";

import { getSetting, setSetting } from "../core/persist";

/** `root` → `rel` → ascending one-based lines. */
export type BreakpointMap = Record<string, Record<string, number[]>>;

const KEY = "ide";

export const breakpoints = writable<BreakpointMap>({});

/** Reads what was saved; anything malformed is dropped rather than trusted. */
export function parseBreakpoints(raw: unknown): BreakpointMap {
  const out: BreakpointMap = {};
  if (typeof raw !== "object" || raw === null) return out;
  for (const [root, files] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof files !== "object" || files === null) continue;
    for (const [rel, lines] of Object.entries(files as Record<string, unknown>)) {
      if (!Array.isArray(lines)) continue;
      const clean = [...new Set(lines.filter((l): l is number => Number.isInteger(l) && l >= 1))].sort((a, b) => a - b);
      if (clean.length > 0) (out[root] ??= {})[rel] = clean;
    }
  }
  return out;
}

/** `map` with the breakpoint on `line` toggled. */
export function toggled(map: BreakpointMap, root: string, rel: string, line: number): BreakpointMap {
  const current = map[root]?.[rel] ?? [];
  const next = current.includes(line) ? current.filter((l) => l !== line) : [...current, line].sort((a, b) => a - b);
  const files = { ...map[root] };
  if (next.length > 0) files[rel] = next;
  else delete files[rel];
  const out = { ...map, [root]: files };
  if (Object.keys(files).length === 0) delete out[root];
  return out;
}

/** The lines of one file, as a set. */
export function linesOf(map: BreakpointMap, root: string, rel: string): ReadonlySet<number> {
  return new Set(map[root]?.[rel] ?? []);
}

/** Every file of `root` with its lines — what a session starts with. */
export function filesOf(map: BreakpointMap, root: string): { rel: string; lines: number[] }[] {
  return Object.entries(map[root] ?? {}).map(([rel, lines]) => ({ rel, lines }));
}

/** After a rename in the tree, the breakpoints go with the file (or the folder). */
export function renamed(map: BreakpointMap, root: string, from: string, to: string): BreakpointMap {
  const files = map[root];
  if (!files) return map;
  const moved: Record<string, number[]> = {};
  let changed = false;
  for (const [rel, lines] of Object.entries(files)) {
    if (rel === from || rel.startsWith(`${from}/`)) {
      moved[to + rel.slice(from.length)] = lines;
      changed = true;
    } else {
      moved[rel] = lines;
    }
  }
  return changed ? { ...map, [root]: moved } : map;
}

export function loadBreakpoints(): void {
  breakpoints.set(parseBreakpoints(getSetting<{ breakpoints?: unknown }>(KEY)?.breakpoints));
}

function save(map: BreakpointMap): void {
  setSetting(KEY, { ...getSetting<Record<string, unknown>>(KEY), breakpoints: map });
}

/** Toggles a breakpoint and remembers it. */
export function toggleBreakpoint(root: string, rel: string, line: number): void {
  const next = toggled(get(breakpoints), root, rel, line);
  breakpoints.set(next);
  save(next);
}

export function followRename(root: string, from: string, to: string): void {
  const next = renamed(get(breakpoints), root, from, to);
  if (next !== get(breakpoints)) {
    breakpoints.set(next);
    save(next);
  }
}

/** Removes every breakpoint (the panel's “remove all”). */
export function clearBreakpoints(root: string): void {
  const { [root]: _gone, ...rest } = get(breakpoints);
  breakpoints.set(rest);
  save(rest);
}

/** A replacement as the editor's document reports it (`TextChange`); positions are zero-based. */
export interface EditSpan {
  start: { line: number; col: number };
  oldEnd: { line: number; col: number };
  newEnd: { line: number; col: number };
}

/**
 * Where the breakpoints (one-based `lines`) of a file stand after one edit — so they stay on the code they
 * were set on while you type above them.
 *
 *  - Lines before the edit stay; lines after it move by the number of lines it added or removed.
 *  - Enter at the very start of a line pushes that line (and its breakpoint) down, as in other IDEs; Enter
 *    elsewhere in a line leaves the breakpoint on the line it was set on.
 *  - Lines deleted outright take their breakpoints with them; lines merged into the one before lose theirs.
 *  - Replacing the whole document (a reload, a formatter) keeps the breakpoints that still fit on a line.
 */
export function shiftBreakpoints(lines: number[], edit: EditSpan, lineCountBefore: number): number[] {
  const { start, oldEnd, newEnd } = edit;
  const delta = newEnd.line - oldEnd.line;
  if (start.line === 0 && start.col === 0 && oldEnd.line >= lineCountBefore - 1 && oldEnd.line > 0) {
    // The whole text was replaced: stay put where the line still exists.
    return lines.filter((l) => l - 1 <= newEnd.line);
  }
  const wholeLines = start.col === 0 && oldEnd.col === 0 && oldEnd.line > start.line;
  const pureInsert = oldEnd.line === start.line && oldEnd.col === start.col;
  const out = new Set<number>();
  for (const one of lines) {
    const l = one - 1;
    let to: number | null;
    if (l < start.line) to = l;
    else if (wholeLines && l < oldEnd.line) to = null;
    else if (l === start.line) to = pureInsert && start.col === 0 && delta > 0 ? l + delta : l;
    else if (l <= oldEnd.line) to = wholeLines && l === oldEnd.line ? l + delta : null;
    else to = l + delta;
    if (to !== null && to >= 0) out.add(to + 1);
  }
  return [...out].sort((a, b) => a - b);
}

/** Replaces one file's breakpoints (kept sorted, an empty list removes the file), and remembers them. */
export function setBreakpointLines(root: string, rel: string, lines: number[]): void {
  const map = get(breakpoints);
  const before = map[root]?.[rel] ?? [];
  if (before.length === lines.length && before.every((l, i) => l === lines[i])) return;
  const files = { ...map[root] };
  if (lines.length > 0) files[rel] = lines;
  else delete files[rel];
  const next = { ...map, [root]: files };
  if (Object.keys(files).length === 0) delete next[root];
  breakpoints.set(next);
  save(next);
}
