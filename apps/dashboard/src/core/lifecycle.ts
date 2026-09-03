/**
 * Instance lifecycle: the one place that turns a module *type* into a placed
 * instance (default size, cascaded position, singleton guard) and removes
 * one again. Used by the module picker, the `/add` command (step 11) and
 * the agent bridge (step 12). Mount/teardown of the Svelte component itself
 * is a consequence of the keyed `{#each}` in Canvas.svelte.
 */

import { get } from "svelte/store";

import { getModule } from "./registry";
import { addInstance, instances, removeInstance } from "./stores";
import type { CanvasInstance } from "./types";

/** Where the first tile lands; each further tile cascades by CASCADE_PX. */
export const ORIGIN = { x: 40, y: 40 };
export const CASCADE_PX = 32;
export const CASCADE_WRAP = 10;

export type CreateResult =
  | { ok: true; instance: CanvasInstance }
  | { ok: false; reason: string };

/** True if a `singleton` module already has an instance on the canvas. */
export function isPlacedSingleton(type: string): boolean {
  const def = getModule(type);
  return def?.singleton === true && get(instances).some((i) => i.type === type);
}

export function createInstance(
  type: string,
  overrides: Partial<Pick<CanvasInstance, "x" | "y" | "w" | "h" | "config">> = {},
): CreateResult {
  const def = getModule(type);
  if (!def) {
    return { ok: false, reason: `unknown module type "${type}"` };
  }
  if (isPlacedSingleton(type)) {
    return { ok: false, reason: `"${def.title}" allows only one instance` };
  }
  const step = get(instances).length % CASCADE_WRAP;
  const instance = addInstance({
    type,
    x: overrides.x ?? ORIGIN.x + step * CASCADE_PX,
    y: overrides.y ?? ORIGIN.y + step * CASCADE_PX,
    w: overrides.w ?? def.defaultSize.w,
    h: overrides.h ?? def.defaultSize.h,
    config: overrides.config ?? {},
  });
  return { ok: true, instance };
}

export function destroyInstance(id: string): boolean {
  const exists = get(instances).some((i) => i.id === id);
  if (exists) removeInstance(id);
  return exists;
}
