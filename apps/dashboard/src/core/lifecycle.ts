/**
 * Instance lifecycle: the one place that turns a module *type* into a placed
 * instance (default size, cascaded position, singleton guard) and removes
 * one again. Used by the module picker, the `/add` command (step 11) and
 * the agent bridge (step 12). Mount/teardown of the Svelte component itself
 * is a consequence of the keyed `{#each}` in Canvas.svelte.
 */

import { get } from "svelte/store";

import { getModule } from "./registry";
import { addInstance, canvasSize, instances, removeInstance } from "./stores";
import type { CanvasInstance } from "./types";

/** Where a tile lands when the canvas size is not known yet. */
export const ORIGIN = { x: 48, y: 48 };
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
  if (def.background) {
    const instance = addInstance({ type, x: 0, y: 0, w: 0, h: 0, config: overrides.config ?? {} });
    return { ok: true, instance };
  }
  // `computeDefaultSize` (if the module declares one — see its own doc
  // comment in `core/types.ts`) wins over the static `defaultSize`; either
  // way an explicit `overrides.w`/`h` (the module picker's own size
  // controls, a `/add` command argument, …) wins over both. `types.ts`
  // documents `defaultSize` as the fallback "if this throws" — `??` alone
  // doesn't catch a thrown error (only a `null`/`undefined` return), so
  // that promise needs an actual `try`/`catch` here, not just optional
  // chaining (architecture review, Checkpoint 5e: caught as a real gap
  // between what three separate doc comments promised and what the code
  // actually did).
  let size = def.defaultSize;
  if (def.computeDefaultSize) {
    try {
      size = def.computeDefaultSize();
    } catch {
      // Falls back to `def.defaultSize`, already assigned above.
    }
  }
  // Centred on the canvas, then cascaded, rather than cascading from the top
  // left: a new tile should appear where the eye already is. On a 21:9 screen
  // the old origin put it in the far corner, a long way from the middle of
  // the window the user is looking at. Falls back to the origin before the
  // canvas has been measured (`canvasSize` starts at 0×0).
  const canvas = get(canvasSize);
  const base =
    canvas.w > 0 && canvas.h > 0
      ? {
          x: Math.max(0, Math.round((canvas.w - (overrides.w ?? size.w)) / 2)),
          y: Math.max(0, Math.round((canvas.h - (overrides.h ?? size.h)) / 2)),
        }
      : ORIGIN;

  // Centred, and only nudged aside if that exact spot is already taken.
  // Cascading unconditionally — which is what this did — meant a tile opened
  // centred only when it was the first one, and every later tile appeared
  // progressively further down and to the right of where it was expected.
  const taken = (x: number, y: number) =>
    get(instances).some((other) => other.x === x && other.y === y);
  const spot = { ...base };
  for (let n = 0; n < CASCADE_WRAP && taken(spot.x, spot.y); n += 1) {
    spot.x = base.x + (n + 1) * CASCADE_PX;
    spot.y = base.y + (n + 1) * CASCADE_PX;
  }

  const instance = addInstance({
    type,
    x: overrides.x ?? spot.x,
    y: overrides.y ?? spot.y,
    w: overrides.w ?? size.w,
    h: overrides.h ?? size.h,
    config: overrides.config ?? {},
  });
  return { ok: true, instance };
}

export function destroyInstance(id: string): boolean {
  const exists = get(instances).some((i) => i.id === id);
  if (exists) removeInstance(id);
  return exists;
}
