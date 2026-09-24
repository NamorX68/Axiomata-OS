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
