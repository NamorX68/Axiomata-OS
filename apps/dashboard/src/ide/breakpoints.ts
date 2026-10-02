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
