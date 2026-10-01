/**
 * The git panel's frontend side (`docs/plans/editor-projekt-werkzeuge.md`, #48): the `git_*`
 * commands, and loading one file's diff with both whole sides so the editor-based diff view has
 * what it needs. Only this file knows the wire shape (`snake_case`).
 */

import { invokeBackend as invoke, type FileDiff } from "../core/backend";
import type { DiffHunk } from "../editor/diff/hunks";
import { textLines, type DiffSource } from "../editor/diff/model";
import { fileBackend } from "./backend";

export type ChangeKind = "added" | "modified" | "deleted" | "renamed" | "type_changed";

export interface GitEntry {
  path: string;
  old_path: string | null;
  staged: ChangeKind | null;
  unstaged: ChangeKind | null;
  untracked: boolean;
  conflicted: boolean;
}

export interface RepoStatus {
  branch: string | null;
  head: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  entries: GitEntry[];
}

export type GitState = { state: "not_a_repo" } | { state: "ready"; status: RepoStatus };

export type Side = "staged" | "unstaged";

type Blob = { kind: "absent" } | { kind: "too_large"; size: number } | { kind: "binary" } | { kind: "text"; text: string };

export interface GitApi {
  status(root: string): Promise<GitState>;
  stage(root: string, paths: string[]): Promise<void>;
  unstage(root: string, paths: string[]): Promise<void>;
  stageAll(root: string): Promise<void>;
  unstageAll(root: string): Promise<void>;
  diff(root: string, path: string, oldPath: string | null, side: Side): Promise<FileDiff>;
  blob(root: string, path: string, source: "head" | "index"): Promise<Blob>;
  applyHunk(root: string, path: string, oldPath: string | null, side: Side, index: number, header: string): Promise<void>;
  commit(root: string, message: string): Promise<string>;
  fetch(root: string): Promise<void>;
  /** The working file's text, or `null` when it cannot be read as text. */
  workingText(root: string, path: string): Promise<string | null>;
}

export const gitApi: GitApi = {
  status: (root) => invoke<GitState>("git_status", { root }),
  stage: (root, paths) => invoke<void>("git_stage", { root, paths }),
  unstage: (root, paths) => invoke<void>("git_unstage", { root, paths }),
  stageAll: (root) => invoke<void>("git_stage_all", { root }),
  unstageAll: (root) => invoke<void>("git_unstage_all", { root }),
  diff: (root, path, oldPath, side) => invoke<FileDiff>("git_diff", { root, path, oldPath, side }),
  blob: (root, path, source) => invoke<Blob>("git_blob", { root, path, source }),
  applyHunk: (root, path, oldPath, side, index, header) =>
    invoke<void>("git_apply_hunk", { root, path, oldPath, side, index, header }),
  commit: (root, message) => invoke<string>("git_commit", { root, message }),
  fetch: (root) => invoke<void>("git_fetch", { root }),
  workingText: async (root, path) => {
    try {
      return (await fileBackend.read(root, path)).content;
    } catch {
      return null;
    }
  },
};

/** Git's hunks in the engine's words. */
function toHunks(diff: FileDiff): DiffHunk[] {
  return diff.hunks.map((h) => ({
    header: h.header,
    lines: h.lines.map((l) => ({ kind: l.kind, oldLine: l.old_line, newLine: l.new_line, text: l.text })),
  }));
}

/** Everything the diff view needs for one file on one side. */
export interface LoadedGitDiff {
  diff: FileDiff;
  source: DiffSource;
  oldText: string | null;
  newText: string | null;
}

const textOf = (blob: Blob): string | null => (blob.kind === "text" ? blob.text : null);

/**
 * Loads one file's diff with both whole sides: staged is `HEAD` against the index, unstaged is
 * the index against the working file (an untracked file has no index side).
 */
export async function loadGitDiff(
  root: string,
  entry: Pick<GitEntry, "path" | "old_path">,
  side: Side,
  api: GitApi = gitApi,
): Promise<LoadedGitDiff> {
  const diff = await api.diff(root, entry.path, entry.old_path, side);
  // A binary file has no text, and a file too large to diff is not read whole either.
  const wantText = !diff.binary && !(diff.truncated && diff.hunks.length === 0);
  const [oldText, newText] = wantText
    ? await Promise.all([
        side === "staged"
          ? api.blob(root, entry.old_path ?? entry.path, "head").then(textOf)
          : api.blob(root, entry.path, "index").then(textOf),
        side === "staged" ? api.blob(root, entry.path, "index").then(textOf) : api.workingText(root, entry.path),
      ])
    : [null, null];
  return {
    diff,
    source: {
      hunks: toHunks(diff),
      oldLines: oldText === null ? null : textLines(oldText),
      newLines: newText === null ? null : textLines(newText),
      truncated: diff.truncated,
    },
    oldText,
    newText,
  };
}
