/**
 * Live status of the Studio for the Orbit: how many card sessions (workers and reviewers) are at work right now. The
 * Studio's own status poll (`ide/agentStatus.ts`) only runs while the Studio is on screen and per project, so the Orbit
 * asks the backend's list of open card sessions instead — it works with the Studio closed.
 */

import { writable, type Writable } from "svelte/store";

/** How often the list is asked for; a session starts or ends rarely, and the pulse need not be exact to the second. */
export const ACTIVITY_POLL_MS = 5000;

/** Sessions at work right now; 0 = quiet. */
export const studioActivity: Writable<number> = writable(0);

/** Counts the entries of an `open_card_sessions` answer; anything that is not a list of sessions counts as none. */
export function sessionCount(open: unknown): number {
  if (!Array.isArray(open)) return 0;
  const isSession = (s: unknown) =>
    typeof s === "object" && s !== null && typeof (s as { card_id?: unknown }).card_id === "number";
  return open.filter(isSession).length;
}

/**
 * Polls `fetchOpen` every `intervalMs` (and once right away) and reports the number of sessions to `report`. Skips a
 * round while the window is hidden or the previous answer is still pending, and keeps the last count when a call fails
 * (a short backend hiccup must not read as "all quiet"). Returns the function that stops it.
 */
export function startActivityPoll(
  fetchOpen: () => Promise<unknown>,
  report: (count: number) => void,
  intervalMs: number = ACTIVITY_POLL_MS,
  isHidden: () => boolean = () => document.hidden,
): () => void {
  let pending = false;
  let stopped = false;
  const poll = async () => {
    if (pending || isHidden()) return;
    pending = true;
    try {
      const count = sessionCount(await fetchOpen());
      if (!stopped) report(count);
    } catch {
      // Keep the last count.
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
