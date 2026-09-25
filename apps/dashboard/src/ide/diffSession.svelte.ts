/**
 * One agent's Diffs view, without the view (`docs/plans/git-layer.md`, CP8,
 * CP9): the changed files, the open file's diff, and the handgrips on them —
 * as reactive state a component draws, tested with a stand-in backend
 * (architecture review, CP8: the load/refresh races deserved a seam).
 *
 * * **Every answer is checked against what it was for.** A list reload that
 *   an older one overtook is dropped; a file load that arrives after another
 *   file was picked is dropped — a 5-second poll that began before a click
 *   must not put the old file back (found in CP8's browser test).
 * * **A poll keeps the open diff** when its hunks did not change, so folds and
 *   the cursor stay while the agent works (G4).
 * * **Every handgrip reloads everything afterwards** (`refresh(true)`): a
 *   discard, commit or take-over changes the list, the hunks, or both.
 */

import type { AgentDiffState, AgentFileChange, FileDiff, TakeOverMode, TakeOverResult } from "../core/backend";
import { messageOf } from "../core/errors";
import { DiffModel } from "../editor/diff/model";
import {
  agentChanges,
  agentCommit,
  agentDiscard,
  agentDiscardHunk,
  agentFileDiff,
  agentLastSubject,
  agentTakeOver,
  loadFileDiff,
  sameFileDiff,
  type LoadedDiff,
} from "./git";

/** What the session needs from the git layer — the real commands, or a test's stand-in. */
export interface DiffBackend {
  changes(id: number): Promise<AgentDiffState>;
  fileDiff(id: number, path: string, oldPath: string | null): Promise<FileDiff>;
  load(id: number, change: AgentFileChange): Promise<LoadedDiff>;
  discard(id: number, paths: string[]): Promise<void>;
  discardHunk(id: number, path: string, oldPath: string | null, index: number, header: string): Promise<void>;
  commit(id: number, message: string): Promise<string>;
  takeOver(id: number, mode: TakeOverMode, message: string): Promise<TakeOverResult>;
  lastSubject(id: number): Promise<string | null>;
}

export const gitBackend: DiffBackend = {
  changes: agentChanges,
  fileDiff: agentFileDiff,
  load: loadFileDiff,
  discard: agentDiscard,
  discardHunk: agentDiscardHunk,
  commit: agentCommit,
  takeOver: agentTakeOver,
  lastSubject: agentLastSubject,
};

export class AgentDiffSession {
  diffState = $state.raw<AgentDiffState | null>(null);
  listError = $state("");
  selected = $state<string | null>(null);
  loaded = $state.raw<LoadedDiff | null>(null);
  model = $state.raw<DiffModel | null>(null);
  diffError = $state("");
  /** Bumped when the model changed outside the diff panes (a new file, whole file on/off). */
  revision = $state(0);
  wholeFile = $state(false);
  refreshing = $state(false);
  /** Called after a new file's diff is shown — the view puts the cursor on its first change. */
  onLoaded: (() => void) | null = null;

  private listGeneration = 0;
  private fileGeneration = 0;

  constructor(
    readonly agentId: number,
    private readonly backend: DiffBackend = gitBackend,
  ) {}

  get files(): AgentFileChange[] {
    return this.diffState?.state === "ready" ? this.diffState.changes.files : [];
  }

  get base() {
    return this.diffState?.state === "ready" ? this.diffState.changes.base : null;
  }

  /** The open file's change, if it is still in the list. */
  get current(): AgentFileChange | null {
    return this.files.find((f) => f.path === this.selected) ?? null;
  }

  /** Some change is not committed yet — what "Commit" is for, and what blocks "Take over" (G12). */
  get hasUncommitted(): boolean {
    return this.files.some((f) => f.uncommitted);
  }

