/**
 * Run/Tasks, frontend side (`docs/plans/editor-projekt-werkzeuge.md`, #50): the three `task*` commands over
 * `axiomata-tasks`. Only this file knows the wire shape (`snake_case`).
 */

import { invokeBackend as invoke } from "../core/backend";

export type TaskSource = "detected" | "personal" | "project";
export type TaskGroup = "run" | "build" | "test" | "lint" | "other";

export interface TaskInfo {
  id: string;
  label: string;
  command: string;
  cwd: string | null;
  env: [string, string][];
  source: TaskSource;
  group: TaskGroup;
}

export interface TaskListInfo {
  tasks: TaskInfo[];
  /** The project's own `.axiomata/tasks.json`, when it has one. */
  project_file: { hash: string; trusted: boolean } | null;
  problems: string[];
}

export const listTasks = (root: string): Promise<TaskListInfo> => invoke<TaskListInfo>("tasks_list", { root });

/** The owner confirmed the project's `tasks.json` as the panel showed it (`hash` is what was shown). */
export const trustTasks = (root: string, hash: string): Promise<void> => invoke<void>("tasks_trust", { root, hash });

/** The line a task types into a shell; refused with kind `NeedsTrust` for an unconfirmed project task. */
export const taskCommandLine = (root: string, id: string): Promise<string> =>
  invoke<string>("task_command_line", { root, id });

/** What the "new task" form hands in; the same rules apply as for a hand-written file (checked in Rust). */
export interface NewTaskInput {
  label: string;
  command: string;
  cwd?: string | null;
  env?: Record<string, string>;
  group?: TaskGroup;
}

/** Where an own task is kept: the project's `.axiomata/tasks.json`, or the owner's file for every project. */
export type TaskScope = "project" | "personal";

/** Adds a task, or replaces the one called `replace`. */
export const saveTask = (root: string, scope: TaskScope, task: NewTaskInput, replace: string | null = null): Promise<void> =>
  invoke<void>("tasks_save", { root, scope, task, replace });

export const removeTask = (root: string, scope: TaskScope, label: string): Promise<void> =>
  invoke<void>("tasks_remove", { root, scope, label });

/** The task as the form shows it for editing. */
export function inputOf(task: TaskInfo): NewTaskInput {
  return { label: task.label, command: task.command, cwd: task.cwd, env: Object.fromEntries(task.env), group: task.group };
}

/** The one-line command, with its folder and environment, as the confirmation dialog shows it. */
export function describeTask(task: TaskInfo): string {
  const env = task.env.map(([k, v]) => `${k}=${v}`).join(" ");
  return [task.cwd ? `in ${task.cwd}:` : null, env || null, task.command].filter(Boolean).join(" ");
}

/** Tasks by what they are for, in the order the panel lists them. */
export const GROUP_ORDER: TaskGroup[] = ["run", "build", "test", "lint", "other"];
export const GROUP_LABEL: Record<TaskGroup, string> = {
  run: "Run",
  build: "Build",
  test: "Test",
  lint: "Check",
  other: "Other",
};

export function groupTasks(tasks: readonly TaskInfo[]): { group: TaskGroup; tasks: TaskInfo[] }[] {
  return GROUP_ORDER.map((group) => ({ group, tasks: tasks.filter((t) => t.group === group) })).filter(
    (g) => g.tasks.length > 0,
  );
}
