/**
 * Which files have unsaved changes in some open editor — a file app tab, the
 * floating panel or an IDE file pane (`docs/plans/editor.md`, ED5, T14): the
 * project search marks them, since what it lists is the file on disk.
 *
 * Each `FileEditor` reports its own file (`markDirty`) and takes it back when
 * it leaves it; a file open in two editors stays marked while either is dirty.
 */

import { derived, writable, type Readable } from "svelte/store";

import { foldKey } from "./foldMemory";

/** Per editor (an id of its own), the file it has unsaved changes in. */
const byEditor = writable(new Map<string, string>());

/** Every `root\0rel` with unsaved changes somewhere. */
export const dirtyFiles: Readable<Set<string>> = derived(byEditor, (m) => new Set(m.values()));

/** Editor `editor` has unsaved changes in `root`/`rel` — or none (`null`). */
export function markDirty(editor: string, file: { root: string; rel: string } | null): void {
  byEditor.update((m) => {
    const key = file ? foldKey(file.root, file.rel) : null;
    if ((m.get(editor) ?? null) === key) return m;
    const next = new Map(m);
    if (key) next.set(editor, key);
    else next.delete(editor);
    return next;
  });
}

/** Whether `root`/`rel` is in `dirty`. */
export function isDirty(dirty: Set<string>, root: string, rel: string): boolean {
  return dirty.has(foldKey(root, rel));
}
