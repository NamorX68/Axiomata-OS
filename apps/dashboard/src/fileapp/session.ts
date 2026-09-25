/**
 * One open file in the file app: the document, the version on disk it is based
 * on, and what to tell the owner about it (`docs/plans/editor.md`, F8–F10).
 *
 * The rules this class keeps:
 *
 * * **Every save states the version it expects** (E4). If the file moved on,
 *   the write fails with `Conflict` and nothing is overwritten; the banner
 *   offers the choices F10 lists instead.
 * * **An external change is judged by version, not by event.** A change that
 *   carries the version our own save just returned is our own echo and is
 *   ignored. A clean document reloads silently (with a short hint); one with
 *   unsaved edits gets the banner — it is never merged automatically.
 * * **Unsaved text is kept aside** (F8) through `persistRecovery`, and offered
 *   back when the file is opened again, with a note if the file changed since.
 * * **Nothing is asked with a native dialog.** Every question is a banner the
 *   view shows; a destructive choice (reload over unsaved edits, overwrite an
 *   external change) takes a second, explicit click.
 *
 * * **A new note is a session without a file** (`untitled`, W4): its draft is
 *   kept aside under its own recovery key, it is never written or watched, and
 *   the view files it with `create_note` instead of saving.
 *
 * No DOM and no Svelte: the backend is passed in, so every flow is tested with
 * a fake one (`session.test.ts`).
 */

import type { EditorRecovery, FileChange, FileVersion, TextFile } from "../core/backend";
import { EditorDocument } from "../editor/document";
import type { Indent } from "../editor/detect";

/** The backend calls a session needs; the real one is `invokeBackend`. */
export interface FileBackend {
  read(root: string, rel: string): Promise<TextFile>;
  write(root: string, rel: string, content: string, expected: FileVersion | null): Promise<FileVersion>;
  watch(root: string, rel: string): Promise<void>;
  unwatch(root: string, rel: string): Promise<void>;
  recoverySave(root: string, rel: string, base: FileVersion | null, content: string): Promise<void>;
  recoveryLoad(root: string, rel: string): Promise<EditorRecovery | null>;
  recoveryDelete(root: string, rel: string): Promise<void>;
}

/** What the view shows above the editor, if anything. */
export type Banner =
  /** Unsaved text from an earlier session was found. */
  | { kind: "recovery"; entry: EditorRecovery; changedSince: boolean }
  /** The file changed on disk while there are unsaved edits (F10). */
  | { kind: "external"; confirmReload: boolean }
  /** "Keep mine" was chosen; the next save asks before overwriting. */
  | { kind: "confirmOverwrite" }
  /** The file was deleted on disk. */
  | { kind: "deleted" }
  /** The clean document was reloaded silently — a hint, not a question. */
  | { kind: "reloaded" }
  | { kind: "error"; message: string };

/** The recovery key a new note's draft is kept under — a key, not a place on disk. */
export const DRAFT_ROOT = "new-note";
/** `.md`, so the draft gets Markdown's colours and preview like the note it becomes. */
export const DRAFT_REL = "Untitled.md";

export type SaveResult = "saved" | "unchanged" | "conflict" | "readOnly" | "needsConfirm" | "error";

interface FileErrorShape {
  kind?: string;
  message?: string;
}

function errorKind(err: unknown): string | undefined {
  return typeof err === "object" && err !== null ? (err as FileErrorShape).kind : undefined;
}

function errorText(err: unknown): string {
  if (typeof err === "object" && err !== null && "message" in err) return String((err as FileErrorShape).message);
  return String(err);
}

export class FileSession {
  readonly doc: EditorDocument;
  /** The version on disk the document is based on; `null` if the file is gone. */
  version: FileVersion | null;
  readonly readOnly: boolean;
  /** A new note not filed yet: no file behind it (W4). */
  readonly untitled: boolean;
  banner: Banner | null = null;
  /** Set by "keep mine": the next save must be confirmed, then overwrites. */
  private overwriteArmed = false;
  private closed = false;

  private constructor(
    private readonly backend: FileBackend,
    readonly root: string,
    readonly rel: string,
    file: Pick<TextFile, "content" | "large"> & { version: FileVersion | null },
    indentFallback: Indent,
    untitled = false,
  ) {
    this.doc = new EditorDocument(file.content, { indentFallback });
    this.version = file.version;
    this.readOnly = file.large;
    this.untitled = untitled;
  }

  /**
   * A new note: an empty document, or — with the recovery banner — the draft
   * kept from last time. Nothing is read or watched; there is no file yet.
   */
  static async untitled(backend: FileBackend, indentFallback: Indent): Promise<FileSession> {
    const session = new FileSession(
      backend,
      DRAFT_ROOT,
      DRAFT_REL,
      { content: "", version: null, large: false },
      indentFallback,
      true,
    );
    const entry = await backend.recoveryLoad(DRAFT_ROOT, DRAFT_REL).catch(() => null);
    if (entry?.content) session.banner = { kind: "recovery", entry, changedSince: false };
    return session;
  }

