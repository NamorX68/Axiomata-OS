/**
 * The frontend side of the IDE's project commands, plus the one rule about
 * *when* a layout is written.
 *
 * The commands themselves are thin — `invokeBackend` with the arguments the
 * Rust side declares — and are here rather than inline in `IdeView.svelte` so
 * the view keeps dealing in projects and layouts instead of command names.
 *
 * `saveLayoutSoon` is the part worth reading. A dock layout changes on every
 * divider pixel, and a project's `layout_json` is a database write; the two
 * cannot be wired straight together. It debounces exactly the way
 * `core/persist.ts` debounces `dashboard.json`, and for the same reason, with
 * one addition that matters here: `flushLayout` writes immediately, because a
 * project switch must not leave the last few hundred milliseconds of the
 * layout the user just arranged in a timer that is about to be replaced.
 */

import { invokeBackend as invoke, type IdeProject } from "../core/backend";
import { toast } from "../core/toast";

/** Matches `core/persist.ts`'s debounce; the two write at the same rhythm. */
export const LAYOUT_SAVE_DEBOUNCE_MS = 400;

export function listProjects(): Promise<IdeProject[]> {
  return invoke<IdeProject[]>("list_ide_projects");
}

export function createProject(name: string, repoRoot: string): Promise<IdeProject> {
  return invoke<IdeProject>("create_ide_project", { name, repoRoot });
}

export function renameProject(id: number, name: string): Promise<IdeProject | null> {
  return invoke<IdeProject | null>("rename_ide_project", { id, name });
}

/** Changes where a project points without touching its id, name or layout. */
export function setProjectRoot(id: number, repoRoot: string): Promise<IdeProject | null> {
  return invoke<IdeProject | null>("set_ide_project_root", { id, repoRoot });
}

/** Marks the project as just opened — what the project list sorts by. */
export function openProject(id: number): Promise<IdeProject | null> {
  return invoke<IdeProject | null>("open_ide_project", { id });
}

/** Removes the row. Never the folder — the UI says "remove from the list". */
export function deleteProject(id: number): Promise<boolean> {
  return invoke<boolean>("delete_ide_project", { id });
}

let timer: ReturnType<typeof setTimeout> | null = null;
let pendingId: number | null = null;
let pendingLayout: string | null = null;
/** True once a write has failed, until one succeeds — see {@link flushLayout}. */
let failing = false;

/**
 * Writes whatever is waiting, now. Safe to call with nothing pending.
 *
 * Failures are caught and reported here rather than at the three call sites,
 * because the one that matters cannot report anything: the debounce timer
 * fires into nobody's promise. A layout that grows past the store's one-megabyte
 * ceiling would otherwise fail on *every* autosave from then on, for as long
 * as the app runs, with the user never told that their layout stopped being
 * saved. Reported once per run of failures, so a broken write does not turn
 * into a toast every 400 milliseconds.
 */
export async function flushLayout(): Promise<void> {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  const id = pendingId;
  const layout = pendingLayout;
  pendingId = null;
  pendingLayout = null;
  if (id === null || layout === null) return;
  try {
    await invoke<boolean>("set_ide_project_layout", { id, layout });
    failing = false;
  } catch (err) {
    if (!failing) {
      failing = true;
      toast(`The IDE layout could not be saved: ${err instanceof Error ? err.message : String(err)}`, "danger");
    }
  }
}

/**
 * Drops a queued write for a project without sending it.
 *
 * For a project being deleted: ids are never reused (`AUTOINCREMENT`) and the
 * store treats a write to a missing row as an ordinary `false`, so sending it
 * anyway would be harmless — but relying on that is relying on a guarantee
 * made somewhere else for a different reason.
 */
export function cancelLayoutWrite(id: number): void {
  if (pendingId !== id) return;
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  pendingId = null;
  pendingLayout = null;
}

/** Test seam: forget the pending write and whether a failure was reported. */
export function resetLayoutWritesForTests(): void {
  if (timer !== null) clearTimeout(timer);
  timer = null;
  pendingId = null;
  pendingLayout = null;
  failing = false;
}

/**
 * Queues a layout write for a project, replacing any still waiting.
 *
 * A pending write for a *different* project is flushed first rather than
 * dropped: switching projects while one is queued would otherwise throw away
 * the layout of the project just left.
 */
export function saveLayoutSoon(id: number, layout: string): void {
  if (pendingId !== null && pendingId !== id) void flushLayout();
  pendingId = id;
  pendingLayout = layout;
  if (timer !== null) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    void flushLayout();
  }, LAYOUT_SAVE_DEBOUNCE_MS);
}
