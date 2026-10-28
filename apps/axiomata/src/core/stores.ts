/**
 * Canvas state: the list of placed module instances plus the active theme.
 * Every mutation helper calls `markDirty()`; `persist.ts` registers the
 * debounced writer to `~/.axiomata/dashboard.json` via `onDirty`.
 */

import { get, writable, type Writable } from "svelte/store";

import type { CanvasInstance } from "./types";

export const instances: Writable<CanvasInstance[]> = writable([]);
export const activeTheme: Writable<string> = writable("graphite");
/** Draw the dot grid on the canvas (tiles snap to it either way). */
export const showGrid: Writable<boolean> = writable(false);
/** Magnetic edge snapping between tiles. */
export const snapEdges: Writable<boolean> = writable(true);
/** 0 (opaque) – 100 (fully see-through), the one global knob for how see-through all of the app's glass is — panes,
 *  panels, menus, dragged tiles (`docs/plans/glass.md`, G5). `core/glass.ts` turns it into a multiplier on each theme's
 *  own authored alpha (`--ax-tile-glass-opacity`): 50 is each theme's look unchanged, 0 a solid surface (CSS clamps
 *  an alpha above 1), 100 fully transparent. macOS "Reduce transparency" overrides it with solid glass. */
export const windowTransparency: Writable<number> = writable(50);
/** How soft the ground behind the glass is, in px of blur (`docs/plans/glass.md`, G16): the theme's ground or the
 *  owner's background image is blurred once, as a still layer, so every translucent surface over it reads as frosted
 *  without a blur of its own. `core/glass.ts` writes it to `--ax-ground-blur`. */
export const glassBlur: Writable<number> = writable(20);
/** Current canvas box in CSS px (set by Canvas.svelte's ResizeObserver). */
export const canvasSize: Writable<{ w: number; h: number }> = writable({ w: 0, h: 0 });
/** Snap guide lines while a tile is dragged / resized (canvas coordinates). */
export const guides: Writable<{ axis: "x" | "y"; at: number }[]> = writable([]);

/** Set by persist.ts on boot. Until then, a no-op. */
let dirtyHook: () => void = () => {};
export function onDirty(fn: () => void): void {
  dirtyHook = fn;
}
function markDirty(): void {
  dirtyHook();
}

function newId(): string {
  return crypto.randomUUID();
}

/** Highest `z` currently in use, or 0. */
export function topZ(): number {
  return get(instances).reduce((max, i) => Math.max(max, i.z), 0);
}

export function addInstance(
  init: Omit<CanvasInstance, "id" | "z" | "flipped"> &
    Partial<Pick<CanvasInstance, "flipped">>,
): CanvasInstance {
  const instance: CanvasInstance = {
    id: newId(),
    z: topZ() + 1,
    flipped: init.flipped ?? false,
    ...init,
  };
  instances.update((list) => [...list, instance]);
  markDirty();
  return instance;
}

export function removeInstance(id: string): void {
  instances.update((list) => list.filter((i) => i.id !== id));
  markDirty();
}

export function updateInstance(id: string, patch: Partial<CanvasInstance>): void {
  instances.update((list) =>
    list.map((i) => (i.id === id ? { ...i, ...patch } : i)),
  );
  markDirty();
}

/** Raise an instance above all others. */
export function bringToFront(id: string): void {
  const max = topZ();
  const current = get(instances).find((i) => i.id === id);
  if (current && current.z < max) {
    updateInstance(id, { z: max + 1 });
  }
}

/** Replace the whole instance list (used by persist.ts on boot). Does not
 *  mark dirty — this IS the loaded state. */
export function loadInstances(list: CanvasInstance[]): void {
  instances.set(list);
}
