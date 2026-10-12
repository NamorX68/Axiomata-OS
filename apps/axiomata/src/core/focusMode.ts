/**
 * The focus mode (`docs/plans/ui-ux-round.md` UX7a, G13; ⌘⇧.): everything but the work in front recedes — the tiles
 * without the focus and the Orbit blur and fade, the glow stops breathing, the ambient light goes. The app sets
 * `data-focus="on"` on `<html>` while it is on (`App.svelte`), and `styles.css` does the rest; the Dock shows it.
 */

import { writable, type Writable } from "svelte/store";

export const focusMode: Writable<boolean> = writable(false);

export function toggleFocus(): void {
  focusMode.update((on) => !on);
}
