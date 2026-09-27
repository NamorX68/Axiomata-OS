/**
 * Folds kept per file across restarts (`docs/plans/editor.md`, ED5, T7):
 * `settings.editor.folds` in `dashboard.json`, keyed `root\0rel`, each a list
 * of `[start, end]` pairs (`FoldState.serialize`).
 *
 * * **Written when folds change** (a chevron, ⌥⌘[, `zc` …) and brought up to
 *   date when the editor leaves the file — edits move folds, too. Leaving
 *   only updates a file that is remembered already, so a closed tab stays
 *   forgotten.
 * * **Tidied with the tabs**: closing a file app tab or an IDE file pane
 *   forgets its folds, and at most {@link LIMIT} files are kept (the least
 *   recently written go first). A file open in a tab *and* an IDE pane loses
 *   only what is remembered when one of them closes — the other keeps its
 *   folds on screen, and the next fold it opens or closes remembers them again.
 */

import { getSetting, setSetting } from "../core/persist";

const KEY = "editor";
/** Files whose folds are kept at most. */
export const LIMIT = 100;

type Folds = Record<string, [number, number][]>;

/** The key a file's folds are kept under. */
export function foldKey(root: string, rel: string): string {
  return `${root}\0${rel}`;
}

function all(): Folds {
  const folds = getSetting<{ folds?: unknown }>(KEY)?.folds;
  return typeof folds === "object" && folds !== null && !Array.isArray(folds) ? (folds as Folds) : {};
}

function write(folds: Folds): void {
  setSetting(KEY, { ...getSetting<Record<string, unknown>>(KEY), folds });
}

/** The folds kept for `key` (unchecked: `FoldState.restore` validates them). */
export function rememberedFolds(key: string): unknown {
  return all()[key];
}

/** Keeps `folds` for `key` as the most recent entry; no folds forget it. */
export function rememberFolds(key: string, folds: [number, number][]): void {
  const { [key]: _old, ...rest } = all();
  if (folds.length === 0) {
    if (_old !== undefined) write(rest);
    return;
  }
  const entries = Object.entries(rest);
  const kept = entries.slice(Math.max(0, entries.length - (LIMIT - 1)));
  write({ ...Object.fromEntries(kept), [key]: folds });
}

/** Brings `key`'s folds up to date, but only if they are kept (the editor leaving a file). */
export function updateRememberedFolds(key: string, folds: [number, number][]): void {
  if (key in all()) rememberFolds(key, folds);
}

/** Forgets `key`'s folds (its tab or pane closed). */
export function forgetFolds(key: string): void {
  const folds = all();
  if (!(key in folds)) return;
  const { [key]: _gone, ...rest } = folds;
  write(rest);
}
