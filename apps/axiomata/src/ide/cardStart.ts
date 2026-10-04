/**
 * What the Kanban's "Starten" needs to decide before it asks the backend (A2A CP-A6a): which cards can be started at
 * all, and which project the dialog offers first. Pure, so it is tested without a backend.
 */
import type { BoardCard, IdeProject } from "../core/backend";

/** A card is started from "ready": approved, waiting for nobody, held by nobody. The backend checks it again. */
export function canStart(card: Pick<BoardCard, "state">): boolean {
  return card.state === "ready";
}

/**
 * The projects a card can be started in: those whose folder still exists, most recently opened first, so the project
 * the owner is working in is the one the dialog offers. Never opened projects come last.
 */
export function startableProjects(projects: IdeProject[]): IdeProject[] {
  return projects
    .filter((project) => project.root_exists)
    .sort((a, b) => (b.last_opened_at ?? "").localeCompare(a.last_opened_at ?? ""));
}
