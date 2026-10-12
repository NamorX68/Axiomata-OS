/**
 * Live status of the Studio for the rest of the app: how many card sessions (workers and reviewers) are at work right
 * now, and on which cards. The Studio's own status poll (`ide/agentStatus.ts`) only runs while the Studio is on screen
 * and per project, so the app asks the backend's list of open card sessions instead — it works with the Studio closed.
 * One poll serves everyone (`watchActivity`): the Dock's Studio mark, the Orbit's pulse and the Kanban's glowing cards
 * (`docs/plans/ui-ux-round.md` UX6a).
 */

import { writable, type Writable } from "svelte/store";

/** How often the list is asked for; a session starts or ends rarely, and the pulse need not be exact to the second. */
export const ACTIVITY_POLL_MS = 5000;

/** Sessions at work right now; 0 = quiet. */
export const studioActivity: Writable<number> = writable(0);

/** The cards a session is at work on right now. */
export const studioCards: Writable<ReadonlySet<number>> = writable(new Set());

/** The entries of an `open_card_sessions` answer that are sessions; anything else counts as none. */
function sessionsOf(open: unknown): { card_id: number }[] {
  if (!Array.isArray(open)) return [];
  return open.filter(
    (s): s is { card_id: number } =>
      typeof s === "object" && s !== null && typeof (s as { card_id?: unknown }).card_id === "number",
  );
}

/** Counts the sessions of an `open_card_sessions` answer. */
export function sessionCount(open: unknown): number {
  return sessionsOf(open).length;
}

/** The cards the sessions of an `open_card_sessions` answer work on. */
export function sessionCards(open: unknown): Set<number> {
  return new Set(sessionsOf(open).map((s) => s.card_id));
}

/**
 * Polls `fetchOpen` every `intervalMs` (and once right away) and reports the number of sessions and their cards to
 * `report`. Skips a round while the window is hidden or the previous answer is still pending, and keeps the last
 * answer when a call fails (a short backend hiccup must not read as "all quiet"). Returns the function that stops it.
 */
export function startActivityPoll(
  fetchOpen: () => Promise<unknown>,
  report: (count: number, cards: Set<number>) => void,
  intervalMs: number = ACTIVITY_POLL_MS,
  isHidden: () => boolean = () => document.hidden,
): () => void {
  let pending = false;
  let stopped = false;
  const poll = async () => {
    if (pending || isHidden()) return;
    pending = true;
    try {
      const open = await fetchOpen();
      if (!stopped) report(sessionCount(open), sessionCards(open));
    } catch {
      // Keep the last answer.
    } finally {
      pending = false;
    }
  };
  void poll();
  const timer = setInterval(() => void poll(), intervalMs);
  return () => {
    stopped = true;
    clearInterval(timer);
  };
}

let watchers = 0;
let stopShared: (() => void) | null = null;

/**
 * Keeps `studioActivity` and `studioCards` live for as long as anyone watches: the first watcher starts the one poll,
 * the last one's stop ends it. Returns that watcher's stop (calling it twice is harmless).
 */
export function watchActivity(fetchOpen: () => Promise<unknown>): () => void {
  watchers++;
  stopShared ??= startActivityPoll(fetchOpen, (count, cards) => {
    studioActivity.set(count);
    studioCards.set(cards);
  });
  let stopped = false;
  return () => {
    if (stopped) return;
    stopped = true;
    watchers--;
    if (watchers === 0) {
      stopShared?.();
      stopShared = null;
    }
  };
}
