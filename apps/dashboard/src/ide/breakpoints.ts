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
export function renamed<T>(
  map: Record<string, Record<string, T>>,
  root: string,
  from: string,
  to: string,
): Record<string, Record<string, T>> {
  const files = map[root];
  if (!files) return map;
  const moved: Record<string, T> = {};
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
  loadBreakpointInfo();
}

function save(map: BreakpointMap): void {
  setSetting(KEY, { ...getSetting<Record<string, unknown>>(KEY), breakpoints: map });
}

/** Toggles a breakpoint and remembers it. */
export function toggleBreakpoint(root: string, rel: string, line: number): void {
  const was = (get(breakpoints)[root]?.[rel] ?? []).includes(line);
  const next = toggled(get(breakpoints), root, rel, line);
  breakpoints.set(next);
  save(next);
  // A breakpoint taken away takes its condition with it.
  if (was && infoOf(get(breakpointInfo), root, rel, line)) {
    const gone = withInfo(get(breakpointInfo), root, rel, line, null);
    breakpointInfo.set(gone);
    saveInfo(gone);
  }
}

export function followRename(root: string, from: string, to: string): void {
  const next = renamed(get(breakpoints), root, from, to);
  if (next !== get(breakpoints)) {
    breakpoints.set(next);
    save(next);
  }
  const info = renamed(get(breakpointInfo), root, from, to);
  if (info !== get(breakpointInfo)) {
    breakpointInfo.set(info);
    saveInfo(info);
  }
}

