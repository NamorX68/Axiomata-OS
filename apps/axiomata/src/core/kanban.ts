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

import type {
  BoardCard,
  BoardColumn,
  BoardPlan,
  CardEventKind,
  CardFields,
  CardStatus,
  TaskState,
} from "./backend";

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
  /** Short human label: "today", "in 3 T", "2 T überfällig". */
  label: string;
  overdue: boolean;
  /** Due today or tomorrow: the card says so in the warning colour (editor-look B1). */
  soon: boolean;
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

  if (days < 0) return { label: `${Math.abs(days)} d overdue`, overdue: true, soon: false };
  if (days === 0) return { label: "today", overdue: false, soon: true };
  if (days === 1) return { label: "tomorrow", overdue: false, soon: true };
  return { label: `in ${days} d`, overdue: false, soon: false };
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

// ------------------------------------------------------------ agent flow ---
// What the board shows of the agent flow (a2a.md CP-A2). The state itself is derived in Rust and arrives with the
// card; this file only decides how it reads and which controls make sense.


/** How a card's state reads on the board. */
export const STATE_LABEL: Record<TaskState, string> = {
  proposed: "proposal",
  blocked: "waiting",
  ready: "ready",
  working: "in progress",
  input_required: "question",
  in_review: "in review",
  done: "done",
  verified: "checked",
  integrated: "in the plan",
  taken_over: "taken over",
  failed: "failed",
  canceled: "canceled",
};

/**
 * Whether a state deserves a badge on the card. The column already says "open", "in progress", "in review" or
 * "done", so repeating those would only add noise; what the column cannot say is that a card waits for another one,
 * asks a question, failed, was called off or has been taken over.
 */
export function isNotableState(state: TaskState): boolean {
  return (
    state === "blocked" ||
    state === "input_required" ||
    state === "failed" ||
    state === "canceled" ||
    state === "integrated" ||
    state === "taken_over"
  );
}

/** The tone a badge takes (`data-tone` in the stylesheet). */
export function stateTone(state: TaskState): "warn" | "bad" | "muted" {
  switch (state) {
    case "failed":
    case "canceled":
      return "bad";
    case "integrated":
    case "taken_over":
      return "muted";
    default:
      return "warn";
  }
}

/**
 * Every writable field of a card, for `update_card`.
 *
 * `update_card` replaces all of them, so a save that sends only the field it changed would wipe the others. Collected
 * here, in one place, with a test that lists the keys — a field added to `CardFields` and forgotten in this function
 * shows up as a failing test instead of as silently lost data.
 */
export function fieldsOf(card: BoardCard): CardFields {
  return {
    title: card.title,
    body: card.body,
    labels: card.labels,
    assignee: card.assignee,
    due_at: card.due_at,
    plan_id: card.plan_id,
    agent: card.agent,
    agent_reason: card.agent_reason,
    tier: card.tier,
    kind: card.kind,
    acceptance: card.acceptance,
  };
}

/**
 * Drops the proposal column while nothing is in it (a2a.md A13): every board has one, but it only takes space once an
 * agent or a planner has put something there. Judged on the **unfiltered** cards, so typing in the filter box never
 * makes the column come and go.
 */
export function hideEmptyProposal(groups: ColumnWithCards[], all: BoardCard[]): ColumnWithCards[] {
  return groups.filter(({ column }) => {
    if (column.stage !== "proposal") return true;
    return all.some((card) => card.column_id === column.id && card.archived_at === null);
  });
}

/**
 * The cards `card` could be made to wait for: others of the same plan that are not archived, not waited for yet and
 * that do not already (indirectly) wait for `card` — that edge would be a cycle. The Rust side is the authority; this
 * only keeps the picker from offering what it would refuse.
 */
export function dependencyCandidates(card: BoardCard, cards: BoardCard[]): BoardCard[] {
  if (card.plan_id === null) return [];
  const byId = new Map(cards.map((other) => [other.id, other]));
  const waitsForCard = (startId: number): boolean => {
    const seen = new Set<number>();
    const stack = [startId];
    while (stack.length > 0) {
      const current = stack.pop()!;
      if (current === card.id) return true;
      if (seen.has(current)) continue;
      seen.add(current);
      stack.push(...(byId.get(current)?.depends_on ?? []));
    }
    return false;
  };
  return cards
    .filter(
      (other) =>
        other.id !== card.id &&
        other.plan_id === card.plan_id &&
        other.archived_at === null &&
        !card.depends_on.includes(other.id) &&
        !waitsForCard(other.id),
    )
    .sort((a, b) => a.id - b.id);
}

/** The plan a card belongs to, if it is on this board. */
export function planOf(card: BoardCard, plans: BoardPlan[]): BoardPlan | null {
  return card.plan_id === null ? null : (plans.find((plan) => plan.id === card.plan_id) ?? null);
}

/** How a plan reads in a list. */
export function planLabel(plan: BoardPlan): string {
  const status = { draft: "draft", approved: "released", closed: "closed" }[plan.status];
  return `${plan.name} · ${status}`;
}

/** How a line of a card's history reads. */
export const EVENT_LABEL: Record<CardEventKind, string> = {
  started: "started",
  reported: "reported done",
  approved: "checked",
  returned: "returned",
  escalated: "escalated",
  limit_stop: "limit reached",
  input_required: "question",
  input_provided: "answered",
  failed: "failed",
  canceled: "called off",
  released: "released",
  taken_over: "taken over",
  integrated: "taken into the plan",
  note: "note",
};
