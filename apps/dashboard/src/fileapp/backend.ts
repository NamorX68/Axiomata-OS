/**
 * The file app's backend: `FileSession`'s calls mapped onto the Tauri commands
 * (`src-tauri/src/files.rs`), with the DEV browser mock behind `invokeBackend`.
 */

import {
  invokeBackend,
  type EditorRecovery,
  type FileRootInfo,
  type FileVersion,
  type PickedFile,
  type TextFile,
} from "../core/backend";
import type { FileBackend } from "./session";

export const fileBackend: FileBackend = {
  read: (root, rel) => invokeBackend<TextFile>("file_read", { root, rel }),
  write: (root, rel, content, expected) => invokeBackend<FileVersion>("file_write", { root, rel, content, expected }),
  watch: (root, rel) => invokeBackend<void>("file_watch", { root, rel }),
  unwatch: (root, rel) => invokeBackend<void>("file_unwatch", { root, rel }),
  recoverySave: (root, rel, base, content) => invokeBackend<void>("editor_recovery_save", { root, rel, base, content }),
  recoveryLoad: (root, rel) => invokeBackend<EditorRecovery | null>("editor_recovery_load", { root, rel }),
  recoveryDelete: (root, rel) => invokeBackend<void>("editor_recovery_delete", { root, rel }),
};

/** The native open dialog (driven from Rust); `null` when cancelled. */
export function pickFile(): Promise<PickedFile | null> {
  return invokeBackend<PickedFile | null>("file_pick", { folder: false });
}

export function listRoots(): Promise<FileRootInfo[]> {
  return invokeBackend<FileRootInfo[]>("file_roots");
}

/** One entry of a listed folder (`file_list`, editor plan W6). */
export interface DirEntry {
  name: string;
  kind: "file" | "dir" | "link" | "other";
  size: number | null;
  /** A `.gitignore` ignores it: shown greyed out. */
  ignored: boolean;
}

export interface Listing {
  entries: DirEntry[];
  truncated: boolean;
}

/** The folder `rel` of `root` (`""`: the root itself) — folders first. */
export function listDir(root: string, rel: string): Promise<Listing> {
  return invokeBackend<Listing>("file_list", { root, rel });
}

export function makeDir(root: string, rel: string): Promise<void> {
  return invokeBackend<void>("file_mkdir", { root, rel });
}

/** Renames or moves inside one root; refused when something is already at `to`. */
export function renameEntry(root: string, from: string, to: string): Promise<void> {
  return invokeBackend<void>("file_rename", { root, from, to });
}

/** How many entries the folder `rel` holds (for the question before deleting it). */
export function countTree(root: string, rel: string): Promise<number> {
  return invokeBackend<number>("file_count", { root, rel });
}

/** Deletes a folder with everything in it, or a file. */
export function deleteTree(root: string, rel: string): Promise<void> {
  return invokeBackend<void>("file_delete_tree", { root, rel });
}

/** A new, empty file; refused if something is already there — never an overwrite (`O_EXCL` in Rust). */
export function createFile(root: string, rel: string): Promise<FileVersion> {
  return invokeBackend<FileVersion>("file_create", { root, rel });
}

/** `files:renamed`: `from` in `root` is now `to` — sent the moment a rename succeeded (W13). */
export interface FileRenamed {
  root: string;
  from: string;
  to: string;
}

/** `files:removed`: `rel` in `root`, and everything under it, is gone. */
export interface FileRemoved {
  root: string;
  rel: string;
}