/** Removes every breakpoint (the panel's “remove all”). */
export function clearBreakpoints(root: string): void {
  const { [root]: _gone, ...rest } = get(breakpoints);
  breakpoints.set(rest);
  save(rest);
  const { [root]: _info, ...restInfo } = get(breakpointInfo);
  breakpointInfo.set(restInfo);
  saveInfo(restInfo);
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
export function lineMap(lines: number[], edit: EditSpan, lineCountBefore: number): Array<[number, number | null]> {
  const { start, oldEnd, newEnd } = edit;
  const delta = newEnd.line - oldEnd.line;
  if (start.line === 0 && start.col === 0 && oldEnd.line >= lineCountBefore - 1 && oldEnd.line > 0) {
    // The whole text was replaced: stay put where the line still exists.
    return lines.map((l): [number, number | null] => [l, l - 1 <= newEnd.line ? l : null]);
  }
  const wholeLines = start.col === 0 && oldEnd.col === 0 && oldEnd.line > start.line;
  const pureInsert = oldEnd.line === start.line && oldEnd.col === start.col;
  return lines.map((one): [number, number | null] => {
    const l = one - 1;
    let to: number | null;
    if (l < start.line) to = l;
    else if (wholeLines && l < oldEnd.line) to = null;
    else if (l === start.line) to = pureInsert && start.col === 0 && delta > 0 ? l + delta : l;
    else if (l <= oldEnd.line) to = wholeLines && l === oldEnd.line ? l + delta : null;
    else to = l + delta;
    return [one, to !== null && to >= 0 ? to + 1 : null];
  });
}

/** The lines after the edit — [`lineMap`] without the old ones, ascending and without doubles. */
export function shiftBreakpoints(lines: number[], edit: EditSpan, lineCountBefore: number): number[] {
  const out = new Set<number>();
  for (const [, to] of lineMap(lines, edit, lineCountBefore)) if (to !== null) out.add(to);
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


// ---- conditions, hit counts and log messages -------------------------------------------------------------

/** What turns a breakpoint into a conditional one (each optional; all empty = a plain breakpoint). */
export interface BpInfo {
  /** Stop only when this expression is true. */
  condition?: string;
  /** Stop only on this hit — `5`, `>3`, `% 10` (the adapter's own syntax). */
  hit?: string;
  /** Do not stop; print this instead (`{expression}` placeholders). */
  log?: string;
}

/** `root` → `rel` → one-based line → its extras. */
export type InfoMap = Record<string, Record<string, Record<number, BpInfo>>>;

export const breakpointInfo = writable<InfoMap>({});

const clean = (text: unknown): string | undefined =>
  typeof text === "string" && text.trim() ? text.trim().slice(0, 500) : undefined;

export function parseInfo(raw: unknown): InfoMap {
  const out: InfoMap = {};
  if (typeof raw !== "object" || raw === null) return out;
  for (const [root, files] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof files !== "object" || files === null) continue;
    for (const [rel, lines] of Object.entries(files as Record<string, unknown>)) {
      if (typeof lines !== "object" || lines === null) continue;
      for (const [line, info] of Object.entries(lines as Record<string, unknown>)) {
        const n = Number(line);
        if (!Number.isInteger(n) || n < 1 || typeof info !== "object" || info === null) continue;
        const i = info as Record<string, unknown>;
        const kept: BpInfo = { condition: clean(i.condition), hit: clean(i.hit), log: clean(i.log) };
        if (kept.condition || kept.hit || kept.log) ((out[root] ??= {})[rel] ??= {})[n] = kept;
      }
    }
  }
  return out;
}

export function loadBreakpointInfo(): void {
  breakpointInfo.set(parseInfo(getSetting<{ breakpointInfo?: unknown }>(KEY)?.breakpointInfo));
}

function saveInfo(map: InfoMap): void {
  setSetting(KEY, { ...getSetting<Record<string, unknown>>(KEY), breakpointInfo: map });
}

export function infoOf(map: InfoMap, root: string, rel: string, line: number): BpInfo | undefined {
  return map[root]?.[rel]?.[line];
}

/** `info` as the file `rel`'s extras, replacing what was there; an empty one removes them. */
export function withInfo(map: InfoMap, root: string, rel: string, line: number, info: BpInfo | null): InfoMap {
  const kept = info && (clean(info.condition) || clean(info.hit) || clean(info.log))
    ? { condition: clean(info.condition), hit: clean(info.hit), log: clean(info.log) }
    : null;
  const lines = { ...map[root]?.[rel] };
  if (kept) lines[line] = kept;
  else delete lines[line];
  const files = { ...map[root] };
  if (Object.keys(lines).length > 0) files[rel] = lines;
  else delete files[rel];
  const out = { ...map, [root]: files };
  if (Object.keys(files).length === 0) delete out[root];
  return out;
}

/** Gives the breakpoint at `line` its condition / hit count / log message (sets the breakpoint if it is not there). */
export function setBreakpointInfo(root: string, rel: string, line: number, info: BpInfo | null): void {
  if (!(get(breakpoints)[root]?.[rel] ?? []).includes(line)) toggleBreakpoint(root, rel, line);
  const next = withInfo(get(breakpointInfo), root, rel, line, info);
  breakpointInfo.set(next);
  saveInfo(next);
}

/** What the debugger is told: every file of `root` with its lines and their extras. */
export function specsOf(
  lines: BreakpointMap,
  info: InfoMap,
  root: string,
): { rel: string; breakpoints: BreakpointSpec[] }[] {
  return Object.entries(lines[root] ?? {}).map(([rel, list]) => ({
    rel,
    breakpoints: list.map((line) => specOf(line, info[root]?.[rel]?.[line])),
  }));
}

/** The wire shape of one breakpoint (`axiomata_dap::BreakpointSpec`). */
export interface BreakpointSpec {
  line: number;
  condition: string | null;
  hit_condition: string | null;
  log_message: string | null;
}

export function specOf(line: number, info?: BpInfo): BreakpointSpec {
  return { line, condition: info?.condition ?? null, hit_condition: info?.hit ?? null, log_message: info?.log ?? null };
}

/** `map` with all extras of the file `rel` replaced by `lines` (none = the file has no extras). */
function withFileInfo(map: InfoMap, root: string, rel: string, lines: Record<number, BpInfo>): InfoMap {
  const files = { ...map[root] };
  if (Object.keys(lines).length > 0) files[rel] = lines;
  else delete files[rel];
  const out = { ...map, [root]: files };
  if (Object.keys(files).length === 0) delete out[root];
  return out;
}

/**
 * An edit moved lines of a file: the breakpoints and their extras move with them. Returns the new lines when
 * anything changed, else `null`.
 */
export function applyEdit(root: string, rel: string, edit: EditSpan, lineCountBefore: number): number[] | null {
  const before = get(breakpoints)[root]?.[rel] ?? [];
  if (before.length === 0) return null;
  const moves = lineMap(before, edit, lineCountBefore);
  const after = [...new Set(moves.flatMap(([, to]) => (to === null ? [] : [to])))].sort((a, b) => a - b);
  const oldInfo = get(breakpointInfo)[root]?.[rel] ?? {};
  const hasInfo = Object.keys(oldInfo).length > 0;
  const moved: Record<number, BpInfo> = {};
  for (const [from, to] of moves) if (to !== null && oldInfo[from]) moved[to] = oldInfo[from];
  const sameLines = before.length === after.length && before.every((l, i) => l === after[i]);
  const sameInfo = !hasInfo || Object.entries(oldInfo).every(([l, i]) => moved[Number(l)] === i);
  if (sameLines && sameInfo) return null;
  if (hasInfo) {
    const next = withFileInfo(get(breakpointInfo), root, rel, moved);
    breakpointInfo.set(next);
    saveInfo(next);
  }
  if (!sameLines) setBreakpointLines(root, rel, after);
  return after;
}
