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

/** A reported card can have its review started by hand — when the studio could not, or the owner wants another engine. */
export function canReview(card: Pick<BoardCard, "state">): boolean {
  return card.state === "in_review";
}

/** A card the reviewer signed off waits for the owner's take-over, the second gate. */
export function canTakeOver(card: Pick<BoardCard, "state">): boolean {
  return card.state === "verified";
}

/** What the take-over's commit says unless the owner writes something else: the card's number and its first line. */
export function defaultTakeOverMessage(card: Pick<BoardCard, "id" | "title">): string {
  return `#${card.id} ${card.title.split("\n")[0].trim()}`;
}

/**
 * The engine a card's role would run on by itself: its own, else the first fallback that is in the catalog. `null` when
 * the role names none that exists — then the owner has to pick one when the card starts, and the form must not offer
 * "the role's own" as if there were one.
 */
export function roleEngine(
  role: { engine: string | null; fallback_engines: string[] } | undefined,
  catalog: { id: string }[],
): string | null {
  if (!role) return null;
  const known = new Set(catalog.map((engine) => engine.id));
  return [role.engine, ...role.fallback_engines].find((id): id is string => id !== null && known.has(id)) ?? null;
}
