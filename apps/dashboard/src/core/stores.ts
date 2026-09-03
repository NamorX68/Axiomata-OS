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
