/**
 * Plans whose cards are grilled once the planner is done: the owner ticked "Danach grillen". Kept outside the
 * planning panel
 * so switching the Flow's mode (which unmounts the panel) does not forget the tick; gone with the app, which is fine
 * — it is
 * one click to make again. Who grills is read from the panel's pickers when the grill starts.
 */

import { writable } from "svelte/store";

export const grillAfter = writable<Record<number, boolean>>({});

/** Ticks or clears the wish of one plan. */
export function setGrillWish(planId: number, wanted: boolean): void {
  grillAfter.update((all) => {
    const next = { ...all };
    if (wanted) next[planId] = true;
    else delete next[planId];
    return next;
  });
}
