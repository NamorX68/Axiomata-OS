/**
 * Carrying out a language server's edit across files — a rename (`docs/plans/editor.md`,
 * ED6.5, L16) or a code action (ED6.7, L23):
 *
 * * **A file open in an editor** is changed in its document — one undo step,
 *   left unsaved like any edit. The editor the edit started in applies it
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

export interface EditPorts {
  /** The document of the editor the edit started in, and how it applies changes (its surface). */
  here: { uri: string; doc: EditorDocument; apply(changes: Change[]): void };
  /** Another editor's document for `uri`, if one has it open. */
  openDoc(uri: string): EditorDocument | null;
  /** The file for `uri` in a root the editor may write; `null` outside every root. */
  fileOf(uri: string): { root: string; rel: string } | null;
  read(root: string, rel: string): Promise<{ content: string; version: FileVersion }>;
  write(root: string, rel: string, content: string, expected: FileVersion): Promise<unknown>;
}

export interface EditOutcome {
  /** Files changed (open ones included). */
  changed: number;
  /** Files left as they were, with why. */
  skipped: { file: string; reason: string }[];
  /**
   * Why nothing at all was changed: a closed file the edit needs could not be
   * read — most often one it would create (TypeScript's "Move to a new file"
   * writes into a file that does not exist yet). Half an edit would leave broken code.
   */
  refused: string | null;
}

/** The changes that turn `doc`'s text into what the server's `edits` make of it. */
export function documentChanges(doc: EditorDocument, edits: readonly TextEdit[]): Change[] {
  const sent = doc.textForSave().replace(/\r\n?/g, "\n");
  return textChanges(doc.store.text(), bodyForStore(applyTextEdits(sent, edits), doc.shape));
}

/**
 * Applies a server's edits (per file URI) where each file is. Every closed
 * file is read first: when one cannot be, nothing is changed (`refused`).
 */
export async function applyWorkspaceEdit(edits: Map<string, TextEdit[]>, ports: EditPorts): Promise<EditOutcome> {
  const outcome: EditOutcome = { changed: 0, skipped: [], refused: null };
  type Closed = { root: string; rel: string; content: string; version: FileVersion; edits: TextEdit[] };
  const closed: Closed[] = [];
  // `doc` null: this editor's own document, changed through its surface.
  const open: { doc: EditorDocument | null; edits: TextEdit[] }[] = [];
  for (const [uri, fileEdits] of edits) {
    if (fileEdits.length === 0) continue;
    const doc = uri === ports.here.uri ? null : ports.openDoc(uri);
    if (uri === ports.here.uri || doc) {
      open.push({ doc, edits: fileEdits });
      continue;
    }
    const file = ports.fileOf(uri);
    if (!file) {
      outcome.skipped.push({ file: uri, reason: "outside the project" });
      continue;
    }
    try {
      const { content, version } = await ports.read(file.root, file.rel);
      closed.push({ ...file, content, version, edits: fileEdits });
    } catch (err) {
      const kind = (err as { kind?: string } | null)?.kind;
      const why = kind === "NotFound" ? "it would create" : "it cannot read";
      return { changed: 0, skipped: [], refused: `${why} ${file.rel}` };
    }
  }
  let touched = false;
  for (const { doc, edits: fileEdits } of open) {
    if (doc === null) {
      ports.here.apply(documentChanges(ports.here.doc, fileEdits));
    } else {
      const ctx = { tabSize: 4, layout: UNWRAPPED, pageRows: 1, commentPrefix: null };
      run(doc, { type: "replaceText", changes: documentChanges(doc, fileEdits) }, ctx);
      touched = true;
    }
    outcome.changed++;
  }
  for (const file of closed) {
    try {
      await ports.write(file.root, file.rel, applyTextEdits(file.content, file.edits), file.version);
      outcome.changed++;
    } catch (err) {
      const kind = (err as { kind?: string } | null)?.kind;
      outcome.skipped.push({ file: file.rel, reason: kind === "Conflict" ? "changed on disk meanwhile" : String(err) });
    }
  }
  if (touched) docTouched.update((n) => n + 1);
  return outcome;
}
