/**
 * Canvas state: the list of placed module instances plus the active theme.
 * Persistence to `~/.axiomata/dashboard.json` is wired in step 6; the mutation
 * helpers here already call `markDirty()` so that hook is a one-liner.
 */

import { get, writable, type Writable } from "svelte/store";

import type { CanvasInstance } from "./types";

export const instances: Writable<CanvasInstance[]> = writable([]);
export const activeTheme: Writable<string> = writable("graphite");

/** Set by persist.ts (step 6). Until then, a no-op. */
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
