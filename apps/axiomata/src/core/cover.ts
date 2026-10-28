/**
 * Whether something covers the app's ground (`docs/plans/glass.md`, GL3): the Studio, a staged panel or a floating
 * window. Under one, what moves on the ground — the Orbit's spin, the pointer light — holds still: nobody sees it, and
 * a panel's glass blurs what lies beneath, which a moving ground would make WebKit compute again on every frame.
 *
 * Each covering view takes a hold for as long as it is open and gives it back on close; holds are counted, so two
 * open panels release the ground only when both are gone.
 */

import { derived, writable, type Readable } from "svelte/store";

const holds = writable(0);

/** True while at least one view covers the ground. */
export const groundCovered: Readable<boolean> = derived(holds, (count) => count > 0);

/** Covers the ground until the returned function is called; calling it twice gives the hold back once. */
export function coverGround(): () => void {
  holds.update((count) => count + 1);
  let released = false;
  return () => {
    if (released) return;
    released = true;
    holds.update((count) => Math.max(0, count - 1));
  };
}
