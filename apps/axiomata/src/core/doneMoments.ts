/**
 * The Kanban's "done" moment (`docs/plans/ui-ux-round.md` UX6a, G18): when a card arrives in a done column — whoever
 * moved it — it bursts into hex sparks while the board is on screen; a card that got there while nobody looked
 * glows once the next time the board is opened. Plain bookkeeping here; `modules/kanban.svelte` draws.
 */

import type { BoardCard, BoardColumn } from "./backend";

/** Each card's column role (`open` / `doing` / `done`) as last seen, by card id. */
export type RoleMap = ReadonlyMap<number, string>;

/** Every card's column role now; a card in a column the board does not list has none. */
export function rolesOf(cards: readonly BoardCard[], columns: readonly BoardColumn[]): Map<number, string> {
  const roleOfColumn = new Map(columns.map((c) => [c.id, c.maps_to_status as string]));
  const roles = new Map<number, string>();
  for (const card of cards) {
    const role = roleOfColumn.get(card.column_id);
    if (role) roles.set(card.id, role);
  }
  return roles;
}

/**
 * The cards that have just arrived in a done column: done now, and seen elsewhere before. A card new to the board
 * does not count (it was made there, not finished), and nothing counts on the first look (`before` is `null`).
 */
export function arrivedDone(before: RoleMap | null, now: RoleMap): number[] {
  if (before === null) return [];
  const arrived: number[] = [];
  for (const [id, role] of now) {
    const was = before.get(id);
    if (role === "done" && was !== undefined && was !== "done") arrived.push(id);
  }
  return arrived;
}

/**
 * The done cards that changed since the board was last looked at (`seenAt`, an ISO time): they glow once when it
 * opens. Never on a board looked at for the first time — everything would glow.
 */
export function doneSince(
  cards: readonly BoardCard[],
  columns: readonly BoardColumn[],
  seenAt: string | null | undefined,
): number[] {
  if (!seenAt) return [];
  const seen = Date.parse(seenAt);
  if (Number.isNaN(seen)) return [];
  const roles = rolesOf(cards, columns);
  return cards
    .filter((c) => roles.get(c.id) === "done" && c.archived_at === null && Date.parse(c.updated_at) > seen)
    .map((c) => c.id);
}
