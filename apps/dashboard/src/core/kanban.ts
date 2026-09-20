/**
 * Kanban board logic — everything the module does that isn't drawing.
 *
 * No DOM, no Tauri, no Svelte, matching the repo's pure-logic-plus-vitest
 * convention (`core/todo.ts`, `core/routineInterval.ts`). The `.svelte` shell
 * stays thin and every rule that could be wrong lives here where a test can
 * reach it.
 *
 * Two deliberate limits, so the next reader doesn't go looking for them:
 *
 * * A card's status is never stored on the card. It is the `maps_to_status` of
 *   the column it sits in — `statusOf` resolves it. Two stored copies would
 *   drift, and the drift would be silent.
 * * Ordering within a column is the card's `position` alone. Sorting by due
 *   date is deliberately not offered: there can only be one truth about order,
 *   and it is the one the user dragged into place.
 */

import type { BoardCard, BoardColumn, CardStatus } from "./backend";

/** A column together with the cards in it, in the order they are drawn. */
export interface ColumnWithCards {
  column: BoardColumn;
  cards: BoardCard[];
}

export interface CardFilter {
  /** Case-insensitive match against title and body. */
  text?: string;
  /** A card must carry every label listed here. */
  labels?: string[];
  /** Only cards due at or before this instant. */
  dueBefore?: Date;
  /** Archived cards are hidden unless this is set. */
  includeArchived?: boolean;
}

/**
 * Groups cards into their columns, both in drawing order.
 *
 * A card whose `column_id` matches no column is dropped rather than shown
 * somewhere arbitrary — that state means the data is inconsistent, and
 * inventing a home for it would hide the problem instead of surfacing it.
 */
export function groupByColumn(columns: BoardColumn[], cards: BoardCard[]): ColumnWithCards[] {
  const byColumn = new Map<number, BoardCard[]>();
  for (const column of columns) byColumn.set(column.id, []);
  for (const card of cards) byColumn.get(card.column_id)?.push(card);

  return [...columns]
    .sort((a, b) => a.position - b.position || a.id - b.id)
    .map((column) => ({
      column,
      cards: (byColumn.get(column.id) ?? []).sort(
        (a, b) => a.position - b.position || a.id - b.id,
      ),
    }));
}

/** The status a card currently has, which is its column's. */
export function statusOf(card: BoardCard, columns: BoardColumn[]): CardStatus | null {
  return columns.find((column) => column.id === card.column_id)?.maps_to_status ?? null;
}

export function applyFilter(cards: BoardCard[], filter: CardFilter = {}): BoardCard[] {
  const needle = filter.text?.trim().toLowerCase();
  const wanted = filter.labels ?? [];
  const dueBefore = filter.dueBefore?.getTime();

  return cards.filter((card) => {
    if (!filter.includeArchived && card.archived_at !== null) return false;
    if (needle && !`${card.title}\n${card.body}`.toLowerCase().includes(needle)) return false;
    if (wanted.length > 0 && !wanted.every((label) => card.labels.includes(label))) return false;
    if (dueBefore !== undefined) {
      if (card.due_at === null) return false;
      if (new Date(card.due_at).getTime() > dueBefore) return false;
    }
    return true;
  });
}

/** Every label in use on the board, deduplicated, alphabetical. */
export function collectLabels(cards: BoardCard[]): string[] {
  return [...new Set(cards.flatMap((card) => card.labels))].sort((a, b) => a.localeCompare(b));
}

export interface DueState {
  /** Short human label: "heute", "in 3 T", "2 T überfällig". */
  label: string;
  overdue: boolean;
}

/**
 * How a due date reads, relative to `now`.
 *
 * Whole days apart by calendar date, not by elapsed hours: something due late
 * tonight is "heute", not "in 0 T", and something due first thing tomorrow is
 * "morgen" even if that is only nine hours away. Hour-based arithmetic would
 * be defensible and would also be wrong on the only question the badge is
 * asked, which is "is this today".
 */
export function dueState(dueAt: string, now: Date = new Date()): DueState {
  const due = new Date(dueAt);
  const days = Math.round(
    (Date.UTC(due.getFullYear(), due.getMonth(), due.getDate()) -
      Date.UTC(now.getFullYear(), now.getMonth(), now.getDate())) /
      86_400_000,
  );

  if (days < 0) return { label: `${Math.abs(days)} T überfällig`, overdue: true };
  if (days === 0) return { label: "heute", overdue: false };
  if (days === 1) return { label: "morgen", overdue: false };
  return { label: `in ${days} T`, overdue: false };
}

/** Palette index for a label, so the same word always reads the same colour. */
export function labelColorIndex(label: string, paletteSize: number): number {
  let hash = 0;
  for (let i = 0; i < label.length; i += 1) {
    hash = (hash * 31 + label.charCodeAt(i)) >>> 0;
  }
  return hash % paletteSize;
}

/** An actor string as it should be shown: `"agent:claude-1"` → `"claude-1"`. */
export function actorLabel(actor: string): string {
  const separator = actor.indexOf(":");
  return separator === -1 ? actor : actor.slice(separator + 1);
}

/**
 * Whether an assignee is worth showing at all.
 *
 * With a single human there is exactly one assignee the board would otherwise
 * repeat on every card, which is noise. Everyone else — every agent, and any
 * second human — is worth naming. This lights up on its own once agents start
 * taking cards, with no UI work at that point.
 */
export const DEFAULT_HUMAN = "human:owner";

export function showsAssignee(assignee: string | null): assignee is string {
  return assignee !== null && assignee !== DEFAULT_HUMAN;
}
