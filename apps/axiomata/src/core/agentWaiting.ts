/**
 * Which agents wait for the owner — a permission prompt or a question — in any project, for the whole app
 * (`docs/plans/agent-waiting.md`, board card #87). The backend pushes the list on every change (`agents:waiting`,
 * `src-tauri/src/waiting_watch.rs`); the page asks once at its start so it does not wait for the next change. The
 * badge in the Dock, over the Orbit and on the Kanban's cards reads {@link waitingAgents}.
 */

import { writable, type Writable } from "svelte/store";

/** The event the backend sends with the whole new list. */
export const AGENTS_WAITING = "agents:waiting";

/** An agent that waits for the owner (`axiomata_core::ide_status::WaitingAgent`). */
export interface WaitingAgent {
  agent_id: number;
  project_id: number;
  name: string;
  card_id: number | null;
  card_title: string | null;
  /** When it began to wait (RFC 3339), if known. */
  since: string | null;
}

/** The agents waiting right now, oldest wait first. */
export const waitingAgents: Writable<WaitingAgent[]> = writable([]);

const isNumber = (value: unknown): value is number => typeof value === "number";
const isOptional = <T>(value: unknown, guard: (v: unknown) => v is T): boolean =>
  value === null || value === undefined || guard(value);
const isString = (value: unknown): value is string => typeof value === "string";

/** The entries of an answer that are waiting agents; anything else is dropped. */
export function parseWaiting(value: unknown): WaitingAgent[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter(
      (entry): entry is Record<string, unknown> =>
        typeof entry === "object" &&
        entry !== null &&
        isNumber((entry as Record<string, unknown>).agent_id) &&
        isNumber((entry as Record<string, unknown>).project_id) &&
        isString((entry as Record<string, unknown>).name) &&
        isOptional((entry as Record<string, unknown>).card_id, isNumber) &&
        isOptional((entry as Record<string, unknown>).card_title, isString) &&
        isOptional((entry as Record<string, unknown>).since, isString),
    )
    .map((entry) => ({
      agent_id: entry.agent_id as number,
      project_id: entry.project_id as number,
      name: entry.name as string,
      card_id: (entry.card_id as number | null | undefined) ?? null,
      card_title: (entry.card_title as string | null | undefined) ?? null,
      since: (entry.since as string | null | undefined) ?? null,
    }));
}

/** The waiting agents that work on (or review) `cardId`. */
export function waitingOnCard(waiting: WaitingAgent[], cardId: number): WaitingAgent[] {
  return waiting.filter((agent) => agent.card_id === cardId);
}

/** What a click on a badge does: open the one agent's pane, or show the list of several (W3). */
export type BadgeClick = { kind: "open"; agent: WaitingAgent } | { kind: "list" } | { kind: "none" };

export function badgeClick(waiting: WaitingAgent[]): BadgeClick {
  const [first, ...rest] = waiting;
  if (first === undefined) return { kind: "none" };
  return rest.length === 0 ? { kind: "open", agent: first } : { kind: "list" };
}

/** How long an agent has waited, short: "now", "4 min", "2 h". */
export function waitedFor(since: string | null, now: number): string {
  const start = since === null ? Number.NaN : Date.parse(since);
  if (Number.isNaN(start)) return "";
  const minutes = Math.floor((now - start) / 60_000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h`;
}

/** A box on screen, in CSS pixels. */
export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/**
 * Where a list of `size` opens next to the badge at `anchor`, inside a viewport of `viewport`: below the badge and
 * aligned to its right edge when it fits, else above; never closer than `margin` to the window's edges. A badge in a
 * scrolling Kanban column or near the Dock's left end must not open a list that is cut off or runs off screen.
 */
export function placeList(
  anchor: Box,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  gap: number,
  margin: number,
): { left: number; top: number } {
  const maxLeft = Math.max(margin, viewport.width - size.width - margin);
  const left = Math.min(Math.max(anchor.right - size.width, margin), maxLeft);
  const below = anchor.bottom + gap;
  const above = anchor.top - gap - size.height;
  const fitsBelow = below + size.height <= viewport.height - margin;
  const top = fitsBelow || above < margin ? below : above;
  return { left, top: Math.max(margin, Math.min(top, viewport.height - size.height - margin)) };
}

/**
 * Keeps {@link waitingAgents} live: listens to the backend's event first, then asks once — so no change is lost between
 * the two, and a push that overtook the read wins. Returns the function that stops listening. A failed first read leaves the list as it is.
 */
export async function watchWaiting(
  fetchNow: () => Promise<unknown>,
  listen: (handler: (payload: unknown) => void) => Promise<() => void>,
  store: Writable<WaitingAgent[]> = waitingAgents,
): Promise<() => void> {
  let pushed = false;
  const stop = await listen((payload) => {
    pushed = true;
    store.set(parseWaiting(payload));
  });
  try {
    const first = parseWaiting(await fetchNow());
    // A push that came while the read was under way is newer than the read.
    if (!pushed) store.set(first);
  } catch {
    // The next change brings the list.
  }
  return stop;
}
