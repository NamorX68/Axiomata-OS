/**
 * A tiny "latest request wins" guard for out-of-order async responses —
 * pulled out of `terminal.svelte`'s `fetchHistory` (architecture review,
 * Checkpoint 4) for the same reason `TerminalScreen.ts`'s colour resolution
 * and `terminalInput.ts`'s keyboard mapping already were: it's small,
 * easy-to-get-subtly-wrong logic that deserves a real unit test instead of
 * only being reachable by hand-testing a live scrollback fetch race in
 * `cargo tauri dev`.
 *
 * The concrete problem this solves: rapid mouse-wheel ticks each start a
 * `terminal_scrollback` IPC call, and nothing guarantees they resolve in
 * the order they were sent — a slower earlier request can resolve *after*
 * a faster later one, and applying it would overwrite the newer, more
 * correct viewport with a stale one.
 */

export interface SequenceGuard {
  /** Call when starting a new request; returns that request's own number. */
  next: () => number;
  /** Call when a request with the given number resolves — `true` only if
   *  no *later* request has started since (i.e. this response is still the
   *  most current one worth applying). */
  isCurrent: (seq: number) => boolean;
}

export function createSequenceGuard(): SequenceGuard {
  let current = 0;
  return {
    next: () => ++current,
    isCurrent: (seq: number) => seq === current,
  };
}
