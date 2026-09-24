/**
 * "Recently opened" for the file app (`docs/plans/editor.md`, F7), kept under
 * `settings.editor.recent` in `dashboard.json` (the same `getSetting` path the
 * Kanban and Second-Brain preferences use). A file is remembered by root id and
 * path only — the file service decides again at every open whether it may be
 * read, so a stale entry just fails to open and is then forgotten.
 */

import { getSetting, setSetting } from "../core/persist";

export interface RecentFile {
  root: string;
  rel: string;
}

/** How many files the list keeps. */
export const MAX_RECENT = 12;

const KEY = "editor";

interface EditorPrefs {
  recent?: RecentFile[];
}

function isRecent(value: unknown): value is RecentFile {
  const v = value as RecentFile;
  return typeof v === "object" && v !== null && typeof v.root === "string" && typeof v.rel === "string";
}

/** `file` moved to the front of `list`, without duplicates, capped. */
export function withRecent(list: readonly RecentFile[], file: RecentFile): RecentFile[] {
  const rest = list.filter((f) => f.root !== file.root || f.rel !== file.rel);
  return [{ root: file.root, rel: file.rel }, ...rest].slice(0, MAX_RECENT);
}

export function recentFiles(): RecentFile[] {
  const prefs = getSetting<EditorPrefs>(KEY);
  return Array.isArray(prefs?.recent) ? prefs.recent.filter(isRecent).slice(0, MAX_RECENT) : [];
}

export function rememberRecent(file: RecentFile): void {
  setSetting(KEY, { ...getSetting<EditorPrefs>(KEY), recent: withRecent(recentFiles(), file) });
}

export function forgetRecent(file: RecentFile): void {
  const recent = recentFiles().filter((f) => f.root !== file.root || f.rel !== file.rel);
  setSetting(KEY, { ...getSetting<EditorPrefs>(KEY), recent });
}
