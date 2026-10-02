/**
 * The lines the task panes were started with — in memory only.
 *
 * A task pane's tab carries just a task id; the command line it types into the shell is **never** in the
 * stored layout (`modules/terminal.svelte`'s `initialCommand` says why: opening a project must not be able
 * to run what a stored file says). It lives here, set by the code that resolved it through `task_command_line`,
 * and a pane restored from a stored layout is dropped instead of re-run (`projectSession.withoutTaskPanes`).
 */

import { get, writable } from "svelte/store";

interface Run {
  line: string;
  /** Bumped by a restart, which remounts the terminal. */
  runs: number;
}

const state = writable<Record<string, Run>>({});

export const taskRuns = { subscribe: state.subscribe };

/** Starts (or restarts) the pane `tabId` with `line`. */
export function startTaskRun(tabId: string, line: string): void {
  state.update((all) => ({ ...all, [tabId]: { line, runs: (all[tabId]?.runs ?? 0) + 1 } }));
}

export function forgetTaskRun(tabId: string): void {
  state.update((all) => {
    const { [tabId]: _gone, ...rest } = all;
    return rest;
  });
}

export function taskRunOf(tabId: string): Run | undefined {
  return get(state)[tabId];
}
