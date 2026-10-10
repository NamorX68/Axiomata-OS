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

import type { AgentSpec, IdeAgent, IdeProject } from "../core/backend";
import { toast } from "../core/toast";

import { createAgent, deleteAgent, listAgents, updateAgent } from "./agents";

import { addTab, allGroups, allTabs, closeTab, emptyLayout, singleGroupLayout, type Layout, type PaneTab } from "./layout";
import { agentTabsOf, balanceFlow, flowAgentTarget, withFlowPanes } from "./planning";
import { FILES_PANE, TASK_PANE, planTab } from "./paneKinds";
import {
  emptyEditorLayout,
  parseWorkspace,
  serializeWorkspace,
  switchMode as swapMode,
  type Mode,
  type Parked,
  type Workspace,
} from "./modes";
import { newProjectFolder, openProjectFolder } from "../fileapp/backend";
import { applyProjectCwd } from "./paneCwd";
import {
  cancelLayoutWrite,
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
  /** The open project's agent profiles. Empty while no project is open. */
  agents: IdeAgent[];
  /** True while an open is in flight, so the picker can stop taking clicks. */
  switching: boolean;
  /** Which mode's layout the view shows (`ide/modes.ts`). */
  mode: Mode;
  /** The other modes' layouts: not shown, their panes kept mounted and running. */
  parked: Parked;
}

const EMPTY: ProjectSession = {
  projects: [],
  current: null,
  agents: [],
  switching: false,
  mode: "agents",
  parked: {},
};

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

/** What a project's Flow starts with: the planning panel, and nothing else until a planner is started. */
function startingFlowLayout(): Layout {
  return singleGroupLayout([planTab()]);
}

/**
 * What a project's Canvas starts with: nothing. The agents the studio starts fill it, and a terminal is one click away
 * (the files are in the shared sidebar) — a terminal nobody asked for would take a quarter of the width for good.
 */
function startingLayout(_project: IdeProject): Layout {
  return emptyLayout();
}

/**
 * Layouts stored before the shared sidebar start with a Files pane of their own — a second copy of the tree
 * the sidebar shows. The sidebar owns the tree now, so those panes are dropped on load.
 */
export function withoutFilesPanes(layout: Layout): Layout {
  return allTabs(layout)
    // Task panes go too: a task is a run of this session, and a stored layout must not start it again.
    .filter((t) => t.kind === FILES_PANE || t.kind === TASK_PANE)
    .reduce((acc, t) => closeTab(acc, t.id), layout);
}

/**
 * The workspace to show for a project: its stored layouts, or a starting one.
 *
 * A stored layout that cannot be parsed says so rather than vanishing quietly
 * — losing an arrangement without a word is what `core/persist.ts` refuses to
 * do for `dashboard.json`, and the same applies here.
 */
