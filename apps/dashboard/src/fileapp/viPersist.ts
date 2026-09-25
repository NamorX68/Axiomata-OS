/**
 * What Vi remembers across restarts (`docs/plans/editor.md`, ED3, V4), as the
 * JSON of `~/.axiomata/editor-vi.json`: the named registers `a`–`z` (and so
 * the macros in them), the file marks `A`–`Z`, the search and command-line
 * histories and the last search.
 *
 * The file is the frontend's, like `editor-settings.json`: Rust only checks
 * "object with a numeric version". Everything read back is validated field by
 * field here — a hand-edited or half-broken file loses what is wrong with it,
 * never the rest.
 *
 * A file mark keeps its root id (`project:3`, `worktree:7` …) even after that
 * project or worktree is gone; it then simply never matches an open file again
 * and its letter is overwritten the next time it is set. At most 26 of them.
 */

import type { ViShared } from "../editor/vi/machine";
import type { RegisterContent, RegisterKind } from "../editor/vi/registers";

export const VI_STATE_VERSION = 1;

/** A register larger than this is neither written to the file nor read back from it. */
export const MAX_PERSISTED_REGISTER = 256 * 1024;

/**
 * All registers together, at most. The file must stay well under `json_state`'s 4 MiB, or the
 * next start would set it aside as corrupt and forget everything in it.
 */
export const MAX_PERSISTED_REGISTERS_TOTAL = 2 * 1024 * 1024;

/** How many entries of each history are kept in the file, and how long one may be. */
const HISTORY_KEPT = 100;
const MAX_HISTORY_ENTRY = 4 * 1024;

export interface ViPersisted {
  version: number;
  registers: Record<string, RegisterContent>;
  fileMarks: Record<string, { file: string; line: number; col: number }>;
  history: { cmd: string[]; search: string[] };
  lastSearch: { pattern: string; backward: boolean } | null;
}

const KINDS = new Set<RegisterKind>(["char", "line", "block"]);

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isCount(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0;
}

/** What `shared` holds that is worth keeping. */
export function snapshotVi(shared: ViShared): ViPersisted {
  const registers: Record<string, RegisterContent> = {};
  let total = 0;
  for (const [name, content] of Object.entries(shared.registers.named()).sort(([a], [b]) => a.localeCompare(b))) {
    const size = content.text.length;
    if (size > MAX_PERSISTED_REGISTER || total + size > MAX_PERSISTED_REGISTERS_TOTAL) continue;
    registers[name] = content;
    total += size;
  }
  const keep = (list: string[]) => list.filter((entry) => entry.length <= MAX_HISTORY_ENTRY).slice(-HISTORY_KEPT);
  const fileMarks: ViPersisted["fileMarks"] = {};
  for (const [name, { file, at }] of shared.fileMarks) fileMarks[name] = { file, line: at.line, col: at.col };
  const search = shared.search;
  return {
    version: VI_STATE_VERSION,
    registers,
    fileMarks,
    history: { cmd: keep(search.history.cmd), search: keep(search.history.search) },
    lastSearch: search.last,
  };
}

/** Puts what `raw` (the parsed file) validly holds back into `shared`; anything malformed is skipped. */
export function restoreVi(shared: ViShared, raw: unknown): void {
  if (!isObject(raw)) return;
  if (isObject(raw.registers)) {
    const named: Record<string, RegisterContent> = {};
    for (const [name, value] of Object.entries(raw.registers)) {
      if (!/^[a-z]$/.test(name) || !isObject(value)) continue;
      if (typeof value.text !== "string" || !KINDS.has(value.kind as RegisterKind)) continue;
      // What the app would never have written (a hand-edited file) is not taken either.
      if (value.text.length > MAX_PERSISTED_REGISTER) continue;
      named[name] = { text: value.text, kind: value.kind as RegisterKind };
    }
    shared.registers.restoreNamed(named);
  }
  if (isObject(raw.fileMarks)) {
    for (const [name, value] of Object.entries(raw.fileMarks)) {
      if (!/^[A-Z]$/.test(name) || !isObject(value)) continue;
      if (typeof value.file !== "string" || !isCount(value.line) || !isCount(value.col)) continue;
      shared.fileMarks.set(name, { file: value.file, at: { line: value.line, col: value.col } });
    }
  }
  const search = shared.search;
  if (isObject(raw.history)) {
    for (const which of ["cmd", "search"] as const) {
      const list = raw.history[which];
      if (!Array.isArray(list)) continue;
      for (const entry of list.slice(-HISTORY_KEPT)) {
        if (typeof entry === "string" && entry.length <= MAX_HISTORY_ENTRY) search.remember(which, entry);
      }
    }
  }
  const last = raw.lastSearch;
  if (isObject(last) && typeof last.pattern === "string" && last.pattern && typeof last.backward === "boolean") {
    search.last = { pattern: last.pattern, backward: last.backward };
    // As after `:noh`: `n` finds it again, but nothing lights up just from opening a file.
    search.highlight = false;
  }
}