  /** Reloads the list, then the open file if it changed (or always, when `force`). */
  async refresh(force: boolean): Promise<void> {
    const mine = ++this.listGeneration;
    this.refreshing = true;
    try {
      const next = await this.backend.changes(this.agentId);
      if (mine !== this.listGeneration) return;
      this.diffState = next;
      this.listError = "";
    } catch (err) {
      if (mine === this.listGeneration) this.listError = messageOf(err);
      return;
    } finally {
      if (mine === this.listGeneration) this.refreshing = false;
    }
    const current = this.current;
    if (current) await this.reloadIfChanged(current, force);
    else await this.select(this.files[0] ?? null);
  }

  /** Opens `change`'s diff (or clears the view for `null`). */
  async select(change: AgentFileChange | null): Promise<void> {
    this.selected = change?.path ?? null;
    if (change) await this.load(change);
    else {
      this.loaded = null;
      this.model = null;
    }
  }

  /** The next (`1`) or previous (`-1`) file in the list (H9: ⌥⌘↓/⌥⌘↑). */
  stepFile(step: 1 | -1): void {
    const files = this.files;
    if (files.length === 0) return;
    const at = files.findIndex((f) => f.path === this.selected);
    const next = files[Math.max(0, Math.min(files.length - 1, at + step))];
    if (next && next.path !== this.selected) void this.select(next);
  }

  toggleWholeFile(): void {
    this.wholeFile = !this.wholeFile;
    this.model?.setWholeFile(this.wholeFile);
    this.revision++;
  }

  /** Puts the open file back to the base (G13); then reloads. */
  async discardFile(): Promise<void> {
    const change = this.current;
    if (!change) return;
    const paths = change.old_path ? [change.path, change.old_path] : [change.path];
    await this.backend.discard(this.agentId, paths);
    await this.refresh(true);
  }

  /** Puts the open file's `index`-th hunk back (H6); then reloads. */
  async discardHunk(index: number): Promise<void> {
    const change = this.current;
    const header = this.loaded?.diff.hunks[index]?.header;
    if (!change || header === undefined) return;
    try {
      await this.backend.discardHunk(this.agentId, change.path, change.old_path, index, header);
    } finally {
      // Refused because the agent changed the file meanwhile (H13): the
      // reload shows what is there now, next to the reason.
      await this.refresh(true);
    }
  }

  /** Commits what the agent left uncommitted (G3); resolves to the commit. */
  async commit(message: string): Promise<string> {
    const commit = await this.backend.commit(this.agentId, message);
    await this.refresh(true);
    return commit;
  }

  /** Takes the committed work over into the project folder (G7–G12). */
  async takeOver(mode: TakeOverMode, message: string): Promise<TakeOverResult> {
    const result = await this.backend.takeOver(this.agentId, mode, message);
    await this.refresh(true);
    return result;
  }

  lastSubject(): Promise<string | null> {
    return this.backend.lastSubject(this.agentId);
  }

  /** Reloads the open file, unless its diff is exactly what is shown already. */
  private async reloadIfChanged(change: AgentFileChange, force: boolean): Promise<void> {
    const shown = this.loaded;
    if (!force && shown && shown.change.path === change.path) {
      try {
        const diff = await this.backend.fileDiff(this.agentId, change.path, change.old_path);
        // Another file was picked meanwhile: that pick has its own load.
        if (this.selected !== change.path) return;
        if (sameFileDiff(diff, shown.diff)) return;
      } catch {
        // Fall through to a full load, which reports the error.
      }
    }
    await this.load(change);
  }

  private async load(change: AgentFileChange): Promise<void> {
    const mine = ++this.fileGeneration;
    this.diffError = "";
    try {
      const next = await this.backend.load(this.agentId, change);
      // A newer load, or a poll that started before another file was picked.
      if (mine !== this.fileGeneration || this.selected !== change.path) return;
      const model = new DiffModel(next.source);
      if (this.wholeFile) model.setWholeFile(true);
      this.loaded = next;
      this.model = model;
      this.revision++;
      this.onLoaded?.();
    } catch (err) {
      if (mine === this.fileGeneration) this.diffError = messageOf(err);
    }
  }
}
