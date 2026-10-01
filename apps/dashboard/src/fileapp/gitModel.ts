/**
 * What the git panel lists, as plain functions (`docs/plans/editor-projekt-werkzeuge.md`, #48):
 * the two groups, one row per path and group, and the letter that tells the change.
 */

import type { ChangeKind, GitEntry, Side } from "./gitBackend";

export interface GitRow {
  entry: GitEntry;
  side: Side;
  kind: ChangeKind;
  /** Not tracked yet — only in the unstaged group. */
  untracked: boolean;
}

export interface GitGroups {
  /** What the next commit contains. */
  staged: GitRow[];
  /** What is changed or new but not staged (conflicts first, since they need doing first). */
  changes: GitRow[];
}

/** Splits entries into the two groups; a path changed on both sides is in both. */
export function groupEntries(entries: readonly GitEntry[]): GitGroups {
  const staged: GitRow[] = [];
  const changes: GitRow[] = [];
  for (const entry of entries) {
    if (entry.staged !== null) staged.push({ entry, side: "staged", kind: entry.staged, untracked: false });
    if (entry.unstaged !== null || entry.untracked || entry.conflicted) {
      changes.push({ entry, side: "unstaged", kind: entry.unstaged ?? "modified", untracked: entry.untracked });
    }
  }
  changes.sort((a, b) => Number(b.entry.conflicted) - Number(a.entry.conflicted));
  return { staged, changes };
}

const LETTERS: Record<ChangeKind, string> = { added: "A", modified: "M", deleted: "D", renamed: "R", type_changed: "T" };

/** The one-letter mark of a row: `U` for an untracked file, `!` for a conflict. */
export function markOf(row: GitRow): string {
  if (row.entry.conflicted) return "!";
  return row.untracked ? "U" : LETTERS[row.kind];
}

/** The file's name and the folder it is in, for a compact row. */
export function splitPath(path: string): { name: string; dir: string } {
  const at = path.lastIndexOf("/");
  return at < 0 ? { name: path, dir: "" } : { name: path.slice(at + 1), dir: path.slice(0, at) };
}

/** Whether the commit button can act: something staged, a message typed. */
export function canCommit(groups: GitGroups, message: string): boolean {
  return groups.staged.length > 0 && message.trim() !== "";
}