  /**
   * Opens `rel` under `root`: reads it, starts watching it, and looks for
   * unsaved text kept from before.
   *
   * Errors: whatever `read` rejects with — the view shows it; nothing is watched.
   */
  static async open(backend: FileBackend, root: string, rel: string, indentFallback: Indent): Promise<FileSession> {
    const file = await backend.read(root, rel);
    const session = new FileSession(backend, root, rel, file, indentFallback);
    await backend.watch(root, rel).catch(() => {
      // Without a watcher the file still opens; conflicts are then caught at
      // save time by the expected version.
    });
    const entry = await backend.recoveryLoad(root, rel).catch(() => null);
    if (entry && entry.content !== file.content) {
      session.banner = { kind: "recovery", entry, changedSince: entry.base_version !== file.version };
    } else if (entry) {
      await backend.recoveryDelete(root, rel).catch(() => undefined);
    }
    return session;
  }

  /** The file's name, for titles and the comment prefix. */
  get fileName(): string {
    return this.rel.split("/").pop() ?? this.rel;
  }

  /**
   * Saves with the expected version. A conflict raises the external-change
   * banner; after "keep mine" the first save only asks (`needsConfirm`), and
   * `save(true)` then overwrites.
   */
  async save(confirmed = false): Promise<SaveResult> {
    if (this.readOnly) return "readOnly";
    // A new note is filed by the view (`create_note`), never written here — not even by autosave.
    if (this.untitled) return "unchanged";
    if (!this.doc.dirty && this.version !== null && !this.overwriteArmed) return "unchanged";
    if (this.overwriteArmed && !confirmed) {
      this.banner = { kind: "confirmOverwrite" };
      return "needsConfirm";
    }
    const expected = this.overwriteArmed || this.version === null ? null : this.version;
    try {
      this.version = await this.backend.write(this.root, this.rel, this.doc.textForSave(), expected);
    } catch (err) {
      if (errorKind(err) === "Conflict") {
        this.banner = { kind: "external", confirmReload: false };
        return "conflict";
      }
      this.banner = { kind: "error", message: errorText(err) };
      return "error";
    }
    this.doc.markSaved();
    this.overwriteArmed = false;
    if (this.banner?.kind !== "recovery") this.banner = null;
    await this.backend.recoveryDelete(this.root, this.rel).catch(() => undefined);
    return "saved";
  }

  /**
   * Reacts to `files:changed`. Returns whether anything visible changed. The
   * echo of our own save (same version) is ignored.
   */
  async onExternalChange(change: FileChange): Promise<boolean> {
    if (this.closed || change.root !== this.root || change.rel !== this.rel) return false;
    if (change.version !== null && change.version === this.version) return false;
    if (change.kind === "deleted") {
      this.version = null;
      this.banner = { kind: "deleted" };
      return true;
    }
    if (!this.doc.dirty && !this.overwriteArmed) {
      await this.reload();
      this.banner = { kind: "reloaded" };
      return true;
    }
    this.banner = { kind: "external", confirmReload: false };
    return true;
  }

  /** "Neu laden" over unsaved edits: the first call asks, the second reloads. */
  async requestReload(): Promise<void> {
    if (this.doc.dirty && !(this.banner?.kind === "external" && this.banner.confirmReload)) {
      this.banner = { kind: "external", confirmReload: true };
      return;
    }
    await this.reload();
    this.banner = null;
  }

  /** Backs out of the reload confirmation to the plain external-change banner. */
  cancelReload(): void {
    if (this.banner?.kind === "external") this.banner = { kind: "external", confirmReload: false };
  }

  /** "Meine behalten": the banner goes; the next save asks before overwriting. */
  keepMine(): void {
    this.overwriteArmed = true;
    this.banner = null;
  }

  /** What is on disk now, for the read-only side view (F10, until CP8). */
  async diskText(): Promise<string> {
    if (this.untitled) return "";
    return (await this.backend.read(this.root, this.rel)).content;
  }

  /** Puts the kept unsaved text back into the editor (as one undoable step). */
  async restoreRecovery(): Promise<void> {
    if (this.banner?.kind !== "recovery") return;
    const { entry, changedSince } = this.banner;
    this.doc.replaceAll(entry.content);
    // Based on an older version: the next save must not silently win over it.
    if (changedSince) this.overwriteArmed = true;
    this.banner = null;
  }

  async discardRecovery(): Promise<void> {
    await this.backend.recoveryDelete(this.root, this.rel).catch(() => undefined);
    if (this.banner?.kind === "recovery") this.banner = null;
  }

  /** Keeps the unsaved text aside (F8); called by the view after 2 s of quiet. */
  async persistRecovery(): Promise<void> {
    if (this.closed || this.readOnly) return;
    if (!this.doc.dirty) {
      await this.backend.recoveryDelete(this.root, this.rel).catch(() => undefined);
      return;
    }
    await this.backend.recoverySave(this.root, this.rel, this.version, this.doc.textForSave()).catch(() => undefined);
  }

  /** Discards unsaved edits for good (the view asked first). */
  async discardChanges(): Promise<void> {
    if (this.untitled) this.doc.reset("");
    else await this.reload();
    await this.backend.recoveryDelete(this.root, this.rel).catch(() => undefined);
    this.banner = null;
  }

  dismissBanner(): void {
    this.banner = null;
  }

  /** Stops watching; unsaved text stays in the recovery entry. */
  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    if (!this.untitled) await this.backend.unwatch(this.root, this.rel).catch(() => undefined);
  }

  private async reload(): Promise<void> {
    const file = await this.backend.read(this.root, this.rel);
    this.doc.reset(file.content);
    this.version = file.version;
    this.overwriteArmed = false;
  }
}
