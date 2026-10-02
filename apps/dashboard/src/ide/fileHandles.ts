/**
 * What the dock can ask of an open file pane without owning its editor: is there unsaved text, save
 * it, throw it away. A file pane registers itself under its tab's id while it is mounted
 * (`panes/FilePane.svelte`); closing a tab asks here first (`IdeView.requestClose`).
 */

export interface FileHandle {
  hasUnsaved: () => boolean;
  /** `false` when the save did not happen (a conflict, a failure) — the tab then stays open. */
  saveNow: () => Promise<boolean>;
  discard: () => Promise<void>;
}

const handles = new Map<string, FileHandle>();

/** Registers `handle` for `tabId`; the returned function removes it again. */
export function registerFileHandle(tabId: string, handle: FileHandle): () => void {
  handles.set(tabId, handle);
  return () => {
    if (handles.get(tabId) === handle) handles.delete(tabId);
  };
}

export function fileHandle(tabId: string): FileHandle | null {
  return handles.get(tabId) ?? null;
}

/** A live session handed over from the floating panel, waiting for its tab's pane to take it (`fileapp/handoff.ts`). */
export interface Handover {
  session: import("../fileapp/session").FileSession;
  viewMode: import("../fileapp/fileKinds").ViewMode;
}

const handovers = new Map<string, Handover>();

export function stashHandover(tabId: string, handed: Handover): void {
  handovers.set(tabId, handed);
}

/** The session waiting for `tabId`, once. */
export function takeHandover(tabId: string): Handover | null {
  const handed = handovers.get(tabId) ?? null;
  handovers.delete(tabId);
  return handed;
}
