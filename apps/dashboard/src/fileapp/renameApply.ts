/**
 * Carrying out a rename across files (`docs/plans/editor.md`, ED6.5, L16):
 *
 * * **A file open in an editor** is changed in its document — one undo step,
 *   left unsaved like any edit. The editor the rename started in applies it
 *   itself (`applyHere`, through its surface); another editor's document is
 *   changed here and its surface told (`docTouched`).
 * * **A file not open** is read and written through the file service, with
 *   the version it was read at — a file that changed in between is left
 *   alone and reported, never overwritten.
 * * **A file outside every root** (a server's read-only place) is not changed.
 *
 * The server's edits are in the coordinates of the text it has: a document's
 * text as sent (`textForSave`), a closed file's text on disk.
 */

import { writable } from "svelte/store";

import { run, UNWRAPPED } from "../editor/commands";
import { bodyForStore } from "../editor/detect";
import type { Change, EditorDocument } from "../editor/document";
import { applyTextEdits, textChanges, type TextEdit } from "../editor/textEdits";
import type { FileVersion } from "../core/backend";

/** Bumped when a document was changed from outside its editor, so every surface redraws if it is its own. */
export const docTouched = writable(0);

export interface RenamePorts {
  /** The document of the editor the rename started in, and how it applies changes (its surface). */
  here: { uri: string; doc: EditorDocument; apply(changes: Change[]): void };
  /** Another editor's document for `uri`, if one has it open. */
  openDoc(uri: string): EditorDocument | null;
  /** The file for `uri` in a root the editor may write; `null` outside every root. */
  fileOf(uri: string): { root: string; rel: string } | null;
  read(root: string, rel: string): Promise<{ content: string; version: FileVersion }>;
  write(root: string, rel: string, content: string, expected: FileVersion): Promise<unknown>;
}

export interface RenameOutcome {
  /** Files changed (open ones included). */
  changed: number;
  /** Files left as they were, with why. */
  skipped: { file: string; reason: string }[];
}

/** The changes that turn `doc`'s text into what the server's `edits` make of it. */
export function documentChanges(doc: EditorDocument, edits: readonly TextEdit[]): Change[] {
  const sent = doc.textForSave().replace(/\r\n?/g, "\n");
  return textChanges(doc.store.text(), bodyForStore(applyTextEdits(sent, edits), doc.shape));
}

/** Applies a rename's edits (per file URI) where each file is. */
export async function applyRename(edits: Map<string, TextEdit[]>, ports: RenamePorts): Promise<RenameOutcome> {
  const outcome: RenameOutcome = { changed: 0, skipped: [] };
  let touched = false;
  for (const [uri, fileEdits] of edits) {
    if (fileEdits.length === 0) continue;
    if (uri === ports.here.uri) {
      ports.here.apply(documentChanges(ports.here.doc, fileEdits));
      outcome.changed++;
      continue;
    }
    const open = ports.openDoc(uri);
    if (open) {
      const ctx = { tabSize: 4, layout: UNWRAPPED, pageRows: 1, commentPrefix: null };
      run(open, { type: "replaceText", changes: documentChanges(open, fileEdits) }, ctx);
      touched = true;
      outcome.changed++;
      continue;
    }
    const file = ports.fileOf(uri);
    if (!file) {
      outcome.skipped.push({ file: uri, reason: "outside the project" });
      continue;
    }
    try {
      const { content, version } = await ports.read(file.root, file.rel);
      await ports.write(file.root, file.rel, applyTextEdits(content, fileEdits), version);
      outcome.changed++;
    } catch (err) {
      const kind = (err as { kind?: string } | null)?.kind;
      outcome.skipped.push({ file: file.rel, reason: kind === "Conflict" ? "changed on disk meanwhile" : String(err) });
    }
  }
  if (touched) docTouched.update((n) => n + 1);
  return outcome;
}
