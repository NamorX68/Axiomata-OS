/**
 * The git layer's frontend side (`docs/plans/git-layer.md`, CP8/CP9): the CP7
 * commands, and loading one file's diff with both whole sides (H2, H8) so the
 * editor-based diff view has what it needs.
 *
 * Only this file knows the wire shape (`snake_case`, git's one-based lines in
 * `GitHunk`); the view works with the editor engine's `DiffHunk`.
 */

import {
  invokeBackend as invoke,
  type AgentDiffState,
  type AgentFileChange,
  type BaseFile,
  type FileDiff,
  type TakeOverMode,
  type TakeOverResult,
} from "../core/backend";
import type { DiffHunk } from "../editor/diff/hunks";
import { textLines, type DiffSource } from "../editor/diff/model";
import { fileBackend } from "../fileapp/backend";

export function agentChanges(id: number): Promise<AgentDiffState> {
  return invoke<AgentDiffState>("ide_agent_changes", { id });
}

export function agentFileDiff(id: number, path: string, oldPath: string | null): Promise<FileDiff> {
  return invoke<FileDiff>("ide_agent_file_diff", { id, path, oldPath });
}

export function agentBaseFile(id: number, path: string): Promise<BaseFile> {
  return invoke<BaseFile>("ide_agent_base_file", { id, path });
}

/** Puts files back to the base (G13) — committed changes included. */
export function agentDiscard(id: number, paths: string[]): Promise<void> {
  return invoke<void>("ide_agent_discard", { id, paths });
}

/** Puts one hunk back (H6); refused unless the file's `index`-th hunk still reads `header`. */
export function agentDiscardHunk(
  id: number,
  path: string,
  oldPath: string | null,
  index: number,
  header: string,
): Promise<void> {
  return invoke<void>("ide_agent_discard_hunk", { id, path, oldPath, index, header });
}

/** Commits everything the agent left uncommitted (G3); resolves to the commit. */
export function agentCommit(id: number, message: string): Promise<string> {
  return invoke<string>("ide_agent_commit", { id, message });
}

/** Takes the agent's committed work over into the project folder (G7–G12). */
export function agentTakeOver(id: number, mode: TakeOverMode, message: string): Promise<TakeOverResult> {
  return invoke<TakeOverResult>("ide_agent_take_over", { id, mode, message });
}

/** The subject of the agent's latest own commit (H10), or `null`. */
export function agentLastSubject(id: number): Promise<string | null> {
  return invoke<string | null>("ide_agent_last_subject", { id });
}

/** The file-service root of an agent's worktree (ED0, E1). */
export function worktreeRoot(agentId: number): string {
  return `worktree:${agentId}`;
}

/** Git's hunks in the engine's words. */
export function toHunks(diff: FileDiff): DiffHunk[] {
  return diff.hunks.map((h) => ({
    header: h.header,
    lines: h.lines.map((l) => ({ kind: l.kind, oldLine: l.old_line, newLine: l.new_line, text: l.text })),
  }));
}

/** Extensions shown as pictures (H8) — the raster types the file service reads as images. */
const IMAGE = /\.(png|jpe?g|gif|webp|bmp|tiff?|heic|heif|avif)$/i;

export function isImagePath(path: string): boolean {
  return IMAGE.test(path);
}

/** Everything the diff view needs for one file. */
export interface LoadedDiff {
  change: AgentFileChange;
  diff: FileDiff;
  /** Hunks plus the whole text of each side that could be read as text. */
  source: DiffSource;
  /** The whole texts, for highlighting each side (H2). */
  oldText: string | null;
  newText: string | null;
  /** `data:` URIs of both sides of a changed picture (H8); `null` for a side without one. */
  images: { old: string | null; new: string | null } | null;
}

/** The base side's text, or `null` when there is none to show as text. */
async function baseText(agentId: number, change: AgentFileChange): Promise<string | null> {
  if (change.kind === "added") return null;
  const base = await agentBaseFile(agentId, change.old_path ?? change.path);
  return base.kind === "text" ? base.text : null;
}

/** The worktree side's text; a file the service will not read as text simply has none. */
async function worktreeText(agentId: number, change: AgentFileChange): Promise<string | null> {
  if (change.kind === "deleted") return null;
  try {
    return (await fileBackend.read(worktreeRoot(agentId), change.path)).content;
  } catch {
    return null;
  }
}

async function images(agentId: number, change: AgentFileChange): Promise<LoadedDiff["images"]> {
  const uri = (mime: string, base64: string) => `data:${mime};base64,${base64}`;
  const [old, now] = await Promise.all([
    change.kind === "added"
      ? null
      : agentBaseFile(agentId, change.old_path ?? change.path).then((b) =>
          b.kind === "image" ? uri(b.mime, b.base64) : null,
        ),
    change.kind === "deleted"
      ? null
      : invoke<{ mime: string; base64: string }>("file_read_image", { root: worktreeRoot(agentId), rel: change.path })
          .then((img) => uri(img.mime, img.base64))
          .catch(() => null),
  ]);
  return { old, new: now };
}

/** Loads one changed file: its hunks, and both sides whole where they are text or pictures. */
export async function loadFileDiff(agentId: number, change: AgentFileChange): Promise<LoadedDiff> {
  const diff = await agentFileDiff(agentId, change.path, change.old_path);
  const picture = (change.binary || diff.binary) && isImagePath(change.path);
  // A binary file has no text, and a file too large to diff is not read whole either.
  const text = !change.binary && !diff.binary && !(diff.truncated && diff.hunks.length === 0);
  const [oldText, newText, pictures] = await Promise.all([
    text ? baseText(agentId, change) : null,
    text ? worktreeText(agentId, change) : null,
    picture ? images(agentId, change) : null,
  ]);
  return {
    change,
    diff,
    source: {
      hunks: toHunks(diff),
      oldLines: oldText === null ? null : textLines(oldText),
      newLines: newText === null ? null : textLines(newText),
      truncated: diff.truncated,
    },
    oldText,
    newText,
    images: pictures,
  };
}

/**
 * Whether two diffs of one file are the same — what a poll compares before it
 * reloads the open file (G4). Field by field with an early exit, not by
 * serialising both: most polls find nothing changed (performance review, CP8).
 */
export function sameFileDiff(a: FileDiff, b: FileDiff): boolean {
  if (a.binary !== b.binary || a.truncated !== b.truncated) return false;
  if (a.old_size !== b.old_size || a.new_size !== b.new_size || a.hunks.length !== b.hunks.length) return false;
  return a.hunks.every((h, i) => {
    const other = b.hunks[i];
    if (h.header !== other.header || h.lines.length !== other.lines.length) return false;
    return h.lines.every((l, j) => {
      const o = other.lines[j];
      return l.kind === o.kind && l.old_line === o.old_line && l.new_line === o.new_line && l.text === o.text;
    });
  });
}
