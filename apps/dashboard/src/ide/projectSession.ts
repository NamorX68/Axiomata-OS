/**
 * Which project the IDE has open, and everything that changes that.
 *
 * Split out of `IdeView.svelte` at the end of M7.1 (architecture review): the
 * view was carrying four unrelated jobs — project session, dock tree, tab
 * dragging, divider dragging — and M7.2 adds agent panes that need to hook
 * into project switching too. This is the `core/boardStore.ts`-shaped half:
 * backend-backed state in a Svelte store, with the rules that are easy to get
 * wrong living where a test can reach them. The view keeps the dock tree and
 * the two drag engines.
 *
 * Three rules are the reason this is a module and not a few functions in a
 * component:
 *
 * * **A slower answer never wins.** Two quick picks in the project menu are
 *   two independent IPC round trips, and Tauri does not promise they come back
 *   in the order they went out. A sequence guard discards a resolution that a
 *   newer open has already overtaken — the same guard `terminal.svelte` uses
 *   for scrollback fetches, and for the same reason.
 * * **The outgoing layout is written before anything else.** `flushLayout`
 *   first, always: the debounce timer is about to be handed a different
 *   project, and the arrangement the user just made would go with it.
 * * **A pane's working directory is derived, never trusted.** See
 *   `paneCwd.ts`; a stored `cwd` outlives the fact it was copied from.
 */

import { get, writable, type Readable } from "svelte/store";

import type { IdeProject } from "../core/backend";
import { toast } from "../core/toast";

import { emptyLayout, parseLayout, serializeLayout, singleGroupLayout, type Layout, type PaneTab } from "./layout";
import { applyProjectCwd } from "./paneCwd";
import {
  cancelLayoutWrite,
  createProject,
  deleteProject,
  flushLayout,
  listProjects,
  openProject,
  saveLayoutSoon,
  setProjectRoot,
} from "./projects";

export interface ProjectSession {
  projects: IdeProject[];
  current: IdeProject | null;
  /** True while an open is in flight, so the picker can stop taking clicks. */
  switching: boolean;
}

const EMPTY: ProjectSession = { projects: [], current: null, switching: false };

const state = writable<ProjectSession>(EMPTY);

/** The session, for the view to subscribe to. */
export const session: Readable<ProjectSession> = { subscribe: state.subscribe };

let sequence = 0;

function report(err: unknown): void {
  toast(err instanceof Error ? err.message : String(err), "danger");
}

/** A fresh terminal pane. Its id is the module's `instanceId` for its lifetime. */
export function terminalTab(): PaneTab {
  return { id: crypto.randomUUID(), kind: "terminal", title: "Terminal" };
}

/** What a project gets the first time it is opened: one terminal in it. */
function startingLayout(project: IdeProject): Layout {
  return applyProjectCwd(singleGroupLayout([terminalTab()]), project.repo_root);
}

/**
 * The layout to show for a project: its stored one, or a starting one.
 *
 * A stored layout that cannot be parsed says so rather than vanishing quietly
 * — losing an arrangement without a word is what `core/persist.ts` refuses to
 * do for `dashboard.json`, and the same applies here.
 */
export function layoutFor(project: IdeProject): Layout {
  if (project.layout_json === null) return startingLayout(project);
  const parsed = parseLayout(project.layout_json);
  if (!parsed) {
    toast(`The stored layout for “${project.name}” could not be read; starting fresh.`, "warning");
    return startingLayout(project);
  }
  return applyProjectCwd(parsed, project.repo_root);
}

async function refresh(): Promise<void> {
  state.update((s) => ({ ...s, projects: [] }));
  const projects = await listProjects();
  state.update((s) => ({ ...s, projects }));
}

/**
 * Opens a project and returns the layout to show, or `null` when the open was
 * overtaken by a newer one, failed, or the project is gone.
 *
 * `null` means "do not touch the view": either somebody else's answer is
 * already on screen, or there is nothing to show.
 */
export async function open(id: number): Promise<Layout | null> {
  const seq = ++sequence;
  state.update((s) => ({ ...s, switching: true }));
  try {
    await flushLayout();
    const project = await openProject(id);
    if (seq !== sequence) return null;
    if (!project) {
      await refresh();
      return null;
    }
    const layout = layoutFor(project);
    state.update((s) => ({ ...s, current: project }));
    await refresh();
    return seq === sequence ? layout : null;
  } catch (err) {
    report(err);
    return null;
  } finally {
    if (seq === sequence) state.update((s) => ({ ...s, switching: false }));
  }
}

/** Loads the project list and opens the most recent one, if there is any. */
export async function start(): Promise<Layout | null> {
  try {
    const projects = await listProjects();
    state.update((s) => ({ ...s, projects }));
    return projects.length > 0 ? await open(projects[0].id) : null;
  } catch (err) {
    report(err);
    return null;
  }
}

/** Creates a project and opens it. Returns its layout, or `null` on failure. */
export async function create(name: string, repoRoot: string): Promise<Layout | null> {
  try {
    const created = await createProject(name, repoRoot);
    await refresh();
    return await open(created.id);
  } catch (err) {
    report(err);
    return null;
  }
}

/**
 * Points a project at a different folder.
 *
 * Returns a corrected layout when the project being moved is the open one —
 * its panes were started in the old folder and their stored `cwd` now names a
 * place the project has nothing to do with.
 */
export async function changeRoot(id: number, repoRoot: string, layout: Layout): Promise<Layout | null> {
  try {
    const updated = await setProjectRoot(id, repoRoot);
    await refresh();
    if (!updated) return null;
    const isOpen = get(state).current?.id === id;
    if (!isOpen) return null;
    state.update((s) => ({ ...s, current: updated }));
    return applyProjectCwd(layout, updated.repo_root);
  } catch (err) {
    report(err);
    return null;
  }
}

/**
 * Removes a project from the list. Never its folder.
 *
 * Returns `true` when the open project was the one removed, so the view can
 * clear itself. Any layout write still waiting for that project is dropped:
 * harmless today because ids are never reused, but writing into a row that no
 * longer exists is not something to rely on staying harmless.
 */
export async function remove(id: number): Promise<boolean> {
  try {
    cancelLayoutWrite(id);
    await deleteProject(id);
    const wasOpen = get(state).current?.id === id;
    if (wasOpen) state.update((s) => ({ ...s, current: null }));
    await refresh();
    return wasOpen;
  } catch (err) {
    report(err);
    return false;
  }
}

/** Queues a debounced write of the open project's layout. */
export function save(layout: Layout): void {
  const project = get(state).current;
  if (project) saveLayoutSoon(project.id, serializeLayout(layout));
}

/** The layout an IDE with no project shows: nothing. */
export function noProjectLayout(): Layout {
  return emptyLayout();
}

/** Test seam: forget everything this module remembers. */
export function resetSessionForTests(): void {
  state.set(EMPTY);
  sequence = 0;
}
