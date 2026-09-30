/**
 * The editor's project, as plain functions (`docs/plans/editor-projekt-werkzeuge.md`, "Projekt neu/öffnen/schließen").
 *
 * A project is a row of the IDE's `projects` table — one registry for the
 * editor and the IDE — and its folder is the file root `project:<id>`. The
 * editor holds exactly one open at a time, remembered in `settings.editor.tree`.
 */

import type { IdeProject } from "../core/backend";

/** The file-service root of a project's folder. */
export function projectRootId(id: number): string {
  return `project:${id}`;
}

/** The roots the tree shows: the open project's folder, or nothing. Files opened singly stay out of it. */
export function treeRootsOf<T extends { id: string }>(roots: readonly T[], project: number | null): T[] {
  if (project === null) return [];
  const wanted = projectRootId(project);
  return roots.filter((r) => r.id === wanted);
}

/** The remembered project if it is still in the registry — a removed one means "none open". */
export function resolveProject(projects: readonly IdeProject[], saved: number | null): IdeProject | null {
  return saved === null ? null : (projects.find((p) => p.id === saved) ?? null);
}

/** What the menu lists: the projects except the open one, most recently opened first (the registry's own order), at most `limit`. */
export function otherProjects(projects: readonly IdeProject[], current: number | null, limit = 8): IdeProject[] {
  return projects.filter((p) => p.id !== current).slice(0, limit);
}

/**
 * Why `name` cannot be a new project's folder, or `null` if it can — the same rules as
 * `axiomata_ide::newproject::check_folder_name`, so the form says so before any dialog opens.
 */
export function folderNameProblem(name: string): string | null {
  const n = name.trim();
  if (n === "") return "Enter a project name first.";
  if (n.length > 100) return "That name is too long.";
  if (n.startsWith(".")) return "The name must not start with a dot.";
  if (/[/\\:\0]/.test(n)) return "The name must not contain / \\ or :.";
  return null;
}
