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

/* ------------------------------------------------------------ dragging --- */

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** One column's geometry, measured once when a drag starts. */
export interface ColumnGeometry {
  columnId: number;
  rect: Rect;
  /** The cards in it, in drawing order, with their boxes. */
  cards: { id: number; rect: Rect }[];
}

export interface DropTarget {
  columnId: number;
  /**
   * Where the card goes, counting only the cards it will sit **among** — the
   * dragged card is excluded. Same meaning as the store's `move_card` index,
   * so the two cannot disagree about what a drop indicator promised.
   */
  index: number;
}

/**
 * Which column a pointer is over, and where in it a dragged card would land.
 *
 * Takes a snapshot rather than reading the DOM: the snapshot is measured once
 * at drag start, because the lifted card leaves a hole and re-measuring
 * mid-drag would chase boxes that move as a result of the very thing being
 * measured. Vertical position is compared against each card's midpoint, which
 * is what makes the indicator flip at the moment the eye expects it to.
 *
 * `null` when the pointer is outside every column — the caller shows no
 * indicator and a release there is a cancelled drag, not a drop into whatever
 * column happens to be nearest.
 */
export function dropTarget(
  snapshot: ColumnGeometry[],
  x: number,
  y: number,
  movingId: number,
): DropTarget | null {
  const column = snapshot.find(
    (c) => x >= c.rect.x && x <= c.rect.x + c.rect.w && y >= c.rect.y && y <= c.rect.y + c.rect.h,
  );
  if (!column) return null;

  const others = column.cards.filter((card) => card.id !== movingId);
  const index = others.findIndex((card) => y < card.rect.y + card.rect.h / 2);
  return { columnId: column.columnId, index: index === -1 ? others.length : index };
}

/**
 * The drop target one step away, for moving a card by keyboard.
 *
 * Arrow keys walk the same target type the pointer produces, so both paths end
 * in exactly the same `move_card` call. Up and down step within the column;
 * left and right change column, keeping the row as close as the new column
 * allows.
 */
export function stepTarget(
  snapshot: ColumnGeometry[],
  from: DropTarget,
  key: "up" | "down" | "left" | "right",
  movingId: number,
): DropTarget {
  const at = snapshot.findIndex((c) => c.columnId === from.columnId);
  if (at === -1) return from;

  const countIn = (i: number) => snapshot[i].cards.filter((card) => card.id !== movingId).length;

  if (key === "up") return { ...from, index: Math.max(0, from.index - 1) };
  if (key === "down") return { ...from, index: Math.min(countIn(at), from.index + 1) };

  const next = key === "left" ? at - 1 : at + 1;
  if (next < 0 || next >= snapshot.length) return from;
  return { columnId: snapshot[next].columnId, index: Math.min(from.index, countIn(next)) };
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