export function workspaceFor(project: IdeProject): Workspace {
  const starting = (): Workspace => ({
    mode: "agents",
    active: startingLayout(project),
    parked: { editor: emptyEditorLayout(), flow: startingFlowLayout() },
  });
  if (project.layout_json === null) return starting();
  const parsed = parseWorkspace(project.layout_json, {
    editor: emptyEditorLayout,
    agents: () => startingLayout(project),
    flow: startingFlowLayout,
  });
  if (!parsed) {
    toast(`The stored layout for “${project.name}” could not be read; starting fresh.`, "warning");
    return starting();
  }
  // The Flow always has its planning and team panels: a closed one would have no way back.
  const prepared = (layout: Layout, mode: Mode) => {
    const clean = applyProjectCwd(withoutFilesPanes(layout), project.repo_root);
    return mode === "flow" ? withFlowPanes(clean) : clean;
  };
  const parked: Parked = {};
  for (const [mode, layout] of Object.entries(parsed.parked) as [Mode, Layout][]) parked[mode] = prepared(layout, mode);
  return { mode: parsed.mode, active: prepared(parsed.active, parsed.mode), parked };
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
    const ws = workspaceFor(project);
    const agents = await listAgents(project.id);
    if (seq !== sequence) return null;
    state.update((s) => ({ ...s, current: project, agents, mode: ws.mode, parked: ws.parked }));
    await refresh();
    return seq === sequence ? ws.active : null;
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

/** "Open folder": the native dialog picks it; it is opened as a project. `null` when cancelled or failed. */
export async function openFolder(): Promise<Layout | null> {
  try {
    const picked = await openProjectFolder();
    if (!picked) return null;
    await refresh();
    return await open(picked.id);
  } catch (err) {
    report(err);
    return null;
  }
}

/** "New project": a new folder (optionally a git repository) in a folder the dialog picks, opened as a project. */
export async function createFolder(name: string, gitInit: boolean): Promise<Layout | null> {
  try {
    const created = await newProjectFolder(name, gitInit);
    if (!created) return null;
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
export async function changeRoot(id: number, layout: Layout): Promise<Layout | null> {
  try {
    const updated = await setProjectRoot(id);
    await refresh();
    if (!updated) return null;
    const isOpen = get(state).current?.id === id;
    if (!isOpen) return null;
    state.update((s) => {
      const parked: Parked = {};
      for (const [mode, layout] of Object.entries(s.parked) as [Mode, Layout][]) {
        parked[mode] = applyProjectCwd(layout, updated.repo_root);
      }
      return { ...s, current: updated, parked };
    });
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
    // The agents went with the project — the foreign key cascades.
    if (wasOpen) state.update((s) => ({ ...s, current: null, agents: [], parked: {}, mode: "agents" }));
    await refresh();
    return wasOpen;
  } catch (err) {
    report(err);
    return false;
  }
}

/**
 * "Close project": the open project is left (its layout written first) and the IDE shows nothing.
 * The row stays in the list; its panes are unmounted by the caller's empty layout.
 */
export async function close(): Promise<void> {
  sequence++;
  await flushLayout();
  state.update((s) => ({ ...s, current: null, agents: [], switching: false, parked: {}, mode: "agents" }));
}

/** Queues a debounced write of the open project's layout. */
export function save(layout: Layout): void {
  const { current: project, mode, parked } = get(state);
  if (project) saveLayoutSoon(project.id, serializeWorkspace({ mode, active: layout, parked }));
}

/**
 * Shows mode `next`: `layout` (on screen) is parked and the one parked for `next` is returned for the view to show.
 * Nothing is unmounted — the view keeps rendering the parked layouts' panes, hidden.
 */
export function switchMode(layout: Layout, next: Mode): Layout {
  const { mode, parked } = get(state);
  const swapped = swapMode({ mode, active: layout, parked }, next);
  state.update((s) => ({ ...s, mode: swapped.mode, parked: swapped.parked }));
  return next === "flow" ? withFlowPanes(swapped.active) : swapped.active;
}

/**
 * Adds a pane to the layout of a mode that is *not* on screen, beside what is in it — a session the studio started by
 * itself gets its pane without the owner being taken out of the mode they are in. The pane is mounted at once (a
 * hidden layout's panes are), so its harness starts.
 *
 * The Flow is repaired before anything lands in it (`withFlowPanes`, what showing the mode does): without its panels a
 * card session parked from the Kanban would sit in a layout that has only itself, and open beside the Planung tab
 * instead of beside the team's tiles. Its session columns are then shared out evenly, exactly as when the Flow is on
 * screen — so the owner does not come back to one column holding everything.
 */
export function addTabParked(mode: Mode, tab: PaneTab): void {
  state.update((s) => {
    if (mode === s.mode) return s;
    const parked = s.parked[mode] ?? emptyLayout();
    const layout = mode === "flow" ? withFlowPanes(parked) : parked;
    const groups = allGroups(layout);
    const last = groups.length > 0 ? groups[groups.length - 1].id : layout.root.id;
    const flow = mode === "flow" ? flowAgentTarget(layout) : null;
    const target = flow ?? { nodeId: last, side: "right" as const };
    const added = addTab(layout, tab, target);
    // The shown path's rule (`openAgent`): a pane that became a tab in the newest column added no column to balance.
    const settled = flow && flow.side !== "center" ? balanceFlow(added) : added;
    return { ...s, parked: { ...s.parked, [mode]: settled } };
  });
}

/**
 * Closes the panes of sessions that are gone in the layouts that are *not* on screen (the shown layout is the view's
 * to change). A plan that has had its say ends its planner, and a pane of it left in a hidden mode would go on running a
 * harness for a session the backend has forgotten.
 */
export function closeParkedAgentTabs(agentIds: number[]): void {
  state.update((s) => {
    const parked: Parked = {};
    for (const [mode, layout] of Object.entries(s.parked) as [Mode, Layout][]) {
      parked[mode] = agentTabsOf(layout, agentIds).reduce((acc, tab) => closeTab(acc, tab.id), layout);
    }
    return { ...s, parked };
  });
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

/* ------------------------------------------------------------- agents --- */

/** Reloads the open project's agents. */
export async function refreshAgents(): Promise<void> {
  const project = get(state).current;
  if (!project) return;
  const agents = await listAgents(project.id);
  state.update((s) => (s.current?.id === project.id ? { ...s, agents } : s));
}

/** Adds an agent, made on an engine, to the open project. Returns it, or `null` on failure. */
export async function addAgent(spec: AgentSpec): Promise<IdeAgent | null> {
  const project = get(state).current;
  if (!project) return null;
  try {
    const created = await createAgent(project.id, spec);
    await refreshAgents();
    return created;
  } catch (err) {
    report(err);
    return null;
  }
}

/** Renames an agent and moves it to another engine or role. A running pane picks the change up on restart. */
export async function editAgent(id: number, spec: AgentSpec): Promise<IdeAgent | null> {
  try {
    const updated = await updateAgent(id, spec);
    await refreshAgents();
    return updated;
  } catch (err) {
    report(err);
    return null;
  }
}

/**
 * Removes an agent profile.
 *
 * Panes showing it stay open and say the profile is gone rather than vanishing
 * from under whatever is running in them — closing a pane would take a live
 * shell with it, and the user did not ask for that.
 */
export async function removeAgent(id: number): Promise<boolean> {
  try {
    return await deleteAgent(id);
  } catch (err) {
    report(err);
    return false;
  } finally {
    // Also after an error: the row can be gone while cleaning up its status
    // folder failed, and the list must not keep showing a deleted agent.
    await refreshAgents().catch(report);
  }
}

/** An agent by id, or `null` — what a pane asks to know what it is showing. */
export function agentById(id: number): IdeAgent | null {
  return get(state).agents.find((agent) => agent.id === id) ?? null;
}
